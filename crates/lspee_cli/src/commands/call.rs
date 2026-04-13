use std::path::Path;
use std::path::PathBuf;
use std::process;

use clap::Args;
use clap::ValueEnum;
use lspee_config::resolve;
use lspee_daemon::Attach;
use lspee_daemon::AttachCapabilities;
use lspee_daemon::Call;
use lspee_daemon::CallOk;
use lspee_daemon::ClientKind;
use lspee_daemon::ClientMeta;
use lspee_daemon::ControlEnvelope;
use lspee_daemon::Release;
use lspee_daemon::SessionKeyWire;
use lspee_daemon::StreamMode;
use lspee_daemon::TYPE_ATTACH;
use lspee_daemon::TYPE_ATTACH_OK;
use lspee_daemon::TYPE_CALL;
use lspee_daemon::TYPE_CALL_OK;
use lspee_daemon::TYPE_RELEASE;
use lspee_daemon::TYPE_RELEASE_OK;
use serde_json::Value;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;

use super::client;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CallOutput {
	/// Compact JSON payload (best for agents).
	Json,
	/// Pretty formatted JSON payload (best for humans).
	Pretty,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CallClientKind {
	Agent,
	Human,
	Ci,
}

#[derive(Debug, Args)]
pub struct CallCommand {
	/// LSP server identifier to target (e.g. rust-analyzer).
	#[arg(long = "lsp")]
	pub lsp: String,

	/// Override project root used for config resolution and session identity.
	#[arg(long)]
	pub root: Option<PathBuf>,

	/// Raw JSON request payload or @path/to/file.json.
	#[arg(long)]
	pub request: String,

	/// Disable daemon auto-start when socket is missing.
	#[arg(long)]
	pub no_start_daemon: bool,

	/// Declare the caller kind for eviction prioritization.
	#[arg(long, value_enum, default_value_t = CallClientKind::Human)]
	pub client_kind: CallClientKind,

	/// Output format.
	#[arg(long, value_enum, default_value_t = CallOutput::Pretty)]
	pub output: CallOutput,
}

pub fn run(cmd: CallCommand) -> anyhow::Result<()> {
	let runtime = tokio::runtime::Runtime::new()?;

	runtime.block_on(run_async(cmd))
}

async fn run_async(cmd: CallCommand) -> anyhow::Result<()> {
	let resolved = resolve(cmd.root.as_deref())?;
	let request_payload = load_request_payload(&cmd.request)?;

	let stream = client::connect(&resolved.project_root, !cmd.no_start_daemon).await?;
	let (reader, mut writer) = stream.into_split();
	let mut lines = BufReader::new(reader).lines();

	let client_kind = match cmd.client_kind {
		CallClientKind::Agent => ClientKind::Agent,
		CallClientKind::Human => ClientKind::Human,
		CallClientKind::Ci => ClientKind::Ci,
	};

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
				client_name: "lspee_cli".to_string(),
				client_version: env!("CARGO_PKG_VERSION").to_string(),
				client_kind: Some(client_kind),
				pid: Some(process::id()),
				cwd: std::env::current_dir()
					.ok()
					.map(|cwd| cwd.display().to_string()),
			},
			capabilities: Some(AttachCapabilities {
				stream_mode: vec![StreamMode::MuxControl],
			}),
		})?,
	};

	client::write_frame(&mut writer, &attach).await?;

	let attach_response = client::read_response_for_id(&mut lines, &attach_id).await?;
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

	let call_id = client::new_request_id("call");
	let call = ControlEnvelope {
		v: lspee_daemon::PROTOCOL_VERSION,
		id: Some(call_id.clone()),
		message_type: TYPE_CALL.to_string(),
		payload: serde_json::to_value(Call {
			lease_id: lease_id.clone(),
			request: request_payload,
		})?,
	};

	client::write_frame(&mut writer, &call).await?;

	let call_response = client::read_response_for_id(&mut lines, &call_id).await;

	// Always attempt lease release once attached.
	let release_result = release_lease(&mut writer, &mut lines, &lease_id).await;

	if let Err(error) = release_result {
		tracing::warn!(?error, lease_id, "failed to release lease after call");
	}

	let call_response = call_response?;
	client::ensure_not_error(&call_response)?;

	if call_response.message_type != TYPE_CALL_OK {
		anyhow::bail!(
			"unexpected response type for Call: {}",
			call_response.message_type
		);
	}

	let call_ok: CallOk = serde_json::from_value(call_response.payload)
		.map_err(|error| anyhow::anyhow!("invalid CallOk payload: {error}"))?;

	match cmd.output {
		CallOutput::Json => println!("{}", serde_json::to_string(&call_ok.response)?),
		CallOutput::Pretty => println!("{}", serde_json::to_string_pretty(&call_ok.response)?),
	}

	Ok(())
}

async fn release_lease(
	writer: &mut tokio::net::unix::OwnedWriteHalf,
	lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
	lease_id: &str,
) -> anyhow::Result<()> {
	let release_id = client::new_request_id("release");
	let release = ControlEnvelope {
		v: lspee_daemon::PROTOCOL_VERSION,
		id: Some(release_id.clone()),
		message_type: TYPE_RELEASE.to_string(),
		payload: serde_json::to_value(Release {
			lease_id: lease_id.to_string(),
			reason: None,
		})?,
	};

	client::write_frame(writer, &release).await?;

	let release_response = client::read_response_for_id(lines, &release_id).await?;
	client::ensure_not_error(&release_response)?;

	if release_response.message_type != TYPE_RELEASE_OK {
		anyhow::bail!(
			"unexpected response type for Release: {}",
			release_response.message_type
		);
	}

	Ok(())
}

fn load_request_payload(request: &str) -> anyhow::Result<Value> {
	let content = if let Some(path) = request.strip_prefix('@') {
		std::fs::read_to_string(Path::new(path))?
	} else {
		request.to_string()
	};

	let payload: Value = serde_json::from_str(&content)
		.map_err(|e| anyhow::anyhow!("invalid JSON request payload: {e}"))?;

	Ok(payload)
}
