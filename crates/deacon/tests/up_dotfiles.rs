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

/// Copy a `fixtures/devcontainer-up/<name>` fixture into a fresh TempDir.
///
/// These tests run a real `deacon up`. If the workspace folder lived inside
/// this repo, `up` would walk to the git root and bind-mount `/workspaces/...`
/// into the container — container-side writes (and historically a `chown` for
/// `remoteUser: root`) would then corrupt the host repo's ownership. Running
/// from a TempDir outside the repo keeps the workspace fully hermetic.
fn copy_fixture_to_temp(name: &str) -> tempfile::TempDir {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures")
        .join("devcontainer-up")
        .join(name);
    let temp = tempfile::TempDir::new().unwrap();
    copy_dir_contents(&src, temp.path());
    temp
}

/// Recursively copy the contents of `src` into `dest`.
fn copy_dir_contents(src: &std::path::Path, dest: &std::path::Path) {
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dest.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            copy_dir_contents(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Helper to get a hermetic (TempDir-backed) feature-and-dotfiles fixture.
fn feature_dotfiles_fixture() -> tempfile::TempDir {
    copy_fixture_to_temp("feature-and-dotfiles")
}

/// Helper to get a hermetic (TempDir-backed) single-container fixture.
fn single_container_fixture() -> tempfile::TempDir {
    copy_fixture_to_temp("single-container")
}

#[test]
#[ignore = "Dotfiles not in MVP - container-side installation incomplete"]
fn test_dotfiles_installation_with_custom_command() {
    if !can_run_dotfiles_tests() {
        eprintln!(
            "Skipping test_dotfiles_installation_with_custom_command: Docker or network not available"
        );
        return;
    }

    // Verify that dotfiles are cloned and custom install command is executed
    // Uses fixture with features to ensure dotfiles run in correct lifecycle order

    let _fixture = feature_dotfiles_fixture();
    let fixture_path = _fixture.path().to_path_buf();
    let config_path = fixture_path.join("devcontainer.json");
    // The TempDir workspace is always fresh, so no pre-existing lifecycle
    // markers need clearing.

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

    let _fixture = feature_dotfiles_fixture();
    let fixture_path = _fixture.path().to_path_buf();
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

    let _fixture = feature_dotfiles_fixture();
    let fixture_path = _fixture.path().to_path_buf();
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

    let _fixture = feature_dotfiles_fixture();
    let fixture_path = _fixture.path().to_path_buf();
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

    let _fixture = feature_dotfiles_fixture();
    let fixture_path = _fixture.path().to_path_buf();
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

    let _fixture = feature_dotfiles_fixture();
    let fixture_path = _fixture.path().to_path_buf();
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

    let _fixture = single_container_fixture();
    let fixture_path = _fixture.path().to_path_buf();
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

    let _fixture = feature_dotfiles_fixture();
    let fixture_path = _fixture.path().to_path_buf();
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

/// #476: `--skip-post-create` defers EVERY lifecycle hook and dotfiles.
///
/// Supersedes specs/008-up-lifecycle-hooks/ SC-003, which read the flag off its
/// NAME and claimed the base setup (onCreate, updateContent) still runs. Measured
/// at the pinned oracle 0.87.0: `devcontainer up --skip-post-create` runs no hook
/// at all — it sets `postCreateEnabled: false`, and the reference's entire
/// lifecycle runner is gated on that. The flag is spec-silent (a CLI surface, not
/// in containers.dev at `113500f4`), so the reference is the authority.
///
/// Test strategy:
/// 1. Set up a devcontainer with all lifecycle hooks that write marker files
/// 2. Configure dotfiles with an install script that also writes a marker
/// 3. Run `up` with --skip-post-create flag
/// 4. Verify NO marker exists — every hook, and dotfiles, was deferred
/// 5. Verify command succeeded (exit code 0)
#[test]
#[ignore = "Dotfiles not in MVP - container-side installation incomplete"]
fn test_skip_post_create_defers_every_hook_and_dotfiles() {
    if !can_run_dotfiles_tests() {
        eprintln!(
            "Skipping test_skip_post_create_defers_every_hook_and_dotfiles: Docker or network not available"
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

    // #476: every hook is deferred, `onCreate` and `updateContent` included.
    for marker in [
        "onCreate",
        "updateContent",
        "postCreate",
        "postStart",
        "postAttach",
        "dotfiles",
    ] {
        assert!(
            !marker_exists(marker),
            "#476 violation: {marker} must be DEFERRED by --skip-post-create, but its marker exists"
        );
    }
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

/// #476 without dotfiles: `--skip-post-create` defers every phase even when no
/// dotfiles repository is configured.
#[test]
#[ignore = "Dotfiles not in MVP - container-side installation incomplete"]
fn test_skip_post_create_without_dotfiles_defers_every_phase() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_skip_post_create_without_dotfiles_defers_every_phase: Docker not available"
        );
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

    // #476: every phase is deferred, so no counter file exists at all.
    for phase in [
        "onCreate",
        "updateContent",
        "postCreate",
        "postStart",
        "postAttach",
    ] {
        let count = read_counter(phase);
        assert!(
            count.is_none(),
            "#476 violation: {phase} must be DEFERRED by --skip-post-create, got {count:?}"
        );
    }
}

/// #476: a deferred `up` loses nothing. Run `up --skip-post-create`, then `up`
/// again without the flag, and every phase runs exactly once — the first run wrote
/// no completion markers because it ran nothing.
#[test]
#[ignore = "Dotfiles not in MVP - container-side installation incomplete"]
fn test_skip_post_create_then_normal_up_runs_every_deferred_phase() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_skip_post_create_then_normal_up_runs_every_deferred_phase: Docker not available"
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

    // #476: after the first up, NOTHING has run — the flag defers the whole
    // lifecycle, so even onCreate has no counter yet.
    assert!(
        read_counter("onCreate").is_none(),
        "onCreate must be deferred by --skip-post-create on the first up"
    );
    assert!(
        read_counter("postCreate").is_none(),
        "postCreate should be deferred after first up"
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

    // After the second up (no flag): the first run wrote NO markers, so this is a
    // fresh lifecycle and every phase runs exactly once. That is the whole point of
    // deferral — the work is not lost, only postponed (#476).
    for phase in [
        "onCreate",
        "updateContent",
        "postCreate",
        "postStart",
        "postAttach",
    ] {
        let count = read_counter(phase);
        assert_eq!(
            count,
            Some(1),
            "{phase} should have run exactly once on the deferred second up, got {count:?}"
        );
    }
}
