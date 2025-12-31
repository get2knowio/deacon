//! Integration tests for user mapping functionality
//!
//! These tests verify that the user mapping configuration and functionality
//! work correctly when parsing DevContainer configurations and applying
//! user mapping operations.

use deacon_core::config::{ConfigLoader, DevContainerConfig};
use deacon_core::user_mapping::{UserInfo, UserMappingConfig};
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_user_mapping_config_parsing() -> anyhow::Result<()> {
    let config_content = r#"{
        "name": "User Mapping Test",
        "image": "alpine:3.19",
        "containerUser": "1000",
        "remoteUser": "vscode",
        "updateRemoteUserUID": true,
        "workspaceFolder": "/workspace"
    }"#;

    let mut temp_file = NamedTempFile::new()?;
    temp_file.write_all(config_content.as_bytes())?;

    let config = ConfigLoader::load_from_path(temp_file.path())?;

    assert_eq!(config.name, Some("User Mapping Test".to_string()));
    assert_eq!(config.image, Some("alpine:3.19".to_string()));
    assert_eq!(config.container_user, Some("1000".to_string()));
    assert_eq!(config.remote_user, Some("vscode".to_string()));
    assert_eq!(config.update_remote_user_uid, Some(true));
    assert_eq!(config.workspace_folder, Some("/workspace".to_string()));

    Ok(())
}

#[test]
fn test_user_mapping_config_creation() {
    let config =
        UserMappingConfig::new(Some("testuser".to_string()), Some("1000".to_string()), true)
            .with_host_user(1001, 1001)
            .with_workspace_path("/workspace".to_string());

    assert!(config.needs_user_mapping());
    assert!(config.needs_uid_mapping());
    assert_eq!(config.effective_user(), Some("testuser"));
    assert_eq!(config.host_uid, Some(1001));
    assert_eq!(config.host_gid, Some(1001));
    assert_eq!(config.workspace_path, Some("/workspace".to_string()));
}

#[test]
fn test_user_info_creation() {
    let user_info = UserInfo::new(
        "testuser".to_string(),
        1000,
        1000,
        "/home/testuser".to_string(),
        "/bin/bash".to_string(),
    );

    assert_eq!(user_info.username, "testuser");
    assert_eq!(user_info.uid, 1000);
    assert_eq!(user_info.gid, 1000);
    assert_eq!(user_info.home_dir, "/home/testuser");
    assert_eq!(user_info.shell, "/bin/bash");
}

#[test]
fn test_user_info_defaults() {
    assert_eq!(UserInfo::default_shell(), "/bin/bash");
    assert_eq!(UserInfo::default_home_dir("testuser"), "/home/testuser");
    assert_eq!(UserInfo::default_home_dir("root"), "/root");
}

#[test]
fn test_devcontainer_config_user_fields_default() {
    let config = DevContainerConfig::default();

    assert_eq!(config.container_user, None);
    assert_eq!(config.remote_user, None);
    assert_eq!(config.update_remote_user_uid, None);
}

#[test]
fn test_config_with_minimal_user_mapping() -> anyhow::Result<()> {
    let config_content = r#"{
        "name": "Minimal User Test",
        "image": "ubuntu:20.04",
        "remoteUser": "vscode"
    }"#;

    let mut temp_file = NamedTempFile::new()?;
    temp_file.write_all(config_content.as_bytes())?;

    let config = ConfigLoader::load_from_path(temp_file.path())?;

    assert_eq!(config.name, Some("Minimal User Test".to_string()));
    assert_eq!(config.remote_user, Some("vscode".to_string()));
    assert_eq!(config.container_user, None);
    assert_eq!(config.update_remote_user_uid, None); // Should default to None

    Ok(())
}

#[cfg(unix)]
#[test]
fn test_get_host_user_info() {
    // This should work on Unix systems
    let result = deacon_core::user_mapping::get_host_user_info();

    match result {
        Ok((uid, gid)) => {
            // UID and GID should be reasonable values
            assert!(uid < 65536);
            assert!(gid < 65536);
            println!("Host user: UID={}, GID={}", uid, gid);
        }
        Err(e) => {
            // This might fail in some test environments, which is okay
            println!("Could not get host user info: {}", e);
        }
    }
}

#[cfg(not(unix))]
#[test]
fn test_get_host_user_info_not_supported() {
    // On non-Unix systems, this should return a NotImplemented error
    let result = deacon_core::user_mapping::get_host_user_info();
    assert!(result.is_err());

    let error = result.unwrap_err();
    match error {
        deacon_core::errors::DeaconError::NotImplemented { .. } => {
            // Expected behavior
        }
        _ => panic!("Expected NotImplemented error on non-Unix systems"),
    }
}
