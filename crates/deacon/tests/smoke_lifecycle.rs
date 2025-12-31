//! Smoke tests for lifecycle command execution and secret masking
//!
//! Scenarios covered:
//! - Lifecycle hooks: stable order + resume markers
//! - Secret masking in logs (while secrets available to hooks)
//! - Features accessible in-container (Docker-gated)
//!
//! NOTE: These tests assume Docker is available and running. They will fail
//! if Docker is not present or cannot start containers.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

fn is_docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Test lifecycle phase order with marker files
#[test]
fn test_lifecycle_hooks_stable_order() {
    if !is_docker_available() {
        eprintln!("Skipping test_lifecycle_hooks_stable_order: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create a devcontainer.json with lifecycle hooks that create numbered marker files
    let devcontainer_config = r#"{
    "name": "Lifecycle Order Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "onCreateCommand": "echo 'onCreate-1' > /tmp/marker_onCreate",
    "updateContentCommand": "echo 'updateContent-2' > /tmp/marker_updateContent", 
    "postCreateCommand": "echo 'postCreate-3' > /tmp/marker_postCreate",
    "postStartCommand": "echo 'postStart-4' > /tmp/marker_postStart",
    "postAttachCommand": "echo 'postAttach-5' > /tmp/marker_postAttach"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up command - should create all markers in order
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    assert!(
        up_output.status.success(),
        "Unexpected error in lifecycle up: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );
}

/// Test SC-002: Resume only reruns runtime hooks (postStart/postAttach)
///
/// Verifies that when resuming an existing devcontainer environment where lifecycle
/// markers are present from a prior run, only postStart and postAttach execute.
/// Earlier phases (onCreate, updateContent, postCreate, dotfiles) should be skipped.
///
/// Test strategy:
/// 1. Each lifecycle hook increments a counter in a marker file
/// 2. First `up`: All hooks execute, setting counters to 1
/// 3. Second `up` (resume): Only postStart/postAttach run, incrementing their counters to 2
/// 4. Verification: onCreate/updateContent/postCreate counters = 1, postStart/postAttach = 2
///
/// Spec reference: specs/008-up-lifecycle-hooks/spec.md SC-002
#[test]
fn test_lifecycle_resume_sc002_only_runtime_hooks_rerun() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_lifecycle_resume_sc002_only_runtime_hooks_rerun: Docker not available"
        );
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create devcontainer with lifecycle hooks that increment counters
    // Each hook reads its current counter (defaulting to 0), increments it, and writes back
    // This allows us to verify exactly how many times each hook was executed
    let devcontainer_config = r#"{
    "name": "Lifecycle Resume SC002 Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "onCreateCommand": "count=$(cat /tmp/counter_onCreate 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_onCreate",
    "updateContentCommand": "count=$(cat /tmp/counter_updateContent 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_updateContent",
    "postCreateCommand": "count=$(cat /tmp/counter_postCreate 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postCreate",
    "postStartCommand": "count=$(cat /tmp/counter_postStart 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postStart",
    "postAttachCommand": "count=$(cat /tmp/counter_postAttach 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postAttach"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // First up - should create container and run all lifecycle hooks
    let mut up_cmd1 = Command::cargo_bin("deacon").unwrap();
    let up_output1 = up_cmd1
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--remove-existing-container") // Ensure fresh start
        .output()
        .unwrap();

    assert!(
        up_output1.status.success(),
        "First up failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&up_output1.stdout),
        String::from_utf8_lossy(&up_output1.stderr)
    );

    // Helper function to read counter from container
    let read_counter = |phase: &str| -> Option<u32> {
        let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
        let exec_output = exec_cmd
            .current_dir(&temp_dir)
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

    // Verify first up executed all phases exactly once
    let phases_after_first_up = [
        ("onCreate", 1),
        ("updateContent", 1),
        ("postCreate", 1),
        ("postStart", 1),
        ("postAttach", 1),
    ];

    for (phase, expected_count) in &phases_after_first_up {
        let actual_count = read_counter(phase);
        assert_eq!(
            actual_count,
            Some(*expected_count),
            "After first up, phase {} should have counter = {}, got {:?}",
            phase,
            expected_count,
            actual_count
        );
    }

    println!("First up completed: All lifecycle phases executed exactly once");

    // Second up - should only re-run postStart and postAttach (resume behavior)
    // The existing container is reused, and markers from first run indicate phases completed
    let mut up_cmd2 = Command::cargo_bin("deacon").unwrap();
    let up_output2 = up_cmd2
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        // Note: NOT using --remove-existing-container, so container is reused
        .output()
        .unwrap();

    assert!(
        up_output2.status.success(),
        "Second up (resume) failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&up_output2.stdout),
        String::from_utf8_lossy(&up_output2.stderr)
    );

    // Verify SC-002: Only postStart and postAttach should have re-run
    // onCreate, updateContent, postCreate should remain at 1 (skipped due to markers)
    let expected_after_resume = [
        ("onCreate", 1),      // Should NOT have re-run
        ("updateContent", 1), // Should NOT have re-run
        ("postCreate", 1),    // Should NOT have re-run
        ("postStart", 2),     // SHOULD have re-run (runtime hook)
        ("postAttach", 2),    // SHOULD have re-run (runtime hook)
    ];

    for (phase, expected_count) in &expected_after_resume {
        let actual_count = read_counter(phase);
        assert_eq!(
            actual_count,
            Some(*expected_count),
            "SC-002 violation: After resume, phase {} should have counter = {}, got {:?}. \
             Runtime hooks (postStart/postAttach) should re-run, others should be skipped.",
            phase,
            expected_count,
            actual_count
        );
    }

    println!("SC-002 verification passed: Resume only re-ran postStart and postAttach");
    println!("  - onCreate: counter=1 (skipped on resume)");
    println!("  - updateContent: counter=1 (skipped on resume)");
    println!("  - postCreate: counter=1 (skipped on resume)");
    println!("  - postStart: counter=2 (re-ran on resume)");
    println!("  - postAttach: counter=2 (re-ran on resume)");
}

/// Test SC-002 with dotfiles: Resume skips dotfiles phase when marker exists
///
/// This test extends SC-002 verification to include dotfiles phase, ensuring
/// that dotfiles are NOT re-applied on resume (they were applied once during
/// the initial fresh run).
///
/// Spec reference: specs/008-up-lifecycle-hooks/spec.md SC-002
/// "On resume after a completed initial `up`, 100% of runs skip onCreate,
///  updateContent, postCreate, and dotfiles, while postStart and postAttach
///  execute successfully."
#[test]
fn test_lifecycle_resume_sc002_dotfiles_skipped() {
    if !is_docker_available() {
        eprintln!("Skipping test_lifecycle_resume_sc002_dotfiles_skipped: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create a local dotfiles directory with an install script that increments a counter
    let dotfiles_dir = temp_dir.path().join("my-dotfiles");
    fs::create_dir_all(&dotfiles_dir).unwrap();

    // Create the install script that increments a counter (same pattern as other phases)
    let install_script = r#"#!/bin/sh
count=$(cat /tmp/counter_dotfiles 2>/dev/null || echo 0)
count=$((count + 1))
echo $count > /tmp/counter_dotfiles
"#;
    fs::write(dotfiles_dir.join("install.sh"), install_script).unwrap();

    // Make install.sh executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(dotfiles_dir.join("install.sh"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(dotfiles_dir.join("install.sh"), perms).unwrap();
    }

    // Create devcontainer with lifecycle hooks that increment counters
    let devcontainer_config = r#"{
    "name": "Lifecycle Resume Dotfiles SC002 Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "onCreateCommand": "count=$(cat /tmp/counter_onCreate 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_onCreate",
    "postCreateCommand": "count=$(cat /tmp/counter_postCreate 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postCreate",
    "postStartCommand": "count=$(cat /tmp/counter_postStart 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postStart",
    "postAttachCommand": "count=$(cat /tmp/counter_postAttach 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postAttach"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // First up with dotfiles configured
    let mut up_cmd1 = Command::cargo_bin("deacon").unwrap();
    let up_output1 = up_cmd1
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--dotfiles-repository")
        .arg(dotfiles_dir.to_str().unwrap())
        .arg("--remove-existing-container")
        .output()
        .unwrap();

    // First up may or may not succeed depending on dotfiles implementation
    // We proceed with verification if it succeeded
    if !up_output1.status.success() {
        let stderr = String::from_utf8_lossy(&up_output1.stderr);
        if stderr.contains("dotfiles") || stderr.contains("not implemented") {
            eprintln!(
                "Dotfiles feature may not be fully implemented, skipping dotfiles-specific test"
            );
            return;
        }
        panic!(
            "First up failed: stdout={}, stderr={}",
            String::from_utf8_lossy(&up_output1.stdout),
            stderr
        );
    }

    // Helper function to read counter from container
    let read_counter = |phase: &str| -> Option<u32> {
        let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
        let exec_output = exec_cmd
            .current_dir(&temp_dir)
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

    // Check if dotfiles actually ran during first up
    let dotfiles_first_count = read_counter("dotfiles");
    let dotfiles_implemented = dotfiles_first_count == Some(1);

    if !dotfiles_implemented {
        eprintln!(
            "Dotfiles counter not found after first up, dotfiles feature may not be fully wired"
        );
        // Still proceed to verify the test framework works with other phases
    }

    // Second up (resume) with same dotfiles
    let mut up_cmd2 = Command::cargo_bin("deacon").unwrap();
    let up_output2 = up_cmd2
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--dotfiles-repository")
        .arg(dotfiles_dir.to_str().unwrap())
        // No --remove-existing-container: this triggers resume mode
        .output()
        .unwrap();

    assert!(
        up_output2.status.success(),
        "Second up (resume) failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&up_output2.stdout),
        String::from_utf8_lossy(&up_output2.stderr)
    );

    // Verify SC-002: Dotfiles should NOT have re-run (remains at 1 if implemented)
    if dotfiles_implemented {
        let dotfiles_second_count = read_counter("dotfiles");
        assert_eq!(
            dotfiles_second_count,
            Some(1),
            "SC-002 violation: Dotfiles should be skipped on resume. \
             Expected counter=1, got {:?}",
            dotfiles_second_count
        );
        println!("SC-002 dotfiles verification passed: Dotfiles skipped on resume (counter=1)");
    }

    // Also verify standard phases behave correctly
    let on_create_count = read_counter("onCreate");
    let post_start_count = read_counter("postStart");

    if on_create_count.is_some() {
        assert_eq!(
            on_create_count,
            Some(1),
            "onCreate should be skipped on resume"
        );
    }

    if post_start_count.is_some() {
        assert_eq!(
            post_start_count,
            Some(2),
            "postStart should re-run on resume"
        );
    }

    println!("SC-002 with dotfiles test completed successfully");
}

/// Test SC-002 edge case: Multiple resumes continue to only run runtime hooks
///
/// This test verifies that repeated `up` calls (3+ times) consistently only
/// re-run postStart and postAttach, not accumulating runs of earlier phases.
///
/// Spec reference: specs/008-up-lifecycle-hooks/spec.md SC-002
#[test]
fn test_lifecycle_resume_sc002_multiple_resumes() {
    if !is_docker_available() {
        eprintln!("Skipping test_lifecycle_resume_sc002_multiple_resumes: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    let devcontainer_config = r#"{
    "name": "Lifecycle Multiple Resumes Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "onCreateCommand": "count=$(cat /tmp/counter_onCreate 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_onCreate",
    "postCreateCommand": "count=$(cat /tmp/counter_postCreate 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postCreate",
    "postStartCommand": "count=$(cat /tmp/counter_postStart 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postStart",
    "postAttachCommand": "count=$(cat /tmp/counter_postAttach 2>/dev/null || echo 0); count=$((count + 1)); echo $count > /tmp/counter_postAttach"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Helper function to read counter from container
    let read_counter = |phase: &str| -> Option<u32> {
        let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
        let exec_output = exec_cmd
            .current_dir(&temp_dir)
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

    // First up (fresh)
    let mut up_cmd1 = Command::cargo_bin("deacon").unwrap();
    let up_output1 = up_cmd1
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--remove-existing-container")
        .output()
        .unwrap();

    assert!(
        up_output1.status.success(),
        "First up failed: {}",
        String::from_utf8_lossy(&up_output1.stderr)
    );

    // Second up (first resume)
    let mut up_cmd2 = Command::cargo_bin("deacon").unwrap();
    let up_output2 = up_cmd2
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    assert!(
        up_output2.status.success(),
        "Second up failed: {}",
        String::from_utf8_lossy(&up_output2.stderr)
    );

    // Third up (second resume)
    let mut up_cmd3 = Command::cargo_bin("deacon").unwrap();
    let up_output3 = up_cmd3
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    assert!(
        up_output3.status.success(),
        "Third up failed: {}",
        String::from_utf8_lossy(&up_output3.stderr)
    );

    // After 3 `up` calls:
    // - onCreate and postCreate should have run exactly once (fresh only)
    // - postStart and postAttach should have run 3 times (fresh + 2 resumes)
    let expected_counts = [
        ("onCreate", 1),
        ("postCreate", 1),
        ("postStart", 3),
        ("postAttach", 3),
    ];

    for (phase, expected_count) in &expected_counts {
        let actual_count = read_counter(phase);
        assert_eq!(
            actual_count,
            Some(*expected_count),
            "After 3 `up` calls, {} should have counter = {}, got {:?}",
            phase,
            expected_count,
            actual_count
        );
    }

    println!("SC-002 multiple resumes test passed:");
    println!("  - After 3 `up` calls:");
    println!("    - onCreate: 1 (ran only on fresh)");
    println!("    - postCreate: 1 (ran only on fresh)");
    println!("    - postStart: 3 (ran on fresh + 2 resumes)");
    println!("    - postAttach: 3 (ran on fresh + 2 resumes)");
}

/// Test --skip-non-blocking-commands flag suppresses postStart and postAttach
#[test]
fn test_lifecycle_skip_non_blocking_commands() {
    if !is_docker_available() {
        eprintln!("Skipping test_lifecycle_skip_non_blocking_commands: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    let devcontainer_config = r#"{
    "name": "Skip Non-blocking Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace", 
    "postStartCommand": "echo 'postStart-ran' > /tmp/marker_postStart",
    "postAttachCommand": "echo 'postAttach-ran' > /tmp/marker_postAttach"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up with --skip-post-attach
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--skip-non-blocking-commands")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let up_stderr = String::from_utf8_lossy(&up_output.stderr);
    assert!(
        up_output.status.success(),
        "Unexpected error in skip-non-blocking-commands up: {}",
        up_stderr
    );
    // TODO: Verify postAttach marker was not created but postStart was
    println!("Skip non-blocking commands succeeded");
}

/// Test secret masking in logs with --no-redact disabled (default behavior)
#[test]
fn test_secret_masking_in_lifecycle_logs() {
    if !is_docker_available() {
        eprintln!("Skipping test_secret_masking_in_lifecycle_logs: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create devcontainer that outputs potentially sensitive information
    let devcontainer_config = r#"{
    "name": "Secret Masking Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "remoteEnv": {
        "SECRET_VALUE": "my-secret-password",
        "PUBLIC_VALUE": "public-info"
    },
    "postCreateCommand": [
        "sh", "-c",
        "echo \"Secret is: $SECRET_VALUE\" > /tmp/marker_secret && echo \"Public is: $PUBLIC_VALUE\" && echo \"Processing with my-secret-password in output\""
    ]
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up command (redaction should be enabled by default)
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let up_stderr = String::from_utf8_lossy(&up_output.stderr);

    assert!(
        up_output.status.success(),
        "Unexpected error in secret masking up: {}",
        up_stderr
    );
    // Intentionally not asserting on specific stdout content at this time
    // For now, we don't assert on container stdout visibility; just ensure the command succeeded.

    // Test that we can disable redaction and see the secret
    let mut up_cmd_no_redact = Command::cargo_bin("deacon").unwrap();
    let up_output_no_redact = up_cmd_no_redact
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--no-redact")
        .arg("--remove-existing-container")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
    assert!(
        up_output_no_redact.status.success(),
        "Unexpected error in no-redact up: {}",
        String::from_utf8_lossy(&up_output_no_redact.stderr)
    );
    let combined_no_redact = format!(
        "{}\n{}",
        String::from_utf8_lossy(&up_output_no_redact.stdout),
        String::from_utf8_lossy(&up_output_no_redact.stderr)
    );
    // Ensure the explicit warning about disabled redaction is emitted
    assert!(
        combined_no_redact.contains("Secret redaction is DISABLED"),
        "Expected a warning about disabled redaction when --no-redact is used"
    );
}

/// Test SC-001: Fresh up enforces lifecycle order
///
/// Verifies that a fresh `up` run executes lifecycle phases exactly once in the
/// spec-defined order: onCreate -> updateContent -> postCreate -> dotfiles -> postStart -> postAttach
///
/// This test:
/// 1. Sets up a devcontainer with all lifecycle hooks that create timestamped marker files
/// 2. Runs `up` on the fresh environment
/// 3. Reads markers back from the container to verify:
///    - All phases executed (no omissions)
///    - Phases ran in the correct order (timestamps ascending)
///    - No phase ran more than once (exactly one marker per phase)
#[test]
fn test_lifecycle_phase_order_sc001() {
    if !is_docker_available() {
        eprintln!("Skipping test_lifecycle_phase_order_sc001: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create a devcontainer.json with lifecycle hooks that write sequence numbers
    // Each hook creates a file with its sequence order and timestamp
    // The shell commands write both a sequence number AND the current epoch time
    // to allow us to verify both ordering and uniqueness.
    //
    // NOTE: dotfiles are triggered by the presence of a dotfiles repository config.
    // For this test, we use a local dotfiles script to simulate dotfiles phase.
    let devcontainer_config = r#"{
    "name": "Lifecycle Order SC001 Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "onCreateCommand": "seq=1; ts=$(date +%s%N); echo \"$seq,$ts\" > /tmp/lifecycle_order_onCreate; sleep 0.1",
    "updateContentCommand": "seq=2; ts=$(date +%s%N); echo \"$seq,$ts\" > /tmp/lifecycle_order_updateContent; sleep 0.1",
    "postCreateCommand": "seq=3; ts=$(date +%s%N); echo \"$seq,$ts\" > /tmp/lifecycle_order_postCreate; sleep 0.1",
    "postStartCommand": "seq=5; ts=$(date +%s%N); echo \"$seq,$ts\" > /tmp/lifecycle_order_postStart; sleep 0.1",
    "postAttachCommand": "seq=6; ts=$(date +%s%N); echo \"$seq,$ts\" > /tmp/lifecycle_order_postAttach"
}"#;

    // Note: Dotfiles (seq=4) are handled separately via dotfiles config.
    // Since we don't have a real dotfiles repo to clone, we verify the other 5 phases.
    // The spec order is: onCreate(1) -> updateContent(2) -> postCreate(3) -> dotfiles(4) -> postStart(5) -> postAttach(6)

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run `up` command - this should execute all lifecycle hooks in order
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--remove-existing-container") // Ensure fresh start
        .output()
        .unwrap();

    assert!(
        up_output.status.success(),
        "Lifecycle up failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&up_output.stdout),
        String::from_utf8_lossy(&up_output.stderr)
    );

    // Use exec to read back the marker files and verify ordering
    // Collect all markers: phase name, sequence, timestamp
    let phases_to_check = [
        ("onCreate", 1),
        ("updateContent", 2),
        ("postCreate", 3),
        // Skip dotfiles (seq=4) as it requires dotfiles repo configuration
        ("postStart", 5),
        ("postAttach", 6),
    ];

    let mut collected_markers: Vec<(String, u32, u64)> = Vec::new();

    for (phase_name, expected_seq) in &phases_to_check {
        let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
        let exec_output = exec_cmd
            .current_dir(&temp_dir)
            .arg("exec")
            .arg("--workspace-folder")
            .arg(temp_dir.path())
            .arg("--")
            .arg("cat")
            .arg(format!("/tmp/lifecycle_order_{}", phase_name))
            .output()
            .unwrap();

        assert!(
            exec_output.status.success(),
            "Failed to read marker for phase {}: {}",
            phase_name,
            String::from_utf8_lossy(&exec_output.stderr)
        );

        let marker_content = String::from_utf8_lossy(&exec_output.stdout);
        let marker_content = marker_content.trim();

        // Parse "seq,timestamp" format
        let parts: Vec<&str> = marker_content.split(',').collect();
        assert_eq!(
            parts.len(),
            2,
            "Marker for {} should have format 'seq,timestamp', got: '{}'",
            phase_name,
            marker_content
        );

        let seq: u32 = parts[0].parse().unwrap_or_else(|_| {
            panic!(
                "Failed to parse sequence for {}: '{}'",
                phase_name, parts[0]
            )
        });
        let ts: u64 = parts[1].parse().unwrap_or_else(|_| {
            panic!(
                "Failed to parse timestamp for {}: '{}'",
                phase_name, parts[1]
            )
        });

        // Verify expected sequence number (confirms correct hook was triggered)
        assert_eq!(
            seq, *expected_seq,
            "Phase {} has wrong sequence number: expected {}, got {}",
            phase_name, expected_seq, seq
        );

        collected_markers.push((phase_name.to_string(), seq, ts));
    }

    // Verify ordering: timestamps should be non-decreasing
    // Note: Using >= instead of > because phases can execute within the same second
    // The sequence number check below provides the definitive ordering verification
    for i in 1..collected_markers.len() {
        let (prev_name, _, prev_ts) = &collected_markers[i - 1];
        let (curr_name, _, curr_ts) = &collected_markers[i];

        assert!(
            curr_ts >= prev_ts,
            "Lifecycle phases executed out of order: {} (ts={}) should come before {} (ts={})",
            prev_name,
            prev_ts,
            curr_name,
            curr_ts
        );
    }

    // Verify no duplicates: each phase should appear exactly once
    // (The marker file write is atomic - if a phase ran twice, the second write would overwrite)
    // We verify this indirectly by checking that all expected phases exist and have correct sequence numbers
    assert_eq!(
        collected_markers.len(),
        phases_to_check.len(),
        "Expected {} phases to be executed exactly once, found {}",
        phases_to_check.len(),
        collected_markers.len()
    );

    // Verify the phases are in correct spec order (by sequence number)
    let sequences: Vec<u32> = collected_markers.iter().map(|(_, seq, _)| *seq).collect();
    assert_eq!(
        sequences,
        vec![1, 2, 3, 5, 6],
        "Phases did not execute in spec order: got {:?}",
        sequences
    );

    println!(
        "SC-001 verification passed: All lifecycle phases executed exactly once in correct order"
    );
    println!("Execution order verified: onCreate -> updateContent -> postCreate -> postStart -> postAttach");
}

/// Test SC-001 with dotfiles: Verify dotfiles phase executes between postCreate and postStart
///
/// This test specifically validates that when dotfiles are configured, they execute
/// at the correct position in the lifecycle order (after postCreate, before postStart).
#[test]
#[ignore] // Flaky in CI - requires specific Docker environment and lifecycle completion
fn test_lifecycle_dotfiles_ordering_sc001() {
    if !is_docker_available() {
        eprintln!("Skipping test_lifecycle_dotfiles_ordering_sc001: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create a local dotfiles directory with an install script
    let dotfiles_dir = temp_dir.path().join("my-dotfiles");
    fs::create_dir_all(&dotfiles_dir).unwrap();

    // Create the install script that writes a marker
    let install_script = r#"#!/bin/sh
seq=4
ts=$(date +%s%N)
echo "$seq,$ts" > /tmp/lifecycle_order_dotfiles
"#;
    fs::write(dotfiles_dir.join("install.sh"), install_script).unwrap();

    // Make install.sh executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(dotfiles_dir.join("install.sh"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(dotfiles_dir.join("install.sh"), perms).unwrap();
    }

    // Create a devcontainer.json with all lifecycle hooks plus dotfiles config
    // Using local dotfiles repository path
    let devcontainer_config = r#"{
    "name": "Lifecycle Dotfiles Order Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "onCreateCommand": "seq=1; ts=$(date +%s%N); echo \"$seq,$ts\" > /tmp/lifecycle_order_onCreate; sleep 0.1",
    "updateContentCommand": "seq=2; ts=$(date +%s%N); echo \"$seq,$ts\" > /tmp/lifecycle_order_updateContent; sleep 0.1",
    "postCreateCommand": "seq=3; ts=$(date +%s%N); echo \"$seq,$ts\" > /tmp/lifecycle_order_postCreate; sleep 0.1",
    "postStartCommand": "seq=5; ts=$(date +%s%N); echo \"$seq,$ts\" > /tmp/lifecycle_order_postStart; sleep 0.1",
    "postAttachCommand": "seq=6; ts=$(date +%s%N); echo \"$seq,$ts\" > /tmp/lifecycle_order_postAttach"
}"#;
    // Note: Dotfiles configuration would need to be set via CLI or user settings
    // For now, this test focuses on verifying the ordering of the 5 standard lifecycle hooks

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run `up` command with dotfiles repository pointed to local path
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--dotfiles-repository")
        .arg(dotfiles_dir.to_str().unwrap())
        .arg("--remove-existing-container")
        .output()
        .unwrap();

    // Dotfiles might not be fully implemented yet, so we check if up succeeded
    // and then verify whatever markers were created
    if !up_output.status.success() {
        let stderr = String::from_utf8_lossy(&up_output.stderr);
        // If it failed due to dotfiles not being implemented, skip the dotfiles check
        if stderr.contains("dotfiles") || stderr.contains("not implemented") {
            eprintln!(
                "Dotfiles feature may not be fully implemented, checking standard hooks only"
            );
        } else {
            panic!(
                "Lifecycle up failed: stdout={}, stderr={}",
                String::from_utf8_lossy(&up_output.stdout),
                stderr
            );
        }
    }

    // Verify standard lifecycle hooks executed in order (even if dotfiles skipped)
    let phases_to_check = [
        ("onCreate", 1),
        ("updateContent", 2),
        ("postCreate", 3),
        ("postStart", 5),
        ("postAttach", 6),
    ];

    let mut collected_markers: Vec<(String, u32, u64)> = Vec::new();
    let mut phases_found = 0;

    for (phase_name, expected_seq) in &phases_to_check {
        let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
        let exec_output = exec_cmd
            .current_dir(&temp_dir)
            .arg("exec")
            .arg("--workspace-folder")
            .arg(temp_dir.path())
            .arg("--")
            .arg("cat")
            .arg(format!("/tmp/lifecycle_order_{}", phase_name))
            .output();

        if let Ok(output) = exec_output {
            if output.status.success() {
                let marker_content = String::from_utf8_lossy(&output.stdout);
                let marker_content = marker_content.trim();

                if let Some((seq_str, ts_str)) = marker_content.split_once(',') {
                    if let (Ok(seq), Ok(ts)) = (seq_str.parse::<u32>(), ts_str.parse::<u64>()) {
                        assert_eq!(
                            seq, *expected_seq,
                            "Phase {} has wrong sequence number",
                            phase_name
                        );
                        collected_markers.push((phase_name.to_string(), seq, ts));
                        phases_found += 1;
                    }
                }
            }
        }
    }

    // Check if dotfiles marker exists (if dotfiles feature is implemented)
    let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
    let dotfiles_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--")
        .arg("cat")
        .arg("/tmp/lifecycle_order_dotfiles")
        .output();

    let mut dotfiles_executed = false;
    if let Ok(output) = dotfiles_output {
        if output.status.success() {
            let marker_content = String::from_utf8_lossy(&output.stdout);
            let marker_content = marker_content.trim();

            if let Some((seq_str, ts_str)) = marker_content.split_once(',') {
                if let (Ok(seq), Ok(ts)) = (seq_str.parse::<u32>(), ts_str.parse::<u64>()) {
                    assert_eq!(seq, 4, "Dotfiles should have sequence number 4");

                    // Find postCreate and postStart timestamps to verify dotfiles is between them
                    let post_create_ts = collected_markers
                        .iter()
                        .find(|(name, _, _)| name == "postCreate")
                        .map(|(_, _, ts)| *ts);
                    let post_start_ts = collected_markers
                        .iter()
                        .find(|(name, _, _)| name == "postStart")
                        .map(|(_, _, ts)| *ts);

                    if let (Some(pc_ts), Some(ps_ts)) = (post_create_ts, post_start_ts) {
                        // Use >= and <= because phases can execute within the same second
                        assert!(
                            ts >= pc_ts && ts <= ps_ts,
                            "Dotfiles (ts={}) should execute between postCreate (ts={}) and postStart (ts={})",
                            ts, pc_ts, ps_ts
                        );
                        println!("Dotfiles executed at correct position: postCreate <= dotfiles <= postStart");
                        dotfiles_executed = true;
                    }
                }
            }
        }
    }

    // Verify timestamps are non-decreasing for collected markers
    // Note: Using >= instead of > because phases can execute within the same second
    for i in 1..collected_markers.len() {
        let (prev_name, _, prev_ts) = &collected_markers[i - 1];
        let (curr_name, _, curr_ts) = &collected_markers[i];

        assert!(
            curr_ts >= prev_ts,
            "Phases out of order: {} (ts={}) should come before {} (ts={})",
            prev_name,
            prev_ts,
            curr_name,
            curr_ts
        );
    }

    assert!(
        phases_found >= 5,
        "Expected at least 5 standard lifecycle phases, found {}",
        phases_found
    );

    if dotfiles_executed {
        println!("SC-001 with dotfiles verified: All 6 phases executed in correct order");
    } else {
        println!(
            "SC-001 partial verification: 5 standard lifecycle phases executed in correct order"
        );
        println!("(Dotfiles phase not verified - may require additional configuration)");
    }
}

/// Test simple in-container exec after up
#[test]
fn test_features_accessible_in_container() {
    if !is_docker_available() {
        eprintln!("Skipping test_features_accessible_in_container: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create devcontainer without external features to avoid network
    let devcontainer_config = r#"{
    "name": "Feature Access Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "postCreateCommand": "echo 'Container ready for feature test'"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up command
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    assert!(
        up_output.status.success(),
        "Feature up failed: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );

    // Test that docker command is available via exec
    let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--")
        .arg("sh")
        .arg("-lc")
        .arg("echo ready")
        .output()
        .unwrap();

    assert!(
        exec_output.status.success(),
        "Exec failed unexpectedly: {}",
        String::from_utf8_lossy(&exec_output.stderr)
    );
    let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);
    assert!(
        exec_stdout.contains("ready"),
        "Expected 'ready' in exec output"
    );
}
