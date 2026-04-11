use std::path::PathBuf;

use clap::Args;
use clap::ValueEnum;
use lspee_config::languages;
use lspee_config::resolve;
use serde_json::json;

use super::client;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DoctorOutput {
	Human,
	Json,
}

#[derive(Debug, Args)]
pub struct DoctorCommand {
	/// Override project root used for checks.
	#[arg(long = "project-root")]
	pub project_root: Option<PathBuf>,

	/// Output format.
	#[arg(long, value_enum, default_value_t = DoctorOutput::Human)]
	pub output: DoctorOutput,
}

pub fn run(cmd: DoctorCommand) -> anyhow::Result<()> {
	let runtime = tokio::runtime::Runtime::new()?;
	runtime.block_on(run_async(cmd))
}

async fn run_async(cmd: DoctorCommand) -> anyhow::Result<()> {
	let resolved = resolve(cmd.project_root.as_deref())?;

	let daemon_socket = client::daemon_socket_path(&resolved.project_root);
	let daemon_reachable = client::connect(&resolved.project_root, false).await.is_ok();

	let user_cfg = std::env::var_os("HOME")
		.map(PathBuf::from)
		.map(|home| home.join(".config/lspee/config.toml"));
	let project_cfg = resolved.project_root.join("lspee.toml");

	let sample_ids = ["rust-analyzer", "pyright", "gopls", "taplo"];
	let mut lsp_health = Vec::new();
	for lsp_id in sample_ids {
		if let Some(selection) =
			languages::lsp_for_id(lsp_id, user_cfg.as_deref(), Some(project_cfg.as_path()))?
		{
			lsp_health.push(json!({
				"id": lsp_id,
				"command": selection.command,
				"executable_found": selection.executable_found,
			}));
		}
	}

	match cmd.output {
		DoctorOutput::Human => {
			println!("doctor=ok");
			println!("project_root={}", resolved.project_root.display());
			println!("check=config_resolution:ok");
			println!(
				"check=daemon_socket:{} ({})",
				if daemon_reachable {
					"ok"
				} else {
					"not_running"
				},
				daemon_socket.display()
			);
			for item in &lsp_health {
				println!(
					"check=lsp_binary:{}:{} ({})",
					item["id"].as_str().unwrap_or("unknown"),
					if item["executable_found"].as_bool().unwrap_or(false) {
						"ok"
					} else {
						"missing"
					},
					item["command"].as_str().unwrap_or("")
				);
			}
		}
		DoctorOutput::Json => {
			let payload = json!({
				"doctor": "ok",
				"project_root": resolved.project_root,
				"checks": {
					"config_resolution": "ok",
					"daemon_socket": {
						"status": if daemon_reachable { "ok" } else { "not_running" },
						"path": daemon_socket,
					},
					"lsp_binaries": lsp_health,
				}
			});
			println!("{}", serde_json::to_string(&payload)?);
		}
	}

	Ok(())
}
