use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use serde_json::Value;
use serde_json::json;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::ChildStdin;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::time::Duration;
use tokio::time::timeout;
use tracing::instrument;

#[derive(Debug, Clone)]
pub struct LspMessage {
	pub id: Option<Value>,
	pub method: Option<String>,
	pub payload: Value,
}

impl LspMessage {
	fn from_json(payload: Value) -> Self {
		let id = payload.get("id").cloned();
		let method = payload
			.get("method")
			.and_then(Value::as_str)
			.map(ToOwned::to_owned);

		Self {
			id,
			method,
			payload,
		}
	}
}

#[derive(Clone)]
pub struct LspRuntime {
	inbound_tx: mpsc::Sender<Value>,
	outbound_tx: broadcast::Sender<LspMessage>,
	_writer: Arc<Mutex<ChildStdin>>,
	child: Arc<Mutex<Child>>,
}

impl std::fmt::Debug for LspRuntime {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("LspRuntime").finish_non_exhaustive()
	}
}

impl LspRuntime {
	pub async fn send(&self, msg: Value) -> Result<()> {
		self.inbound_tx
			.send(msg)
			.await
			.context("failed to queue inbound lsp message")
	}

	pub fn subscribe(&self) -> broadcast::Receiver<LspMessage> {
		self.outbound_tx.subscribe()
	}

	pub async fn pid(&self) -> Option<u32> {
		self.child.lock().await.id()
	}

	pub async fn rss_bytes(&self) -> Result<Option<u64>> {
		let Some(pid) = self.pid().await else {
			return Ok(None);
		};

		let output = Command::new("ps")
			.args(["-o", "rss=", "-p", &pid.to_string()])
			.output()
			.await
			.context("failed to sample lsp process memory via ps")?;

		if !output.status.success() {
			return Ok(None);
		}

		let stdout = String::from_utf8_lossy(&output.stdout);
		let rss_kb = stdout.trim().parse::<u64>().ok();
		Ok(rss_kb.map(|kb| kb * 1024))
	}

	#[instrument(skip(self))]
	pub async fn shutdown(&self) -> Result<()> {
		let _ = self
			.send(json!({"jsonrpc":"2.0","id":"lspee-shutdown","method":"shutdown","params":null}))
			.await;
		let _ = self
			.send(json!({"jsonrpc":"2.0","method":"exit","params":null}))
			.await;

		let mut child = self.child.lock().await;
		let result = timeout(Duration::from_secs(2), child.wait()).await;
		if let Ok(wait_result) = result {
			wait_result.context("failed waiting for lsp child process to exit")?;
			Ok(())
		} else {
			child
				.kill()
				.await
				.context("failed to kill lsp child after graceful shutdown timeout")?;
			Ok(())
		}
	}

	pub async fn force_stop(&self) -> Result<()> {
		let mut child = self.child.lock().await;
		if child
			.try_wait()
			.context("failed to query lsp child state")?
			.is_none()
		{
			child
				.kill()
				.await
				.context("failed to kill lsp child process")?;
		}
		Ok(())
	}

	#[instrument(skip_all, fields(request_id = ?request.get("id")))]
	pub async fn call(&self, request: Value) -> Result<Value> {
		let expected_id = request
			.get("id")
			.cloned()
			.ok_or_else(|| anyhow!("request message must include id"))?;

		let mut rx = self.subscribe();
		self.send(request).await?;

		loop {
			let msg = rx
				.recv()
				.await
				.context("lsp response channel closed while waiting for call response")?;

			if msg.id.as_ref() == Some(&expected_id) {
				return Ok(msg.payload);
			}
		}
	}
}

#[derive(Debug)]
pub struct LspTransport {
	root: PathBuf,
}

impl LspTransport {
	#[must_use]
	pub fn new(root: PathBuf) -> Self {
		Self { root }
	}

	pub fn prepare(&self, lsp: &lspee_config::LspConfig) -> Result<()> {
		if lsp.command.trim().is_empty() {
			return Err(anyhow!(
				"lsp command is empty; set [[lsp]].command in configuration"
			));
		}

		tracing::debug!(
			root = ?self.root,
			command = %lsp.command,
			args = ?lsp.args,
			"preparing lsp transport"
		);
		Ok(())
	}

	#[instrument(skip(self), fields(lsp_id = %lsp.id, command = %lsp.command))]
	pub async fn spawn(&self, lsp: &lspee_config::LspConfig) -> Result<LspRuntime> {
		self.prepare(lsp)?;

		let mut cmd = Command::new(&lsp.command);
		cmd.args(&lsp.args)
			.envs(&lsp.env)
			.current_dir(&self.root)
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::inherit());

		let mut child = cmd
			.spawn()
			.with_context(|| format!("failed to spawn lsp command: {}", lsp.command))?;

		let stdin = child
			.stdin
			.take()
			.ok_or_else(|| anyhow!("failed to capture lsp stdin"))?;
		let stdout = child
			.stdout
			.take()
			.ok_or_else(|| anyhow!("failed to capture lsp stdout"))?;

		let writer = Arc::new(Mutex::new(stdin));
		let child = Arc::new(Mutex::new(child));

		let (inbound_tx, mut inbound_rx) = mpsc::channel::<Value>(256);
		let (outbound_tx, _) = broadcast::channel::<LspMessage>(256);

		let writer_task = Arc::clone(&writer);
		tokio::spawn(async move {
			while let Some(msg) = inbound_rx.recv().await {
				let frame = match encode_lsp_frame(&msg) {
					Ok(frame) => frame,
					Err(err) => {
						tracing::error!(error = ?err, "failed to encode inbound lsp message frame");
						continue;
					}
				};

				let mut guard = writer_task.lock().await;
				if let Err(err) = guard.write_all(&frame).await {
					tracing::error!(error = ?err, "failed to write lsp frame to stdin");
					break;
				}
				if let Err(err) = guard.flush().await {
					tracing::error!(error = ?err, "failed to flush lsp stdin");
					break;
				}
			}
		});

		let outbound = outbound_tx.clone();
		tokio::spawn(async move {
			let mut reader = BufReader::new(stdout);
			loop {
				let payload = match read_lsp_frame(&mut reader).await {
					Ok(Some(payload)) => payload,
					Ok(None) => break,
					Err(err) => {
						tracing::error!(error = ?err, "failed to read lsp frame from stdout");
						break;
					}
				};

				let _ = outbound.send(LspMessage::from_json(payload));
			}
		});

		Ok(LspRuntime {
			inbound_tx,
			outbound_tx,
			_writer: writer,
			child,
		})
	}
}

pub fn encode_lsp_frame(message: &Value) -> Result<Vec<u8>> {
	let body = serde_json::to_vec(message).context("failed to serialize lsp json payload")?;
	let header = format!("Content-Length: {}\r\n\r\n", body.len());

	let mut frame = header.into_bytes();
	frame.extend_from_slice(&body);
	Ok(frame)
}

pub async fn read_lsp_frame<R>(reader: &mut BufReader<R>) -> Result<Option<Value>>
where
	R: tokio::io::AsyncRead + Unpin,
{
	let mut content_length: Option<usize> = None;

	loop {
		let mut line = String::new();
		let bytes = reader
			.read_line(&mut line)
			.await
			.context("failed to read lsp header line")?;

		if bytes == 0 {
			return Ok(None);
		}

		let line = line.trim_end_matches(['\r', '\n']);
		if line.is_empty() {
			break;
		}

		if let Some(value) = line.strip_prefix("Content-Length:") {
			let parsed = value
				.trim()
				.parse::<usize>()
				.context("failed to parse Content-Length header")?;
			content_length = Some(parsed);
		}
	}

	let content_length = content_length.ok_or_else(|| anyhow!("missing Content-Length header"))?;
	let mut body = vec![0_u8; content_length];
	reader
		.read_exact(&mut body)
		.await
		.context("failed to read lsp frame body")?;

	let payload: Value =
		serde_json::from_slice(&body).context("failed to decode lsp frame JSON payload")?;
	Ok(Some(payload))
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;
	use std::fs;
	use std::path::PathBuf;
	use std::time::SystemTime;
	use std::time::UNIX_EPOCH;

	use lspee_config::LspConfig;
	use serde_json::json;

	use super::LspTransport;
	use super::encode_lsp_frame;

	fn test_lsp_config(command: &str, args: Vec<String>) -> LspConfig {
		LspConfig {
			id: "test".to_string(),
			command: command.to_string(),
			args,
			env: BTreeMap::new(),
			initialization_options: BTreeMap::new(),
		}
	}

	fn unique_temp_dir(name: &str) -> PathBuf {
		let nanos = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("system time should be valid")
			.as_nanos();

		let dir = std::env::temp_dir().join(format!("lspee-lsp-{name}-{nanos}"));
		fs::create_dir_all(&dir).expect("should create temp dir");
		dir
	}

	#[test]
	fn encode_frame_includes_content_length_header() {
		let payload = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
		let frame = encode_lsp_frame(&payload).expect("frame should encode");
		let frame_text = String::from_utf8_lossy(&frame);

		assert!(frame_text.starts_with("Content-Length:"));
		assert!(frame_text.contains("\r\n\r\n"));
	}

	#[cfg(unix)]
	#[tokio::test]
	async fn runtime_call_roundtrips_with_cat_process() {
		let root = unique_temp_dir("cat");
		let lsp = test_lsp_config("cat", Vec::new());
		let transport = LspTransport::new(root.clone());

		let runtime = transport
			.spawn(&lsp)
			.await
			.expect("cat runtime should spawn");

		let request = json!({
			"jsonrpc": "2.0",
			"id": 77,
			"method": "workspace/symbol",
			"params": {"query": "anything"}
		});

		let response = runtime
			.call(request.clone())
			.await
			.expect("call should roundtrip through cat");

		assert_eq!(response, request);

		runtime
			.force_stop()
			.await
			.expect("cat runtime should stop cleanly");

		let _ = fs::remove_dir_all(&root);
	}
}
