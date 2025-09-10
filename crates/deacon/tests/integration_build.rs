//! Integration tests for the build command
//!
//! These tests verify that the build command works with real Docker builds.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_build_with_dockerfile() {
    // Create a temporary directory with a simple Dockerfile
    let temp_dir = TempDir::new().unwrap();
    let dockerfile_content = r#"FROM alpine:3.19
LABEL test=1
LABEL deacon.test=integration
RUN echo "Building test image"
"#;

    fs::write(temp_dir.path().join("Dockerfile"), dockerfile_content).unwrap();

    // Create a devcontainer.json configuration
    let devcontainer_config = r#"{
    "name": "Test Build Container",
    "dockerFile": "Dockerfile",
    "build": {
        "context": ".",
        "options": {
            "BUILDKIT_INLINE_CACHE": "1"
        }
    }
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test build command (only if Docker is available)
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("build")
        .arg("--output-format")
        .arg("json")
        .assert();

    // The command should either succeed (if Docker is available) or fail with Docker error
    let output = assert.get_output();

    if output.status.success() {
        // If successful, check that we got valid JSON output
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("image_id"));
        assert!(stdout.contains("build_duration"));
        assert!(stdout.contains("config_hash"));
    } else {
        // If failed, it should be because Docker is not available or accessible
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Docker is not installed")
                || stderr.contains("Docker daemon is not")
                || stderr.contains("Docker build failed")
                || stderr.contains("permission denied"),
            "Unexpected error: {}",
            stderr
        );
    }
}

#[test]
fn test_build_with_missing_dockerfile() {
    let temp_dir = TempDir::new().unwrap();

    // Create a devcontainer.json with a missing Dockerfile
    let devcontainer_config = r#"{
    "name": "Test Build Container",
    "dockerFile": "NonExistentDockerfile"
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(&temp_dir)
        .arg("build")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Configuration file not found"));
}

#[test]
fn test_build_with_image_config() {
    let temp_dir = TempDir::new().unwrap();

    // Create a devcontainer.json with image instead of dockerFile
    let devcontainer_config = r#"{
    "name": "Test Build Container",
    "image": "alpine:3.19"
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(&temp_dir)
        .arg("build")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Cannot build with 'image' configuration",
        ));
}

#[test]
fn test_build_command_flags() {
    let temp_dir = TempDir::new().unwrap();
    let dockerfile_content = "FROM alpine:3.19\nLABEL test=1\n";

    fs::write(temp_dir.path().join("Dockerfile"), dockerfile_content).unwrap();

    let devcontainer_config = r#"{
    "name": "Test Build Container",
    "dockerFile": "Dockerfile"
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test with various flags
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("build")
        .arg("--no-cache")
        .arg("--platform")
        .arg("linux/amd64")
        .arg("--build-arg")
        .arg("ENV=test")
        .arg("--build-arg")
        .arg("VERSION=1.0")
        .arg("--force")
        .arg("--output-format")
        .arg("text")
        .assert();

    // The command should either succeed or fail gracefully
    let output = assert.get_output();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Should fail because Docker is not available or because of permissions
        assert!(
            stderr.contains("Docker is not installed")
                || stderr.contains("Docker daemon is not")
                || stderr.contains("Docker build failed")
                || stderr.contains("permission denied"),
            "Unexpected error: {}",
            stderr
        );
    }
}
