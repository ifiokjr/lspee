use clap::Args;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct ServeCommand {
    /// Override project root used for daemon socket location and config resolution.
    #[arg(long = "project-root")]
    pub project_root: Option<PathBuf>,
}

pub fn run(cmd: ServeCommand) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run_async(cmd))
}

async fn run_async(cmd: ServeCommand) -> anyhow::Result<()> {
    let resolved = lspee_config::resolve(cmd.project_root.as_deref())?;
    let daemon = lspee_daemon::Daemon::new(resolved.project_root.clone(), resolved);
    daemon.run().await
}
