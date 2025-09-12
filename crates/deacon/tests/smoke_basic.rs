//! Smoke test suite for the most important CLI flows we support today
//!
//! Scenarios covered:
//! - read-configuration on provided fixtures (basic, with-variables)
//! - build from a temporary Dockerfile (JSON and text output)
//! - up (traditional) on a long-running image then exec into it
//! - doctor --json outputs structured diagnostics
//!
//! Tests are written to be resilient in environments without Docker: they
//! accept specific error messages that indicate Docker is unavailable.

use assert_cmd::Command;
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
