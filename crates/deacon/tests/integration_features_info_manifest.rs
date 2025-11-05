//! Integration tests for `deacon features info manifest` subcommand
//!
//! Tests text and JSON output formats, plus error handling for invalid refs.

use assert_cmd::Command;

/// Helper function to extract JSON from mixed output (logs + JSON)
fn extract_json_from_output(output: &str) -> Result<serde_json::Value, serde_json::Error> {
    // Try to find JSON by looking for complete JSON objects
    // Skip lines that look like log messages (contain timestamp patterns)
    for line in output.lines() {
        let trimmed = line.trim();
        // Skip lines that contain log timestamps or ANSI codes
        if trimmed.contains("Z ") || trimmed.contains("\x1b[") || trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            if let Ok(json) = serde_json::from_str(trimmed) {
                return Ok(json);
            }
        }
    }

    // If that doesn't work, try to extract everything after the last log line
    let lines: Vec<&str> = output.lines().collect();
    for i in (0..lines.len()).rev() {
        let line = lines[i].trim();
        if line.starts_with('{') {
            // Collect all lines from this point onwards and try to parse as JSON
            let json_part = lines[i..].join("\n");
            if let Ok(json) = serde_json::from_str(&json_part) {
                return Ok(json);
            }
        }
    }

    // Last resort - try the whole output
    serde_json::from_str(output)
}

/// Test manifest mode with valid remote feature (text output)
/// Requires network access - gated by DEACON_NETWORK_TESTS=1
#[test]
fn test_manifest_remote_text() {
    // Skip test unless network tests are enabled
    if std::env::var("DEACON_NETWORK_TESTS").is_err() {
        eprintln!("Skipping network test - set DEACON_NETWORK_TESTS=1 to enable");
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
    // Skip test unless network tests are enabled
    if std::env::var("DEACON_NETWORK_TESTS").is_err() {
        eprintln!("Skipping network test - set DEACON_NETWORK_TESTS=1 to enable");
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
