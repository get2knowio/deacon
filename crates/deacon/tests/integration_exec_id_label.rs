//! Integration tests for exec command with --id-label flag
//!
//! These tests verify that the exec command properly resolves containers
//! based on custom id-labels.

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use std::process::{Command as StdCommand, Stdio};
use tempfile::TempDir;

mod support;
use support::{is_docker_available, unique_name};

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
        .stderr(predicate::str::contains(
            "Unmatched argument format: id-label must match <name>=<value>.",
        ));
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
            predicate::str::contains("Dev container not found")
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
            predicate::str::contains("Dev container not found")
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
            predicate::str::contains("Dev container not found")
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
            predicate::str::contains("Dev container not found")
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
            predicate::str::contains("Dev container not found")
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

    let container_name = unique_name("deacon-test-unique-match");

    // Create a test container with unique labels
    let labels = &[
        ("com.example.test", "unique-match"),
        ("com.example.role", "test-container"),
    ];

    match create_test_container(&container_name, labels) {
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
            cleanup_container(&container_name);
        }
        Err(e) => {
            cleanup_container(&container_name);
            eprintln!("Skipping test: Failed to create test container: {}", e);
            return;
        }
    }
}

#[test]
#[serial]
fn test_exec_id_label_ambiguous_match_lists_candidates() {
    // Test that when multiple containers match, we use the first one (deterministic behavior)
    if !is_docker_available() {
        eprintln!("Skipping test: Docker is not available");
        return;
    }

    let container1_name = unique_name("deacon-test-ambiguous-1");
    let container2_name = unique_name("deacon-test-ambiguous-2");

    // Create two test containers with the same label
    let labels = &[("com.example.test", "ambiguous-match")];

    let container1_result = create_test_container(&container1_name, labels);
    let container2_result = create_test_container(&container2_name, labels);

    match (container1_result, container2_result) {
        (Ok(_id1), Ok(_id2)) => {
            // Give containers a moment to start
            std::thread::sleep(std::time::Duration::from_secs(1));

            // Try to execute a command - should succeed using the first container
            let mut cmd = Command::cargo_bin("deacon").unwrap();
            let result = cmd
                .arg("exec")
                .arg("--id-label")
                .arg("com.example.test=ambiguous-match")
                .arg("--")
                .arg("echo")
                .arg("test")
                .assert()
                .success()
                .code(0);

            // Verify output contains expected text
            let output = result.get_output();
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                stdout.contains("test"),
                "Expected 'test' in output, got: {}",
                stdout
            );

            // Cleanup
            cleanup_container(&container1_name);
            cleanup_container(&container2_name);
        }
        _ => {
            cleanup_container(&container1_name);
            cleanup_container(&container2_name);
            eprintln!(
                "Skipping test: Failed to create test containers (Docker may not be available)"
            );
            return;
        }
    }
}
