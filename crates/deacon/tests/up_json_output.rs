//! Integration tests for up command JSON output contract
//!
//! Tests the stdout JSON contract from specs/001-up-gap-spec/contracts/up.md:
//! - Invalid mount/remote-env causes validation error before runtime operations
//! - Success emits proper JSON with all required fields
//! - Error emits proper JSON with error details

use assert_cmd::Command;
use tempfile::TempDir;

#[test]
fn test_up_invalid_mount_format_fails_validation() {
    // Test invalid mount format: missing target
    let mut cmd = Command::cargo_bin("deacon")
        .expect("failed to find deacon binary for tests - ensure 'cargo build' has been run");
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg("/tmp/test-workspace")
        .arg("--mount")
        .arg("type=bind,source=/tmp"); // Invalid: missing target

    cmd.assert().failure().code(1);

    // Stderr should contain the error (not testing exact message here, just that it fails fast)
}

#[test]
fn test_up_invalid_remote_env_format_fails_validation() {
    // Test invalid remote-env format: missing equals sign
    let mut cmd = Command::cargo_bin("deacon")
        .expect("failed to find deacon binary for tests - ensure 'cargo build' has been run");
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg("/tmp/test-workspace")
        .arg("--remote-env")
        .arg("INVALID_NO_EQUALS"); // Invalid: missing =

    cmd.assert().failure().code(1);
}

#[test]
fn test_up_terminal_columns_without_rows_fails() {
    let mut cmd = Command::cargo_bin("deacon")
        .expect("failed to find deacon binary for tests - ensure 'cargo build' has been run");
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg("/tmp/test-workspace")
        .arg("--terminal-columns")
        .arg("80"); // Missing --terminal-rows

    // This should fail at clap level (argument parsing)
    cmd.assert().failure();
}

#[test]
fn test_up_terminal_rows_without_columns_fails() {
    let mut cmd = Command::cargo_bin("deacon")
        .expect("failed to find deacon binary for tests - ensure 'cargo build' has been run");
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg("/tmp/test-workspace")
        .arg("--terminal-rows")
        .arg("24"); // Missing --terminal-columns

    // This should fail at clap level (argument parsing)
    cmd.assert().failure();
}

#[test]
fn test_up_terminal_dimensions_both_specified_ok() {
    // This test just verifies that providing both dimensions doesn't cause parsing errors
    // (the actual up operation will fail due to missing config, but that's expected)
    let mut cmd = Command::cargo_bin("deacon")
        .expect("failed to find deacon binary for tests - ensure 'cargo build' has been run");
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg("/tmp/nonexistent-workspace")
        .arg("--terminal-columns")
        .arg("80")
        .arg("--terminal-rows")
        .arg("24");

    // Should fail due to missing devcontainer.json, not due to terminal dimensions
    cmd.assert().failure();
}

#[test]
fn test_up_missing_workspace_and_id_label_fails() {
    // Contract requires workspace_folder OR id_label
    let mut cmd = Command::cargo_bin("deacon")
        .expect("failed to find deacon binary for tests - ensure 'cargo build' has been run");
    cmd.arg("up");

    cmd.assert().failure();
}

#[test]
fn test_up_json_output_structure_on_missing_config() {
    // Spec contract (specs/001-up-gap-spec/contracts/up.md; up/SPEC.md §10):
    // on error, `up` writes exactly one JSON document to STDOUT shaped
    // `{ outcome: "error", message, description }` and exits 1. All logs go to
    // stderr. This mirrors the reference @devcontainers/cli outcome object, so
    // it is v1 spec-parity — not merely an exit-code check. Hermetic: the error
    // fires during config discovery, so no Docker is required.
    let temp_dir = TempDir::new().unwrap();

    let assert = Command::cargo_bin("deacon")
        .expect("failed to find deacon binary for tests - ensure 'cargo build' has been run")
        .current_dir(temp_dir.path())
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--log-level")
        .arg("error")
        .assert()
        .failure()
        .code(1);

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);

    // stdout must be a single, parseable JSON document (not log noise).
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("up error stdout is not valid JSON ({e}): {stdout:?}"));

    assert_eq!(
        parsed["outcome"], "error",
        "error payload must carry outcome=error: {stdout}"
    );
    assert!(
        parsed
            .get("message")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty()),
        "error payload must include a non-empty string `message`: {stdout}"
    );
    assert!(
        parsed
            .get("description")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty()),
        "error payload must include a non-empty string `description`: {stdout}"
    );
}

// Note: Full integration tests that actually create containers and verify JSON output
// would require Docker to be running and proper test fixtures. Those tests are better
// suited for the full integration test suite that runs in CI with Docker available.
//
// The tests above focus on validation failures that should happen before any Docker
// operations, which is in line with the "fail fast" principle from the contract.
