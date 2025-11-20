//! Integration tests for up command config resolution
//!
//! Tests from specs/001-up-gap-spec/contracts/up.md and tasks.md:
//! - Config filename validation (must be devcontainer.json or .devcontainer.json)
//! - Disallowed feature error handling
//! - Image metadata merge into resolved configuration

use assert_cmd::Command;

// Config filename validation tests

#[test]
fn test_config_must_be_named_devcontainer_json() {
    // Valid config names: devcontainer.json, .devcontainer.json, .devcontainer/devcontainer.json
    // Invalid: custom-config.json (only allowed via --override-config)

    // This test is a placeholder - actual validation happens during config loading
    // and would require proper fixture setup
}

#[test]
fn test_override_config_can_have_custom_name() {
    // --override-config can point to any filename
    // This is allowed by the spec for override scenarios

    // Placeholder - requires fixture setup
}

// Disallowed feature tests

#[test]
fn test_disallowed_feature_causes_error_before_build() {
    // Contract: If a feature is in the disallowed list, error before any build/runtime ops
    // Expected error JSON: { "outcome": "error", "disallowedFeatureId": "feature-id", ... }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg("/tmp/test-workspace")
        .arg("--additional-features")
        .arg(r#"{"disallowed-feature":"latest"}"#);

    cmd.assert().failure().code(1);

    // TODO: Parse JSON output and verify disallowedFeatureId field is present
}

// Image metadata merge tests

#[test]
fn test_image_metadata_merges_into_configuration() {
    // When includeConfiguration or includeMergedConfiguration is set,
    // the returned config should include metadata from the base image
    // (e.g., labels added by features, environment variables, etc.)

    // This requires:
    // 1. A test fixture with a devcontainer that has an image
    // 2. Running up with --include-merged-configuration
    // 3. Inspecting the JSON output to verify merged metadata

    // Placeholder - complex integration test
}

#[test]
fn test_id_label_discovery_without_workspace() {
    // Contract: Can use --id-label to find container without --workspace-folder
    // This is for reconnection scenarios

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--id-label")
        .arg("devcontainer.local_folder=/some/path");

    // Should attempt to find container by label
    // Will fail if container doesn't exist, but shouldn't fail due to missing workspace
    cmd.assert().failure(); // Expected to fail (no such container in test)
}

// TODO: Add more comprehensive tests once the implementation is complete
// These tests currently serve as documentation of the expected behavior
// and will be enabled/expanded as T007-T011, T028-T029 are implemented.
