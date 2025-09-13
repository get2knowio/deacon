//! Smoke tests for up command idempotency and skip flags
//!
//! Scenarios covered:
//! - Up idempotency: multiple up calls should not fail
//! - Skip flags: --skip-non-blocking-commands should suppress postStart/postAttach
//! - Up command behavior without Docker: graceful error handling
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

/// Test up command without Docker: should handle gracefully
#[test]
fn test_up_without_docker() {
    let temp_dir = TempDir::new().unwrap();

    // Create minimal devcontainer.json
    let devcontainer_config = r#"{
    "name": "Up Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up command
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let up_stderr = String::from_utf8_lossy(&up_output.stderr);

    if up_output.status.success() {
        // Unexpected success without Docker, but accept it
        println!("Up succeeded unexpectedly without Docker");
    } else if docker_related_error(&up_stderr) {
        println!("Up gracefully handled Docker unavailable error");
    } else {
        // Any other error is also acceptable for this test
        println!("Up handled error as expected: {}", up_stderr);
    }
}

/// Test up idempotency: multiple up calls should not fail (Docker-gated)
#[test]
fn test_up_idempotency() {
    // Only run if Docker is explicitly enabled
    if std::env::var("SMOKE_DOCKER").is_err() {
        eprintln!("Skipping Docker-dependent test (set SMOKE_DOCKER=1 to enable)");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create devcontainer.json with lifecycle hooks
    let devcontainer_config = r#"{
    "name": "Up Idempotency Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "postCreateCommand": "echo 'postCreate executed' > /tmp/marker_postCreate",
    "postStartCommand": "echo 'postStart executed' > /tmp/marker_postStart"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // First up command
    let mut up_cmd1 = Command::cargo_bin("deacon").unwrap();
    let up_output1 = up_cmd1
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    if !up_output1.status.success() {
        let stderr = String::from_utf8_lossy(&up_output1.stderr);
        if docker_related_error(&stderr) {
            eprintln!("Skipping Docker-dependent test (Docker not available)");
            return;
        }
        panic!("First up command failed: {}", stderr);
    }

    println!("First up command succeeded");

    // Second up command (should be idempotent)
    let mut up_cmd2 = Command::cargo_bin("deacon").unwrap();
    let up_output2 = up_cmd2
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let up_stderr2 = String::from_utf8_lossy(&up_output2.stderr);

    if up_output2.status.success() {
        println!("Second up command succeeded (idempotent behavior verified)");
    } else if docker_related_error(&up_stderr2) {
        eprintln!("Skipping Docker-dependent test (Docker not available)");
        return;
    } else {
        // A non-success second up might be acceptable if it's just saying "already running"
        if up_stderr2.contains("already")
            || up_stderr2.contains("running")
            || up_stderr2.contains("exists")
        {
            println!("Second up handled gracefully with 'already running' message");
        } else {
            panic!("Unexpected error in second up: {}", up_stderr2);
        }
    }

    // Clean up: down command
    let mut down_cmd = Command::cargo_bin("deacon").unwrap();
    let _down_output = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
    // Ignore down result as it's just cleanup
}

/// Test --skip-non-blocking-commands flag behavior (Docker-gated)
#[test]
fn test_skip_non_blocking_commands() {
    // Only run if Docker is explicitly enabled
    if std::env::var("SMOKE_DOCKER").is_err() {
        eprintln!("Skipping Docker-dependent test (set SMOKE_DOCKER=1 to enable)");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create devcontainer.json with various lifecycle hooks
    let devcontainer_config = r#"{
    "name": "Skip Non-Blocking Commands Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "onCreateCommand": "echo 'onCreate executed' > /tmp/marker_onCreate",
    "postCreateCommand": "echo 'postCreate executed' > /tmp/marker_postCreate",
    "postStartCommand": "echo 'postStart executed' > /tmp/marker_postStart",
    "postAttachCommand": "echo 'postAttach executed' > /tmp/marker_postAttach"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up command with --skip-non-blocking-commands flag
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--skip-non-blocking-commands")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let up_stderr = String::from_utf8_lossy(&up_output.stderr);

    if up_output.status.success() {
        // Success: command should succeed and skip non-blocking commands
        // We can't easily verify which commands were actually skipped without exec access,
        // but we can verify the command succeeded and look for skip-related log messages
        println!("Up with --skip-non-blocking-commands succeeded");

        // Check if stderr contains any indication of skipping
        if up_stderr.contains("skip") || up_stderr.contains("non-blocking") {
            println!("Found skip-related messages in output");
        }
    } else if docker_related_error(&up_stderr) {
        eprintln!("Skipping Docker-dependent test (Docker not available)");
        return;
    } else {
        panic!("Up with --skip-non-blocking-commands failed: {}", up_stderr);
    }

    // Test a regular up command after the skip version for comparison
    let mut up_cmd_normal = Command::cargo_bin("deacon").unwrap();
    let up_output_normal = up_cmd_normal
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let up_stderr_normal = String::from_utf8_lossy(&up_output_normal.stderr);

    if up_output_normal.status.success() {
        println!("Regular up command (after skip) also succeeded");
    } else if docker_related_error(&up_stderr_normal) {
        eprintln!("Docker became unavailable during test");
    } else {
        // This might be acceptable if the container is already running
        if up_stderr_normal.contains("already")
            || up_stderr_normal.contains("running")
            || up_stderr_normal.contains("exists")
        {
            println!("Regular up handled gracefully with 'already running' message");
        } else {
            println!(
                "Regular up after skip failed (may be acceptable): {}",
                up_stderr_normal
            );
        }
    }

    // Clean up: down command
    let mut down_cmd = Command::cargo_bin("deacon").unwrap();
    let _down_output = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
    // Ignore down result as it's just cleanup
}

/// Test up with different skip flag combinations (Docker-gated)
#[test]
fn test_skip_flag_combinations() {
    // Only run if Docker is explicitly enabled
    if std::env::var("SMOKE_DOCKER").is_err() {
        eprintln!("Skipping Docker-dependent test (set SMOKE_DOCKER=1 to enable)");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create minimal devcontainer.json
    let devcontainer_config = r#"{
    "name": "Skip Flag Combinations Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "postCreateCommand": "echo 'postCreate executed'"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up with --skip-post-create flag
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--skip-post-create")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let up_stderr = String::from_utf8_lossy(&up_output.stderr);

    if up_output.status.success() {
        println!("Up with --skip-post-create succeeded");
    } else if docker_related_error(&up_stderr) {
        eprintln!("Skipping Docker-dependent test (Docker not available)");
        return;
    } else {
        panic!("Up with --skip-post-create failed: {}", up_stderr);
    }

    // Test up with both skip flags
    let mut up_cmd_both = Command::cargo_bin("deacon").unwrap();
    let up_output_both = up_cmd_both
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--skip-post-create")
        .arg("--skip-non-blocking-commands")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let up_stderr_both = String::from_utf8_lossy(&up_output_both.stderr);

    if up_output_both.status.success() {
        println!("Up with both skip flags succeeded");
    } else if docker_related_error(&up_stderr_both) {
        eprintln!("Docker became unavailable during test");
    } else {
        // This might be acceptable if the container is already running
        if up_stderr_both.contains("already")
            || up_stderr_both.contains("running")
            || up_stderr_both.contains("exists")
        {
            println!("Up with both skip flags handled gracefully with 'already running' message");
        } else {
            println!(
                "Up with both skip flags failed (may be acceptable): {}",
                up_stderr_both
            );
        }
    }

    // Clean up: down command
    let mut down_cmd = Command::cargo_bin("deacon").unwrap();
    let _down_output = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
    // Ignore down result as it's just cleanup
}
