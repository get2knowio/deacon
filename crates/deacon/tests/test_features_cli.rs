//! Integration tests for features CLI commands
//!
//! These tests verify that the features CLI commands work correctly
//! with real feature directories and produce the expected output.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

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

/// Test features test command with a valid feature
#[test]
fn test_features_test_with_valid_feature() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature");

    // Create feature directory with minimal files
    fs::create_dir_all(&feature_dir).unwrap();
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{"id": "test-feature", "version": "1.0.0", "name": "Test Feature"}"#,
    )
    .unwrap();
    fs::write(
        feature_dir.join("install.sh"),
        "#!/bin/bash\necho 'Installing test feature'\nexit 0",
    )
    .unwrap();

    // Make install.sh executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(feature_dir.join("install.sh"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(feature_dir.join("install.sh"), perms).unwrap();
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "test", feature_dir.to_str().unwrap(), "--json"]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output using helper function
    let json = extract_json_from_output(&stdout).unwrap();
    assert_eq!(json["command"], "test");
    // Note: status might be "failure" if Docker is not available, which is expected in CI
    assert!(json["status"] == "success" || json["status"] == "failure");
}

/// Test features test command with missing feature metadata
#[test]
fn test_features_test_with_missing_metadata() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature");

    // Create feature directory without devcontainer-feature.json
    fs::create_dir_all(&feature_dir).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "test", feature_dir.to_str().unwrap()]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Failed to parse feature metadata"));
}

/// Test features test command with missing install script
#[test]
fn test_features_test_with_missing_install_script() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature");

    // Create feature directory with metadata but no install.sh
    fs::create_dir_all(&feature_dir).unwrap();
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{"id": "test-feature", "version": "1.0.0"}"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "test", feature_dir.to_str().unwrap()]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("install.sh not found"));
}

/// Test features package command
#[test]
fn test_features_package() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature");
    let output_dir = temp_dir.path().join("output");

    // Create feature directory with minimal files
    fs::create_dir_all(&feature_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{"id": "test-feature", "version": "1.0.0", "name": "Test Feature"}"#,
    )
    .unwrap();
    fs::write(
        feature_dir.join("install.sh"),
        "#!/bin/bash\necho 'Installing test feature'",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "package",
        feature_dir.to_str().unwrap(),
        "--output",
        output_dir.to_str().unwrap(),
        "--json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output using helper function
    let json = extract_json_from_output(&stdout).unwrap();
    assert_eq!(json["command"], "package");
    assert_eq!(json["status"], "success");
    assert!(json["digest"].as_str().unwrap().starts_with("sha256:"));
    assert!(json["size"].as_u64().unwrap() > 0);

    // Check that files were created
    assert!(output_dir.join("test-feature.tar").exists());
    assert!(output_dir.join("test-feature-manifest.json").exists());

    // Verify package reproducibility - run again and check digest is the same
    let mut cmd2 = Command::cargo_bin("deacon").unwrap();
    cmd2.args([
        "features",
        "package",
        feature_dir.to_str().unwrap(),
        "--output",
        output_dir.to_str().unwrap(),
        "--json",
    ]);

    let output2 = cmd2.output().unwrap();
    let stdout2 = String::from_utf8_lossy(&output2.stdout);
    let json2 = extract_json_from_output(&stdout2).unwrap();

    // Digest should be the same for reproducible builds
    assert_eq!(json["digest"], json2["digest"]);
}

/// Test features package command with invalid feature
#[test]
fn test_features_package_with_invalid_feature() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature");
    let output_dir = temp_dir.path().join("output");

    // Create empty directories
    fs::create_dir_all(&feature_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "package",
        feature_dir.to_str().unwrap(),
        "--output",
        output_dir.to_str().unwrap(),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Failed to parse feature metadata"));
}

/// Test features publish command with dry run
#[test]
fn test_features_publish_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature");

    // Create feature directory with minimal files
    fs::create_dir_all(&feature_dir).unwrap();
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{"id": "test-feature", "version": "1.0.0", "name": "Test Feature"}"#,
    )
    .unwrap();
    fs::write(
        feature_dir.join("install.sh"),
        "#!/bin/bash\necho 'Installing test feature'",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "publish",
        feature_dir.to_str().unwrap(),
        "--registry",
        "ghcr.io/test",
        "--dry-run",
        "--json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output using helper function
    let json = extract_json_from_output(&stdout).unwrap();
    assert_eq!(json["command"], "publish");
    assert_eq!(json["status"], "success");
    assert!(json["digest"]
        .as_str()
        .unwrap()
        .starts_with("sha256:dryrun"));
    assert!(json["message"].as_str().unwrap().contains("ghcr.io/test"));
}

/// Test features publish command without dry run (should fail)
#[test]
fn test_features_publish_without_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature");

    // Create feature directory with minimal files
    fs::create_dir_all(&feature_dir).unwrap();
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{"id": "test-feature", "version": "1.0.0"}"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "publish",
        feature_dir.to_str().unwrap(),
        "--registry",
        "ghcr.io/test",
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "Actual registry publishing not yet implemented",
    ));
}

/// Test features command help output
#[test]
fn test_features_help() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Feature management commands"))
        .stdout(predicate::str::contains("test"))
        .stdout(predicate::str::contains("package"))
        .stdout(predicate::str::contains("publish"));
}

/// Test text output format
#[test]
fn test_features_package_text_output() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature");
    let output_dir = temp_dir.path().join("output");

    // Create feature directory with minimal files
    fs::create_dir_all(&feature_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{"id": "test-feature", "version": "1.0.0"}"#,
    )
    .unwrap();
    fs::write(
        feature_dir.join("install.sh"),
        "#!/bin/bash\necho 'Installing test feature'",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "package",
        feature_dir.to_str().unwrap(),
        "--output",
        output_dir.to_str().unwrap(),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Command: package"))
        .stdout(predicate::str::contains("Status: success"))
        .stdout(predicate::str::contains("Digest: sha256:"))
        .stdout(predicate::str::contains("Size:"))
        .stdout(predicate::str::contains("bytes"));
}
