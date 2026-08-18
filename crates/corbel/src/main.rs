mod cli;
mod commands;
mod mcp;

use clap::Parser;
use cli::{Cli, Command};
use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Command::Index { path } => commands::index::run(&path, cli.verbose),
        Command::Serve { path } => commands::serve::run(&path),
    }
}

fn init_tracing(verbose: bool) {
    let default_level = if verbose { "debug" } else { "info" };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!(
            "corbel={default_level},corbel_core={default_level},corbel_lang={default_level}"
        ))
    });

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}
