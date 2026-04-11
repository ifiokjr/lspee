use std::path::PathBuf;

use clap::Args;

#[derive(Debug, Args)]
pub struct McpCommand {
	/// Override project root used for daemon socket lookup and config resolution.
	#[arg(long = "project-root")]
	pub project_root: Option<PathBuf>,
}

pub fn run(cmd: McpCommand) -> anyhow::Result<()> {
	let runtime = tokio::runtime::Runtime::new()?;
	runtime.block_on(async {
		let server = lspee_mcp::LspeeMcpServer::new(cmd.project_root);
		let transport = rmcp::transport::io::stdio();
		let service = rmcp::serve_server(server, transport).await?;
		service.waiting().await?;
		Ok(())
	})
}
