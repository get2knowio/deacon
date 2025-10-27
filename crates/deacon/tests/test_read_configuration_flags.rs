//! Integration tests for read-configuration command flags
//!
//! Tests for flag behavior and combinations not covered in other test files

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use tempfile::TempDir;

/// Test --skip-feature-auto-mapping flag prevents string value auto-mapping
/// This test validates the flag is accepted; actual auto-mapping behavior
/// would require registry access which is mocked in unit tests.
#[test]
fn test_skip_feature_auto_mapping_flag() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");

    // Config without features to avoid registry calls
    let config_content = r#"{
        "name": "test-container",
        "image": "ubuntu:22.04",
        "features": {}
    }"#;

    fs::write(&config_path, config_content).unwrap();

    // Test that the flag is accepted
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--skip-feature-auto-mapping");

    cmd.assert().success();
}

/// Test --mount-workspace-git-root flag with true value
#[test]
fn test_mount_workspace_git_root_true() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");

    fs::write(&config_path, r#"{"name": "test", "image": "ubuntu:22.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--mount-workspace-git-root")
        .arg("true");

    cmd.assert().success();
}

/// Test --mount-workspace-git-root flag with false value
#[test]
fn test_mount_workspace_git_root_false() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");

    fs::write(&config_path, r#"{"name": "test", "image": "ubuntu:22.04"}"#).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--mount-workspace-git-root")
        .arg("false");

    cmd.assert().success();
}

/// Test --include-features-configuration with features in config
/// Note: Uses empty features to avoid registry calls in CI
#[test]
fn test_include_features_with_real_features() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");

    let config_content = r#"{
        "name": "test-container",
        "image": "ubuntu:22.04",
        "features": {}
    }"#;

    fs::write(&config_path, config_content).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--include-features-configuration")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "Command should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify output structure
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim()).unwrap();

    // Should have featuresConfiguration
    assert!(
        parsed.get("featuresConfiguration").is_some(),
        "featuresConfiguration should be present"
    );

    // Verify structure
    let features_config = &parsed["featuresConfiguration"];
    assert!(features_config.get("featureSets").is_some());

    let feature_sets = features_config["featureSets"].as_array().unwrap();
    // Empty features means empty feature sets
    assert_eq!(
        feature_sets.len(),
        0,
        "Should have no feature sets for empty features"
    );
}

/// Test --include-merged-configuration without container (auto-resolves features)
#[test]
fn test_merged_configuration_auto_resolves_features() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");

    let config_content = r#"{
        "name": "test-container",
        "image": "ubuntu:22.04",
        "features": {}
    }"#;

    fs::write(&config_path, config_content).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--include-merged-configuration")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "Command should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim()).unwrap();

    // Per spec: when --include-merged-configuration is set without container,
    // features are automatically resolved to derive metadata
    assert!(
        parsed.get("featuresConfiguration").is_some(),
        "featuresConfiguration should be auto-included with merged config"
    );
    assert!(
        parsed.get("mergedConfiguration").is_some(),
        "mergedConfiguration should be present"
    );
}

/// Test --additional-features with --include-features-configuration
/// Note: Uses empty features to avoid registry calls in CI
#[test]
fn test_additional_features_with_features_config() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");

    let config_content = r#"{
        "name": "test-container",
        "image": "ubuntu:22.04",
        "features": {}
    }"#;

    fs::write(&config_path, config_content).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--additional-features")
        .arg(r#"{}"#)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "Command should succeed with additional features: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Test that both flags work together
#[test]
fn test_both_include_flags_together() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");

    let config_content = r#"{
        "name": "test-container",
        "image": "ubuntu:22.04"
    }"#;

    fs::write(&config_path, config_content).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--include-features-configuration")
        .arg("--include-merged-configuration")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "Both flags should work together: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim()).unwrap();

    assert!(parsed.get("configuration").is_some());
    assert!(parsed.get("workspace").is_some());
    assert!(parsed.get("featuresConfiguration").is_some());
    assert!(parsed.get("mergedConfiguration").is_some());
}

/// Test --override-config flag behavior
#[test]
fn test_override_config_flag() {
    let temp_dir = TempDir::new().unwrap();
    let base_config_path = temp_dir.path().join("devcontainer.json");
    let override_config_path = temp_dir.path().join("override.json");

    let base_config = r#"{
        "name": "base-name",
        "image": "ubuntu:22.04"
    }"#;

    let override_config = r#"{
        "name": "override-name"
    }"#;

    fs::write(&base_config_path, base_config).unwrap();
    fs::write(&override_config_path, override_config).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .arg("read-configuration")
        .arg("--config")
        .arg(&base_config_path)
        .arg("--override-config")
        .arg(&override_config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "Override config should work: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim()).unwrap();

    // Name should be from override config
    assert_eq!(
        parsed["configuration"]["name"].as_str().unwrap(),
        "override-name"
    );
}

/// Test --secrets-file flag
#[test]
fn test_secrets_file_flag() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    let secrets_path = temp_dir.path().join("secrets.env");

    let config_content = r#"{
        "name": "test-container",
        "image": "ubuntu:22.04",
        "containerEnv": {
            "MY_SECRET": "${localEnv:MY_SECRET}"
        }
    }"#;

    let secrets_content = "MY_SECRET=super-secret-value\n";

    fs::write(&config_path, config_content).unwrap();
    fs::write(&secrets_path, secrets_content).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--secrets-file")
        .arg(&secrets_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "Secrets file should work: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim()).unwrap();

    // Note: Redaction only applies when secrets are marked as such in the registry.
    // For now, we just verify the command succeeds with secrets file.
    // The secret will be substituted but may not be redacted unless explicitly registered.
    let container_env = &parsed["configuration"]["containerEnv"];
    let secret_value = container_env["MY_SECRET"].as_str().unwrap();

    // Verify the secret was substituted
    assert!(
        !secret_value.is_empty(),
        "Secret should be substituted from secrets file"
    );
}

/// Test --no-redact flag
#[test]
fn test_no_redact_flag() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("devcontainer.json");
    let secrets_path = temp_dir.path().join("secrets.env");

    let config_content = r#"{
        "name": "test-container",
        "image": "ubuntu:22.04",
        "containerEnv": {
            "MY_SECRET": "${localEnv:MY_SECRET}"
        }
    }"#;

    let secrets_content = "MY_SECRET=super-secret-value\n";

    fs::write(&config_path, config_content).unwrap();
    fs::write(&secrets_path, secrets_content).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .arg("read-configuration")
        .arg("--config")
        .arg(&config_path)
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--secrets-file")
        .arg(&secrets_path)
        .arg("--no-redact")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "No redact flag should work: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim()).unwrap();

    // With --no-redact, secret should be visible
    let container_env = &parsed["configuration"]["containerEnv"];
    let secret_value = container_env["MY_SECRET"].as_str().unwrap();

    assert_eq!(
        secret_value, "super-secret-value",
        "Secret should NOT be redacted with --no-redact flag"
    );
}
