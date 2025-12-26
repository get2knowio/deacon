#![cfg(feature = "full")]
//! Integration tests for `deacon features info verbose` subcommand
//!
//! Tests text and JSON output formats, plus partial-failure scenarios.

mod support;
use assert_cmd::Command;
use support::{extract_json_from_output, skip_if_no_network_tests};

/// Test verbose mode with valid remote feature (text output)
/// Should contain all three boxed sections: Manifest, Published Tags, Dependency Tree
/// Requires network access - gated by DEACON_NETWORK_TESTS=1
#[test]
fn test_verbose_remote_text() {
    // Skip test unless network tests are enabled
    if skip_if_no_network_tests() {
        return;
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "verbose",
        "ghcr.io/devcontainers/features/node:1",
        "--output-format",
        "text",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed
    assert!(output.status.success(), "Command should succeed");

    // Should contain all three boxed sections
    assert!(
        stdout.contains("Manifest"),
        "Should contain 'Manifest' section"
    );
    assert!(
        stdout.contains("Canonical Identifier"),
        "Should contain 'Canonical Identifier' section"
    );
    assert!(
        stdout.contains("Published Tags"),
        "Should contain 'Published Tags' section"
    );
    assert!(
        stdout.contains("Dependency Tree"),
        "Should contain 'Dependency Tree' section"
    );

    // Verify Mermaid graph is present
    assert!(
        stdout.contains("graph TD"),
        "Should contain Mermaid graph syntax"
    );
    assert!(
        stdout.contains("https://mermaid.live/"),
        "Should contain mermaid.live URL"
    );
}

/// Test verbose mode with valid remote feature (JSON output)
/// Should contain manifest, canonicalId, and publishedTags (no dependency graph)
/// Requires network access - gated by DEACON_NETWORK_TESTS=1
#[test]
fn test_verbose_remote_json() {
    // Skip test unless network tests are enabled
    if skip_if_no_network_tests() {
        return;
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "verbose",
        "ghcr.io/devcontainers/features/node:1",
        "--output-format",
        "json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed
    assert!(output.status.success(), "Command should succeed");

    // Parse JSON output
    let json = extract_json_from_output(&stdout).unwrap();

    // Verify JSON contract: { "manifest": {...}, "canonicalId": "...", "publishedTags": [...] }
    assert!(
        json.get("manifest").is_some(),
        "Should have 'manifest' field"
    );
    assert!(
        json.get("canonicalId").is_some(),
        "Should have 'canonicalId' field"
    );
    assert!(
        json.get("publishedTags").is_some(),
        "Should have 'publishedTags' field"
    );

    // Verify canonicalId is a string (not null for remote refs)
    assert!(
        json["canonicalId"].is_string(),
        "canonicalId should be a string for remote refs"
    );

    // Verify publishedTags is an array
    let tags = json["publishedTags"].as_array().unwrap();
    assert!(!tags.is_empty(), "Should have at least one tag");

    // Verify manifest is an object
    assert!(json["manifest"].is_object(), "Manifest should be an object");

    // Verify no dependency graph in JSON (text-only)
    assert!(
        json.get("dependencyGraph").is_none(),
        "Should not include dependency graph in JSON mode"
    );
    assert!(
        json.get("dependencies").is_none(),
        "Should not include dependencies field in JSON mode"
    );
}

/// Test verbose mode with invalid registry reference (should return {} + exit 1)
#[test]
fn test_verbose_invalid_ref() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "verbose",
        "invalid.registry.example.com/nonexistent/feature",
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

    // Should output empty JSON object {} or partial result with errors
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();

    // In verbose mode, we may get partial results with errors
    // Check if we got {} (total failure) or partial results with errors field
    if json.as_object().unwrap().is_empty() {
        assert_eq!(
            json,
            serde_json::json!({}),
            "Total failure should output empty JSON object"
        );
    } else {
        // Partial failure - should have errors field
        assert!(
            json.get("errors").is_some(),
            "Partial failure should include 'errors' field"
        );
    }
}

/// Test verbose mode partial failure scenario: simulate failure in one sub-mode
/// This test validates the error accumulation behavior
#[test]
fn test_verbose_partial_failure_malformed_ref() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "verbose",
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

    // Parse JSON to check structure
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();

    // Should be empty JSON or have errors
    if !json.as_object().unwrap().is_empty() {
        assert!(
            json.get("errors").is_some(),
            "Should include 'errors' field for partial failure"
        );
    }
}

/// Test verbose mode with text output for invalid ref (should print error and exit 1)
#[test]
fn test_verbose_invalid_ref_text() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "verbose",
        "invalid.registry.example.com/nonexistent/feature",
        "--output-format",
        "text",
    ]);

    let output = cmd.output().unwrap();

    // Should fail with exit code 1
    assert!(
        !output.status.success(),
        "Command should fail for invalid ref in text mode"
    );

    // In text mode, we don't check for specific error format - just that it fails
}

/// Test that verbose mode JSON does not include dependency graph data
/// Requires network access - gated by DEACON_NETWORK_TESTS=1
#[test]
fn test_verbose_json_excludes_graph() {
    // Skip test unless network tests are enabled
    if skip_if_no_network_tests() {
        return;
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "verbose",
        "ghcr.io/devcontainers/features/common-utils",
        "--output-format",
        "json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed
    assert!(output.status.success(), "Command should succeed");

    // Parse JSON
    let json = extract_json_from_output(&stdout).unwrap();

    // Verify no graph-related fields
    assert!(
        json.get("dependencyGraph").is_none(),
        "Should not include dependencyGraph in JSON mode"
    );
    assert!(
        json.get("graph").is_none(),
        "Should not include graph in JSON mode"
    );
    assert!(
        json.get("mermaid").is_none(),
        "Should not include mermaid in JSON mode"
    );

    // Should still have the three expected fields
    assert!(json.get("manifest").is_some(), "Should have manifest");
    assert!(json.get("canonicalId").is_some(), "Should have canonicalId");
    assert!(
        json.get("publishedTags").is_some(),
        "Should have publishedTags"
    );
}
