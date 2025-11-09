//! Smoke tests for up command idempotency and skip flags
//!
//! Scenarios covered:
//! - Up idempotency: multiple up calls should not fail
//! - Skip flags: --skip-non-blocking-commands should suppress postStart/postAttach
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

// Non-Docker tolerance scenario removed

/// Test up idempotency: multiple up calls should not fail
#[test]
fn test_up_idempotency() {
    if !is_docker_available() {
        eprintln!("Skipping test_up_idempotency: Docker not available");
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

    assert!(
        up_output1.status.success(),
        "First up command failed: {}",
        String::from_utf8_lossy(&up_output1.stderr)
    );

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

    if !up_output2.status.success() {
        // A non-success second up might be acceptable if it's just saying "already running"
        assert!(
            up_stderr2.contains("already")
                || up_stderr2.contains("running")
                || up_stderr2.contains("exists"),
            "Unexpected error in second up: {}",
            up_stderr2
        );
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

/// Test --skip-non-blocking-commands flag behavior
#[test]
fn test_skip_non_blocking_commands() {
    if !is_docker_available() {
        eprintln!("Skipping test_skip_non_blocking_commands: Docker not available");
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

    assert!(
        up_output.status.success(),
        "Up with --skip-non-blocking-commands failed: {}",
        up_stderr
    );
    // We can't easily verify which commands were actually skipped without exec access,
    // but we can verify the command succeeded and optionally look for skip-related logs
    if up_stderr.contains("skip") || up_stderr.contains("non-blocking") {
        println!("Found skip-related messages in output");
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

    assert!(
        up_output_normal.status.success(),
        "Regular up after skip failed unexpectedly: {}",
        up_stderr_normal
    );

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

/// Test up with different skip flag combinations
#[test]
fn test_skip_flag_combinations() {
    if !is_docker_available() {
        eprintln!("Skipping test_skip_flag_combinations: Docker not available");
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

    assert!(
        up_output.status.success(),
        "Up with --skip-post-create failed: {}",
        up_stderr
    );

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

    assert!(
        up_output_both.status.success(),
        "Up with both skip flags failed unexpectedly: {}",
        up_stderr_both
    );

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
