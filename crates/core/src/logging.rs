//! Logging and observability
//!
//! This module provides structured logging, tracing, and observability utilities.
//! It supports both traditional text-based logging and optional JSON formatting
//! controlled by feature flags and environment variables.

use anyhow::Result;
use std::sync::Once;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

static INIT: Once = Once::new();

/// Initialize the logging system with optional format specification
///
/// This function sets up tracing-subscriber with either JSON or text formatting
/// based on runtime configuration. It can be called multiple times
/// safely - subsequent calls will be no-ops.
///
/// ## Arguments
///
/// * `format` - Optional format specification string. Supports:
///   - `None` or `"text"` for human-readable text format
///   - `"json"` for structured JSON format
///
/// ## Environment Variables
///
/// * `DEACON_LOG` - Controls the logging filter level
/// * `RUST_LOG` - Standard Rust logging environment variable (used as fallback)
///
/// ## JSON Schema
///
/// When using JSON format, logs follow this structure:
/// ```json
/// {
///   "timestamp": "2025-09-09T23:32:24.004390Z",
///   "level": "INFO",
///   "target": "deacon_core::logging",
///   "span": { "name": "config_load", "id": 1 },
///   "message": "Configuration loaded successfully",
///   "fields": { "config_path": "/path/to/config" }
/// }
/// ```
///
/// ## Example
///
/// ```rust
/// use deacon_core::logging;
///
/// // Initialize with default text format
/// logging::init(None).expect("Failed to initialize logging");
///
/// // Initialize with JSON format
/// logging::init(Some("json")).expect("Failed to initialize logging");
/// ```
pub fn init(format: Option<&str>) -> Result<()> {
    INIT.call_once(|| {
        let filter = create_env_filter(None);

        match format {
            Some("json") => {
                tracing_subscriber::registry()
                    .with(
                        fmt::layer().json().with_target(true).with_span_events(
                            fmt::format::FmtSpan::NEW | fmt::format::FmtSpan::CLOSE,
                        ),
                    )
                    .with(filter)
                    .init();
            }
            _ => {
                // Default to text format (including None, "text", or any other value)
                tracing_subscriber::registry()
                    .with(
                        fmt::layer().with_target(true).with_span_events(
                            fmt::format::FmtSpan::NEW | fmt::format::FmtSpan::CLOSE,
                        ),
                    )
                    .with(filter)
                    .init();
            }
        }

        tracing::info!(
            "Logging initialized with format: {}",
            format.unwrap_or("text")
        );
    });

    Ok(())
}

/// Create an EnvFilter based on environment variables
fn create_env_filter(logging_spec: Option<&str>) -> EnvFilter {
    if let Some(spec) = logging_spec {
        // Use provided specification (for backward compatibility)
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
        assert!(init(Some("json")).is_ok());
        assert!(init(Some("text")).is_ok());
    }

    #[test]
    fn test_init_format_selection() {
        let _guard = TEST_MUTEX.lock().unwrap();

        // Test various format options
        assert!(init(None).is_ok()); // Default text format
        assert!(init(Some("json")).is_ok()); // JSON format
        assert!(init(Some("text")).is_ok()); // Explicit text format
        assert!(init(Some("invalid")).is_ok()); // Should fall back to text format
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

    #[test]
    fn test_json_format() {
        let _guard = TEST_MUTEX.lock().unwrap();

        // Test that JSON format initialization works
        assert!(init(Some("json")).is_ok());
    }
}
