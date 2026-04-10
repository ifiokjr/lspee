use clap::{Args, ValueEnum};
use lspee_config::resolve;
use lspee_daemon::{ControlEnvelope, Stats, StatsOk, TYPE_STATS, TYPE_STATS_OK};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::client;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum StatusOutput {
    /// Human-readable key/value output.
    Human,
    /// Compact JSON output for automation/agents.
    Json,
}

#[derive(Debug, Args)]
pub struct StatusCommand {
    /// Override project root used for daemon socket lookup.
    #[arg(long = "project-root")]
    pub project_root: Option<PathBuf>,

    /// Disable daemon auto-start when socket is missing.
    #[arg(long)]
    pub no_start_daemon: bool,

    /// Output format.
    #[arg(long, value_enum, default_value_t = StatusOutput::Human)]
    pub output: StatusOutput,
}

pub fn run(cmd: StatusCommand) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run_async(cmd))
}

async fn run_async(cmd: StatusCommand) -> anyhow::Result<()> {
    let resolved = resolve(cmd.project_root.as_deref())?;
    let stream = client::connect(&resolved.project_root, !cmd.no_start_daemon).await?;

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
            "unexpected response type for Stats: {}",
            response.message_type
        );
    }

    let stats: StatsOk = serde_json::from_value(response.payload)
        .map_err(|e| anyhow::anyhow!("invalid StatsOk payload: {e}"))?;

    match cmd.output {
        StatusOutput::Human => {
            println!("daemon_status=ok");
            println!("project_root={}", resolved.project_root.display());
            println!("sessions={}", stats.sessions);
            println!("leases={}", stats.leases);
            println!("uptime_ms={}", stats.uptime_ms);
            println!("memory_total_bytes={}", stats.memory.total_bytes);
            println!(
                "memory_limits=max_session_bytes:{:?} max_total_bytes:{:?}",
                stats.memory.max_session_bytes, stats.memory.max_total_bytes,
            );
            println!(
                "counters=sessions_spawned_total:{} sessions_reused_total:{} sessions_gc_idle_total:{} sessions_evicted_memory_total:{} session_crashes_total:{} attach_requests_total:{}",
                stats.counters.sessions_spawned_total,
                stats.counters.sessions_reused_total,
                stats.counters.sessions_gc_idle_total,
                stats.counters.sessions_evicted_memory_total,
                stats.counters.session_crashes_total,
                stats.counters.attach_requests_total
            );
        }
        StatusOutput::Json => {
            let payload = serde_json::json!({
                "daemon_status": "ok",
                "project_root": resolved.project_root,
                "stats": stats,
            });
            println!("{}", serde_json::to_string(&payload)?);
        }
    }

    Ok(())
}
