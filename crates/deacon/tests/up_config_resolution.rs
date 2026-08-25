//! Integration tests for up command config resolution
//!
//! Tests from specs/001-up-gap-spec/contracts/up.md and tasks.md:
//! - Config filename validation (must be devcontainer.json or .devcontainer.json)
//! - Disallowed feature error handling
//! - Image metadata merge into resolved configuration

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

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

/// A workspace declaring exactly `features`, so the gate has something to see.
///
/// Hermetic on purpose: the disallowed-Features gate refuses before deacon
/// contacts a registry or a daemon, which is the property these tests are here
/// to hold on to.
fn workspace_declaring(features: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().join(".devcontainer");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("devcontainer.json"),
        format!(r#"{{ "image": "alpine:3.19", "features": {features} }}"#),
    )
    .unwrap();
    dir
}

fn error_json(output: &[u8]) -> serde_json::Value {
    serde_json::from_slice(output).expect("up must emit exactly one JSON document on stdout")
}

#[test]
fn test_disallowed_feature_causes_error_before_build() {
    // Contract: a Feature on the disallowed list errors before any build or
    // runtime operation, and the error JSON names it structurally.
    let ws = workspace_declaring(r#"{ "ghcr.io/devcontainers/features/node:1": {} }"#);

    let assert = Command::cargo_bin("deacon")
        .unwrap()
        .arg("up")
        .arg("--workspace-folder")
        .arg(ws.path())
        .env(
            "DEACON_DISALLOWED_FEATURES",
            "ghcr.io/devcontainers/features/node:1",
        )
        .assert()
        .failure()
        .code(1);

    let json = error_json(&assert.get_output().stdout);
    assert_eq!(json["outcome"], "error");
    assert_eq!(
        json["disallowedFeatureId"], "ghcr.io/devcontainers/features/node:1",
        "the blocked Feature must be reported structurally, not only in prose"
    );
}

#[test]
fn a_disallowed_entry_covers_the_versioned_feature_id() {
    // #675: an entry matches by prefix terminated at a Feature-id separator, so
    // the unversioned entry an operator actually writes covers `…:1`. Exact
    // matching made that entry block nothing.
    let ws = workspace_declaring(r#"{ "ghcr.io/devcontainers/features/node:1": {} }"#);

    let assert = Command::cargo_bin("deacon")
        .unwrap()
        .arg("up")
        .arg("--workspace-folder")
        .arg(ws.path())
        .env(
            "DEACON_DISALLOWED_FEATURES",
            "ghcr.io/devcontainers/features/node",
        )
        .assert()
        .failure()
        .code(1);

    let json = error_json(&assert.get_output().stdout);
    assert_eq!(
        json["disallowedFeatureId"],
        "ghcr.io/devcontainers/features/node:1"
    );
}

#[test]
fn the_gate_sees_features_arriving_via_additional_features() {
    // #675: the reference gates the union of the configuration's Features and
    // `--additional-features`. Gating the configuration alone let a caller walk
    // past the policy by moving the Feature to the command line.
    let ws = workspace_declaring("{}");

    let assert = Command::cargo_bin("deacon")
        .unwrap()
        .arg("up")
        .arg("--workspace-folder")
        .arg(ws.path())
        .arg("--additional-features")
        .arg(r#"{"ghcr.io/devcontainers/features/node:1":{}}"#)
        .env(
            "DEACON_DISALLOWED_FEATURES",
            "ghcr.io/devcontainers/features/node",
        )
        .assert()
        .failure()
        .code(1);

    let json = error_json(&assert.get_output().stdout);
    assert_eq!(
        json["disallowedFeatureId"],
        "ghcr.io/devcontainers/features/node:1"
    );
}

#[test]
fn an_ignored_additional_feature_is_out_of_the_gates_scope() {
    // `--ignore-additional-features` drops the overlay, so nothing disallowed
    // would be installed and the run must not be refused for it. This is what
    // gating the MERGED set buys over gating the raw union.
    //
    // The configuration names a local Feature that does not exist, so the run
    // still fails — hermetically, at Feature resolution, without reaching a
    // registry or creating a container. That failure is the evidence the run
    // got PAST the gate; an assertion that it merely "did not say
    // disallowedFeatureId" would also pass if nothing ran at all.
    let ws = workspace_declaring(r#"{ "./tripwire": {} }"#);

    let assert = Command::cargo_bin("deacon")
        .unwrap()
        .arg("up")
        .arg("--workspace-folder")
        .arg(ws.path())
        .arg("--additional-features")
        .arg(r#"{"ghcr.io/devcontainers/features/node:1":{}}"#)
        .arg("--ignore-additional-features")
        .env(
            "DEACON_DISALLOWED_FEATURES",
            "ghcr.io/devcontainers/features/node",
        )
        .assert()
        .failure()
        .stdout(predicates::str::contains("disallowedFeatureId").not())
        .stderr(predicates::str::contains("./tripwire"));
    drop(assert);
}

#[test]
fn build_consults_the_disallowed_gate_too() {
    // #675: `build` had no gate at all, so a Feature refused on `up` was
    // installable by building the same configuration. The reference gates both.
    let ws = workspace_declaring(r#"{ "ghcr.io/devcontainers/features/node:1": {} }"#);

    Command::cargo_bin("deacon")
        .unwrap()
        .arg("build")
        .arg("--workspace-folder")
        .arg(ws.path())
        .env(
            "DEACON_DISALLOWED_FEATURES",
            "ghcr.io/devcontainers/features/node",
        )
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "is disallowed by DEACON_DISALLOWED_FEATURES",
        ));
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
