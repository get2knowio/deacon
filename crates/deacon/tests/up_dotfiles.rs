//! Integration tests for dotfiles installation idempotency
//!
//! Tests dotfiles behavior from specs/001-up-gap-spec/:
//! - Dotfiles repository cloned when --dotfiles-repository specified
//! - Install command executed (custom or auto-detected)
//! - Idempotent behavior: reruns do not fail if dotfiles already present
//! - Dotfiles execute after updateContent and features, before postCreate
//! - Errors during dotfiles installation are surfaced as JSON errors

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
#[ignore] // Requires Docker runtime and network access for dotfiles repo
fn test_dotfiles_installation_with_custom_command() {
    // Verify that dotfiles are cloned and custom install command is executed
    // Uses fixture with features to ensure dotfiles run in correct lifecycle order

    let fixture_path = feature_dotfiles_fixture();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(fixture_path)
        .arg("--dotfiles-repository")
        .arg("https://github.com/example/dotfiles") // Requires public test repo
        .arg("--dotfiles-install-command")
        .arg("echo 'Custom dotfiles install'")
        .arg("--skip-non-blocking-commands")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"outcome\":\"success\""));

    // Dotfiles should be cloned to container and install command executed
    // Verification requires container inspection (deferred to manual testing)
}

#[test]
#[ignore] // Requires Docker runtime and network access
fn test_dotfiles_idempotency_on_rerun() {
    // Verify that running up again with same dotfiles config does not fail
    // Even if dotfiles target directory already exists from previous run

    let fixture_path = feature_dotfiles_fixture();

    // First run: install dotfiles
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--dotfiles-repository")
        .arg("https://github.com/example/dotfiles")
        .arg("--skip-non-blocking-commands")
        .assert()
        .success();

    // Second run: dotfiles already present, should be idempotent
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--dotfiles-repository")
        .arg("https://github.com/example/dotfiles")
        .arg("--skip-non-blocking-commands")
        .arg("--expect-existing-container")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"outcome\":\"success\""));

    // Both runs should succeed; second run should handle existing dotfiles gracefully
}

#[test]
#[ignore] // Requires Docker runtime
fn test_dotfiles_auto_detected_install_script() {
    // Verify that install script is auto-detected when no custom command provided
    // Should detect and run install.sh or setup.sh from dotfiles repo

    let fixture_path = feature_dotfiles_fixture();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(fixture_path)
        .arg("--dotfiles-repository")
        .arg("https://github.com/example/dotfiles-with-install-sh")
        .arg("--skip-non-blocking-commands")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"outcome\":\"success\""));

    // Auto-detection should find and execute install.sh
}

#[test]
#[ignore] // Requires Docker runtime
fn test_dotfiles_custom_target_path() {
    // Verify that --dotfiles-target-path is respected
    // Dotfiles should be cloned to specified path instead of default

    let fixture_path = feature_dotfiles_fixture();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(fixture_path)
        .arg("--dotfiles-repository")
        .arg("https://github.com/example/dotfiles")
        .arg("--dotfiles-target-path")
        .arg("/home/vscode/.config/dotfiles")
        .arg("--skip-non-blocking-commands")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"outcome\":\"success\""));

    // Dotfiles should be cloned to custom path
    // Verification requires container filesystem inspection
}

#[test]
#[ignore] // Requires Docker runtime and network access
fn test_dotfiles_invalid_repository_error() {
    // Verify that invalid dotfiles repository URL produces clear error JSON

    let fixture_path = feature_dotfiles_fixture();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(fixture_path)
        .arg("--dotfiles-repository")
        .arg("https://github.com/invalid/nonexistent-repo-xyz123")
        .arg("--skip-non-blocking-commands")
        .assert()
        .failure() // Exit code 1
        .stdout(predicate::str::contains("\"outcome\":\"error\""))
        .stdout(predicate::str::contains("dotfiles").or(predicate::str::contains("clone")));

    // Error should indicate dotfiles clone failure
}

#[test]
#[ignore] // Requires Docker runtime
fn test_dotfiles_install_script_failure_error() {
    // Verify that dotfiles install script failure produces error JSON
    // Uses dotfiles repo with intentionally failing install script

    let fixture_path = feature_dotfiles_fixture();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(fixture_path)
        .arg("--dotfiles-repository")
        .arg("https://github.com/example/dotfiles-failing-install")
        .arg("--skip-non-blocking-commands")
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"outcome\":\"error\""))
        .stdout(predicate::str::contains("install"));

    // Error should indicate install script failure
}

#[test]
#[ignore] // Requires Docker runtime
fn test_dotfiles_without_features() {
    // Verify dotfiles work on simple fixture without features
    // Ensures dotfiles module is not dependent on features module

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
        .arg("--dotfiles-repository")
        .arg("https://github.com/example/dotfiles")
        .arg("--skip-non-blocking-commands")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"outcome\":\"success\""));

    // Dotfiles should work on simple containers without features
}

#[test]
#[ignore] // Requires Docker runtime
fn test_dotfiles_with_prebuild_mode() {
    // Verify dotfiles behavior in prebuild mode
    // Dotfiles should NOT be installed during prebuild (CI image creation)
    // Only features and updateContent run in prebuild

    let fixture_path = feature_dotfiles_fixture();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(fixture_path)
        .arg("--prebuild")
        .arg("--dotfiles-repository")
        .arg("https://github.com/example/dotfiles")
        .arg("--skip-non-blocking-commands")
        .assert()
        .success();

    // Prebuild should succeed without installing dotfiles
    // Dotfiles are user-specific and should not be in CI prebuilt images
}
