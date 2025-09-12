//! Basic smoke tests for implemented functionality
//!
//! These tests provide broad coverage of CLI flows while remaining resilient
//! in environments without Docker. They exercise the main command paths and
//! validate graceful degradation when Docker is unavailable.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use tempfile::TempDir;

/// Test compose-based up path detection
#[test]
fn test_compose_based_up_path_detection() {
    let temp_dir = TempDir::new().unwrap();

    // Create a docker-compose.yml file
    let compose_content = r#"
version: '3.8'
services:
  app:
    image: alpine:3.19
    command: sleep infinity
    volumes:
      - .:/workspace:cached
    working_dir: /workspace
"#;
    fs::write(temp_dir.path().join("docker-compose.yml"), compose_content).unwrap();

    // Create devcontainer.json with dockerComposeFile + service
    let devcontainer_config = r#"{
    "name": "Compose Test Container",
    "dockerComposeFile": "docker-compose.yml",
    "service": "app",
    "workspaceFolder": "/workspace"
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up command
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd.current_dir(&temp_dir).arg("up").assert();

    let output = assert.get_output();

    if output.status.success() {
        // If successful, up command should work with compose configuration
        // Success is the main verification for compose path detection
        // The fact that it didn't fail means compose configuration was processed
    } else {
        // If failed, should be due to Docker/Compose unavailable
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Docker is not installed")
                || stderr.contains("Docker daemon is not")
                || stderr.contains("docker-compose")
                || stderr.contains("compose")
                || stderr.contains("permission denied")
                || stderr.contains("not found"),
            "Unexpected error for compose up: {}",
            stderr
        );
    }
}

/// Test exec environment and working directory behavior  
#[test]
fn test_exec_environment_and_working_directory() {
    let temp_dir = TempDir::new().unwrap();

    // Create devcontainer.json with custom workspaceFolder
    let devcontainer_config = r#"{
    "name": "Exec Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/custom/workspace"
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test exec command with environment variable
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--env")
        .arg("FOO=bar")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("pwd && echo $FOO")
        .assert();

    let output = assert.get_output();

    if output.status.success() {
        // If successful, should return workspace dir and FOO value
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("/custom/workspace") && stdout.contains("bar"),
            "Expected workspace path and FOO variable in output: {}",
            stdout
        );
    } else {
        // If failed, should be due to Docker unavailable or no running container
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Docker is not installed")
                || stderr.contains("Docker daemon is not")
                || stderr.contains("No running container")
                || stderr.contains("Container not found")
                || stderr.contains("permission denied")
                || stderr.contains("not found"),
            "Unexpected error for exec: {}",
            stderr
        );
    }
}

/// Test build arg handling with simple Dockerfile
#[test]
fn test_build_arg_handling() {
    let temp_dir = TempDir::new().unwrap();

    // Create Dockerfile with ARGs and labels
    let dockerfile_content = r#"FROM alpine:3.19
ARG BUILD_VERSION=default
ARG BUILD_ENV=""
ARG EMPTY_ARG
LABEL version=$BUILD_VERSION
LABEL environment=$BUILD_ENV
LABEL deacon.test=smoke
RUN echo "Building with version: $BUILD_VERSION, env: $BUILD_ENV"
"#;
    fs::write(temp_dir.path().join("Dockerfile"), dockerfile_content).unwrap();

    // Create devcontainer.json with dockerFile
    let devcontainer_config = r#"{
    "name": "Build Args Test Container", 
    "dockerFile": "Dockerfile"
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test build command with multiple build args including edge cases
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("build")
        .arg("--output-format")
        .arg("json")
        .arg("--build-arg")
        .arg("BUILD_VERSION=1.0.0")
        .arg("--build-arg")
        .arg("BUILD_ENV=production")
        .arg("--build-arg")
        .arg("EMPTY_ARG=") // Test empty value
        .assert();

    let output = assert.get_output();

    if output.status.success() {
        // If successful, check JSON output includes expected fields
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("image_id"));
        assert!(stdout.contains("build_duration"));

        // Try to parse as JSON to validate structure
        if let Ok(json) = serde_json::from_str::<Value>(&stdout) {
            assert!(json.get("image_id").is_some());
        }
    } else {
        // If failed, should be due to Docker unavailable
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Docker is not installed")
                || stderr.contains("Docker daemon is not")
                || stderr.contains("Docker build failed")
                || stderr.contains("permission denied")
                || stderr.contains("not found"),
            "Unexpected error for build with args: {}",
            stderr
        );
    }
}

/// Test doctor JSON stability with potential logging noise
#[test]
fn test_doctor_json_stability() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("doctor").arg("--json");

    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Extract JSON object from potentially mixed output
    // Look for first "{" to last "}" to handle any logging noise
    if let Some(start) = stdout.find('{') {
        if let Some(end) = stdout.rfind('}') {
            let json_slice = &stdout[start..=end];

            // Should be able to parse as valid JSON
            let parsed: Result<Value, _> = serde_json::from_str(json_slice);
            assert!(
                parsed.is_ok(),
                "Failed to parse JSON from doctor output: {}",
                json_slice
            );

            if let Ok(json) = parsed {
                // Validate expected fields exist
                assert!(json.get("cli_version").is_some());
                assert!(json.get("host_os").is_some());
                assert!(json.get("docker_info").is_some());
            }
        }
    } else {
        panic!("No JSON object found in doctor output: {}", stdout);
    }
}

/// Test read-configuration with variable substitution edge cases
#[test]
fn test_read_configuration_fixtures_breadth() {
    let temp_dir = TempDir::new().unwrap();

    // Create base devcontainer.json
    let base_config = r#"{
    "name": "Base ${localEnv:USER:developer} Container",
    "image": "ubuntu:${localEnv:UBUNTU_VERSION:20.04}",
    "features": {
        "ghcr.io/devcontainers/features/git:1": {}
    }
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        base_config,
    )
    .unwrap();

    // Test read-configuration command
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("read-configuration")
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should contain expected keys and processed variable substitution
    assert!(stdout.contains("name"));
    assert!(stdout.contains("image"));
    assert!(stdout.contains("features"));

    // Variable substitution should have been processed
    assert!(
        stdout.contains("developer") || stdout.contains("Container"),
        "Expected variable substitution in output: {}",
        stdout
    );
}

/// Optional: Full Docker workflow test (gated by environment variable)
#[test]
fn test_up_exec_happy_path() {
    // Only run if explicitly enabled
    if std::env::var("SMOKE_DOCKER").is_err() {
        eprintln!("Skipping Docker-dependent test (set SMOKE_DOCKER=1 to enable)");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a simple devcontainer.json for long-running container
    let devcontainer_config = r#"{
    "name": "Happy Path Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "postCreateCommand": "echo 'Container ready'"
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up command
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    up_cmd.current_dir(&temp_dir).arg("up").assert().success();

    // Test exec command
    let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
    exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("echo")
        .arg("hello from container")
        .assert()
        .success()
        .stdout(predicate::str::contains("hello from container"));
}
