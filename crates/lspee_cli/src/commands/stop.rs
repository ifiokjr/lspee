use clap::Args;
use lspee_config::resolve;
use lspee_daemon::{ControlEnvelope, Shutdown, ShutdownOk, TYPE_SHUTDOWN, TYPE_SHUTDOWN_OK};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::client;

#[derive(Debug, Args)]
pub struct StopCommand {
    /// Override project root used for daemon socket lookup.
    #[arg(long = "project-root")]
    pub project_root: Option<PathBuf>,
}

pub fn run(cmd: StopCommand) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run_async(cmd))
}

async fn run_async(cmd: StopCommand) -> anyhow::Result<()> {
    let resolved = resolve(cmd.project_root.as_deref())?;
    let stream = match client::connect(&resolved.project_root, false).await {
        Ok(stream) => stream,
        Err(_) => {
            println!("daemon_status=not_running");
            println!("project_root={}", resolved.project_root.display());
            return Ok(());
        }
    };

    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let req_id = client::new_request_id("shutdown");
    let request = ControlEnvelope {
        v: lspee_daemon::PROTOCOL_VERSION,
        id: Some(req_id.clone()),
        message_type: TYPE_SHUTDOWN.to_string(),
        payload: serde_json::to_value(Shutdown::default())?,
    };

    client::write_frame(&mut writer, &request).await?;
    let response = client::read_response_for_id(&mut lines, &req_id).await?;
    client::ensure_not_error(&response)?;

    if response.message_type != TYPE_SHUTDOWN_OK {
        anyhow::bail!(
            "unexpected response type for Shutdown: {}",
            response.message_type
        );
    }

    let _payload: ShutdownOk = serde_json::from_value(response.payload)
        .map_err(|error| anyhow::anyhow!("invalid ShutdownOk payload: {error}"))?;

    println!("daemon_status=stopped");
    println!("project_root={}", resolved.project_root.display());
    Ok(())
}
