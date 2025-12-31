//! Integration tests for read-configuration output structure compliance
//!
//! These tests verify that the JSON output from read-configuration matches
//! the specification in docs/subcommand-specs/read-configuration/DATA-STRUCTURES.md

use anyhow::Result;
use assert_cmd::prelude::*;
use serde_json::Value;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Test that workspace field is included in basic output
#[test]
fn test_workspace_field_included() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;
    let config_path = devcontainer_dir.join("devcontainer.json");

    let config_content = r#"{
        "name": "test-container",
        "image": "ubuntu:22.04"
    }"#;

    fs::write(&config_path, config_content)?;

    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .current_dir(&temp_dir)
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()?;

    assert!(
        output.status.success(),
        "Command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim())?;

    // Verify workspace field exists
    assert!(
        parsed.get("workspace").is_some(),
        "workspace field should be present in output"
    );

    // Verify workspace field structure per spec
    let workspace = &parsed["workspace"];
    assert!(
        workspace.get("workspaceFolder").is_some(),
        "workspaceFolder should be present"
    );
    assert!(
        workspace.get("configFolderPath").is_some(),
        "configFolderPath should be present"
    );
    assert!(
        workspace.get("rootFolderPath").is_some(),
        "rootFolderPath should be present"
    );

    // workspaceMount is optional, but if present should be a string
    if let Some(mount) = workspace.get("workspaceMount") {
        assert!(mount.is_string(), "workspaceMount should be a string");
    }

    Ok(())
}

/// Test that configuration field is always included
#[test]
fn test_configuration_field_always_included() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;
    let config_path = devcontainer_dir.join("devcontainer.json");

    let config_content = r#"{
        "name": "test-container",
        "image": "alpine:latest"
    }"#;

    fs::write(&config_path, config_content)?;

    // Test basic output
    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .current_dir(&temp_dir)
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim())?;

    assert!(
        parsed.get("configuration").is_some(),
        "configuration field should always be present"
    );
    assert_eq!(parsed["configuration"]["name"], "test-container");

    // Test with merged configuration flag
    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .current_dir(&temp_dir)
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--include-merged-configuration")
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim())?;

    assert!(
        parsed.get("configuration").is_some(),
        "configuration field should be present even with merged configuration"
    );
    assert_eq!(parsed["configuration"]["name"], "test-container");

    Ok(())
}

/// Test that featuresConfiguration is included when flag is set
#[test]
fn test_features_configuration_with_flag() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;
    let config_path = devcontainer_dir.join("devcontainer.json");

    let config_content = r#"{
        "name": "test-container",
        "image": "ubuntu:22.04",
        "features": {}
    }"#;

    fs::write(&config_path, config_content)?;

    // Without flag - should not include featuresConfiguration
    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .current_dir(&temp_dir)
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim())?;

    assert!(
        parsed.get("featuresConfiguration").is_none(),
        "featuresConfiguration should not be present without flag"
    );

    // With flag - should include featuresConfiguration
    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .current_dir(&temp_dir)
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--include-features-configuration")
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim())?;

    assert!(
        parsed.get("featuresConfiguration").is_some(),
        "featuresConfiguration should be present with flag"
    );

    // Verify structure per spec
    let features_config = &parsed["featuresConfiguration"];
    assert!(
        features_config.get("featureSets").is_some(),
        "featureSets should be present"
    );
    assert!(
        features_config["featureSets"].is_array(),
        "featureSets should be an array"
    );

    Ok(())
}

/// Test that mergedConfiguration is included when flag is set
#[test]
fn test_merged_configuration_with_flag() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;
    let config_path = devcontainer_dir.join("devcontainer.json");

    let config_content = r#"{
        "name": "test-container",
        "image": "ubuntu:22.04"
    }"#;

    fs::write(&config_path, config_content)?;

    // Without flag - should not include mergedConfiguration
    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .current_dir(&temp_dir)
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim())?;

    assert!(
        parsed.get("mergedConfiguration").is_none(),
        "mergedConfiguration should not be present without flag"
    );

    // With flag - should include mergedConfiguration
    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .current_dir(&temp_dir)
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--include-merged-configuration")
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim())?;

    assert!(
        parsed.get("mergedConfiguration").is_some(),
        "mergedConfiguration should be present with flag"
    );

    Ok(())
}

/// Test that featuresConfiguration is included automatically when merged config is requested
/// (per spec: features are needed to derive metadata when no container is available)
#[test]
fn test_features_configuration_included_with_merged() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;
    let config_path = devcontainer_dir.join("devcontainer.json");

    let config_content = r#"{
        "name": "test-container",
        "image": "ubuntu:22.04"
    }"#;

    fs::write(&config_path, config_content)?;

    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .current_dir(&temp_dir)
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--include-merged-configuration")
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim())?;

    // Per spec: when --include-merged-configuration is set without a container,
    // featuresConfiguration is automatically computed to derive metadata
    assert!(
        parsed.get("featuresConfiguration").is_some(),
        "featuresConfiguration should be present when merged config is requested without container"
    );
    assert!(
        parsed.get("mergedConfiguration").is_some(),
        "mergedConfiguration should be present"
    );

    Ok(())
}

/// Test complete output structure with all optional fields
#[test]
fn test_complete_output_structure() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;
    let config_path = devcontainer_dir.join("devcontainer.json");

    let config_content = r#"{
        "name": "complete-test",
        "image": "ubuntu:22.04",
        "features": {}
    }"#;

    fs::write(&config_path, config_content)?;

    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .current_dir(&temp_dir)
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--include-features-configuration")
        .arg("--include-merged-configuration")
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim())?;

    // Verify all expected top-level fields are present
    assert!(
        parsed.get("configuration").is_some(),
        "Missing configuration"
    );
    assert!(parsed.get("workspace").is_some(), "Missing workspace");
    assert!(
        parsed.get("featuresConfiguration").is_some(),
        "Missing featuresConfiguration"
    );
    assert!(
        parsed.get("mergedConfiguration").is_some(),
        "Missing mergedConfiguration"
    );

    // Verify no unexpected fields
    let keys: Vec<&str> = parsed
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    for key in &keys {
        assert!(
            matches!(
                *key,
                "configuration" | "workspace" | "featuresConfiguration" | "mergedConfiguration"
            ),
            "Unexpected field in output: {}",
            key
        );
    }

    Ok(())
}

/// Test workspace field structure in detail
#[test]
fn test_workspace_field_structure() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;
    let config_path = devcontainer_dir.join("devcontainer.json");

    let config_content = r#"{
        "name": "workspace-test",
        "image": "ubuntu:22.04"
    }"#;

    fs::write(&config_path, config_content)?;

    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .current_dir(&temp_dir)
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim())?;

    let workspace = &parsed["workspace"];

    // Per spec: WorkspaceConfig has these fields
    // workspaceFolder: string (required)
    let workspace_folder = workspace["workspaceFolder"].as_str().unwrap();
    assert!(
        workspace_folder.starts_with("/workspaces/"),
        "workspaceFolder should start with /workspaces/"
    );

    // workspaceMount: string | Mount (optional)
    if let Some(mount) = workspace.get("workspaceMount") {
        let mount_str = mount.as_str().unwrap();
        assert!(
            mount_str.contains("type=bind"),
            "workspaceMount should be a bind mount"
        );
        assert!(
            mount_str.contains("source="),
            "workspaceMount should have source"
        );
        assert!(
            mount_str.contains("target="),
            "workspaceMount should have target"
        );
    }

    // configFolderPath: string (required)
    let config_folder = workspace["configFolderPath"].as_str().unwrap();
    assert!(
        !config_folder.is_empty(),
        "configFolderPath should not be empty"
    );

    // rootFolderPath: string (required)
    let root_folder = workspace["rootFolderPath"].as_str().unwrap();
    assert!(
        !root_folder.is_empty(),
        "rootFolderPath should not be empty"
    );

    Ok(())
}
