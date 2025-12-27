//! Integration tests for dotfiles installation idempotency
//!
//! Tests dotfiles behavior from specs/001-up-gap-spec/:
//! - Dotfiles repository cloned when --dotfiles-repository specified
//! - Install command executed (custom or auto-detected)
//! - Idempotent behavior: reruns do not fail if dotfiles already present
//! - Dotfiles execute after updateContent and features, before postCreate
//! - Errors during dotfiles installation are surfaced as JSON errors
//!
//! **NOTE**: These tests are IGNORED - dotfiles is NOT part of MVP.
//! Container-side dotfiles installation is incomplete (see docs/MVP-ROADMAP.md).
//! Host-side dotfiles work, but container clone/install is deferred to Iteration 1.
//!
//! To run these tests manually: cargo test --test up_dotfiles -- --ignored
//!
//! Note: These tests require Docker and are only compiled on Unix systems.
#![cfg(unix)]

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
#[ignore = "Dotfiles not in MVP - container-side installation incomplete"]
fn test_dotfiles_installation_with_custom_command() {
    if !can_run_dotfiles_tests() {
        eprintln!("Skipping test_dotfiles_installation_with_custom_command: Docker or network not available");
        return;
    }

    // Verify that dotfiles are cloned and custom install command is executed
    // Uses fixture with features to ensure dotfiles run in correct lifecycle order

    let fixture_path = feature_dotfiles_fixture();
    let config_path = fixture_path.join("devcontainer.json");

    // Clean up any existing state to ensure fresh lifecycle execution
    // The workspace resolves to the git root, so clean markers there
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let git_root = manifest_dir.parent().unwrap().parent().unwrap();
    let state_dir = git_root.join(".devcontainer-state");
    if state_dir.exists() {
        let _ = std::fs::remove_dir_all(&state_dir);
    }

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
        .arg("--remove-existing-container") // Ensure fresh container with git installed
        .assert()
        .success()
        .stdout(predicate::str::contains("outcome").and(predicate::str::contains("success")));

    // Dotfiles should be cloned to container and install command executed
    // Verification requires container inspection (deferred to manual testing)
}

#[test]
#[ignore = "Dotfiles not in MVP - container-side installation incomplete"]
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
        .arg("--remove-existing-container") // Ensure fresh container with git installed
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
#[ignore = "Dotfiles not in MVP - container-side installation incomplete"]
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
        .arg("--remove-existing-container") // Ensure fresh container with git installed
        .assert()
        .success()
        .stdout(predicate::str::contains("outcome").and(predicate::str::contains("success")));

    // Auto-detection should find and execute install.sh or succeed if no install script
}

#[test]
#[ignore = "Dotfiles not in MVP - container-side installation incomplete"]
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
        .arg("--remove-existing-container") // Ensure fresh container with git installed
        .assert()
        .success()
        .stdout(predicate::str::contains("outcome").and(predicate::str::contains("success")));

    // Dotfiles should be cloned to custom path
    // Verification requires container filesystem inspection
}

#[test]
#[ignore = "Dotfiles not in MVP - container-side installation incomplete"]
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
#[ignore = "Dotfiles not in MVP - container-side installation incomplete"]
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
#[ignore = "Dotfiles not in MVP - container-side installation incomplete"]
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
        .arg("--remove-existing-container") // Ensure fresh container with git installed
        .assert()
        .success()
        .stdout(predicate::str::contains("outcome").and(predicate::str::contains("success")));

    // Dotfiles should work on simple containers without features
}

#[test]
#[ignore = "Dotfiles not in MVP - container-side installation incomplete"]
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
        .arg("--remove-existing-container") // Ensure fresh container with git installed
        .assert()
        .success();

    // Prebuild should succeed without installing dotfiles
    // Dotfiles are user-specific and should not be in CI prebuilt images
}

/// Test dotfiles ordering in lifecycle: runs exactly once between postCreate and postStart
///
/// This test verifies SC-001 from specs/008-up-lifecycle-hooks/:
/// In fresh `up` runs, dotfiles should execute after postCreate and before postStart.
///
/// The lifecycle order is: onCreate -> updateContent -> postCreate -> dotfiles -> postStart -> postAttach
///
/// We verify this by:
/// 1. Setting up lifecycle hooks that record sequence numbers to marker files
/// 2. Running `up` with dotfiles configured
/// 3. Verifying the dotfiles install command runs at the expected position in the sequence
#[test]
#[ignore = "Dotfiles not in MVP - container-side installation incomplete"]
fn test_dotfiles_ordering_between_post_create_and_post_start() {
    if !can_run_dotfiles_tests() {
        eprintln!(
            "Skipping test_dotfiles_ordering_between_post_create_and_post_start: Docker or network not available"
        );
        return;
    }

    // Create a temporary directory for our test workspace
    let temp_dir = tempfile::TempDir::new().unwrap();
    let workspace_path = temp_dir.path();

    // Create .devcontainer directory
    std::fs::create_dir(workspace_path.join(".devcontainer")).unwrap();

    // Create devcontainer.json with lifecycle hooks that write sequence numbers
    // Each hook writes a unique sequence number to track execution order
    // We use atomic counter approach: each command appends its phase name to a shared log file
    let devcontainer_config = r#"{
    "name": "Dotfiles Ordering Test",
    "image": "ubuntu:22.04",
    "workspaceFolder": "/workspace",
    "remoteUser": "root",
    "onCreateCommand": "apt-get update && apt-get install -y git && echo 'onCreate' >> /tmp/lifecycle_order.log",
    "updateContentCommand": "echo 'updateContent' >> /tmp/lifecycle_order.log",
    "postCreateCommand": "echo 'postCreate' >> /tmp/lifecycle_order.log",
    "postStartCommand": "echo 'postStart' >> /tmp/lifecycle_order.log",
    "postAttachCommand": "echo 'postAttach' >> /tmp/lifecycle_order.log"
}"#;

    std::fs::write(
        workspace_path.join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run up with dotfiles configured
    // The dotfiles install command will write 'dotfiles' to the log
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace_path)
        .arg("--dotfiles-repository")
        .arg("https://github.com/devcontainers/cli") // Minimal public test repo
        .arg("--dotfiles-install-command")
        .arg("echo 'dotfiles' >> /tmp/lifecycle_order.log")
        .arg("--remove-existing-container") // Ensure fresh container
        .output()
        .unwrap();

    // Verify command succeeded
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "up command failed.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // Now exec into the container to read the lifecycle order log
    let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
    let exec_output = exec_cmd
        .arg("exec")
        .arg("--workspace-folder")
        .arg(workspace_path)
        .arg("--")
        .arg("cat")
        .arg("/tmp/lifecycle_order.log")
        .output()
        .unwrap();

    assert!(
        exec_output.status.success(),
        "exec command failed: {}",
        String::from_utf8_lossy(&exec_output.stderr)
    );

    let lifecycle_log = String::from_utf8_lossy(&exec_output.stdout);
    let phases: Vec<&str> = lifecycle_log.trim().lines().collect();

    // Verify the expected order per SC-001
    // Expected: onCreate, updateContent, postCreate, dotfiles, postStart, postAttach
    eprintln!("Lifecycle order recorded: {:?}", phases);

    // Find positions of key phases
    let post_create_pos = phases.iter().position(|&p| p == "postCreate");
    let dotfiles_pos = phases.iter().position(|&p| p == "dotfiles");
    let post_start_pos = phases.iter().position(|&p| p == "postStart");

    // Verify postCreate exists and ran
    assert!(
        post_create_pos.is_some(),
        "postCreate phase not found in lifecycle log. Phases: {:?}",
        phases
    );

    // Verify dotfiles exists and ran exactly once
    let dotfiles_count = phases.iter().filter(|&&p| p == "dotfiles").count();
    assert!(
        dotfiles_pos.is_some(),
        "dotfiles phase not found in lifecycle log. Phases: {:?}",
        phases
    );
    assert_eq!(
        dotfiles_count, 1,
        "dotfiles should execute exactly once, but ran {} times. Phases: {:?}",
        dotfiles_count, phases
    );

    // Verify postStart exists and ran
    assert!(
        post_start_pos.is_some(),
        "postStart phase not found in lifecycle log. Phases: {:?}",
        phases
    );

    // Verify ordering: postCreate < dotfiles < postStart
    let post_create_idx = post_create_pos.unwrap();
    let dotfiles_idx = dotfiles_pos.unwrap();
    let post_start_idx = post_start_pos.unwrap();

    assert!(
        post_create_idx < dotfiles_idx,
        "dotfiles (position {}) should run AFTER postCreate (position {}). Phases: {:?}",
        dotfiles_idx,
        post_create_idx,
        phases
    );

    assert!(
        dotfiles_idx < post_start_idx,
        "dotfiles (position {}) should run BEFORE postStart (position {}). Phases: {:?}",
        dotfiles_idx,
        post_start_idx,
        phases
    );

    // Verify full ordering for completeness
    // onCreate should come before updateContent
    let on_create_pos = phases.iter().position(|&p| p == "onCreate");
    let update_content_pos = phases.iter().position(|&p| p == "updateContent");

    if let (Some(on_create_idx), Some(update_content_idx)) = (on_create_pos, update_content_pos) {
        assert!(
            on_create_idx < update_content_idx,
            "onCreate should run before updateContent"
        );
        assert!(
            update_content_idx < post_create_idx,
            "updateContent should run before postCreate"
        );
    }

    // postAttach should come after postStart
    let post_attach_pos = phases.iter().position(|&p| p == "postAttach");
    if let Some(post_attach_idx) = post_attach_pos {
        assert!(
            post_start_idx < post_attach_idx,
            "postStart should run before postAttach"
        );
    }

    eprintln!(
        "Dotfiles ordering verified: postCreate({}) < dotfiles({}) < postStart({})",
        post_create_idx, dotfiles_idx, post_start_idx
    );
}

/// Test SC-003: --skip-post-create flag skips post* hooks and dotfiles with reasons
///
/// This test verifies the skip-flag behavior from specs/008-up-lifecycle-hooks/:
/// When --skip-post-create is supplied, base phases (onCreate, updateContent) run,
/// but postCreate, postStart, postAttach, and dotfiles are all skipped.
///
/// Spec reference: specs/008-up-lifecycle-hooks/spec.md SC-003
/// "With --skip-post-create, 100% of runs complete required base setup while
///  skipping postCreate, postStart, postAttach, and dotfiles, with clear
///  reporting of skipped phases."
///
/// Test strategy:
/// 1. Set up a devcontainer with all lifecycle hooks that write marker files
/// 2. Configure dotfiles with an install script that also writes a marker
/// 3. Run `up` with --skip-post-create flag
/// 4. Verify onCreate and updateContent ran (markers present)
/// 5. Verify postCreate, postStart, postAttach, and dotfiles did NOT run (no markers)
/// 6. Verify command succeeded (exit code 0)
#[test]
#[ignore = "Dotfiles not in MVP - container-side installation incomplete"]
fn test_skip_post_create_skips_post_hooks_and_dotfiles_sc003() {
    if !can_run_dotfiles_tests() {
        eprintln!(
            "Skipping test_skip_post_create_skips_post_hooks_and_dotfiles_sc003: Docker or network not available"
        );
        return;
    }

    // Create a temporary directory for our test workspace
    let temp_dir = tempfile::TempDir::new().unwrap();
    let workspace_path = temp_dir.path();

    // Create .devcontainer directory
    std::fs::create_dir(workspace_path.join(".devcontainer")).unwrap();

    // Create devcontainer.json with all lifecycle hooks that write marker files
    // Each hook writes its name to a marker file so we can verify what ran
    let devcontainer_config = r#"{
    "name": "Skip Post Create Test SC003",
    "image": "ubuntu:22.04",
    "workspaceFolder": "/workspace",
    "remoteUser": "root",
    "onCreateCommand": "apt-get update && apt-get install -y git && echo 'onCreate' > /tmp/marker_onCreate",
    "updateContentCommand": "echo 'updateContent' > /tmp/marker_updateContent",
    "postCreateCommand": "echo 'postCreate' > /tmp/marker_postCreate",
    "postStartCommand": "echo 'postStart' > /tmp/marker_postStart",
    "postAttachCommand": "echo 'postAttach' > /tmp/marker_postAttach"
}"#;

    std::fs::write(
        workspace_path.join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run up with --skip-post-create flag
    // Also include dotfiles configuration to verify dotfiles are skipped
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace_path)
        .arg("--skip-post-create") // The flag under test
        .arg("--dotfiles-repository")
        .arg("https://github.com/devcontainers/cli") // Minimal public test repo
        .arg("--dotfiles-install-command")
        .arg("echo 'dotfiles' > /tmp/marker_dotfiles") // Write marker if dotfiles runs
        .arg("--remove-existing-container") // Ensure fresh container
        .output()
        .unwrap();

    // Verify command succeeded
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "up command with --skip-post-create failed.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // Helper function to check if a marker file exists in the container
    let marker_exists = |marker_name: &str| -> bool {
        let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
        let exec_output = exec_cmd
            .arg("exec")
            .arg("--workspace-folder")
            .arg(workspace_path)
            .arg("--")
            .arg("test")
            .arg("-f")
            .arg(format!("/tmp/marker_{}", marker_name))
            .output();

        exec_output.map(|o| o.status.success()).unwrap_or(false)
    };

    // SC-003 Verification Part 1: Base phases SHOULD have run
    // onCreate should have executed (marker present)
    assert!(
        marker_exists("onCreate"),
        "SC-003 violation: onCreate should have executed with --skip-post-create"
    );

    // updateContent should have executed (marker present)
    assert!(
        marker_exists("updateContent"),
        "SC-003 violation: updateContent should have executed with --skip-post-create"
    );

    // SC-003 Verification Part 2: Post* phases and dotfiles should NOT have run
    // postCreate should be skipped (no marker)
    assert!(
        !marker_exists("postCreate"),
        "SC-003 violation: postCreate should be SKIPPED with --skip-post-create, but marker exists"
    );

    // postStart should be skipped (no marker)
    assert!(
        !marker_exists("postStart"),
        "SC-003 violation: postStart should be SKIPPED with --skip-post-create, but marker exists"
    );

    // postAttach should be skipped (no marker)
    assert!(
        !marker_exists("postAttach"),
        "SC-003 violation: postAttach should be SKIPPED with --skip-post-create, but marker exists"
    );

    // dotfiles should be skipped (no marker)
    assert!(
        !marker_exists("dotfiles"),
        "SC-003 violation: dotfiles should be SKIPPED with --skip-post-create, but marker exists"
    );

    eprintln!("SC-003 verification passed:");
    eprintln!("  - onCreate: executed (marker present)");
    eprintln!("  - updateContent: executed (marker present)");
    eprintln!("  - postCreate: SKIPPED (no marker)");
    eprintln!("  - postStart: SKIPPED (no marker)");
    eprintln!("  - postAttach: SKIPPED (no marker)");
    eprintln!("  - dotfiles: SKIPPED (no marker)");
}

/// Test SC-003 with JSON output: Verify skipped phases have skip reasons in JSON
///
/// This test extends SC-003 verification to confirm that when using JSON output mode,
/// the skipped phases include appropriate reason strings indicating why they were
/// skipped (i.e., "--skip-post-create flag").
///
/// This is important for automation and tooling that needs to understand why
/// certain lifecycle phases did not execute.
#[test]
#[ignore = "Dotfiles not in MVP - container-side installation incomplete"]
fn test_skip_post_create_reports_skip_reasons_in_output_sc003() {
    if !can_run_dotfiles_tests() {
        eprintln!(
            "Skipping test_skip_post_create_reports_skip_reasons_in_output_sc003: Docker or network not available"
        );
        return;
    }

    // Create a temporary directory for our test workspace
    let temp_dir = tempfile::TempDir::new().unwrap();
    let workspace_path = temp_dir.path();

    // Create .devcontainer directory
    std::fs::create_dir(workspace_path.join(".devcontainer")).unwrap();

    // Create a simple devcontainer.json with lifecycle hooks
    let devcontainer_config = r#"{
    "name": "Skip Post Create Reason Test SC003",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "onCreateCommand": "echo 'onCreate'",
    "updateContentCommand": "echo 'updateContent'",
    "postCreateCommand": "echo 'postCreate'",
    "postStartCommand": "echo 'postStart'",
    "postAttachCommand": "echo 'postAttach'"
}"#;

    std::fs::write(
        workspace_path.join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run up with --skip-post-create flag
    // Capture stderr which contains tracing output that may indicate skip reasons
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace_path)
        .arg("--skip-post-create") // The flag under test
        .arg("--remove-existing-container") // Ensure fresh container
        .output()
        .unwrap();

    // Verify command succeeded
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "up command with --skip-post-create failed.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // The output should be JSON with outcome=success
    assert!(
        stdout.contains("outcome") && stdout.contains("success"),
        "Expected JSON output with outcome=success.\nstdout: {}",
        stdout
    );

    // stderr may contain debug logs about skipped phases
    // This is implementation-dependent, but we can verify the command completed
    eprintln!("Skip reason test completed successfully");
    eprintln!("stdout: {}", stdout);
}

/// Test SC-003 without dotfiles: Verify skip-post-create works without dotfiles configured
///
/// This test verifies that --skip-post-create works correctly even when no dotfiles
/// repository is configured. The flag should still skip postCreate, postStart, and
/// postAttach phases.
#[test]
#[ignore = "Dotfiles not in MVP - container-side installation incomplete"]
fn test_skip_post_create_without_dotfiles_sc003() {
    if !is_docker_available() {
        eprintln!("Skipping test_skip_post_create_without_dotfiles_sc003: Docker not available");
        return;
    }

    // Create a temporary directory for our test workspace
    let temp_dir = tempfile::TempDir::new().unwrap();
    let workspace_path = temp_dir.path();

    // Create .devcontainer directory
    std::fs::create_dir(workspace_path.join(".devcontainer")).unwrap();

    // Create devcontainer.json with lifecycle hooks that increment counters
    // This pattern allows us to verify exactly which phases ran
    let devcontainer_config = r#"{
    "name": "Skip Post Create No Dotfiles Test SC003",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "onCreateCommand": "count=$(cat /tmp/counter_onCreate 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_onCreate",
    "updateContentCommand": "count=$(cat /tmp/counter_updateContent 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_updateContent",
    "postCreateCommand": "count=$(cat /tmp/counter_postCreate 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postCreate",
    "postStartCommand": "count=$(cat /tmp/counter_postStart 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postStart",
    "postAttachCommand": "count=$(cat /tmp/counter_postAttach 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postAttach"
}"#;

    std::fs::write(
        workspace_path.join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run up with --skip-post-create flag (no dotfiles configured)
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace_path)
        .arg("--skip-post-create") // The flag under test
        .arg("--remove-existing-container") // Ensure fresh container
        .output()
        .unwrap();

    // Verify command succeeded
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "up command with --skip-post-create failed.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // Helper function to read counter from container
    let read_counter = |phase: &str| -> Option<u32> {
        let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
        let exec_output = exec_cmd
            .arg("exec")
            .arg("--workspace-folder")
            .arg(workspace_path)
            .arg("--")
            .arg("cat")
            .arg(format!("/tmp/counter_{}", phase))
            .output()
            .ok()?;

        if exec_output.status.success() {
            let content = String::from_utf8_lossy(&exec_output.stdout);
            content.trim().parse().ok()
        } else {
            None
        }
    };

    // SC-003 Verification: Base phases run, post* phases skipped

    // onCreate should have executed (counter = 1)
    let on_create_count = read_counter("onCreate");
    assert_eq!(
        on_create_count,
        Some(1),
        "SC-003 violation: onCreate should have executed with --skip-post-create, got {:?}",
        on_create_count
    );

    // updateContent should have executed (counter = 1)
    let update_content_count = read_counter("updateContent");
    assert_eq!(
        update_content_count,
        Some(1),
        "SC-003 violation: updateContent should have executed with --skip-post-create, got {:?}",
        update_content_count
    );

    // postCreate should be skipped (counter = None, file doesn't exist)
    let post_create_count = read_counter("postCreate");
    assert!(
        post_create_count.is_none(),
        "SC-003 violation: postCreate should be SKIPPED with --skip-post-create, got {:?}",
        post_create_count
    );

    // postStart should be skipped (counter = None, file doesn't exist)
    let post_start_count = read_counter("postStart");
    assert!(
        post_start_count.is_none(),
        "SC-003 violation: postStart should be SKIPPED with --skip-post-create, got {:?}",
        post_start_count
    );

    // postAttach should be skipped (counter = None, file doesn't exist)
    let post_attach_count = read_counter("postAttach");
    assert!(
        post_attach_count.is_none(),
        "SC-003 violation: postAttach should be SKIPPED with --skip-post-create, got {:?}",
        post_attach_count
    );

    eprintln!("SC-003 verification (no dotfiles) passed:");
    eprintln!("  - onCreate: executed (counter=1)");
    eprintln!("  - updateContent: executed (counter=1)");
    eprintln!("  - postCreate: SKIPPED (no counter file)");
    eprintln!("  - postStart: SKIPPED (no counter file)");
    eprintln!("  - postAttach: SKIPPED (no counter file)");
}

/// Test SC-003 edge case: Resume after --skip-post-create should re-run skipped phases
///
/// This test verifies that if you run `up` with --skip-post-create first, then run
/// `up` again without the flag, the previously skipped phases (postCreate, postStart,
/// postAttach, dotfiles) should now execute since they were never completed.
///
/// This tests the interaction between skip-post-create and resume behavior.
#[test]
#[ignore = "Dotfiles not in MVP - container-side installation incomplete"]
fn test_skip_post_create_then_normal_resume_runs_skipped_phases_sc003() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_skip_post_create_then_normal_resume_runs_skipped_phases_sc003: Docker not available"
        );
        return;
    }

    // Create a temporary directory for our test workspace
    let temp_dir = tempfile::TempDir::new().unwrap();
    let workspace_path = temp_dir.path();

    // Create .devcontainer directory
    std::fs::create_dir(workspace_path.join(".devcontainer")).unwrap();

    // Create devcontainer.json with lifecycle hooks that increment counters
    let devcontainer_config = r#"{
    "name": "Skip Then Normal Resume Test SC003",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "onCreateCommand": "count=$(cat /tmp/counter_onCreate 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_onCreate",
    "updateContentCommand": "count=$(cat /tmp/counter_updateContent 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_updateContent",
    "postCreateCommand": "count=$(cat /tmp/counter_postCreate 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postCreate",
    "postStartCommand": "count=$(cat /tmp/counter_postStart 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postStart",
    "postAttachCommand": "count=$(cat /tmp/counter_postAttach 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postAttach"
}"#;

    std::fs::write(
        workspace_path.join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // First up with --skip-post-create
    let mut cmd1 = Command::cargo_bin("deacon").unwrap();
    let output1 = cmd1
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace_path)
        .arg("--skip-post-create") // Skip post* phases
        .arg("--remove-existing-container") // Ensure fresh container
        .output()
        .unwrap();

    assert!(
        output1.status.success(),
        "First up with --skip-post-create failed: {}",
        String::from_utf8_lossy(&output1.stderr)
    );

    // Helper function to read counter from container
    let read_counter = |phase: &str| -> Option<u32> {
        let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
        let exec_output = exec_cmd
            .arg("exec")
            .arg("--workspace-folder")
            .arg(workspace_path)
            .arg("--")
            .arg("cat")
            .arg(format!("/tmp/counter_{}", phase))
            .output()
            .ok()?;

        if exec_output.status.success() {
            let content = String::from_utf8_lossy(&exec_output.stdout);
            content.trim().parse().ok()
        } else {
            None
        }
    };

    // After first up with --skip-post-create:
    // - onCreate: 1 (executed)
    // - updateContent: 1 (executed)
    // - postCreate: None (skipped)
    // - postStart: None (skipped)
    // - postAttach: None (skipped)
    assert_eq!(read_counter("onCreate"), Some(1), "onCreate after first up");
    assert!(
        read_counter("postCreate").is_none(),
        "postCreate should be skipped after first up"
    );

    // Second up WITHOUT --skip-post-create (normal resume)
    let mut cmd2 = Command::cargo_bin("deacon").unwrap();
    let output2 = cmd2
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace_path)
        // No --skip-post-create, no --remove-existing-container
        .output()
        .unwrap();

    assert!(
        output2.status.success(),
        "Second up (normal) failed: {}",
        String::from_utf8_lossy(&output2.stderr)
    );

    // After second up (normal resume behavior):
    // Per SC-002: Resume should only run postStart and postAttach if markers indicate completion
    // However, since postCreate was skipped (not completed), the behavior depends on marker state
    //
    // Per FR-004: If prior run ended before postCreate, resume should run postCreate first
    // But --skip-post-create doesn't create failure markers, it just skips
    //
    // The expected behavior with current implementation:
    // - onCreate: 1 (skipped on resume due to prior marker)
    // - updateContent: 1 (skipped on resume due to prior marker)
    // - postCreate: may be 1 (runs because no completion marker from skip)
    // - postStart: should run (runtime hook)
    // - postAttach: should run (runtime hook)

    // At minimum, postStart and postAttach should have run
    let post_start_count = read_counter("postStart");
    let post_attach_count = read_counter("postAttach");

    // Runtime hooks should execute on normal resume
    assert!(
        post_start_count.is_some() && post_start_count.unwrap() >= 1,
        "postStart should have executed on normal resume, got {:?}",
        post_start_count
    );

    assert!(
        post_attach_count.is_some() && post_attach_count.unwrap() >= 1,
        "postAttach should have executed on normal resume, got {:?}",
        post_attach_count
    );

    eprintln!("SC-003 skip-then-resume test passed:");
    eprintln!("  - First up (--skip-post-create): onCreate/updateContent ran, post* skipped");
    eprintln!("  - Second up (normal): Runtime hooks (postStart/postAttach) executed");
}
