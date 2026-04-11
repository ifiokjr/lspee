pub mod call;
pub mod capabilities;
pub mod client;
pub mod config;
pub mod do_cmd;
pub mod doctor;
pub mod lsp;
pub mod lsps;
pub mod mcp;
pub mod proxy;
pub mod restart;
pub mod serve;
pub mod status;
pub mod stop;

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum Command {
	/// Resolve configuration and print effective session identity.
	Lsp(lsp::LspCommand),
	/// Make a synchronous JSON request through the broker.
	Call(call::CallCommand),
	/// Query LSP server capabilities (supported methods).
	Capabilities(capabilities::CapabilitiesCommand),
	/// Show daemon and session broker status.
	Status(status::StatusCommand),
	/// Run the background daemon control server in the foreground.
	Serve(serve::ServeCommand),
	/// Act as an editor-facing LSP proxy backed by the shared daemon session.
	Proxy(proxy::ProxyCommand),
	/// Stop a running daemon for the selected project root.
	Stop(stop::StopCommand),
	/// Restart daemon for the selected project root.
	Restart(restart::RestartCommand),
	/// List matching/available LSPs.
	Lsps(lsps::LspsCommand),
	/// Run health checks for environment and integration readiness.
	Doctor(doctor::DoctorCommand),
	/// Manage project configuration (show, init, add-lsp, remove-lsp, set).
	Config(config::ConfigCommand),
	/// Start an MCP (Model Context Protocol) server over stdio, exposing lspee
	/// tools for LLM integration.
	Mcp(mcp::McpCommand),
	/// Execute LSP methods with ergonomic flags — no raw JSON-RPC required.
	///
	/// Auto-resolves the LSP server from the file extension when `--lsp`
	/// is omitted. Wraps responses with metadata (`lsp_id`, method, file,
	/// position, `elapsed_ms`). For location results (definition, references,
	/// implementation, type-definition), adds a `context_line` field with
	/// the source text at each location.
	#[command(name = "do")]
	Do(do_cmd::DoCommand),
}

pub fn run(command: Command) -> anyhow::Result<()> {
	match command {
		Command::Lsp(cmd) => lsp::run(&cmd),
		Command::Call(cmd) => call::run(cmd),
		Command::Capabilities(cmd) => capabilities::run(cmd),
		Command::Status(cmd) => status::run(cmd),
		Command::Serve(cmd) => serve::run(cmd),
		Command::Proxy(cmd) => proxy::run(cmd),
		Command::Stop(cmd) => stop::run(cmd),
		Command::Restart(cmd) => restart::run(cmd),
		Command::Lsps(cmd) => lsps::run(cmd),
		Command::Doctor(cmd) => doctor::run(cmd),
		Command::Config(cmd) => config::run(&cmd),
		Command::Mcp(cmd) => mcp::run(cmd),
		Command::Do(cmd) => do_cmd::run(cmd),
	}
}
