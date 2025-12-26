#![cfg(feature = "full")]
//! Integration tests for additional features CLI functionality
//!
//! Tests the complete workflow of CLI feature injection and feature install order override.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_help_shows_additional_features_flags() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up").arg("--help");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Check that our new CLI flags are documented
    assert!(stdout.contains("--additional-features"));
    assert!(stdout.contains("--prefer-cli-features"));
    assert!(stdout.contains("--feature-install-order"));
}

#[test]
fn test_build_help_shows_additional_features_flags() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("build").arg("--help");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Check that our new CLI flags are documented for build command too
    assert!(stdout.contains("--additional-features"));
    assert!(stdout.contains("--prefer-cli-features"));
    assert!(stdout.contains("--feature-install-order"));
}

#[test]
fn test_invalid_additional_features_json() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();
    let devcontainer_path = devcontainer_dir.join("devcontainer.json");

    // Create a minimal devcontainer.json
    fs::write(&devcontainer_path, r#"{"image": "ubuntu:20.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--additional-features")
        .arg("invalid json");

    let output = cmd.output().unwrap();

    // Should fail due to invalid JSON
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    println!("Stderr output: {}", stderr); // Debug output
    assert!(
        stderr.contains("Failed to parse additional features JSON")
            || stderr.contains("parse")
            || stderr.contains("JSON")
    );
}

#[test]
fn test_invalid_feature_install_order() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();
    let devcontainer_path = devcontainer_dir.join("devcontainer.json");

    // Create a minimal devcontainer.json
    fs::write(
        &devcontainer_path,
        r#"{"image": "ubuntu:20.04", "features": {"git": true}}"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--feature-install-order")
        .arg("git,unknown,git"); // duplicate git

    let output = cmd.output().unwrap();

    // Should fail due to duplicate feature ID
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Duplicate feature ID"));
}

#[test]
fn test_additional_features_with_invalid_type() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();
    let devcontainer_path = devcontainer_dir.join("devcontainer.json");

    // Create a minimal devcontainer.json
    fs::write(&devcontainer_path, r#"{"image": "ubuntu:20.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--additional-features")
        .arg(r#"{"git": 123}"#); // number not allowed

    let output = cmd.output().unwrap();

    // Should fail due to invalid value type
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("has invalid value type"));
}

#[test]
fn test_additional_features_array_not_object() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();
    let devcontainer_path = devcontainer_dir.join("devcontainer.json");

    // Create a minimal devcontainer.json
    fs::write(&devcontainer_path, r#"{"image": "ubuntu:20.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--additional-features")
        .arg(r#"["git", "node"]"#); // array not allowed

    let output = cmd.output().unwrap();

    // Should fail because additional features must be an object
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("must be a JSON object"));
}

#[test]
fn test_empty_feature_install_order() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();
    let devcontainer_path = devcontainer_dir.join("devcontainer.json");

    // Create a minimal devcontainer.json
    fs::write(&devcontainer_path, r#"{"image": "ubuntu:20.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--feature-install-order")
        .arg(""); // empty string

    let output = cmd.output().unwrap();

    // Should fail due to empty install order
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Feature install order cannot be empty"));
}

#[test]
fn test_feature_merging_integration() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();
    let devcontainer_path = devcontainer_dir.join("devcontainer.json");

    // Create a devcontainer.json with existing features
    fs::write(
        &devcontainer_path,
        r#"{
        "image": "ubuntu:20.04",
        "features": {
            "git": true,
            "node": "18"
        },
        "overrideFeatureInstallOrder": ["git", "node"]
    }"#,
    )
    .unwrap();

    // Test 1: Additional features without conflicts (should succeed)
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--additional-features")
        .arg(r#"{"docker": true, "python": "3.9"}"#);

    let output = cmd.output().unwrap();

    // This would normally try to start a container but should at least parse and merge features successfully
    // The failure will be due to Docker not being available, not feature parsing
    let stderr = String::from_utf8(output.stderr).unwrap();

    // Should not contain feature parsing errors
    assert!(!stderr.contains("Failed to parse additional features JSON"));
    assert!(!stderr.contains("has invalid value type"));
    assert!(!stderr.contains("must be a JSON object"));
}

#[test]
fn test_feature_install_order_override() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();
    let devcontainer_path = devcontainer_dir.join("devcontainer.json");

    // Create a devcontainer.json with features
    fs::write(
        &devcontainer_path,
        r#"{
        "image": "ubuntu:20.04",
        "features": {
            "git": true,
            "node": "18",
            "docker": true
        }
    }"#,
    )
    .unwrap();

    // Test with custom install order
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--feature-install-order")
        .arg("docker,node,git");

    let output = cmd.output().unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    // Should not contain feature parsing errors
    assert!(!stderr.contains("Duplicate feature ID"));
    assert!(!stderr.contains("Feature install order cannot be empty"));
}

#[test]
fn test_cli_features_precedence() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();
    let devcontainer_path = devcontainer_dir.join("devcontainer.json");

    // Create a devcontainer.json with features
    fs::write(
        &devcontainer_path,
        r#"{
        "image": "ubuntu:20.04",
        "features": {
            "node": "16"
        }
    }"#,
    )
    .unwrap();

    // Test with CLI features overriding config features
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--additional-features")
        .arg(r#"{"node": "18"}"#)
        .arg("--prefer-cli-features");

    let output = cmd.output().unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    // Should not contain feature parsing errors
    assert!(!stderr.contains("Failed to parse additional features JSON"));
    assert!(!stderr.contains("has invalid value type"));
}
