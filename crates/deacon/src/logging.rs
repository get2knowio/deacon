use anyhow::Result;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

pub fn init() -> Result<()> {
    if tracing::dispatcher::has_been_set() { return Ok(()); }

    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))?;

    let fmt_layer = fmt::layer()
        .with_target(false)
        .with_line_number(true)
        .with_file(true);

    let error_layer = tracing_error::ErrorLayer::default();

    tracing_subscriber::registry()
        .with(filter)
        .with(error_layer)
        .with(fmt_layer)
        .init();
    Ok(())
}
