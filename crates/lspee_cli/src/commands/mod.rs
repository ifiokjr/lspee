pub mod call;
mod client;
pub mod config;
pub mod doctor;
pub mod lsp;
pub mod lsps;
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
}

pub fn run(command: Command) -> anyhow::Result<()> {
    match command {
        Command::Lsp(cmd) => lsp::run(cmd),
        Command::Call(cmd) => call::run(cmd),
        Command::Status(cmd) => status::run(cmd),
        Command::Serve(cmd) => serve::run(cmd),
        Command::Proxy(cmd) => proxy::run(cmd),
        Command::Stop(cmd) => stop::run(cmd),
        Command::Restart(cmd) => restart::run(cmd),
        Command::Lsps(cmd) => lsps::run(cmd),
        Command::Doctor(cmd) => doctor::run(cmd),
        Command::Config(cmd) => config::run(cmd),
    }
}
