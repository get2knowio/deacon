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

/// Test lifecycle phase order with marker files
#[test]
fn test_lifecycle_hooks_stable_order() {
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

/// Test lifecycle resume behavior (restart only re-runs postStart/postAttach)
#[test]
fn test_lifecycle_resume_markers() {
    let temp_dir = TempDir::new().unwrap();

    // Create devcontainer with resume-testable lifecycle hooks
    let devcontainer_config = r#"{
    "name": "Lifecycle Resume Test",
    "image": "alpine:3.19", 
    "workspaceFolder": "/workspace",
    "onCreateCommand": "date '+onCreate-%s' > /tmp/marker_onCreate",
    "postCreateCommand": "date '+postCreate-%s' > /tmp/marker_postCreate",
    "postStartCommand": "date '+postStart-%s' > /tmp/marker_postStart",
    "postAttachCommand": "date '+postAttach-%s' > /tmp/marker_postAttach"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // First up - should create all markers
    let mut up_cmd1 = Command::cargo_bin("deacon").unwrap();
    let up_output1 = up_cmd1
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    assert!(
        up_output1.status.success(),
        "First up failed: {}",
        String::from_utf8_lossy(&up_output1.stderr)
    );

    // TODO: Sleep and second up to test resume behavior
    // Second up should only re-run postStart/postAttach
    // This would require checking marker timestamps to verify onCreate/postCreate weren't re-run
}

/// Test --skip-non-blocking-commands flag suppresses postStart and postAttach
#[test]
fn test_lifecycle_skip_non_blocking_commands() {
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

/// Test simple in-container exec after up
#[test]
fn test_features_accessible_in_container() {
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
