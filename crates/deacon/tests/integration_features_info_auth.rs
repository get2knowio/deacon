//! Integration tests for `features info` authentication failures
//!
//! These tests verify that authentication errors are properly handled and reported
//! by the `features info` command for all modes (manifest, tags, dependencies, verbose).
//!
//! **Network Tests**: These tests require network access and are gated by the
//! `DEACON_NETWORK_TESTS` environment variable. Set `DEACON_NETWORK_TESTS=1` to run.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

/// Helper to check if network tests should run
fn should_run_network_tests() -> bool {
    std::env::var("DEACON_NETWORK_TESTS")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false)
}

/// Test auth failure for manifest mode (text output)
#[test]
fn test_manifest_auth_failure_text() {
    if !should_run_network_tests() {
        eprintln!("Skipping network test (DEACON_NETWORK_TESTS not set)");
        return;
    }

    // Use a private registry that requires authentication
    // Note: This will fail with a 401 error without proper credentials
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("manifest")
        .arg("ghcr.io/private/feature:1.0.0")
        .arg("--output-format")
        .arg("text");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Authentication"));
}

/// Test auth failure for manifest mode (JSON output)
#[test]
fn test_manifest_auth_failure_json() {
    if !should_run_network_tests() {
        eprintln!("Skipping network test (DEACON_NETWORK_TESTS not set)");
        return;
    }

    // Use a private registry that requires authentication
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("manifest")
        .arg("ghcr.io/private/feature:1.0.0")
        .arg("--output-format")
        .arg("json");

    let output = cmd.assert().failure();

    // Verify JSON output is empty object {} on error
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let json: Value = serde_json::from_str(&stdout).unwrap_or(serde_json::json!(null));
    assert_eq!(
        json,
        serde_json::json!({}),
        "JSON output should be {{}} on auth error"
    );
}

/// Test auth failure for tags mode (text output)
#[test]
fn test_tags_auth_failure_text() {
    if !should_run_network_tests() {
        eprintln!("Skipping network test (DEACON_NETWORK_TESTS not set)");
        return;
    }

    // Use a private registry that requires authentication
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("tags")
        .arg("ghcr.io/private/feature")
        .arg("--output-format")
        .arg("text");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Authentication"));
}

/// Test auth failure for tags mode (JSON output)
#[test]
fn test_tags_auth_failure_json() {
    if !should_run_network_tests() {
        eprintln!("Skipping network test (DEACON_NETWORK_TESTS not set)");
        return;
    }

    // Use a private registry that requires authentication
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("tags")
        .arg("ghcr.io/private/feature")
        .arg("--output-format")
        .arg("json");

    let output = cmd.assert().failure();

    // Verify JSON output is empty object {} on error
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let json: Value = serde_json::from_str(&stdout).unwrap_or(serde_json::json!(null));
    assert_eq!(
        json,
        serde_json::json!({}),
        "JSON output should be {{}} on auth error"
    );
}

/// Test auth failure for verbose mode (text output)
#[test]
fn test_verbose_auth_failure_text() {
    if !should_run_network_tests() {
        eprintln!("Skipping network test (DEACON_NETWORK_TESTS not set)");
        return;
    }

    // Use a private registry that requires authentication
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("verbose")
        .arg("ghcr.io/private/feature:1.0.0")
        .arg("--output-format")
        .arg("text");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Authentication"));
}

/// Test auth failure for verbose mode (JSON output with partial failure)
#[test]
fn test_verbose_auth_failure_json_partial() {
    if !should_run_network_tests() {
        eprintln!("Skipping network test (DEACON_NETWORK_TESTS not set)");
        return;
    }

    // Use a private registry that requires authentication
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("verbose")
        .arg("ghcr.io/private/feature:1.0.0")
        .arg("--output-format")
        .arg("json");

    let output = cmd.assert().failure();

    // Verbose mode may include partial data with errors
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let json: Value = serde_json::from_str(&stdout).unwrap_or(serde_json::json!(null));

    // Should contain errors object in verbose mode
    assert!(
        json.as_object().is_some(),
        "JSON output should be an object in verbose mode"
    );

    // If all operations fail, it could be {} or contain errors
    let obj = json.as_object().unwrap();
    if !obj.is_empty() {
        assert!(
            obj.contains_key("errors"),
            "Verbose JSON should contain 'errors' key on failure"
        );
    }
}

/// Test 403 Forbidden error handling
#[test]
fn test_manifest_forbidden_error() {
    if !should_run_network_tests() {
        eprintln!("Skipping network test (DEACON_NETWORK_TESTS not set)");
    }

    // This test is a placeholder - actual 403 testing requires a registry setup
    // that returns 403. For now, we just document the expected behavior.

    // Expected behavior on 403:
    // - Text mode: Error message with "Authorization denied" or similar
    // - JSON mode: {} with exit code 1
    // - Logs should not contain any credentials or tokens

    // Note: Implement actual test when we have a test registry that can return 403
}
