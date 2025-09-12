//! Integration tests for exec command container resolution
//!
//! These tests verify that the exec command properly resolves containers
//! based on workspace and configuration labels.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_exec_with_missing_config() {
    // Test that exec properly fails when no config file is found
    let temp_dir = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(&temp_dir)
        .arg("exec")
        .arg("echo")
        .arg("test")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("Configuration file not found"));
}

#[test]
fn test_exec_with_empty_command() {
    // Test that exec fails with no command specified
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("exec")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("No command specified for exec"));
}

#[test]
fn test_exec_with_valid_config_but_no_container() {
    // Test that exec properly loads config but fails to find container
    let temp_dir = TempDir::new().unwrap();

    // Create a basic devcontainer.json
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();

    let config_content = r#"{
        "name": "test-container",
        "image": "ubuntu:20.04"
    }"#;

    fs::write(devcontainer_dir.join("devcontainer.json"), config_content).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(&temp_dir)
        .arg("exec")
        .arg("echo")
        .arg("test")
        .assert()
        .failure()
        .code(1)
        .stderr(
            predicate::str::contains("No running container found")
                .or(predicate::str::contains("Failed to spawn docker"))
                .or(predicate::str::contains("Docker CLI error"))
                .or(predicate::str::contains(
                    "Docker is not installed or not accessible",
                )),
        );
}

#[test]
fn test_exec_invalid_env_format() {
    // Test that exec validates environment variable format
    let temp_dir = TempDir::new().unwrap();

    // Create a basic devcontainer.json
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();

    let config_content = r#"{
        "name": "test-container",
        "image": "ubuntu:20.04"
    }"#;

    fs::write(devcontainer_dir.join("devcontainer.json"), config_content).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(&temp_dir)
        .arg("exec")
        .arg("--env")
        .arg("INVALID_FORMAT") // Missing = sign
        .arg("echo")
        .arg("test")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "Invalid environment variable format",
        ));
}

#[cfg(feature = "docker")]
#[test]
fn test_exec_working_directory_config() {
    // This test would require a running Docker container, so we'll just verify
    // the config loading and parsing works correctly for workspace folder settings
    let temp_dir = TempDir::new().unwrap();

    // Create a devcontainer.json with custom workspace folder
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();

    let config_content = r#"{
        "name": "test-container",
        "image": "ubuntu:20.04",
        "workspaceFolder": "/custom/workspace"
    }"#;

    fs::write(devcontainer_dir.join("devcontainer.json"), config_content).unwrap();

    // Test with valid config that should load properly but fail to find container
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(&temp_dir)
        .arg("exec")
        .arg("echo")
        .arg("test")
        .assert()
        .failure()
        .code(1)
        .stderr(
            predicate::str::contains("No running container found")
                .or(predicate::str::contains("Failed to spawn docker"))
                .or(predicate::str::contains("Docker CLI error"))
                .or(predicate::str::contains(
                    "Docker is not installed or not accessible",
                )),
        );
}
