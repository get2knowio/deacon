#![cfg(feature = "full")]
//! Integration tests for features package command
//!
//! These tests verify that the features package command works end-to-end
//! with real file system operations and produces the expected artifacts.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Test single feature packaging produces correct artifacts
#[test]
fn test_single_feature_packaging_artifacts() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature");
    let output_dir = temp_dir.path().join("output");

    // Create feature directory with minimal required files
    fs::create_dir_all(&feature_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    // Create devcontainer-feature.json
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{
            "id": "test-feature",
            "version": "1.0.0",
            "name": "Test Feature",
            "description": "A test feature for integration testing"
        }"#,
    )
    .unwrap();

    // Create install.sh script
    fs::write(
        feature_dir.join("install.sh"),
        "#!/bin/bash\nset -e\necho 'Installing test feature'\n",
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

    // Run package command
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
        .stdout(predicate::str::contains("Size:"));

    // Verify artifacts were created
    assert!(output_dir.join("test-feature-1.0.0.tgz").exists());
    assert!(output_dir.join("devcontainer-collection.json").exists());

    // Verify .tgz file is not empty
    let tgz_metadata = fs::metadata(output_dir.join("test-feature-1.0.0.tgz")).unwrap();
    assert!(tgz_metadata.len() > 0);

    // Verify devcontainer-collection.json content
    let collection_content =
        fs::read_to_string(output_dir.join("devcontainer-collection.json")).unwrap();
    let collection_json: serde_json::Value = serde_json::from_str(&collection_content).unwrap();

    // Verify collection structure
    assert_eq!(
        collection_json["sourceInformation"]["source"],
        "devcontainer-cli"
    );
    assert!(collection_json["features"].is_object());
    assert!(collection_json["features"]["test-feature"].is_object());

    let feature = &collection_json["features"]["test-feature"];
    assert_eq!(feature["id"], "test-feature");
    assert_eq!(feature["version"], "1.0.0");
    assert_eq!(feature["name"], "Test Feature");
}

/// Test packaging with default output directory
#[test]
fn test_packaging_with_default_output() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature");

    // Create feature directory
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

    // Change to temp directory and run package with default output
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(&temp_dir)
        .args(["features", "package", feature_dir.to_str().unwrap()]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Command: package"))
        .stdout(predicate::str::contains("Status: success"));

    // Verify ./output directory was created with artifacts
    let output_dir = temp_dir.path().join("output");
    assert!(output_dir.exists());
    assert!(output_dir.join("test-feature-1.0.0.tgz").exists());
    assert!(output_dir.join("devcontainer-collection.json").exists());
}

/// Test packaging with omitted target path (defaults to current directory)
#[test]
fn test_packaging_with_default_target_path() {
    let temp_dir = TempDir::new().unwrap();

    // Create feature files directly in temp directory (simulating current directory)
    fs::write(
        temp_dir.path().join("devcontainer-feature.json"),
        r#"{
            "id": "current-dir-feature",
            "version": "1.0.0",
            "name": "Current Directory Feature",
            "description": "A feature in the current directory"
        }"#,
    )
    .unwrap();

    fs::write(
        temp_dir.path().join("install.sh"),
        "#!/bin/bash\necho 'Installing current directory feature'",
    )
    .unwrap();

    // Run package command without specifying target path (should default to ".")
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(&temp_dir).args(["features", "package"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Command: package"))
        .stdout(predicate::str::contains("Status: success"))
        .stdout(predicate::str::contains("Mode: single"))
        .stdout(predicate::str::contains("Created artifacts:"))
        .stdout(predicate::str::contains("current-dir-feature-1.0.0.tgz"))
        .stdout(predicate::str::contains("devcontainer-collection.json"));

    // Verify ./output directory was created with artifacts
    let output_dir = temp_dir.path().join("output");
    assert!(output_dir.exists());
    assert!(output_dir.join("current-dir-feature-1.0.0.tgz").exists());
    assert!(output_dir.join("devcontainer-collection.json").exists());
}

/// Test packaging with feature that has options
#[test]
fn test_packaging_feature_with_options() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("feature-with-options");
    let output_dir = temp_dir.path().join("output");

    // Create feature directory
    fs::create_dir_all(&feature_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    // Create devcontainer-feature.json with options
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{
            "id": "feature-with-options",
            "version": "1.0.0",
            "name": "Feature with Options",
            "description": "A feature with configurable options",
            "options": {
                "enableFeature": {
                    "type": "boolean",
                    "default": true,
                    "description": "Enable the feature"
                },
                "version": {
                    "type": "string",
                    "default": "latest",
                    "description": "Version to install"
                }
            }
        }"#,
    )
    .unwrap();

    fs::write(
        feature_dir.join("install.sh"),
        "#!/bin/bash\necho 'Installing feature with options'",
    )
    .unwrap();

    // Run package command
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "package",
        feature_dir.to_str().unwrap(),
        "--output",
        output_dir.to_str().unwrap(),
    ]);

    cmd.assert().success();

    // Verify collection.json includes options
    let collection_content =
        fs::read_to_string(output_dir.join("devcontainer-collection.json")).unwrap();
    let collection_json: serde_json::Value = serde_json::from_str(&collection_content).unwrap();

    let feature = &collection_json["features"]["feature-with-options"];
    assert!(feature["options"].is_object());
    assert!(feature["options"]["enableFeature"].is_object());
    assert!(feature["options"]["version"].is_object());
}

/// Test packaging fails with invalid feature metadata
#[test]
fn test_packaging_invalid_feature_fails() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("invalid-feature");
    let output_dir = temp_dir.path().join("output");

    // Create directory without devcontainer-feature.json
    fs::create_dir_all(&feature_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    // Create invalid JSON file
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{"id": "invalid", "version": }"#,
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

    cmd.assert().failure().stderr(predicate::str::contains(
        "Failed to parse devcontainer-feature.json",
    ));
}

/// Test packaging succeeds with missing install script (not required)
#[test]
fn test_packaging_missing_install_script_succeeds() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("missing-script-feature");
    let output_dir = temp_dir.path().join("output");

    // Create directory with only devcontainer-feature.json
    fs::create_dir_all(&feature_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{"id": "missing-script", "version": "1.0.0", "name": "Missing Script"}"#,
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

    cmd.assert().success();

    // Verify artifacts were still created
    assert!(output_dir.join("missing-script-1.0.0.tgz").exists());
    assert!(output_dir.join("devcontainer-collection.json").exists());
}

/// Test packaging with feature containing multiple files
#[test]
fn test_packaging_feature_with_multiple_files() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("multi-file-feature");
    let output_dir = temp_dir.path().join("output");

    // Create feature directory with multiple files
    fs::create_dir_all(&feature_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    // Create devcontainer-feature.json
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{"id": "multi-file-feature", "version": "1.0.0", "name": "Multi File Feature"}"#,
    )
    .unwrap();

    // Create install.sh
    fs::write(
        feature_dir.join("install.sh"),
        "#!/bin/bash\necho 'Installing multi-file feature'",
    )
    .unwrap();

    // Create additional files
    fs::write(
        feature_dir.join("README.md"),
        "# Multi File Feature\n\nThis is a test feature.",
    )
    .unwrap();
    fs::write(feature_dir.join("config.json"), r#"{"setting": "value"}"#).unwrap();

    // Create subdirectory with file
    fs::create_dir_all(feature_dir.join("scripts")).unwrap();
    fs::write(
        feature_dir.join("scripts/helper.sh"),
        "#!/bin/bash\necho 'Helper script'",
    )
    .unwrap();

    // Run package command
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "package",
        feature_dir.to_str().unwrap(),
        "--output",
        output_dir.to_str().unwrap(),
    ]);

    cmd.assert().success();

    // Verify .tgz was created and is reasonably sized (contains multiple files)
    let tgz_path = output_dir.join("multi-file-feature-1.0.0.tgz");
    assert!(tgz_path.exists());
    let tgz_metadata = fs::metadata(&tgz_path).unwrap();
    assert!(tgz_metadata.len() > 100); // Should be reasonably sized with multiple files

    // Verify collection.json was created
    assert!(output_dir.join("devcontainer-collection.json").exists());
}

/// Test collection packaging produces correct artifacts for multiple features
#[test]
fn test_collection_packaging_artifacts() {
    let temp_dir = TempDir::new().unwrap();
    let collection_dir = temp_dir.path().join("collection");
    let src_dir = collection_dir.join("src");
    let output_dir = temp_dir.path().join("output");

    // Create collection directory structure
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    // Create first feature
    let feature1_dir = src_dir.join("feature-a");
    fs::create_dir_all(&feature1_dir).unwrap();
    fs::write(
        feature1_dir.join("devcontainer-feature.json"),
        r#"{
            "id": "feature-a",
            "version": "1.0.0",
            "name": "Feature A",
            "description": "First test feature"
        }"#,
    )
    .unwrap();
    fs::write(
        feature1_dir.join("install.sh"),
        "#!/bin/bash\necho 'Installing feature A'",
    )
    .unwrap();

    // Create second feature
    let feature2_dir = src_dir.join("feature-b");
    fs::create_dir_all(&feature2_dir).unwrap();
    fs::write(
        feature2_dir.join("devcontainer-feature.json"),
        r#"{
            "id": "feature-b",
            "version": "2.0.0",
            "name": "Feature B",
            "description": "Second test feature"
        }"#,
    )
    .unwrap();
    fs::write(
        feature2_dir.join("install.sh"),
        "#!/bin/bash\necho 'Installing feature B'",
    )
    .unwrap();

    // Run package command on collection
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "package",
        collection_dir.to_str().unwrap(),
        "--output",
        output_dir.to_str().unwrap(),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Command: package"))
        .stdout(predicate::str::contains("Status: success"))
        .stdout(predicate::str::contains("Size:"));

    // Verify artifacts were created
    assert!(output_dir.join("feature-a-1.0.0.tgz").exists());
    assert!(output_dir.join("feature-b-2.0.0.tgz").exists());
    assert!(output_dir.join("devcontainer-collection.json").exists());

    // Verify .tgz files are not empty
    let tgz1_metadata = fs::metadata(output_dir.join("feature-a-1.0.0.tgz")).unwrap();
    let tgz2_metadata = fs::metadata(output_dir.join("feature-b-2.0.0.tgz")).unwrap();
    assert!(tgz1_metadata.len() > 0);
    assert!(tgz2_metadata.len() > 0);

    // Verify devcontainer-collection.json content
    let collection_content =
        fs::read_to_string(output_dir.join("devcontainer-collection.json")).unwrap();
    let collection_json: serde_json::Value = serde_json::from_str(&collection_content).unwrap();

    // Verify collection structure
    assert_eq!(
        collection_json["sourceInformation"]["source"],
        "devcontainer-cli"
    );
    assert!(collection_json["features"].is_object());
    let features = collection_json["features"].as_object().unwrap();
    assert_eq!(features.len(), 2);

    // Verify feature-a
    let feature_a = &features["feature-a"];
    assert_eq!(feature_a["id"], "feature-a");
    assert_eq!(feature_a["version"], "1.0.0");
    assert_eq!(feature_a["name"], "Feature A");

    // Verify feature-b
    let feature_b = &features["feature-b"];
    assert_eq!(feature_b["id"], "feature-b");
    assert_eq!(feature_b["version"], "2.0.0");
    assert_eq!(feature_b["name"], "Feature B");
}

/// Test collection packaging fails with mixed valid/invalid subfolders
#[test]
fn test_collection_packaging_mixed_valid_invalid_fails() {
    let temp_dir = TempDir::new().unwrap();
    let collection_dir = temp_dir.path().join("mixed-collection");
    let src_dir = collection_dir.join("src");
    let output_dir = temp_dir.path().join("output");

    // Create collection directory structure
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    // Create valid feature
    let valid_feature_dir = src_dir.join("valid-feature");
    fs::create_dir_all(&valid_feature_dir).unwrap();
    fs::write(
        valid_feature_dir.join("devcontainer-feature.json"),
        r#"{
            "id": "valid-feature",
            "version": "1.0.0",
            "name": "Valid Feature",
            "description": "A valid feature"
        }"#,
    )
    .unwrap();
    fs::write(
        valid_feature_dir.join("install.sh"),
        "#!/bin/bash\necho 'Installing valid feature'",
    )
    .unwrap();

    // Create invalid feature (missing devcontainer-feature.json)
    let invalid_feature_dir = src_dir.join("invalid-feature");
    fs::create_dir_all(&invalid_feature_dir).unwrap();
    // Don't create devcontainer-feature.json - this makes it invalid

    // Create another invalid feature (corrupt JSON)
    let corrupt_feature_dir = src_dir.join("corrupt-feature");
    fs::create_dir_all(&corrupt_feature_dir).unwrap();
    fs::write(
        corrupt_feature_dir.join("devcontainer-feature.json"),
        r#"{"id": "corrupt", "version": }"#,
    )
    .unwrap();

    // Run package command on collection - should fail
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "package",
        collection_dir.to_str().unwrap(),
        "--output",
        output_dir.to_str().unwrap(),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Found 2 invalid feature(s)"))
        .stderr(predicate::str::contains("invalid-feature"))
        .stderr(predicate::str::contains("corrupt-feature"));

    // Verify no artifacts were created (since packaging should fail)
    assert!(!output_dir.join("valid-feature-1.0.0.tgz").exists());
    assert!(!output_dir.join("devcontainer-collection.json").exists());
}

/// Test that package command rejects global JSON log format
#[test]
fn test_package_command_rejects_global_json_log_format() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature");
    let output_dir = temp_dir.path().join("output");

    // Create minimal feature directory
    fs::create_dir_all(&feature_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{"id": "test-feature", "version": "1.0.0", "name": "Test Feature"}"#,
    )
    .unwrap();

    // Run package command with global --log-format json - should fail
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "--log-format",
        "json",
        "features",
        "package",
        feature_dir.to_str().unwrap(),
        "--output",
        output_dir.to_str().unwrap(),
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "JSON output is not supported for features package",
    ));

    // Verify no artifacts were created
    assert!(!output_dir.join("test-feature-1.0.0.tgz").exists());
    assert!(!output_dir.join("devcontainer-collection.json").exists());
}

/// Test packaging with non-ASCII filenames round-trips correctly
#[test]
fn test_packaging_non_ascii_filenames_round_trip() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature");
    let output_dir = temp_dir.path().join("output");
    let extract_dir = temp_dir.path().join("extracted");

    // Create feature directory with non-ASCII filenames
    fs::create_dir_all(&feature_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();
    fs::create_dir_all(&extract_dir).unwrap();

    // Create devcontainer-feature.json
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{"id": "test-feature", "version": "1.0.0", "name": "Test Feature"}"#,
    )
    .unwrap();

    // Create files with non-ASCII names
    fs::write(
        feature_dir.join("文件.txt"), // Chinese characters
        "Content with Chinese filename",
    )
    .unwrap();
    fs::write(
        feature_dir.join("файл.txt"), // Cyrillic characters
        "Content with Cyrillic filename",
    )
    .unwrap();
    fs::write(
        feature_dir.join("café.txt"), // Latin with diacritics
        "Content with accented filename",
    )
    .unwrap();
    fs::write(
        feature_dir.join("🚀.txt"), // Emoji
        "Content with emoji filename",
    )
    .unwrap();

    // Run package command
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "features",
        "package",
        feature_dir.to_str().unwrap(),
        "--output",
        output_dir.to_str().unwrap(),
    ]);

    cmd.assert().success();

    // Verify .tgz was created
    let tgz_path = output_dir.join("test-feature-1.0.0.tgz");
    assert!(tgz_path.exists());

    // Extract the tar file to verify contents
    let tar_file = fs::File::open(&tgz_path).unwrap();
    let tar = flate2::read::GzDecoder::new(tar_file);
    let mut archive = tar::Archive::new(tar);
    archive.unpack(&extract_dir).unwrap();

    // Verify all non-ASCII files were preserved with correct content
    assert!(extract_dir.join("文件.txt").exists());
    assert_eq!(
        fs::read_to_string(extract_dir.join("文件.txt")).unwrap(),
        "Content with Chinese filename"
    );

    assert!(extract_dir.join("файл.txt").exists());
    assert_eq!(
        fs::read_to_string(extract_dir.join("файл.txt")).unwrap(),
        "Content with Cyrillic filename"
    );

    assert!(extract_dir.join("café.txt").exists());
    assert_eq!(
        fs::read_to_string(extract_dir.join("café.txt")).unwrap(),
        "Content with accented filename"
    );

    assert!(extract_dir.join("🚀.txt").exists());
    assert_eq!(
        fs::read_to_string(extract_dir.join("🚀.txt")).unwrap(),
        "Content with emoji filename"
    );

    // Verify devcontainer-feature.json was also preserved
    assert!(extract_dir.join("devcontainer-feature.json").exists());
}

/// Test packaging with deeply nested directory structure
#[test]
fn test_packaging_deep_nesting_path_handling() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature");
    let output_dir = temp_dir.path().join("output");

    // Create feature directory
    fs::create_dir_all(&feature_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    // Create devcontainer-feature.json
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{"id": "test-feature", "version": "1.0.0", "name": "Test Feature"}"#,
    )
    .unwrap();

    // Create deeply nested directory structure
    // This creates a path that's likely to exceed typical filesystem limits
    let mut current_path = feature_dir.clone();
    for i in 0..50 {
        current_path = current_path.join(format!("level_{}", i));
        fs::create_dir_all(&current_path).unwrap();

        // Add a file at each level
        fs::write(
            current_path.join(format!("file_{}.txt", i)),
            format!("Content of file at level {}", i),
        )
        .unwrap();
    }

    // Run package command - should either succeed or fail with clear path-length error
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let cmd_result = cmd
        .args([
            "features",
            "package",
            feature_dir.to_str().unwrap(),
            "--output",
            output_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    // The command should either succeed or fail with a path-related error
    if !cmd_result.status.success() {
        let stderr = String::from_utf8_lossy(&cmd_result.stderr);
        // If it fails, it should be due to path length issues, not other errors
        // The error should mention something about paths or file system limits
        assert!(
            stderr.contains("path")
                || stderr.contains("length")
                || stderr.contains("filesystem")
                || stderr.contains("too long")
                || stderr.contains("deep")
                || stderr.contains("nesting"),
            "Expected path-related error, got: {}",
            stderr
        );
    } else {
        // If it succeeds, verify the .tgz was created
        let tgz_path = output_dir.join("test-feature-1.0.0.tgz");
        assert!(
            tgz_path.exists(),
            "Package should have been created successfully"
        );

        // Verify the package is not empty (contains the nested structure)
        let metadata = fs::metadata(&tgz_path).unwrap();
        assert!(
            metadata.len() > 1000,
            "Package should contain substantial content"
        );
    }
}
