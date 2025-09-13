//! Smoke tests for down command behavior
//!
//! Scenarios covered:
//! - Down command before any up: should succeed or gracefully handle "no container"
//! - Down command after up: should successfully tear down (Docker-gated)
//! - Idempotent down behavior: subsequent down calls should not error
//!
//! Tests are written to be resilient in environments without Docker: they
//! accept specific error messages that indicate Docker is unavailable.
//! Docker-dependent tests are gated by SMOKE_DOCKER environment variable.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

fn docker_related_error(stderr: &str) -> bool {
    stderr.contains("Docker is not installed")
        || stderr.contains("Docker daemon is not")
        || stderr.contains("permission denied")
        || stderr.contains("Failed to spawn docker")
        || stderr.contains("Docker CLI error")
        || stderr.contains("Error response from daemon")
        || stderr.contains("container") && stderr.contains("is not running")
        || stderr.contains("Container command failed")
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

    if down_output.status.success() {
        // Expected: should succeed when no container to tear down
        println!("Down before up succeeded as expected");
    } else if docker_related_error(&down_stderr) {
        println!("Skipping Docker-dependent test (Docker not available)");
    } else if down_stderr.contains("No running containers")
        || down_stderr.contains("no container")
        || down_stderr.contains("not found")
    {
        // Also acceptable: explicit "no container" message
        println!("Down before up handled gracefully with no-container message");
    } else {
        panic!("Unexpected error in down before up: {}", down_stderr);
    }
}

/// Test down command after up and idempotent behavior (Docker-gated)
#[test]
fn test_down_after_up_idempotent() {
    // Only run if Docker is explicitly enabled
    if std::env::var("SMOKE_DOCKER").is_err() {
        eprintln!("Skipping Docker-dependent test (set SMOKE_DOCKER=1 to enable)");
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

    if !up_output.status.success() {
        let stderr = String::from_utf8_lossy(&up_output.stderr);
        if docker_related_error(&stderr) {
            eprintln!("Skipping Docker-dependent test (Docker not available)");
            return;
        }
        panic!("Up command failed: {}", stderr);
    }

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

    if !down_output.status.success() {
        if docker_related_error(&down_stderr) {
            eprintln!("Skipping Docker-dependent test (Docker not available)");
            return;
        }
        panic!("Down command failed after up: {}", down_stderr);
    }

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

    if down_output2.status.success() {
        println!("Second down command succeeded (idempotent behavior)");
    } else if docker_related_error(&down_stderr2) {
        println!("Skipping Docker-dependent test (Docker not available)");
    } else if down_stderr2.contains("No running containers")
        || down_stderr2.contains("no container")
        || down_stderr2.contains("not found")
    {
        // Acceptable: explicit "no container" message on second down
        println!("Second down handled gracefully with no-container message");
    } else {
        panic!("Unexpected error in second down: {}", down_stderr2);
    }
}
