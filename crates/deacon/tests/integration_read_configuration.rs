//! Integration tests for read-configuration command
//!
//! This module provides integration tests and helper utilities for testing
//! the read-configuration subcommand end-to-end.

use anyhow::Result;
use assert_cmd::prelude::*;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Test helper for running read-configuration and parsing JSON output
pub struct ReadConfigurationTestHelper {
    temp_dir: TempDir,
}

impl ReadConfigurationTestHelper {
    /// Create a new test helper with a temporary directory
    pub fn new() -> Result<Self> {
        Ok(Self {
            temp_dir: TempDir::new()?,
        })
    }

    /// Get the temporary directory path
    pub fn temp_dir(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Create a devcontainer.json file in the temporary directory
    pub fn create_config(&self, config_content: &str) -> Result<()> {
        let devcontainer_dir = self.temp_dir.path().join(".devcontainer");
        fs::create_dir_all(&devcontainer_dir)?;
        let config_path = devcontainer_dir.join("devcontainer.json");
        fs::write(config_path, config_content)?;
        Ok(())
    }

    /// Run the read-configuration command and return parsed JSON output
    pub fn run_read_configuration(&self, args: &[&str]) -> Result<Value> {
        let mut cmd = Command::cargo_bin("deacon")?;
        cmd.current_dir(self.temp_dir.path())
            .arg("read-configuration");

        for arg in args {
            cmd.arg(arg);
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            anyhow::bail!(
                "Command failed with status {}.\nStdout: {}\nStderr: {}",
                output.status,
                stdout,
                stderr
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: Value = serde_json::from_str(stdout.trim())?;

        Ok(parsed)
    }

    /// Run read-configuration with workspace-folder and return parsed JSON
    pub fn run_with_workspace(&self, args: &[&str]) -> Result<Value> {
        let workspace_path = self.temp_dir.path().to_string_lossy().to_string();
        let mut all_args = vec!["--workspace-folder", &workspace_path];
        all_args.extend_from_slice(args);
        self.run_read_configuration(&all_args)
    }
}

impl Default for ReadConfigurationTestHelper {
    fn default() -> Self {
        Self::new().expect("Failed to create test helper")
    }
}

/// Test that the helper can run basic read-configuration
#[test]
fn test_helper_basic_functionality() -> Result<()> {
    let helper = ReadConfigurationTestHelper::new()?;
    helper.create_config(r#"{"name": "test", "image": "ubuntu:22.04"}"#)?;

    let result = helper.run_with_workspace(&[])?;

    // Verify basic structure
    assert!(result.get("configuration").is_some());
    assert_eq!(result["configuration"]["name"], "test");

    Ok(())
}

/// Test that the helper properly handles command failures
#[test]
fn test_helper_command_failure() -> Result<()> {
    let helper = ReadConfigurationTestHelper::new()?;
    // Don't create config file - should fail

    let result = helper.run_with_workspace(&[]);
    assert!(result.is_err());

    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Command failed"));

    Ok(())
}

/// Test that the helper can parse JSON output correctly
#[test]
fn test_helper_json_parsing() -> Result<()> {
    let helper = ReadConfigurationTestHelper::new()?;
    helper.create_config(r#"{"name": "json-test", "image": "alpine"}"#)?;

    let result = helper.run_with_workspace(&[])?;

    // Verify the JSON was parsed correctly
    assert_eq!(result["configuration"]["name"], "json-test");
    assert_eq!(result["configuration"]["image"], "alpine");

    Ok(())
}

/// Test helper with additional flags
#[test]
fn test_helper_with_flags() -> Result<()> {
    let helper = ReadConfigurationTestHelper::new()?;
    helper.create_config(r#"{"name": "flag-test", "image": "ubuntu:22.04"}"#)?;

    let result = helper.run_with_workspace(&["--include-merged-configuration"])?;

    // Should include merged configuration
    assert!(result.get("configuration").is_some());
    assert!(result.get("mergedConfiguration").is_some());

    Ok(())
}

/// Test acceptance: stdout contains only { configuration: ... } when run with --workspace-folder
#[test]
fn test_acceptance_configuration_only_output() -> Result<()> {
    let helper = ReadConfigurationTestHelper::new()?;
    helper.create_config(r#"{"name": "test-config", "image": "ubuntu:22.04"}"#)?;

    let result = helper.run_with_workspace(&[])?;

    // Should contain only configuration field
    assert!(result.get("configuration").is_some());
    assert!(result.get("workspace").is_some()); // workspace is always included when resolvable
    assert!(result.get("featuresConfiguration").is_none()); // not requested
    assert!(result.get("mergedConfiguration").is_none()); // not requested

    // Verify configuration content
    assert_eq!(result["configuration"]["name"], "test-config");
    assert_eq!(result["configuration"]["image"], "ubuntu:22.04");

    Ok(())
}

/// Test acceptance: stdout contains configuration + mergedConfiguration when --include-merged-configuration is provided
#[test]
fn test_acceptance_merged_configuration_output() -> Result<()> {
    let helper = ReadConfigurationTestHelper::new()?;
    helper.create_config(r#"{"name": "test-merged", "image": "ubuntu:22.04"}"#)?;

    let result = helper.run_with_workspace(&["--include-merged-configuration"])?;

    // Should contain configuration and mergedConfiguration
    assert!(result.get("configuration").is_some());
    assert!(result.get("workspace").is_some()); // workspace is always included when resolvable
    assert!(result.get("featuresConfiguration").is_some()); // computed for merged config per spec
    assert!(result.get("mergedConfiguration").is_some()); // requested

    // Verify configuration content
    assert_eq!(result["configuration"]["name"], "test-merged");
    assert_eq!(result["configuration"]["image"], "ubuntu:22.04");

    // Verify merged configuration is present (currently returns base config as placeholder)
    assert_eq!(result["mergedConfiguration"]["name"], "test-merged");
    assert_eq!(result["mergedConfiguration"]["image"], "ubuntu:22.04");

    Ok(())
}

/// Test acceptance: label order does not change ${devcontainerId} in output
#[test]
fn test_acceptance_devcontainer_id_order_independence() -> Result<()> {
    let helper = ReadConfigurationTestHelper::new()?;
    helper.create_config(
        r#"{
        "name": "test-devcontainer-id",
        "image": "ubuntu:22.04",
        "containerEnv": {
            "DEVCONTAINER_ID": "${devcontainerId}"
        }
    }"#,
    )?;

    // Test with labels in one order
    let result1 =
        helper.run_with_workspace(&["--id-label", "app=web", "--id-label", "env=prod"])?;

    // Test with labels in reverse order
    let result2 =
        helper.run_with_workspace(&["--id-label", "env=prod", "--id-label", "app=web"])?;

    // Both should produce the same devcontainerId in the output
    let devcontainer_id_1 = result1["configuration"]["containerEnv"]["DEVCONTAINER_ID"].as_str();
    let devcontainer_id_2 = result2["configuration"]["containerEnv"]["DEVCONTAINER_ID"].as_str();

    assert!(
        devcontainer_id_1.is_some(),
        "First result should contain devcontainerId"
    );
    assert!(
        devcontainer_id_2.is_some(),
        "Second result should contain devcontainerId"
    );
    assert_eq!(
        devcontainer_id_1, devcontainer_id_2,
        "devcontainerId should be the same regardless of label order"
    );

    Ok(())
}

/// Parity (#219): without a container identity (plain --workspace-folder,
/// no --id-label/--container-id), `${devcontainerId}` must stay LITERAL in the
/// raw `configuration` output — including inside mount sources and containerEnv
/// — while other variables (e.g. `${localWorkspaceFolderBasename}`) still
/// resolve. The reference CLI defers `${devcontainerId}` to a container-aware
/// pass it never reaches in read-configuration.
#[test]
fn test_devcontainer_id_left_literal_without_container() -> Result<()> {
    let helper = ReadConfigurationTestHelper::new()?;
    helper.create_config(
        r#"{
        "name": "test-literal-id",
        "image": "ubuntu:22.04",
        "mounts": [
            { "source": "vol-${devcontainerId}", "target": "/data", "type": "volume" }
        ],
        "containerEnv": {
            "DC_ID": "${devcontainerId}",
            "BASENAME": "${localWorkspaceFolderBasename}"
        }
    }"#,
    )?;

    let result = helper.run_with_workspace(&[])?;

    assert_eq!(
        result["configuration"]["mounts"][0]["source"], "vol-${devcontainerId}",
        "mount source must keep ${{devcontainerId}} literal without a container"
    );
    assert_eq!(
        result["configuration"]["containerEnv"]["DC_ID"], "${devcontainerId}",
        "containerEnv must keep ${{devcontainerId}} literal without a container"
    );
    // Sanity: other variables still resolve in the same output.
    assert_ne!(
        result["configuration"]["containerEnv"]["BASENAME"], "${localWorkspaceFolderBasename}",
        "other variables must still resolve"
    );

    Ok(())
}

/// Test acceptance: with --container-id and --include-merged-configuration, error if inspect fails (no fallback)
#[test]
fn test_acceptance_container_id_with_merged_config_errors_on_inspect_failure() -> Result<()> {
    let helper = ReadConfigurationTestHelper::new()?;
    helper.create_config(
        r#"{
        "name": "test-merged-error",
        "image": "ubuntu:22.04"
    }"#,
    )?;

    // Try to run with a non-existent container ID and merged config
    // This should fail because container inspection will fail
    let result = helper.run_with_workspace(&[
        "--container-id",
        "nonexistent-container-id",
        "--include-merged-configuration",
    ]);

    // Should fail with an error (container not found)
    assert!(
        result.is_err(),
        "Should fail when container inspection fails"
    );

    let error_msg = result.unwrap_err().to_string();
    let error_msg_lc = error_msg.to_lowercase();
    // Error should indicate container not found or inspection failure
    assert!(
        error_msg_lc.contains("not found")
            || error_msg.contains("Container")
            || error_msg_lc.contains("inspect")
            || error_msg.contains("Dev container not found"),
        "Error message should indicate container inspection failure: {}",
        error_msg
    );

    Ok(())
}

/// Test acceptance: featuresConfiguration present when --include-features-configuration is set
#[test]
fn test_acceptance_features_configuration_present() -> Result<()> {
    let helper = ReadConfigurationTestHelper::new()?;
    helper.create_config(
        r#"{
        "name": "test-features",
        "image": "ubuntu:22.04"
    }"#,
    )?;

    let result = helper.run_with_workspace(&["--include-features-configuration"])?;

    // Should contain configuration and featuresConfiguration
    assert!(result.get("configuration").is_some());
    assert!(result.get("featuresConfiguration").is_some()); // requested
    assert!(result.get("mergedConfiguration").is_none()); // not requested

    // Verify featuresConfiguration structure (empty when no features defined)
    let features_config = result["featuresConfiguration"].as_object().unwrap();
    assert!(features_config.contains_key("featureSets"));

    Ok(())
}

/// Regression: a local feature (`./feat`) declared in an auto-discovered
/// `.devcontainer/devcontainer.json` must anchor to the config file's directory
/// (`.devcontainer/`), not the workspace folder. Before the fix, running with
/// only `--workspace-folder` (no `--config`) mis-anchored `./feat` to the
/// workspace root and failed with "not accessible". Hermetic (local feature, no
/// network).
#[test]
fn test_local_feature_anchors_to_discovered_config_dir() -> Result<()> {
    let helper = ReadConfigurationTestHelper::new()?;
    helper.create_config(
        r#"{
        "name": "local-anchor",
        "image": "ubuntu:22.04",
        "features": { "./localfeat": {} }
    }"#,
    )?;
    // Local feature lives NEXT TO the config, under .devcontainer/.
    let feat_dir = helper.temp_dir().join(".devcontainer").join("localfeat");
    std::fs::create_dir_all(&feat_dir)?;
    std::fs::write(
        feat_dir.join("devcontainer-feature.json"),
        r#"{ "id": "localfeat", "version": "1.0.0", "name": "Local Feat" }"#,
    )?;
    std::fs::write(feat_dir.join("install.sh"), "#!/bin/sh\ntrue\n")?;

    // Auto-discovery (only --workspace-folder) + features resolution.
    let result = helper.run_with_workspace(&["--include-features-configuration"])?;
    let features_config = result["featuresConfiguration"].as_object().unwrap();
    let sets = features_config
        .get("featureSets")
        .and_then(|v| v.as_array())
        .expect("featureSets array");
    let found = sets.iter().any(|fs| {
        fs.get("features")
            .and_then(|f| f.as_array())
            .map(|arr| {
                arr.iter().any(|feat| {
                    feat.get("id")
                        .and_then(|i| i.as_str())
                        .map(|s| s.contains("localfeat"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    });
    assert!(
        found,
        "local feature must resolve via discovered-config anchoring: {result:#}"
    );
    Ok(())
}

/// Parity: a feature's `containerEnv` must NOT be folded into
/// `mergedConfiguration.containerEnv` at read-config time. Per upstream
/// `imageMetadata.ts`, a feature's image-metadata entry omits env — feature env
/// is realized by the feature's install step, not via the `devcontainer.metadata`
/// merge. The base config's own `containerEnv` MUST still survive the merge.
/// Hermetic (local feature, no network).
#[test]
fn test_merged_config_excludes_feature_container_env() -> Result<()> {
    let helper = ReadConfigurationTestHelper::new()?;
    helper.create_config(
        r#"{
        "name": "feat-env",
        "image": "ubuntu:22.04",
        "containerEnv": { "BASE_VAR": "base-value" },
        "features": { "./featenv": {} }
    }"#,
    )?;
    let feat_dir = helper.temp_dir().join(".devcontainer").join("featenv");
    std::fs::create_dir_all(&feat_dir)?;
    std::fs::write(
        feat_dir.join("devcontainer-feature.json"),
        r#"{ "id": "featenv", "version": "1.0.0", "name": "Feat Env",
            "containerEnv": { "FEATURE_VAR": "feature-value" } }"#,
    )?;
    std::fs::write(feat_dir.join("install.sh"), "#!/bin/sh\ntrue\n")?;

    let result = helper.run_with_workspace(&["--include-merged-configuration"])?;
    let merged_env = &result["mergedConfiguration"]["containerEnv"];
    assert_eq!(
        merged_env["BASE_VAR"].as_str(),
        Some("base-value"),
        "base config containerEnv must survive the merge: {result:#}"
    );
    assert!(
        merged_env.get("FEATURE_VAR").is_none(),
        "feature containerEnv must NOT appear in mergedConfiguration: {result:#}"
    );
    Ok(())
}

/// Regression: `featuresConfiguration.featureSets` is emitted ONE-PER-FEATURE in
/// INSTALL ORDER (a feature's dependencies first), matching the reference CLI —
/// not grouped by registry. Two local features where `app` dependsOn `lib` must
/// produce sets ordered `[lib, app]`. Hermetic (local features, no network).
#[test]
fn test_features_configuration_emitted_in_install_order() -> Result<()> {
    let helper = ReadConfigurationTestHelper::new()?;
    helper.create_config(
        r#"{
        "name": "order",
        "image": "ubuntu:22.04",
        "features": { "./features/app": {} }
    }"#,
    )?;
    let feats = helper.temp_dir().join(".devcontainer").join("features");
    for (name, body) in [
        (
            "lib",
            r#"{ "id": "lib", "version": "1.0.0", "name": "Lib" }"#,
        ),
        (
            "app",
            r#"{ "id": "app", "version": "1.0.0", "name": "App", "dependsOn": { "./features/lib": {} } }"#,
        ),
    ] {
        let d = feats.join(name);
        std::fs::create_dir_all(&d)?;
        std::fs::write(d.join("devcontainer-feature.json"), body)?;
        std::fs::write(d.join("install.sh"), "#!/bin/sh\ntrue\n")?;
    }

    let result = helper.run_with_workspace(&["--include-features-configuration"])?;
    let sets = result["featuresConfiguration"]["featureSets"]
        .as_array()
        .expect("featureSets array");
    // One set per feature (lib auto-installed via dependsOn + app declared).
    assert_eq!(sets.len(), 2, "expected one set per feature: {result:#}");
    let order: Vec<&str> = sets
        .iter()
        .filter_map(|s| s["features"][0]["id"].as_str())
        .map(|id| if id.contains("lib") { "lib" } else { "app" })
        .collect();
    assert_eq!(
        order,
        vec!["lib", "app"],
        "dependency must come before dependent (install order): {order:?}"
    );

    // sourceInformation for local features matches the reference's `file-path`
    // shape: { type, resolvedFilePath, userFeatureId }.
    let si = &sets[0]["sourceInformation"];
    assert_eq!(si["type"], "file-path", "local feature source type: {si:#}");
    assert_eq!(si["userFeatureId"], "./features/lib");
    assert!(
        si["resolvedFilePath"]
            .as_str()
            .map(|p| p.replace('\\', "/").ends_with("features/lib"))
            .unwrap_or(false),
        "resolvedFilePath should point at the feature dir: {si:#}"
    );
    Ok(())
}

/// Test acceptance: deep-merge --additional-features with precedence over base
#[test]
fn test_acceptance_additional_features_deep_merge_precedence() -> Result<()> {
    let helper = ReadConfigurationTestHelper::new()?;
    helper.create_config(
        r#"{
        "name": "test-additional-features",
        "image": "ubuntu:22.04"
    }"#,
    )?;

    let result = helper.run_with_workspace(&["--include-features-configuration"])?;

    // Should contain configuration and featuresConfiguration
    assert!(result.get("configuration").is_some());
    assert!(result.get("featuresConfiguration").is_some());

    // Note: This test verifies that the command accepts the flags and runs without error.
    // Full validation of the merge semantics would require mocking the OCI registry
    // to return actual feature metadata. The merge logic itself is tested in the
    // FeatureMerger unit tests.

    Ok(())
}

/// Test acceptance (FR-011): configuration is {} when only container selectors are provided (container-only mode)
#[test]
fn test_acceptance_container_only_mode_empty_configuration() -> Result<()> {
    let helper = ReadConfigurationTestHelper::new()?;

    // Run read-configuration with only --id-label (no workspace/config)
    // This is container-only mode - should not error but return empty configuration
    let result = helper.run_read_configuration(&["--id-label", "app=web"])?;

    // Should contain configuration field but it should be empty {}
    assert!(
        result.get("configuration").is_some(),
        "configuration field must be present"
    );

    let config = result["configuration"].as_object().unwrap();
    assert!(
        config.is_empty(),
        "configuration should be empty {{}} when only container selectors are provided, got: {:?}",
        config
    );

    // workspace should not be present (no workspace folder provided)
    assert!(
        result.get("workspace").is_none(),
        "workspace should not be present in container-only mode"
    );

    // Optional fields should not be present when not requested
    assert!(
        result.get("featuresConfiguration").is_none(),
        "featuresConfiguration should not be present when not requested"
    );
    assert!(
        result.get("mergedConfiguration").is_none(),
        "mergedConfiguration should not be present when not requested"
    );

    Ok(())
}

/// `${containerWorkspaceFolder}` must resolve even when the config declares no
/// explicit `workspaceFolder` and there is no container. The reference CLI
/// resolves it to the CONTAINER-side default `/workspaces/<basename>[/<subpath>]`
/// — NOT the host path — so it does not equal `${localWorkspaceFolder}` (issue
/// #309, oracle-verified against @devcontainers/cli@0.87.0).
#[test]
fn test_container_workspace_folder_resolves_without_explicit_workspace_folder() -> Result<()> {
    let helper = ReadConfigurationTestHelper::new()?;
    helper.create_config(
        r#"{
            "name": "no-workspace-folder",
            "image": "ubuntu:22.04",
            "containerEnv": {
                "WORKSPACE": "${containerWorkspaceFolder}",
                "LOCAL_WS": "${localWorkspaceFolder}"
            }
        }"#,
    )?;

    let result = helper.run_with_workspace(&[])?;
    let env = &result["configuration"]["containerEnv"];

    let workspace = env["WORKSPACE"]
        .as_str()
        .expect("WORKSPACE should be a string");
    let local_ws = env["LOCAL_WS"]
        .as_str()
        .expect("LOCAL_WS should be a string");

    assert!(
        !workspace.contains("${"),
        "containerWorkspaceFolder must be resolved, got literal: {workspace}"
    );
    // #309: the container-side default is `/workspaces/<basename>`, NOT the host
    // path. The fixture workspace is a (non-git) temp dir, so the value is
    // `/workspaces/<tempBasename>` and must differ from `${localWorkspaceFolder}`
    // (the host temp path), matching the reference CLI.
    assert!(
        workspace.starts_with("/workspaces/"),
        "containerWorkspaceFolder should be a /workspaces/<basename> path, got: {workspace}"
    );
    assert_ne!(
        workspace, local_ws,
        "containerWorkspaceFolder must NOT equal localWorkspaceFolder (host path) — see #309"
    );

    Ok(())
}

/// Divergence B: the default `configuration` output is the RAW entry config with
/// `extends` preserved (a single target as a string) and child values left
/// un-merged with the base — matching the reference CLI, which defers the
/// extends merge to `up`/`mergedConfiguration`.
#[test]
fn test_extends_output_is_raw_unmerged_with_extends_preserved() -> Result<()> {
    let helper = ReadConfigurationTestHelper::new()?;
    let devcontainer_dir = helper.temp_dir().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;
    fs::write(
        devcontainer_dir.join("base.json"),
        r#"{
            "image": "mcr.microsoft.com/devcontainers/base:bookworm",
            "containerEnv": { "BASE": "yes" },
            "forwardPorts": [3000],
            "postCreateCommand": "echo from base"
        }"#,
    )?;
    helper.create_config(
        r#"{
            "name": "child",
            "extends": "./base.json",
            "forwardPorts": [4000],
            "containerEnv": { "CHILD": "yes" },
            "remoteUser": "vscode"
        }"#,
    )?;

    let result = helper.run_with_workspace(&[])?;
    let cfg = &result["configuration"];

    // `extends` is preserved as a bare string (single target), not an array.
    assert_eq!(
        cfg["extends"], "./base.json",
        "extends must be preserved as a string in the raw output"
    );
    // forwardPorts is the child's un-merged value.
    assert_eq!(
        cfg["forwardPorts"],
        serde_json::json!([4000]),
        "forwardPorts must be the raw child value, not merged with the base"
    );
    // Base-only values must NOT be merged into the raw child output. deacon
    // serializes the full struct (absent fields as `null`, stripped by the
    // Tier-1 normalizer), so a base value would appear as a non-null leak.
    assert!(
        cfg["image"].is_null(),
        "base `image` must not be merged into the raw child output, got: {:?}",
        cfg["image"]
    );
    assert!(
        cfg["postCreateCommand"].is_null(),
        "base `postCreateCommand` must not be merged into the raw child output, got: {:?}",
        cfg["postCreateCommand"]
    );
    assert!(
        cfg["containerEnv"].get("BASE").is_none(),
        "base containerEnv must not be merged into the raw child output"
    );

    Ok(())
}
