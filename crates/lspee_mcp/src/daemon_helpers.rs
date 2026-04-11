//! Helpers that communicate with the lspee daemon over its Unix socket.
//!
//! The patterns here mirror `lspee_cli::commands::client` but are kept
//! self-contained so the MCP crate does not depend on the CLI.

use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use std::process;
use std::process::Stdio;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use lspee_daemon::Attach;
use lspee_daemon::AttachCapabilities;
use lspee_daemon::AttachOk;
use lspee_daemon::Call;
use lspee_daemon::CallOk;
use lspee_daemon::ClientKind;
use lspee_daemon::ClientMeta;
use lspee_daemon::ControlEnvelope;
use lspee_daemon::Release;
use lspee_daemon::SessionKeyWire;
use lspee_daemon::Stats;
use lspee_daemon::StatsOk;
use lspee_daemon::StreamMode;
use lspee_daemon::TYPE_ATTACH;
use lspee_daemon::TYPE_ATTACH_OK;
use lspee_daemon::TYPE_CALL;
use lspee_daemon::TYPE_CALL_OK;
use lspee_daemon::TYPE_ERROR;
use lspee_daemon::TYPE_RELEASE;
use lspee_daemon::TYPE_RELEASE_OK;
use lspee_daemon::TYPE_STATS;
use lspee_daemon::TYPE_STATS_OK;
use serde_json::Value;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::time::Duration;
use tokio::time::sleep;

const DAEMON_CONNECT_ATTEMPTS: usize = 40;
const DAEMON_CONNECT_BACKOFF_MS: u64 = 100;

// ---------------------------------------------------------------------------
// Public helpers used by tool implementations
// ---------------------------------------------------------------------------

/// Query LSP capabilities via the daemon.
pub(crate) async fn query_capabilities(lsp_id: &str, root: Option<&Path>) -> Result<String> {
	let resolved = lspee_config::resolve(root)?;
	let stream = connect(&resolved.project_root, true).await?;
	let (reader, mut writer) = stream.into_split();
	let mut lines = BufReader::new(reader).lines();

	// Attach
	let attach_id = new_request_id("mcp-attach");
	let attach = build_attach_envelope(&attach_id, &resolved, lsp_id);
	write_frame(&mut writer, &attach).await?;
	let attach_resp = read_response_for_id(&mut lines, &attach_id).await?;
	ensure_not_error(&attach_resp)?;
	if attach_resp.message_type != TYPE_ATTACH_OK {
		anyhow::bail!(
			"unexpected response type for Attach: {}",
			attach_resp.message_type
		);
	}

	let attach_ok: AttachOk = serde_json::from_value(attach_resp.payload)
		.map_err(|e| anyhow!("invalid AttachOk payload: {e}"))?;

	let lease_id = attach_ok.lease_id.clone();
	let capabilities = extract_capabilities(lsp_id, attach_ok.initialize_result.as_ref());

	// Release
	let _ = release_lease(&mut writer, &mut lines, &lease_id).await;

	Ok(serde_json::to_string_pretty(&capabilities)?)
}

/// Send a raw LSP request via the daemon.
pub(crate) async fn raw_call(
	lsp_id: &str,
	request_json: &str,
	root: Option<&Path>,
) -> Result<String> {
	let request_payload: Value = serde_json::from_str(request_json)
		.map_err(|e| anyhow!("invalid JSON request payload: {e}"))?;

	let resolved = lspee_config::resolve(root)?;
	let stream = connect(&resolved.project_root, true).await?;
	let (reader, mut writer) = stream.into_split();
	let mut lines = BufReader::new(reader).lines();

	// Attach
	let attach_id = new_request_id("mcp-attach");
	let attach = build_attach_envelope(&attach_id, &resolved, lsp_id);
	write_frame(&mut writer, &attach).await?;
	let attach_resp = read_response_for_id(&mut lines, &attach_id).await?;
	ensure_not_error(&attach_resp)?;
	if attach_resp.message_type != TYPE_ATTACH_OK {
		anyhow::bail!(
			"unexpected response type for Attach: {}",
			attach_resp.message_type
		);
	}

	let lease_id = attach_resp
		.payload
		.get("lease_id")
		.and_then(Value::as_str)
		.ok_or_else(|| anyhow!("AttachOk missing lease_id"))?
		.to_string();

	// Call
	let call_id = new_request_id("mcp-call");
	let call = ControlEnvelope {
		v: lspee_daemon::PROTOCOL_VERSION,
		id: Some(call_id.clone()),
		message_type: TYPE_CALL.to_string(),
		payload: serde_json::to_value(Call {
			lease_id: lease_id.clone(),
			request: request_payload,
		})?,
	};
	write_frame(&mut writer, &call).await?;
	let call_resp = read_response_for_id(&mut lines, &call_id).await?;

	// Release regardless of call outcome
	let _ = release_lease(&mut writer, &mut lines, &lease_id).await;

	ensure_not_error(&call_resp)?;
	if call_resp.message_type != TYPE_CALL_OK {
		anyhow::bail!(
			"unexpected response type for Call: {}",
			call_resp.message_type
		);
	}

	let call_ok: CallOk = serde_json::from_value(call_resp.payload)
		.map_err(|e| anyhow!("invalid CallOk payload: {e}"))?;

	Ok(serde_json::to_string_pretty(&call_ok.response)?)
}

/// Query daemon stats.
pub(crate) async fn query_status(root: Option<&Path>) -> Result<String> {
	let resolved = lspee_config::resolve(root)?;
	let stream = connect(&resolved.project_root, true).await?;
	let (reader, mut writer) = stream.into_split();
	let mut lines = BufReader::new(reader).lines();

	let req_id = new_request_id("mcp-stats");
	let request = ControlEnvelope {
		v: lspee_daemon::PROTOCOL_VERSION,
		id: Some(req_id.clone()),
		message_type: TYPE_STATS.to_string(),
		payload: serde_json::to_value(Stats::default())?,
	};

	write_frame(&mut writer, &request).await?;
	let response = read_response_for_id(&mut lines, &req_id).await?;
	ensure_not_error(&response)?;

	if response.message_type != TYPE_STATS_OK {
		anyhow::bail!(
			"unexpected response type for Stats: {}",
			response.message_type
		);
	}

	let stats: StatsOk = serde_json::from_value(response.payload)
		.map_err(|e| anyhow!("invalid StatsOk payload: {e}"))?;

	let payload = serde_json::json!({
		"daemon_status": "ok",
		"project_root": resolved.project_root,
		"stats": stats,
	});

	Ok(serde_json::to_string_pretty(&payload)?)
}

// ---------------------------------------------------------------------------
// Internal helpers (mirroring lspee_cli::commands::client)
// ---------------------------------------------------------------------------

fn daemon_socket_path(project_root: &Path) -> PathBuf {
	project_root.join(".lspee").join("daemon.sock")
}

async fn connect(project_root: &Path, auto_start: bool) -> Result<UnixStream> {
	let socket_path = daemon_socket_path(project_root);

	match UnixStream::connect(&socket_path).await {
		Ok(stream) => return Ok(stream),
		Err(error) if !auto_start => {
			return Err(anyhow!(
				"failed to connect to daemon socket {}: {error}",
				socket_path.display()
			));
		}
		Err(error)
			if !matches!(
				error.kind(),
				ErrorKind::NotFound | ErrorKind::ConnectionRefused | ErrorKind::ConnectionReset
			) =>
		{
			return Err(anyhow!(
				"failed to connect to daemon socket {}: {error}",
				socket_path.display()
			));
		}
		Err(_) => {}
	}

	spawn_daemon(project_root)?;

	for _ in 0..DAEMON_CONNECT_ATTEMPTS {
		match UnixStream::connect(&socket_path).await {
			Ok(stream) => return Ok(stream),
			Err(_) => sleep(Duration::from_millis(DAEMON_CONNECT_BACKOFF_MS)).await,
		}
	}

	Err(anyhow!(
		"failed to connect to daemon socket {} after auto-start",
		socket_path.display()
	))
}

fn spawn_daemon(project_root: &Path) -> Result<()> {
	let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
	let log_dir = project_root.join(".lspee");
	let _ = std::fs::create_dir_all(&log_dir);
	let log_file = log_dir.join("daemon.log");

	let mut cmd = process::Command::new(current_exe);
	cmd.arg("serve")
		.arg("--project-root")
		.arg(project_root)
		.arg("--log-file")
		.arg(&log_file)
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null());

	if let Ok(log_filter) = std::env::var("LSPEE_LOG") {
		cmd.env("LSPEE_LOG", log_filter);
	}
	if let Ok(log_format) = std::env::var("LSPEE_LOG_FORMAT") {
		cmd.env("LSPEE_LOG_FORMAT", log_format);
	}

	cmd.spawn()
		.context("failed to spawn background daemon process")?;

	tracing::debug!(log_file = %log_file.display(), "auto-started daemon (from MCP server)");
	Ok(())
}

fn new_request_id(prefix: &str) -> String {
	let nanos = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map(|d| d.as_nanos())
		.unwrap_or_default();
	format!("{prefix}-{nanos}")
}

fn build_attach_envelope(
	id: &str,
	resolved: &lspee_config::ResolvedConfig,
	lsp_id: &str,
) -> ControlEnvelope<Value> {
	let payload = serde_json::to_value(Attach {
		session_key: SessionKeyWire {
			project_root: resolved.project_root.display().to_string(),
			config_hash: resolved.config_hash.clone(),
			lsp_id: lsp_id.to_string(),
		},
		client_meta: ClientMeta {
			client_name: "lspee_mcp".to_string(),
			client_version: env!("CARGO_PKG_VERSION").to_string(),
			client_kind: Some(ClientKind::Agent),
			pid: Some(process::id()),
			cwd: std::env::current_dir()
				.ok()
				.map(|cwd| cwd.display().to_string()),
		},
		capabilities: Some(AttachCapabilities {
			stream_mode: vec![StreamMode::MuxControl],
		}),
	})
	.expect("Attach payload must serialize");

	ControlEnvelope {
		v: lspee_daemon::PROTOCOL_VERSION,
		id: Some(id.to_string()),
		message_type: TYPE_ATTACH.to_string(),
		payload,
	}
}

async fn write_frame(
	writer: &mut tokio::net::unix::OwnedWriteHalf,
	envelope: &ControlEnvelope<Value>,
) -> Result<()> {
	let mut bytes = serde_json::to_vec(envelope)?;
	bytes.push(b'\n');
	writer.write_all(&bytes).await?;
	writer.flush().await?;
	Ok(())
}

async fn read_response_for_id(
	lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
	expected_id: &str,
) -> Result<ControlEnvelope<Value>> {
	while let Some(line) = lines.next_line().await? {
		let response: ControlEnvelope<Value> = serde_json::from_str(&line)
			.map_err(|e| anyhow!("invalid daemon response JSON: {e}"))?;
		if response.id.as_deref() == Some(expected_id) {
			return Ok(response);
		}
	}

	Err(anyhow!(
		"daemon socket closed before response for id={expected_id}"
	))
}

fn ensure_not_error(response: &ControlEnvelope<Value>) -> Result<()> {
	if response.message_type == TYPE_ERROR {
		let code = response
			.payload
			.get("code")
			.and_then(Value::as_str)
			.unwrap_or("E_UNKNOWN");
		let message = response
			.payload
			.get("message")
			.and_then(Value::as_str)
			.unwrap_or("Unknown daemon error");
		return Err(anyhow!("daemon error {code}: {message}"));
	}
	Ok(())
}

async fn release_lease(
	writer: &mut tokio::net::unix::OwnedWriteHalf,
	lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
	lease_id: &str,
) -> Result<()> {
	let release_id = new_request_id("mcp-release");
	let release = ControlEnvelope {
		v: lspee_daemon::PROTOCOL_VERSION,
		id: Some(release_id.clone()),
		message_type: TYPE_RELEASE.to_string(),
		payload: serde_json::to_value(Release {
			lease_id: lease_id.to_string(),
			reason: None,
		})?,
	};

	write_frame(writer, &release).await?;
	let release_resp = read_response_for_id(lines, &release_id).await?;
	ensure_not_error(&release_resp)?;
	if release_resp.message_type != TYPE_RELEASE_OK {
		anyhow::bail!(
			"unexpected response type for Release: {}",
			release_resp.message_type
		);
	}

	Ok(())
}

/// Extract capability information from the LSP `initialize` result.
///
/// Mirrors the logic in `lspee_cli::commands::capabilities`.
fn extract_capabilities(lsp_id: &str, initialize_result: Option<&Value>) -> Value {
	let Some(init) = initialize_result else {
		return serde_json::json!({
			"lsp_id": lsp_id,
			"error": "no initialize_result available",
			"methods": {},
			"raw": null,
		});
	};

	let caps = init
		.get("result")
		.and_then(|r| r.get("capabilities"))
		.or_else(|| init.get("capabilities"));

	let Some(caps) = caps else {
		return serde_json::json!({
			"lsp_id": lsp_id,
			"error": "no capabilities field in initialize result",
			"methods": {},
			"raw": init,
		});
	};

	let methods = serde_json::json!({
		"textDocument/completion": has_capability(caps, "completionProvider"),
		"textDocument/hover": has_capability(caps, "hoverProvider"),
		"textDocument/signatureHelp": has_capability(caps, "signatureHelpProvider"),
		"textDocument/declaration": has_capability(caps, "declarationProvider"),
		"textDocument/definition": has_capability(caps, "definitionProvider"),
		"textDocument/typeDefinition": has_capability(caps, "typeDefinitionProvider"),
		"textDocument/implementation": has_capability(caps, "implementationProvider"),
		"textDocument/references": has_capability(caps, "referencesProvider"),
		"textDocument/documentHighlight": has_capability(caps, "documentHighlightProvider"),
		"textDocument/documentSymbol": has_capability(caps, "documentSymbolProvider"),
		"textDocument/codeAction": has_capability(caps, "codeActionProvider"),
		"textDocument/codeLens": has_capability(caps, "codeLensProvider"),
		"textDocument/documentLink": has_capability(caps, "documentLinkProvider"),
		"textDocument/colorPresentation": has_capability(caps, "colorProvider"),
		"textDocument/formatting": has_capability(caps, "documentFormattingProvider"),
		"textDocument/rangeFormatting": has_capability(caps, "documentRangeFormattingProvider"),
		"textDocument/onTypeFormatting": has_capability(caps, "documentOnTypeFormattingProvider"),
		"textDocument/rename": has_capability(caps, "renameProvider"),
		"textDocument/foldingRange": has_capability(caps, "foldingRangeProvider"),
		"textDocument/selectionRange": has_capability(caps, "selectionRangeProvider"),
		"textDocument/linkedEditingRange": has_capability(caps, "linkedEditingRangeProvider"),
		"textDocument/prepareCallHierarchy": has_capability(caps, "callHierarchyProvider"),
		"textDocument/prepareTypeHierarchy": has_capability(caps, "typeHierarchyProvider"),
		"textDocument/semanticTokens": has_capability(caps, "semanticTokensProvider"),
		"textDocument/moniker": has_capability(caps, "monikerProvider"),
		"textDocument/inlayHint": has_capability(caps, "inlayHintProvider"),
		"textDocument/inlineValue": has_capability(caps, "inlineValueProvider"),
		"textDocument/diagnostic": has_capability(caps, "diagnosticProvider"),
		"workspace/symbol": has_capability(caps, "workspaceSymbolProvider"),
	});

	let server_info = init
		.get("result")
		.and_then(|r| r.get("serverInfo"))
		.or_else(|| init.get("serverInfo"))
		.cloned();

	serde_json::json!({
		"lsp_id": lsp_id,
		"server_info": server_info,
		"methods": methods,
		"raw_capabilities": caps,
	})
}

fn has_capability(caps: &Value, key: &str) -> bool {
	match caps.get(key) {
		Some(Value::Null) | None => false,
		Some(Value::Bool(b)) => *b,
		Some(_) => true,
	}
}
