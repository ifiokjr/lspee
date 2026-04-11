use std::path::PathBuf;

use clap::Args;
use clap::ValueEnum;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LogFormat {
	/// Human-readable log output.
	Human,
	/// Structured JSON log output (best for agents).
	Json,
}

#[derive(Debug, Args)]
pub struct ServeCommand {
	/// Override project root used for daemon socket location and config resolution.
	#[arg(long = "project-root")]
	pub project_root: Option<PathBuf>,

	/// Log output format.
	#[arg(long, value_enum, env = "LSPEE_LOG_FORMAT", default_value_t = LogFormat::Human)]
	pub log_format: LogFormat,

	/// Write logs to a file instead of stderr.
	#[arg(long, env = "LSPEE_LOG_FILE")]
	pub log_file: Option<PathBuf>,
}

pub fn run(cmd: ServeCommand) -> anyhow::Result<()> {
	init_tracing(&cmd)?;
	let runtime = tokio::runtime::Runtime::new()?;
	runtime.block_on(run_async(cmd))
}

fn init_tracing(cmd: &ServeCommand) -> anyhow::Result<()> {
	let filter =
		EnvFilter::try_from_env("LSPEE_LOG").unwrap_or_else(|_| EnvFilter::new("lspee=info,warn"));

	if let Some(log_path) = &cmd.log_file {
		if let Some(parent) = log_path.parent() {
			std::fs::create_dir_all(parent)?;
		}
		let file = std::fs::OpenOptions::new()
			.create(true)
			.append(true)
			.open(log_path)?;

		match cmd.log_format {
			LogFormat::Json => {
				tracing_subscriber::fmt()
					.json()
					.with_env_filter(filter)
					.with_writer(file)
					.with_target(true)
					.init();
			}
			LogFormat::Human => {
				tracing_subscriber::fmt()
					.with_env_filter(filter)
					.with_writer(file)
					.with_target(true)
					.init();
			}
		}
	} else {
		match cmd.log_format {
			LogFormat::Json => {
				tracing_subscriber::fmt()
					.json()
					.with_env_filter(filter)
					.with_target(true)
					.init();
			}
			LogFormat::Human => {
				tracing_subscriber::fmt()
					.with_env_filter(filter)
					.with_target(true)
					.init();
			}
		}
	}

	Ok(())
}

async fn run_async(cmd: ServeCommand) -> anyhow::Result<()> {
	let resolved = lspee_config::resolve(cmd.project_root.as_deref())?;
	let daemon = lspee_daemon::Daemon::new(resolved.project_root.clone(), resolved);
	daemon.run().await
}
