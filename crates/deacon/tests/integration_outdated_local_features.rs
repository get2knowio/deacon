#![cfg(feature = "full")]
// Integration test for filtering out local/non-OCI feature references
//
// Per spec §9 and §14: Invalid/legacy feature identifiers (local paths, unknown schemes)
// are skipped, not errors. Outdated focuses on versionable OCI identifiers.

use assert_cmd::prelude::*;
use serde_json::Value;
use std::error::Error;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_outdated_skips_local_features() -> Result<(), Box<dyn Error>> {
    // Create a temporary workspace with mixed OCI and local features
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    // Config with both OCI features and local features
    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:18": {},
        "./local-feature": {},
        "../relative/path": {},
        "ghcr.io/devcontainers/features/python:3.11": {},
        "https://example.com/feature.tgz": {},
        "/absolute/path": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    // Force OCI client failure to make fetch_latest_stable_version return None
    let nonexist = td.path().join("nonexistent-ca.pem");

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    cmd.arg("outdated")
        .arg("--workspace-folder")
        .arg(td.path())
        .arg("--output")
        .arg("json");

    let output = cmd.output()?;
    assert!(output.status.success());

    // Parse JSON output
    let json_str = String::from_utf8(output.stdout)?;
    let parsed: Value = serde_json::from_str(&json_str)?;

    // Verify structure
    assert!(parsed.is_object());
    assert!(parsed.get("features").is_some());

    let features = parsed["features"].as_object().unwrap();

    // Should only have 2 features (the OCI ones), not 6
    assert_eq!(features.len(), 2, "Only OCI features should be in output");

    // Check that OCI features are present
    assert!(features.contains_key("ghcr.io/devcontainers/features/node"));
    assert!(features.contains_key("ghcr.io/devcontainers/features/python"));

    // Check that local/non-OCI features are NOT present
    assert!(!features.contains_key("./local-feature"));
    assert!(!features.contains_key("../relative/path"));
    assert!(!features.contains_key("https://example.com/feature.tgz"));
    assert!(!features.contains_key("/absolute/path"));

    Ok(())
}

#[test]
fn test_outdated_text_skips_local_features() -> Result<(), Box<dyn Error>> {
    // Test text output also skips local features
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{
      "features": {
        "./feature-a": {},
        "ghcr.io/devcontainers/features/node:18": {},
        "./feature-b": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    let nonexist = td.path().join("nonexistent-ca.pem");

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    cmd.arg("outdated")
        .arg("--workspace-folder")
        .arg(td.path())
        .arg("--output")
        .arg("text");

    let output = cmd.output()?;
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout)?;

    // Check that only the OCI feature appears in output
    assert!(stdout.contains("ghcr.io/devcontainers/features/node"));

    // Check that local features don't appear
    assert!(!stdout.contains("./feature-a"));
    assert!(!stdout.contains("./feature-b"));

    Ok(())
}

#[test]
fn test_outdated_all_local_features() -> Result<(), Box<dyn Error>> {
    // Test with only local features (should return empty features object)
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{
      "features": {
        "./feature-a": {},
        "./feature-b": {},
        "./feature-c": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.arg("outdated")
        .arg("--workspace-folder")
        .arg(td.path())
        .arg("--output")
        .arg("json");

    let output = cmd.output()?;
    assert!(output.status.success());

    let json_str = String::from_utf8(output.stdout)?;
    let parsed: Value = serde_json::from_str(&json_str)?;

    let features = parsed["features"].as_object().unwrap();
    assert_eq!(
        features.len(),
        0,
        "All local features should be filtered out"
    );

    Ok(())
}
