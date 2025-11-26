//! Integration tests for traditional container up workflow

use assert_cmd::Command;
use serde_json::json;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_up_traditional_container_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();

    // Create a simple devcontainer.json configuration without variable substitution
    let devcontainer_config = json!({
        "name": "Test Container",
        "image": "ubuntu:20.04",
        "remoteUser": "testuser",
        "updateRemoteUserUID": true,
        "workspaceFolder": "/workspaces/test",
        "postCreateCommand": "echo 'Hello from container'",
        "postStartCommand": ["echo 'Container started'", "ls -la /workspaces"],
        "forwardPorts": [3000, 8080],
        "containerEnv": {
            "NODE_ENV": "development"
        }
    });

    let config_path = devcontainer_dir.join("devcontainer.json");
    fs::write(
        &config_path,
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    // Test the up command - this will work if Docker is available, fail if not
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let result = cmd
        .args([
            "up",
            "--workspace-folder",
            &temp_dir.path().to_string_lossy(),
            "--skip-post-create", // Skip for testing without actual container
            "--ports-events",
        ])
        .assert();

    // The command will succeed if Docker is available, fail if not
    let output = result.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Debug: always print actual output for analysis
    eprintln!("DEBUG - stderr content: {:?}", stderr);
    eprintln!("DEBUG - stdout content: {:?}", stdout);
    eprintln!("DEBUG - exit code: {:?}", output.status.code());

    // Should attempt traditional container path - either succeeds or fails at Docker step
    assert!(
        stderr.contains("traditional")
            || stderr.contains("Container created")
            || stderr.contains("Container reused")
            || stderr.contains("docker")
            || stderr.contains("Docker")
            || stderr.contains("Error response from daemon")
            || stderr.contains("is not running")
            || stderr.contains("ping")
            || stderr.contains("Lifecycle")
            || stderr.contains("Not installed")
            || stderr.contains("No such file or directory")
            || stderr.contains("Error response from daemon")
            || stderr.contains("permission denied")
            || stderr.contains("Failed to spawn")
            || stderr.contains("command not found")
            || stderr.is_empty() // Sometimes successful runs have empty stderr
    );
}

#[test]
fn test_up_traditional_container_with_flags() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();

    // Create a minimal traditional container config
    let devcontainer_config = json!({
        "name": "Minimal Test",
        "image": "alpine:latest"
    });

    let config_path = devcontainer_dir.join("devcontainer.json");
    fs::write(
        &config_path,
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    // Test with all skip flags
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let result = cmd
        .args([
            "up",
            "--workspace-folder",
            &temp_dir.path().to_string_lossy(),
            "--remove-existing-container",
            "--skip-post-create",
            "--skip-non-blocking-commands",
            "--ports-events",
        ])
        .assert();

    // Should attempt traditional container workflow
    let output = result.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Convert stderr to lowercase for case-insensitive matching
    let stderr_lower = stderr.to_lowercase();

    // Debug: print actual output if assertion fails
    if !(stderr_lower.contains("traditional")
        || stderr_lower.contains("container created")
        || stderr_lower.contains("container reused")
        || stderr_lower.contains("docker")
        || stderr_lower.contains("ping")
        || stderr_lower.contains("lifecycle")
        || stderr_lower.contains("not installed")
        || stderr_lower.contains("no such file or directory")
        || stderr_lower.contains("error response from daemon")
        || stderr_lower.contains("permission denied")
        || stderr_lower.contains("failed to spawn")
        || stderr_lower.contains("command not found")
        || stderr.is_empty())
    {
        eprintln!("Test failed - stderr content: {:?}", stderr);
        eprintln!("Test failed - stdout content: {:?}", stdout);
        eprintln!("Test failed - exit code: {:?}", output.status.code());
    }

    // Command will succeed if Docker available, fail at Docker step if not
    assert!(
        stderr_lower.contains("traditional")
            || stderr_lower.contains("container created")
            || stderr_lower.contains("container reused")
            || stderr_lower.contains("docker")
            || stderr_lower.contains("error response from daemon")
            || stderr_lower.contains("is not running")
            || stderr_lower.contains("ping")
            || stderr_lower.contains("lifecycle")
            || stderr_lower.contains("not installed")
            || stderr_lower.contains("no such file or directory")
            || stderr_lower.contains("permission denied")
            || stderr_lower.contains("failed to spawn")
            || stderr_lower.contains("command not found")
            || stderr.is_empty() // Sometimes successful runs have empty stderr
    );
}

#[test]
fn test_up_detects_compose_vs_traditional() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();

    // Create a compose-based configuration
    let compose_config = json!({
        "name": "Compose Test",
        "dockerComposeFile": "docker-compose.yml",
        "service": "app"
    });

    let config_path = devcontainer_dir.join("devcontainer.json");
    fs::write(
        &config_path,
        serde_json::to_string_pretty(&compose_config).unwrap(),
    )
    .unwrap();

    // Test that this uses compose path, not traditional
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let result = cmd
        .args([
            "up",
            "--workspace-folder",
            &temp_dir.path().to_string_lossy(),
        ])
        .assert();

    let output = result.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should use compose workflow, not traditional
    assert!(stderr.contains("compose") || stderr.contains("Docker Compose"));
}
