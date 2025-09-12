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

    cmd.arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "No running containers or compose projects found for workspace",
        ));
}

/// Test that down command accepts remove flag
#[test]
fn test_down_command_with_remove() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();

    // Create a temp directory for testing
    let temp_dir = TempDir::new().unwrap();

    cmd.arg("down")
        .arg("--remove")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "No running containers or compose projects found for workspace",
        ));
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
