use std::path::Path;
use std::path::PathBuf;

use clap::Args;
use clap::ValueEnum;
use lspee_config::languages;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LspsOutput {
	Human,
	Json,
}

#[derive(Debug, Args)]
pub struct LspsCommand {
	/// Source file used to infer project root and matching LSP choices.
	#[arg(long)]
	pub file: Option<PathBuf>,

	/// Output format.
	#[arg(long, value_enum, default_value_t = LspsOutput::Human)]
	pub output: LspsOutput,
}

pub fn run(cmd: LspsCommand) -> anyhow::Result<()> {
	let Some(file) = cmd.file else {
		match cmd.output {
			LspsOutput::Human => {
				println!("lsps=none");
				println!("hint=pass --file <path> to query language registry");
			}
			LspsOutput::Json => {
				let payload = serde_json::json!({
					"file": null,
					"lsps": [],
					"hint": "pass --file <path> to query language registry",
				});
				println!("{}", serde_json::to_string(&payload)?);
			}
		}
		return Ok(());
	};

	let user_cfg = std::env::var_os("HOME")
		.map(PathBuf::from)
		.map(|home| home.join(".config/lspee/config.toml"));

	let project_cfg = file
		.parent()
		.map(|parent| parent.join("lspee.toml"))
		.filter(|path| path.exists());

	let matches = languages::lsps_for_file(
		&file,
		user_cfg.as_deref(),
		project_cfg.as_deref().map(Path::new),
	)?;

	match cmd.output {
		LspsOutput::Human => {
			if matches.is_empty() {
				println!("file={}", file.display());
				println!("lsps=none");
				println!("hint=no language server mapping found for file extension");
				return Ok(());
			}

			println!("file={}", file.display());
			for lsp in matches {
				let health = if lsp.executable_found {
					"ok"
				} else {
					"missing"
				};
				println!(
					"lsp={} command={} args={:?} health={} root_markers={:?}",
					lsp.id, lsp.command, lsp.args, health, lsp.root_markers
				);
			}
		}
		LspsOutput::Json => {
			let payload = serde_json::json!({
				"file": file,
				"lsps": matches,
			});
			println!("{}", serde_json::to_string(&payload)?);
		}
	}

	Ok(())
}
