//! Integration tests for exec selection methods (ID, label, workspace)
//!
//! Tests: T008, T009, T010, T036

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;
use testcontainers::runners::AsyncRunner;

mod testcontainers_helpers;
use testcontainers_helpers::{alpine_sleep_image, container_id};

#[tokio::test]
async fn test_exec_with_container_id_selection() {
    // T008: Integration: direct ID selection
    // Start a container using testcontainers (auto-cleanup on drop)
    let container = alpine_sleep_image().start().await.unwrap();
    let id = container_id(&container);

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("exec")
        .arg("--container-id")
        .arg(&id)
        .arg("--")
        .arg("echo")
        .arg("hello")
        .assert()
        .success()
        .stdout(predicate::str::contains("hello"));

    // Container automatically cleaned up when dropped
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
