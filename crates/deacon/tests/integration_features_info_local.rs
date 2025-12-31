#![cfg(feature = "full")]
//! Integration tests for `features info` with local feature paths
//!
//! These tests verify that local feature paths are properly handled:
//! - Manifest mode should work and set canonicalId to null in JSON
//! - Other modes should fail with clear error messages

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use tempfile::TempDir;

/// Helper to create a temporary feature directory with metadata
fn create_test_feature(name: &str) -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let feature_path = temp_dir.path().join(name);
    fs::create_dir_all(&feature_path).unwrap();

    // Create devcontainer-feature.json
    let metadata = serde_json::json!({
        "id": name,
        "version": "1.0.0",
        "name": format!("Test {}", name),
        "description": format!("Test feature {}", name),
        "options": {}
    });

    fs::write(
        feature_path.join("devcontainer-feature.json"),
        serde_json::to_string_pretty(&metadata).unwrap(),
    )
    .unwrap();

    // Create install.sh
    fs::write(
        feature_path.join("install.sh"),
        "#!/bin/bash\necho 'Installing test feature'\n",
    )
    .unwrap();

    temp_dir
}

#[test]
fn test_local_manifest_text_mode() {
    let temp_dir = create_test_feature("test-local");
    let feature_path = temp_dir.path().join("test-local");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("manifest")
        .arg(feature_path.to_str().unwrap())
        .arg("--output-format")
        .arg("text");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Manifest"))
        .stdout(predicate::str::contains("Canonical Identifier"))
        .stdout(predicate::str::contains("local feature"));
}

#[test]
fn test_local_manifest_json_mode() {
    let temp_dir = create_test_feature("test-local-json");
    let feature_path = temp_dir.path().join("test-local-json");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("manifest")
        .arg(feature_path.to_str().unwrap())
        .arg("--output-format")
        .arg("json");

    let output = cmd.assert().success();

    // Parse JSON output
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let json: Value = serde_json::from_str(&stdout).expect("Should parse valid JSON");

    // Verify structure
    assert!(json.is_object(), "Output should be a JSON object");
    let obj = json.as_object().unwrap();

    // Must have manifest and canonicalId
    assert!(
        obj.contains_key("manifest"),
        "Output should contain 'manifest'"
    );
    assert!(
        obj.contains_key("canonicalId"),
        "Output should contain 'canonicalId'"
    );

    // canonicalId must be null for local features
    assert!(
        obj["canonicalId"].is_null(),
        "canonicalId should be null for local features"
    );

    // manifest should contain feature metadata
    let manifest = &obj["manifest"];
    assert!(manifest.is_object(), "manifest should be an object");
    assert_eq!(manifest["id"], "test-local-json");
    assert_eq!(manifest["version"], "1.0.0");
}

#[test]
fn test_local_tags_mode_fails() {
    let temp_dir = create_test_feature("test-local-tags");
    let feature_path = temp_dir.path().join("test-local-tags");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("tags")
        .arg(feature_path.to_str().unwrap())
        .arg("--output-format")
        .arg("text");

    cmd.assert().failure().stderr(
        predicate::str::contains("requires registry access")
            .or(predicate::str::contains("Local features only support")),
    );
}

#[test]
fn test_local_dependencies_mode_fails() {
    let temp_dir = create_test_feature("test-local-deps");
    let feature_path = temp_dir.path().join("test-local-deps");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("dependencies")
        .arg(feature_path.to_str().unwrap())
        .arg("--output-format")
        .arg("text");

    cmd.assert().failure().stderr(
        predicate::str::contains("requires registry access")
            .or(predicate::str::contains("Local features only support")),
    );
}

#[test]
fn test_local_verbose_mode_fails() {
    let temp_dir = create_test_feature("test-local-verbose");
    let feature_path = temp_dir.path().join("test-local-verbose");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("verbose")
        .arg(feature_path.to_str().unwrap())
        .arg("--output-format")
        .arg("text");

    cmd.assert().failure().stderr(
        predicate::str::contains("requires registry access")
            .or(predicate::str::contains("Local features only support")),
    );
}

#[test]
fn test_local_missing_metadata_file() {
    let temp_dir = TempDir::new().unwrap();
    let feature_path = temp_dir.path().join("no-metadata");
    fs::create_dir_all(&feature_path).unwrap();

    // Don't create devcontainer-feature.json

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("manifest")
        .arg(feature_path.to_str().unwrap())
        .arg("--output-format")
        .arg("text");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Failed to parse feature metadata"));
}

#[test]
fn test_local_invalid_metadata_json() {
    let temp_dir = TempDir::new().unwrap();
    let feature_path = temp_dir.path().join("invalid-json");
    fs::create_dir_all(&feature_path).unwrap();

    // Create invalid JSON
    fs::write(
        feature_path.join("devcontainer-feature.json"),
        "{ invalid json }",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("manifest")
        .arg(feature_path.to_str().unwrap())
        .arg("--output-format")
        .arg("json");

    let output = cmd.assert().failure();

    // JSON mode should return {} on error
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let json: Value = serde_json::from_str(&stdout).unwrap_or(serde_json::json!(null));
    assert_eq!(
        json,
        serde_json::json!({}),
        "JSON output should be {{}} on error"
    );
}

#[test]
fn test_local_relative_path() {
    let temp_dir = create_test_feature("test-relative");
    let _feature_path = temp_dir.path().join("test-relative");

    // Change to temp directory and use relative path
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("info")
        .arg("manifest")
        .arg("./test-relative")
        .arg("--output-format")
        .arg("json");

    let result = cmd.assert().success();

    // Restore original directory
    std::env::set_current_dir(original_dir).unwrap();

    // Verify canonicalId is null
    let stdout = String::from_utf8_lossy(&result.get_output().stdout);
    let json: Value = serde_json::from_str(&stdout).expect("Should parse valid JSON");
    assert!(json["canonicalId"].is_null());
}
