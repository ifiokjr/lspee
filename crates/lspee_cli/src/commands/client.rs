use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use lspee_daemon::ControlEnvelope;
use lspee_daemon::TYPE_ERROR;
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::time::Duration;
use tokio::time::sleep;

const DAEMON_CONNECT_ATTEMPTS: usize = 40;
const DAEMON_CONNECT_BACKOFF_MS: u64 = 100;

pub fn daemon_socket_path(project_root: &Path) -> PathBuf {
	project_root.join(".lspee").join("daemon.sock")
}

/// Connect to an existing daemon or auto-start one if needed.
#[doc = include_str!("../../../../docs/src/includes/daemon-auto-start.md")]
pub async fn connect(project_root: &Path, auto_start: bool) -> Result<UnixStream> {
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

/// Spawn a background daemon process for the given project root.
#[doc = include_str!("../../../../docs/src/includes/daemon-spawn-mechanism.md")]
fn spawn_daemon(project_root: &Path) -> Result<()> {
	let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
	let log_dir = project_root.join(".lspee");
	let _ = std::fs::create_dir_all(&log_dir);
	let log_file = log_dir.join("daemon.log");

	let mut cmd = std::process::Command::new(current_exe);
	cmd.arg("serve")
		.arg("--project-root")
		.arg(project_root)
		.arg("--log-file")
		.arg(&log_file)
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null());

	// Forward log settings if set by the user.
	if let Ok(log_filter) = std::env::var("LSPEE_LOG") {
		cmd.env("LSPEE_LOG", log_filter);
	}

	if let Ok(log_format) = std::env::var("LSPEE_LOG_FORMAT") {
		cmd.env("LSPEE_LOG_FORMAT", log_format);
	}

	cmd.spawn()
		.context("failed to spawn background daemon process")?;

	tracing::debug!(log_file = %log_file.display(), "auto-started daemon");

	Ok(())
}

pub fn new_request_id(prefix: &str) -> String {
	let nanos = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map(|d| d.as_nanos())
		.unwrap_or_default();

	format!("{prefix}-{nanos}")
}

pub async fn write_frame(
	writer: &mut tokio::net::unix::OwnedWriteHalf,
	envelope: &ControlEnvelope<Value>,
) -> Result<()> {
	let mut bytes = serde_json::to_vec(envelope)?;
	bytes.push(b'\n');

	writer.write_all(&bytes).await?;
	writer.flush().await?;

	Ok(())
}

pub async fn read_response_for_id(
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

pub fn ensure_not_error(response: &ControlEnvelope<Value>) -> Result<()> {
	if response.message_type == TYPE_ERROR {
		return Err(anyhow!(render_error_payload(&response.payload)));
	}

	Ok(())
}

pub fn render_error_payload(payload: &Value) -> String {
	let code = payload
		.get("code")
		.and_then(Value::as_str)
		.unwrap_or("E_UNKNOWN");
	let message = payload
		.get("message")
		.and_then(Value::as_str)
		.unwrap_or("Unknown daemon error");
	format!("daemon error {code}: {message}")
}
