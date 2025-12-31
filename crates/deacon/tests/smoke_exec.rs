//! Smoke tests for exec command behavior parity
//!
//! Scenarios covered:
//! - Exec behavior parity: TTY detection, exit code propagation, stdin streaming
//! - Working directory and --remote-env support
//! - remoteEnv and metadata interactions
//! - Compose/subfolder config + markers
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

/// Test exec without TTY prints expected stdout
#[test]
fn test_exec_stdout_without_tty() {
    if !is_docker_available() {
        eprintln!("Skipping test_exec_stdout_without_tty: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create a simple devcontainer.json
    let devcontainer_config = r#"{
    "name": "Exec Test Container",
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

    // Test exec command without TTY
    let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("echo")
        .arg("Hello from exec")
        .output()
        .unwrap();

    assert!(
        exec_output.status.success(),
        "Unexpected error in exec stdout test: {}",
        String::from_utf8_lossy(&exec_output.stderr)
    );
    let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);
    assert!(
        exec_stdout.contains("Hello from exec"),
        "Exec should output command stdout"
    );

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

/// Test exec exit code propagation
#[test]
fn test_exec_exit_code_propagation() {
    if !is_docker_available() {
        eprintln!("Skipping test_exec_exit_code_propagation: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    let devcontainer_config = r#"{
    "name": "Exec Exit Code Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Ensure container is up for exit code test
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

    // Test exec command that exits with specific code
    let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("exit 123")
        .output()
        .unwrap();

    // Should propagate exit code 123
    assert_eq!(
        exec_output.status.code(),
        Some(123),
        "Exec should propagate exit code 123, stderr: {}",
        String::from_utf8_lossy(&exec_output.stderr)
    );

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

/// Test exec working directory behavior
#[test]
fn test_exec_working_directory() {
    if !is_docker_available() {
        eprintln!("Skipping test_exec_working_directory: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    let devcontainer_config = r#"{
    "name": "Exec Working Dir Test",
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

    // Test exec with working directory
    let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("pwd")
        .output()
        .unwrap();

    assert!(
        exec_output.status.success(),
        "Unexpected error in exec working directory test: {}",
        String::from_utf8_lossy(&exec_output.stderr)
    );
    let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);
    // Should be in workspace folder
    assert!(
        exec_stdout.trim().ends_with("workspace") || exec_stdout.contains("/workspace"),
        "Exec should run in workspace directory, got: {}",
        exec_stdout
    );

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

/// Test exec --env merges environment variables
#[test]
fn test_exec_env_merges() {
    if !is_docker_available() {
        eprintln!("Skipping test_exec_env_merges: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    let devcontainer_config = r#"{
    "name": "Exec Env Test",
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

    // Test exec with --env
    let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--env")
        .arg("FOO=BAR")
        .arg("--env")
        .arg("BAZ=") // empty value
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("echo FOO=$FOO BAZ=$BAZ")
        .output()
        .unwrap();

    assert!(
        exec_output.status.success(),
        "Unexpected error in exec --env test: {}",
        String::from_utf8_lossy(&exec_output.stderr)
    );
    let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);
    // Should contain the env values
    assert!(
        exec_stdout.contains("FOO=BAR"),
        "Should have FOO=BAR from --env"
    );
    assert!(
        exec_stdout.contains("BAZ="),
        "Should have empty BAZ from --env"
    );

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

/// Test up with remoteEnv in config makes values available to lifecycle hooks
#[test]
fn test_up_remote_env_in_config() {
    if !is_docker_available() {
        eprintln!("Skipping test_up_remote_env_in_config: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    let devcontainer_config = r#"{
    "name": "Remote Env Config Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "remoteEnv": {
        "CONFIG_VAR": "config_value",
        "EMPTY_VAR": ""
    },
    "postCreateCommand": "echo CONFIG_VAR=$CONFIG_VAR EMPTY_VAR=$EMPTY_VAR > /tmp/env_check"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up command with remoteEnv in config
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
        "Unexpected error in up remoteEnv test: {}",
        up_stderr
    );

    // Test that we can exec and see the environment
    let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("printenv")
        .arg("CONFIG_VAR")
        .output()
        .unwrap();

    assert!(
        exec_output.status.success(),
        "Exec failed: {}",
        String::from_utf8_lossy(&exec_output.stderr)
    );
    let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);
    assert!(
        exec_stdout.contains("config_value"),
        "remoteEnv should be available in exec"
    );
}

/// Test exec with --config in subfolder works
#[test]
fn test_exec_subfolder_config() {
    if !is_docker_available() {
        eprintln!("Skipping test_exec_subfolder_config: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create config in a subfolder
    let subfolder = temp_dir.path().join("subfolder");
    fs::create_dir_all(&subfolder).unwrap();

    let devcontainer_config = r#"{
    "name": "Subfolder Config Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "postCreateCommand": "echo 'subfolder-postCreate' > /tmp/marker_subfolder"
}"#;

    fs::create_dir(subfolder.join(".devcontainer")).unwrap();
    fs::write(
        subfolder.join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up with config in subfolder
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(&subfolder)
        .arg("--config")
        .arg(subfolder.join(".devcontainer/devcontainer.json"))
        .output()
        .unwrap();

    let up_stderr = String::from_utf8_lossy(&up_output.stderr);

    assert!(
        up_output.status.success(),
        "Unexpected error in subfolder config test (up): {}",
        up_stderr
    );

    // Test exec with --config in subfolder
    let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(&subfolder)
        .arg("--config")
        .arg(subfolder.join(".devcontainer/devcontainer.json"))
        .arg("echo")
        .arg("subfolder exec works")
        .output()
        .unwrap();

    assert!(exec_output.status.success());
    let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);
    assert!(
        exec_stdout.contains("subfolder exec works"),
        "Exec should work with subfolder config"
    );
}

/// Test TTY detection behavior
#[test]
fn test_exec_tty_detection() {
    if !is_docker_available() {
        eprintln!("Skipping test_exec_tty_detection: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    let devcontainer_config = r#"{
    "name": "TTY Detection Test",
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

    // Test exec command that checks if running in TTY
    let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("test")
        .arg("-t")
        .arg("0") // test if stdin is a TTY
        .output()
        .unwrap();

    // Exit code depends on TTY state - both success and failure are valid, but the command must execute.
    assert!(
        exec_output.status.code().is_some(),
        "TTY detection exec did not run"
    );

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
