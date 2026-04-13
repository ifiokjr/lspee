use std::path::PathBuf;
use std::process;

use clap::Args;
use lspee_config::resolve;
use lspee_daemon::Attach;
use lspee_daemon::AttachCapabilities;
use lspee_daemon::ClientKind;
use lspee_daemon::ClientMeta;
use lspee_daemon::ControlEnvelope;
use lspee_daemon::SessionKeyWire;
use lspee_daemon::StreamErrorPayload;
use lspee_daemon::StreamFrame;
use lspee_daemon::StreamFrameType;
use lspee_daemon::StreamMode;
use lspee_daemon::TYPE_ATTACH;
use lspee_daemon::TYPE_ATTACH_OK;
use serde_json::Value;
use serde_json::json;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::UnixStream;

use super::client;

#[derive(Debug, Args)]
pub struct ProxyCommand {
	/// LSP server identifier to target (e.g. rust-analyzer).
	#[arg(long = "lsp")]
	pub lsp: String,

	/// Override project root used for config resolution and session identity.
	#[arg(long)]
	pub root: Option<PathBuf>,

	/// Disable daemon auto-start when socket is missing.
	#[arg(long)]
	pub no_start_daemon: bool,
}

pub fn run(cmd: ProxyCommand) -> anyhow::Result<()> {
	let runtime = tokio::runtime::Runtime::new()?;

	runtime.block_on(run_async(cmd))
}

async fn run_async(cmd: ProxyCommand) -> anyhow::Result<()> {
	let resolved = resolve(cmd.root.as_deref())?;

	let control_stream = client::connect(&resolved.project_root, !cmd.no_start_daemon).await?;
	let (control_reader, mut control_writer) = control_stream.into_split();
	let mut control_lines = BufReader::new(control_reader).lines();

	let attach_id = client::new_request_id("attach");
	let attach = ControlEnvelope {
		v: lspee_daemon::PROTOCOL_VERSION,
		id: Some(attach_id.clone()),
		message_type: TYPE_ATTACH.to_string(),
		payload: serde_json::to_value(Attach {
			session_key: SessionKeyWire {
				project_root: resolved.project_root.display().to_string(),
				config_hash: resolved.config_hash,
				lsp_id: cmd.lsp,
			},
			client_meta: ClientMeta {
				client_name: "lspee_proxy".to_string(),
				client_version: env!("CARGO_PKG_VERSION").to_string(),
				client_kind: Some(ClientKind::Editor),
				pid: Some(process::id()),
				cwd: std::env::current_dir()
					.ok()
					.map(|cwd| cwd.display().to_string()),
			},
			capabilities: Some(AttachCapabilities {
				stream_mode: vec![StreamMode::Dedicated],
			}),
		})?,
	};

	client::write_frame(&mut control_writer, &attach).await?;
	let attach_response = client::read_response_for_id(&mut control_lines, &attach_id).await?;
	client::ensure_not_error(&attach_response)?;

	if attach_response.message_type != TYPE_ATTACH_OK {
		anyhow::bail!(
			"unexpected response type for Attach: {}",
			attach_response.message_type
		);
	}

	let lease_id = attach_response
		.payload
		.get("lease_id")
		.and_then(Value::as_str)
		.ok_or_else(|| anyhow::anyhow!("AttachOk missing lease_id"))?
		.to_string();
	let initialize_result = attach_response
		.payload
		.get("initialize_result")
		.cloned()
		.unwrap_or(Value::Null);
	let endpoint = attach_response
		.payload
		.get("stream")
		.and_then(|stream| stream.get("endpoint"))
		.and_then(Value::as_str)
		.ok_or_else(|| anyhow::anyhow!("AttachOk missing stream endpoint"))?;
	let endpoint = endpoint
		.strip_prefix("unix://")
		.ok_or_else(|| anyhow::anyhow!("unsupported stream endpoint: {endpoint}"))?;

	// Control connection no longer needed once dedicated stream is established.
	drop(control_writer);
	drop(control_lines);

	let stream = UnixStream::connect(endpoint).await.map_err(|error| {
		anyhow::anyhow!("failed to connect to dedicated stream endpoint {endpoint}: {error}")
	})?;
	let (stream_reader, mut stream_writer) = stream.into_split();
	let mut stream_lines = BufReader::new(stream_reader).lines();

	let mut stdin_reader = BufReader::new(tokio::io::stdin());
	let mut stdout = tokio::io::stdout();
	let mut seq = 1_u64;

	loop {
		tokio::select! {
			incoming = lspee_lsp::read_lsp_frame(&mut stdin_reader) => {
				match incoming? {
					Some(message) => {
						if handle_editor_message(
							&message,
							&initialize_result,
							&lease_id,
							&mut seq,
							&mut stream_writer,
							&mut stdout,
						).await? {
							break;
						}
					}
					None => break,
				}
			}
			next = stream_lines.next_line() => {
				let Some(line) = next? else {
					break;
				};

				let frame: StreamFrame<Value> = serde_json::from_str(&line)
					.map_err(|error| anyhow::anyhow!("invalid stream frame from daemon: {error}"))?;

				match frame.frame_type {
					StreamFrameType::LspOut => {
						write_lsp_message(&mut stdout, &frame.payload).await?;
					}
					StreamFrameType::StreamError => {
						let payload: StreamErrorPayload = serde_json::from_value(frame.payload)
							.map_err(|error| anyhow::anyhow!("invalid stream error payload: {error}"))?;
						let warning = stream_error_to_editor_warning(&payload);
						write_lsp_message(&mut stdout, &warning).await?;
						break;
					}
					StreamFrameType::LspIn => {}
				}
			}
		}
	}

	Ok(())
}

async fn handle_editor_message(
	message: &Value,
	initialize_result: &Value,
	lease_id: &str,
	seq: &mut u64,
	stream_writer: &mut tokio::net::unix::OwnedWriteHalf,
	stdout: &mut tokio::io::Stdout,
) -> anyhow::Result<bool> {
	let method = message.get("method").and_then(Value::as_str);
	let id = message.get("id").cloned();

	match (method, id.as_ref()) {
		(Some("initialize"), Some(id)) => {
			let response = initialize_response_for_editor(id, initialize_result);
			write_lsp_message(stdout, &response).await?;

			return Ok(false);
		}
		(Some("shutdown"), Some(id)) => {
			let response = shutdown_response_for_editor(id);
			write_lsp_message(stdout, &response).await?;

			return Ok(false);
		}
		(Some("initialized"), None) => return Ok(false),
		(Some("exit"), None) => return Ok(true),
		_ => {}
	}

	let frame = StreamFrame {
		v: lspee_daemon::PROTOCOL_VERSION,
		frame_type: StreamFrameType::LspIn,
		lease_id: lease_id.to_string(),
		seq: *seq,
		payload: message.clone(),
	};

	*seq = seq.saturating_add(1);

	let mut encoded = serde_json::to_vec(&frame)?;
	encoded.push(b'\n');

	stream_writer.write_all(&encoded).await?;
	stream_writer.flush().await?;

	Ok(false)
}

async fn write_lsp_message(stdout: &mut tokio::io::Stdout, message: &Value) -> anyhow::Result<()> {
	let frame = lspee_lsp::encode_lsp_frame(message)?;

	stdout.write_all(&frame).await?;
	stdout.flush().await?;

	Ok(())
}

fn initialize_response_for_editor(id: &Value, initialize_result: &Value) -> Value {
	let result = initialize_result
		.get("result")
		.cloned()
		.unwrap_or_else(|| initialize_result.clone());

	json!({
		"jsonrpc": "2.0",
		"id": id,
		"result": result,
	})
}

fn shutdown_response_for_editor(id: &Value) -> Value {
	json!({
		"jsonrpc": "2.0",
		"id": id,
		"result": null,
	})
}

fn stream_error_to_editor_warning(payload: &StreamErrorPayload) -> Value {
	let resume_hint = payload
		.details
		.as_ref()
		.and_then(|details| details.get("resume_hint"))
		.and_then(Value::as_str)
		.unwrap_or("Retry attaching to the daemon session.");

	json!({
		"jsonrpc": "2.0",
		"method": "window/showMessage",
		"params": {
			"type": 2,
			"message": format!("lspee warning: {} {}", payload.message, resume_hint),
		}
	})
}

#[cfg(test)]
mod tests {
	use lspee_daemon::StreamErrorPayload;
	use serde_json::json;

	use super::initialize_response_for_editor;
	use super::shutdown_response_for_editor;
	use super::stream_error_to_editor_warning;

	#[test]
	fn initialize_response_reuses_backend_result() {
		let response = initialize_response_for_editor(
			&json!(1),
			&json!({
				"jsonrpc": "2.0",
				"id": "lspee-initialize",
				"result": {"capabilities": {"hoverProvider": true}}
			}),
		);

		assert_eq!(response["id"], 1);
		assert_eq!(response["result"]["capabilities"]["hoverProvider"], true);
	}

	#[test]
	fn shutdown_response_returns_null_result() {
		let response = shutdown_response_for_editor(&json!(7));
		assert_eq!(response["id"], 7);
		assert!(response["result"].is_null());
	}

	#[test]
	fn stream_error_warning_includes_resume_hint() {
		let warning = stream_error_to_editor_warning(&StreamErrorPayload {
			code: "E_SESSION_EVICTED_MEMORY".to_string(),
			message: "memory pressure".to_string(),
			retryable: true,
			details: Some(json!({"resume_hint": "Restart the LSP in Helix."})),
		});

		assert_eq!(warning["method"], "window/showMessage");
		assert!(
			warning["params"]["message"]
				.as_str()
				.expect("warning message should be string")
				.contains("Restart the LSP in Helix.")
		);
	}

	#[test]
	fn initialize_response_falls_back_to_whole_value() {
		// When initialize_result has no "result" key, the entire value is used.
		let caps = json!({"capabilities": {"completionProvider": true}});
		let response = initialize_response_for_editor(&json!("init-1"), &caps);

		assert_eq!(response["id"], "init-1");
		assert_eq!(
			response["result"]["capabilities"]["completionProvider"],
			true
		);
		assert_eq!(response["jsonrpc"], "2.0");
	}

	#[test]
	fn initialize_response_with_string_id() {
		let response = initialize_response_for_editor(
			&json!("string-id"),
			&json!({"result": {"capabilities": {}}}),
		);
		assert_eq!(response["id"], "string-id");
		assert!(response["result"]["capabilities"].is_object());
	}

	#[test]
	fn shutdown_response_with_string_id() {
		let response = shutdown_response_for_editor(&json!("shutdown-99"));
		assert_eq!(response["id"], "shutdown-99");
		assert!(response["result"].is_null());
		assert_eq!(response["jsonrpc"], "2.0");
	}

	#[test]
	fn stream_error_warning_uses_default_resume_hint_when_none() {
		let warning = stream_error_to_editor_warning(&StreamErrorPayload {
			code: "E_CRASH".to_string(),
			message: "server crashed".to_string(),
			retryable: false,
			details: None,
		});

		let msg = warning["params"]["message"]
			.as_str()
			.expect("warning message should be string");
		assert!(msg.contains("server crashed"));
		assert!(msg.contains("Retry attaching to the daemon session."));
	}

	#[test]
	fn stream_error_warning_uses_default_when_no_resume_hint_key() {
		let warning = stream_error_to_editor_warning(&StreamErrorPayload {
			code: "E_OOM".to_string(),
			message: "out of memory".to_string(),
			retryable: true,
			details: Some(json!({"some_other_key": "value"})),
		});

		let msg = warning["params"]["message"]
			.as_str()
			.expect("warning message should be string");
		assert!(msg.contains("out of memory"));
		assert!(msg.contains("Retry attaching to the daemon session."));
	}

	#[test]
	fn stream_error_warning_message_type_is_warning() {
		let warning = stream_error_to_editor_warning(&StreamErrorPayload {
			code: "E_TEST".to_string(),
			message: "test".to_string(),
			retryable: false,
			details: None,
		});

		// LSP MessageType 2 = Warning
		assert_eq!(warning["params"]["type"], 2);
	}

	#[test]
	fn proxy_command_struct_defaults() {
		use clap::Parser;

		#[derive(Parser)]
		struct Cli {
			#[command(flatten)]
			proxy: super::ProxyCommand,
		}

		let cli = Cli::parse_from(["test", "--lsp", "rust-analyzer"]);
		assert_eq!(cli.proxy.lsp, "rust-analyzer");
		assert!(cli.proxy.root.is_none());
		assert!(!cli.proxy.no_start_daemon);
	}

	#[test]
	fn proxy_command_all_args() {
		use clap::Parser;

		#[derive(Parser)]
		struct Cli {
			#[command(flatten)]
			proxy: super::ProxyCommand,
		}

		let cli = Cli::parse_from([
			"test",
			"--lsp",
			"taplo",
			"--root",
			"/tmp/project",
			"--no-start-daemon",
		]);
		assert_eq!(cli.proxy.lsp, "taplo");
		assert_eq!(
			cli.proxy.root.as_deref(),
			Some(std::path::Path::new("/tmp/project"))
		);
		assert!(cli.proxy.no_start_daemon);
	}
}
