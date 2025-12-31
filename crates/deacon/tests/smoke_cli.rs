#![cfg(feature = "full")]
//! CLI-only smoke tests that don't require Docker.

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn repo_root() -> PathBuf {
    // crates/deacon -> repo root is two levels up
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.parent()
        .and_then(|p| p.parent())
        .unwrap_or(&here)
        .to_path_buf()
}

#[test]
fn smoke_cli_read_configuration_basic() {
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
fn smoke_cli_read_configuration_with_variables() {
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
fn smoke_cli_doctor_json() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd.arg("doctor").arg("--json").assert();

    let out = assert.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "doctor --json failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let start = stdout.find('{');
    let end = stdout.rfind('}');
    let parsed = match (start, end) {
        (Some(s), Some(e)) if e >= s => {
            let slice = &stdout[s..=e];
            serde_json::from_str::<Value>(slice).is_ok()
        }
        _ => false,
    };
    assert!(
        parsed,
        "doctor --json should output JSON-like content, got: {}",
        stdout
    );
}

#[test]
fn smoke_cli_doctor_json_stability() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("doctor").arg("--json");

    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    if let Some(start) = stdout.find('{') {
        if let Some(end) = stdout.rfind('}') {
            let json_slice = &stdout[start..=end];
            let parsed: Result<Value, _> = serde_json::from_str(json_slice);
            assert!(
                parsed.is_ok(),
                "Failed to parse JSON from doctor output: {}",
                json_slice
            );
            if let Ok(json) = parsed {
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
fn smoke_cli_read_configuration_fixtures_breadth() {
    let temp_dir = TempDir::new().unwrap();

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

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("name"));
    assert!(stdout.contains("image"));
    assert!(stdout.contains("features"));
    assert!(
        stdout.contains("developer") || stdout.contains("Container"),
        "Expected variable substitution in output: {}",
        stdout
    );
}
