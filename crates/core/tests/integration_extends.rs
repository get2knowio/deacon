//! Integration tests for configuration extends functionality
//!
//! This module tests the extends chain resolution, configuration merging,
//! and cycle detection as required by the CLI specification.

use anyhow::Result;
use deacon_core::config::{ConfigLoader, ConfigMerger, DevContainerConfig};
use deacon_core::errors::{ConfigError, DeaconError};
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
fn test_simple_extends() -> Result<()> {
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
    let merged_config = ConfigLoader::load_with_extends(&config_path)?;

    // Check merged values
    assert_eq!(merged_config.name, Some("App Container".to_string())); // Override
    assert_eq!(merged_config.image, Some("ubuntu:20.04".to_string())); // From base

    // Environment variables should merge
    assert_eq!(
        merged_config.container_env.get("BASE_VAR"),
        Some(&"base_value".to_string())
    );
    assert_eq!(
        merged_config.container_env.get("APP_VAR"),
        Some(&"app_value".to_string())
    );

    // runArgs should concatenate
    assert_eq!(merged_config.run_args, vec!["--base-arg", "--app-arg"]);

    // extends should be removed from final config
    assert_eq!(merged_config.extends, None);

    Ok(())
}

#[test]
fn test_multi_level_extends() -> Result<()> {
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

    // Create middle configuration
    create_config_file(
        &temp_dir,
        "middle/devcontainer.json",
        r#"{
            "extends": "../base/devcontainer.json",
            "name": "Middle Container",
            "containerEnv": {
                "MIDDLE_VAR": "middle_value",
                "BASE_VAR": "overridden_base_value"
            },
            "runArgs": ["--middle-arg"]
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
            },
            "runArgs": ["--app-arg"]
        }"#,
    )?;

    let config_path = temp_dir.path().join("app/devcontainer.json");
    let merged_config = ConfigLoader::load_with_extends(&config_path)?;

    // Check merged values (app overrides middle overrides base)
    assert_eq!(merged_config.name, Some("App Container".to_string()));
    assert_eq!(merged_config.image, Some("ubuntu:20.04".to_string()));

    // Environment variables should merge with proper precedence
    assert_eq!(
        merged_config.container_env.get("BASE_VAR"),
        Some(&"overridden_base_value".to_string())
    );
    assert_eq!(
        merged_config.container_env.get("MIDDLE_VAR"),
        Some(&"middle_value".to_string())
    );
    assert_eq!(
        merged_config.container_env.get("APP_VAR"),
        Some(&"app_value".to_string())
    );

    // runArgs should concatenate in order: base -> middle -> app
    assert_eq!(
        merged_config.run_args,
        vec!["--base-arg", "--middle-arg", "--app-arg"]
    );

    Ok(())
}

#[test]
fn test_multiple_extends_array() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create first base configuration
    create_config_file(
        &temp_dir,
        "base1/devcontainer.json",
        r#"{
            "name": "Base1 Container",
            "image": "ubuntu:20.04",
            "containerEnv": {
                "BASE1_VAR": "base1_value"
            },
            "runArgs": ["--base1-arg"]
        }"#,
    )?;

    // Create second base configuration
    create_config_file(
        &temp_dir,
        "base2/devcontainer.json",
        r#"{
            "name": "Base2 Container",
            "containerEnv": {
                "BASE2_VAR": "base2_value",
                "BASE1_VAR": "overridden_by_base2"
            },
            "runArgs": ["--base2-arg"]
        }"#,
    )?;

    // Create extending configuration
    create_config_file(
        &temp_dir,
        "app/devcontainer.json",
        r#"{
            "extends": ["../base1/devcontainer.json", "../base2/devcontainer.json"],
            "name": "App Container",
            "containerEnv": {
                "APP_VAR": "app_value"
            },
            "runArgs": ["--app-arg"]
        }"#,
    )?;

    let config_path = temp_dir.path().join("app/devcontainer.json");
    let merged_config = ConfigLoader::load_with_extends(&config_path)?;

    // Check merged values (app overrides base2 overrides base1)
    assert_eq!(merged_config.name, Some("App Container".to_string()));
    assert_eq!(merged_config.image, Some("ubuntu:20.04".to_string())); // From base1

    // Environment variables should merge with proper precedence
    assert_eq!(
        merged_config.container_env.get("BASE1_VAR"),
        Some(&"overridden_by_base2".to_string())
    );
    assert_eq!(
        merged_config.container_env.get("BASE2_VAR"),
        Some(&"base2_value".to_string())
    );
    assert_eq!(
        merged_config.container_env.get("APP_VAR"),
        Some(&"app_value".to_string())
    );

    // runArgs should concatenate in order: base1 -> base2 -> app
    assert_eq!(
        merged_config.run_args,
        vec!["--base1-arg", "--base2-arg", "--app-arg"]
    );

    Ok(())
}

#[test]
fn test_cycle_detection() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create configuration A that extends B
    create_config_file(
        &temp_dir,
        "a/devcontainer.json",
        r#"{
            "extends": "../b/devcontainer.json",
            "name": "Config A"
        }"#,
    )?;

    // Create configuration B that extends C
    create_config_file(
        &temp_dir,
        "b/devcontainer.json",
        r#"{
            "extends": "../c/devcontainer.json",
            "name": "Config B"
        }"#,
    )?;

    // Create configuration C that extends A (creates cycle)
    create_config_file(
        &temp_dir,
        "c/devcontainer.json",
        r#"{
            "extends": "../a/devcontainer.json",
            "name": "Config C"
        }"#,
    )?;

    let config_path = temp_dir.path().join("a/devcontainer.json");
    let result = ConfigLoader::load_with_extends(&config_path);

    assert!(result.is_err());
    match result.unwrap_err() {
        DeaconError::Config(ConfigError::ExtendsCycle { chain }) => {
            assert!(chain.contains("devcontainer.json"));
        }
        _ => panic!("Expected Config(ExtendsCycle) error"),
    }

    Ok(())
}

#[test]
fn test_features_merge() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create base configuration with features
    create_config_file(
        &temp_dir,
        "base/devcontainer.json",
        r#"{
            "features": {
                "ghcr.io/devcontainers/features/common-utils:1": {
                    "installZsh": true
                },
                "ghcr.io/devcontainers/features/docker-in-docker:1": {}
            }
        }"#,
    )?;

    // Create extending configuration with additional features
    create_config_file(
        &temp_dir,
        "app/devcontainer.json",
        r#"{
            "extends": "../base/devcontainer.json",
            "features": {
                "ghcr.io/devcontainers/features/common-utils:1": {
                    "installZsh": false,
                    "username": "devuser"
                },
                "ghcr.io/devcontainers/features/node:1": {
                    "version": "18"
                }
            }
        }"#,
    )?;

    let config_path = temp_dir.path().join("app/devcontainer.json");
    let merged_config = ConfigLoader::load_with_extends(&config_path)?;

    // Features should deep merge
    let features = &merged_config.features;
    assert!(features.is_object());

    let features_obj = features.as_object().unwrap();

    // common-utils should be merged with app taking precedence
    let common_utils = &features_obj["ghcr.io/devcontainers/features/common-utils:1"];
    assert_eq!(common_utils["installZsh"], json!(false)); // Overridden
    assert_eq!(common_utils["username"], json!("devuser")); // Added

    // docker-in-docker should be preserved from base
    assert!(features_obj.contains_key("ghcr.io/devcontainers/features/docker-in-docker:1"));

    // node should be added from app
    let node_feature = &features_obj["ghcr.io/devcontainers/features/node:1"];
    assert_eq!(node_feature["version"], json!("18"));

    Ok(())
}

#[test]
fn test_lifecycle_commands_override() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create base configuration with lifecycle commands
    create_config_file(
        &temp_dir,
        "base/devcontainer.json",
        r#"{
            "postCreateCommand": "echo 'base post create'",
            "postStartCommand": ["echo", "base", "post", "start"]
        }"#,
    )?;

    // Create extending configuration with some lifecycle commands
    create_config_file(
        &temp_dir,
        "app/devcontainer.json",
        r#"{
            "extends": "../base/devcontainer.json",
            "postCreateCommand": ["echo", "app", "post", "create"],
            "postAttachCommand": "echo 'app post attach'"
        }"#,
    )?;

    let config_path = temp_dir.path().join("app/devcontainer.json");
    let merged_config = ConfigLoader::load_with_extends(&config_path)?;

    // Lifecycle commands should override (not merge)
    assert_eq!(
        merged_config.post_create_command,
        Some(json!(["echo", "app", "post", "create"]))
    );
    assert_eq!(
        merged_config.post_start_command,
        Some(json!(["echo", "base", "post", "start"]))
    );
    assert_eq!(
        merged_config.post_attach_command,
        Some(json!("echo 'app post attach'"))
    );

    Ok(())
}

#[test]
fn test_oci_reference_not_implemented() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create configuration with OCI reference
    create_config_file(
        &temp_dir,
        "app/devcontainer.json",
        r#"{
            "extends": "ghcr.io/devcontainers/features/base:latest",
            "name": "App Container"
        }"#,
    )?;

    let config_path = temp_dir.path().join("app/devcontainer.json");
    let result = ConfigLoader::load_with_extends(&config_path);

    assert!(result.is_err());
    match result.unwrap_err() {
        DeaconError::Config(ConfigError::NotImplemented { feature }) => {
            assert!(feature.contains("OCI extends reference"));
        }
        _ => panic!("Expected Config(NotImplemented) error"),
    }

    Ok(())
}

#[test]
fn test_config_merger_empty_configs() {
    let configs = vec![];
    let merged = ConfigMerger::merge_configs(&configs);
    assert_eq!(merged, DevContainerConfig::default());
}

#[test]
fn test_config_merger_single_config() {
    let config = DevContainerConfig {
        name: Some("Test".to_string()),
        ..Default::default()
    };

    let configs = vec![config.clone()];
    let merged = ConfigMerger::merge_configs(&configs);
    assert_eq!(merged, config);
}

#[test]
fn test_config_merger_runargs_concatenation() {
    let config1 = DevContainerConfig {
        run_args: vec!["--arg1".to_string(), "--arg2".to_string()],
        ..Default::default()
    };

    let config2 = DevContainerConfig {
        run_args: vec!["--arg3".to_string()],
        ..Default::default()
    };

    let configs = vec![config1, config2];
    let merged = ConfigMerger::merge_configs(&configs);

    assert_eq!(merged.run_args, vec!["--arg1", "--arg2", "--arg3"]);
}

#[test]
fn test_config_merger_env_merge() {
    let config1 = DevContainerConfig {
        container_env: [
            ("VAR1".to_string(), "value1".to_string()),
            ("VAR2".to_string(), "value2".to_string()),
        ]
        .iter()
        .cloned()
        .collect(),
        ..Default::default()
    };

    let config2 = DevContainerConfig {
        container_env: [
            ("VAR2".to_string(), "overridden_value2".to_string()),
            ("VAR3".to_string(), "value3".to_string()),
        ]
        .iter()
        .cloned()
        .collect(),
        ..Default::default()
    };

    let configs = vec![config1, config2];
    let merged = ConfigMerger::merge_configs(&configs);

    assert_eq!(
        merged.container_env.get("VAR1"),
        Some(&"value1".to_string())
    );
    assert_eq!(
        merged.container_env.get("VAR2"),
        Some(&"overridden_value2".to_string())
    );
    assert_eq!(
        merged.container_env.get("VAR3"),
        Some(&"value3".to_string())
    );
}
