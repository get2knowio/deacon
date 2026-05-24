#![cfg(feature = "full")]
//! Integration tests for compose overrideCommand support (Bead 13).
//!
//! Covers BEAD-13-T01, T02, T04 from .maverick/plans/consumer-pt2/briefing.md:
//! - T01: overrideCommand=true (default) keeps a short-lived compose service alive
//! - T02: overrideCommand=false runs the service's natural command (may exit)
//! - T04: lifecycle commands execute successfully in compose mode with override active
//!
//! These hit a real Docker daemon and are docker-gated via a graceful skip.

use assert_cmd::Command;
use std::fs;
use std::path::Path;
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

/// Best-effort cleanup; ignore failures since the project may already be torn down.
fn deacon_down(workspace: &Path) {
    let _ = Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(workspace)
        .arg("down")
        .arg("--workspace-folder")
        .arg(workspace)
        .output();
}

/// Look up the running container id for a compose service via `docker compose ps`.
fn compose_service_container_id(workspace: &Path, service: &str) -> Option<String> {
    let output = std::process::Command::new("docker")
        .current_dir(workspace)
        .args(["compose", "ps", "-q", service])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

fn docker_inspect_state_running(container_id: &str) -> Option<bool> {
    let output = std::process::Command::new("docker")
        .args(["inspect", "--format", "{{.State.Running}}", container_id])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Some(text == "true")
}

fn docker_inspect_cmd(container_id: &str) -> Option<String> {
    let output = std::process::Command::new("docker")
        .args(["inspect", "--format", "{{json .Config.Cmd}}", container_id])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// BEAD-13-T01: default overrideCommand keeps a short-lived service running.
#[test]
fn test_compose_override_command_default_keeps_service_alive() {
    if !is_docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Compose service runs `echo hello` — would exit in milliseconds without override.
    let compose_yml = r#"services:
  app:
    image: alpine:3.18
    command: ["echo", "hello"]
"#;
    let devcontainer_json = r#"{
  "name": "Compose Override Default",
  "dockerComposeFile": "docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace"
}"#;

    fs::write(workspace.join("docker-compose.yml"), compose_yml).unwrap();
    fs::create_dir(workspace.join(".devcontainer")).unwrap();
    fs::write(
        workspace.join(".devcontainer/devcontainer.json"),
        devcontainer_json,
    )
    .unwrap();

    let up_output = Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(workspace)
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&up_output.stderr).to_string();
    let success = up_output.status.success();

    if !success {
        deacon_down(workspace);
        panic!("deacon up failed: {}", stderr);
    }

    let container_id =
        compose_service_container_id(workspace, "app").expect("compose ps should return id");
    let running = docker_inspect_state_running(&container_id).unwrap_or(false);

    deacon_down(workspace);

    assert!(
        running,
        "container should still be running with default overrideCommand=true; stderr was: {}",
        stderr
    );
}

/// BEAD-13-T02: overrideCommand=false runs the service's natural command.
#[test]
fn test_compose_override_command_explicit_false_runs_natural_command() {
    if !is_docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Natural command is `sleep 30` — long enough to inspect, distinct from our
    // override's `sleep infinity || tail -f /dev/null`.
    let compose_yml = r#"services:
  app:
    image: alpine:3.18
    command: ["sleep", "30"]
"#;
    let devcontainer_json = r#"{
  "name": "Compose Override False",
  "dockerComposeFile": "docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace",
  "overrideCommand": false
}"#;

    fs::write(workspace.join("docker-compose.yml"), compose_yml).unwrap();
    fs::create_dir(workspace.join(".devcontainer")).unwrap();
    fs::write(
        workspace.join(".devcontainer/devcontainer.json"),
        devcontainer_json,
    )
    .unwrap();

    let up_output = Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(workspace)
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&up_output.stderr).to_string();
    if !up_output.status.success() {
        deacon_down(workspace);
        panic!("deacon up failed: {}", stderr);
    }

    let container_id =
        compose_service_container_id(workspace, "app").expect("compose ps should return id");
    let cmd_json = docker_inspect_cmd(&container_id).unwrap_or_default();

    deacon_down(workspace);

    // The container's CMD must be the compose-file natural command, not our override.
    assert!(
        cmd_json.contains("sleep") && cmd_json.contains("30"),
        "container CMD should be the natural [sleep 30], got: {}",
        cmd_json
    );
    assert!(
        !cmd_json.contains("sleep infinity"),
        "container CMD should NOT be the deacon override; got: {}",
        cmd_json
    );
}

/// BEAD-13-T04: lifecycle commands execute in compose mode with override active.
#[test]
fn test_compose_override_command_lifecycle_runs() {
    if !is_docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Without our override, `echo init` would exit before postCreateCommand
    // could run. With override active, the marker file proves the lifecycle ran.
    let compose_yml = r#"services:
  app:
    image: alpine:3.18
    command: ["echo", "init"]
"#;
    let devcontainer_json = r#"{
  "name": "Compose Lifecycle Marker",
  "dockerComposeFile": "docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace",
  "postCreateCommand": "touch /tmp/deacon-lifecycle-marker"
}"#;

    fs::write(workspace.join("docker-compose.yml"), compose_yml).unwrap();
    fs::create_dir(workspace.join(".devcontainer")).unwrap();
    fs::write(
        workspace.join(".devcontainer/devcontainer.json"),
        devcontainer_json,
    )
    .unwrap();

    let up_output = Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(workspace)
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&up_output.stderr).to_string();
    if !up_output.status.success() {
        deacon_down(workspace);
        panic!("deacon up failed: {}", stderr);
    }

    let container_id =
        compose_service_container_id(workspace, "app").expect("compose ps should return id");

    let marker = std::process::Command::new("docker")
        .args([
            "exec",
            &container_id,
            "test",
            "-f",
            "/tmp/deacon-lifecycle-marker",
        ])
        .output()
        .unwrap();

    deacon_down(workspace);

    assert!(
        marker.status.success(),
        "postCreateCommand should have created /tmp/deacon-lifecycle-marker; stderr from up: {}",
        stderr
    );
}
