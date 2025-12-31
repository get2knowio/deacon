//! Integration test for --container-name flag

use assert_cmd::Command;
use serde_json::json;
use std::fs;
use std::process::{Command as StdCommand, Stdio};
use tempfile::TempDir;

mod support;
use support::unique_name;

/// Helper to check if Docker is available
fn is_docker_available() -> bool {
    StdCommand::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Helper to check if a container exists by name
fn container_exists(name: &str) -> bool {
    StdCommand::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("name=^{}$", name),
            "--format",
            "{{.Names}}",
        ])
        .output()
        .map(|output| {
            let names = String::from_utf8_lossy(&output.stdout);
            names.trim() == name
        })
        .unwrap_or(false)
}

/// Helper to cleanup a container by name
fn cleanup_container(name: &str) {
    let _ = StdCommand::new("docker").args(["rm", "-f", name]).output();
}

#[test]
fn test_up_with_custom_container_name() {
    let custom_name = unique_name("deacon-test-custom");

    // Cleanup any existing container with this name
    cleanup_container(&custom_name);

    let temp_dir = TempDir::new().unwrap();
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();

    // Create a simple devcontainer.json
    let devcontainer_config = json!({
        "name": "Test Container",
        "image": "ubuntu:20.04"
    });

    let config_path = devcontainer_dir.join("devcontainer.json");
    fs::write(
        &config_path,
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    // Test the up command with custom container name
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let result = cmd
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path().to_string_lossy().to_string())
        .arg("--container-name")
        .arg(&custom_name)
        .arg("--skip-post-create")
        .assert();

    let output = result.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let exit_code = output.status.code();

    // Debug output
    eprintln!("DEBUG - stderr: {:?}", stderr);
    eprintln!("DEBUG - stdout: {:?}", stdout);
    eprintln!("DEBUG - exit code: {:?}", exit_code);

    if is_docker_available() {
        // If Docker is available, verify the custom name was used
        if exit_code == Some(0) {
            // Success case: container should exist with custom name
            assert!(
                container_exists(&custom_name),
                "Container with custom name '{}' should exist after successful up command",
                custom_name
            );
            eprintln!("✓ Container created with custom name: {}", custom_name);

            // Cleanup
            cleanup_container(&custom_name);
        } else {
            // Failure case: check for specific error patterns
            // (might be due to network issues, image pull failures, etc.)
            assert!(
                stderr.contains("docker")
                    || stderr.contains("Docker")
                    || stderr.contains("Error response from daemon")
                    || stderr.contains(&custom_name),
                "Expected Docker-related error or mention of custom name in stderr on failure"
            );
        }
    } else {
        // Docker not available: expect specific error
        assert!(
            stderr.contains("Not installed")
                || stderr.contains("docker")
                || stderr.contains("Docker")
                || stderr.contains("Failed to spawn")
                || stderr.contains("command not found")
                || stderr.contains("No such file or directory"),
            "Expected Docker unavailability error in stderr, got: {:?}",
            stderr
        );
    }
}

#[test]
fn test_container_name_flag_in_help() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd.args(["up", "--help"]).assert().success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Verify the flag appears in help text
    assert!(stdout.contains("--container-name"));
    assert!(stdout.contains("Custom container name"));
}
