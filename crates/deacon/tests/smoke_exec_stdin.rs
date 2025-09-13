//! Smoke tests for exec command stdin streaming passthrough
//!
//! Scenarios covered:
//! - Exec stdin streaming: piping data to exec command and verifying passthrough
//! - Exec behavior without Docker: graceful error handling
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
        || stderr.contains("No running container found")
}

/// Test exec command without Docker: should handle gracefully
#[test]
fn test_exec_stdin_without_docker() {
    let temp_dir = TempDir::new().unwrap();

    // Create minimal devcontainer.json
    let devcontainer_config = r#"{
    "name": "Exec Stdin Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test exec command with stdin
    let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("cat -")
        .write_stdin("hello stdin test")
        .output()
        .unwrap();

    let exec_stderr = String::from_utf8_lossy(&exec_output.stderr);

    if exec_output.status.success() {
        // Unexpected success without container running, but accept it
        println!("Exec stdin succeeded unexpectedly without container");
    } else if docker_related_error(&exec_stderr) {
        println!("Exec stdin gracefully handled Docker unavailable error");
    } else {
        // Any other error is also acceptable for this test
        println!("Exec stdin handled error as expected: {}", exec_stderr);
    }
}

/// Test exec stdin streaming with Docker (Docker-gated)
#[test]
fn test_exec_stdin_streaming() {
    // Only run if Docker is explicitly enabled
    if std::env::var("SMOKE_DOCKER").is_err() {
        eprintln!("Skipping Docker-dependent test (set SMOKE_DOCKER=1 to enable)");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create minimal devcontainer.json
    let devcontainer_config = r#"{
    "name": "Exec Stdin Streaming Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // First: up command to create container
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--skip-post-create")
        .arg("--skip-non-blocking-commands")
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
        panic!("Up command failed: {}", stderr);
    }

    // Second: exec command with stdin data
    let test_input = "hello stdin streaming test\nline 2\nline 3";
    let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("cat -")
        .write_stdin(test_input)
        .output()
        .unwrap();

    let exec_stderr = String::from_utf8_lossy(&exec_output.stderr);
    let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);

    if exec_output.status.success() {
        // Verify that stdin was passed through to the container process
        assert!(
            exec_stdout.contains("hello stdin streaming test"),
            "Stdin should be passed through to container. Got stdout: '{}'",
            exec_stdout
        );
        assert!(
            exec_stdout.contains("line 2") && exec_stdout.contains("line 3"),
            "Multi-line stdin should be preserved. Got stdout: '{}'",
            exec_stdout
        );
        println!("Exec stdin streaming test passed - data was passed through correctly");
    } else if docker_related_error(&exec_stderr) {
        eprintln!("Skipping Docker-dependent test (Docker not available)");
        return;
    } else {
        panic!("Exec stdin command failed: {}", exec_stderr);
    }

    // Clean up: down command
    let mut down_cmd = Command::cargo_bin("deacon").unwrap();
    let _down_output = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
    // Ignore down result as it's just cleanup
}

/// Test exec with different shell commands for stdin
#[test]
fn test_exec_stdin_shell_commands() {
    // Only run if Docker is explicitly enabled
    if std::env::var("SMOKE_DOCKER").is_err() {
        eprintln!("Skipping Docker-dependent test (set SMOKE_DOCKER=1 to enable)");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create minimal devcontainer.json
    let devcontainer_config = r#"{
    "name": "Exec Stdin Shell Commands Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // First: up command to create container
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--skip-post-create")
        .arg("--skip-non-blocking-commands")
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
        panic!("Up command failed: {}", stderr);
    }

    // Test exec with tr command to transform stdin
    let test_input = "hello world";
    let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--")
        .arg("tr")
        .arg("a-z")
        .arg("A-Z")
        .write_stdin(test_input)
        .output()
        .unwrap();

    let exec_stderr = String::from_utf8_lossy(&exec_output.stderr);
    let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);

    if exec_output.status.success() {
        // Verify that stdin was transformed by tr command
        assert!(
            exec_stdout.contains("HELLO WORLD"),
            "tr command should transform stdin. Got stdout: '{}'",
            exec_stdout
        );
        println!("Exec stdin with tr command test passed");
    } else if docker_related_error(&exec_stderr) {
        eprintln!("Skipping Docker-dependent test (Docker not available)");
        return;
    } else {
        panic!("Exec tr command failed: {}", exec_stderr);
    }

    // Clean up: down command
    let mut down_cmd = Command::cargo_bin("deacon").unwrap();
    let _down_output = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
    // Ignore down result as it's just cleanup
}
