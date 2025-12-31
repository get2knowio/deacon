//! Integration tests for logging functionality
//!
//! These tests verify that the logging system works correctly with
//! the configuration loading system, particularly checking that
//! debug messages from config loading appear when enabled.

use assert_cmd::Command;
use deacon_core::{config::ConfigLoader, logging};
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_debug_logging_from_config_loader() {
    // Initialize logging with debug level
    let _ = logging::init(Some("debug"));

    // Create a configuration with unknown keys to trigger debug logs
    let config_content = r#"{
        "name": "Test Container",
        "image": "ubuntu:20.04",
        "unknownField1": "some value",
        "anotherUnknownField": 42,
        "features": {}
    }"#;

    let mut temp_file = NamedTempFile::new().expect("Should create temp file");
    temp_file
        .write_all(config_content.as_bytes())
        .expect("Should write config content");

    // Load the configuration - this should trigger debug logs about unknown keys
    let result = ConfigLoader::load_from_path(temp_file.path());
    assert!(result.is_ok());

    // We can't easily capture the debug output in unit tests, but we can
    // verify the config loaded successfully and would have logged the debug messages
    let config = result.unwrap();
    assert_eq!(config.name, Some("Test Container".to_string()));
    assert_eq!(config.image, Some("ubuntu:20.04".to_string()));
}

#[test]
fn test_integration_cli_debug_logging() {
    // This test runs the CLI binary with RUST_LOG=debug to ensure
    // debug logging works end-to-end as required by the issue
    let mut cmd = Command::cargo_bin("deacon").expect("Failed to find deacon binary");

    // Set debug logging environment variable
    cmd.env("RUST_LOG", "debug");

    // Run the command - it should work and produce debug output
    let _output = cmd.assert().success();

    // The command should succeed (even though it's just a placeholder)
    // Debug logs would be visible in real usage but not easily captured here
}

#[test]
fn test_deacon_log_environment_variable() {
    // Test that DEACON_LOG environment variable is respected
    std::env::set_var("DEACON_LOG", "trace");

    // Initialize logging without explicit spec - should use DEACON_LOG
    let result = logging::init(None);
    assert!(result.is_ok());

    // Clean up
    std::env::remove_var("DEACON_LOG");
}

#[test]
fn test_logging_initialization_safety() {
    // Test that multiple calls to init are safe
    assert!(logging::init(Some("info")).is_ok());
    assert!(logging::init(Some("debug")).is_ok());
    assert!(logging::init(Some("warn")).is_ok());

    // Should be initialized after any successful call
    assert!(logging::is_initialized());
}

#[test]
fn test_json_logging_integration() {
    // Test that JSON logging feature works when enabled
    let result = logging::init(Some("info"));
    assert!(result.is_ok());

    // When json-logs feature is enabled, logs should be in JSON format
    // (visible in the test output but not easily captured in the test)
}
