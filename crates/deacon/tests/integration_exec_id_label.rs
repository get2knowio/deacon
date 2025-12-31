//! Docker-backed tests for `deacon exec --id-label` matching behavior.

use assert_cmd::Command;
use testcontainers::runners::AsyncRunner;

mod support;
mod testcontainers_helpers;
use support::unique_name;
use testcontainers_helpers::alpine_sleep_with_labels;

#[tokio::test]
async fn test_exec_id_label_successful_unique_match() {
    // Test successful execution with a uniquely matched container
    // Use a unique label value to ensure we match only our container
    let unique_value = unique_name("unique-match");

    // Create a test container with unique labels using testcontainers
    let labels = &[
        ("com.example.test", unique_value.as_str()),
        ("com.example.role", "test-container"),
    ];

    let container = alpine_sleep_with_labels(labels).start().await.unwrap();

    // Execute a command in the container using id-label
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let result = cmd
        .arg("exec")
        .arg("--id-label")
        .arg(format!("com.example.test={}", unique_value))
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

    // Container automatically cleaned up when dropped
    drop(container);
}

#[tokio::test]
async fn test_exec_id_label_ambiguous_match_lists_candidates() {
    // Test that when multiple containers match, we use the first one (deterministic behavior)
    // Use a unique label value to ensure we match only our containers
    let unique_value = unique_name("ambiguous-match");

    // Create two test containers with the same label using testcontainers
    let labels = &[("com.example.test", unique_value.as_str())];

    let container1 = alpine_sleep_with_labels(labels).start().await.unwrap();
    let container2 = alpine_sleep_with_labels(labels).start().await.unwrap();

    // Try to execute a command - should succeed using the first container
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let result = cmd
        .arg("exec")
        .arg("--id-label")
        .arg(format!("com.example.test={}", unique_value))
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

    // Containers automatically cleaned up when dropped
    drop(container1);
    drop(container2);
}
