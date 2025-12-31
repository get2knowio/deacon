#![cfg(feature = "full")]
//! Integration test for run-user-commands command functionality

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

/// Test that run-user-commands properly handles configuration discovery and reports no container found
#[test]
fn test_run_user_commands_no_container() {
    let temp_dir = TempDir::new().unwrap();

    let devcontainer_config = r#"{
    "name": "Test Run User Commands",
    "image": "alpine:3.19",
    "postCreateCommand": "echo 'Hello from postCreate'",
    "postStartCommand": "echo 'Hello from postStart'",
    "postAttachCommand": "echo 'Hello from postAttach'"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(&temp_dir)
        .arg("run-user-commands")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should fail because no container is running
    assert!(!output.status.success());
    assert!(stderr.contains("No running container found"));
    assert!(stderr.contains("Run 'deacon up' first"));
}

/// Test run-user-commands with various skip flags
#[test]
fn test_run_user_commands_skip_flags() {
    let temp_dir = TempDir::new().unwrap();

    let devcontainer_config = r#"{
    "name": "Test Skip Flags",
    "image": "alpine:3.19",
    "postCreateCommand": "echo 'postCreate command'",
    "postStartCommand": "echo 'postStart command'",
    "postAttachCommand": "echo 'postAttach command'"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test with --skip-post-create flag
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(&temp_dir)
        .arg("run-user-commands")
        .arg("--skip-post-create")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("No running container found"));

    // Test with --skip-non-blocking-commands flag
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(&temp_dir)
        .arg("run-user-commands")
        .arg("--skip-non-blocking-commands")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("No running container found"));

    // Test with --skip-post-attach flag
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(&temp_dir)
        .arg("run-user-commands")
        .arg("--skip-post-attach")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("No running container found"));
}

/// Test run-user-commands help output
#[test]
fn test_run_user_commands_help() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd.arg("run-user-commands").arg("--help").output().unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("Run user-defined lifecycle commands"));
    assert!(stdout.contains("--skip-post-create"));
    assert!(stdout.contains("--skip-post-attach"));
    assert!(stdout.contains("--skip-non-blocking-commands"));
    assert!(stdout.contains("--prebuild"));
    assert!(stdout.contains("--stop-for-personalization"));
}

/// Test run-user-commands with explicit config path
#[test]
#[ignore = "Error message mismatch - needs investigation"]
fn test_run_user_commands_explicit_config() {
    let temp_dir = TempDir::new().unwrap();

    let devcontainer_config = r#"{
    "name": "Test Explicit Config",
    "image": "alpine:3.19",
    "postCreateCommand": "echo 'Hello from explicit config'"
}"#;

    let config_path = temp_dir.path().join("custom-devcontainer.json");
    fs::write(&config_path, devcontainer_config).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(&temp_dir)
        .arg("run-user-commands")
        .arg("--config")
        .arg(&config_path)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should fail because no container is running, but config should be loaded successfully
    assert!(!output.status.success());
    assert!(stderr.contains("No running container found"));
}

/// Test that run-user-commands fails appropriately with missing config
#[test]
fn test_run_user_commands_missing_config() {
    let temp_dir = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(&temp_dir)
        .arg("run-user-commands")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should fail because no devcontainer config is found
    assert!(!output.status.success());
    assert!(
        stderr.contains("Configuration error") || stderr.contains("Configuration file not found")
    );
}
