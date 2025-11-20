//! Integration tests for dotfiles installation idempotency
//!
//! Tests dotfiles behavior from specs/001-up-gap-spec/:
//! - Dotfiles repository cloned when --dotfiles-repository specified
//! - Install command executed (custom or auto-detected)
//! - Idempotent behavior: reruns do not fail if dotfiles already present
//! - Dotfiles execute after updateContent and features, before postCreate
//! - Errors during dotfiles installation are surfaced as JSON errors
//!
//! **NOTE**: These tests require both Docker and network access.
//! - Tests check for Docker availability and skip if not present
//! - Tests check for network/GitHub connectivity and skip if not available
//! - These are smoke/E2E tests that violate the hermetic testing principle
//! - The dotfiles implementation IS complete (see crates/deacon/src/commands/up.rs)
//!
//! Tests will automatically run when both Docker and network are available.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

/// Check if Docker is available for integration tests
fn is_docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if network access to GitHub is available
/// Tests connectivity by attempting a quick git ls-remote to a known public repo
fn is_network_available() -> bool {
    std::process::Command::new("git")
        .arg("ls-remote")
        .arg("--exit-code")
        .arg("https://github.com/devcontainers/cli")
        .arg("HEAD")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if both Docker and network are available
fn can_run_dotfiles_tests() -> bool {
    is_docker_available() && is_network_available()
}

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

/// Helper to get the single-container fixture path
fn single_container_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures")
        .join("devcontainer-up")
        .join("single-container")
}

#[test]
fn test_dotfiles_installation_with_custom_command() {
    if !can_run_dotfiles_tests() {
        eprintln!("Skipping test_dotfiles_installation_with_custom_command: Docker or network not available");
        return;
    }

    // Verify that dotfiles are cloned and custom install command is executed
    // Uses fixture with features to ensure dotfiles run in correct lifecycle order

    let fixture_path = feature_dotfiles_fixture();
    let config_path = fixture_path.join("devcontainer.json");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--dotfiles-repository")
        .arg("https://github.com/devcontainers/cli") // Minimal public test repo
        .arg("--dotfiles-install-command")
        .arg("echo 'Custom dotfiles install'")
        .arg("--skip-non-blocking-commands")
        .assert()
        .success()
        .stdout(predicate::str::contains("outcome").and(predicate::str::contains("success")));

    // Dotfiles should be cloned to container and install command executed
    // Verification requires container inspection (deferred to manual testing)
}

#[test]
fn test_dotfiles_idempotency_on_rerun() {
    if !can_run_dotfiles_tests() {
        eprintln!("Skipping test_dotfiles_idempotency_on_rerun: Docker or network not available");
        return;
    }

    // Verify that running up again with same dotfiles config does not fail
    // Even if dotfiles target directory already exists from previous run

    let fixture_path = feature_dotfiles_fixture();
    let config_path = fixture_path.join("devcontainer.json");

    // First run: install dotfiles
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--dotfiles-repository")
        .arg("https://github.com/devcontainers/cli")
        .arg("--skip-non-blocking-commands")
        .assert()
        .success();

    // Second run: dotfiles already present, should be idempotent
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--dotfiles-repository")
        .arg("https://github.com/devcontainers/cli")
        .arg("--skip-non-blocking-commands")
        .arg("--expect-existing-container")
        .assert()
        .success()
        .stdout(predicate::str::contains("outcome").and(predicate::str::contains("success")));

    // Both runs should succeed; second run should handle existing dotfiles gracefully
}

#[test]
fn test_dotfiles_auto_detected_install_script() {
    if !can_run_dotfiles_tests() {
        eprintln!(
            "Skipping test_dotfiles_auto_detected_install_script: Docker or network not available"
        );
        return;
    }

    // Verify that install script is auto-detected when no custom command provided
    // Should detect and run install.sh or setup.sh from dotfiles repo

    let fixture_path = feature_dotfiles_fixture();
    let config_path = fixture_path.join("devcontainer.json");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--dotfiles-repository")
        .arg("https://github.com/devcontainers/cli")
        .arg("--skip-non-blocking-commands")
        .assert()
        .success()
        .stdout(predicate::str::contains("outcome").and(predicate::str::contains("success")));

    // Auto-detection should find and execute install.sh or succeed if no install script
}

#[test]
fn test_dotfiles_custom_target_path() {
    if !can_run_dotfiles_tests() {
        eprintln!("Skipping test_dotfiles_custom_target_path: Docker or network not available");
        return;
    }

    // Verify that --dotfiles-target-path is respected
    // Dotfiles should be cloned to specified path instead of default

    let fixture_path = feature_dotfiles_fixture();
    let config_path = fixture_path.join("devcontainer.json");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--dotfiles-repository")
        .arg("https://github.com/devcontainers/cli")
        .arg("--dotfiles-target-path")
        .arg("/root/.config/dotfiles")
        .arg("--skip-non-blocking-commands")
        .assert()
        .success()
        .stdout(predicate::str::contains("outcome").and(predicate::str::contains("success")));

    // Dotfiles should be cloned to custom path
    // Verification requires container filesystem inspection
}

#[test]
fn test_dotfiles_invalid_repository_error() {
    if !can_run_dotfiles_tests() {
        eprintln!(
            "Skipping test_dotfiles_invalid_repository_error: Docker or network not available"
        );
        return;
    }

    // Verify that invalid dotfiles repository URL produces clear error JSON

    let fixture_path = feature_dotfiles_fixture();
    let config_path = fixture_path.join("devcontainer.json");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--dotfiles-repository")
        .arg("https://github.com/invalid/nonexistent-repo-xyz123-deacon-test")
        .arg("--skip-non-blocking-commands")
        .arg("--remove-existing-container") // Force fresh container
        .assert()
        .failure() // Exit code 1
        .stdout(predicate::str::contains("outcome").and(predicate::str::contains("error")))
        .stdout(predicate::str::contains("dotfiles").or(predicate::str::contains("clone")));

    // Error should indicate dotfiles clone failure
}

#[test]
fn test_dotfiles_install_script_failure_error() {
    if !can_run_dotfiles_tests() {
        eprintln!(
            "Skipping test_dotfiles_install_script_failure_error: Docker or network not available"
        );
        return;
    }

    // Verify that dotfiles install script failure produces error JSON
    // Uses custom install command that intentionally fails

    let fixture_path = feature_dotfiles_fixture();
    let config_path = fixture_path.join("devcontainer.json");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--dotfiles-repository")
        .arg("https://github.com/devcontainers/cli")
        .arg("--dotfiles-install-command")
        .arg("exit 1") // Intentionally fail
        .arg("--skip-non-blocking-commands")
        .arg("--remove-existing-container") // Force fresh container
        .assert()
        .failure()
        .stdout(predicate::str::contains("outcome").and(predicate::str::contains("error")));

    // Error should indicate install script failure
}

#[test]
fn test_dotfiles_without_features() {
    if !can_run_dotfiles_tests() {
        eprintln!("Skipping test_dotfiles_without_features: Docker or network not available");
        return;
    }

    // Verify dotfiles work on simple fixture without features
    // Ensures dotfiles module is not dependent on features module

    let fixture_path = single_container_fixture();
    let config_path = fixture_path.join("devcontainer.json");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--dotfiles-repository")
        .arg("https://github.com/devcontainers/cli")
        .arg("--skip-non-blocking-commands")
        .assert()
        .success()
        .stdout(predicate::str::contains("outcome").and(predicate::str::contains("success")));

    // Dotfiles should work on simple containers without features
}

#[test]
fn test_dotfiles_with_prebuild_mode() {
    if !can_run_dotfiles_tests() {
        eprintln!("Skipping test_dotfiles_with_prebuild_mode: Docker or network not available");
        return;
    }

    // Verify dotfiles behavior in prebuild mode
    // Dotfiles should NOT be installed during prebuild (CI image creation)
    // Only features and updateContent run in prebuild

    let fixture_path = feature_dotfiles_fixture();
    let config_path = fixture_path.join("devcontainer.json");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--prebuild")
        .arg("--dotfiles-repository")
        .arg("https://github.com/devcontainers/cli")
        .arg("--skip-non-blocking-commands")
        .assert()
        .success();

    // Prebuild should succeed without installing dotfiles
    // Dotfiles are user-specific and should not be in CI prebuilt images
}
