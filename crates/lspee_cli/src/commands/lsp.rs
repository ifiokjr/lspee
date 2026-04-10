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

    match cmd.output {
        LspOutput::Human => {
            println!("project_root={}", resolved.project_root.display());
            println!("config_hash={}", resolved.config_hash);
            println!("lsp_id={}", resolved.merged.lsp.id);
        }
        LspOutput::Json => {
            let payload = serde_json::json!({
                "project_root": resolved.project_root,
                "config_hash": resolved.config_hash,
                "lsp_id": resolved.merged.lsp.id,
            });
            println!("{}", serde_json::to_string(&payload)?);
        }
    }

    Ok(())
}
