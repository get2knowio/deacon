use anyhow::Result;
use clap::Parser;

mod cli;

fn main() -> Result<()> {
    // Initialize logging from core crate
    deacon_core::logging::init()?;

    // Parse CLI arguments
    let parsed = cli::Cli::parse();

    // Dispatch to CLI handler
    parsed.dispatch()?;

    Ok(())
}
