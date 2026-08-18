use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "corbel", version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Index {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    Serve {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}
