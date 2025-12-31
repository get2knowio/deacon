#![cfg(feature = "full")]
//! Integration tests for `deacon features info tags` subcommand
//!
//! Tests text and JSON output formats, plus error handling for invalid refs.

mod support;
use assert_cmd::Command;
use support::{extract_json_from_output, skip_if_no_network_tests};

/// Test tags mode with valid remote feature (text output)
/// Requires network access - gated by DEACON_NETWORK_TESTS=1
#[test]
fn test_tags_remote_text() {
    // Skip test unless network tests are enabled
    if skip_if_no_network_tests() {
        return;
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "tags",
        "ghcr.io/devcontainers/features/node",
        "--output-format",
        "text",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed and contain published tags
    assert!(output.status.success(), "Command should succeed");
    assert!(
        stdout.contains("Published Tags"),
        "Should contain 'Published Tags' section"
    );
}

/// Test tags mode with valid remote feature (JSON output)
/// Requires network access - gated by DEACON_NETWORK_TESTS=1
#[test]
fn test_tags_remote_json() {
    // Skip test unless network tests are enabled
    if skip_if_no_network_tests() {
        return;
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "tags",
        "ghcr.io/devcontainers/features/node",
        "--output-format",
        "json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed and produce valid JSON
    assert!(output.status.success(), "Command should succeed");

    // Parse JSON output
    let json = extract_json_from_output(&stdout).unwrap();

    // Verify JSON contract: { "publishedTags": [...] }
    assert!(
        json.get("publishedTags").is_some(),
        "Should have 'publishedTags' field"
    );

    // Verify publishedTags is an array
    let tags = json["publishedTags"].as_array().unwrap();
    assert!(!tags.is_empty(), "Should have at least one tag");

    // Verify all tags are strings
    for tag in tags {
        assert!(tag.is_string(), "All tags should be strings");
    }
}

/// Test tags mode with invalid registry reference (should return {} + exit 1)
#[test]
fn test_tags_invalid_ref() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "tags",
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

    // Should output empty JSON object {}
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({}),
        "Should output empty JSON object on error"
    );
}

/// Test tags mode with malformed reference (should return {} + exit 1)
#[test]
fn test_tags_malformed_ref() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "tags",
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

/// Test tags mode with deterministic sorting
/// Requires network access - gated by DEACON_NETWORK_TESTS=1
#[test]
fn test_tags_deterministic_sorting() {
    // Skip test unless network tests are enabled
    if skip_if_no_network_tests() {
        return;
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "tags",
        "ghcr.io/devcontainers/features/node",
        "--output-format",
        "json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed
    assert!(output.status.success(), "Command should succeed");

    // Parse JSON output
    let json = extract_json_from_output(&stdout).unwrap();
    let tags = json["publishedTags"].as_array().unwrap();

    // Verify tags are sorted (lexicographically)
    let tag_strings: Vec<String> = tags
        .iter()
        .map(|t| t.as_str().unwrap().to_string())
        .collect();
    let mut sorted_tags = tag_strings.clone();
    sorted_tags.sort();

    assert_eq!(
        tag_strings, sorted_tags,
        "Tags should be sorted deterministically"
    );
}
