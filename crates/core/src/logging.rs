//! Logging and observability
//!
//! This module provides structured logging, tracing, and observability utilities.
//! It supports both traditional text-based logging and optional JSON formatting
//! controlled by feature flags and environment variables.

use anyhow::Result;
use std::sync::Once;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

static INIT: Once = Once::new();

/// Initialize the logging system with optional logging specification
///
/// This function sets up tracing-subscriber with either JSON or text formatting
/// based on feature flags and configuration. It can be called multiple times
/// safely - subsequent calls will be no-ops.
///
/// ## Arguments
///
/// * `logging_spec` - Optional logging specification string. If None, defaults
///   are used. Format should follow EnvFilter syntax (e.g., "debug", "info,tokio=warn").
///   If not provided, the function will check the `DEACON_LOG` environment variable,
///   falling back to "info" if not set.
///
/// ## Environment Variables
///
/// * `DEACON_LOG` - Controls the logging filter level when `logging_spec` is None
/// * `RUST_LOG` - Standard Rust logging environment variable (used as fallback)
///
/// ## Features
///
/// * `json-logs` - When enabled, outputs logs in JSON format instead of text
///
/// ## Example
///
/// ```rust
/// use deacon_core::logging;
///
/// // Initialize with default settings
/// logging::init(None).expect("Failed to initialize logging");
///
/// // Initialize with custom filter
/// logging::init(Some("debug,reqwest=warn")).expect("Failed to initialize logging");
/// ```
pub fn init(logging_spec: Option<&str>) -> Result<()> {
    INIT.call_once(|| {
        let filter = create_env_filter(logging_spec);

        #[cfg(feature = "json-logs")]
        {
            tracing_subscriber::registry()
                .with(fmt::layer().json())
                .with(filter)
                .init();
        }

        #[cfg(not(feature = "json-logs"))]
        {
            tracing_subscriber::registry()
                .with(fmt::layer())
                .with(filter)
                .init();
        }

        tracing::info!("Logging initialized");
    });

    Ok(())
}

/// Create an EnvFilter based on the provided specification or environment variables
fn create_env_filter(logging_spec: Option<&str>) -> EnvFilter {
    if let Some(spec) = logging_spec {
        // Use provided specification
        EnvFilter::try_new(spec).unwrap_or_else(|_| {
            tracing::warn!(
                "Invalid logging specification '{}', using default 'info'",
                spec
            );
            EnvFilter::new("info")
        })
    } else if let Ok(deacon_log) = std::env::var("DEACON_LOG") {
        // Use DEACON_LOG environment variable
        EnvFilter::try_new(&deacon_log).unwrap_or_else(|_| {
            tracing::warn!(
                "Invalid DEACON_LOG specification '{}', using default 'info'",
                deacon_log
            );
            EnvFilter::new("info")
        })
    } else {
        // Fall back to standard RUST_LOG or default
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    }
}

/// Check if logging has been initialized
///
/// This is primarily useful for testing scenarios where you need to know
/// if the logging system has already been set up.
pub fn is_initialized() -> bool {
    INIT.is_completed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Use a mutex to ensure tests don't interfere with each other
    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_init_multiple_calls_safe() {
        let _guard = TEST_MUTEX.lock().unwrap();

        // Multiple calls should not panic or fail
        assert!(init(None).is_ok());
        assert!(init(Some("debug")).is_ok());
        assert!(init(Some("warn")).is_ok());
    }

    #[test]
    fn test_env_filter_creation() {
        // Test with specific specification
        let _filter = create_env_filter(Some("debug"));
        // We can't easily test the internal state, but we can verify it doesn't panic

        // Test with invalid specification
        let _filter = create_env_filter(Some("invalid_spec_@@"));
        // Should fall back to "info" level
    }

    #[test]
    fn test_env_filter_with_env_vars() {
        // Test with DEACON_LOG environment variable
        std::env::set_var("DEACON_LOG", "trace");
        let _filter = create_env_filter(None);
        std::env::remove_var("DEACON_LOG");

        // Test with RUST_LOG fallback
        std::env::set_var("RUST_LOG", "warn");
        let _filter = create_env_filter(None);
        std::env::remove_var("RUST_LOG");
    }

    #[test]
    fn test_is_initialized() {
        let _guard = TEST_MUTEX.lock().unwrap();

        // After calling init, should be initialized
        let _ = init(None);
        assert!(is_initialized());
    }

    #[cfg(feature = "json-logs")]
    #[test]
    fn test_json_logs_feature() {
        let _guard = TEST_MUTEX.lock().unwrap();

        // Test that initialization works with json-logs feature
        assert!(init(Some("info")).is_ok());
    }
}
