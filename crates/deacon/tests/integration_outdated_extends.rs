// Integration test for outdated subcommand with extends resolution
//
// Tests that features from extended configurations are properly resolved
// and included in the outdated report (addressing the gap identified in
// crates/deacon/src/commands/outdated.rs:92-141)

use assert_cmd::prelude::*;
use serde_json::Value;
use std::error::Error;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_outdated_resolves_features_from_extends() -> Result<(), Box<dyn Error>> {
    // Create a temporary workspace with a base config that extends another
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    // Base configuration with a feature
    let base_config = r#"{
      "name": "base",
      "features": {
        "ghcr.io/devcontainers/features/node:18": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("base.json"), base_config)?;

    // Main configuration that extends the base and adds another feature
    let main_config = r#"{
      "name": "main",
      "extends": ["./base.json"],
      "features": {
        "ghcr.io/devcontainers/features/python:3.11": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), main_config)?;

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

    // CRITICAL: Both features should be present:
    // - node from base.json (via extends)
    // - python from devcontainer.json
    assert_eq!(
        features.len(),
        2,
        "Expected 2 features (node from extends, python from main config), got {}",
        features.len()
    );

    // Check node feature from extended config
    let node = features.get("ghcr.io/devcontainers/features/node:18");
    assert!(
        node.is_some(),
        "Feature 'node' from extended config should be present in outdated report"
    );
    let node = node.unwrap();
    assert_eq!(node["current"], "18");
    assert_eq!(node["wanted"], "18");

    // Check python feature from main config
    let python = features.get("ghcr.io/devcontainers/features/python:3.11");
    assert!(
        python.is_some(),
        "Feature 'python' from main config should be present in outdated report"
    );
    let python = python.unwrap();
    assert_eq!(python["current"], "3.11");
    assert_eq!(python["wanted"], "3.11");

    Ok(())
}

#[test]
fn test_outdated_extends_chain_resolution() -> Result<(), Box<dyn Error>> {
    // Test a deeper extends chain: base -> intermediate -> main
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    // Base configuration
    let base_config = r#"{
      "name": "base",
      "features": {
        "ghcr.io/devcontainers/features/node:16": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("base.json"), base_config)?;

    // Intermediate configuration
    let intermediate_config = r#"{
      "name": "intermediate",
      "extends": ["./base.json"],
      "features": {
        "ghcr.io/devcontainers/features/python:3.10": {}
      }
    }"#;
    fs::write(
        devcontainer_dir.join("intermediate.json"),
        intermediate_config,
    )?;

    // Main configuration
    let main_config = r#"{
      "name": "main",
      "extends": ["./intermediate.json"],
      "features": {
        "ghcr.io/devcontainers/features/docker-in-docker:2": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), main_config)?;

    // Force OCI client failure
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

    let features = parsed["features"].as_object().unwrap();

    // All three features should be present
    assert_eq!(
        features.len(),
        3,
        "Expected 3 features from extends chain, got {}",
        features.len()
    );

    // Verify all three features
    assert!(
        features.contains_key("ghcr.io/devcontainers/features/node:16"),
        "Feature 'node' from base should be present"
    );
    assert!(
        features.contains_key("ghcr.io/devcontainers/features/python:3.10"),
        "Feature 'python' from intermediate should be present"
    );
    assert!(
        features.contains_key("ghcr.io/devcontainers/features/docker-in-docker:2"),
        "Feature 'docker-in-docker' from main should be present"
    );

    Ok(())
}

#[test]
fn test_outdated_extends_feature_override() -> Result<(), Box<dyn Error>> {
    // Test that features in the main config override those in extended configs
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    // Base configuration with node:16
    let base_config = r#"{
      "name": "base",
      "features": {
        "ghcr.io/devcontainers/features/node:16": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("base.json"), base_config)?;

    // Main configuration that overrides with node:18
    let main_config = r#"{
      "name": "main",
      "extends": ["./base.json"],
      "features": {
        "ghcr.io/devcontainers/features/node:18": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), main_config)?;

    // Force OCI client failure
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

    let features = parsed["features"].as_object().unwrap();

    // The child's tag overrides the base's: exactly one node entry, at the child's version.
    //
    // This assertion has been round the houses and the history is worth keeping. It
    // originally read "exactly one node feature (the override)" and passed — but only
    // because `outdated` keyed its report by the CANONICAL untagged id, so `node:16` and
    // `node:18` collapsed onto one key and the last won. That LOOKED like override
    // semantics while the merge was actually producing both. Fixing the report key (#407)
    // removed the collapse and exposed the truth, and this test was rewritten to record
    // it: two entries, both installed, which is what #411 filed. Merging `features` by
    // canonical Feature id (#411) makes the override real, so the original expectation is
    // correct again — this time because the merge does it, not because a reporting bug
    // hid it.
    let node_features: Vec<_> = features.keys().filter(|k| k.contains("node")).collect();
    assert_eq!(
        node_features.len(),
        1,
        "the child's node tag must override the base's; got {node_features:?}"
    );

    let node = features
        .get("ghcr.io/devcontainers/features/node:18")
        .expect("the surviving entry must be the CHILD's version, keyed by its declared ref");
    assert_eq!(
        node["current"], "18",
        "node version should be 18 (from the main config override), not 16 (from the base)"
    );
    assert!(
        !features.contains_key("ghcr.io/devcontainers/features/node:16"),
        "the base's superseded node:16 must not survive the merge"
    );

    Ok(())
}

#[test]
fn test_outdated_text_output_with_extends() -> Result<(), Box<dyn Error>> {
    // Test text output format also includes features from extends
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    // Base configuration
    let base_config = r#"{
      "name": "base",
      "features": {
        "ghcr.io/devcontainers/features/node:18": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("base.json"), base_config)?;

    // Main configuration
    let main_config = r#"{
      "name": "main",
      "extends": ["./base.json"],
      "features": {
        "ghcr.io/devcontainers/features/python:3.11": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), main_config)?;

    // Force OCI client failure
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

    let text_output = String::from_utf8(output.stdout)?;

    // Verify both features appear in text output
    assert!(
        text_output.contains("ghcr.io/devcontainers/features/node:18"),
        "Text output should include node feature from extends"
    );
    assert!(
        text_output.contains("ghcr.io/devcontainers/features/python:3.11"),
        "Text output should include python feature from main config"
    );

    Ok(())
}
