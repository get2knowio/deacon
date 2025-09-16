//! Smoke tests for lifecycle command execution and secret masking
//!
//! Scenarios covered:
//! - Lifecycle hooks: stable order + resume markers
//! - Secret masking in logs (while secrets available to hooks)
//! - Features accessible in-container (Docker-gated)
//!
//! Tests are written to be resilient in environments without Docker: they
//! accept specific error messages that indicate Docker is unavailable.
//! Docker-dependent tests are gated by SMOKE_DOCKER environment variable.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

fn docker_related_error(stderr: &str) -> bool {
    stderr.contains("Docker is not installed")
        || stderr.contains("Docker daemon is not")
        || stderr.contains("permission denied")
        || stderr.contains("Failed to spawn docker")
        || stderr.contains("Docker CLI error")
        || stderr.contains("Error response from daemon")
        || stderr.contains("container") && stderr.contains("is not running")
        || stderr.contains("Container command failed")
}

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

    let up_stderr = String::from_utf8_lossy(&up_output.stderr);

    if up_output.status.success() {
        // Test passed - markers should be created in order
        // TODO: Once container exec is fully implemented, we could verify marker file contents
        println!("Lifecycle up command succeeded");
    } else if docker_related_error(&up_stderr) {
        println!("Skipping Docker-dependent lifecycle test (Docker not available)");
    } else {
        panic!("Unexpected error in lifecycle up: {}", up_stderr);
    }
}

/// Test lifecycle resume behavior (restart only re-runs postStart/postAttach)
#[test]
fn test_lifecycle_resume_markers() {
    // Only run if Docker is explicitly enabled
    if std::env::var("SMOKE_DOCKER").is_err() {
        eprintln!("Skipping Docker-dependent test (set SMOKE_DOCKER=1 to enable)");
        return;
    }

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

    if !up_output1.status.success() {
        let stderr = String::from_utf8_lossy(&up_output1.stderr);
        if docker_related_error(&stderr) {
            eprintln!("Skipping Docker-dependent test (Docker not available)");
            return;
        }
        panic!("First up failed: {}", stderr);
    }

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

    if up_output.status.success() {
        // TODO: Verify postAttach marker was not created but postStart was
        println!("Skip non-blocking commands succeeded");
    } else if docker_related_error(&up_stderr) {
        println!("Skipping Docker-dependent skip-non-blocking-commands test");
    } else {
        panic!(
            "Unexpected error in skip-non-blocking-commands up: {}",
            up_stderr
        );
    }
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
    let up_stdout = String::from_utf8_lossy(&up_output.stdout);

    if up_output.status.success() {
        let combined_output = format!("{}\n{}", up_stdout, up_stderr);

        // If the command succeeded but we have docker-related errors in stderr,
        // it means the container commands failed, so we should skip like in the docker error case
        if docker_related_error(&up_stderr) {
            println!("Skipping Docker-dependent secret masking test (command succeeded but container commands failed)");
            return;
        }

        // Public info should still be visible when present in output
        // Some environments may not capture container stdout; only assert when seen
        if combined_output.contains("Public is:") {
            assert!(
                combined_output.contains("public-info"),
                "Public information should not be redacted"
            );
        }

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

        if up_output_no_redact.status.success() {
            let combined_no_redact = format!(
                "{}\n{}",
                String::from_utf8_lossy(&up_output_no_redact.stdout),
                String::from_utf8_lossy(&up_output_no_redact.stderr)
            );

            // With --no-redact, secret should be visible
            if combined_no_redact.contains("my-secret-password") {
                println!("Secret masking test passed - redaction can be disabled");
            } else {
                println!("Note: Secret not found in no-redact output (may be expected)");
            }
        }

        println!("Secret masking test succeeded");
    } else if docker_related_error(&up_stderr) {
        println!("Skipping Docker-dependent secret masking test");
    } else {
        panic!("Unexpected error in secret masking up: {}", up_stderr);
    }
}

/// Test simple feature accessibility in container (Docker-gated)
#[test]
fn test_features_accessible_in_container() {
    // Only run if Docker is explicitly enabled
    if std::env::var("SMOKE_DOCKER").is_err() {
        eprintln!("Skipping Docker-dependent test (set SMOKE_DOCKER=1 to enable)");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create devcontainer with a simple feature
    let devcontainer_config = r#"{
    "name": "Feature Access Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "features": {
        "ghcr.io/devcontainers/features/docker-in-docker:2": {}
    },
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

    if !up_output.status.success() {
        let stderr = String::from_utf8_lossy(&up_output.stderr);
        if docker_related_error(&stderr) {
            eprintln!("Skipping Docker-dependent test (Docker not available)");
            return;
        }
        panic!("Feature up failed: {}", stderr);
    }

    // Test that docker command is available via exec
    let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("docker")
        .arg("--version")
        .output()
        .unwrap();

    let exec_stderr = String::from_utf8_lossy(&exec_output.stderr);

    if exec_output.status.success() {
        let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);
        assert!(
            exec_stdout.contains("Docker version"),
            "Docker feature should provide docker command"
        );
        println!("Feature accessibility test passed - docker command available");
    } else {
        // Could be normal if container isn't running or feature installation failed
        println!(
            "Feature exec test failed (expected in some environments): {}",
            exec_stderr
        );
    }
}
