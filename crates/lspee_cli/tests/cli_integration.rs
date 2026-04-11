#![cfg(unix)]

//! Integration tests that exercise CLI command functions against a live daemon.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use lspee_daemon::Daemon;
use tokio::{task::JoinHandle, time::sleep};

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be valid")
        .as_nanos();
    let dir = PathBuf::from("/tmp").join(format!("lspee-cli-test-{name}-{nanos}"));
    fs::create_dir_all(&dir).expect("temp dir should be created");
    fs::canonicalize(&dir).expect("temp dir should canonicalize")
}

fn write_project_config(root: &Path) {
    let config = r#"
workspace_mode = "single"

[[lsp]]
id = "rust-analyzer"
command = "cat"
args = []
"#;
    fs::write(root.join("lspee.toml"), config).expect("project config should be written");
}

fn spawn_daemon(root: &Path) -> JoinHandle<anyhow::Result<()>> {
    let resolved = lspee_config::resolve(Some(root)).expect("config should resolve");
    let daemon = Daemon::new(root.to_path_buf(), resolved);
    tokio::spawn(async move { daemon.run().await })
}

async fn wait_for_socket(root: &Path) {
    let socket = root.join(".lspee").join("daemon.sock");
    for _ in 0..100 {
        if tokio::net::UnixStream::connect(&socket).await.is_ok() {
            return;
        }
        sleep(Duration::from_millis(25)).await;
    }
    panic!("daemon socket did not become available");
}

async fn shutdown_daemon(root: &Path) {
    use lspee_daemon::{ControlEnvelope, PROTOCOL_VERSION, Shutdown, TYPE_SHUTDOWN};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let socket = root.join(".lspee").join("daemon.sock");
    if let Ok(stream) = tokio::net::UnixStream::connect(&socket).await {
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();
        let envelope = ControlEnvelope {
            v: PROTOCOL_VERSION,
            id: Some("shutdown".to_string()),
            message_type: TYPE_SHUTDOWN.to_string(),
            payload: serde_json::to_value(Shutdown::default()).unwrap(),
        };
        let mut bytes = serde_json::to_vec(&envelope).unwrap();
        bytes.push(b'\n');
        let _ = writer.write_all(&bytes).await;
        let _ = writer.flush().await;
        let _ = lines.next_line().await;
    }
}

// ---------------------------------------------------------------------------
// lspee lsp (exercises lsp.rs)
// ---------------------------------------------------------------------------

#[test]
fn lsp_command_runs_successfully() {
    let root = unique_temp_dir("lsp-cmd");
    write_project_config(&root);

    let result = lspee_cli::commands::lsp::run(lspee_cli::commands::lsp::LspCommand {
        project_root: Some(root.clone()),
        output: lspee_cli::commands::lsp::LspOutput::Json,
    });
    assert!(result.is_ok());

    let result = lspee_cli::commands::lsp::run(lspee_cli::commands::lsp::LspCommand {
        project_root: Some(root.clone()),
        output: lspee_cli::commands::lsp::LspOutput::Human,
    });
    assert!(result.is_ok());

    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// lspee lsps (exercises lsps.rs)
// ---------------------------------------------------------------------------

#[test]
fn lsps_command_with_file_json() {
    let root = unique_temp_dir("lsps-cmd");
    write_project_config(&root);

    let result = lspee_cli::commands::lsps::run(lspee_cli::commands::lsps::LspsCommand {
        file: Some(PathBuf::from("src/main.rs")),
        output: lspee_cli::commands::lsps::LspsOutput::Json,
    });
    assert!(result.is_ok());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn lsps_command_with_file_human() {
    let result = lspee_cli::commands::lsps::run(lspee_cli::commands::lsps::LspsCommand {
        file: Some(PathBuf::from("src/main.rs")),
        output: lspee_cli::commands::lsps::LspsOutput::Human,
    });
    assert!(result.is_ok());
}

#[test]
fn lsps_command_without_file() {
    let result = lspee_cli::commands::lsps::run(lspee_cli::commands::lsps::LspsCommand {
        file: None,
        output: lspee_cli::commands::lsps::LspsOutput::Json,
    });
    assert!(result.is_ok());

    let result = lspee_cli::commands::lsps::run(lspee_cli::commands::lsps::LspsCommand {
        file: None,
        output: lspee_cli::commands::lsps::LspsOutput::Human,
    });
    assert!(result.is_ok());
}

#[test]
fn lsps_command_unknown_extension() {
    let result = lspee_cli::commands::lsps::run(lspee_cli::commands::lsps::LspsCommand {
        file: Some(PathBuf::from("file.zzzzzzz")),
        output: lspee_cli::commands::lsps::LspsOutput::Human,
    });
    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// lspee doctor (exercises doctor.rs)
// ---------------------------------------------------------------------------

#[test]
fn doctor_command_json() {
    let root = unique_temp_dir("doctor-cmd");
    write_project_config(&root);

    let result = lspee_cli::commands::doctor::run(lspee_cli::commands::doctor::DoctorCommand {
        project_root: Some(root.clone()),
        output: lspee_cli::commands::doctor::DoctorOutput::Json,
    });
    assert!(result.is_ok());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn doctor_command_human() {
    let root = unique_temp_dir("doctor-human");
    write_project_config(&root);

    let result = lspee_cli::commands::doctor::run(lspee_cli::commands::doctor::DoctorCommand {
        project_root: Some(root.clone()),
        output: lspee_cli::commands::doctor::DoctorOutput::Human,
    });
    assert!(result.is_ok());

    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Helper: spawn daemon in a background thread (not tokio) for CLI run() tests
// ---------------------------------------------------------------------------

fn spawn_daemon_thread(root: &Path) -> std::thread::JoinHandle<()> {
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let resolved = lspee_config::resolve(Some(&root)).unwrap();
            let daemon = Daemon::new(root.clone(), resolved);
            let _ = daemon.run().await;
        });
    })
}

fn wait_for_socket_sync(root: &Path) {
    let socket = root.join(".lspee").join("daemon.sock");
    for _ in 0..100 {
        if std::os::unix::net::UnixStream::connect(&socket).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("daemon socket did not become available");
}

// ---------------------------------------------------------------------------
// lspee status (exercises status.rs + client.rs)
// ---------------------------------------------------------------------------

#[test]
fn status_command_json() {
    let root = unique_temp_dir("status-json");
    write_project_config(&root);

    let daemon_thread = spawn_daemon_thread(&root);
    wait_for_socket_sync(&root);

    let result = lspee_cli::commands::status::run(lspee_cli::commands::status::StatusCommand {
        project_root: Some(root.clone()),
        no_start_daemon: false,
        output: lspee_cli::commands::status::StatusOutput::Json,
    });
    assert!(result.is_ok());

    let result = lspee_cli::commands::status::run(lspee_cli::commands::status::StatusCommand {
        project_root: Some(root.clone()),
        no_start_daemon: false,
        output: lspee_cli::commands::status::StatusOutput::Human,
    });
    assert!(result.is_ok());

    // Stop daemon
    lspee_cli::commands::stop::run(lspee_cli::commands::stop::StopCommand {
        project_root: Some(root.clone()),
    })
    .unwrap();
    let _ = daemon_thread.join();
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// lspee call (exercises call.rs + client.rs)
// ---------------------------------------------------------------------------

#[test]
fn call_command_json_output() {
    let root = unique_temp_dir("call-json");
    write_project_config(&root);

    let daemon_thread = spawn_daemon_thread(&root);
    wait_for_socket_sync(&root);

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "textDocument/hover",
        "params": {}
    });

    let result = lspee_cli::commands::call::run(lspee_cli::commands::call::CallCommand {
        lsp: "rust-analyzer".to_string(),
        root: Some(root.clone()),
        request: serde_json::to_string(&request).unwrap(),
        no_start_daemon: false,
        client_kind: lspee_cli::commands::call::CallClientKind::Agent,
        output: lspee_cli::commands::call::CallOutput::Json,
    });
    assert!(result.is_ok());

    let result = lspee_cli::commands::call::run(lspee_cli::commands::call::CallCommand {
        lsp: "rust-analyzer".to_string(),
        root: Some(root.clone()),
        request: serde_json::to_string(&request).unwrap(),
        no_start_daemon: false,
        client_kind: lspee_cli::commands::call::CallClientKind::Human,
        output: lspee_cli::commands::call::CallOutput::Pretty,
    });
    assert!(result.is_ok());

    lspee_cli::commands::stop::run(lspee_cli::commands::stop::StopCommand {
        project_root: Some(root.clone()),
    })
    .unwrap();
    let _ = daemon_thread.join();
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn call_command_from_file() {
    let root = unique_temp_dir("call-file");
    write_project_config(&root);

    let daemon_thread = spawn_daemon_thread(&root);
    wait_for_socket_sync(&root);

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "workspace/symbol",
        "params": {"query": "test"}
    });
    let req_file = root.join("request.json");
    fs::write(&req_file, serde_json::to_string(&request).unwrap()).unwrap();

    let result = lspee_cli::commands::call::run(lspee_cli::commands::call::CallCommand {
        lsp: "rust-analyzer".to_string(),
        root: Some(root.clone()),
        request: format!("@{}", req_file.display()),
        no_start_daemon: false,
        client_kind: lspee_cli::commands::call::CallClientKind::Ci,
        output: lspee_cli::commands::call::CallOutput::Json,
    });
    assert!(result.is_ok());

    lspee_cli::commands::stop::run(lspee_cli::commands::stop::StopCommand {
        project_root: Some(root.clone()),
    })
    .unwrap();
    let _ = daemon_thread.join();
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// lspee stop (exercises stop.rs + client.rs)
// ---------------------------------------------------------------------------

#[test]
fn stop_command_stops_daemon() {
    let root = unique_temp_dir("stop-fn");
    write_project_config(&root);

    let daemon_thread = spawn_daemon_thread(&root);
    wait_for_socket_sync(&root);

    let result = lspee_cli::commands::stop::run(lspee_cli::commands::stop::StopCommand {
        project_root: Some(root.clone()),
    });
    assert!(result.is_ok());

    let _ = daemon_thread.join();
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn stop_command_when_daemon_not_running() {
    let root = unique_temp_dir("stop-norun");
    write_project_config(&root);

    let result = lspee_cli::commands::stop::run(lspee_cli::commands::stop::StopCommand {
        project_root: Some(root.clone()),
    });
    assert!(result.is_ok());

    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// lspee config (exercises config.rs)
// ---------------------------------------------------------------------------

#[test]
fn config_show_json() {
    let root = unique_temp_dir("config-show");
    write_project_config(&root);

    let result = lspee_cli::commands::config::run(lspee_cli::commands::config::ConfigCommand {
        action: lspee_cli::commands::config::ConfigAction::Show(
            lspee_cli::commands::config::ShowCommand {
                root: Some(root.clone()),
                output: lspee_cli::commands::config::ConfigOutput::Json,
            },
        ),
    });
    assert!(result.is_ok());

    let result = lspee_cli::commands::config::run(lspee_cli::commands::config::ConfigCommand {
        action: lspee_cli::commands::config::ConfigAction::Show(
            lspee_cli::commands::config::ShowCommand {
                root: Some(root.clone()),
                output: lspee_cli::commands::config::ConfigOutput::Human,
            },
        ),
    });
    assert!(result.is_ok());

    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Protocol edge cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn daemon_rejects_invalid_json() {
    let root = unique_temp_dir("bad-json");
    write_project_config(&root);

    let daemon_task = spawn_daemon(&root);
    wait_for_socket(&root).await;

    use lspee_daemon::TYPE_ERROR;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let socket = root.join(".lspee").join("daemon.sock");
    let stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    writer.write_all(b"not json\n").await.unwrap();
    writer.flush().await.unwrap();

    let line = lines.next_line().await.unwrap().unwrap();
    let response: lspee_daemon::ControlEnvelope<serde_json::Value> =
        serde_json::from_str(&line).unwrap();
    assert_eq!(response.message_type, TYPE_ERROR);
    assert_eq!(response.payload["code"], "E_BAD_MESSAGE");

    shutdown_daemon(&root).await;
    let _ = daemon_task.await;
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn daemon_rejects_wrong_protocol_version() {
    let root = unique_temp_dir("bad-version");
    write_project_config(&root);

    let daemon_task = spawn_daemon(&root);
    wait_for_socket(&root).await;

    use lspee_daemon::TYPE_ERROR;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let socket = root.join(".lspee").join("daemon.sock");
    let stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let bad = serde_json::json!({"v": 999, "id": "bad", "type": "Stats", "payload": {}});
    let mut bytes = serde_json::to_vec(&bad).unwrap();
    bytes.push(b'\n');
    writer.write_all(&bytes).await.unwrap();
    writer.flush().await.unwrap();

    let line = lines.next_line().await.unwrap().unwrap();
    let response: lspee_daemon::ControlEnvelope<serde_json::Value> =
        serde_json::from_str(&line).unwrap();
    assert_eq!(response.message_type, TYPE_ERROR);
    assert_eq!(response.payload["code"], "E_UNSUPPORTED_VERSION");

    shutdown_daemon(&root).await;
    let _ = daemon_task.await;
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn daemon_rejects_unknown_message_type() {
    let root = unique_temp_dir("bad-type");
    write_project_config(&root);

    let daemon_task = spawn_daemon(&root);
    wait_for_socket(&root).await;

    use lspee_daemon::{PROTOCOL_VERSION, TYPE_ERROR};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let socket = root.join(".lspee").join("daemon.sock");
    let stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let bad = serde_json::json!({"v": PROTOCOL_VERSION, "id": "x", "type": "Bogus", "payload": {}});
    let mut bytes = serde_json::to_vec(&bad).unwrap();
    bytes.push(b'\n');
    writer.write_all(&bytes).await.unwrap();
    writer.flush().await.unwrap();

    let line = lines.next_line().await.unwrap().unwrap();
    let response: lspee_daemon::ControlEnvelope<serde_json::Value> =
        serde_json::from_str(&line).unwrap();
    assert_eq!(response.message_type, TYPE_ERROR);
    assert_eq!(response.payload["code"], "E_UNKNOWN_TYPE");

    shutdown_daemon(&root).await;
    let _ = daemon_task.await;
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn daemon_rejects_invalid_session_key() {
    let root = unique_temp_dir("bad-key");
    write_project_config(&root);

    let daemon_task = spawn_daemon(&root);
    wait_for_socket(&root).await;

    use lspee_daemon::{
        Attach, AttachCapabilities, ClientKind, ClientMeta, ControlEnvelope, PROTOCOL_VERSION,
        SessionKeyWire, StreamMode, TYPE_ATTACH, TYPE_ERROR,
    };
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let socket = root.join(".lspee").join("daemon.sock");
    let stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let attach = ControlEnvelope {
        v: PROTOCOL_VERSION,
        id: Some("bad-attach".to_string()),
        message_type: TYPE_ATTACH.to_string(),
        payload: serde_json::to_value(Attach {
            session_key: SessionKeyWire {
                project_root: root.display().to_string(),
                config_hash: "hash".to_string(),
                lsp_id: "".to_string(),
            },
            client_meta: ClientMeta {
                client_name: "test".to_string(),
                client_version: "0.1.0".to_string(),
                client_kind: Some(ClientKind::Agent),
                pid: None,
                cwd: None,
            },
            capabilities: Some(AttachCapabilities {
                stream_mode: vec![StreamMode::MuxControl],
            }),
        })
        .unwrap(),
    };
    let mut bytes = serde_json::to_vec(&attach).unwrap();
    bytes.push(b'\n');
    writer.write_all(&bytes).await.unwrap();
    writer.flush().await.unwrap();

    let line = lines.next_line().await.unwrap().unwrap();
    let response: ControlEnvelope<serde_json::Value> = serde_json::from_str(&line).unwrap();
    assert_eq!(response.message_type, TYPE_ERROR);
    assert_eq!(response.payload["code"], "E_INVALID_SESSION_KEY");

    shutdown_daemon(&root).await;
    let _ = daemon_task.await;
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn release_nonexistent_lease_returns_error() {
    let root = unique_temp_dir("bad-lease");
    write_project_config(&root);

    let daemon_task = spawn_daemon(&root);
    wait_for_socket(&root).await;

    use lspee_daemon::{ControlEnvelope, PROTOCOL_VERSION, Release, TYPE_ERROR, TYPE_RELEASE};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let socket = root.join(".lspee").join("daemon.sock");
    let stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let release = ControlEnvelope {
        v: PROTOCOL_VERSION,
        id: Some("release-bad".to_string()),
        message_type: TYPE_RELEASE.to_string(),
        payload: serde_json::to_value(Release {
            lease_id: "nonexistent_lease".to_string(),
            reason: None,
        })
        .unwrap(),
    };
    let mut bytes = serde_json::to_vec(&release).unwrap();
    bytes.push(b'\n');
    writer.write_all(&bytes).await.unwrap();
    writer.flush().await.unwrap();

    let line = lines.next_line().await.unwrap().unwrap();
    let response: ControlEnvelope<serde_json::Value> = serde_json::from_str(&line).unwrap();
    assert_eq!(response.message_type, TYPE_ERROR);
    assert_eq!(response.payload["code"], "E_LEASE_NOT_FOUND");

    shutdown_daemon(&root).await;
    let _ = daemon_task.await;
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Client helpers
// ---------------------------------------------------------------------------

#[test]
fn client_new_request_id_has_prefix() {
    let id = lspee_cli::commands::client::new_request_id("test");
    assert!(id.starts_with("test-"));
}

#[test]
fn client_daemon_socket_path() {
    let path = lspee_cli::commands::client::daemon_socket_path(Path::new("/my/project"));
    assert_eq!(path, PathBuf::from("/my/project/.lspee/daemon.sock"));
}

#[test]
fn client_render_error_payload() {
    let payload = serde_json::json!({"code": "E_TEST", "message": "test error"});
    let rendered = lspee_cli::commands::client::render_error_payload(&payload);
    assert!(rendered.contains("E_TEST"));
    assert!(rendered.contains("test error"));
}

#[test]
fn client_render_error_payload_missing_fields() {
    let payload = serde_json::json!({});
    let rendered = lspee_cli::commands::client::render_error_payload(&payload);
    assert!(rendered.contains("E_UNKNOWN"));
}
