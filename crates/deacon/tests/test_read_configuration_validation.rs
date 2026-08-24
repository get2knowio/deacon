//! Validation tests for read-configuration command
//!
//! Tests exact error messages and validation rules per specification issue #294

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Reference parity (#615): with no selector at all, `read-configuration` resolves
/// its configuration from the CURRENT DIRECTORY instead of demanding a flag. This
/// asserts it reads THAT directory's document and not merely that it exits 0: the
/// `name` and the reported `configFilePath` both have to come from the temp
/// workspace, which a run that silently walked elsewhere could not produce.
///
/// MEASURED at oracle 0.87.0 in the same shape: the reference prints a byte-identical
/// result document. Before the fix deacon exited 1 with `Missing required argument: …`
/// without ever looking at the cwd.
#[test]
fn test_no_selectors_defaults_workspace_to_current_directory() {
    let temp_dir = TempDir::new().unwrap();
    // The workspace is named as the temp dir gives it. The process's OWN view of its cwd is
    // what deacon reports since #665, and that view is platform-specific in opposite
    // directions — macOS's `getcwd` always resolves the `/var` symlink, Windows reports the
    // non-verbatim `C:\…` form even when the child was started at a `\\?\` path — so
    // neither spelling this test could hold is right on both. The assertions compare a
    // TRAILING FRAGMENT instead, anchored on the temp directory's unique basename.
    let workspace = temp_dir.path().to_path_buf();
    let config_dir = workspace.join(".devcontainer");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("devcontainer.json"),
        r#"{"name": "cwd-default-marker", "image": "ubuntu:22.04"}"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(&workspace).arg("read-configuration");

    let output = cmd.assert().success().get_output().stdout.clone();
    let parsed: serde_json::Value = serde_json::from_slice(&output).expect("stdout must be JSON");

    assert_eq!(
        parsed["configuration"]["name"], "cwd-default-marker",
        "the cwd's own configuration must be the one read: {parsed}"
    );
    let config_file_path = parsed["configuration"]["configFilePath"]["path"]
        .as_str()
        .expect("configFilePath.path must be reported")
        .replace('\\', "/");
    let expected_tail = format!(
        "{}/.devcontainer/devcontainer.json",
        workspace
            .file_name()
            .and_then(|n| n.to_str())
            .expect("temp dir has a basename")
    );
    assert!(
        config_file_path.ends_with(&expected_tail),
        "the defaulted workspace must resolve to the cwd's document: {config_file_path} \
         should end with {expected_tail}"
    );
}

/// The negative arm of the same default: an existing cwd with NO devcontainer
/// document exits 1 for the right reason — a missing CONFIG, naming the absolute path
/// it looked for, not a missing FLAG. The absence assertion is the load-bearing half;
/// exit 1 alone was already true before the fix, for the wrong reason.
#[test]
fn test_no_selectors_no_config_in_cwd_names_the_missing_document() {
    let temp_dir = TempDir::new().unwrap();
    // Same platform-specific cwd spelling as the test above: anchor on a trailing fragment
    // built with the platform's own separator rather than on a whole path.
    let workspace = temp_dir.path().to_path_buf();
    let expected = std::path::Path::new(
        workspace
            .file_name()
            .and_then(|n| n.to_str())
            .expect("temp dir has a basename"),
    )
    .join(".devcontainer")
    .join("devcontainer.json")
    .display()
    .to_string();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(&workspace).arg("read-configuration");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Configuration file not found:"))
        .stderr(predicate::str::contains(expected))
        // It must no longer refuse on a missing `--workspace-folder` (#615).
        .stderr(predicate::str::contains("Missing required argument").not());
}

#[test]
fn test_selector_requirement_with_only_config() {
    // Spec parity (#66): --config alone is sufficient. The upstream
    // reference CLI accepts an explicit config path without any workspace
    // selector; deacon now matches.
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    fs::write(&config_path, r#"{"name": "test", "image": "ubuntu:22.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path);

    cmd.assert().success();
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
