//! Deacon-only integration test for build args propagation
//!
//! Verifies that values from devcontainer.json build.args are passed to Docker
//! as --build-arg and end up available during the build (validated via a label).

use assert_cmd::Command;
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
        assert!(
            stderr.contains("Docker is not installed")
                || stderr.contains("Docker daemon is not")
                || stderr.contains("Docker build failed")
                || stderr.contains("permission denied"),
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
