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

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Failed to publish feature"));
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
        .stdout(predicate::str::contains("--json"));
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
        "--json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output
    let json: serde_json::Value = extract_json_from_output(&stdout).unwrap();

    // Verify basic fields
    assert_eq!(json["id"], "feature-with-options");
    assert_eq!(json["version"], "1.0.0");
    assert_eq!(json["name"], "Feature with Options");
    assert!(json["description"]
        .as_str()
        .unwrap()
        .contains("test feature"));

    // Verify options are present
    assert!(json["options"].is_object());
    assert!(json["options"]["enableFeature"].is_object());

    // Verify dependencies
    assert!(json["installsAfter"].is_array());
    assert_eq!(json["installsAfter"][0], "common-utils");
    assert!(json["dependsOn"].is_object());
}

/// Test features info manifest mode with local feature (text)
#[test]
fn test_features_info_manifest_local_text() {
    let fixture = fixture_path("fixtures/features/with-options");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "info", "manifest", fixture.to_str().unwrap()]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Feature Manifest:"))
        .stdout(predicate::str::contains("ID: feature-with-options"))
        .stdout(predicate::str::contains("Version: 1.0.0"))
        .stdout(predicate::str::contains("Name: Feature with Options"));
}

/// Test features info tags mode with local feature (JSON)
#[test]
fn test_features_info_tags_local_json() {
    let fixture = fixture_path("fixtures/features/with-options");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "tags",
        fixture.to_str().unwrap(),
        "--json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output
    let json: serde_json::Value = extract_json_from_output(&stdout).unwrap();

    // Verify fields
    assert_eq!(json["id"], "feature-with-options");
    assert!(json["tags"].is_array());
    assert_eq!(json["tags"][0], "1.0.0");
}

/// Test features info tags mode with local feature (text)
#[test]
fn test_features_info_tags_local_text() {
    let fixture = fixture_path("fixtures/features/with-options");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "info", "tags", fixture.to_str().unwrap()]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Available Tags"))
        .stdout(predicate::str::contains("feature-with-options"))
        .stdout(predicate::str::contains("1.0.0"));
}

/// Test features info dependencies mode with local feature (JSON)
#[test]
fn test_features_info_dependencies_local_json() {
    let fixture = fixture_path("fixtures/features/with-options");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "dependencies",
        fixture.to_str().unwrap(),
        "--json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output
    let json: serde_json::Value = extract_json_from_output(&stdout).unwrap();

    // Verify fields
    assert_eq!(json["id"], "feature-with-options");
    assert!(json["installsAfter"].is_array());
    assert_eq!(json["installsAfter"][0], "common-utils");
    assert!(json["dependsOn"].is_object());
    assert_eq!(json["dependsOn"]["common-utils"], "latest");
}

/// Test features info dependencies mode with local feature (text)
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

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Dependencies for"))
        .stdout(predicate::str::contains("feature-with-options"))
        .stdout(predicate::str::contains("Installs After:"))
        .stdout(predicate::str::contains("common-utils"))
        .stdout(predicate::str::contains("Depends On:"))
        .stdout(predicate::str::contains("latest"));
}

/// Test features info verbose mode with local feature (JSON)
#[test]
fn test_features_info_verbose_local_json() {
    let fixture = fixture_path("fixtures/features/with-options");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "info",
        "verbose",
        fixture.to_str().unwrap(),
        "--json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output
    let json: serde_json::Value = extract_json_from_output(&stdout).unwrap();

    // Verify all fields are present
    assert_eq!(json["id"], "feature-with-options");
    assert_eq!(json["version"], "1.0.0");
    assert_eq!(json["name"], "Feature with Options");
    assert!(json["options"].is_object());
    assert!(json["containerEnv"].is_object());
    assert!(json["mounts"].is_array());
    assert_eq!(json["init"], true);
    assert_eq!(json["privileged"], false);
    assert!(json["capAdd"].is_array());
    assert!(json["securityOpt"].is_array());
    assert!(json["installsAfter"].is_array());
    assert!(json["dependsOn"].is_object());
}

/// Test features info verbose mode with local feature (text)
#[test]
fn test_features_info_verbose_local_text() {
    let fixture = fixture_path("fixtures/features/with-options");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["features", "info", "verbose", fixture.to_str().unwrap()]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Feature Information (Verbose)"))
        .stdout(predicate::str::contains("Basic Information:"))
        .stdout(predicate::str::contains("ID: feature-with-options"))
        .stdout(predicate::str::contains("Version: 1.0.0"))
        .stdout(predicate::str::contains("Options:"))
        .stdout(predicate::str::contains("Dependencies:"))
        .stdout(predicate::str::contains("Container Environment Variables:"))
        .stdout(predicate::str::contains("Mounts:"))
        .stdout(predicate::str::contains("Container Options:"))
        .stdout(predicate::str::contains("Lifecycle Commands:"));
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
        "--json",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output
    let json: serde_json::Value = extract_json_from_output(&stdout).unwrap();

    // Verify only required fields
    assert_eq!(json["id"], "minimal-feature");
    assert!(json["version"].is_null());
    assert!(json["name"].is_null());
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

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Invalid mode"))
        .stderr(predicate::str::contains(
            "manifest, tags, dependencies, verbose",
        ));
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
                "Local features are not supported by 'features plan'",
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
