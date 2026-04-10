#![cfg_attr(not(unix), allow(unused))]

#[cfg(not(unix))]
compile_error!("lspee_daemon currently supports unix-like platforms only (linux/macOS)");

mod eviction;
mod memory;
mod protocol;
mod registry;
mod stream;

use std::{path::Path, path::PathBuf, sync::Arc, time::Instant};

use anyhow::{Result, anyhow};
pub use eviction::EvictionLoop;
pub use protocol::*;
pub use registry::{
    Lease, RegistryCounters, RegistrySnapshot, SessionHandle, SessionKey, SessionRegistry,
    SessionSnapshot,
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::watch,
};
use url::Url;

pub struct Daemon {
    root: PathBuf,
    config: lspee_config::ResolvedConfig,
    registry: SessionRegistry,
    started_at: Instant,
}

impl Daemon {
    #[must_use]
    pub fn new(root: PathBuf, config: lspee_config::ResolvedConfig) -> Self {
        let idle_ttl =
            std::time::Duration::from_secs(config.merged.session.idle_ttl_secs);
        Self {
            root,
            config,
            registry: SessionRegistry::with_idle_ttl(idle_ttl),
            started_at: Instant::now(),
        }
    }

    #[must_use]
    pub fn registry(&self) -> SessionRegistry {
        self.registry.clone()
    }

    fn control_socket_path(&self) -> PathBuf {
        self.root.join(".lspee").join("daemon.sock")
    }

    fn memory_settings(&self) -> memory::MemoryBudgetSettings {
        memory::MemoryBudgetSettings {
            max_session_bytes: self
                .config
                .merged
                .memory
                .max_session_mb
                .map(|mb| mb * 1024 * 1024),
            max_total_bytes: self
                .config
                .merged
                .memory
                .max_total_mb
                .map(|mb| mb * 1024 * 1024),
            check_interval: std::time::Duration::from_millis(
                self.config.merged.memory.check_interval_ms,
            ),
        }
    }

    pub async fn run(&self) -> Result<()> {
        tracing::info!(
            root = ?self.root,
            default_lsp = %self.config.merged.lsp.id,
            "starting daemon runtime"
        );

        let eviction_loop = EvictionLoop::start(self.registry.clone());
        let memory_monitor =
            memory::MemoryMonitor::start(self.registry.clone(), self.memory_settings());

        let socket_path = self.control_socket_path();
        if let Some(parent) = socket_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if tokio::fs::try_exists(&socket_path).await.unwrap_or(false) {
            let _ = tokio::fs::remove_file(&socket_path).await;
        }

        let listener = UnixListener::bind(&socket_path)?;
        tracing::info!(path = %socket_path.display(), "daemon control socket listening");

        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        let memory_settings = self.memory_settings();

        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let (stream, _) = accepted?;
                    let registry = self.registry.clone();
                    let uptime_from = self.started_at;
                    let shutdown_tx = shutdown_tx.clone();
                    tokio::spawn(async move {
                        if let Err(error) = handle_control_connection(
                            stream,
                            registry,
                            uptime_from,
                            memory_settings,
                            shutdown_tx,
                        ).await {
                            tracing::warn!(error = ?error, "control connection ended with error");
                        }
                    });
                }
                changed = shutdown_rx.changed() => {
                    if changed.is_ok() && *shutdown_rx.borrow() {
                        tracing::info!("shutdown signal received");
                        break;
                    }
                }
            }
        }

        memory_monitor.shutdown().await;
        eviction_loop.shutdown().await;
        shutdown_all_sessions(&self.registry).await;

        if tokio::fs::try_exists(&socket_path).await.unwrap_or(false) {
            let _ = tokio::fs::remove_file(&socket_path).await;
        }

        tracing::info!("daemon stopped");
        Ok(())
    }
}

async fn handle_control_connection(
    stream: UnixStream,
    registry: SessionRegistry,
    started_at: Instant,
    memory_settings: memory::MemoryBudgetSettings,
    shutdown_tx: watch::Sender<bool>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        if line.len() > MAX_FRAME_SIZE_BYTES {
            let err = error_envelope(
                None,
                ERROR_FRAME_TOO_LARGE,
                "Control frame exceeds 1 MiB maximum",
                false,
                None,
            );
            write_envelope(&mut writer, &err).await?;
            break;
        }

        let raw: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                let err =
                    error_envelope(None, ERROR_BAD_MESSAGE, "Invalid JSON frame", false, None);
                write_envelope(&mut writer, &err).await?;
                continue;
            }
        };

        let id = raw.get("id").and_then(Value::as_str).map(ToOwned::to_owned);
        let version = raw.get("v").and_then(Value::as_u64).map(|v| v as u32);
        if version != Some(PROTOCOL_VERSION) {
            let err = error_envelope(
                id,
                ERROR_UNSUPPORTED_VERSION,
                "Unsupported protocol version",
                false,
                None,
            );
            write_envelope(&mut writer, &err).await?;
            break;
        }

        let req: ControlEnvelope<Value> = match serde_json::from_value(raw) {
            Ok(v) => v,
            Err(_) => {
                let err = error_envelope(
                    None,
                    ERROR_BAD_MESSAGE,
                    "Malformed control envelope",
                    false,
                    None,
                );
                write_envelope(&mut writer, &err).await?;
                continue;
            }
        };

        let outcome =
            dispatch_control_request(req, &registry, started_at, memory_settings, &shutdown_tx)
                .await;
        write_envelope(&mut writer, &outcome.response).await?;

        if outcome.shutdown_requested {
            break;
        }
    }

    Ok(())
}

struct DispatchOutcome {
    response: ControlEnvelope<Value>,
    shutdown_requested: bool,
}

async fn dispatch_control_request(
    req: ControlEnvelope<Value>,
    registry: &SessionRegistry,
    started_at: Instant,
    memory_settings: memory::MemoryBudgetSettings,
    shutdown_tx: &watch::Sender<bool>,
) -> DispatchOutcome {
    let ControlEnvelope {
        id,
        message_type,
        payload,
        ..
    } = req;

    match message_type.as_str() {
        TYPE_ATTACH => dispatch_attach(id, payload, registry).await,
        TYPE_RELEASE => dispatch_release(id, payload, registry).await,
        TYPE_CALL => dispatch_call(id, payload, registry).await,
        TYPE_STATS => dispatch_stats(id, registry, started_at, memory_settings).await,
        TYPE_SHUTDOWN => dispatch_shutdown(id, payload, shutdown_tx).await,
        _ => DispatchOutcome {
            response: error_envelope(
                id,
                ERROR_UNKNOWN_TYPE,
                "Unknown control message type",
                false,
                None,
            ),
            shutdown_requested: false,
        },
    }
}

async fn dispatch_attach(
    id: Option<String>,
    payload: Value,
    registry: &SessionRegistry,
) -> DispatchOutcome {
    let payload: Attach = match serde_json::from_value(payload) {
        Ok(payload) => payload,
        Err(_) => {
            return DispatchOutcome {
                response: error_envelope(
                    id,
                    ERROR_BAD_MESSAGE,
                    "Invalid Attach payload",
                    false,
                    None,
                ),
                shutdown_requested: false,
            };
        }
    };

    if let Err(message) = validate_session_key(&payload.session_key) {
        return DispatchOutcome {
            response: error_envelope(id, ERROR_INVALID_SESSION_KEY, &message, false, None),
            shutdown_requested: false,
        };
    }

    let requested_stream_mode = preferred_stream_mode(payload.capabilities.as_ref());
    let session_root = PathBuf::from(&payload.session_key.project_root);
    let lsp_id = payload.session_key.lsp_id;
    let config_hash = payload.session_key.config_hash;
    let client_kind = payload.client_meta.client_kind.clone();
    let key = SessionKey::new(session_root.clone(), lsp_id.clone(), config_hash);

    let response = match registry
        .acquire_or_spawn(key.clone(), client_kind, move |spawn_key| {
            let spawn_root = session_root.clone();
            let spawn_lsp_id = lsp_id.clone();
            async move {
                let mut resolved =
                    lspee_config::resolve(Some(&spawn_root)).map_err(anyhow::Error::from)?;

                apply_lsp_runtime_defaults(&mut resolved, &spawn_root, &spawn_lsp_id)?;

                let transport = Arc::new(lspee_lsp::LspTransport::new(spawn_root.clone()));
                let runtime = Arc::new(transport.spawn(&resolved).await?);
                let initialize_result =
                    bootstrap_lsp_session(&runtime, &spawn_root, &resolved).await?;
                let (events, _) = tokio::sync::broadcast::channel(32);

                Ok(SessionHandle {
                    key: spawn_key,
                    transport,
                    runtime,
                    initialize_result,
                    events,
                })
            }
        })
        .await
    {
        Ok(lease) => {
            let handle = match registry.session_handle(&key).await {
                Some(handle) => handle,
                None => {
                    return DispatchOutcome {
                        response: error_envelope(
                            id,
                            ERROR_INTERNAL,
                            "session handle missing after attach",
                            true,
                            None,
                        ),
                        shutdown_requested: false,
                    };
                }
            };

            let stream = match requested_stream_mode {
                StreamMode::Dedicated => match stream::spawn_dedicated_stream_endpoint(
                    &key.root,
                    lease.lease_id(),
                    handle.clone(),
                    registry.clone(),
                )
                .await
                {
                    Ok(endpoint) => StreamInfo {
                        mode: StreamMode::Dedicated,
                        endpoint: Some(format!("unix://{}", endpoint.display())),
                    },
                    Err(error) => {
                        let _ = registry.release_by_lease_id(lease.lease_id()).await;
                        return DispatchOutcome {
                            response: error_envelope(
                                id,
                                ERROR_INTERNAL,
                                &format!("failed to create dedicated stream endpoint: {error}"),
                                true,
                                None,
                            ),
                            shutdown_requested: false,
                        };
                    }
                },
                StreamMode::MuxControl => StreamInfo {
                    mode: StreamMode::MuxControl,
                    endpoint: None,
                },
            };

            let body = AttachOk {
                lease_id: lease.lease_id().to_string(),
                session_id: None,
                stream,
                server: Some(ServerInfo {
                    state: "Ready".to_string(),
                    reused: true,
                }),
                initialize_result: Some(handle.initialize_result.clone()),
            };
            ok_envelope(id, TYPE_ATTACH_OK, body)
        }
        Err(error) => error_envelope(
            id,
            ERROR_SESSION_SPAWN_FAILED,
            &error.to_string(),
            true,
            None,
        ),
    };

    DispatchOutcome {
        response,
        shutdown_requested: false,
    }
}

async fn dispatch_release(
    id: Option<String>,
    payload: Value,
    registry: &SessionRegistry,
) -> DispatchOutcome {
    let payload: Release = match serde_json::from_value(payload) {
        Ok(payload) => payload,
        Err(_) => {
            return DispatchOutcome {
                response: error_envelope(
                    id,
                    ERROR_BAD_MESSAGE,
                    "Invalid Release payload",
                    false,
                    None,
                ),
                shutdown_requested: false,
            };
        }
    };

    let response = match registry.release_by_lease_id(&payload.lease_id).await {
        Some(ref_count) => ok_envelope(
            id,
            TYPE_RELEASE_OK,
            ReleaseOk {
                lease_id: payload.lease_id,
                ref_count: ref_count as u64,
            },
        ),
        None => error_envelope(id, ERROR_LEASE_NOT_FOUND, "Lease not found", false, None),
    };

    DispatchOutcome {
        response,
        shutdown_requested: false,
    }
}

async fn dispatch_call(
    id: Option<String>,
    payload: Value,
    registry: &SessionRegistry,
) -> DispatchOutcome {
    let payload: Call = match serde_json::from_value(payload) {
        Ok(payload) => payload,
        Err(_) => {
            return DispatchOutcome {
                response: error_envelope(
                    id,
                    ERROR_BAD_MESSAGE,
                    "Invalid Call payload",
                    false,
                    None,
                ),
                shutdown_requested: false,
            };
        }
    };

    let lease_id = payload.lease_id.clone();
    let response = match registry.call_by_lease_id(&lease_id, payload.request).await {
        Ok(Some(response)) => ok_envelope(id, TYPE_CALL_OK, CallOk { lease_id, response }),
        Ok(None) => error_envelope(id, ERROR_LEASE_NOT_FOUND, "Lease not found", false, None),
        Err(error) => {
            let message = error.to_string();
            let (code, retryable) = if message.starts_with(ERROR_SESSION_EVICTED_MEMORY) {
                (ERROR_SESSION_EVICTED_MEMORY, true)
            } else {
                (ERROR_INTERNAL, true)
            };
            error_envelope(id, code, &message, retryable, None)
        }
    };

    DispatchOutcome {
        response,
        shutdown_requested: false,
    }
}

async fn dispatch_stats(
    id: Option<String>,
    registry: &SessionRegistry,
    started_at: Instant,
    memory_settings: memory::MemoryBudgetSettings,
) -> DispatchOutcome {
    let snapshot = registry.snapshot().await;
    let memory_total_bytes = memory::total_memory_bytes(registry).await;
    let body = StatsOk {
        sessions: snapshot.sessions.len() as u64,
        leases: snapshot.lease_count as u64,
        uptime_ms: started_at.elapsed().as_millis() as u64,
        counters: StatsCounters {
            sessions_spawned_total: snapshot.counters.sessions_spawned_total,
            sessions_reused_total: snapshot.counters.sessions_reused_total,
            sessions_gc_idle_total: snapshot.counters.sessions_gc_idle_total,
            sessions_evicted_memory_total: snapshot.counters.sessions_evicted_memory_total,
            session_crashes_total: snapshot.counters.session_crashes_total,
            attach_requests_total: snapshot.counters.attach_requests_total,
        },
        memory: MemoryStats {
            total_bytes: memory_total_bytes,
            max_total_bytes: memory_settings.max_total_bytes,
            max_session_bytes: memory_settings.max_session_bytes,
        },
    };

    DispatchOutcome {
        response: ok_envelope(id, TYPE_STATS_OK, body),
        shutdown_requested: false,
    }
}

async fn dispatch_shutdown(
    id: Option<String>,
    payload: Value,
    shutdown_tx: &watch::Sender<bool>,
) -> DispatchOutcome {
    let payload: Shutdown = match serde_json::from_value(payload) {
        Ok(payload) => payload,
        Err(_) => {
            return DispatchOutcome {
                response: error_envelope(
                    id,
                    ERROR_BAD_MESSAGE,
                    "Invalid Shutdown payload",
                    false,
                    None,
                ),
                shutdown_requested: false,
            };
        }
    };
    let _ = payload;

    let _ = shutdown_tx.send(true);
    DispatchOutcome {
        response: ok_envelope(id, TYPE_SHUTDOWN_OK, ShutdownOk { accepted: true }),
        shutdown_requested: true,
    }
}

fn preferred_stream_mode(capabilities: Option<&AttachCapabilities>) -> StreamMode {
    let Some(capabilities) = capabilities else {
        return StreamMode::MuxControl;
    };

    if capabilities
        .stream_mode
        .iter()
        .any(|mode| matches!(mode, StreamMode::Dedicated))
    {
        StreamMode::Dedicated
    } else {
        StreamMode::MuxControl
    }
}

fn apply_lsp_runtime_defaults(
    resolved: &mut lspee_config::ResolvedConfig,
    project_root: &Path,
    requested_lsp_id: &str,
) -> Result<()> {
    let existing_lsp_id = resolved.merged.lsp.id.clone();
    let existing_command = resolved.merged.lsp.command.trim().to_string();

    let user_config = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".config/lspee/config.toml"));
    let project_config = project_root.join("lspee.toml");

    let catalog = lspee_config::languages::lsp_for_id(
        requested_lsp_id,
        user_config.as_deref(),
        Some(project_config.as_path()),
    )
    .map_err(anyhow::Error::from)?;

    let explicit_config_for_requested_id =
        existing_lsp_id == requested_lsp_id && !existing_command.is_empty();

    resolved.merged.lsp.id = requested_lsp_id.to_string();

    if explicit_config_for_requested_id {
        if resolved.merged.lsp.args.is_empty() {
            if let Some(catalog) = catalog {
                resolved.merged.lsp.args = catalog.args;
            }
        }
        return Ok(());
    }

    if let Some(catalog) = catalog {
        resolved.merged.lsp.command = catalog.command;
        resolved.merged.lsp.args = catalog.args;
        if resolved.merged.root_markers.is_empty() {
            resolved.merged.root_markers = catalog.root_markers;
        }
    }

    if resolved.merged.lsp.command.trim().is_empty() {
        return Err(anyhow!(
            "no command found for lsp id '{requested_lsp_id}'. set [lsp].command or add a catalog entry"
        ));
    }

    Ok(())
}

async fn bootstrap_lsp_session(
    runtime: &lspee_lsp::LspRuntime,
    project_root: &Path,
    resolved: &lspee_config::ResolvedConfig,
) -> Result<Value> {
    let mut params = json!({
        "processId": null,
        "capabilities": {}
    });

    if let Ok(root_uri) = Url::from_directory_path(project_root) {
        params["rootUri"] = Value::String(root_uri.to_string());
    }

    if !resolved.merged.lsp.initialization_options.is_empty() {
        let options = serde_json::to_value(&resolved.merged.lsp.initialization_options)?;
        params["initializationOptions"] = options;
    }

    let initialize_result = runtime
        .call(json!({
            "jsonrpc": "2.0",
            "id": "lspee-initialize",
            "method": "initialize",
            "params": params
        }))
        .await?;

    runtime
        .send(json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }))
        .await?;

    Ok(initialize_result)
}

async fn shutdown_all_sessions(registry: &SessionRegistry) {
    let handles = registry.all_handles().await;

    for handle in handles {
        if let Err(error) = handle.runtime.shutdown().await {
            tracing::warn!(key = ?handle.key, ?error, "failed graceful session shutdown during daemon stop");
            if let Err(force_error) = handle.runtime.force_stop().await {
                tracing::error!(key = ?handle.key, ?force_error, "failed force-stop during daemon stop");
            }
        }

        registry.remove(&handle.key).await;
    }
}

fn validate_session_key(key: &SessionKeyWire) -> std::result::Result<(), String> {
    let project_root = std::path::Path::new(&key.project_root);
    if key.project_root.is_empty() || !project_root.is_absolute() {
        return Err("session_key.project_root must be an absolute non-empty path".to_string());
    }
    if key.config_hash.trim().is_empty() {
        return Err("session_key.config_hash must be non-empty".to_string());
    }
    if key.lsp_id.is_empty()
        || !key
            .lsp_id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
    {
        return Err("session_key.lsp_id must match ^[a-z0-9._-]+$".to_string());
    }
    Ok(())
}

fn ok_envelope<T: serde::Serialize>(
    id: Option<String>,
    message_type: &str,
    payload: T,
) -> ControlEnvelope<Value> {
    ControlEnvelope {
        v: PROTOCOL_VERSION,
        id,
        message_type: message_type.to_string(),
        payload: serde_json::to_value(payload).unwrap_or(Value::Null),
    }
}

fn error_envelope(
    id: Option<String>,
    code: &str,
    message: &str,
    retryable: bool,
    details: Option<Value>,
) -> ControlEnvelope<Value> {
    ControlEnvelope {
        v: PROTOCOL_VERSION,
        id,
        message_type: TYPE_ERROR.to_string(),
        payload: serde_json::to_value(ErrorResponse {
            code: code.to_string(),
            message: message.to_string(),
            retryable,
            details,
        })
        .unwrap_or(Value::Null),
    }
}

async fn write_envelope(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    envelope: &ControlEnvelope<Value>,
) -> Result<()> {
    let mut bytes = serde_json::to_vec(envelope)?;
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_session_key;
    use crate::protocol::SessionKeyWire;

    #[test]
    fn validate_session_key_accepts_absolute_paths() {
        let cwd = std::env::current_dir().expect("cwd should resolve");
        let key = SessionKeyWire {
            project_root: cwd.display().to_string(),
            config_hash: "abc123".to_string(),
            lsp_id: "rust-analyzer".to_string(),
        };

        assert!(validate_session_key(&key).is_ok());
    }

    #[test]
    fn validate_session_key_rejects_invalid_lsp_id() {
        let cwd = std::env::current_dir().expect("cwd should resolve");
        let key = SessionKeyWire {
            project_root: cwd.display().to_string(),
            config_hash: "abc123".to_string(),
            lsp_id: "Rust Analyzer".to_string(),
        };

        assert!(validate_session_key(&key).is_err());
    }
}
