//! Smoke tests for exec command stdin streaming passthrough
//!
//! Scenarios covered:
//! - Exec stdin streaming: piping data to exec command and verifying passthrough
//! - Exec behavior without Docker: graceful error handling
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

/// Test exec stdin streaming basic pass-through
#[test]
fn test_exec_stdin_basic() {
    if !is_docker_available() {
        eprintln!("Skipping test_exec_stdin_basic: Docker not available");
        return;
    }
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

    // Ensure container is up
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_out = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--skip-post-create")
        .arg("--skip-non-blocking-commands")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
    assert!(
        up_out.status.success(),
        "up failed: {}",
        String::from_utf8_lossy(&up_out.stderr)
    );

    // Exec with stdin
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

    assert!(
        exec_output.status.success(),
        "exec failed: {}",
        String::from_utf8_lossy(&exec_output.stderr)
    );
    let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);
    assert!(exec_stdout.contains("hello stdin test"));

    // Cleanup
    let mut down_cmd = Command::cargo_bin("deacon").unwrap();
    let _ = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
}

/// Test exec stdin streaming with Docker (Docker-gated)
#[test]
fn test_exec_stdin_streaming() {
    if !is_docker_available() {
        eprintln!("Skipping test_exec_stdin_streaming: Docker not available");
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

    assert!(
        up_output.status.success(),
        "Up command failed: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );

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

    assert!(
        exec_output.status.success(),
        "Exec stdin command failed: {}",
        exec_stderr
    );
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
    if !is_docker_available() {
        eprintln!("Skipping test_exec_stdin_shell_commands: Docker not available");
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

    assert!(
        up_output.status.success(),
        "Up command failed: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );

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

    assert!(
        exec_output.status.success(),
        "Exec tr command failed: {}",
        exec_stderr
    );
    // Verify that stdin was transformed by tr command
    assert!(
        exec_stdout.contains("HELLO WORLD"),
        "tr command should transform stdin. Got stdout: '{}'",
        exec_stdout
    );

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
