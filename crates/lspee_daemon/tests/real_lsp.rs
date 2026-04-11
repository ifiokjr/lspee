#![cfg(unix)]

//! Integration tests with a real rust-analyzer LSP server.
//!
//! These tests are **ignored by default** because they require `rust-analyzer`
//! to be installed and take 10-30 seconds each while rust-analyzer indexes the
//! fixture project.
//!
//! Run them manually with:
//!
//! ```sh
//! cargo test -p lspee_daemon --test real_lsp -- --ignored --test-threads=1
//! ```

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
use lspee_daemon::Notify;
use lspee_daemon::Release;
use lspee_daemon::SessionKeyWire;
use lspee_daemon::Shutdown;
use lspee_daemon::StreamMode;
use lspee_daemon::TYPE_ATTACH;
use lspee_daemon::TYPE_ATTACH_OK;
use lspee_daemon::TYPE_CALL;
use lspee_daemon::TYPE_CALL_OK;
use lspee_daemon::TYPE_NOTIFY;
use lspee_daemon::TYPE_NOTIFY_OK;
use lspee_daemon::TYPE_RELEASE;
use lspee_daemon::TYPE_RELEASE_OK;
use lspee_daemon::TYPE_SHUTDOWN;
use serde_json::Value;
use serde_json::json;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::task::JoinHandle;
use tokio::time::sleep;

// ---------------------------------------------------------------------------
// Helpers (shared pattern with control_ipc.rs)
// ---------------------------------------------------------------------------

fn fixture_dir() -> PathBuf {
	let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
	manifest
		.parent()
		.expect("crates dir should exist")
		.parent()
		.expect("workspace root should exist")
		.join("fixtures")
		.join("real-rust-project")
}

fn unique_temp_dir(name: &str) -> PathBuf {
	let nanos = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.expect("system time should be valid")
		.as_nanos();
	let dir = PathBuf::from("/tmp").join(format!("lspee-real-lsp-test-{name}-{nanos}"));
	fs::create_dir_all(&dir).expect("temp dir should be created");
	fs::canonicalize(&dir).expect("temp dir should canonicalize")
}

/// Copy the fixture into a fresh temp directory so each test has an isolated
/// project root (rust-analyzer writes into the directory it analyzes).
fn setup_fixture(name: &str) -> PathBuf {
	let src = fixture_dir();
	let dest = unique_temp_dir(name);

	// Copy Cargo.toml
	fs::copy(src.join("Cargo.toml"), dest.join("Cargo.toml"))
		.expect("Cargo.toml should copy");

	// Copy lspee.toml
	fs::copy(src.join("lspee.toml"), dest.join("lspee.toml"))
		.expect("lspee.toml should copy");

	// Copy src/
	fs::create_dir_all(dest.join("src")).expect("src dir should be created");
	fs::copy(src.join("src/lib.rs"), dest.join("src/lib.rs"))
		.expect("src/lib.rs should copy");

	dest
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
			Err(_) => sleep(Duration::from_millis(50)).await,
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

async fn do_attach(
	writer: &mut tokio::net::unix::OwnedWriteHalf,
	lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
	root: &Path,
	config_hash: &str,
) -> (String, Value) {
	let attach_id = format!("attach-{config_hash}");
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
				client_name: "real-lsp-test".to_string(),
				client_version: "0.1.0".to_string(),
				client_kind: Some(ClientKind::Agent),
				pid: None,
				cwd: None,
			},
			capabilities: Some(AttachCapabilities {
				stream_mode: vec![StreamMode::MuxControl],
			}),
		})
		.expect("attach payload should serialize"),
	)
	.await;

	assert_eq!(
		response.message_type, TYPE_ATTACH_OK,
		"attach should succeed, got: {:?}",
		response
	);

	let lease_id = response.payload["lease_id"]
		.as_str()
		.expect("AttachOk should include lease_id")
		.to_string();

	let initialize_result = response.payload["initialize_result"].clone();

	(lease_id, initialize_result)
}

async fn do_release(
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

async fn do_notify(
	writer: &mut tokio::net::unix::OwnedWriteHalf,
	lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
	id: &str,
	lease_id: &str,
	message: Value,
) {
	let response = request(
		writer,
		lines,
		id,
		TYPE_NOTIFY,
		serde_json::to_value(Notify {
			lease_id: lease_id.to_string(),
			message,
		})
		.expect("notify payload should serialize"),
	)
	.await;

	assert_eq!(
		response.message_type, TYPE_NOTIFY_OK,
		"notify should succeed, got: {:?}",
		response
	);
}

async fn do_call(
	writer: &mut tokio::net::unix::OwnedWriteHalf,
	lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
	id: &str,
	lease_id: &str,
	lsp_request: Value,
) -> ControlEnvelope<Value> {
	request(
		writer,
		lines,
		id,
		TYPE_CALL,
		serde_json::to_value(Call {
			lease_id: lease_id.to_string(),
			request: lsp_request,
		})
		.expect("call payload should serialize"),
	)
	.await
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

/// Send `textDocument/didOpen` for the fixture's `src/lib.rs`.
async fn send_did_open(
	writer: &mut tokio::net::unix::OwnedWriteHalf,
	lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
	lease_id: &str,
	root: &Path,
) {
	let lib_path = root.join("src/lib.rs");
	let text = fs::read_to_string(&lib_path).expect("fixture src/lib.rs should be readable");
	let uri = format!("file://{}", lib_path.display());

	do_notify(
		writer,
		lines,
		"did-open-1",
		lease_id,
		json!({
			"jsonrpc": "2.0",
			"method": "textDocument/didOpen",
			"params": {
				"textDocument": {
					"uri": uri,
					"languageId": "rust",
					"version": 1,
					"text": text
				}
			}
		}),
	)
	.await;
}

/// Wait for rust-analyzer to finish indexing by retrying a hover request until
/// it returns a non-null result or we hit a timeout.
async fn wait_for_indexing(
	writer: &mut tokio::net::unix::OwnedWriteHalf,
	lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
	lease_id: &str,
	root: &Path,
	timeout: Duration,
) {
	let lib_uri = format!("file://{}", root.join("src/lib.rs").display());
	let start = std::time::Instant::now();
	let mut attempt = 0u32;

	while start.elapsed() < timeout {
		attempt += 1;
		let response = do_call(
			writer,
			lines,
			&format!("index-probe-{attempt}"),
			lease_id,
			json!({
				"jsonrpc": "2.0",
				"id": 9000 + attempt,
				"method": "textDocument/hover",
				"params": {
					"textDocument": { "uri": &lib_uri },
					"position": { "line": 0, "character": 7 }
				}
			}),
		)
		.await;

		if response.message_type == TYPE_CALL_OK {
			let result = &response.payload["response"]["result"];
			if !result.is_null() {
				return;
			}
		}

		sleep(Duration::from_secs(1)).await;
	}

	// Don't panic here -- the caller tests may still pass with partial results.
	eprintln!("warning: rust-analyzer may not have finished indexing within {timeout:?}");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Start the daemon with rust-analyzer, attach, and verify that the
/// `initialize_result` contains real capabilities such as `hoverProvider`
/// and `definitionProvider`.
#[tokio::test]
#[ignore]
async fn real_lsp_initialize_returns_capabilities() {
	let root = setup_fixture("init-caps");
	let daemon_task = spawn_daemon(&root);
	let socket_path = root.join(".lspee").join("daemon.sock");
	let stream = connect_with_retry(&socket_path).await;
	let (reader, mut writer) = stream.into_split();
	let mut lines = BufReader::new(reader).lines();

	let (lease_id, initialize_result) =
		do_attach(&mut writer, &mut lines, &root, "real-init").await;

	// rust-analyzer returns capabilities in initialize_result.result.capabilities
	let capabilities = &initialize_result["result"]["capabilities"];
	assert!(
		!capabilities.is_null(),
		"initialize_result should contain capabilities, got: {initialize_result}"
	);
	assert!(
		capabilities.get("hoverProvider").is_some(),
		"capabilities should include hoverProvider"
	);
	assert!(
		capabilities.get("definitionProvider").is_some(),
		"capabilities should include definitionProvider"
	);
	assert!(
		capabilities.get("completionProvider").is_some(),
		"capabilities should include completionProvider"
	);

	do_release(&mut writer, &mut lines, &lease_id).await;
	shutdown_daemon(&mut writer, &mut lines).await;
	let _ = daemon_task.await;
	let _ = fs::remove_dir_all(root);
}

/// Attach, send `textDocument/didOpen` for `src/lib.rs`, then send
/// `textDocument/hover` on the `add` function. Verify the response contains
/// hover content describing the function signature.
#[tokio::test]
#[ignore]
async fn real_lsp_hover_returns_result() {
	let root = setup_fixture("hover");
	let daemon_task = spawn_daemon(&root);
	let socket_path = root.join(".lspee").join("daemon.sock");
	let stream = connect_with_retry(&socket_path).await;
	let (reader, mut writer) = stream.into_split();
	let mut lines = BufReader::new(reader).lines();

	let (lease_id, _) = do_attach(&mut writer, &mut lines, &root, "real-hover").await;
	send_did_open(&mut writer, &mut lines, &lease_id, &root).await;

	// Wait for rust-analyzer to index the project.
	wait_for_indexing(
		&mut writer,
		&mut lines,
		&lease_id,
		&root,
		Duration::from_secs(60),
	)
	.await;

	// Hover over `add` at line 0, character 7 (the `a` in `add`).
	let lib_uri = format!("file://{}", root.join("src/lib.rs").display());
	let hover_response = do_call(
		&mut writer,
		&mut lines,
		"hover-add",
		&lease_id,
		json!({
			"jsonrpc": "2.0",
			"id": 100,
			"method": "textDocument/hover",
			"params": {
				"textDocument": { "uri": &lib_uri },
				"position": { "line": 0, "character": 7 }
			}
		}),
	)
	.await;

	assert_eq!(
		hover_response.message_type, TYPE_CALL_OK,
		"hover call should succeed: {:?}",
		hover_response
	);

	let result = &hover_response.payload["response"]["result"];
	assert!(
		!result.is_null(),
		"hover result should not be null (rust-analyzer may still be indexing)"
	);

	// The hover content should mention the function signature.
	let contents = &result["contents"];
	let content_str = if let Some(value) = contents.get("value") {
		value.as_str().unwrap_or("")
	} else if let Some(s) = contents.as_str() {
		s
	} else {
		&contents.to_string()
	};
	assert!(
		content_str.contains("add") || content_str.contains("i32"),
		"hover content should mention 'add' or 'i32', got: {content_str}"
	);

	do_release(&mut writer, &mut lines, &lease_id).await;
	shutdown_daemon(&mut writer, &mut lines).await;
	let _ = daemon_task.await;
	let _ = fs::remove_dir_all(root);
}

/// Send `textDocument/definition` on `Greeter` in the `impl Greeter` line
/// of `src/lib.rs`. Verify the response points back to the struct definition
/// at the top of the file.
///
/// We test within a single file to avoid cross-file resolution timing issues
/// with rust-analyzer startup.
#[tokio::test]
#[ignore]
async fn real_lsp_definition_resolves() {
	let root = setup_fixture("definition");

	let daemon_task = spawn_daemon(&root);
	let socket_path = root.join(".lspee").join("daemon.sock");
	let stream = connect_with_retry(&socket_path).await;
	let (reader, mut writer) = stream.into_split();
	let mut lines = BufReader::new(reader).lines();

	let (lease_id, _) = do_attach(&mut writer, &mut lines, &root, "real-def").await;
	send_did_open(&mut writer, &mut lines, &lease_id, &root).await;

	// Wait for indexing.
	wait_for_indexing(
		&mut writer,
		&mut lines,
		&lease_id,
		&root,
		Duration::from_secs(60),
	)
	.await;

	// Go to definition of `Greeter` on the `impl Greeter` line.
	// In src/lib.rs:
	//   line 0: pub fn add(a: i32, b: i32) -> i32 {
	//   ...
	//   line 4: pub struct Greeter {        <-- definition (line 4)
	//   ...
	//   line 8: impl Greeter {              <-- we click here (line 8, char 5)
	let lib_uri = format!("file://{}", root.join("src/lib.rs").display());
	let def_response = do_call(
		&mut writer,
		&mut lines,
		"def-greeter",
		&lease_id,
		json!({
			"jsonrpc": "2.0",
			"id": 200,
			"method": "textDocument/definition",
			"params": {
				"textDocument": { "uri": &lib_uri },
				"position": { "line": 8, "character": 5 }
			}
		}),
	)
	.await;

	assert_eq!(
		def_response.message_type, TYPE_CALL_OK,
		"definition call should succeed: {:?}",
		def_response
	);

	let result = &def_response.payload["response"]["result"];
	assert!(
		!result.is_null(),
		"definition result should not be null"
	);

	// rust-analyzer may return a single Location or an array of Locations.
	let locations: Vec<&Value> = if result.is_array() {
		result.as_array().expect("should be array").iter().collect()
	} else {
		vec![result]
	};

	assert!(
		!locations.is_empty(),
		"should have at least one definition location"
	);

	// The definition should point to lib.rs (same file) at the struct
	// definition, which is on line 4.
	let any_points_to_struct = locations.iter().any(|loc| {
		let uri_matches = loc
			.get("uri")
			.or_else(|| loc.get("targetUri"))
			.and_then(Value::as_str)
			.is_some_and(|uri| uri == lib_uri);

		let range = loc
			.get("range")
			.or_else(|| loc.get("targetRange"));
		let line_matches = range
			.and_then(|r| r.get("start"))
			.and_then(|s| s.get("line"))
			.and_then(Value::as_u64)
			.is_some_and(|line| line == 4);

		uri_matches && line_matches
	});
	assert!(
		any_points_to_struct,
		"definition should point to struct Greeter at line 4 in lib.rs, got: {result}"
	);

	do_release(&mut writer, &mut lines, &lease_id).await;
	shutdown_daemon(&mut writer, &mut lines).await;
	let _ = daemon_task.await;
	let _ = fs::remove_dir_all(root);
}

/// Send `workspace/symbol` with query "add" and verify rust-analyzer finds
/// the `add` function.
#[tokio::test]
#[ignore]
async fn real_lsp_workspace_symbols() {
	let root = setup_fixture("symbols");
	let daemon_task = spawn_daemon(&root);
	let socket_path = root.join(".lspee").join("daemon.sock");
	let stream = connect_with_retry(&socket_path).await;
	let (reader, mut writer) = stream.into_split();
	let mut lines = BufReader::new(reader).lines();

	let (lease_id, _) = do_attach(&mut writer, &mut lines, &root, "real-sym").await;
	send_did_open(&mut writer, &mut lines, &lease_id, &root).await;

	// Wait for indexing.
	wait_for_indexing(
		&mut writer,
		&mut lines,
		&lease_id,
		&root,
		Duration::from_secs(60),
	)
	.await;

	// workspace/symbol with query "add"
	let sym_response = do_call(
		&mut writer,
		&mut lines,
		"sym-add",
		&lease_id,
		json!({
			"jsonrpc": "2.0",
			"id": 300,
			"method": "workspace/symbol",
			"params": { "query": "add" }
		}),
	)
	.await;

	assert_eq!(
		sym_response.message_type, TYPE_CALL_OK,
		"workspace/symbol call should succeed: {:?}",
		sym_response
	);

	let result = &sym_response.payload["response"]["result"];
	assert!(
		!result.is_null(),
		"workspace/symbol result should not be null"
	);

	let symbols = result.as_array().expect("result should be an array");
	let has_add = symbols.iter().any(|sym| {
		sym.get("name")
			.and_then(Value::as_str)
			.is_some_and(|name| name == "add")
	});
	assert!(
		has_add,
		"workspace/symbol results should include 'add', got: {result}"
	);

	do_release(&mut writer, &mut lines, &lease_id).await;
	shutdown_daemon(&mut writer, &mut lines).await;
	let _ = daemon_task.await;
	let _ = fs::remove_dir_all(root);
}
