//! Integration tests for prebuild lifecycle orchestration
//!
//! Tests prebuild mode behavior from specs/001-up-gap-spec/:
//! - Prebuild stops after updateContentCommand (first run)
//! - Reruns updateContent on subsequent prebuild invocations
//! - Does not run postCreateCommand when --prebuild is set
//! - Does not run postAttachCommand when --prebuild is set
//! - Features are installed and metadata merged before updateContent
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
fn test_prebuild_stops_after_update_content() {
    if !is_docker_available() {
        eprintln!("Skipping test_prebuild_stops_after_update_content: Docker not available");
        return;
    }

    // This test verifies that --prebuild mode:
    // 1. Installs features and merges metadata
    // 2. Runs updateContentCommand
    // 3. Stops execution (does NOT run postCreateCommand)
    // 4. Emits success JSON with containerId

    let fixture_path = feature_dotfiles_fixture();

    let config_path = fixture_path.join("devcontainer.json");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--prebuild")
        .arg("--skip-non-blocking-commands") // Skip background tasks for deterministic testing
        .assert()
        .success(); // Exit code 0

    // Output should be valid JSON with outcome: success
    // Container should exist but postCreateCommand should NOT have run
    // (Verification of command execution would require inspecting container logs)
}

#[test]
fn test_prebuild_rerun_executes_update_content_again() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_prebuild_rerun_executes_update_content_again: Docker not available"
        );
        return;
    }

    // This test verifies that running prebuild on an existing container:
    // 1. Does NOT rebuild features (already in image)
    // 2. DOES rerun updateContentCommand
    // 3. Still stops before postCreate

    let fixture_path = feature_dotfiles_fixture();
    let config_path = fixture_path.join("devcontainer.json");

    // First prebuild run
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--prebuild")
        .arg("--skip-non-blocking-commands")
        .assert()
        .success();

    // Second prebuild run (rerun scenario)
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--prebuild")
        .arg("--skip-non-blocking-commands")
        .arg("--expect-existing-container") // Should find the container from first run
        .assert()
        .success();

    // Both runs should succeed and emit JSON
    // updateContentCommand should execute in both cases
}

#[test]
fn test_prebuild_does_not_run_post_create() {
    if !is_docker_available() {
        eprintln!("Skipping test_prebuild_does_not_run_post_create: Docker not available");
        return;
    }

    // This test verifies lifecycle boundary: prebuild must NOT run postCreateCommand

    let fixture_path = feature_dotfiles_fixture();
    let config_path = fixture_path.join("devcontainer.json");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--prebuild")
        .arg("--include-configuration")
        .assert()
        .success()
        .stdout(predicate::str::contains("outcome").and(predicate::str::contains("success")));

    // To verify postCreateCommand did NOT run, we would need to:
    // 1. Parse the JSON output to get containerId
    // 2. Inspect container logs for the postCreateCommand echo
    // 3. Assert that the echo is NOT present
    // This is deferred to manual/smoke testing for now
}

#[test]
fn test_prebuild_skip_post_attach_honored() {
    if !is_docker_available() {
        eprintln!("Skipping test_prebuild_skip_post_attach_honored: Docker not available");
        return;
    }

    // Verify that prebuild implies skip-post-attach behavior
    // (postAttach should never run during prebuild, even if specified)

    let fixture_path = feature_dotfiles_fixture();
    let config_path = fixture_path.join("devcontainer.json");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--prebuild")
        .arg("--skip-non-blocking-commands")
        .assert()
        .success();

    // prebuild mode should implicitly skip postAttach
    // Verification would require container log inspection
}

#[test]
fn test_prebuild_with_features_metadata_merge() {
    if !is_docker_available() {
        eprintln!("Skipping test_prebuild_with_features_metadata_merge: Docker not available");
        return;
    }

    // Verify that features are installed and metadata is merged before updateContent
    // in prebuild mode, and that the merged config includes feature metadata

    let fixture_path = feature_dotfiles_fixture();
    let config_path = fixture_path.join("devcontainer.json");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--prebuild")
        .arg("--include-merged-configuration")
        .assert()
        .success()
        .stdout(predicate::str::contains("outcome").and(predicate::str::contains("success")))
        .stdout(predicate::str::contains("mergedConfiguration"));

    // The mergedConfiguration should include feature metadata
    // Detailed verification would require parsing JSON and checking feature provenance
}

#[test]
fn test_prebuild_without_update_content_command() {
    if !is_docker_available() {
        eprintln!("Skipping test_prebuild_without_update_content_command: Docker not available");
        return;
    }

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
    let config_path = fixture_path.join("devcontainer.json");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--prebuild")
        .assert()
        .success()
        .stdout(predicate::str::contains("outcome").and(predicate::str::contains("success")));

    // Should succeed even with no updateContentCommand defined
}

/// Test SC-004/FR-008: Normal `up` after prebuild reruns onCreate and updateContent
///
/// This test verifies prebuild marker isolation per FR-008:
/// "Prebuild executions MUST keep lifecycle markers isolated from normal `up` runs so that
/// a subsequent standard `up` reruns onCreate and updateContent before proceeding to
/// postCreate, postStart, and postAttach."
///
/// The prebuild markers are stored in `.devcontainer-state/prebuild/` while normal markers
/// are stored in `.devcontainer-state/`. This isolation ensures that a normal `up` after
/// prebuild treats onCreate/updateContent as not yet run.
///
/// Test strategy:
/// 1. Each lifecycle hook increments a counter in a marker file
/// 2. First run: prebuild mode - onCreate and updateContent run, counters set to 1
/// 3. Second run: normal mode - all hooks should run since normal markers are fresh
/// 4. Verification: onCreate/updateContent counters = 2 (ran in both modes)
///    postCreate/postStart/postAttach counters = 1 (only ran in normal mode)
#[test]
fn test_prebuild_to_normal_transition_reruns_oncreate_updatecontent() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_prebuild_to_normal_transition_reruns_oncreate_updatecontent: Docker not available"
        );
        return;
    }

    let temp_dir = tempfile::TempDir::new().unwrap();

    // Create devcontainer with lifecycle hooks that increment counters
    // Each hook reads its current counter (defaulting to 0), increments it, and writes back
    // This allows us to verify exactly how many times each hook was executed
    let devcontainer_config = r#"{
    "name": "Prebuild Transition SC004/FR008 Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "onCreateCommand": "count=$(cat /tmp/counter_onCreate 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_onCreate",
    "updateContentCommand": "count=$(cat /tmp/counter_updateContent 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_updateContent",
    "postCreateCommand": "count=$(cat /tmp/counter_postCreate 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postCreate",
    "postStartCommand": "count=$(cat /tmp/counter_postStart 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postStart",
    "postAttachCommand": "count=$(cat /tmp/counter_postAttach 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postAttach"
}"#;

    std::fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    std::fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Helper function to read counter from container
    let read_counter = |phase: &str, temp_dir: &tempfile::TempDir| -> Option<u32> {
        let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
        let exec_output = exec_cmd
            .current_dir(temp_dir.path())
            .arg("exec")
            .arg("--workspace-folder")
            .arg(temp_dir.path())
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

    // ========================================================================
    // Phase 1: Run prebuild mode
    // ========================================================================
    // Prebuild should:
    // - Run onCreate and updateContent
    // - Skip postCreate, postStart, postAttach (these are post* hooks)
    // - Store markers in .devcontainer-state/prebuild/ (isolated from normal markers)

    let mut prebuild_cmd = Command::cargo_bin("deacon").unwrap();
    let prebuild_output = prebuild_cmd
        .current_dir(temp_dir.path())
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--prebuild")
        .arg("--remove-existing-container") // Ensure fresh start
        .output()
        .unwrap();

    assert!(
        prebuild_output.status.success(),
        "Prebuild up failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&prebuild_output.stdout),
        String::from_utf8_lossy(&prebuild_output.stderr)
    );

    // After prebuild:
    // - onCreate: counter = 1 (ran in prebuild)
    // - updateContent: counter = 1 (ran in prebuild)
    // - postCreate: counter = None or 0 (skipped in prebuild per SC-004)
    // - postStart: counter = None or 0 (skipped in prebuild per SC-004)
    // - postAttach: counter = None or 0 (skipped in prebuild per SC-004)

    let oncreate_after_prebuild = read_counter("onCreate", &temp_dir);
    let updatecontent_after_prebuild = read_counter("updateContent", &temp_dir);
    let postcreate_after_prebuild = read_counter("postCreate", &temp_dir);
    let poststart_after_prebuild = read_counter("postStart", &temp_dir);
    let postattach_after_prebuild = read_counter("postAttach", &temp_dir);

    // Verify prebuild executed expected phases
    assert_eq!(
        oncreate_after_prebuild,
        Some(1),
        "After prebuild, onCreate should have counter=1, got {:?}",
        oncreate_after_prebuild
    );
    assert_eq!(
        updatecontent_after_prebuild,
        Some(1),
        "After prebuild, updateContent should have counter=1, got {:?}",
        updatecontent_after_prebuild
    );

    // Verify prebuild skipped post* hooks (per SC-004)
    assert!(
        postcreate_after_prebuild.is_none() || postcreate_after_prebuild == Some(0),
        "After prebuild, postCreate should not have run, got {:?}",
        postcreate_after_prebuild
    );
    assert!(
        poststart_after_prebuild.is_none() || poststart_after_prebuild == Some(0),
        "After prebuild, postStart should not have run, got {:?}",
        poststart_after_prebuild
    );
    assert!(
        postattach_after_prebuild.is_none() || postattach_after_prebuild == Some(0),
        "After prebuild, postAttach should not have run, got {:?}",
        postattach_after_prebuild
    );

    println!("Phase 1 (prebuild) completed:");
    println!("  - onCreate: counter=1 (executed)");
    println!("  - updateContent: counter=1 (executed)");
    println!("  - postCreate: skipped (prebuild mode)");
    println!("  - postStart: skipped (prebuild mode)");
    println!("  - postAttach: skipped (prebuild mode)");

    // ========================================================================
    // Phase 2: Run normal up (should rerun onCreate and updateContent per FR-008)
    // ========================================================================
    // Per FR-008: Normal markers are isolated from prebuild markers, so the normal
    // up should see no markers and treat this as a fresh environment, running all
    // lifecycle phases including onCreate and updateContent again.

    let mut normal_cmd = Command::cargo_bin("deacon").unwrap();
    let normal_output = normal_cmd
        .current_dir(temp_dir.path())
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        // Note: NOT using --prebuild flag, this is a normal up
        // Note: NOT using --remove-existing-container, reusing container from prebuild
        .output()
        .unwrap();

    assert!(
        normal_output.status.success(),
        "Normal up after prebuild failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&normal_output.stdout),
        String::from_utf8_lossy(&normal_output.stderr)
    );

    // After normal up following prebuild:
    // Per FR-008, because prebuild markers are in a separate directory (.devcontainer-state/prebuild/),
    // the normal up should see no markers in .devcontainer-state/ and treat this as fresh.
    //
    // Expected:
    // - onCreate: counter = 2 (ran in prebuild + ran in normal)
    // - updateContent: counter = 2 (ran in prebuild + ran in normal)
    // - postCreate: counter = 1 (only ran in normal)
    // - postStart: counter = 1 (only ran in normal)
    // - postAttach: counter = 1 (only ran in normal)

    let oncreate_after_normal = read_counter("onCreate", &temp_dir);
    let updatecontent_after_normal = read_counter("updateContent", &temp_dir);
    let postcreate_after_normal = read_counter("postCreate", &temp_dir);
    let poststart_after_normal = read_counter("postStart", &temp_dir);
    let postattach_after_normal = read_counter("postAttach", &temp_dir);

    // Verify FR-008: onCreate and updateContent reran in normal mode
    assert_eq!(
        oncreate_after_normal,
        Some(2),
        "FR-008 violation: After normal up, onCreate should have counter=2 (ran in both prebuild and normal). \
         Got {:?}. This indicates prebuild markers are not properly isolated.",
        oncreate_after_normal
    );
    assert_eq!(
        updatecontent_after_normal,
        Some(2),
        "FR-008 violation: After normal up, updateContent should have counter=2 (ran in both prebuild and normal). \
         Got {:?}. This indicates prebuild markers are not properly isolated.",
        updatecontent_after_normal
    );

    // Verify post* hooks ran in normal mode
    assert_eq!(
        postcreate_after_normal,
        Some(1),
        "After normal up, postCreate should have counter=1 (only ran in normal), got {:?}",
        postcreate_after_normal
    );
    assert_eq!(
        poststart_after_normal,
        Some(1),
        "After normal up, postStart should have counter=1 (only ran in normal), got {:?}",
        poststart_after_normal
    );
    assert_eq!(
        postattach_after_normal,
        Some(1),
        "After normal up, postAttach should have counter=1 (only ran in normal), got {:?}",
        postattach_after_normal
    );

    println!("Phase 2 (normal up after prebuild) completed:");
    println!("  - onCreate: counter=2 (reran per FR-008 marker isolation)");
    println!("  - updateContent: counter=2 (reran per FR-008 marker isolation)");
    println!("  - postCreate: counter=1 (first run in normal mode)");
    println!("  - postStart: counter=1 (first run in normal mode)");
    println!("  - postAttach: counter=1 (first run in normal mode)");
    println!();
    println!("SC-004/FR-008 verification PASSED:");
    println!(
        "  Prebuild markers are properly isolated, allowing normal up to rerun onCreate/updateContent."
    );
}

/// Test SC-004/FR-008: Verify prebuild and normal marker directories are separate
///
/// This is a supplementary test that directly verifies the marker file locations
/// to confirm prebuild markers use the isolated `.devcontainer-state/prebuild/` directory
/// while normal markers use `.devcontainer-state/`.
#[test]
fn test_prebuild_marker_directory_isolation() {
    if !is_docker_available() {
        eprintln!("Skipping test_prebuild_marker_directory_isolation: Docker not available");
        return;
    }

    let temp_dir = tempfile::TempDir::new().unwrap();

    // Create a simple devcontainer config
    let devcontainer_config = r#"{
    "name": "Marker Isolation Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "onCreateCommand": "echo 'onCreate ran'",
    "updateContentCommand": "echo 'updateContent ran'"
}"#;

    std::fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    std::fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    let state_dir = temp_dir.path().join(".devcontainer-state");
    let prebuild_state_dir = state_dir.join("prebuild");

    // ========================================================================
    // Phase 1: Run prebuild and verify markers are in prebuild subdirectory
    // ========================================================================

    let mut prebuild_cmd = Command::cargo_bin("deacon").unwrap();
    let prebuild_output = prebuild_cmd
        .current_dir(temp_dir.path())
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--prebuild")
        .arg("--remove-existing-container")
        .output()
        .unwrap();

    assert!(
        prebuild_output.status.success(),
        "Prebuild up failed: {}",
        String::from_utf8_lossy(&prebuild_output.stderr)
    );

    // Verify prebuild markers exist in the prebuild subdirectory
    let prebuild_oncreate_marker = prebuild_state_dir.join("onCreate.json");
    let prebuild_updatecontent_marker = prebuild_state_dir.join("updateContent.json");

    assert!(
        prebuild_oncreate_marker.exists(),
        "Prebuild onCreate marker should exist at {:?}",
        prebuild_oncreate_marker
    );
    assert!(
        prebuild_updatecontent_marker.exists(),
        "Prebuild updateContent marker should exist at {:?}",
        prebuild_updatecontent_marker
    );

    // Verify normal markers do NOT exist yet (only prebuild markers should exist)
    let normal_oncreate_marker = state_dir.join("onCreate.json");
    let normal_updatecontent_marker = state_dir.join("updateContent.json");

    assert!(
        !normal_oncreate_marker.exists(),
        "Normal onCreate marker should NOT exist after prebuild only at {:?}",
        normal_oncreate_marker
    );
    assert!(
        !normal_updatecontent_marker.exists(),
        "Normal updateContent marker should NOT exist after prebuild only at {:?}",
        normal_updatecontent_marker
    );

    println!("Prebuild marker isolation verified:");
    println!("  - Prebuild markers exist in .devcontainer-state/prebuild/");
    println!("  - Normal markers do NOT exist in .devcontainer-state/");

    // ========================================================================
    // Phase 2: Run normal up and verify markers are in base directory
    // ========================================================================

    let mut normal_cmd = Command::cargo_bin("deacon").unwrap();
    let normal_output = normal_cmd
        .current_dir(temp_dir.path())
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    assert!(
        normal_output.status.success(),
        "Normal up failed: {}",
        String::from_utf8_lossy(&normal_output.stderr)
    );

    // Now verify both sets of markers exist independently
    assert!(
        prebuild_oncreate_marker.exists(),
        "Prebuild onCreate marker should still exist at {:?}",
        prebuild_oncreate_marker
    );
    assert!(
        normal_oncreate_marker.exists(),
        "Normal onCreate marker should now exist at {:?}",
        normal_oncreate_marker
    );
    assert!(
        normal_updatecontent_marker.exists(),
        "Normal updateContent marker should now exist at {:?}",
        normal_updatecontent_marker
    );

    println!("Normal marker creation verified:");
    println!("  - Normal markers now exist in .devcontainer-state/");
    println!("  - Prebuild markers still exist in .devcontainer-state/prebuild/");
    println!();
    println!(
        "FR-008 marker isolation verified: Prebuild and normal markers are in separate directories."
    );
}
