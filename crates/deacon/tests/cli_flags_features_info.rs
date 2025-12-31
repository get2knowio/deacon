#![cfg(feature = "full")]
//! Integration tests for `features info` CLI flag handling
//!
//! These tests verify that the `features info` subcommand properly handles
//! various CLI flags including --output-format, --log-level, and validation
//! of flag combinations.

mod support;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use support::skip_if_no_network_tests;

#[test]
fn test_output_format_text() {
    if skip_if_no_network_tests() {
        return;
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("manifest")
        .arg("ghcr.io/devcontainers/features/node:1")
        .arg("--output-format")
        .arg("text");

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("Manifest"))
        .stdout(predicates::str::contains("Canonical Identifier"));
}

#[test]
fn test_output_format_json() {
    if skip_if_no_network_tests() {
        return;
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("manifest")
        .arg("ghcr.io/devcontainers/features/node:1")
        .arg("--output-format")
        .arg("json");

    let output = cmd.assert().success();

    // Verify valid JSON
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let json: Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    // Verify structure
    assert!(json.is_object());
    let obj = json.as_object().unwrap();
    assert!(obj.contains_key("manifest"));
    assert!(obj.contains_key("canonicalId"));
}

#[test]
fn test_output_format_invalid() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("manifest")
        .arg("ghcr.io/devcontainers/features/node:1")
        .arg("--output-format")
        .arg("invalid");

    cmd.assert().failure().stderr(
        predicates::str::contains("invalid value 'invalid'").or(predicates::str::contains("error")),
    );
}

#[test]
fn test_log_level_info() {
    if skip_if_no_network_tests() {
        return;
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("manifest")
        .arg("ghcr.io/devcontainers/features/node:1")
        .arg("--log-level")
        .arg("info")
        .arg("--output-format")
        .arg("text");

    cmd.assert().success();
}

#[test]
fn test_log_level_debug() {
    if skip_if_no_network_tests() {
        return;
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("manifest")
        .arg("ghcr.io/devcontainers/features/node:1")
        .arg("--log-level")
        .arg("debug")
        .arg("--output-format")
        .arg("text");

    cmd.assert()
        .success()
        .stderr(predicates::str::contains("DEBUG").or(predicates::str::is_empty()));
}

#[test]
fn test_log_level_trace() {
    if skip_if_no_network_tests() {
        return;
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("manifest")
        .arg("ghcr.io/devcontainers/features/node:1")
        .arg("--log-level")
        .arg("trace")
        .arg("--output-format")
        .arg("text");

    cmd.assert()
        .success()
        .stderr(predicates::str::contains("TRACE").or(predicates::str::is_empty()));
}

#[test]
fn test_log_level_invalid() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("manifest")
        .arg("ghcr.io/devcontainers/features/node:1")
        .arg("--log-level")
        .arg("invalid");

    cmd.assert().failure().stderr(
        predicates::str::contains("invalid value 'invalid'").or(predicates::str::contains("error")),
    );
}

#[test]
fn test_json_output_with_different_log_levels() {
    if skip_if_no_network_tests() {
        return;
    }

    // Test that JSON output is pure (only JSON on stdout) regardless of log level
    for log_level in &["info", "debug", "trace"] {
        let mut cmd = Command::cargo_bin("deacon").unwrap();
        cmd.arg("features")
            .arg("info")
            .arg("manifest")
            .arg("ghcr.io/devcontainers/features/node:1")
            .arg("--output-format")
            .arg("json")
            .arg("--log-level")
            .arg(*log_level);

        let output = cmd.assert().success();

        // Stdout should contain ONLY valid JSON
        let stdout = String::from_utf8_lossy(&output.get_output().stdout);
        let json_result: Result<Value, _> = serde_json::from_str(&stdout);

        assert!(
            json_result.is_ok(),
            "JSON output should be valid for log-level={}. Output: {}",
            log_level,
            stdout
        );

        // Verify no log messages leaked into stdout
        assert!(
            !stdout.contains("INFO") && !stdout.contains("DEBUG") && !stdout.contains("TRACE"),
            "Log messages should not appear in JSON stdout for log-level={}",
            log_level
        );
    }
}

#[test]
fn test_missing_required_mode() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("ghcr.io/devcontainers/features/node:1");

    cmd.assert().failure().stderr(
        predicates::str::contains("required arguments").or(predicates::str::contains("MODE")),
    );
}

#[test]
fn test_missing_required_feature() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features").arg("info").arg("manifest");

    cmd.assert().failure().stderr(
        predicates::str::contains("required arguments").or(predicates::str::contains("FEATURE")),
    );
}

#[test]
fn test_invalid_mode() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("invalid-mode")
        .arg("ghcr.io/devcontainers/features/node:1");

    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("invalid-mode").or(predicates::str::contains("error")));
}

#[test]
fn test_help_flag() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features").arg("info").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("features info"))
        .stdout(predicates::str::contains("MODE"))
        .stdout(predicates::str::contains("FEATURE"))
        .stdout(predicates::str::contains("--output-format"))
        .stdout(predicates::str::contains("--log-level"));
}

#[test]
fn test_all_modes_listed_in_help() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features").arg("info").arg("--help");

    let output = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Verify all modes are documented
    assert!(stdout.contains("manifest"));
    assert!(stdout.contains("tags"));
    assert!(stdout.contains("dependencies"));
    assert!(stdout.contains("verbose"));
}
