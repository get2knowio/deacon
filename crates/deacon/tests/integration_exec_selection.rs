//! Integration tests for exec selection methods (ID, label, workspace)
//!
//! Tests: T008, T009, T010, T036

use assert_cmd::Command;
use predicates::prelude::*;

use tempfile::TempDir;

#[test]
fn test_exec_with_container_id_selection() {
    // T008: Integration: direct ID selection
    // This test requires Docker; skip if not available.
    use std::process::Command as StdCommand;
    let docker_available = StdCommand::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !docker_available {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    // Create container
    let output = StdCommand::new("docker")
        .arg("run")
        .arg("-d")
        .arg("--rm")
        .arg("--name")
        .arg("deacon-test-exec-id")
        .arg("alpine:3.19")
        .arg("sh")
        .arg("-c")
        .arg("sleep 3600")
        .output()
        .expect("docker run failed");

    if !output.status.success() {
        eprintln!(
            "Failed to create test container: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    std::thread::sleep(std::time::Duration::from_secs(1));

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("exec")
        .arg("--container-id")
        .arg(&container_id)
        .arg("--")
        .arg("echo")
        .arg("hello")
        .assert()
        .success()
        .stdout(predicate::str::contains("hello"));

    // Cleanup
    let _ = StdCommand::new("docker")
        .arg("rm")
        .arg("-f")
        .arg(&container_id)
        .output();
}

#[test]
fn test_exec_with_id_label_selection_requires_validation() {
    // T009: label selection and validation
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("exec")
        .arg("--id-label")
        .arg("INVALID_FORMAT")
        .arg("echo")
        .arg("test")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "Unmatched argument format: id-label must match <name>=<value>.",
        ));
}

#[test]
fn test_exec_with_workspace_discovery_missing_config_error() {
    // T010 & T036: workspace discovery selection when no config should return exact message
    let temp_dir = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("echo")
        .arg("test")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("Dev container config ("));
}
