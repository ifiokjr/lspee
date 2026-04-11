use std::time::Duration;

use tokio::{sync::oneshot, task::JoinHandle, time};

use crate::registry::SessionRegistry;

const EVICTION_TICK: Duration = Duration::from_secs(1);

pub struct EvictionLoop {
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

impl EvictionLoop {
    pub fn start(registry: SessionRegistry) -> Self {
        let (stop, mut stop_rx) = oneshot::channel();
        let idle_ttl = registry.idle_ttl();

        let task = tokio::spawn(tracing::Instrument::instrument(
            async move {
                let mut ticker = time::interval(EVICTION_TICK);
                ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

                loop {
                    tokio::select! {
                            _ = ticker.tick() => {
                                let candidates = registry.idle_candidates().await;
                                for candidate in candidates {
                                    tracing::info!(lsp_id = %candidate.key.lsp_id, idle_ttl_secs = idle_ttl.as_secs(), "evicting idle LSP session");

                                if let Err(error) = candidate.runtime.shutdown().await {
                                    tracing::warn!(key = ?candidate.key, ?error, "failed graceful LSP shutdown; forcing stop");
                                    if let Err(force_error) = candidate.runtime.force_stop().await {
                                        tracing::error!(key = ?candidate.key, ?force_error, "failed force-stop for idle LSP session");
                                    }
                                }

                                registry.remove(&candidate.key).await;
                                registry.increment_idle_gc().await;
                            }
                        }
                        _ = &mut stop_rx => {
                            break;
                        }
                    }
                }
            },
            tracing::info_span!("eviction_loop"),
        ));

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
