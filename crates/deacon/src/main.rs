use anyhow::Result;
use clap::Parser;

mod cli;

fn main() -> Result<()> {
    // Parse CLI arguments
    let parsed = cli::Cli::parse();

    // Dispatch to CLI handler and handle NotImplemented errors
    match parsed.dispatch() {
        Ok(()) => Ok(()),
        Err(err) => {
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
