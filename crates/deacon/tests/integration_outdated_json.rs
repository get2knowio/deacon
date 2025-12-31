#![cfg(feature = "full")]
// T040: CLI integration test for JSON output (User Story 2)
//
// Tests JSON serialization, schema stability, and compact/pretty formatting

use assert_cmd::prelude::*;
use serde_json::Value;
use std::error::Error;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_outdated_json_output_format() -> Result<(), Box<dyn Error>> {
    // Create a temporary workspace with features
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:18": {},
        "ghcr.io/devcontainers/features/python:3.11": {}
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

    // Verify structure: { features: { "canonical-id": { current, wanted, latest, ... } } }
    assert!(parsed.is_object());
    assert!(parsed.get("features").is_some());

    let features = parsed["features"].as_object().unwrap();
    assert_eq!(features.len(), 2);

    // Check node feature
    let node = features.get("ghcr.io/devcontainers/features/node").unwrap();
    assert_eq!(node["current"], "18");
    assert_eq!(node["wanted"], "18");
    // latest should be null since we forced OCI failure
    assert!(node["latest"].is_null());

    // Check python feature
    let python = features
        .get("ghcr.io/devcontainers/features/python")
        .unwrap();
    assert_eq!(python["current"], "3.11");
    assert_eq!(python["wanted"], "3.11");
    assert!(python["latest"].is_null());

    Ok(())
}

#[test]
fn test_outdated_json_empty_features() -> Result<(), Box<dyn Error>> {
    // Test with no features declared
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{ "features": {} }"#;
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
    assert_eq!(features.len(), 0);

    Ok(())
}

#[test]
fn test_outdated_json_preserves_declaration_order() -> Result<(), Box<dyn Error>> {
    // Test that JSON output preserves declaration order (not alphabetical)
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    // Deliberately order features alphabetically differently from expected
    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/zzz:1": {},
        "ghcr.io/devcontainers/features/aaa:1": {},
        "ghcr.io/devcontainers/features/mmm:1": {}
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
        .arg("json");

    let output = cmd.output()?;
    assert!(output.status.success());

    let json_str = String::from_utf8(output.stdout)?;
    let parsed: Value = serde_json::from_str(&json_str)?;

    let features = parsed["features"].as_object().unwrap();
    assert_eq!(features.len(), 3);

    // Verify declaration order is preserved (zzz, aaa, mmm), not alphabetical (aaa, mmm, zzz)
    let keys: Vec<&String> = features.keys().collect();
    assert_eq!(keys.len(), 3);
    assert_eq!(keys[0], "ghcr.io/devcontainers/features/zzz");
    assert_eq!(keys[1], "ghcr.io/devcontainers/features/aaa");
    assert_eq!(keys[2], "ghcr.io/devcontainers/features/mmm");

    Ok(())
}

#[test]
fn test_outdated_json_schema_stability() -> Result<(), Box<dyn Error>> {
    // Verify that all expected fields are present (even if null)
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:18": {}
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
        .arg("json");

    let output = cmd.output()?;
    assert!(output.status.success());

    let json_str = String::from_utf8(output.stdout)?;
    let parsed: Value = serde_json::from_str(&json_str)?;

    let node = &parsed["features"]["ghcr.io/devcontainers/features/node"];

    // Verify expected fields exist
    assert!(node.get("current").is_some());
    assert!(node.get("wanted").is_some());
    assert!(node.get("latest").is_some());
    assert!(node.get("wantedMajor").is_some());
    assert!(node.get("latestMajor").is_some());

    Ok(())
}

#[test]
fn test_outdated_json_non_interactive_compact() -> Result<(), Box<dyn Error>> {
    // Non-interactive (CI) should produce compact JSON
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:18": {}
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
        .arg("json");

    let output = cmd.output()?;
    assert!(output.status.success());

    let json_str = String::from_utf8(output.stdout)?;

    // In non-TTY environment (like test), output should be compact (single line or minimal whitespace)
    // We can verify it's valid JSON and reasonably compact
    let _parsed: Value = serde_json::from_str(&json_str)?;

    // Compact JSON should not have excessive newlines (pretty format would have many)
    let newline_count = json_str.matches('\n').count();
    // Compact format might have 0-1 newlines; pretty would have many more
    // This is a heuristic check
    assert!(
        newline_count < 5,
        "Expected compact JSON but got {} newlines",
        newline_count
    );

    Ok(())
}
