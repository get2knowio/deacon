//! Logging and observability
//!
//! This module provides structured logging, tracing, and observability utilities.

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize the logging system
pub fn init() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    tracing::info!("Logging initialized");
    Ok(())
}
