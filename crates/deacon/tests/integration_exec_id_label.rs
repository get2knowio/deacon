//! Integration tests for exec command with --id-label flag
//!
//! These tests verify that the exec command properly resolves containers
//! based on custom id-labels.

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use std::process::{Command as StdCommand, Stdio};
use tempfile::TempDir;

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

/// Helper to create a test container with specific labels
fn create_test_container(name: &str, labels: &[(&str, &str)]) -> Result<String, String> {
    let mut cmd = StdCommand::new("docker");
    cmd.arg("run").arg("-d").arg("--name").arg(name).arg("--rm");

    // Add labels
    for (key, value) in labels {
        cmd.arg("--label").arg(format!("{}={}", key, value));
    }

    // Use alpine and keep it running
    cmd.arg("alpine:3.19").arg("sh").arg("-c").arg("sleep 3600");

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run docker: {}", e))?;

    if output.status.success() {
        let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(container_id)
    } else {
        Err(format!(
            "Failed to create container: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Helper to stop and remove a container
fn cleanup_container(name: &str) {
    let _ = StdCommand::new("docker")
        .arg("rm")
        .arg("-f")
        .arg(name)
        .output();
}

#[test]
fn test_exec_id_label_with_invalid_format() {
    // Test that exec validates id-label format (must have = sign)
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("exec")
        .arg("--id-label")
        .arg("INVALID_FORMAT") // Missing = sign
        .arg("echo")
        .arg("test")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("Invalid id-label format"));
}

#[test]
fn test_exec_id_label_with_no_matching_containers() {
    // Test that exec fails when no containers match the labels
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("exec")
        .arg("--id-label")
        .arg("com.example.role=nonexistent")
        .arg("echo")
        .arg("test")
        .assert()
        .failure()
        .code(1)
        .stderr(
            predicate::str::contains("No running container found matching labels")
                .or(predicate::str::contains("Failed to spawn docker"))
                .or(predicate::str::contains("Docker CLI error"))
                .or(predicate::str::contains(
                    "Docker is not installed or not accessible",
                )),
        );
}

#[test]
fn test_exec_id_label_multiple_labels() {
    // Test that exec can accept multiple id-label flags
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("exec")
        .arg("--id-label")
        .arg("com.example.role=api")
        .arg("--id-label")
        .arg("com.example.env=prod")
        .arg("echo")
        .arg("test")
        .assert()
        .failure()
        .code(1)
        .stderr(
            predicate::str::contains("No running container found matching labels")
                .or(predicate::str::contains("Failed to spawn docker"))
                .or(predicate::str::contains("Docker CLI error"))
                .or(predicate::str::contains(
                    "Docker is not installed or not accessible",
                )),
        );
}

#[test]
fn test_exec_id_label_without_config() {
    // Test that exec with --id-label doesn't require a devcontainer.json
    let temp_dir = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(&temp_dir)
        .arg("exec")
        .arg("--id-label")
        .arg("com.example.app=myapp")
        .arg("echo")
        .arg("test")
        .assert()
        .failure()
        .code(1)
        .stderr(
            predicate::str::contains("No running container found matching labels")
                .or(predicate::str::contains("Failed to spawn docker"))
                .or(predicate::str::contains("Docker CLI error"))
                .or(predicate::str::contains(
                    "Docker is not installed or not accessible",
                )),
        );
}

#[test]
fn test_exec_id_label_with_workdir() {
    // Test that exec properly combines --id-label with --workdir
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("exec")
        .arg("--id-label")
        .arg("com.example.service=web")
        .arg("--workdir")
        .arg("/app")
        .arg("pwd")
        .assert()
        .failure()
        .code(1)
        .stderr(
            predicate::str::contains("No running container found matching labels")
                .or(predicate::str::contains("Failed to spawn docker"))
                .or(predicate::str::contains("Docker CLI error"))
                .or(predicate::str::contains(
                    "Docker is not installed or not accessible",
                )),
        );
}

#[test]
fn test_exec_id_label_with_env() {
    // Test that exec properly combines --id-label with --env
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("exec")
        .arg("--id-label")
        .arg("com.example.tier=frontend")
        .arg("--env")
        .arg("TEST_VAR=value")
        .arg("echo")
        .arg("test")
        .assert()
        .failure()
        .code(1)
        .stderr(
            predicate::str::contains("No running container found matching labels")
                .or(predicate::str::contains("Failed to spawn docker"))
                .or(predicate::str::contains("Docker CLI error"))
                .or(predicate::str::contains(
                    "Docker is not installed or not accessible",
                )),
        );
}

#[test]
#[serial]
fn test_exec_id_label_successful_unique_match() {
    // Test successful execution with a uniquely matched container
    if !is_docker_available() {
        eprintln!("Skipping test: Docker is not available");
        return;
    }

    let container_name = "deacon-test-unique-match";

    // Create a test container with unique labels
    let labels = &[
        ("com.example.test", "unique-match"),
        ("com.example.role", "test-container"),
    ];

    match create_test_container(container_name, labels) {
        Ok(_container_id) => {
            // Give container a moment to start
            std::thread::sleep(std::time::Duration::from_secs(1));

            // Execute a command in the container using id-label
            let mut cmd = Command::cargo_bin("deacon").unwrap();
            let result = cmd
                .arg("exec")
                .arg("--id-label")
                .arg("com.example.test=unique-match")
                .arg("--")
                .arg("echo")
                .arg("success")
                .assert()
                .success()
                .code(0);

            // Verify output contains expected text
            let output = result.get_output();
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                stdout.contains("success"),
                "Expected 'success' in output, got: {}",
                stdout
            );

            // Cleanup
            cleanup_container(container_name);
        }
        Err(e) => {
            cleanup_container(container_name);
            panic!("Failed to create test container: {}", e);
        }
    }
}

#[test]
#[serial]
fn test_exec_id_label_ambiguous_match_lists_candidates() {
    // Test that ambiguous matches list container IDs and names
    if !is_docker_available() {
        eprintln!("Skipping test: Docker is not available");
        return;
    }

    let container1_name = "deacon-test-ambiguous-1";
    let container2_name = "deacon-test-ambiguous-2";

    // Create two test containers with the same label
    let labels = &[("com.example.test", "ambiguous-match")];

    let container1_result = create_test_container(container1_name, labels);
    let container2_result = create_test_container(container2_name, labels);

    match (container1_result, container2_result) {
        (Ok(id1), Ok(id2)) => {
            // Give containers a moment to start
            std::thread::sleep(std::time::Duration::from_secs(1));

            // Try to execute a command - should fail with ambiguous match
            let mut cmd = Command::cargo_bin("deacon").unwrap();
            let result = cmd
                .arg("exec")
                .arg("--id-label")
                .arg("com.example.test=ambiguous-match")
                .arg("--")
                .arg("echo")
                .arg("test")
                .assert()
                .failure()
                .code(1);

            // Verify error message contains key information
            let output = result.get_output();
            let stderr = String::from_utf8_lossy(&output.stderr);

            // Check for key error message components
            assert!(
                stderr.contains("Found 2 running containers matching labels"),
                "Expected ambiguous match error, got: {}",
                stderr
            );
            assert!(
                stderr.contains("Please refine your label selector"),
                "Expected refinement suggestion, got: {}",
                stderr
            );

            // Verify that both container IDs are listed in the error
            assert!(
                stderr.contains(&id1[..12]) || stderr.contains(&id1),
                "Expected container ID {} in error, got: {}",
                id1,
                stderr
            );
            assert!(
                stderr.contains(&id2[..12]) || stderr.contains(&id2),
                "Expected container ID {} in error, got: {}",
                id2,
                stderr
            );

            // Verify that container names are listed
            assert!(
                stderr.contains(container1_name),
                "Expected container name {} in error, got: {}",
                container1_name,
                stderr
            );
            assert!(
                stderr.contains(container2_name),
                "Expected container name {} in error, got: {}",
                container2_name,
                stderr
            );

            // Cleanup
            cleanup_container(container1_name);
            cleanup_container(container2_name);
        }
        _ => {
            cleanup_container(container1_name);
            cleanup_container(container2_name);
            panic!("Failed to create test containers");
        }
    }
}
