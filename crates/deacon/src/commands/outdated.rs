// Minimal skeleton for the `outdated` subcommand
// Implementations will be expanded in later tasks per the spec

use anyhow::Result;
use tracing::info;

pub struct OutdatedArgs {
    pub workspace_folder: String,
}

pub async fn run(_args: OutdatedArgs) -> Result<()> {
    // placeholder implementation
    info!("outdated subcommand invoked");
    println!("Feature | Current | Wanted | Latest");
    Ok(())
}
