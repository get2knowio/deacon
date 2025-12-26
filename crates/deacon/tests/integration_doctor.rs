#![cfg(feature = "full")]
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
    // Explicitly enable info logging so the support bundle log message is emitted
    cmd.arg("--log-level")
        .arg("info")
        .arg("doctor")
        .arg("--bundle")
        .arg(&bundle_path);

    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.contains("Support bundle created")
            || stderr.contains("Support bundle created"),
        "Unexpected stdout, failed var.contains(Support bundle created)\n--- stdout ---\n{}\n--- stderr ---\n{}",
        stdout,
        stderr
    );

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

#[test]
fn test_doctor_bundle_contains_enhanced_details() {
    let temp_dir = TempDir::new().unwrap();
    let bundle_path = temp_dir.path().join("enhanced-bundle.zip");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("--log-level")
        .arg("info")
        .arg("doctor")
        .arg("--bundle")
        .arg(&bundle_path);

    cmd.assert().success();

    // Verify bundle was created
    assert!(bundle_path.exists());

    // Verify it's a valid zip and contains expected files
    let bundle_content = fs::read(&bundle_path).unwrap();
    assert!(!bundle_content.is_empty());
    assert_eq!(&bundle_content[0..2], b"PK"); // ZIP file signature

    // Use zip crate to verify contents
    let file = fs::File::open(&bundle_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();

    // Check that new files exist in the bundle
    let file_names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_string())
        .collect();

    assert!(
        file_names.contains(&"doctor.json".to_string()),
        "Bundle should contain doctor.json"
    );
    assert!(
        file_names.contains(&"environment.json".to_string()),
        "Bundle should contain environment.json"
    );
    assert!(
        file_names.contains(&"runtime-config.json".to_string()),
        "Bundle should contain runtime-config.json"
    );
    assert!(
        file_names.contains(&"resources.json".to_string()),
        "Bundle should contain resources.json"
    );

    // Verify environment.json contains expected structure
    let mut env_file = archive.by_name("environment.json").unwrap();
    let mut env_content = String::new();
    std::io::Read::read_to_string(&mut env_file, &mut env_content).unwrap();

    // Parse JSON to ensure it's valid
    let env_json: serde_json::Value = serde_json::from_str(&env_content).unwrap();
    assert!(env_json.get("variables").is_some());
    assert!(env_json.get("shell").is_some());
    assert!(env_json.get("home").is_some());
}
