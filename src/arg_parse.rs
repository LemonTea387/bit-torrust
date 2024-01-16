use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub action: Action,
}

#[derive(Subcommand)]
pub enum Action {
    Decode {
        bencode: String,
    },
    Info {
        file: PathBuf,
        #[arg(long,short = 'p')]
        peer_discovery: bool,
    },
}
