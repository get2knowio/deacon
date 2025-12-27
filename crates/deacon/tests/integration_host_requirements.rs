//! Integration tests for host requirements evaluation
//!
//! Tests the complete flow of host requirements validation in CLI commands.
//!
//! Note: These tests rely on Unix-specific host detection mechanisms.
#![cfg(unix)]

use deacon::commands::up::{execute_up, UpArgs};
use deacon_core::config::{HostRequirements, ResourceSpec};
use deacon_core::errors::{ConfigError, DeaconError};
use std::fs;
use tempfile::TempDir;

/// Create a test devcontainer.json with host requirements
fn create_test_devcontainer_with_requirements(
    temp_dir: &TempDir,
    requirements: HostRequirements,
) -> std::io::Result<()> {
    // Create .devcontainer directory
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let devcontainer_path = devcontainer_dir.join("devcontainer.json");
    let config = serde_json::json!({
        "image": "ubuntu:22.04",
        "hostRequirements": requirements
    });
    fs::write(devcontainer_path, serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

#[tokio::test]
async fn test_host_requirements_validation_passes_with_reasonable_requirements() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create config with reasonable requirements
    let requirements = HostRequirements {
        cpus: Some(ResourceSpec::Number(1.0)),
        memory: Some(ResourceSpec::String("100MB".to_string())),
        storage: Some(ResourceSpec::String("100MB".to_string())),
    };

    create_test_devcontainer_with_requirements(&temp_dir, requirements)
        .expect("Failed to create devcontainer.json");

    let args = UpArgs {
        skip_post_create: true,
        skip_non_blocking_commands: true,
        workspace_folder: Some(temp_dir.path().to_path_buf()),
        ..Default::default()
    };

    // This should not fail due to host requirements
    // Note: This test might fail for other reasons (no Docker, etc.)
    // but host requirements validation should pass
    let result = execute_up(args).await;

    // If it fails, it should not be due to host requirements validation
    if let Err(e) = result {
        if let Some(DeaconError::Config(ConfigError::Validation { message })) =
            e.downcast_ref::<DeaconError>()
        {
            if message.contains("Host requirements not met") {
                panic!(
                    "Host requirements validation failed unexpectedly: {}",
                    message
                );
            }
        }
        // Other errors are acceptable for this test
    }
}

#[tokio::test]
async fn test_host_requirements_validation_fails_with_unrealistic_requirements() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create config with unrealistic requirements
    let requirements = HostRequirements {
        cpus: Some(ResourceSpec::Number(1000.0)), // Impossible number of CPUs
        memory: Some(ResourceSpec::String("1TB".to_string())), // Very large memory
        storage: Some(ResourceSpec::String("1PB".to_string())), // Impossible storage
    };

    create_test_devcontainer_with_requirements(&temp_dir, requirements)
        .expect("Failed to create devcontainer.json");

    let args = UpArgs {
        skip_post_create: true,
        skip_non_blocking_commands: true,
        workspace_folder: Some(temp_dir.path().to_path_buf()),
        ..Default::default()
    };

    let result = execute_up(args).await;

    // This should fail due to host requirements not being met
    assert!(result.is_err());

    let err = result.unwrap_err();
    if let Some(deacon_error) = err.downcast_ref::<DeaconError>() {
        if let DeaconError::Config(ConfigError::Validation { message }) = deacon_error {
            assert!(
                message.contains("Host requirements not met"),
                "Expected host requirements validation error, got: {}",
                message
            );
        } else {
            panic!("Expected ConfigError::Validation, got: {:?}", deacon_error);
        }
    } else {
        panic!("Expected DeaconError, got: {:?}", err);
    }
}

#[tokio::test]
async fn test_host_requirements_ignored_with_flag() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create config with unrealistic requirements
    let requirements = HostRequirements {
        cpus: Some(ResourceSpec::Number(1000.0)),
        memory: Some(ResourceSpec::String("1TB".to_string())),
        storage: Some(ResourceSpec::String("1PB".to_string())),
    };

    create_test_devcontainer_with_requirements(&temp_dir, requirements)
        .expect("Failed to create devcontainer.json");

    let args = UpArgs {
        skip_post_create: true,
        skip_non_blocking_commands: true,
        workspace_folder: Some(temp_dir.path().to_path_buf()),
        ignore_host_requirements: true,
        ..Default::default()
    };

    let result = execute_up(args).await;

    // Should not fail due to host requirements when ignore flag is set
    if let Err(e) = result {
        if let Some(DeaconError::Config(ConfigError::Validation { message })) =
            e.downcast_ref::<DeaconError>()
        {
            if message.contains("Host requirements not met") {
                panic!(
                    "Host requirements validation should be ignored with flag, but got: {}",
                    message
                );
            }
        }
        // Other errors (like Docker not available) are acceptable
    }
}

#[test]
fn test_resource_spec_parsing_edge_cases() {
    use deacon_core::config::ResourceSpec;

    // Test various formats
    let spec = ResourceSpec::String("4".to_string());
    assert_eq!(spec.parse_bytes().unwrap(), 4);

    let spec = ResourceSpec::String("2.5".to_string());
    assert_eq!(spec.parse_cpu_cores().unwrap(), 2.5);

    let spec = ResourceSpec::String("512KB".to_string());
    assert_eq!(spec.parse_bytes().unwrap(), 512_000);

    let spec = ResourceSpec::String("2 GiB".to_string());
    assert_eq!(spec.parse_bytes().unwrap(), 2 * 1024 * 1024 * 1024);

    // Test error cases
    let spec = ResourceSpec::String("invalid".to_string());
    assert!(spec.parse_bytes().is_err());

    let spec = ResourceSpec::String("100XB".to_string()); // Invalid unit
    assert!(spec.parse_bytes().is_err());
}

#[test]
fn test_host_requirements_serialization() {
    use deacon_core::config::HostRequirements;

    let requirements = HostRequirements {
        cpus: Some(ResourceSpec::Number(4.0)),
        memory: Some(ResourceSpec::String("8GB".to_string())),
        storage: Some(ResourceSpec::String("50GB".to_string())),
    };

    // Test serialization/deserialization
    let json = serde_json::to_string(&requirements).unwrap();
    let deserialized: HostRequirements = serde_json::from_str(&json).unwrap();

    assert_eq!(requirements, deserialized);
}
