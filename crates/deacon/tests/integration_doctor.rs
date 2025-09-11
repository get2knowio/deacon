//! Integration tests for the doctor command

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_doctor_command_basic() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("doctor");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Deacon Doctor Diagnostics"))
        .stdout(predicate::str::contains("CLI Version:"))
        .stdout(predicate::str::contains("Host OS:"))
        .stdout(predicate::str::contains("Docker:"));
}

#[test]
fn test_doctor_command_json() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("doctor").arg("--json");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"cli_version\""))
        .stdout(predicate::str::contains("\"host_os\""))
        .stdout(predicate::str::contains("\"docker_info\""));
}

#[test]
fn test_doctor_command_bundle_creation() {
    let temp_dir = TempDir::new().unwrap();
    let bundle_path = temp_dir.path().join("test-bundle.zip");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("doctor").arg("--bundle").arg(&bundle_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Support bundle created"));

    // Verify bundle was created
    assert!(bundle_path.exists());

    // Verify it's a valid zip file
    let bundle_content = fs::read(&bundle_path).unwrap();
    assert!(!bundle_content.is_empty());
    assert_eq!(&bundle_content[0..2], b"PK"); // ZIP file signature
}

#[test]
fn test_doctor_command_exits_successfully() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("doctor");

    cmd.assert().success().code(0);
}
