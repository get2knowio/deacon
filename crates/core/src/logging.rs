//! Logging and observability
//!
//! This module provides structured logging, tracing, and observability utilities.
//! It supports both traditional text-based logging and optional JSON formatting,
//! controlled at runtime via environment variables and CLI flags (no feature flags).
//!
//! All logging output is directed to stderr to preserve stdout for command output.

use crate::redaction::RedactionConfig;
use anyhow::Result;
use std::{io, sync::Once};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

static INIT: Once = Once::new();

/// Initialize the logging system with optional format specification and redaction config
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
/// * `redaction_config` - Optional redaction configuration. If None, defaults to enabled.
///
/// ## Environment Variables
///
/// * `DEACON_LOG_FORMAT` - Controls the log output format ("json" for JSON, any other value for text)
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
/// use deacon_core::redaction::RedactionConfig;
///
/// // Initialize with default text format
/// logging::init_with_redaction(None, None).expect("Failed to initialize logging");
///
/// // Initialize with JSON format and disabled redaction
/// logging::init_with_redaction(Some("json"), Some(RedactionConfig::disabled())).expect("Failed to initialize logging");
///
/// // Environment variable usage:
/// // export DEACON_LOG_FORMAT=json  # Enables JSON logging
/// // Then call: logging::init(None)  # Will use JSON format from env
/// ```
pub fn init_with_redaction(
    format: Option<&str>,
    redaction_config: Option<RedactionConfig>,
) -> Result<()> {
    let _redaction_config = redaction_config.unwrap_or_default();
    INIT.call_once(|| {
        let filter = create_env_filter(None);

        // Determine format from parameter or environment variable
        let env_format = std::env::var("DEACON_LOG_FORMAT").ok();
        let effective_format = format.or(env_format.as_deref()).unwrap_or("text");

        // Control span lifecycle event verbosity via env var; default depends on format
        // - text: NONE (reduce noise)
        // - json: NEW | CLOSE (preserve observability for tests/tools)
        let span_events = span_events_for_format(effective_format);

        match effective_format {
            "json" => {
                tracing_subscriber::registry()
                    .with(
                        fmt::layer()
                            .json()
                            .with_target(true)
                            .with_span_events(span_events)
                            .with_writer(io::stderr),
                    )
                    .with(filter)
                    .init();
            }
            _ => {
                // Default to text format (including None, "text", or any other value)
                tracing_subscriber::registry()
                    .with(
                        fmt::layer()
                            .with_target(true)
                            .with_span_events(span_events)
                            .with_writer(io::stderr),
                    )
                    .with(filter)
                    .init();
            }
        }

        tracing::debug!("Logging initialized with format: {}", effective_format);
    });

    Ok(())
}

/// Initialize the logging system with optional format specification
///
/// This is a convenience wrapper around `init_with_redaction` that uses default redaction settings.
pub fn init(format: Option<&str>) -> Result<()> {
    init_with_redaction(format, None)
}

/// Create an EnvFilter based on environment variables
fn create_env_filter(_logging_spec: Option<&str>) -> EnvFilter {
    if let Ok(deacon_log) = std::env::var("DEACON_LOG") {
        // Use DEACON_LOG environment variable
        EnvFilter::try_new(&deacon_log).unwrap_or_else(|_| {
            tracing::warn!(
                "Invalid DEACON_LOG specification '{}', using default 'info'",
                deacon_log
            );
            EnvFilter::new("info")
        })
    } else {
        // Fall back to standard RUST_LOG or default (info)
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    }
}

/// Determine span lifecycle event configuration based on env var and format
fn span_events_for_format(format: &str) -> fmt::format::FmtSpan {
    use fmt::format::FmtSpan;

    // If env var is set, it overrides defaults
    if let Ok(raw) = std::env::var("DEACON_LOG_SPAN_EVENTS") {
        let mut acc = FmtSpan::NONE;
        for token in raw.split(&[',', '|'][..]).map(|t| t.trim().to_lowercase()) {
            acc |= match token.as_str() {
                "none" => FmtSpan::NONE,
                "new" => FmtSpan::NEW,
                "close" => FmtSpan::CLOSE,
                "enter" => FmtSpan::ENTER,
                "exit" => FmtSpan::EXIT,
                "active" => FmtSpan::ACTIVE,
                "full" => FmtSpan::FULL,
                _ => FmtSpan::NONE,
            };
        }
        return acc;
    }

    match format {
        "json" => FmtSpan::NEW | FmtSpan::CLOSE,
        _ => FmtSpan::NONE,
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
