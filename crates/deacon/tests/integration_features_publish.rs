#![cfg(feature = "full")]
//! Integration tests for features publish JSON output purity
//!
//! These tests verify that the features publish command produces
//! JSON-only output on stdout and logs on stderr, per T009 requirements.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

/// Test that features publish produces JSON-only stdout and logs-to-stderr for success case
#[test]
fn test_features_publish_json_stdout_logs_stderr_success() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature");

    // Create feature directory with minimal files
    fs::create_dir_all(&feature_dir).unwrap();
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{"id": "test-feature", "version": "1.0.0", "name": "Test Feature"}"#,
    )
    .unwrap();
    fs::write(
        feature_dir.join("install.sh"),
        "#!/bin/bash\necho 'Installing test feature'",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "publish",
        feature_dir.to_str().unwrap(),
        "--registry",
        "ghcr.io/test",
        "--namespace",
        "testuser",
        "--dry-run",
        "--json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Command should succeed
    assert!(output.status.success(), "Command failed: {}", stderr);

    // Stdout should be parseable as valid JSON (no log messages)
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|e| format!("stdout is not valid JSON: {} (stdout: '{}')", e, stdout))
        .unwrap();

    // Should contain expected fields from PublishOutput
    assert!(
        parsed.get("features").is_some(),
        "Should have features array"
    );
    assert!(
        parsed.get("summary").is_some(),
        "Should have summary object"
    );

    // Stdout should NOT contain log messages (no timestamps, no log levels)
    assert!(
        !stdout.contains(" INFO "),
        "Stdout should not contain INFO logs"
    );
    assert!(
        !stdout.contains(" DEBUG "),
        "Stdout should not contain DEBUG logs"
    );
    assert!(
        !stdout.contains(" WARN "),
        "Stdout should not contain WARN logs"
    );
    assert!(
        !stdout.contains(" ERROR "),
        "Stdout should not contain ERROR logs"
    );

    // Stderr may contain logs (this is expected and desired)
    // We don't assert on stderr content since it may vary, but stdout must be pure JSON
}

/// Test that features publish produces empty stdout and logs-to-stderr for fatal error case
#[test]
fn test_features_publish_json_stdout_empty_on_fatal_error() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature");

    // Create feature directory with invalid JSON metadata (should cause fatal error)
    fs::create_dir_all(&feature_dir).unwrap();
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{"id": "test-feature", "version": "1.0.0", "name": "Test Feature""#, // Missing closing brace
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "publish",
        feature_dir.to_str().unwrap(),
        "--registry",
        "ghcr.io/test",
        "--namespace",
        "testuser",
        "--dry-run",
        "--json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Command should fail with exit code 1 due to invalid JSON
    assert!(
        !output.status.success(),
        "Command should fail when JSON is invalid"
    );

    // Stdout should be empty (no JSON output for fatal errors)
    assert_eq!(
        stdout.trim(),
        "",
        "Stdout should be empty for fatal errors, got: '{}'",
        stdout
    );

    // Stderr should contain error message
    assert!(
        stderr.contains("error") || stderr.contains("Error"),
        "Stderr should contain error message, got: '{}'",
        stderr
    );

    // Stderr should NOT contain JSON (logs only)
    assert!(
        !stderr.contains("{\"features\":"),
        "Stderr should not contain JSON output"
    );
}

/// Test that features publish fails with validation error when feature has invalid (empty) id
#[test]
fn test_features_publish_no_features_discovered_after_packaging() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("empty-feature");

    // Create feature directory with devcontainer-feature.json that parses but has empty id (invalid)
    fs::create_dir_all(&feature_dir).unwrap();
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{"id": ""}"#, // Empty id - parses but fails validation
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "publish",
        feature_dir.to_str().unwrap(),
        "--registry",
        "ghcr.io/test",
        "--namespace",
        "testuser",
        "--dry-run",
        "--json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Command should fail with exit code 1
    assert!(
        !output.status.success(),
        "Command should fail when no features are found, stderr: {}",
        stderr
    );

    // Stdout should be empty (no JSON output for fatal errors)
    assert_eq!(
        stdout.trim(),
        "",
        "Stdout should be empty for fatal errors, got: '{}'",
        stdout
    );

    // Stderr should contain the validation error for empty ID
    assert!(
        stderr.contains("Feature metadata validation failed"),
        "Stderr should contain 'Feature metadata validation failed', got: '{}'",
        stderr
    );
}

/// Test that features publish maintains JSON purity even with debug logging enabled
#[test]
fn test_features_publish_json_purity_with_debug_logging() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature");

    // Create feature directory with minimal files
    fs::create_dir_all(&feature_dir).unwrap();
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{"id": "test-feature", "version": "1.0.0", "name": "Test Feature"}"#,
    )
    .unwrap();
    fs::write(
        feature_dir.join("install.sh"),
        "#!/bin/bash\necho 'Installing test feature'",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.env("RUST_LOG", "debug").args([
        "features",
        "publish",
        feature_dir.to_str().unwrap(),
        "--registry",
        "ghcr.io/test",
        "--namespace",
        "testuser",
        "--dry-run",
        "--json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Command should succeed
    assert!(output.status.success(), "Command failed: {}", stderr);

    // Stdout should be pure JSON - no log messages even with debug logging
    assert!(!stdout.contains("Starting"));
    assert!(!stdout.contains("DEBUG"));
    assert!(!stdout.contains("INFO"));
    assert!(!stdout.contains("feature.publish"));

    // Stdout should parse as clean JSON
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert!(parsed.get("features").is_some());
    assert!(parsed.get("summary").is_some());

    // Debug logs should appear in stderr (not guaranteed, but if present should not be in stdout)
    // The key requirement is that stdout is pure JSON
}

/// Test that features publish fails with "Invalid semantic version" when version is not valid SemVer
#[test]
fn test_features_publish_invalid_semver_version() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("invalid-version-feature");

    // Create feature directory with invalid SemVer version
    fs::create_dir_all(&feature_dir).unwrap();
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{"id": "invalid-version-feature", "version": "not-a-valid-semver", "name": "Invalid Version Feature"}"#,
    )
    .unwrap();
    fs::write(
        feature_dir.join("install.sh"),
        "#!/bin/bash\necho 'Installing invalid version feature'",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "publish",
        feature_dir.to_str().unwrap(),
        "--registry",
        "ghcr.io/test",
        "--namespace",
        "testuser",
        "--dry-run",
        "--json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Command should fail with exit code 1 due to invalid SemVer
    assert!(
        !output.status.success(),
        "Command should fail when version is invalid SemVer, stderr: {}",
        stderr
    );

    // Stdout should be empty (no JSON output for fatal errors)
    assert_eq!(
        stdout.trim(),
        "",
        "Stdout should be empty for fatal errors, got: '{}'",
        stdout
    );

    // Stderr should contain the specific error message about invalid semantic version
    assert!(
        stderr.contains("Invalid semantic version"),
        "Stderr should contain 'Invalid semantic version', got: '{}'",
        stderr
    );

    // Also check that the invalid version is mentioned in the error
    assert!(
        stderr.contains("not-a-valid-semver"),
        "Stderr should contain the invalid version string, got: '{}'",
        stderr
    );
}
