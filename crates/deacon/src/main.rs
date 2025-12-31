use anyhow::Result;
use clap::Parser;

mod cli;
mod commands;
mod ui;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let parsed = cli::Cli::parse();

    // Dispatch to CLI handler and handle special exit codes
    match parsed.dispatch().await {
        Ok(()) => Ok(()),
        Err(err) => {
            // Check for OutdatedExitCode (exit code 2 for --fail-on-outdated)
            #[cfg(feature = "full")]
            if let Some(outdated_exit) = err.downcast_ref::<commands::outdated::OutdatedExitCode>()
            {
                std::process::exit(outdated_exit.0);
            }

            // Check if this is a NotImplemented error for special handling
            if let Some(deacon_error) = err.downcast_ref::<deacon_core::errors::DeaconError>() {
                if matches!(
                    deacon_error,
                    deacon_core::errors::DeaconError::Config(
                        deacon_core::errors::ConfigError::NotImplemented { .. }
                    )
                ) {
                    eprintln!("Error: {}", deacon_error);
                    std::process::exit(2); // Exit code 2 for NotImplemented
                }
            }

            // For all other errors, return them normally
            Err(err)
        }
    }
}
