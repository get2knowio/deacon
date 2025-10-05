//! Integration tests for the down command

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// Test that down command works with basic arguments
#[test]
fn test_down_command_basic() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();

    // Create a temp directory for testing
    let temp_dir = TempDir::new().unwrap();

    let assert = cmd
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        // .env("DEACON_LOG", "info") // Removed unnecessary env override
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.contains("No running containers or compose projects found for workspace")
            || stderr.contains("No running containers or compose projects found for workspace"),
        "Unexpected stdout, failed var.contains(No running containers or compose projects found for workspace)\n--- stdout ---\n{}\n--- stderr ---\n{}",
        stdout,
        stderr
    );
}

/// Test that down command accepts remove flag
#[test]
fn test_down_command_with_remove() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();

    // Create a temp directory for testing
    let temp_dir = TempDir::new().unwrap();

    let assert = cmd
        .arg("down")
        .arg("--remove")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        // .env("DEACON_LOG", "info") // Removed unnecessary env override
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.contains("No running containers or compose projects found for workspace")
            || stderr.contains("No running containers or compose projects found for workspace"),
        "Unexpected stdout, failed var.contains(No running containers or compose projects found for workspace)\n--- stdout ---\n{}\n--- stderr ---\n{}",
        stdout,
        stderr
    );
}

/// Test that down command shows help when run with --help
#[test]
fn test_down_command_help() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();

    cmd.arg("down")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Stop and optionally remove development container or compose project",
        ));
}

/// Test that up command accepts shutdown flag
#[test]
fn test_up_command_with_shutdown() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();

    // Create a temp directory without devcontainer.json (will fail but should accept the flag)
    let temp_dir = TempDir::new().unwrap();

    cmd.arg("up")
        .arg("--shutdown")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .assert()
        .failure() // Will fail because no devcontainer.json
        .stderr(predicate::str::contains(
            "No devcontainer.json found in workspace",
        ));
}

/// Test that down command accepts --all flag
#[test]
fn test_down_command_with_all() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();

    let temp_dir = TempDir::new().unwrap();

    let assert = cmd
        .arg("down")
        .arg("--all")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.contains("No running containers or compose projects found for workspace")
            || stderr.contains("No running containers or compose projects found for workspace"),
        "Unexpected output\n--- stdout ---\n{}\n--- stderr ---\n{}",
        stdout,
        stderr
    );
}

/// Test that down command accepts --volumes flag
#[test]
fn test_down_command_with_volumes() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();

    let temp_dir = TempDir::new().unwrap();

    let assert = cmd
        .arg("down")
        .arg("--volumes")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.contains("No running containers or compose projects found for workspace")
            || stderr.contains("No running containers or compose projects found for workspace"),
        "Unexpected output\n--- stdout ---\n{}\n--- stderr ---\n{}",
        stdout,
        stderr
    );
}

/// Test that down command accepts --force flag
#[test]
fn test_down_command_with_force() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();

    let temp_dir = TempDir::new().unwrap();

    let assert = cmd
        .arg("down")
        .arg("--force")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.contains("No running containers or compose projects found for workspace")
            || stderr.contains("No running containers or compose projects found for workspace"),
        "Unexpected output\n--- stdout ---\n{}\n--- stderr ---\n{}",
        stdout,
        stderr
    );
}

/// Test that down command accepts --timeout flag
#[test]
fn test_down_command_with_timeout() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();

    let temp_dir = TempDir::new().unwrap();

    let assert = cmd
        .arg("down")
        .arg("--timeout")
        .arg("60")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.contains("No running containers or compose projects found for workspace")
            || stderr.contains("No running containers or compose projects found for workspace"),
        "Unexpected output\n--- stdout ---\n{}\n--- stderr ---\n{}",
        stdout,
        stderr
    );
}

/// Test that down command accepts combined flags
#[test]
fn test_down_command_with_combined_flags() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();

    let temp_dir = TempDir::new().unwrap();

    let assert = cmd
        .arg("down")
        .arg("--remove")
        .arg("--volumes")
        .arg("--force")
        .arg("--timeout")
        .arg("45")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.contains("No running containers or compose projects found for workspace")
            || stderr.contains("No running containers or compose projects found for workspace"),
        "Unexpected output\n--- stdout ---\n{}\n--- stderr ---\n{}",
        stdout,
        stderr
    );
}
