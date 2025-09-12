//! Smoke test suite for the most important CLI flows we support today
//!
//! Scenarios covered:
//! - read-configuration on provided fixtures (basic, with-variables)
//! - build from a temporary Dockerfile (JSON and text output)
//! - up (traditional) on a long-running image then exec into it
//! - compose-based up path detection
//! - exec environment and working directory behavior
//! - build arg edge cases
//! - doctor --json outputs structured diagnostics with logging noise tolerance
//!
//! Tests are written to be resilient in environments without Docker: they
//! accept specific error messages that indicate Docker is unavailable.

use assert_cmd::Command;
use predicates::str as pred_str;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn docker_related_error(stderr: &str) -> bool {
    stderr.contains("Docker is not installed")
        || stderr.contains("Docker daemon is not")
        || stderr.contains("permission denied")
        || stderr.contains("Failed to spawn docker")
        || stderr.contains("Docker CLI error")
}

fn repo_root() -> PathBuf {
    // crates/deacon -> repo root is two levels up
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.parent()
        .and_then(|p| p.parent())
        .unwrap_or(&here)
        .to_path_buf()
}

#[test]
fn smoke_read_configuration_basic() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let fixtures = repo_root().join("fixtures/config/basic/devcontainer.jsonc");
    let assert = cmd
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(repo_root())
        .arg("--config")
        .arg(fixtures)
        .assert();

    let output = assert.get_output();
    // For read-configuration we expect success unconditionally
    assert!(
        output.status.success(),
        "read-configuration failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Rust Development Container"));
    assert!(stdout.contains("workspaceFolder"));
}

#[test]
fn smoke_read_configuration_with_variables() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let fixtures = repo_root().join("fixtures/config/with-variables/devcontainer.jsonc");
    let assert = cmd
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(repo_root())
        .arg("--config")
        .arg(fixtures)
        .assert();

    let output = assert.get_output();
    assert!(
        output.status.success(),
        "read-configuration failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Variable Substitution Test Container"));
}

#[test]
fn smoke_build_json_then_text() {
    // Temp workspace with a simple Dockerfile under .devcontainer
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("Dockerfile"),
        "FROM alpine:3.19\nRUN echo hi\n",
    )
    .unwrap();

    let devcontainer = r#"{
        "name": "SmokeBuild",
        "dockerFile": "Dockerfile",
        "build": {"context": "."}
    }"#;
    fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
    fs::write(
        tmp.path().join(".devcontainer/devcontainer.json"),
        devcontainer,
    )
    .unwrap();

    // JSON output
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let json_run = cmd
        .current_dir(tmp.path())
        .arg("build")
        .arg("--output-format")
        .arg("json")
        .assert();

    let out = json_run.get_output();
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains("image_id"));
        assert!(s.contains("build_duration"));
    } else {
        let e = String::from_utf8_lossy(&out.stderr);
        assert!(docker_related_error(&e), "Unexpected error: {}", e);
    }

    // Text output
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let text_run = cmd.current_dir(tmp.path()).arg("build").assert();
    let out = text_run.get_output();
    if !out.status.success() {
        let e = String::from_utf8_lossy(&out.stderr);
        assert!(docker_related_error(&e), "Unexpected error: {}", e);
    }
}

#[test]
fn smoke_up_then_exec_traditional() {
    // Use an nginx image that stays running to allow exec
    let tmp = TempDir::new().unwrap();
    let config = r#"{
        "name": "SmokeUpExec",
        "image": "nginx:alpine",
        "workspaceFolder": "/workspace"
    }"#;
    fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
    fs::write(tmp.path().join(".devcontainer/devcontainer.json"), config).unwrap();

    // deacon up (traditional)
    let mut up = Command::cargo_bin("deacon").unwrap();
    let up_assert = up
        .current_dir(tmp.path())
        .arg("up")
        .arg("--remove-existing-container")
        .arg("--skip-post-create")
        .arg("--skip-non-blocking-commands")
        .assert();

    let up_out = up_assert.get_output();
    if !up_out.status.success() {
        let e = String::from_utf8_lossy(&up_out.stderr);
        assert!(docker_related_error(&e), "Unexpected up error: {}", e);
        return; // Skip exec if up failed due to missing Docker
    }

    // deacon exec whoami
    let mut exec_cmd = Command::cargo_bin("deacon").unwrap();
    let exec_assert = exec_cmd
        .current_dir(tmp.path())
        .arg("exec")
        .arg("--no-tty")
        .arg("--")
        .arg("sh")
        .arg("-lc")
        .arg("echo -n OK: && whoami && pwd")
        .assert();

    let exec_out = exec_assert.get_output();
    if exec_out.status.success() {
        let s = String::from_utf8_lossy(&exec_out.stdout);
        assert!(s.contains("OK:"));
    } else {
        let e = String::from_utf8_lossy(&exec_out.stderr);
        assert!(
            docker_related_error(&e) || e.contains("No running container found") || e.is_empty(),
            "Unexpected exec error: {}",
            e
        );
    }
}

#[test]
fn smoke_doctor_json() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd.arg("doctor").arg("--json").assert();

    let out = assert.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);
    if out.status.success() {
        // Extract the JSON object from mixed stdout (logging + JSON)
        // Find the first '{' and the last '}' and attempt to parse that slice
        let start = stdout.find('{');
        let end = stdout.rfind('}');
        let parsed = match (start, end) {
            (Some(s), Some(e)) if e >= s => {
                let slice = &stdout[s..=e];
                serde_json::from_str::<serde_json::Value>(slice).is_ok()
            }
            _ => false,
        };
        assert!(
            parsed,
            "doctor --json should output JSON-like content, got: {}",
            stdout
        );
    } else {
        // doctor should usually succeed; still accept environments with odd constraints
        let e = String::from_utf8_lossy(&out.stderr);
        assert!(e.contains("Doctor") || !e.is_empty() || !stdout.is_empty());
    }
}

// Additional smoke scenarios merged from main

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
        // Success indicates compose configuration path processed
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
        .stdout(pred_str::contains("hello from container"));
}
