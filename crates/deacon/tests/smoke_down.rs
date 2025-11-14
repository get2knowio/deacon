//! Smoke tests for down command behavior
//!
//! Scenarios covered:
//! - Down command before any up: should succeed or gracefully handle "no container"
//! - Down command after up: should successfully tear down (Docker-gated)
//! - Idempotent down behavior: subsequent down calls should not error
//!
//! NOTE: These tests assume Docker is available and running. They will fail
//! if Docker is not present or cannot start containers.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

fn is_docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Test down command before any up: should succeed or gracefully handle "no container"
#[test]
fn test_down_before_up() {
    let temp_dir = TempDir::new().unwrap();

    // Create minimal devcontainer.json
    let devcontainer_config = r#"{
    "name": "Down Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test down command before any up
    let mut down_cmd = Command::cargo_bin("deacon").unwrap();
    let down_output = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let down_stderr = String::from_utf8_lossy(&down_output.stderr);
    // Expected: should succeed when no container to tear down
    // If CLI chooses to report "no container" as non-zero, allow known message
    if !down_output.status.success() {
        assert!(
            down_stderr.contains("No running containers")
                || down_stderr.contains("no container")
                || down_stderr.contains("not found"),
            "Down before up failed unexpectedly: {}",
            down_stderr
        );
    }
}

/// Test down command after up and idempotent behavior (Docker-gated)
#[test]
fn test_down_after_up_idempotent() {
    if !is_docker_available() {
        eprintln!("Skipping test_down_after_up_idempotent: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create minimal devcontainer.json
    let devcontainer_config = r#"{
    "name": "Down After Up Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // First: up command
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--skip-post-create")
        .arg("--skip-non-blocking-commands")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    assert!(
        up_output.status.success(),
        "Up command failed: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );

    // Second: down command (should succeed)
    let mut down_cmd = Command::cargo_bin("deacon").unwrap();
    let down_output = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let down_stderr = String::from_utf8_lossy(&down_output.stderr);

    assert!(
        down_output.status.success(),
        "Down command failed after up: {}",
        down_stderr
    );

    println!("Down after up succeeded");

    // Third: down command again (should be idempotent, not error)
    let mut down_cmd2 = Command::cargo_bin("deacon").unwrap();
    let down_output2 = down_cmd2
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let down_stderr2 = String::from_utf8_lossy(&down_output2.stderr);

    if !down_output2.status.success() {
        assert!(
            down_stderr2.contains("No running containers")
                || down_stderr2.contains("no container")
                || down_stderr2.contains("not found"),
            "Unexpected error in second down: {}",
            down_stderr2
        );
    }
}
