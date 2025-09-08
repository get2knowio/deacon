use anyhow::Result;
use clap::Parser;

mod cli;
mod logging;
mod error;
mod commands;

fn main() -> Result<()> {
    // color-eyre returns a different error type; map into anyhow
    color_eyre::install().map_err(|e| anyhow::anyhow!(e))?;
    logging::init()?;
    let parsed = cli::Cli::parse();
    parsed.dispatch()?;
    Ok(())
}

