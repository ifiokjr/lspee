use std::path::PathBuf;

use clap::Args;
use lspee_config::resolve;
use lspee_daemon::ControlEnvelope;
use lspee_daemon::Shutdown;
use lspee_daemon::Stats;
use lspee_daemon::StatsOk;
use lspee_daemon::TYPE_SHUTDOWN;
use lspee_daemon::TYPE_STATS;
use lspee_daemon::TYPE_STATS_OK;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;

use super::client;

#[derive(Debug, Args)]
pub struct RestartCommand {
	/// Override project root used for daemon socket lookup.
	#[arg(long = "project-root")]
	pub project_root: Option<PathBuf>,
}

pub fn run(cmd: RestartCommand) -> anyhow::Result<()> {
	let runtime = tokio::runtime::Runtime::new()?;
	runtime.block_on(run_async(cmd))
}

async fn run_async(cmd: RestartCommand) -> anyhow::Result<()> {
	let resolved = resolve(cmd.project_root.as_deref())?;

	// Best-effort shutdown first.
	if let Ok(stream) = client::connect(&resolved.project_root, false).await {
		let (reader, mut writer) = stream.into_split();
		let mut lines = BufReader::new(reader).lines();

		let shutdown_id = client::new_request_id("shutdown");
		let shutdown = ControlEnvelope {
			v: lspee_daemon::PROTOCOL_VERSION,
			id: Some(shutdown_id.clone()),
			message_type: TYPE_SHUTDOWN.to_string(),
			payload: serde_json::to_value(Shutdown::default())?,
		};

		let _ = client::write_frame(&mut writer, &shutdown).await;
		let _ = client::read_response_for_id(&mut lines, &shutdown_id).await;
	}

	let stream = client::connect(&resolved.project_root, true).await?;

	let (reader, mut writer) = stream.into_split();
	let mut lines = BufReader::new(reader).lines();

	let req_id = client::new_request_id("stats");
	let request = ControlEnvelope {
		v: lspee_daemon::PROTOCOL_VERSION,
		id: Some(req_id.clone()),
		message_type: TYPE_STATS.to_string(),
		payload: serde_json::to_value(Stats::default())?,
	};

	client::write_frame(&mut writer, &request).await?;
	let response = client::read_response_for_id(&mut lines, &req_id).await?;
	client::ensure_not_error(&response)?;

	if response.message_type != TYPE_STATS_OK {
		anyhow::bail!(
			"unexpected response type for Stats during restart: {}",
			response.message_type
		);
	}

	let stats: StatsOk = serde_json::from_value(response.payload)
		.map_err(|error| anyhow::anyhow!("invalid StatsOk payload: {error}"))?;

	println!("daemon_status=restarted");
	println!("project_root={}", resolved.project_root.display());
	println!("uptime_ms={}", stats.uptime_ms);
	Ok(())
}
