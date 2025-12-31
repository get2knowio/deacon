//! CLI-focused tests for `deacon exec --id-label` that don't require Docker.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn test_exec_id_label_with_invalid_format() {
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
fn test_exec_id_label_with_no_matching_containers() {
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
