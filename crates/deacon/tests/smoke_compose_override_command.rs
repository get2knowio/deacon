//! Integration tests for compose overrideCommand support (Bead 13, corrected by T117).
//!
//! Covers BEAD-13-T01, T02, T04 from .maverick/plans/consumer-pt2/briefing.md:
//! - T01: on compose, the DEFAULT runs the service's declared command (spec default `false`)
//! - T02: an explicit `overrideCommand: false` runs the service's natural command
//! - T04: lifecycle commands execute in compose mode when `overrideCommand: true` keeps the
//!   container alive
//!
//! **T117 corrected T01 and T04's premise.** Both were written asserting that the compose
//! default is `overrideCommand: true` — that deacon keeps a service whose command is
//! `echo hello` alive by replacing that command. The spec says the opposite:
//! `overrideCommand` *"Defaults to `true` for when using an image Dockerfile and `false` when
//! referencing a Docker Compose file"*, because *"the default command must run for the
//! container to function properly"*.
//!
//! Verified against the pinned oracle 0.87.0 rather than argued: on the `echo hello` fixture
//! `devcontainer up` **fails** with `{"outcome":"error"}` and exit 1, its container `exited`
//! with code 0 — because the declared command ran and finished. deacon now does the same.
//! These two tests were asserting deacon's defect, which is why the defect survived: any
//! compose service whose command matters never ran.
//!
//! These hit a real Docker daemon and are docker-gated via a graceful skip.

mod support;

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
/// name (deacon-namespaced as `deacon_<workspace_hash>_<config_hash>` — see
/// #265 — and not what a bare `docker compose ps` from the workspace would
/// infer).
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

/// BEAD-13-T01 (corrected, T117): on compose the DEFAULT runs the service's declared
/// command — it is not replaced by deacon's keep-alive.
///
/// The spec default for compose is `overrideCommand: false`, and this fixture omits the key
/// entirely, so the declared command must run. It proves that by leaving a marker file.
///
/// The test previously asserted the opposite — that a service whose command is `echo hello`
/// stays ALIVE, which is only true if the command is discarded. That premise was deacon's
/// defect, and it is why the defect survived: verified against the pinned oracle 0.87.0, the
/// reference FAILS that fixture (`{"outcome":"error"}`, exit 1) because it honors the
/// default and the container exits with its command.
///
/// The assertion is deliberately on the MARKER, not on `up` failing with a short-lived
/// command. Whether a millisecond-lived container is still present when deacon looks for it
/// is a race — a first draft of this test asserting `up` fails was flaky under parallel load
/// for exactly that reason. "Did the declared command run?" is the actual claim and is
/// deterministic.
#[test]
fn test_compose_default_runs_the_declared_command() {
    if !is_docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Long enough to inspect, and it records that it ran. `sleep infinity` would be
    // indistinguishable from deacon's own keep-alive; the marker is what makes this test
    // able to fail.
    let compose_yml = r#"services:
  app:
    image: alpine:3.18
    command: ["sh", "-c", "touch /tmp/declared-command-ran; sleep 60"]
"#;
    // NOTE: no `overrideCommand` key — the point is the DEFAULT.
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
    if !up_output.status.success() {
        deacon_down(workspace);
        panic!("deacon up failed: {}", stderr);
    }

    let container_id = up_container_id(&up_output).expect("deacon up should report a containerId");
    let marker = std::process::Command::new("docker")
        .args([
            "exec",
            &container_id,
            "test",
            "-f",
            "/tmp/declared-command-ran",
        ])
        .output()
        .unwrap();
    let cmd_json = docker_inspect_cmd(&container_id).unwrap_or_default();

    deacon_down(workspace);

    assert!(
        marker.status.success(),
        "the compose service's DECLARED command must run under the default \
         (overrideCommand defaults to false for compose) — no marker means deacon replaced \
         it, so the service's own process never ran (T117). Container Cmd was: {cmd_json}; \
         stderr from up: {stderr}"
    );
    assert!(
        !cmd_json.contains("sleep infinity"),
        "the container's Cmd must still be the service's declared command, not deacon's \
         keep-alive; got: {cmd_json}"
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
        deacon_down(workspace);
        panic!("deacon up failed: {}", stderr);
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

/// BEAD-13-T04 (corrected, T117): lifecycle commands execute in compose mode when
/// `overrideCommand: true` keeps the container alive.
///
/// The fixture now sets `overrideCommand: true` EXPLICITLY. It previously relied on the
/// default doing so, which is the T117 defect: on compose the default is `false`, so
/// `echo init` runs, the container exits, and no lifecycle hook can attach — the reference
/// fails this fixture too. Asking for the keep-alive is what the spec's `true` is for, and
/// the test's real subject is that lifecycle hooks run once the container IS alive.
#[test]
fn test_compose_override_command_lifecycle_runs() {
    if !is_docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // `echo init` exits immediately, so `overrideCommand: true` — which CLEARS the command
    // and lets the keep-alive entrypoint hold the container open — is required for any
    // lifecycle hook to run at all. The marker file proves it ran.
    let compose_yml = r#"services:
  app:
    image: alpine:3.18
    command: ["echo", "init"]
"#;
    let devcontainer_json = r#"{
  "name": "Compose Lifecycle Marker",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace",
  "overrideCommand": true,
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
        deacon_down(workspace);
        panic!("deacon up failed: {}", stderr);
    }

    let container_id = up_container_id(&up_output).expect("deacon up should report a containerId");

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
