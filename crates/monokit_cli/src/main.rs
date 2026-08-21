#![cfg_attr(not(unix), allow(unused))]

#[cfg(not(unix))]
compile_error!("monokit currently supports unix-like platforms only (linux/macOS)");

use clap::Parser;
use monokit_cli::commands;

#[derive(Debug, Parser)]
#[command(name = "monokit")]
struct Cli {
	#[command(subcommand)]
	command: commands::Command,
}

fn main() -> anyhow::Result<()> {
	let cli = Cli::parse();

	commands::run(cli.command)
}
