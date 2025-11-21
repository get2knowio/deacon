//! Validation tests for read-configuration command
//!
//! Tests exact error messages and validation rules per specification issue #294

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_selector_requirement_no_selectors() {
    // When no selector flags are provided (only --config), should fail with exact message
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    fs::write(&config_path, r#"{"name": "test", "image": "ubuntu:22.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path);

    // Should fail because --config alone does not satisfy the selector requirement
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains(
            "Missing required argument: One of --container-id, --id-label or --workspace-folder is required.",
        ));
}

#[test]
fn test_selector_requirement_with_workspace_folder() {
    // When --workspace-folder is provided along with --config, should succeed
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    fs::write(&config_path, r#"{"name": "test", "image": "ubuntu:22.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path());

    // Should succeed because --workspace-folder satisfies the selector requirement
    cmd.assert().success();
}

#[test]
fn test_id_label_invalid_format_missing_equals() {
    // --id-label without '=' should fail with exact message
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    fs::write(&config_path, r#"{"name": "test", "image": "ubuntu:22.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--id-label")
        .arg("invalid");

    cmd.assert().failure().stderr(predicate::str::contains(
        "Unmatched argument format: id-label must match <name>=<value>.",
    ));
}

#[test]
fn test_id_label_invalid_format_empty_name() {
    // --id-label with empty name should fail
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    fs::write(&config_path, r#"{"name": "test", "image": "ubuntu:22.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--id-label")
        .arg("=value");

    cmd.assert().failure().stderr(predicate::str::contains(
        "Unmatched argument format: id-label must match <name>=<value>.",
    ));
}

#[test]
fn test_id_label_invalid_format_empty_value() {
    // --id-label with empty value should fail
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    fs::write(&config_path, r#"{"name": "test", "image": "ubuntu:22.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--id-label")
        .arg("name=");

    cmd.assert().failure().stderr(predicate::str::contains(
        "Unmatched argument format: id-label must match <name>=<value>.",
    ));
}

#[test]
fn test_id_label_valid_format() {
    // Valid --id-label should work (even though container won't be found)
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    fs::write(&config_path, r#"{"name": "test", "image": "ubuntu:22.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--id-label")
        .arg("app=myapp");

    // Should succeed with valid id-label format, even if no container is found
    cmd.assert().success();
}

#[test]
fn test_terminal_dimensions_only_columns() {
    // Only --terminal-columns should fail with pairing error
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    fs::write(&config_path, r#"{"name": "test", "image": "ubuntu:22.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--terminal-columns")
        .arg("80");

    cmd.assert().failure().stderr(predicate::str::contains(
        "--terminal-columns and --terminal-rows must both be provided",
    ));
}

#[test]
fn test_terminal_dimensions_only_rows() {
    // Only --terminal-rows should fail with pairing error
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    fs::write(&config_path, r#"{"name": "test", "image": "ubuntu:22.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--terminal-rows")
        .arg("24");

    cmd.assert().failure().stderr(predicate::str::contains(
        "--terminal-columns and --terminal-rows must both be provided",
    ));
}

#[test]
fn test_terminal_dimensions_both_provided() {
    // Both dimensions should work
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    fs::write(&config_path, r#"{"name": "test", "image": "ubuntu:22.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--terminal-columns")
        .arg("80")
        .arg("--terminal-rows")
        .arg("24");

    cmd.assert().success();
}

#[test]
fn test_terminal_dimensions_neither_provided() {
    // Neither dimension should work (they're optional)
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    fs::write(&config_path, r#"{"name": "test", "image": "ubuntu:22.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path());

    cmd.assert().success();
}

#[test]
fn test_additional_features_invalid_json() {
    // Invalid JSON should fail with parse error
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    fs::write(&config_path, r#"{"name": "test", "image": "ubuntu:22.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--additional-features")
        .arg("not valid json");

    cmd.assert().failure().stderr(predicate::str::contains(
        "Failed to parse --additional-features JSON",
    ));
}

#[test]
fn test_additional_features_not_object() {
    // Non-object JSON (array) should fail
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    fs::write(&config_path, r#"{"name": "test", "image": "ubuntu:22.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--additional-features")
        .arg(r#"["not", "an", "object"]"#);

    cmd.assert().failure().stderr(predicate::str::contains(
        "--additional-features must be a JSON object",
    ));
}

#[test]
fn test_additional_features_valid_object() {
    // Valid JSON object should work
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    fs::write(&config_path, r#"{"name": "test", "image": "ubuntu:22.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--additional-features")
        .arg(r#"{"ghcr.io/devcontainers/features/node:1": "lts"}"#);

    cmd.assert().success();
}

#[test]
fn test_config_not_found_exact_message() {
    // Missing config should have exact error message format
    let temp_dir = TempDir::new().unwrap();
    let missing_path = temp_dir.path().join("devcontainer.json");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&missing_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path());

    let path_str = missing_path.display().to_string();
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Configuration file not found:"))
        .stderr(predicate::str::contains(path_str));
}

#[test]
fn test_config_non_object_root() {
    // Non-object root should have exact error message
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    fs::write(&config_path, "[]").unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path());

    cmd.assert().failure().stderr(predicate::str::contains(
        "must contain a JSON object literal.",
    ));
}

#[test]
fn test_no_stdout_on_validation_error() {
    // Validation errors should not print JSON to stdout
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    fs::write(&config_path, r#"{"name": "test", "image": "ubuntu:22.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--id-label")
        .arg("invalid"); // Invalid format

    cmd.assert().failure().stdout(predicate::str::is_empty()); // No stdout
}

#[test]
fn test_no_stdout_on_config_parse_error() {
    // Config parse errors should not print JSON to stdout
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    fs::write(&config_path, "not valid json").unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path());

    cmd.assert().failure().stdout(predicate::str::is_empty()); // No stdout
}
