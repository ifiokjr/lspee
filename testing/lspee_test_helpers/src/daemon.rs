use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use lspee_daemon::Daemon;

/// Spawns a daemon in a background thread and returns a handle.
/// The daemon serves from the given project root.
/// Use `DaemonHandle::stop()` or drop to shut it down.
pub struct DaemonHandle {
	root: PathBuf,
	thread: Option<std::thread::JoinHandle<()>>,
}

impl DaemonHandle {
	/// Start a daemon for the given project root.
	/// Blocks until the socket is ready.
	pub fn start(root: &Path) -> Self {
		let root = root.to_path_buf();
		let root_clone = root.clone();

		let thread = std::thread::spawn(move || {
			let rt = tokio::runtime::Runtime::new().unwrap();
			rt.block_on(async {
				let resolved = lspee_config::resolve(Some(&root_clone)).unwrap();
				let daemon = Daemon::new(root_clone, resolved);
				let _ = daemon.run().await;
			});
		});

		let socket = root.join(".lspee").join("daemon.sock");

		for _ in 0..200 {
			if std::os::unix::net::UnixStream::connect(&socket).is_ok() {
				return Self {
					root,
					thread: Some(thread),
				};
			}
			std::thread::sleep(Duration::from_millis(25));
		}

		panic!(
			"daemon socket did not become available at {}",
			socket.display()
		);
	}

	/// Stop the daemon gracefully via the protocol.
	pub fn stop(mut self) {
		self.shutdown_internal();
	}

	fn shutdown_internal(&mut self) {
		let rt = tokio::runtime::Runtime::new().unwrap();

		rt.block_on(async {
			use lspee_daemon::ControlEnvelope;
			use lspee_daemon::PROTOCOL_VERSION;
			use lspee_daemon::Shutdown;
			use lspee_daemon::TYPE_SHUTDOWN;
			use tokio::io::AsyncBufReadExt;
			use tokio::io::AsyncWriteExt;
			use tokio::io::BufReader;

			let socket = self.root.join(".lspee").join("daemon.sock");

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
		});

		if let Some(thread) = self.thread.take() {
			let _ = thread.join();
		}
	}
}

impl Drop for DaemonHandle {
	fn drop(&mut self) {
		if self.thread.is_some() {
			self.shutdown_internal();
		}
	}
}
