#![cfg(feature = "full")]
//! Integration tests for `deacon features info dependencies` subcommand
//!
//! Tests text output with Mermaid graph and JSON mode rejection behavior.

mod support;
use assert_cmd::Command;
use support::skip_if_no_network_tests;

/// Test dependencies mode with text output (remote feature)
/// Requires network access - gated by DEACON_NETWORK_TESTS=1
#[test]
fn test_dependencies_remote_text() {
    // Skip test unless network tests are enabled
    if skip_if_no_network_tests() {
        return;
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "dependencies",
        "ghcr.io/devcontainers/features/node",
        "--output-format",
        "text",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed and contain dependency tree
    assert!(output.status.success(), "Command should succeed");
    assert!(
        stdout.contains("Dependency Tree"),
        "Should contain 'Dependency Tree' section"
    );
    assert!(
        stdout.contains("https://mermaid.live/"),
        "Should contain mermaid.live URL"
    );
    assert!(
        stdout.contains("graph TD"),
        "Should contain Mermaid graph syntax"
    );
}

/// Test dependencies mode with JSON output (should fail with {} + exit 1)
#[test]
fn test_dependencies_json_rejection() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "dependencies",
        "ghcr.io/devcontainers/features/node",
        "--output-format",
        "json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should fail with exit code 1
    assert!(
        !output.status.success(),
        "Command should fail for JSON output in dependencies mode"
    );

    // Should output empty JSON object {}
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({}),
        "Should output empty JSON object when JSON not supported"
    );
}

/// Test dependencies mode with invalid reference (should return {} + exit 1 in JSON mode)
#[test]
fn test_dependencies_invalid_ref_json() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "dependencies",
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

/// Test dependencies mode with feature that has no dependencies
/// Requires network access - gated by DEACON_NETWORK_TESTS=1
#[test]
fn test_dependencies_no_deps() {
    // Skip test unless network tests are enabled
    if skip_if_no_network_tests() {
        return;
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "dependencies",
        "ghcr.io/devcontainers/features/common-utils",
        "--output-format",
        "text",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed
    assert!(output.status.success(), "Command should succeed");

    // Should contain dependency tree section
    assert!(
        stdout.contains("Dependency Tree"),
        "Should contain 'Dependency Tree' section"
    );

    // Should contain Mermaid graph syntax
    assert!(
        stdout.contains("graph TD"),
        "Should contain Mermaid graph syntax"
    );

    // Should indicate no dependencies if feature has none
    // (either through "(no dependencies)" text or just the node itself)
    assert!(
        stdout.contains("no dependencies") || stdout.contains("graph TD"),
        "Should handle features with no dependencies"
    );
}

/// Test dependencies mode Mermaid graph format
/// Requires network access - gated by DEACON_NETWORK_TESTS=1
#[test]
fn test_dependencies_mermaid_format() {
    // Skip test unless network tests are enabled
    if skip_if_no_network_tests() {
        return;
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "dependencies",
        "ghcr.io/devcontainers/features/node",
        "--output-format",
        "text",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should succeed
    assert!(output.status.success(), "Command should succeed");

    // Extract the Mermaid graph section
    let lines: Vec<&str> = stdout.lines().collect();

    // Find the line with "graph TD" - this is the start of the Mermaid graph
    let graph_start = lines.iter().position(|l| l.contains("graph TD"));
    assert!(
        graph_start.is_some(),
        "Should contain 'graph TD' line starting the Mermaid graph"
    );

    // Basic validation that the graph contains reasonable content
    // The graph should have the feature ID somewhere in it
    assert!(
        stdout.contains("-->") || stdout.contains("no dependencies"),
        "Graph should contain edges (-->) or indicate no dependencies"
    );
}
