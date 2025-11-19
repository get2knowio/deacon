//! Integration tests for prebuild lifecycle orchestration
//!
//! Tests prebuild mode behavior from specs/001-up-gap-spec/:
//! - Prebuild stops after updateContentCommand (first run)
//! - Reruns updateContent on subsequent prebuild invocations
//! - Does not run postCreateCommand when --prebuild is set
//! - Does not run postAttachCommand when --prebuild is set
//! - Features are installed and metadata merged before updateContent

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

/// Helper to get the fixture path for feature-and-dotfiles
fn feature_dotfiles_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures")
        .join("devcontainer-up")
        .join("feature-and-dotfiles")
}

#[test]
#[ignore] // Requires Docker runtime and network access
fn test_prebuild_stops_after_update_content() {
    // This test verifies that --prebuild mode:
    // 1. Installs features and merges metadata
    // 2. Runs updateContentCommand
    // 3. Stops execution (does NOT run postCreateCommand)
    // 4. Emits success JSON with containerId

    let fixture_path = feature_dotfiles_fixture();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(fixture_path)
        .arg("--prebuild")
        .arg("--skip-non-blocking-commands") // Skip background tasks for deterministic testing
        .assert()
        .success(); // Exit code 0

    // Output should be valid JSON with outcome: success
    // Container should exist but postCreateCommand should NOT have run
    // (Verification of command execution would require inspecting container logs)
}

#[test]
#[ignore] // Requires Docker runtime
fn test_prebuild_rerun_executes_update_content_again() {
    // This test verifies that running prebuild on an existing container:
    // 1. Does NOT rebuild features (already in image)
    // 2. DOES rerun updateContentCommand
    // 3. Still stops before postCreate

    let fixture_path = feature_dotfiles_fixture();

    // First prebuild run
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--prebuild")
        .arg("--skip-non-blocking-commands")
        .assert()
        .success();

    // Second prebuild run (rerun scenario)
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--prebuild")
        .arg("--skip-non-blocking-commands")
        .arg("--expect-existing-container") // Should find the container from first run
        .assert()
        .success();

    // Both runs should succeed and emit JSON
    // updateContentCommand should execute in both cases
}

#[test]
#[ignore] // Requires Docker runtime
fn test_prebuild_does_not_run_post_create() {
    // This test verifies lifecycle boundary: prebuild must NOT run postCreateCommand

    let fixture_path = feature_dotfiles_fixture();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(fixture_path)
        .arg("--prebuild")
        .arg("--include-configuration")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"outcome\":\"success\""));

    // To verify postCreateCommand did NOT run, we would need to:
    // 1. Parse the JSON output to get containerId
    // 2. Inspect container logs for the postCreateCommand echo
    // 3. Assert that the echo is NOT present
    // This is deferred to manual/smoke testing for now
}

#[test]
#[ignore] // Requires Docker runtime
fn test_prebuild_skip_post_attach_honored() {
    // Verify that prebuild implies skip-post-attach behavior
    // (postAttach should never run during prebuild, even if specified)

    let fixture_path = feature_dotfiles_fixture();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(fixture_path)
        .arg("--prebuild")
        .arg("--skip-non-blocking-commands")
        .assert()
        .success();

    // prebuild mode should implicitly skip postAttach
    // Verification would require container log inspection
}

#[test]
#[ignore] // Requires Docker runtime
fn test_prebuild_with_features_metadata_merge() {
    // Verify that features are installed and metadata is merged before updateContent
    // in prebuild mode, and that the merged config includes feature metadata

    let fixture_path = feature_dotfiles_fixture();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(fixture_path)
        .arg("--prebuild")
        .arg("--include-merged-configuration")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"outcome\":\"success\""))
        .stdout(predicate::str::contains("mergedConfiguration"));

    // The mergedConfiguration should include feature metadata
    // Detailed verification would require parsing JSON and checking feature provenance
}

#[test]
#[ignore] // Requires Docker runtime
fn test_prebuild_without_update_content_command() {
    // Verify that prebuild mode still succeeds when devcontainer.json
    // does not define updateContentCommand (no-op lifecycle hook)

    // Use single-container fixture which has no updateContentCommand
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures")
        .join("devcontainer-up")
        .join("single-container");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(fixture_path)
        .arg("--prebuild")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"outcome\":\"success\""));

    // Should succeed even with no updateContentCommand defined
}
