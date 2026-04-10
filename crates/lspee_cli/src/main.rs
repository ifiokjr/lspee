#![cfg_attr(not(unix), allow(unused))]

#[cfg(not(unix))]
compile_error!("lspee currently supports unix-like platforms only (linux/macOS)");

mod commands;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "lspee")]
struct Cli {
    #[command(subcommand)]
    command: commands::Command,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    commands::run(cli.command)
}
