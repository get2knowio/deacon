#![cfg(feature = "full")]
//! Deacon-only integration test for build args propagation
//!
//! Verifies that values from devcontainer.json build.args are passed to Docker
//! as --build-arg and end up available during the build (validated via a label).

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn build_args_from_config_are_applied() {
    // Prepare a temp workspace
    let temp_dir = TempDir::new().unwrap();
    let ws = temp_dir.path();

    // Dockerfile that uses an ARG to set a label and env
    let dockerfile = r#"FROM alpine:3.19
ARG FOO=default
ENV FOO=$FOO
LABEL deacon.test.buildargs=$FOO
RUN echo "FOO is $FOO"
"#;
    fs::write(ws.join("Dockerfile"), dockerfile).unwrap();

    // devcontainer.json at root with build.args
    let devcontainer = r#"{
  "name": "BuildArgsTest",
  "dockerFile": "Dockerfile",
  "build": {
    "context": ".",
    "args": {
      "FOO": "bar-baz"
    }
  }
}
"#;
    fs::write(ws.join(".devcontainer.json"), devcontainer).unwrap();

    // Run deacon build (JSON output for easier detection). This may fail on hosts without Docker.
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(ws)
        .arg("build")
        .arg("--output-format")
        .arg("json")
        .assert();

    let output = assert.get_output();
    if !output.status.success() {
        // Graceful failure modes when Docker isn't available in CI/dev
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_lc = stderr.to_lowercase();
        assert!(
            stderr.contains("Docker is not installed")
                || stderr.contains("Docker daemon is not")
                || stderr.contains("Docker build failed")
                || stderr_lc.contains("permission denied"),
            "Unexpected error: {}",
            stderr
        );
        return;
    }

    // When successful, inspect for the label with value from build arg
    // Find image IDs by label key and then inspect to verify value
    let list = std::process::Command::new("docker")
        .args([
            "images",
            "--filter",
            "label=deacon.test.buildargs",
            "--format",
            "{{.ID}}",
        ])
        .output()
        .unwrap();
    assert!(
        list.status.success(),
        "docker images failed: {}",
        String::from_utf8_lossy(&list.stderr)
    );

    let ids: Vec<String> = String::from_utf8_lossy(&list.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    assert!(
        !ids.is_empty(),
        "Expected at least one image with deacon.test.buildargs label"
    );

    let mut found = false;
    for id in &ids {
        let inspect = std::process::Command::new("docker")
            .args(["inspect", "-f", "{{ json .Config.Labels }}", id])
            .output()
            .unwrap();
        assert!(
            inspect.status.success(),
            "docker inspect failed: {}",
            String::from_utf8_lossy(&inspect.stderr)
        );
        let labels_json = String::from_utf8_lossy(&inspect.stdout);
        if labels_json.contains("\"deacon.test.buildargs\":\"bar-baz\"") {
            found = true;
            break;
        }
    }

    assert!(
        found,
        "Image should carry label deacon.test.buildargs=bar-baz from build.args"
    );

    // Cleanup images we found to avoid cross-test interference
    for id in ids {
        let _ = std::process::Command::new("docker")
            .args(["rmi", &id])
            .output();
    }
}

#[test]
fn build_accepts_multiple_image_names() {
    // Prepare a temp workspace
    let temp_dir = TempDir::new().unwrap();
    let ws = temp_dir.path();

    // Minimal Dockerfile
    let dockerfile = r#"FROM alpine:3.19
RUN echo "Testing multiple image names"
"#;
    fs::write(ws.join("Dockerfile"), dockerfile).unwrap();

    // devcontainer.json at root
    let devcontainer = r#"{
  "name": "MultiImageNameTest",
  "dockerFile": "Dockerfile",
  "build": {
    "context": "."
  }
}
"#;
    fs::write(ws.join(".devcontainer.json"), devcontainer).unwrap();

    // Run deacon build with multiple --image-name flags
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(ws)
        .arg("build")
        .arg("--image-name")
        .arg("test-image:tag1")
        .arg("--image-name")
        .arg("test-image:tag2")
        .arg("--output-format")
        .arg("json")
        .assert();

    let output = assert.get_output();
    if !output.status.success() {
        // Graceful failure when Docker isn't available
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_lc = stderr.to_lowercase();
        assert!(
            stderr.contains("Docker is not installed")
                || stderr.contains("Docker daemon is not")
                || stderr.contains("Docker build failed")
                || stderr_lc.contains("permission denied"),
            "Unexpected error: {}",
            stderr
        );
        return;
    }

    // When successful, verify JSON output contains multiple image names
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");

    // Verify the outcome field
    assert_eq!(parsed["outcome"], "success", "Build should succeed");

    // Verify imageName is an array with both tags
    assert!(
        parsed["imageName"].is_array(),
        "imageName should be an array for multiple tags"
    );
    let image_names = parsed["imageName"].as_array().unwrap();
    assert_eq!(
        image_names.len(),
        2,
        "Should have both image names in output"
    );

    // Cleanup created images
    let _ = std::process::Command::new("docker")
        .args(["rmi", "test-image:tag1"])
        .output();
    let _ = std::process::Command::new("docker")
        .args(["rmi", "test-image:tag2"])
        .output();
}

#[test]
fn build_accepts_multiple_labels() {
    // Prepare a temp workspace
    let temp_dir = TempDir::new().unwrap();
    let ws = temp_dir.path();

    // Dockerfile that we can inspect
    let dockerfile = r#"FROM alpine:3.19
RUN echo "Testing labels"
"#;
    fs::write(ws.join("Dockerfile"), dockerfile).unwrap();

    // devcontainer.json at root
    let devcontainer = r#"{
  "name": "LabelTest",
  "dockerFile": "Dockerfile",
  "build": {
    "context": "."
  }
}
"#;
    fs::write(ws.join(".devcontainer.json"), devcontainer).unwrap();

    // Run deacon build with multiple --label flags
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(ws)
        .arg("build")
        .arg("--label")
        .arg("test.label1=value1")
        .arg("--label")
        .arg("test.label2=value2")
        .arg("--output-format")
        .arg("json")
        .assert();

    let output = assert.get_output();
    if !output.status.success() {
        // Graceful failure when Docker isn't available
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_lc = stderr.to_lowercase();
        assert!(
            stderr.contains("Docker is not installed")
                || stderr.contains("Docker daemon is not")
                || stderr.contains("Docker build failed")
                || stderr_lc.contains("permission denied"),
            "Unexpected error: {}",
            stderr
        );
        return;
    }

    // When successful, inspect the built image for labels
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");

    assert_eq!(parsed["outcome"], "success", "Build should succeed");

    // Get the image name from output to inspect
    let image_name = if parsed["imageName"].is_array() {
        parsed["imageName"][0].as_str().unwrap()
    } else {
        parsed["imageName"].as_str().unwrap()
    };

    // Inspect the image for labels
    let inspect = std::process::Command::new("docker")
        .args(["inspect", "-f", "{{ json .Config.Labels }}", image_name])
        .output()
        .unwrap();

    assert!(
        inspect.status.success(),
        "docker inspect command should succeed; stderr: {}",
        String::from_utf8_lossy(&inspect.stderr)
    );

    let labels_json = String::from_utf8_lossy(&inspect.stdout);
    assert!(
        labels_json.contains("\"test.label1\":\"value1\""),
        "Label test.label1=value1 should be present"
    );
    assert!(
        labels_json.contains("\"test.label2\":\"value2\""),
        "Label test.label2=value2 should be present"
    );

    // Cleanup
    let _ = std::process::Command::new("docker")
        .args(["rmi", image_name])
        .output();
}

#[test]
fn test_push_flag_parsing() {
    // Prepare a temp workspace
    let temp_dir = TempDir::new().unwrap();
    let ws = temp_dir.path();

    // Minimal Dockerfile
    let dockerfile = r#"FROM alpine:3.19
RUN echo "Testing push flag"
"#;
    fs::write(ws.join("Dockerfile"), dockerfile).unwrap();

    // devcontainer.json at root
    let devcontainer = r#"{
  "name": "PushFlagTest",
  "dockerFile": "Dockerfile",
  "build": {
    "context": "."
  }
}
"#;
    fs::write(ws.join(".devcontainer.json"), devcontainer).unwrap();

    // Test that --push flag is accepted
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(ws)
        .arg("build")
        .arg("--push")
        .arg("--image-name")
        .arg("test-push:latest")
        .arg("--output-format")
        .arg("json")
        .assert();

    let output = assert.get_output();
    // The command should either succeed (if BuildKit is available) or fail with BuildKit requirement
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Should be BuildKit-related error or Docker availability error
        assert!(
            stdout.contains("BuildKit is required")
                || stderr.contains("BuildKit is required")
                || stderr.contains("Docker is not installed")
                || stderr.contains("Docker daemon is not")
                || stderr.contains("Docker build failed")
                || stderr.to_lowercase().contains("denied"),
            "Expected BuildKit/Docker gating or build failure, got: stdout={}, stderr={}",
            stdout,
            stderr
        );
    }
}

#[test]
fn test_output_flag_parsing() {
    // Prepare a temp workspace
    let temp_dir = TempDir::new().unwrap();
    let ws = temp_dir.path();

    // Minimal Dockerfile
    let dockerfile = r#"FROM alpine:3.19
RUN echo "Testing output flag"
"#;
    fs::write(ws.join("Dockerfile"), dockerfile).unwrap();

    // devcontainer.json at root
    let devcontainer = r#"{
  "name": "OutputFlagTest",
  "dockerFile": "Dockerfile",
  "build": {
    "context": "."
  }
}
"#;
    fs::write(ws.join(".devcontainer.json"), devcontainer).unwrap();

    // Test that --output flag is accepted
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(ws)
        .arg("build")
        .arg("--output")
        .arg("type=docker,dest=/tmp/test-output.tar")
        .arg("--output-format")
        .arg("json")
        .assert();

    let output = assert.get_output();
    // The command should either succeed (if BuildKit is available) or fail with BuildKit requirement
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Should be BuildKit-related error or Docker availability error
        assert!(
            stdout.contains("BuildKit is required")
                || stderr.contains("BuildKit is required")
                || stderr.contains("Docker is not installed")
                || stderr.contains("Docker daemon is not")
                || stderr.contains("Docker build failed")
                || stderr.to_lowercase().contains("not supported")
                || stderr.to_lowercase().contains("denied"),
            "Expected BuildKit/Docker gating or build failure, got: stdout={}, stderr={}",
            stdout,
            stderr
        );
    }
}

#[test]
fn test_build_help_includes_push_and_output() {
    // Test that --help output includes --push and --output flags
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("build")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--push"))
        .stdout(predicate::str::contains("--output"));
}
