use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow};
use lspee_protocol::{ClientKind, StreamErrorPayload};
use serde_json::Value;
use tokio::sync::{Mutex, Notify, RwLock, broadcast};

const DEFAULT_IDLE_TTL: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionKey {
    pub root: PathBuf,
    pub server_id: String,
    pub config_hash: String,
}

impl SessionKey {
    #[must_use]
    pub fn new(
        root: PathBuf,
        server_id: impl Into<String>,
        config_hash: impl Into<String>,
    ) -> Self {
        Self {
            root,
            server_id: server_id.into(),
            config_hash: config_hash.into(),
        }
    }
}

#[derive(Clone)]
pub struct SessionHandle {
    pub key: SessionKey,
    pub transport: Arc<lspee_lsp::LspTransport>,
    pub runtime: Arc<lspee_lsp::LspRuntime>,
    pub initialize_result: Value,
    pub events: broadcast::Sender<StreamErrorPayload>,
}

impl std::fmt::Debug for SessionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHandle")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct SessionSnapshot {
    pub key: SessionKey,
    pub ref_count: usize,
    pub terminating: bool,
    pub idle_for: Duration,
    pub idle_ttl: Duration,
    pub client_kinds: Vec<ClientKind>,
}

#[derive(Debug, Clone)]
pub struct RegistrySnapshot {
    pub sessions: Vec<SessionSnapshot>,
    pub lease_count: usize,
    pub counters: RegistryCounters,
}

#[derive(Debug, Clone)]
pub struct MemorySessionSnapshot {
    pub handle: SessionHandle,
    pub ref_count: usize,
    pub idle_for: Duration,
    pub client_kinds: Vec<ClientKind>,
}

#[derive(Debug)]
struct SessionRecord {
    handle: SessionHandle,
    ref_count: usize,
    last_used: Instant,
    terminating: bool,
    termination_notice: Option<StreamErrorPayload>,
}

impl SessionRecord {
    fn new(handle: SessionHandle) -> Self {
        Self {
            handle,
            ref_count: 0,
            last_used: Instant::now(),
            terminating: false,
            termination_notice: None,
        }
    }

    fn touch(&mut self) {
        self.last_used = Instant::now();
    }
}

#[derive(Debug)]
struct SpawnGate {
    notify: Arc<Notify>,
    started: bool,
}

impl SpawnGate {
    fn new() -> Self {
        Self {
            notify: Arc::new(Notify::new()),
            started: false,
        }
    }
}

#[derive(Debug, Clone)]
struct LeaseRecord {
    key: SessionKey,
    client_kind: Option<ClientKind>,
}

#[derive(Debug, Clone)]
pub struct Lease {
    lease_id: String,
    key: SessionKey,
    registry: SessionRegistry,
}

impl Lease {
    pub fn key(&self) -> &SessionKey {
        &self.key
    }

    pub fn lease_id(&self) -> &str {
        &self.lease_id
    }

    pub async fn release(self) {
        self.registry.release_by_lease_id(&self.lease_id).await;
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RegistryCounters {
    pub sessions_spawned_total: u64,
    pub sessions_reused_total: u64,
    pub sessions_gc_idle_total: u64,
    pub sessions_evicted_memory_total: u64,
    pub session_crashes_total: u64,
    pub attach_requests_total: u64,
}

#[derive(Debug, Clone)]
pub struct SessionRegistry {
    sessions: Arc<RwLock<HashMap<SessionKey, SessionRecord>>>,
    leases: Arc<RwLock<HashMap<String, LeaseRecord>>>,
    spawn_gates: Arc<Mutex<HashMap<SessionKey, Arc<Mutex<SpawnGate>>>>>,
    counters: Arc<RwLock<RegistryCounters>>,
    lease_seq: Arc<AtomicU64>,
    idle_ttl: Duration,
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::with_idle_ttl(DEFAULT_IDLE_TTL)
    }

    #[must_use]
    pub fn with_idle_ttl(idle_ttl: Duration) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            leases: Arc::new(RwLock::new(HashMap::new())),
            spawn_gates: Arc::new(Mutex::new(HashMap::new())),
            counters: Arc::new(RwLock::new(RegistryCounters::default())),
            lease_seq: Arc::new(AtomicU64::new(1)),
            idle_ttl,
        }
    }

    /// Returns the configured idle TTL for sessions.
    #[must_use]
    pub fn idle_ttl(&self) -> Duration {
        self.idle_ttl
    }

    fn next_lease_id(&self) -> String {
        let id = self.lease_seq.fetch_add(1, Ordering::Relaxed);
        format!("lease_{id}")
    }

    pub async fn acquire_or_spawn<F, Fut>(
        &self,
        key: SessionKey,
        client_kind: Option<ClientKind>,
        spawn: F,
    ) -> Result<Lease>
    where
        F: FnOnce(SessionKey) -> Fut,
        Fut: std::future::Future<Output = Result<SessionHandle>>,
    {
        {
            let mut counters = self.counters.write().await;
            counters.attach_requests_total += 1;
        }

        let gate = {
            let mut gates = self.spawn_gates.lock().await;
            gates
                .entry(key.clone())
                .or_insert_with(|| Arc::new(Mutex::new(SpawnGate::new())))
                .clone()
        };

        loop {
            {
                let mut sessions = self.sessions.write().await;
                if let Some(record) = sessions.get_mut(&key) {
                    record.ref_count += 1;
                    record.terminating = false;
                    record.termination_notice = None;
                    record.touch();
                    {
                        let mut counters = self.counters.write().await;
                        counters.sessions_reused_total += 1;
                    }
                    let lease_id = self.next_lease_id();
                    self.leases.write().await.insert(
                        lease_id.clone(),
                        LeaseRecord {
                            key: key.clone(),
                            client_kind: client_kind.clone(),
                        },
                    );
                    return Ok(Lease {
                        lease_id,
                        key,
                        registry: self.clone(),
                    });
                }
            }

            let wait_for_spawn;
            let should_spawn;
            {
                let mut gate_guard = gate.lock().await;
                if !gate_guard.started {
                    gate_guard.started = true;
                    should_spawn = true;
                    wait_for_spawn = None;
                } else {
                    should_spawn = false;
                    wait_for_spawn = Some(gate_guard.notify.clone().notified_owned());
                }
            }

            if should_spawn {
                let spawn_result = spawn(key.clone()).await;
                {
                    let mut gate_guard = gate.lock().await;
                    gate_guard.started = false;
                    gate_guard.notify.notify_waiters();
                }

                if let Ok(handle) = spawn_result {
                    let mut sessions = self.sessions.write().await;
                    let record = sessions
                        .entry(key.clone())
                        .or_insert_with(|| SessionRecord::new(handle));
                    record.ref_count += 1;
                    record.terminating = false;
                    record.termination_notice = None;
                    record.touch();
                    {
                        let mut counters = self.counters.write().await;
                        counters.sessions_spawned_total += 1;
                    }
                    let lease_id = self.next_lease_id();
                    self.leases.write().await.insert(
                        lease_id.clone(),
                        LeaseRecord {
                            key: key.clone(),
                            client_kind: client_kind.clone(),
                        },
                    );
                    return Ok(Lease {
                        lease_id,
                        key,
                        registry: self.clone(),
                    });
                }

                let mut gates = self.spawn_gates.lock().await;
                gates.remove(&key);
                return spawn_result.map(|_| unreachable!());
            }

            if let Some(wait_for_spawn) = wait_for_spawn {
                wait_for_spawn.await;
            }
        }
    }

    pub async fn session_handle(&self, key: &SessionKey) -> Option<SessionHandle> {
        self.sessions
            .read()
            .await
            .get(key)
            .map(|record| record.handle.clone())
    }

    pub async fn handle_for_lease_id(&self, lease_id: &str) -> Option<SessionHandle> {
        let key = {
            let leases = self.leases.read().await;
            leases.get(lease_id).map(|record| record.key.clone())?
        };
        self.session_handle(&key).await
    }

    pub async fn release_by_lease_id(&self, lease_id: &str) -> Option<usize> {
        let key = self.leases.write().await.remove(lease_id)?.key;
        let mut sessions = self.sessions.write().await;
        if let Some(record) = sessions.get_mut(&key) {
            if record.ref_count > 0 {
                record.ref_count -= 1;
            }
            record.touch();
            return Some(record.ref_count);
        }
        Some(0)
    }

    pub async fn release_many(&self, lease_ids: impl IntoIterator<Item = String>) {
        for lease_id in lease_ids {
            let _ = self.release_by_lease_id(&lease_id).await;
        }
    }

    pub async fn call_by_lease_id(&self, lease_id: &str, request: Value) -> Result<Option<Value>> {
        let key = {
            let leases = self.leases.read().await;
            match leases.get(lease_id) {
                Some(lease) => lease.key.clone(),
                None => return Ok(None),
            }
        };

        let runtime = {
            let mut sessions = self.sessions.write().await;
            let record = match sessions.get_mut(&key) {
                Some(record) => record,
                None => return Ok(None),
            };

            if record.terminating {
                if let Some(notice) = &record.termination_notice {
                    return Err(anyhow!("{}: {}", notice.code, notice.message));
                }
                return Err(anyhow!("session is terminating"));
            }

            record.touch();
            record.handle.runtime.clone()
        };

        let response = runtime.call(request).await?;
        self.touch(&key).await;
        Ok(Some(response))
    }

    pub async fn touch(&self, key: &SessionKey) {
        let mut sessions = self.sessions.write().await;
        if let Some(record) = sessions.get_mut(key) {
            record.touch();
        }
    }

    pub async fn mark_terminating_with_notice(
        &self,
        key: &SessionKey,
        notice: Option<StreamErrorPayload>,
    ) {
        let mut sessions = self.sessions.write().await;
        if let Some(record) = sessions.get_mut(key) {
            record.terminating = true;
            record.termination_notice = notice;
        }
    }

    pub async fn remove(&self, key: &SessionKey) {
        {
            let mut sessions = self.sessions.write().await;
            sessions.remove(key);
        }
        let mut leases = self.leases.write().await;
        leases.retain(|_, lease_record| &lease_record.key != key);
        let mut gates = self.spawn_gates.lock().await;
        gates.remove(key);
    }

    pub async fn idle_candidates(&self) -> Vec<SessionHandle> {
        let now = Instant::now();
        let idle_ttl = self.idle_ttl;
        let mut sessions = self.sessions.write().await;

        sessions
            .values_mut()
            .filter(|record| !record.terminating && record.ref_count == 0)
            .filter(|record| now.duration_since(record.last_used) >= idle_ttl)
            .map(|record| {
                record.terminating = true;
                record.handle.clone()
            })
            .collect()
    }

    pub async fn all_handles(&self) -> Vec<SessionHandle> {
        self.sessions
            .read()
            .await
            .values()
            .map(|record| record.handle.clone())
            .collect()
    }

    pub async fn memory_snapshots(&self) -> Vec<MemorySessionSnapshot> {
        let now = Instant::now();
        let sessions = self.sessions.read().await;
        let leases = self.leases.read().await;

        sessions
            .values()
            .map(|record| {
                let client_kinds = lease_client_kinds(&record.handle.key, &leases);
                MemorySessionSnapshot {
                    handle: record.handle.clone(),
                    ref_count: record.ref_count,
                    idle_for: now.saturating_duration_since(record.last_used),
                    client_kinds,
                }
            })
            .collect()
    }

    pub async fn increment_idle_gc(&self) {
        let mut counters = self.counters.write().await;
        counters.sessions_gc_idle_total += 1;
    }

    pub async fn increment_memory_eviction(&self) {
        let mut counters = self.counters.write().await;
        counters.sessions_evicted_memory_total += 1;
    }

    pub async fn increment_session_crash(&self) {
        let mut counters = self.counters.write().await;
        counters.session_crashes_total += 1;
    }

    pub async fn snapshot(&self) -> RegistrySnapshot {
        let now = Instant::now();
        let idle_ttl = self.idle_ttl;
        let sessions = self.sessions.read().await;
        let leases = self.leases.read().await;
        let counters = *self.counters.read().await;

        let session_snapshots = sessions
            .values()
            .map(|record| SessionSnapshot {
                key: record.handle.key.clone(),
                ref_count: record.ref_count,
                terminating: record.terminating,
                idle_for: now.saturating_duration_since(record.last_used),
                idle_ttl,
                client_kinds: lease_client_kinds(&record.handle.key, &leases),
            })
            .collect();

        RegistrySnapshot {
            sessions: session_snapshots,
            lease_count: leases.len(),
            counters,
        }
    }

    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    pub async fn lease_count(&self) -> usize {
        self.leases.read().await.len()
    }

    pub async fn counters(&self) -> RegistryCounters {
        *self.counters.read().await
    }
}

fn lease_client_kinds(key: &SessionKey, leases: &HashMap<String, LeaseRecord>) -> Vec<ClientKind> {
    let mut kinds = HashSet::new();
    for lease in leases.values() {
        if &lease.key == key {
            if let Some(kind) = &lease.client_kind {
                kinds.insert(kind.clone());
            }
        }
    }

    kinds.into_iter().collect()
}
