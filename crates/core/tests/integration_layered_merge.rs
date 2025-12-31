//! Integration tests for enhanced layered configuration merge with metadata
//!
//! This module tests the enhanced configuration merge functionality that includes
//! layer provenance tracking when using the --include-merged-configuration flag.

use anyhow::Result;
use deacon_core::config::ConfigLoader;
use serde_json::json;
use std::fs;
use tempfile::TempDir;

/// Helper function to create a test configuration file
fn create_config_file(dir: &TempDir, path: &str, content: &str) -> Result<()> {
    let full_path = dir.path().join(path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(full_path, content)?;
    Ok(())
}

#[test]
fn test_enhanced_merge_with_metadata() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create base configuration
    create_config_file(
        &temp_dir,
        "base/devcontainer.json",
        r#"{
            "name": "Base Container",
            "image": "ubuntu:20.04",
            "containerEnv": {
                "BASE_VAR": "base_value"
            },
            "runArgs": ["--base-arg"]
        }"#,
    )?;

    // Create extending configuration
    create_config_file(
        &temp_dir,
        "app/devcontainer.json",
        r#"{
            "extends": "../base/devcontainer.json",
            "name": "App Container",
            "containerEnv": {
                "APP_VAR": "app_value"
            },
            "runArgs": ["--app-arg"]
        }"#,
    )?;

    let config_path = temp_dir.path().join("app/devcontainer.json");
    let merged_result = ConfigLoader::load_with_extends_and_metadata(&config_path, true)?;

    // Check the merged configuration
    assert_eq!(merged_result.config.name, Some("App Container".to_string()));
    assert_eq!(merged_result.config.image, Some("ubuntu:20.04".to_string()));
    assert_eq!(
        merged_result.config.container_env.get("BASE_VAR"),
        Some(&"base_value".to_string())
    );
    assert_eq!(
        merged_result.config.container_env.get("APP_VAR"),
        Some(&"app_value".to_string())
    );
    assert_eq!(
        merged_result.config.run_args,
        vec!["--base-arg", "--app-arg"]
    );

    // Check the metadata
    assert!(merged_result.meta.is_some());
    let meta = merged_result.meta.unwrap();
    assert_eq!(meta.layers.len(), 2);

    // First layer should be the base config
    assert!(meta.layers[0].source.contains("base/devcontainer.json"));
    assert_eq!(meta.layers[0].precedence, 0);
    assert!(!meta.layers[0].hash.is_empty());

    // Second layer should be the app config
    assert!(meta.layers[1].source.contains("app/devcontainer.json"));
    assert_eq!(meta.layers[1].precedence, 1);
    assert!(!meta.layers[1].hash.is_empty());

    // Hashes should be different
    assert_ne!(meta.layers[0].hash, meta.layers[1].hash);

    Ok(())
}

#[test]
fn test_enhanced_merge_without_metadata() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create base configuration
    create_config_file(
        &temp_dir,
        "base/devcontainer.json",
        r#"{
            "name": "Base Container",
            "image": "ubuntu:20.04"
        }"#,
    )?;

    // Create extending configuration
    create_config_file(
        &temp_dir,
        "app/devcontainer.json",
        r#"{
            "extends": "../base/devcontainer.json",
            "name": "App Container"
        }"#,
    )?;

    let config_path = temp_dir.path().join("app/devcontainer.json");
    let merged_result = ConfigLoader::load_with_extends_and_metadata(&config_path, false)?;

    // Check the merged configuration
    assert_eq!(merged_result.config.name, Some("App Container".to_string()));
    assert_eq!(merged_result.config.image, Some("ubuntu:20.04".to_string()));

    // Metadata should be None when not requested
    assert!(merged_result.meta.is_none());

    Ok(())
}

#[test]
fn test_serialization_with_metadata() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create configuration
    create_config_file(
        &temp_dir,
        "devcontainer.json",
        r#"{
            "name": "Test Container",
            "image": "ubuntu:20.04",
            "containerEnv": {
                "TEST_VAR": "test_value"
            }
        }"#,
    )?;

    let config_path = temp_dir.path().join("devcontainer.json");
    let merged_result = ConfigLoader::load_with_extends_and_metadata(&config_path, true)?;

    // Serialize to JSON
    let json_output = serde_json::to_string_pretty(&merged_result)?;

    // Parse back to verify structure
    let parsed: serde_json::Value = serde_json::from_str(&json_output)?;

    // Check that the __meta field exists
    assert!(parsed.get("__meta").is_some());
    let meta = parsed.get("__meta").unwrap();
    assert!(meta.get("layers").is_some());

    // Check that regular config fields are present at the top level
    assert_eq!(parsed.get("name").unwrap(), &json!("Test Container"));
    assert_eq!(parsed.get("image").unwrap(), &json!("ubuntu:20.04"));

    Ok(())
}

#[test]
fn test_multi_level_extends_with_metadata() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create base configuration
    create_config_file(
        &temp_dir,
        "base/devcontainer.json",
        r#"{
            "name": "Base Container",
            "image": "ubuntu:20.04",
            "containerEnv": {
                "BASE_VAR": "base_value"
            }
        }"#,
    )?;

    // Create middle configuration
    create_config_file(
        &temp_dir,
        "middle/devcontainer.json",
        r#"{
            "extends": "../base/devcontainer.json",
            "name": "Middle Container",
            "containerEnv": {
                "MIDDLE_VAR": "middle_value"
            }
        }"#,
    )?;

    // Create final configuration
    create_config_file(
        &temp_dir,
        "app/devcontainer.json",
        r#"{
            "extends": "../middle/devcontainer.json",
            "name": "App Container",
            "containerEnv": {
                "APP_VAR": "app_value"
            }
        }"#,
    )?;

    let config_path = temp_dir.path().join("app/devcontainer.json");
    let merged_result = ConfigLoader::load_with_extends_and_metadata(&config_path, true)?;

    // Check the merged configuration
    assert_eq!(merged_result.config.name, Some("App Container".to_string()));
    assert_eq!(merged_result.config.image, Some("ubuntu:20.04".to_string()));

    // Check all environment variables are merged
    assert_eq!(
        merged_result.config.container_env.get("BASE_VAR"),
        Some(&"base_value".to_string())
    );
    assert_eq!(
        merged_result.config.container_env.get("MIDDLE_VAR"),
        Some(&"middle_value".to_string())
    );
    assert_eq!(
        merged_result.config.container_env.get("APP_VAR"),
        Some(&"app_value".to_string())
    );

    // Check the metadata for 3 layers
    assert!(merged_result.meta.is_some());
    let meta = merged_result.meta.unwrap();
    assert_eq!(meta.layers.len(), 3);

    // Check precedence order
    assert_eq!(meta.layers[0].precedence, 0);
    assert_eq!(meta.layers[1].precedence, 1);
    assert_eq!(meta.layers[2].precedence, 2);

    // Check source paths
    assert!(meta.layers[0].source.contains("base/devcontainer.json"));
    assert!(meta.layers[1].source.contains("middle/devcontainer.json"));
    assert!(meta.layers[2].source.contains("app/devcontainer.json"));

    Ok(())
}
