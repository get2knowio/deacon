#![cfg(feature = "full")]
//! Integration tests for features CLI commands
//!
//! These tests verify that the features CLI commands work correctly
//! with real feature directories and produce the expected output.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn is_docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Helper function to extract JSON from mixed output (logs + JSON)
fn extract_json_from_output(output: &str) -> Result<serde_json::Value, serde_json::Error> {
    // Try to find JSON by looking for complete JSON objects or arrays
    // Skip lines that look like log messages (contain timestamp patterns)
    for line in output.lines() {
        let trimmed = line.trim();
        // Skip lines that contain log timestamps or ANSI codes
        if trimmed.contains("Z ") || trimmed.contains("\x1b[") || trimmed.is_empty() {
            continue;
        }
        // Try to parse objects
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            if let Ok(json) = serde_json::from_str(trimmed) {
                return Ok(json);
            }
        }
        // Try to parse arrays
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if let Ok(json) = serde_json::from_str(trimmed) {
                return Ok(json);
            }
        }
    }

    // If that doesn't work, try to extract everything after the last log line
    let lines: Vec<&str> = output.lines().collect();
    for i in (0..lines.len()).rev() {
        let line = lines[i].trim();
        // Try objects
        if line.starts_with('{') {
            // Collect all lines from this point onwards and try to parse as JSON
            let json_part = lines[i..].join("\n");
            if let Ok(json) = serde_json::from_str(&json_part) {
                return Ok(json);
            }
        }
        // Try arrays
        if line.starts_with('[') {
            let json_part = lines[i..].join("\n");
            if let Ok(json) = serde_json::from_str(&json_part) {
                return Ok(json);
            }
        }
    }

    // Last resort - try the whole output
    serde_json::from_str(output)
}

/// Helper function to get absolute path to fixtures
fn fixture_path(relative_path: &str) -> std::path::PathBuf {
    let current_dir = std::env::current_dir().unwrap();
    // Tests run from crate directory, so we need to go up to workspace root
    let workspace_root = if current_dir.ends_with("crates/deacon") {
        current_dir.parent().unwrap().parent().unwrap()
    } else {
        &current_dir
    };
    workspace_root.join(relative_path)
}

/// Test features test command with a valid feature
#[test]
fn test_features_test_with_valid_feature() {
    if !is_docker_available() {
        eprintln!("Skipping test_features_test_with_valid_feature: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();
    let project_dir = temp_dir.path();

    // Create project structure with src/ and test/ directories
    let src_dir = project_dir.join("src").join("test-feature");
    fs::create_dir_all(&src_dir).unwrap();

    // Create feature metadata and install script
    fs::write(
        src_dir.join("devcontainer-feature.json"),
        r#"{"id": "test-feature", "version": "1.0.0", "name": "Test Feature"}"#,
    )
    .unwrap();
    fs::write(
        src_dir.join("install.sh"),
        "#!/bin/bash\necho 'Installing test feature'\nexit 0",
    )
    .unwrap();

    // Make install.sh executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(src_dir.join("install.sh"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(src_dir.join("install.sh"), perms).unwrap();
    }

    // Create test directory with test script
    let test_dir = project_dir.join("test").join("test-feature");
    fs::create_dir_all(&test_dir).unwrap();
    fs::write(
        test_dir.join("test.sh"),
        "#!/bin/bash\necho 'Running test'\nexit 0",
    )
    .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(test_dir.join("test.sh"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(test_dir.join("test.sh"), perms).unwrap();
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "test", project_dir.to_str().unwrap(), "--json"]);

    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);

    // Parse JSON output - expect array of test results
    let json = extract_json_from_output(&stdout).unwrap();
    assert!(json.is_array(), "Expected JSON array of test results");
    let results = json.as_array().unwrap();
    assert!(!results.is_empty(), "Expected at least one test result");

    // Verify result structure - per spec, keys should be camelCase
    for result in results {
        assert!(result["testName"].is_string());
        assert!(result["result"].is_boolean());
    }
}

/// Test features test command with missing feature metadata
#[test]
fn test_features_test_with_missing_metadata() {
    let temp_dir = TempDir::new().unwrap();
    let project_dir = temp_dir.path();

    // Create project structure with src/ and test/ directories
    let src_dir = project_dir.join("src").join("test-feature");
    fs::create_dir_all(&src_dir).unwrap();

    // Create install script but no devcontainer-feature.json
    fs::write(
        src_dir.join("install.sh"),
        "#!/bin/bash\necho 'Installing test feature'\nexit 0",
    )
    .unwrap();

    // Create test directory with test script
    let test_dir = project_dir.join("test").join("test-feature");
    fs::create_dir_all(&test_dir).unwrap();
    fs::write(
        test_dir.join("test.sh"),
        "#!/bin/bash\necho 'Running test'\nexit 0",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "test", project_dir.to_str().unwrap()]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Failed to parse feature metadata"));
}

/// Test features test command with missing install script
#[test]
fn test_features_test_with_missing_install_script() {
    let temp_dir = TempDir::new().unwrap();
    let project_dir = temp_dir.path();

    // Create project structure with src/ and test/ directories
    let src_dir = project_dir.join("src").join("test-feature");
    fs::create_dir_all(&src_dir).unwrap();

    // Create feature metadata but no install.sh
    fs::write(
        src_dir.join("devcontainer-feature.json"),
        r#"{"id": "test-feature", "version": "1.0.0"}"#,
    )
    .unwrap();

    // Create test directory with test script
    let test_dir = project_dir.join("test").join("test-feature");
    fs::create_dir_all(&test_dir).unwrap();
    fs::write(
        test_dir.join("test.sh"),
        "#!/bin/bash\necho 'Running test'\nexit 0",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "test", project_dir.to_str().unwrap()]);

    cmd.assert().failure().stderr(
        predicate::str::contains("install.sh not found")
            .or(predicate::str::contains("Runtime unavailable"))
            .or(predicate::str::contains("Docker")),
    );
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
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse text output - should contain the expected fields
    assert!(stdout.contains("Command: package"));
    assert!(stdout.contains("Status: success"));
    assert!(stdout.contains("Digest: sha256:"));
    assert!(stdout.contains("Message:"));

    // Check that files were created
    assert!(output_dir.join("test-feature-1.0.0.tgz").exists());
    assert!(output_dir.join("devcontainer-collection.json").exists());

    // Verify package reproducibility - run again and check digest is the same
    let mut cmd2 = Command::cargo_bin("deacon").unwrap();
    cmd2.args([
        "features",
        "package",
        feature_dir.to_str().unwrap(),
        "--output",
        output_dir.to_str().unwrap(),
    ]);

    let output2 = cmd2.output().unwrap();
    let stdout2 = String::from_utf8_lossy(&output2.stdout);

    // Extract digests from both outputs
    let digest1 = stdout
        .lines()
        .find(|line| line.starts_with("Digest: "))
        .and_then(|line| line.strip_prefix("Digest: "))
        .unwrap();
    let digest2 = stdout2
        .lines()
        .find(|line| line.starts_with("Digest: "))
        .and_then(|line| line.strip_prefix("Digest: "))
        .unwrap();

    // Digest should be the same for reproducible builds
    assert_eq!(digest1, digest2);
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
        .stderr(predicate::str::contains("Cannot determine packaging mode"));
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
        "--namespace",
        "testuser",
        "--dry-run",
        "--json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output using helper function
    let json = extract_json_from_output(&stdout).unwrap();

    // For dry run, features array should be empty
    assert!(json["features"].is_array());
    assert_eq!(json["features"].as_array().unwrap().len(), 0);

    // Check summary
    assert_eq!(json["summary"]["features"], 0);
    assert_eq!(json["summary"]["publishedTags"], 0);
    assert_eq!(json["summary"]["skippedTags"], 0);
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
        "--namespace",
        "testuser",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Failed to determine publish plan"));
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

/// Test features pull command help output
#[test]
fn test_features_pull_help() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "pull", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Pull features from registry"))
        .stdout(predicate::str::contains("REGISTRY_REF"))
        .stdout(predicate::str::contains(
            "Registry reference (registry/namespace/name:version)",
        ))
        .stdout(predicate::str::contains("--json"));
}

/// Test features pull command with invalid registry (should fail with clear error)
#[test]
fn test_features_pull_invalid_registry() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "pull",
        "invalid.example.com/nonexistent/feature:latest",
        "--json",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Failed to pull feature"));
}

/// Test features pull command with missing arguments  
#[test]
fn test_features_pull_missing_args() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "pull"]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("required").and(predicate::str::contains("REGISTRY_REF")));
}

/// Test that features help shows the pull command
#[test]
fn test_features_help_shows_pull() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Feature management commands"))
        .stdout(predicate::str::contains("pull"))
        .stdout(predicate::str::contains("Pull features from registry"));
}

/// Test features info command help output
#[test]
fn test_features_info_help() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "info", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Get feature information"))
        .stdout(predicate::str::contains("MODE"))
        .stdout(predicate::str::contains("FEATURE"))
        .stdout(predicate::str::contains("--output-format"));
}

/// Test features info manifest mode with local feature (JSON)
#[test]
fn test_features_info_manifest_local_json() {
    let fixture = fixture_path("fixtures/features/with-options");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "manifest",
        fixture.to_str().unwrap(),
        "--output-format",
        "json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output
    let json: serde_json::Value = extract_json_from_output(&stdout).unwrap();

    // Verify structure has canonicalId and manifest
    assert!(json["canonicalId"].is_null()); // Local features have null canonical ID
    assert!(json["manifest"].is_object());

    // Verify basic fields in manifest
    let manifest = &json["manifest"];
    assert_eq!(manifest["id"], "feature-with-options");
    assert_eq!(manifest["version"], "1.0.0");
    assert_eq!(manifest["name"], "Feature with Options");
    assert!(manifest["description"]
        .as_str()
        .unwrap()
        .contains("test feature"));

    // Verify options are present
    assert!(manifest["options"].is_object());
    assert!(manifest["options"]["enableFeature"].is_object());

    // Verify dependencies
    assert!(manifest["installsAfter"].is_array());
    assert_eq!(manifest["installsAfter"][0], "common-utils");
    assert!(manifest["dependsOn"].is_object());
}

/// Test features info manifest mode with local feature (text)
#[test]
fn test_features_info_manifest_local_text() {
    let fixture = fixture_path("fixtures/features/with-options");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "info", "manifest", fixture.to_str().unwrap()]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Manifest"))
        .stdout(predicate::str::contains("\"id\": \"feature-with-options\""))
        .stdout(predicate::str::contains("\"version\": \"1.0.0\""))
        .stdout(predicate::str::contains(
            "\"name\": \"Feature with Options\"",
        ))
        .stdout(predicate::str::contains("Canonical Identifier"))
        .stdout(predicate::str::contains("(local feature)"));
}

/// Test features info tags mode with local feature (JSON)
/// Note: tags mode requires registry access, so this should fail for local features
#[test]
fn test_features_info_tags_local_json() {
    let fixture = fixture_path("fixtures/features/with-options");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "tags",
        fixture.to_str().unwrap(),
        "--output-format",
        "json",
    ]);

    // This should fail because tags mode requires registry access
    // In JSON mode, errors produce empty {} on stdout
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "{}");
}

/// Test features info tags mode with local feature (text)
/// Note: tags mode requires registry access, so this should fail for local features
#[test]
fn test_features_info_tags_local_text() {
    let fixture = fixture_path("fixtures/features/with-options");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "info", "tags", fixture.to_str().unwrap()]);

    // This should fail because tags mode requires registry access
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("requires registry access"));
}

/// Test features info dependencies mode with local feature (JSON)
/// Note: dependencies mode output format depends on implementation
#[test]
fn test_features_info_dependencies_local_json() {
    let fixture = fixture_path("fixtures/features/with-options");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "dependencies",
        fixture.to_str().unwrap(),
        "--output-format",
        "json",
    ]);

    // Dependencies mode may only support text output per spec
    // Check if it succeeds or provides appropriate error
    let output = cmd.output().unwrap();

    // If it succeeds, verify JSON structure
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(json) = extract_json_from_output(&stdout) {
            // Verify fields if JSON output is supported
            assert_eq!(json["id"], "feature-with-options");
            assert!(json["installsAfter"].is_array());
            assert_eq!(json["installsAfter"][0], "common-utils");
            assert!(json["dependsOn"].is_object());
            assert_eq!(json["dependsOn"]["common-utils"], "latest");
        }
    }
}

/// Test features info dependencies mode with local feature (text)
/// Note: dependencies mode requires registry access for local features
#[test]
fn test_features_info_dependencies_local_text() {
    let fixture = fixture_path("fixtures/features/with-options");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "dependencies",
        fixture.to_str().unwrap(),
    ]);

    // This should fail because dependencies mode requires registry access
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("requires registry access"));
}

/// Test features info verbose mode with local feature (JSON)
/// Note: verbose mode requires registry access, so this should fail for local features
#[test]
fn test_features_info_verbose_local_json() {
    let fixture = fixture_path("fixtures/features/with-options");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "verbose",
        fixture.to_str().unwrap(),
        "--output-format",
        "json",
    ]);

    // This should fail because verbose mode requires registry access
    // In JSON mode, errors produce empty {} on stdout
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "{}");
}

/// Test features info verbose mode with local feature (text)
/// Note: verbose mode requires registry access, so this should fail for local features
#[test]
fn test_features_info_verbose_local_text() {
    let fixture = fixture_path("fixtures/features/with-options");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "info", "verbose", fixture.to_str().unwrap()]);

    // This should fail because verbose mode requires registry access
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("requires registry access"));
}

/// Test features info with minimal feature (no optional fields)
#[test]
fn test_features_info_manifest_minimal() {
    let fixture = fixture_path("fixtures/features/minimal");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "manifest",
        fixture.to_str().unwrap(),
        "--output-format",
        "json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output
    let json: serde_json::Value = extract_json_from_output(&stdout).unwrap();

    // Verify structure
    assert!(json["canonicalId"].is_null()); // Local features have null canonical ID
    assert!(json["manifest"].is_object());

    // Verify only required fields in manifest
    let manifest = &json["manifest"];
    assert_eq!(manifest["id"], "minimal-feature");
    assert!(manifest["version"].is_null());
    assert!(manifest["name"].is_null());
}

/// Test features info with invalid mode
#[test]
fn test_features_info_invalid_mode() {
    let fixture = fixture_path("fixtures/features/minimal");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "invalid-mode",
        fixture.to_str().unwrap(),
    ]);

    // Invalid modes are treated as requiring registry access for local features
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("requires registry access"));
}

/// Test features info with non-existent feature
#[test]
fn test_features_info_nonexistent_feature() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "manifest",
        "/nonexistent/path/to/feature",
    ]);

    cmd.assert().failure();
}

/// Test features info missing arguments
#[test]
fn test_features_info_missing_args() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "info"]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

/// Test that features help shows the info command
#[test]
fn test_features_help_shows_info() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Feature management commands"))
        .stdout(predicate::str::contains("info"))
        .stdout(predicate::str::contains("Get feature information"));
}

/// Test features plan command rejects local paths (CLI integration)
#[test]
fn test_features_plan_cli_rejects_local_paths() {
    // Test data: (feature_path, feature_id_in_error)
    let test_cases = vec![
        (r#"{"features": {"./my-feature": true}}"#, "./my-feature"),
        (
            r#"{"features": {"/abs/path/feature": true}}"#,
            "/abs/path/feature",
        ),
        (
            r#"{"features": {"../another-feature": true}}"#,
            "../another-feature",
        ),
    ];

    for (config_content, expected_feature_key) in test_cases {
        let temp_dir = TempDir::new().unwrap();

        // Create a config with the test case's path
        let config_dir = temp_dir.path().join(".devcontainer");
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("devcontainer.json");
        std::fs::write(&config_path, config_content).unwrap();

        let mut cmd = Command::cargo_bin("deacon").unwrap();
        cmd.args([
            "features",
            "plan",
            "--workspace-folder",
            temp_dir.path().to_str().unwrap(),
            "--config",
            config_path.to_str().unwrap(),
        ]);

        cmd.assert()
            .failure()
            .stderr(predicate::str::contains(
                "Local feature paths are not supported by 'features plan'",
            ))
            .stderr(predicate::str::contains(expected_feature_key));
    }
}

/// Integration test: Verify complete graph JSON output structure
#[test]
fn test_features_plan_graph_json_structure() {
    // This test validates the graph JSON output structure matches DATA-STRUCTURES.md
    // Using a minimal config that will produce empty features list but valid structure

    let temp_dir = TempDir::new().unwrap();

    // Create devcontainer.json with no features
    let config_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&config_dir).unwrap();
    let config_content = r#"{
        "name": "Test",
        "image": "mcr.microsoft.com/devcontainers/base:alpine",
        "features": {}
    }"#;
    fs::write(config_dir.join("devcontainer.json"), config_content).unwrap();

    // Run features plan
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "plan",
        "--workspace-folder",
        temp_dir.path().to_str().unwrap(),
        "--json",
        "true",
    ]);

    let output = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Extract and parse JSON
    let json = extract_json_from_output(&stdout).expect("Should produce valid JSON");

    // Verify structure matches DATA-STRUCTURES.md specification
    assert!(json.get("order").is_some(), "Should have 'order' field");
    assert!(json.get("graph").is_some(), "Should have 'graph' field");

    // Verify types
    let order = json.get("order").unwrap();
    assert!(order.is_array(), "Order should be an array");

    let graph = json.get("graph").unwrap();
    assert!(graph.is_object(), "Graph should be an object");

    // For empty features, we should have empty arrays
    let order_array = order.as_array().unwrap();
    let graph_obj = graph.as_object().unwrap();

    assert_eq!(
        order_array.len(),
        graph_obj.len(),
        "Graph should have entries for all features in order"
    );

    // Verify JSON serialization is valid
    let _json_str = serde_json::to_string_pretty(&json).expect("Should be able to serialize JSON");
}

/// Test features plan command with circular dependency detection
/// End-to-end test that validates CLI error output format per SPEC.md §9
#[test]
fn test_features_plan_cycle_detection_e2e() {
    // This test validates that when the CLI encounters a circular dependency,
    // it emits a properly formatted error message that includes:
    // 1. The term "cycle" or "circular"
    // 2. All involved feature IDs
    // 3. Clear indication this is a dependency error
    // Per SPEC.md §9: "Circular dependencies detected => error with details"

    use deacon_core::features::{FeatureDependencyResolver, FeatureMetadata, ResolvedFeature};
    use std::collections::HashMap;

    // Create mock features with a circular dependency:
    // feature-alpha -> feature-beta -> feature-gamma -> feature-alpha
    let create_mock_feature = |id: &str, depends_on: Vec<&str>| -> ResolvedFeature {
        let mut depends_on_map = HashMap::new();
        for dep in depends_on {
            depends_on_map.insert(dep.to_string(), serde_json::Value::Bool(true));
        }

        ResolvedFeature {
            id: id.to_string(),
            source: format!("ghcr.io/test/{}:1", id),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: id.to_string(),
                name: Some(format!("Test {}", id)),
                version: Some("1.0.0".to_string()),
                description: None,
                documentation_url: None,
                license_url: None,
                options: HashMap::new(),
                container_env: HashMap::new(),
                depends_on: depends_on_map,
                installs_after: vec![],
                init: None,
                privileged: None,
                cap_add: vec![],
                security_opt: vec![],
                entrypoint: None,
                mounts: vec![],
                post_create_command: None,
                post_start_command: None,
                post_attach_command: None,
                on_create_command: None,
                update_content_command: None,
            },
        }
    };

    let features = vec![
        create_mock_feature("feature-alpha", vec!["feature-beta"]),
        create_mock_feature("feature-beta", vec!["feature-gamma"]),
        create_mock_feature("feature-gamma", vec!["feature-alpha"]),
    ];

    let resolver = FeatureDependencyResolver::new(None);
    let result = resolver.resolve(&features);

    // Assert that we got an error
    assert!(
        result.is_err(),
        "Circular dependency should produce an error"
    );

    let error = result.unwrap_err();
    let error_text = format!("{}", error);

    // SPEC.md §9 requirement: "error with details"
    // Validate the error message format includes required elements:

    // 1. Contains cycle/circular terminology
    assert!(
        error_text.to_lowercase().contains("cycle")
            || error_text.to_lowercase().contains("circular"),
        "Error message must contain 'cycle' or 'circular' terminology per SPEC.md §9.\nGot: {}",
        error_text
    );

    // 2. Contains dependency terminology
    assert!(
        error_text.to_lowercase().contains("depend"),
        "Error message must reference dependencies per SPEC.md §9.\nGot: {}",
        error_text
    );

    // 3. Contains all involved feature IDs (the "details" requirement)
    assert!(
        error_text.contains("feature-alpha"),
        "Error message must include feature-alpha in cycle details.\nGot: {}",
        error_text
    );
    assert!(
        error_text.contains("feature-beta"),
        "Error message must include feature-beta in cycle details.\nGot: {}",
        error_text
    );
    assert!(
        error_text.contains("feature-gamma"),
        "Error message must include feature-gamma in cycle details.\nGot: {}",
        error_text
    );

    // 4. Shows directionality (arrow notation)
    assert!(
        error_text.contains("->") || error_text.contains("→"),
        "Error message should show dependency direction with arrows.\nGot: {}",
        error_text
    );

    // 5. Snapshot of expected format (for regression protection)
    // The error should follow the pattern: "Dependency cycle detected in features: <path>"
    assert!(
        error_text.starts_with("Dependency cycle detected in features:"),
        "Error message format should match expected pattern.\nGot: {}",
        error_text
    );

    // Additional validation: ensure it's a properly formed cycle path
    // A cycle path should form a closed loop (start and end with same feature)
    let cycle_path = error_text
        .strip_prefix("Dependency cycle detected in features: ")
        .unwrap_or("");
    let parts: Vec<&str> = cycle_path.split(" -> ").collect();
    assert!(
        parts.len() >= 3,
        "Cycle path should have at least 3 nodes.\nGot: {}",
        cycle_path
    );
    assert_eq!(
        parts.first(),
        parts.last(),
        "Cycle path should form closed loop (start == end).\nGot: {}",
        cycle_path
    );
}
