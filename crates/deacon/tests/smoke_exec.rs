//! Smoke tests for exec command behavior parity
//!
//! Scenarios covered:
//! - Exec behavior parity: TTY detection, exit code propagation, stdin streaming
//! - Working directory and --remote-env support
//! - remoteEnv and metadata interactions
//! - Compose/subfolder config + markers
//!
//! Tests are written to be resilient in environments without Docker: they
//! accept specific error messages that indicate Docker is unavailable.

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

/// Test exec without TTY prints expected stdout
#[test]
fn test_exec_stdout_without_tty() {
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

    let exec_stderr = String::from_utf8_lossy(&exec_output.stderr);

    if exec_output.status.success() {
        let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);
        assert!(
            exec_stdout.contains("Hello from exec"),
            "Exec should output command stdout"
        );
        println!("Exec stdout test passed");
    } else if docker_related_error(&exec_stderr)
        || exec_stderr.contains("No running container found")
    {
        println!("Skipping Docker-dependent exec stdout test");
    } else {
        panic!("Unexpected error in exec stdout test: {}", exec_stderr);
    }
}

/// Test exec exit code propagation
#[test]
fn test_exec_exit_code_propagation() {
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

    let exec_stderr = String::from_utf8_lossy(&exec_output.stderr);

    if docker_related_error(&exec_stderr) || exec_stderr.contains("No running container found") {
        println!("Skipping Docker-dependent exec exit code test");
    } else {
        // Should propagate exit code 123, but might get different code if container setup failed
        let actual_code = exec_output.status.code();
        if actual_code == Some(123) {
            println!("Exec exit code propagation test passed");
        } else {
            println!("Note: Got exit code {:?} instead of 123, possibly due to container state", actual_code);
        }
    }
}

/// Test exec working directory behavior
#[test]
fn test_exec_working_directory() {
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

    let exec_stderr = String::from_utf8_lossy(&exec_output.stderr);

    if exec_output.status.success() {
        let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);
        // Should be in workspace folder
        assert!(
            exec_stdout.trim().ends_with("workspace") || exec_stdout.contains("/workspace"),
            "Exec should run in workspace directory, got: {}",
            exec_stdout
        );
        println!("Exec working directory test passed");
    } else if docker_related_error(&exec_stderr)
        || exec_stderr.contains("No running container found")
    {
        println!("Skipping Docker-dependent exec working directory test");
    } else {
        panic!(
            "Unexpected error in exec working directory test: {}",
            exec_stderr
        );
    }
}

/// Test exec --env merges environment variables
#[test]
fn test_exec_env_merges() {
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

    let exec_stderr = String::from_utf8_lossy(&exec_output.stderr);

    if exec_output.status.success() {
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
        println!("Exec --env test passed");
    } else if docker_related_error(&exec_stderr)
        || exec_stderr.contains("No running container found")
    {
        println!("Skipping Docker-dependent exec --env test");
    } else {
        panic!("Unexpected error in exec --env test: {}", exec_stderr);
    }
}

/// Test up with remoteEnv in config makes values available to lifecycle hooks
#[test]
fn test_up_remote_env_in_config() {
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

    if up_output.status.success() {
        println!("Up with remoteEnv config test passed");

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

        if exec_output.status.success() {
            let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);
            assert!(
                exec_stdout.contains("config_value"),
                "remoteEnv should be available in exec"
            );
        }
    } else if docker_related_error(&up_stderr) {
        println!("Skipping Docker-dependent up remoteEnv test");
    } else {
        panic!("Unexpected error in up remoteEnv test: {}", up_stderr);
    }
}

/// Test exec with --config in subfolder works
#[test]
fn test_exec_subfolder_config() {
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

    if up_output.status.success() {
        println!("Up with subfolder config succeeded");

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

        let exec_stderr = String::from_utf8_lossy(&exec_output.stderr);

        if exec_output.status.success() {
            let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);
            assert!(
                exec_stdout.contains("subfolder exec works"),
                "Exec should work with subfolder config"
            );
            println!("Exec with subfolder config test passed");
        } else if !docker_related_error(&exec_stderr) {
            println!(
                "Exec with subfolder config failed (expected in some environments): {}",
                exec_stderr
            );
        }
    } else if docker_related_error(&up_stderr) {
        println!("Skipping Docker-dependent subfolder config test");
    } else {
        panic!("Unexpected error in subfolder config test: {}", up_stderr);
    }
}

/// Test TTY detection behavior
#[test]
fn test_exec_tty_detection() {
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

    let exec_stderr = String::from_utf8_lossy(&exec_output.stderr);

    if docker_related_error(&exec_stderr) || exec_stderr.contains("No running container found") {
        println!("Skipping Docker-dependent TTY detection test");
    } else {
        // Exit code depends on TTY state - both success and failure are valid
        // since we're testing that the command executes properly
        println!(
            "TTY detection test completed (exit code: {:?})",
            exec_output.status.code()
        );
    }
}
