use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = env!("CARGO_PKG_NAME"),
    version,
    about = "DevContainer CLI (WIP) – no commands implemented yet",
    long_about = "Development container CLI (Rust reimplementation)\n\nThis is a work-in-progress implementation of a DevContainer CLI. No functional commands are implemented yet."
)]
pub struct Cli {
    // No subcommands yet - just global options
}

impl Cli {
    pub fn dispatch(self) -> Result<()> {
        // Just print the placeholder message for now
        println!("DevContainer CLI (WIP) – no commands implemented yet");
        println!("Run with --help to see available options.");
        Ok(())
    }
}
