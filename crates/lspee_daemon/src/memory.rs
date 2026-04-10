use std::{path::PathBuf, time::Duration};

use lspee_protocol::{ClientKind, ERROR_SESSION_EVICTED_MEMORY, StreamErrorPayload};
use serde_json::json;
use tokio::{sync::oneshot, task::JoinHandle, time};

use crate::registry::{MemorySessionSnapshot, SessionRegistry};

#[derive(Debug, Clone, Copy)]
pub struct MemoryBudgetSettings {
    pub max_session_bytes: Option<u64>,
    pub max_total_bytes: Option<u64>,
    pub check_interval: Duration,
}

impl MemoryBudgetSettings {
    #[must_use]
    pub fn enabled(self) -> bool {
        self.max_session_bytes.is_some() || self.max_total_bytes.is_some()
    }
}

pub struct MemoryMonitor {
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

impl MemoryMonitor {
    pub fn start(registry: SessionRegistry, settings: MemoryBudgetSettings) -> Self {
        let (stop, mut stop_rx) = oneshot::channel();

        let task = tokio::spawn(async move {
            let mut ticker = time::interval(settings.check_interval);
            ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if !settings.enabled() {
                            continue;
                        }

                        let samples = collect_samples(&registry).await;
                        let evictions = select_evictions(&samples, settings);

                        for sample in evictions {
                            evict_session(&registry, &sample, settings).await;
                        }
                    }
                    _ = &mut stop_rx => break,
                }
            }
        });

        Self {
            stop: Some(stop),
            task,
        }
    }

    pub async fn shutdown(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let _ = self.task.await;
    }
}

#[derive(Debug, Clone)]
pub struct MemorySample {
    pub key: crate::SessionKey,
    pub root: PathBuf,
    pub rss_bytes: u64,
    pub ref_count: usize,
    pub idle_for: Duration,
    pub client_kinds: Vec<ClientKind>,
    pub handle: crate::SessionHandle,
}

pub async fn total_memory_bytes(registry: &SessionRegistry) -> u64 {
    collect_samples(registry)
        .await
        .into_iter()
        .map(|sample| sample.rss_bytes)
        .sum()
}

async fn collect_samples(registry: &SessionRegistry) -> Vec<MemorySample> {
    let snapshots = registry.memory_snapshots().await;
    let mut samples = Vec::new();

    for snapshot in snapshots {
        let rss_bytes = snapshot
            .handle
            .runtime
            .rss_bytes()
            .await
            .unwrap_or(None)
            .unwrap_or(0);
        samples.push(memory_sample_from_snapshot(snapshot, rss_bytes));
    }

    samples
}

fn memory_sample_from_snapshot(snapshot: MemorySessionSnapshot, rss_bytes: u64) -> MemorySample {
    MemorySample {
        root: snapshot.handle.key.root.clone(),
        key: snapshot.handle.key.clone(),
        rss_bytes,
        ref_count: snapshot.ref_count,
        idle_for: snapshot.idle_for,
        client_kinds: snapshot.client_kinds,
        handle: snapshot.handle,
    }
}

fn select_evictions(samples: &[MemorySample], settings: MemoryBudgetSettings) -> Vec<MemorySample> {
    let mut remaining: Vec<MemorySample> = samples.to_vec();
    let mut selected = Vec::new();

    if let Some(max_session_bytes) = settings.max_session_bytes {
        let mut over_limit: Vec<_> = remaining
            .iter()
            .filter(|sample| sample.rss_bytes > max_session_bytes)
            .cloned()
            .collect();
        sort_candidates(&mut over_limit);

        for sample in over_limit {
            remaining.retain(|candidate| candidate.key != sample.key);
            selected.push(sample);
        }
    }

    if let Some(max_total_bytes) = settings.max_total_bytes {
        let mut total_bytes: u64 = remaining.iter().map(|sample| sample.rss_bytes).sum();
        if total_bytes > max_total_bytes {
            sort_candidates(&mut remaining);
            for sample in remaining.clone() {
                if total_bytes <= max_total_bytes {
                    break;
                }
                total_bytes = total_bytes.saturating_sub(sample.rss_bytes);
                selected.push(sample.clone());
                remaining.retain(|candidate| candidate.key != sample.key);
            }
        }
    }

    dedupe_by_key(selected)
}

fn sort_candidates(samples: &mut [MemorySample]) {
    samples.sort_by(|left, right| {
        left_editor(left)
            .cmp(&left_editor(right))
            .then((left.ref_count > 0).cmp(&(right.ref_count > 0)))
            .then(right.idle_for.cmp(&left.idle_for))
            .then(right.rss_bytes.cmp(&left.rss_bytes))
    });
}

fn left_editor(sample: &MemorySample) -> bool {
    sample.client_kinds.contains(&ClientKind::Editor)
}

fn dedupe_by_key(samples: Vec<MemorySample>) -> Vec<MemorySample> {
    let mut seen = std::collections::HashSet::new();
    let mut deduped = Vec::new();

    for sample in samples {
        if seen.insert(sample.key.clone()) {
            deduped.push(sample);
        }
    }

    deduped
}

async fn evict_session(
    registry: &SessionRegistry,
    sample: &MemorySample,
    settings: MemoryBudgetSettings,
) {
    let notice = StreamErrorPayload {
        code: ERROR_SESSION_EVICTED_MEMORY.to_string(),
        message: format!(
            "Session evicted due to memory budget pressure (rss={} bytes). Re-attach or retry the request to resume.",
            sample.rss_bytes
        ),
        retryable: true,
        details: Some(json!({
            "project_root": sample.root,
            "lsp_id": sample.key.lsp_id,
            "rss_bytes": sample.rss_bytes,
            "max_session_bytes": settings.max_session_bytes,
            "max_total_bytes": settings.max_total_bytes,
            "resume_hint": "Re-attach or retry the request. For Helix, restart the language server if it does not reconnect automatically.",
            "eviction_policy": "idle_lru"
        })),
    };

    registry
        .mark_terminating_with_notice(&sample.key, Some(notice.clone()))
        .await;
    let _ = sample.handle.events.send(notice);

    if let Err(error) = sample.handle.runtime.shutdown().await {
        tracing::warn!(key = ?sample.key, ?error, "failed graceful memory-budget shutdown; forcing stop");
        if let Err(force_error) = sample.handle.runtime.force_stop().await {
            tracing::error!(key = ?sample.key, ?force_error, "failed force-stop after memory eviction");
        }
    }

    registry.remove(&sample.key).await;
    registry.increment_memory_eviction().await;
}

#[cfg(test)]
mod tests {
    use super::{MemoryBudgetSettings, MemorySample, select_evictions};
    use lspee_config::LspConfig;
    use lspee_protocol::ClientKind;
    use std::{collections::BTreeMap, fs, path::PathBuf, time::Duration};

    async fn sample(
        key: &str,
        rss_bytes: u64,
        idle_secs: u64,
        ref_count: usize,
        client_kinds: Vec<ClientKind>,
    ) -> MemorySample {
        let key = crate::SessionKey::new(PathBuf::from(format!("/tmp/{key}")), key, "hash");
        fs::create_dir_all(&key.root).expect("sample root should exist");

        let transport = std::sync::Arc::new(lspee_lsp::LspTransport::new(key.root.clone()));
        let lsp_config = LspConfig {
            id: key.lsp_id.clone(),
            command: "cat".to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
            initialization_options: BTreeMap::new(),
        };
        let runtime = std::sync::Arc::new(
            transport
                .spawn(&lsp_config)
                .await
                .expect("sample runtime should spawn"),
        );
        let (events, _) = tokio::sync::broadcast::channel(4);

        MemorySample {
            key: key.clone(),
            root: key.root.clone(),
            rss_bytes,
            ref_count,
            idle_for: Duration::from_secs(idle_secs),
            client_kinds,
            handle: crate::SessionHandle {
                key,
                transport,
                runtime,
                initialize_result: serde_json::Value::Null,
                events,
            },
        }
    }

    async fn cleanup_samples(samples: &[MemorySample]) {
        for sample in samples {
            let _ = sample.handle.runtime.force_stop().await;
            let _ = fs::remove_dir_all(&sample.root);
        }
    }

    #[tokio::test]
    async fn per_session_limit_selects_over_limit_sessions() {
        let samples = vec![
            sample("a", 500, 60, 0, vec![ClientKind::Agent]).await,
            sample("b", 2_000, 10, 0, vec![ClientKind::Agent]).await,
        ];

        let selected = select_evictions(
            &samples,
            MemoryBudgetSettings {
                max_session_bytes: Some(1_000),
                max_total_bytes: None,
                check_interval: Duration::from_secs(1),
            },
        );

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].key.lsp_id, "b");
        cleanup_samples(&samples).await;
    }

    #[tokio::test]
    async fn total_limit_prefers_idle_agent_over_editor() {
        let samples = vec![
            sample("editor", 800, 90, 1, vec![ClientKind::Editor]).await,
            sample("agent", 800, 120, 0, vec![ClientKind::Agent]).await,
        ];

        let selected = select_evictions(
            &samples,
            MemoryBudgetSettings {
                max_session_bytes: None,
                max_total_bytes: Some(1_000),
                check_interval: Duration::from_secs(1),
            },
        );

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].key.lsp_id, "agent");
        cleanup_samples(&samples).await;
    }

    #[tokio::test]
    async fn total_limit_uses_lru_order_for_agents() {
        let samples = vec![
            sample("old", 600, 200, 0, vec![ClientKind::Agent]).await,
            sample("new", 600, 10, 0, vec![ClientKind::Agent]).await,
            sample("fresh", 600, 1, 0, vec![ClientKind::Agent]).await,
        ];

        let selected = select_evictions(
            &samples,
            MemoryBudgetSettings {
                max_session_bytes: None,
                max_total_bytes: Some(700),
                check_interval: Duration::from_secs(1),
            },
        );

        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].key.lsp_id, "old");
        assert_eq!(selected[1].key.lsp_id, "new");
        cleanup_samples(&samples).await;
    }
}
