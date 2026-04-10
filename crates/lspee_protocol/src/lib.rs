use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_SIZE_BYTES: usize = 1_048_576;

pub const TYPE_ATTACH: &str = "Attach";
pub const TYPE_ATTACH_OK: &str = "AttachOk";
pub const TYPE_RELEASE: &str = "Release";
pub const TYPE_RELEASE_OK: &str = "ReleaseOk";
pub const TYPE_CALL: &str = "Call";
pub const TYPE_CALL_OK: &str = "CallOk";
pub const TYPE_PING: &str = "Ping";
pub const TYPE_PONG: &str = "Pong";
pub const TYPE_SHUTDOWN: &str = "Shutdown";
pub const TYPE_SHUTDOWN_OK: &str = "ShutdownOk";
pub const TYPE_STATS: &str = "Stats";
pub const TYPE_STATS_OK: &str = "StatsOk";
pub const TYPE_ERROR: &str = "Error";

pub const STREAM_TYPE_LSP_IN: &str = "LspIn";
pub const STREAM_TYPE_LSP_OUT: &str = "LspOut";
pub const STREAM_TYPE_STREAM_ERROR: &str = "StreamError";

pub const ERROR_UNSUPPORTED_VERSION: &str = "E_UNSUPPORTED_VERSION";
pub const ERROR_BAD_MESSAGE: &str = "E_BAD_MESSAGE";
pub const ERROR_FRAME_TOO_LARGE: &str = "E_FRAME_TOO_LARGE";
pub const ERROR_UNKNOWN_TYPE: &str = "E_UNKNOWN_TYPE";
pub const ERROR_TIMEOUT: &str = "E_TIMEOUT";
pub const ERROR_INTERNAL: &str = "E_INTERNAL";
pub const ERROR_INVALID_SESSION_KEY: &str = "E_INVALID_SESSION_KEY";
pub const ERROR_SESSION_SPAWN_FAILED: &str = "E_SESSION_SPAWN_FAILED";
pub const ERROR_SESSION_INIT_FAILED: &str = "E_SESSION_INIT_FAILED";
pub const ERROR_SESSION_RESTARTING: &str = "E_SESSION_RESTARTING";
pub const ERROR_DAEMON_SHUTTING_DOWN: &str = "E_DAEMON_SHUTTING_DOWN";
pub const ERROR_RESOURCE_LIMIT: &str = "E_RESOURCE_LIMIT";
pub const ERROR_LEASE_NOT_FOUND: &str = "E_LEASE_NOT_FOUND";
pub const ERROR_LEASE_OWNERSHIP: &str = "E_LEASE_OWNERSHIP";
pub const ERROR_AUTH_FAILED: &str = "E_AUTH_FAILED";
pub const ERROR_PERMISSION_DENIED: &str = "E_PERMISSION_DENIED";
pub const ERROR_SESSION_EVICTED_MEMORY: &str = "E_SESSION_EVICTED_MEMORY";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlEnvelope<T> {
    pub v: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub message_type: String,
    pub payload: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ControlPayload {
    Attach(Attach),
    AttachOk(AttachOk),
    Release(Release),
    ReleaseOk(ReleaseOk),
    Call(Call),
    CallOk(CallOk),
    Ping(Ping),
    Pong(Pong),
    Shutdown(Shutdown),
    ShutdownOk(ShutdownOk),
    Stats(Stats),
    StatsOk(StatsOk),
    Error(ErrorResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attach {
    pub session_key: SessionKeyWire,
    pub client_meta: ClientMeta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<AttachCapabilities>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionKeyWire {
    pub project_root: String,
    pub config_hash: String,
    pub lsp_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientMeta {
    pub client_name: String,
    pub client_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_kind: Option<ClientKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Editor,
    Agent,
    Human,
    Ci,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachCapabilities {
    pub stream_mode: Vec<StreamMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamMode {
    Dedicated,
    MuxControl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachOk {
    pub lease_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub stream: StreamInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<ServerInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initialize_result: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    pub mode: StreamMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub state: String,
    pub reused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub lease_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<ReleaseReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseReason {
    ClientExit,
    ClientDisconnect,
    Shutdown,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseOk {
    pub lease_id: String,
    pub ref_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Call {
    pub lease_id: String,
    pub request: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallOk {
    pub lease_id: String,
    pub response: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ping {
    pub ts_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pong {
    pub ts_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Shutdown {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownOk {
    pub accepted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Stats {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsOk {
    pub sessions: u64,
    pub leases: u64,
    pub uptime_ms: u64,
    pub counters: StatsCounters,
    pub memory: MemoryStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_total_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_session_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsCounters {
    pub sessions_spawned_total: u64,
    pub sessions_reused_total: u64,
    pub sessions_gc_idle_total: u64,
    pub sessions_evicted_memory_total: u64,
    pub session_crashes_total: u64,
    pub attach_requests_total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamFrame<T = Value> {
    pub v: u32,
    #[serde(rename = "type")]
    pub frame_type: StreamFrameType,
    pub lease_id: String,
    pub seq: u64,
    pub payload: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamFrameType {
    LspIn,
    LspOut,
    StreamError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamErrorPayload {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn control_envelope_roundtrips_for_call_payload() {
        let envelope = ControlEnvelope {
            v: PROTOCOL_VERSION,
            id: Some("req-1".to_string()),
            message_type: TYPE_CALL.to_string(),
            payload: serde_json::to_value(Call {
                lease_id: "lease_42".to_string(),
                request: json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {}
                }),
            })
            .expect("call payload should serialize"),
        };

        let encoded = serde_json::to_string(&envelope).expect("envelope should encode");
        let decoded: ControlEnvelope<Value> =
            serde_json::from_str(&encoded).expect("envelope should decode");

        assert_eq!(decoded.v, PROTOCOL_VERSION);
        assert_eq!(decoded.id.as_deref(), Some("req-1"));
        assert_eq!(decoded.message_type, TYPE_CALL);
        assert_eq!(decoded.payload["lease_id"], "lease_42");
    }

    #[test]
    fn stream_frame_roundtrips_with_lsp_out_payload() {
        let frame = StreamFrame {
            v: PROTOCOL_VERSION,
            frame_type: StreamFrameType::LspOut,
            lease_id: "lease_7".to_string(),
            seq: 9,
            payload: json!({
                "jsonrpc": "2.0",
                "id": 99,
                "result": {"ok": true}
            }),
        };

        let encoded = serde_json::to_string(&frame).expect("stream frame should encode");
        let decoded: StreamFrame<Value> =
            serde_json::from_str(&encoded).expect("stream frame should decode");

        assert_eq!(decoded.v, PROTOCOL_VERSION);
        assert_eq!(decoded.lease_id, "lease_7");
        assert_eq!(decoded.seq, 9);
        assert_eq!(decoded.payload["id"], 99);
    }
}
