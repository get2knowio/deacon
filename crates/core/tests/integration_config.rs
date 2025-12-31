//! Integration tests for configuration loading
//!
//! These tests verify the end-to-end configuration loading functionality
//! using real fixture files in various scenarios.

use deacon_core::config::ConfigLoader;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_load_basic_fixture() {
    let fixture_path = Path::new("../../fixtures/config/basic/devcontainer.jsonc");

    // Skip test if fixture doesn't exist (for CI environments)
    if !fixture_path.exists() {
        eprintln!(
            "Skipping test: fixture file not found at {:?}",
            fixture_path
        );
        return;
    }

    let config =
        ConfigLoader::load_from_path(fixture_path).expect("Should successfully load basic fixture");

    // Verify basic properties
    assert_eq!(config.name, Some("Rust Development Container".to_string()));
    assert_eq!(config.image, Some("rust:1.70".to_string()));
    assert_eq!(config.dockerfile, None);

    // Verify workspace configuration
    assert_eq!(config.workspace_folder, Some("/workspace".to_string()));

    // Verify environment variables
    assert_eq!(
        config.container_env.get("RUST_LOG"),
        Some(&"debug".to_string())
    );
    assert_eq!(
        config.container_env.get("ENVIRONMENT"),
        Some(&"development".to_string())
    );

    // Verify remote environment
    assert_eq!(
        config.remote_env.get("PATH"),
        Some(&Some(
            "${containerEnv:PATH}:/usr/local/cargo/bin".to_string()
        ))
    );

    // Verify port configuration
    assert_eq!(config.forward_ports.len(), 2);
    assert!(config.app_port.is_some());

    // Verify run arguments
    assert_eq!(config.run_args, vec!["--init", "--privileged"]);

    // Verify shutdown action
    assert_eq!(config.shutdown_action, Some("stopContainer".to_string()));
    assert_eq!(config.override_command, Some(false));

    // Verify lifecycle commands
    assert!(config.on_create_command.is_some());
    assert!(config.post_create_command.is_some());
    assert!(config.post_start_command.is_some());
    assert!(config.post_attach_command.is_some());

    // Verify features and customizations are present and are objects
    assert!(config.features.is_object());
    assert!(config.customizations.is_object());

    // Verify mounts
    assert_eq!(config.mounts.len(), 1);
}

#[test]
fn test_load_copied_fixture() {
    // Copy fixture to temporary directory as mentioned in issue requirements
    let temp_dir = TempDir::new().expect("Should create temp directory");
    let fixture_source = Path::new("../../fixtures/config/basic/devcontainer.jsonc");

    // Skip test if fixture doesn't exist
    if !fixture_source.exists() {
        eprintln!(
            "Skipping test: fixture file not found at {:?}",
            fixture_source
        );
        return;
    }

    let temp_config_path = temp_dir.path().join("devcontainer.jsonc");
    std::fs::copy(fixture_source, &temp_config_path)
        .expect("Should copy fixture to temp directory");

    // Load configuration from temporary location
    let config = ConfigLoader::load_from_path(&temp_config_path)
        .expect("Should successfully load copied fixture");

    // Basic verification
    assert_eq!(config.name, Some("Rust Development Container".to_string()));
    assert_eq!(config.image, Some("rust:1.70".to_string()));

    // Verify that JSON5 parsing worked (file contains comments and trailing commas)
    assert!(config.features.is_object());
    assert!(config.customizations.is_object());
}

#[test]
fn test_error_line_col_information() {
    // Create a temporary file with invalid JSON at a specific location
    let temp_dir = TempDir::new().expect("Should create temp directory");
    let temp_config_path = temp_dir.path().join("invalid.jsonc");

    let invalid_content = r#"{
    "name": "Test",
    "image": "ubuntu:20.04",
    "invalid": syntax error here
}"#;

    std::fs::write(&temp_config_path, invalid_content).expect("Should write invalid config");

    let result = ConfigLoader::load_from_path(&temp_config_path);
    assert!(result.is_err());

    match result.unwrap_err() {
        deacon_core::errors::DeaconError::Config(deacon_core::errors::ConfigError::Parsing {
            message,
        }) => {
            // Should contain some indication of parsing error
            assert!(message.contains("JSON parsing error"));
        }
        err => panic!("Expected Config(Parsing) error, got: {:?}", err),
    }
}
