#![cfg(unix)]

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use lspee_daemon::{
    Attach, AttachCapabilities, Call, ClientKind, ClientMeta, ControlEnvelope, Daemon, Release,
    SessionKeyWire, Shutdown, Stats, StreamMode, TYPE_ATTACH, TYPE_ATTACH_OK, TYPE_CALL,
    TYPE_CALL_OK, TYPE_RELEASE, TYPE_RELEASE_OK, TYPE_SHUTDOWN, TYPE_STATS, TYPE_STATS_OK,
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    task::JoinHandle,
    time::sleep,
};

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be valid")
        .as_nanos();
    let dir = PathBuf::from("/tmp").join(format!("lspee-daemon-test-{name}-{nanos}"));
    fs::create_dir_all(&dir).expect("temp dir should be created");
    fs::canonicalize(&dir).expect("temp dir should canonicalize")
}

fn write_project_config(root: &Path) {
    write_project_config_with_extras(root, "")
}

fn write_project_config_with_extras(root: &Path, extras: &str) {
    let config = format!(
        r#"
workspace_mode = "single"

[lsp]
id = "rust-analyzer"
command = "cat"
args = []

{extras}
"#
    );

    fs::write(root.join("lspee.toml"), config).expect("project config should be written");
}

fn spawn_daemon(root: &Path) -> JoinHandle<anyhow::Result<()>> {
    let resolved = lspee_config::resolve(Some(root)).expect("config should resolve");
    let daemon = Daemon::new(root.to_path_buf(), resolved);
    tokio::spawn(async move { daemon.run().await })
}

async fn connect_with_retry(socket_path: &Path) -> UnixStream {
    for _ in 0..100 {
        match UnixStream::connect(socket_path).await {
            Ok(stream) => return stream,
            Err(_) => sleep(Duration::from_millis(25)).await,
        }
    }

    panic!(
        "failed to connect to daemon socket {}",
        socket_path.display()
    );
}

async fn write_frame(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    envelope: &ControlEnvelope<Value>,
) {
    let mut bytes = serde_json::to_vec(envelope).expect("envelope should serialize");
    bytes.push(b'\n');
    writer
        .write_all(&bytes)
        .await
        .expect("write should succeed");
    writer.flush().await.expect("flush should succeed");
}

async fn read_for_id(
    lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    expected_id: &str,
) -> ControlEnvelope<Value> {
    loop {
        let Some(line) = lines
            .next_line()
            .await
            .expect("read line from daemon should succeed")
        else {
            panic!("daemon closed stream before returning response")
        };

        let response: ControlEnvelope<Value> =
            serde_json::from_str(&line).expect("daemon response should decode");
        if response.id.as_deref() == Some(expected_id) {
            return response;
        }
    }
}

async fn request(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    id: &str,
    message_type: &str,
    payload: Value,
) -> ControlEnvelope<Value> {
    let envelope = ControlEnvelope {
        v: lspee_daemon::PROTOCOL_VERSION,
        id: Some(id.to_string()),
        message_type: message_type.to_string(),
        payload,
    };

    write_frame(writer, &envelope).await;
    read_for_id(lines, id).await
}

async fn attach(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    root: &Path,
    config_hash: &str,
) -> String {
    let response = attach_with_mode(writer, lines, root, config_hash, StreamMode::MuxControl).await;

    response.payload["lease_id"]
        .as_str()
        .expect("AttachOk should include lease_id")
        .to_string()
}

async fn attach_with_mode(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    root: &Path,
    config_hash: &str,
    stream_mode: StreamMode,
) -> ControlEnvelope<Value> {
    let attach_id = format!("attach-{config_hash}-{:?}", stream_mode);
    let response = request(
        writer,
        lines,
        &attach_id,
        TYPE_ATTACH,
        serde_json::to_value(Attach {
            session_key: SessionKeyWire {
                project_root: root.display().to_string(),
                config_hash: config_hash.to_string(),
                lsp_id: "rust-analyzer".to_string(),
            },
            client_meta: ClientMeta {
                client_name: "integration-test".to_string(),
                client_version: "0.1.0".to_string(),
                client_kind: Some(match &stream_mode {
                    StreamMode::Dedicated => ClientKind::Editor,
                    StreamMode::MuxControl => ClientKind::Agent,
                }),
                pid: None,
                cwd: None,
            },
            capabilities: Some(AttachCapabilities {
                stream_mode: vec![stream_mode],
            }),
        })
        .expect("attach payload should serialize"),
    )
    .await;

    assert_eq!(response.message_type, TYPE_ATTACH_OK);
    response
}

async fn release(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    lease_id: &str,
) {
    let release_id = format!("release-{lease_id}");
    let response = request(
        writer,
        lines,
        &release_id,
        TYPE_RELEASE,
        serde_json::to_value(Release {
            lease_id: lease_id.to_string(),
            reason: None,
        })
        .expect("release payload should serialize"),
    )
    .await;

    assert_eq!(response.message_type, TYPE_RELEASE_OK);
}

async fn shutdown_daemon(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
) {
    let _ = request(
        writer,
        lines,
        "shutdown",
        TYPE_SHUTDOWN,
        serde_json::to_value(Shutdown::default()).expect("shutdown payload should serialize"),
    )
    .await;
}

#[tokio::test]
async fn attach_call_release_and_stats_flow() {
    let root = unique_temp_dir("flow");
    write_project_config(&root);

    let daemon_task = spawn_daemon(&root);
    let socket_path = root.join(".lspee").join("daemon.sock");
    let stream = connect_with_retry(&socket_path).await;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let lease_id = attach(&mut writer, &mut lines, &root, "hash-a").await;

    let call_response = request(
        &mut writer,
        &mut lines,
        "call-1",
        TYPE_CALL,
        serde_json::to_value(Call {
            lease_id: lease_id.clone(),
            request: json!({
                "jsonrpc": "2.0",
                "id": 10,
                "method": "workspace/symbol",
                "params": {"query": "demo"}
            }),
        })
        .expect("call payload should serialize"),
    )
    .await;

    assert_eq!(call_response.message_type, TYPE_CALL_OK);
    assert_eq!(call_response.payload["response"]["id"], 10);

    release(&mut writer, &mut lines, &lease_id).await;

    let stats_response = request(
        &mut writer,
        &mut lines,
        "stats-1",
        TYPE_STATS,
        serde_json::to_value(Stats::default()).expect("stats payload should serialize"),
    )
    .await;

    assert_eq!(stats_response.message_type, TYPE_STATS_OK);
    assert_eq!(stats_response.payload["leases"], 0);

    shutdown_daemon(&mut writer, &mut lines).await;
    let _ = daemon_task.await;

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn same_session_key_reuses_spawned_worker() {
    let root = unique_temp_dir("reuse");
    write_project_config(&root);

    let daemon_task = spawn_daemon(&root);
    let socket_path = root.join(".lspee").join("daemon.sock");
    let stream = connect_with_retry(&socket_path).await;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let lease_1 = attach(&mut writer, &mut lines, &root, "hash-reuse").await;
    release(&mut writer, &mut lines, &lease_1).await;

    let lease_2 = attach(&mut writer, &mut lines, &root, "hash-reuse").await;
    release(&mut writer, &mut lines, &lease_2).await;

    let stats_response = request(
        &mut writer,
        &mut lines,
        "stats-reuse",
        TYPE_STATS,
        serde_json::to_value(Stats::default()).expect("stats payload should serialize"),
    )
    .await;

    assert_eq!(stats_response.message_type, TYPE_STATS_OK);
    assert_eq!(
        stats_response.payload["counters"]["sessions_spawned_total"],
        1
    );
    assert!(
        stats_response.payload["counters"]["sessions_reused_total"]
            .as_u64()
            .expect("sessions_reused_total should be numeric")
            >= 1
    );

    shutdown_daemon(&mut writer, &mut lines).await;
    let _ = daemon_task.await;

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn different_config_hashes_spawn_distinct_sessions() {
    let root = unique_temp_dir("isolation");
    write_project_config(&root);

    let daemon_task = spawn_daemon(&root);
    let socket_path = root.join(".lspee").join("daemon.sock");
    let stream = connect_with_retry(&socket_path).await;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let lease_a = attach(&mut writer, &mut lines, &root, "hash-a").await;
    release(&mut writer, &mut lines, &lease_a).await;

    let lease_b = attach(&mut writer, &mut lines, &root, "hash-b").await;
    release(&mut writer, &mut lines, &lease_b).await;

    let stats_response = request(
        &mut writer,
        &mut lines,
        "stats-isolation",
        TYPE_STATS,
        serde_json::to_value(Stats::default()).expect("stats payload should serialize"),
    )
    .await;

    assert_eq!(stats_response.message_type, TYPE_STATS_OK);
    assert_eq!(
        stats_response.payload["counters"]["sessions_spawned_total"],
        2
    );
    assert_eq!(stats_response.payload["sessions"], 2);

    shutdown_daemon(&mut writer, &mut lines).await;
    let _ = daemon_task.await;

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn dedicated_stream_forwards_bidirectional_lsp_frames() {
    let root = unique_temp_dir("dedicated");
    write_project_config(&root);

    let daemon_task = spawn_daemon(&root);
    let socket_path = root.join(".lspee").join("daemon.sock");
    let control = connect_with_retry(&socket_path).await;
    let (reader, mut writer) = control.into_split();
    let mut lines = BufReader::new(reader).lines();

    let attach_response = attach_with_mode(
        &mut writer,
        &mut lines,
        &root,
        "hash-dedicated",
        StreamMode::Dedicated,
    )
    .await;
    let lease_id = attach_response.payload["lease_id"]
        .as_str()
        .expect("AttachOk should contain lease id")
        .to_string();
    let endpoint = attach_response.payload["stream"]["endpoint"]
        .as_str()
        .expect("AttachOk should include dedicated endpoint")
        .trim_start_matches("unix://")
        .to_string();

    let stream = connect_with_retry(Path::new(&endpoint)).await;
    let (stream_reader, mut stream_writer) = stream.into_split();
    let mut stream_lines = BufReader::new(stream_reader).lines();

    let request_frame = lspee_daemon::StreamFrame {
        v: lspee_daemon::PROTOCOL_VERSION,
        frame_type: lspee_daemon::StreamFrameType::LspIn,
        lease_id: lease_id.clone(),
        seq: 1,
        payload: json!({
            "jsonrpc": "2.0",
            "id": 88,
            "method": "workspace/symbol",
            "params": {"query": "proxy"}
        }),
    };
    let mut bytes = serde_json::to_vec(&request_frame).expect("stream frame should encode");
    bytes.push(b'\n');
    stream_writer
        .write_all(&bytes)
        .await
        .expect("stream write should succeed");
    stream_writer
        .flush()
        .await
        .expect("stream flush should succeed");

    let line = stream_lines
        .next_line()
        .await
        .expect("stream read should succeed")
        .expect("daemon should return LspOut frame");
    let response: lspee_daemon::StreamFrame<Value> =
        serde_json::from_str(&line).expect("stream response should decode");

    assert!(matches!(
        response.frame_type,
        lspee_daemon::StreamFrameType::LspOut
    ));
    assert_eq!(response.payload["id"], 88);

    drop(stream_writer);
    shutdown_daemon(&mut writer, &mut lines).await;
    let _ = daemon_task.await;
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn memory_budget_eviction_emits_stream_error() {
    let root = unique_temp_dir("memory-evict");
    write_project_config_with_extras(
        &root,
        r#"
[memory]
max_session_mb = 0
max_total_mb = 0
check_interval_ms = 25
"#,
    );

    let daemon_task = spawn_daemon(&root);
    let socket_path = root.join(".lspee").join("daemon.sock");
    let control = connect_with_retry(&socket_path).await;
    let (reader, mut writer) = control.into_split();
    let mut lines = BufReader::new(reader).lines();

    let attach_response = attach_with_mode(
        &mut writer,
        &mut lines,
        &root,
        "hash-memory",
        StreamMode::Dedicated,
    )
    .await;
    let endpoint = attach_response.payload["stream"]["endpoint"]
        .as_str()
        .expect("AttachOk should include dedicated endpoint")
        .trim_start_matches("unix://")
        .to_string();

    let stream = connect_with_retry(Path::new(&endpoint)).await;
    let (stream_reader, _stream_writer) = stream.into_split();
    let mut stream_lines = BufReader::new(stream_reader).lines();

    let mut saw_eviction = false;
    for _ in 0..100 {
        if let Some(line) = stream_lines
            .next_line()
            .await
            .expect("stream read should succeed")
        {
            let response: lspee_daemon::StreamFrame<Value> =
                serde_json::from_str(&line).expect("stream response should decode");
            if matches!(
                response.frame_type,
                lspee_daemon::StreamFrameType::StreamError
            ) {
                saw_eviction = true;
                assert_eq!(
                    response.payload["code"],
                    lspee_daemon::ERROR_SESSION_EVICTED_MEMORY
                );
                break;
            }
        }
    }

    assert!(saw_eviction, "expected memory eviction stream error");

    shutdown_daemon(&mut writer, &mut lines).await;
    let _ = daemon_task.await;
    let _ = fs::remove_dir_all(root);
}
