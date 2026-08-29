//! Integration tests for compose overrideCommand support (Bead 13).
//!
//! Covers BEAD-13-T01, T02, T04 from .maverick/plans/consumer-pt2/briefing.md:
//! - T01: overrideCommand=true (default) keeps a short-lived compose service alive
//! - T02: overrideCommand=false runs the service's natural command (may exit)
//! - T04: lifecycle commands execute successfully in compose mode with override active
//!
//! These hit a real Docker daemon and are docker-gated via a graceful skip.

mod support;

use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn is_docker_available() -> bool {
    std::process::Command::new(support::runtime_bin())
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Best-effort cleanup; ignore failures since the project may already be torn down.
fn deacon_down(workspace: &Path) {
    let _ = support::deacon_command()
        .current_dir(workspace)
        .arg("down")
        .arg("--workspace-folder")
        .arg(workspace)
        .output();
}

/// Extract the primary service container id from `deacon up`'s JSON result.
///
/// Using deacon's own reported `containerId` is robust to the compose project
/// name (deacon-namespaced as `deacon_<stem>_<workspace_hash>_<config_hash>` —
/// see #265 and #564 — and not what a bare `docker compose ps` from the
/// workspace would infer).
fn up_container_id(up_output: &std::process::Output) -> Option<String> {
    let stdout = String::from_utf8_lossy(&up_output.stdout);
    let trimmed = stdout.trim();
    let value: serde_json::Value = serde_json::from_str(trimmed).ok().or_else(|| {
        trimmed
            .rfind('{')
            .and_then(|i| serde_json::from_str(&trimmed[i..]).ok())
    })?;
    value
        .get("containerId")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn docker_inspect_state_running(container_id: &str) -> Option<bool> {
    let output = std::process::Command::new(support::runtime_bin())
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
    let output = std::process::Command::new(support::runtime_bin())
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
  "dockerComposeFile": "../docker-compose.yml",
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

    let up_output = support::deacon_command()
        .current_dir(workspace)
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&up_output.stderr).to_string();
    let success = up_output.status.success();

    if !success {
        // Snapshot the runtime BEFORE tearing down. This file carries the #723
        // Podman flake (`conmon bytes ""` on an exec), and the job-level
        // `if: failure()` diagnostics structurally cannot answer the question
        // that matters -- whether the container was running when the exec was
        // attempted -- because the `deacon_down` below has already removed it by
        // the time a job-level step runs.
        let state = support::runtime_state_dump(workspace);
        deacon_down(workspace);
        panic!(
            "deacon up failed: {}\n\n=== runtime state at failure ===\n{}",
            stderr, state
        );
    }

    let container_id = up_container_id(&up_output).expect("deacon up should report a containerId");
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
  "dockerComposeFile": "../docker-compose.yml",
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

    let up_output = support::deacon_command()
        .current_dir(workspace)
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&up_output.stderr).to_string();
    if !up_output.status.success() {
        // Snapshot the runtime BEFORE tearing down. This file carries the #723
        // Podman flake (`conmon bytes ""` on an exec), and the job-level
        // `if: failure()` diagnostics structurally cannot answer the question
        // that matters -- whether the container was running when the exec was
        // attempted -- because the `deacon_down` below has already removed it by
        // the time a job-level step runs.
        let state = support::runtime_state_dump(workspace);
        deacon_down(workspace);
        panic!(
            "deacon up failed: {}\n\n=== runtime state at failure ===\n{}",
            stderr, state
        );
    }

    let container_id = up_container_id(&up_output).expect("deacon up should report a containerId");
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
    //
    // This config declares NO `workspaceFolder` (#460). It used to declare
    // `/workspace` while the service mounted nothing there, and the test passed
    // anyway because the compose path ran its hook through a `cd '<dir>'
    // 2>/dev/null; …` wrapper that shrugged off a missing directory. Now that the
    // compose path uses the shared lifecycle engine, the hook runs under
    // `docker exec -w <workspaceFolder>` and an unmounted one is fatal — which is
    // the REFERENCE's behavior, measured at oracle 0.87.0 on exactly that shape:
    // `chdir to cwd ("/workspace") set in config.json failed: no such file or
    // directory`, the hook exits 127 and `up` exits 1, character for character
    // what deacon now emits. The leniency was the divergence, so the incoherent
    // fixture is corrected rather than the behavior.
    //
    // It is corrected by DROPPING the declaration rather than by mounting it: a
    // compose config with no explicit `workspaceFolder` resolves to `/`
    // (#294/#295), which always exists, and the service keeps the exact shape it
    // has on every runtime today. Adding a `.:/workspace` bind here instead made
    // the container exit before the hook under rootless Podman, which is a
    // property of the added mount and has nothing to do with what this test is
    // for — that lifecycle runs at all once the override keeps the container alive.
    let compose_yml = r#"services:
  app:
    image: alpine:3.18
    command: ["echo", "init"]
"#;
    let devcontainer_json = r#"{
  "name": "Compose Lifecycle Marker",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "postCreateCommand": "touch /tmp/deacon-lifecycle-marker"
}"#;

    fs::write(workspace.join("docker-compose.yml"), compose_yml).unwrap();
    fs::create_dir(workspace.join(".devcontainer")).unwrap();
    fs::write(
        workspace.join(".devcontainer/devcontainer.json"),
        devcontainer_json,
    )
    .unwrap();

    let up_output = support::deacon_command()
        .current_dir(workspace)
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&up_output.stderr).to_string();
    if !up_output.status.success() {
        // Snapshot the runtime BEFORE tearing down. This file carries the #723
        // Podman flake (`conmon bytes ""` on an exec), and the job-level
        // `if: failure()` diagnostics structurally cannot answer the question
        // that matters -- whether the container was running when the exec was
        // attempted -- because the `deacon_down` below has already removed it by
        // the time a job-level step runs.
        let state = support::runtime_state_dump(workspace);
        deacon_down(workspace);
        panic!(
            "deacon up failed: {}\n\n=== runtime state at failure ===\n{}",
            stderr, state
        );
    }

    let container_id = up_container_id(&up_output).expect("deacon up should report a containerId");

    let marker = std::process::Command::new(support::runtime_bin())
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
