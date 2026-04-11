#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use lspee_daemon::Attach;
use lspee_daemon::AttachCapabilities;
use lspee_daemon::Call;
use lspee_daemon::ClientKind;
use lspee_daemon::ClientMeta;
use lspee_daemon::ControlEnvelope;
use lspee_daemon::Daemon;
use lspee_daemon::Release;
use lspee_daemon::SessionKeyWire;
use lspee_daemon::Shutdown;
use lspee_daemon::Stats;
use lspee_daemon::StreamMode;
use lspee_daemon::TYPE_ATTACH;
use lspee_daemon::TYPE_ATTACH_OK;
use lspee_daemon::TYPE_CALL;
use lspee_daemon::TYPE_CALL_OK;
use lspee_daemon::TYPE_RELEASE;
use lspee_daemon::TYPE_RELEASE_OK;
use lspee_daemon::TYPE_SHUTDOWN;
use lspee_daemon::TYPE_STATS;
use lspee_daemon::TYPE_STATS_OK;
use serde_json::Value;
use serde_json::json;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::task::JoinHandle;
use tokio::time::sleep;

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
	write_project_config_with_extras(root, "");
}

fn write_project_config_with_extras(root: &Path, extras: &str) {
	let config = format!(
		r#"
workspace_mode = "single"

[[lsp]]
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
	let attach_id = format!("attach-{config_hash}-{stream_mode:?}");
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
		r"
[memory]
max_session_mb = 0
max_total_mb = 0
check_interval_ms = 25
",
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

// ---------------------------------------------------------------------------
// Daemon persistence and auto-shutdown tests
//
// These tests verify the behaviors documented in
// docs/src/advanced/daemon-internals.md:
//   - idle sessions are evicted after idle_ttl_secs
//   - daemon auto-shuts down after daemon_idle_ttl_secs with zero sessions
//   - daemon stays alive while sessions are active
// ---------------------------------------------------------------------------

/// Verify that a session with no active leases is evicted after the
/// configured idle TTL expires, and that the daemon reports the eviction
/// in its stats counters.
///
/// Covers: docs/src/includes/session-idle-config.md
#[tokio::test]
async fn idle_session_is_evicted_after_ttl() {
    let root = unique_temp_dir("idle-evict");
    // Use a very short idle TTL so the test completes quickly.
    write_project_config_with_extras(
        &root,
        r"
[session]
idle_ttl_secs = 1
",
    );

    let daemon_task = spawn_daemon(&root);
    let socket_path = root.join(".lspee").join("daemon.sock");
    let stream = connect_with_retry(&socket_path).await;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    // Attach and immediately release so the session becomes unleased.
    let lease_id = attach(&mut writer, &mut lines, &root, "hash-idle").await;
    release(&mut writer, &mut lines, &lease_id).await;

    // Wait for eviction to kick in (idle_ttl=1s, eviction tick=1s, plus margin).
    sleep(Duration::from_secs(5)).await;

    let stats = request(
        &mut writer,
        &mut lines,
        "stats-idle",
        TYPE_STATS,
        serde_json::to_value(Stats::default()).expect("stats payload should serialize"),
    )
    .await;

    assert_eq!(stats.message_type, TYPE_STATS_OK);
    assert_eq!(
        stats.payload["sessions"], 0,
        "session should have been evicted"
    );
    assert!(
        stats.payload["counters"]["sessions_gc_idle_total"]
            .as_u64()
            .unwrap_or(0)
            >= 1,
        "idle eviction counter should be incremented"
    );

    shutdown_daemon(&mut writer, &mut lines).await;
    let _ = daemon_task.await;
    let _ = fs::remove_dir_all(root);
}

/// Verify that the daemon auto-shuts down after daemon_idle_ttl_secs
/// when it has no sessions. The daemon task should complete on its own
/// without an explicit Shutdown request.
///
/// Covers: docs/src/advanced/daemon-internals.md (daemon auto-shutdown)
#[tokio::test]
async fn daemon_auto_shuts_down_when_idle() {
    let root = unique_temp_dir("daemon-idle");
    // Set a very short daemon idle TTL (2s) and session TTL (1s).
    write_project_config_with_extras(
        &root,
        r"
[session]
idle_ttl_secs = 1
daemon_idle_ttl_secs = 2
",
    );

    let daemon_task = spawn_daemon(&root);
    let socket_path = root.join(".lspee").join("daemon.sock");

    // Connect and create a session, then release it.
    let stream = connect_with_retry(&socket_path).await;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let lease_id = attach(&mut writer, &mut lines, &root, "hash-autostop").await;
    release(&mut writer, &mut lines, &lease_id).await;

    // Drop the control connection — daemon should eventually shut itself down.
    drop(writer);
    drop(lines);

    // Wait for session eviction (1s) + daemon idle TTL (2s) + margin.
    let result = tokio::time::timeout(Duration::from_secs(8), daemon_task).await;
    assert!(
        result.is_ok(),
        "daemon should have auto-shut down within the timeout"
    );

    // Socket should have been removed.
    assert!(
        !socket_path.exists(),
        "daemon socket should be removed after auto-shutdown"
    );

    let _ = fs::remove_dir_all(root);
}

/// Verify that the daemon stays alive as long as active sessions exist,
/// even after daemon_idle_ttl_secs would have expired for an empty daemon.
///
/// Covers: docs/src/advanced/daemon-internals.md (daemon stays alive with sessions)
#[tokio::test]
async fn daemon_stays_alive_while_sessions_active() {
    let root = unique_temp_dir("daemon-alive");
    // Very short daemon idle TTL, but we'll keep a session active.
    write_project_config_with_extras(
        &root,
        r"
[session]
idle_ttl_secs = 300
daemon_idle_ttl_secs = 2
",
    );

    let daemon_task = spawn_daemon(&root);
    let socket_path = root.join(".lspee").join("daemon.sock");
    let stream = connect_with_retry(&socket_path).await;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    // Attach but do NOT release — session remains active.
    let _lease_id = attach(&mut writer, &mut lines, &root, "hash-alive").await;

    // Wait longer than daemon_idle_ttl_secs.
    sleep(Duration::from_secs(4)).await;

    // Daemon should still be alive — verify via stats.
    let stats = request(
        &mut writer,
        &mut lines,
        "stats-alive",
        TYPE_STATS,
        serde_json::to_value(Stats::default()).expect("stats payload should serialize"),
    )
    .await;

    assert_eq!(stats.message_type, TYPE_STATS_OK);
    assert_eq!(stats.payload["sessions"], 1, "session should still exist");

    shutdown_daemon(&mut writer, &mut lines).await;
    let _ = daemon_task.await;
    let _ = fs::remove_dir_all(root);
}
