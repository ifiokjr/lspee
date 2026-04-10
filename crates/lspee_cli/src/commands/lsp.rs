use clap::{Args, ValueEnum};
use lspee_config::resolve;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LspOutput {
    Human,
    Json,
}

#[derive(Debug, Args)]
pub struct LspCommand {
    /// Override project root used for config resolution and session identity.
    #[arg(long = "project-root")]
    pub project_root: Option<PathBuf>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = LspOutput::Human)]
    pub output: LspOutput,
}

pub fn run(cmd: LspCommand) -> anyhow::Result<()> {
    let resolved = resolve(cmd.project_root.as_deref())?;
    let lsp_ids: Vec<&str> = resolved
        .merged
        .lsps
        .keys()
        .map(String::as_str)
        .collect();

    match cmd.output {
        LspOutput::Human => {
            println!("project_root={}", resolved.project_root.display());
            println!("config_hash={}", resolved.config_hash);
            if lsp_ids.is_empty() {
                println!("configured_lsps=none (using catalog defaults)");
            } else {
                println!("configured_lsps={}", lsp_ids.join(", "));
                for (id, lsp) in &resolved.merged.lsps {
                    println!("  {id}: command={} args={:?}", lsp.command, lsp.args);
                }
            }
        }
        LspOutput::Json => {
            let payload = serde_json::json!({
                "project_root": resolved.project_root,
                "config_hash": resolved.config_hash,
                "configured_lsps": resolved.merged.lsps,
            });
            println!("{}", serde_json::to_string(&payload)?);
        }
    }

    Ok(())
}
