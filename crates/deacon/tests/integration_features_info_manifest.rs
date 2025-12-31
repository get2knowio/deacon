#![cfg(feature = "full")]
//! Integration tests for `deacon features info manifest` subcommand
//!
//! Tests text and JSON output formats, plus error handling for invalid refs.

mod support;
use assert_cmd::Command;
use support::{extract_json_from_output, skip_if_no_network_tests};

/// Test manifest mode with valid remote feature (text output)
/// Requires network access - gated by DEACON_NETWORK_TESTS=1
#[test]
fn test_manifest_remote_text() {
    if skip_if_no_network_tests() {
        return;
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "manifest",
        "ghcr.io/devcontainers/features/node:1",
        "--output-format",
        "text",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed and contain manifest information
    assert!(output.status.success(), "Command should succeed");
    assert!(
        stdout.contains("Manifest"),
        "Should contain 'Manifest' section"
    );
    assert!(
        stdout.contains("Canonical Identifier"),
        "Should contain 'Canonical Identifier' section"
    );
}

/// Test manifest mode with valid remote feature (JSON output)
/// Requires network access - gated by DEACON_NETWORK_TESTS=1
#[test]
fn test_manifest_remote_json() {
    if skip_if_no_network_tests() {
        return;
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "manifest",
        "ghcr.io/devcontainers/features/node:1",
        "--output-format",
        "json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed and produce valid JSON
    assert!(output.status.success(), "Command should succeed");

    // Parse JSON output
    let json = extract_json_from_output(&stdout).unwrap();

    // Verify JSON contract: { manifest, canonicalId }
    assert!(
        json.get("manifest").is_some(),
        "Should have 'manifest' field"
    );
    assert!(
        json.get("canonicalId").is_some(),
        "Should have 'canonicalId' field"
    );

    // Verify manifest is an object
    assert!(json["manifest"].is_object(), "Manifest should be an object");

    // Verify canonicalId is a string starting with sha256:
    let canonical_id = json["canonicalId"].as_str().unwrap();
    assert!(
        canonical_id.starts_with("sha256:"),
        "Canonical ID should start with 'sha256:'"
    );
    assert_eq!(
        canonical_id.len(),
        71,
        "Canonical ID should be 71 characters (sha256: + 64 hex)"
    );
}

/// Test manifest mode with invalid registry reference (should return {} + exit 1)
#[test]
fn test_manifest_invalid_ref() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "manifest",
        "invalid.registry.example.com/nonexistent/feature:latest",
        "--output-format",
        "json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should fail with exit code 1
    assert!(
        !output.status.success(),
        "Command should fail for invalid ref"
    );

    // Should output empty JSON object {}
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({}),
        "Should output empty JSON object on error"
    );
}

/// Test manifest mode with malformed reference (should return {} + exit 1)
#[test]
fn test_manifest_malformed_ref() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "manifest",
        "not-a-valid-reference",
        "--output-format",
        "json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should fail with exit code 1
    assert!(
        !output.status.success(),
        "Command should fail for malformed ref"
    );

    // Should output empty JSON object {}
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({}),
        "Should output empty JSON object on error"
    );
}

/// Test manifest mode with local feature (canonicalId should be null)
#[test]
fn test_manifest_local_feature() {
    // Use a fixture from the workspace
    let fixture_path = {
        let current_dir = std::env::current_dir().unwrap();
        let workspace_root = if current_dir.ends_with("crates/deacon") {
            current_dir.parent().unwrap().parent().unwrap()
        } else {
            &current_dir
        };
        workspace_root.join("fixtures/features/minimal")
    };

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "manifest",
        fixture_path.to_str().unwrap(),
        "--output-format",
        "json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed
    assert!(
        output.status.success(),
        "Command should succeed for local feature"
    );

    // Parse JSON output
    let json = extract_json_from_output(&stdout).unwrap();

    // Verify JSON contract: { manifest, canonicalId: null }
    assert!(
        json.get("manifest").is_some(),
        "Should have 'manifest' field"
    );
    assert_eq!(
        json.get("canonicalId"),
        Some(&serde_json::Value::Null),
        "Canonical ID should be null for local features"
    );

    // Verify manifest content
    assert_eq!(
        json["manifest"]["id"], "minimal-feature",
        "Should have correct feature ID"
    );
}
