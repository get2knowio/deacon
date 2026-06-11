//! Compound-flow regression tests for `up` → `exec --workspace-folder` (#187).
//!
//! These guard against `up` and `exec`/`run-user-commands` computing different
//! `devcontainer.configHash` values for the *same* authored `devcontainer.json`.
//! When they diverge, `exec --workspace-folder X` cannot resolve the container
//! that `up --workspace-folder X` just created (its label selector matches
//! nothing) and fails with "No running container found …".
//!
//! The original bug: `up` builds a Dockerfile config into a
//! `deacon-build:<hash>` image and stamps the identity from the *post-build*
//! config, while `exec` hashes the original `build.dockerfile` config. The fix
//! anchors `up`'s label identity to the config **as loaded**, before any
//! runtime mutation (image-metadata merge, feature merge, substitution, and the
//! Dockerfile build).
//!
//! Docker-gated: skips cleanly when Docker is unavailable. Uses a `TempDir`
//! workspace (in-repo `up` chowns the workspace — see CLAUDE.md) and targets
//! containers purely by `--workspace-folder` (never `--container-id`), which is
//! the whole point of the regression.

use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use tempfile::TempDir;

fn is_docker_available() -> bool {
    StdCommand::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn deacon() -> StdCommand {
    StdCommand::new(env!("CARGO_BIN_EXE_deacon"))
}

fn write(ws: &Path, rel: &str, body: &str) {
    let path = ws.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn up(ws: &Path) {
    let status = deacon()
        .args(["up", "--workspace-folder"])
        .arg(ws)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .expect("spawn deacon up");
    assert!(status.success(), "`deacon up` failed for {}", ws.display());
}

/// Run `exec --workspace-folder <ws> -- echo <marker>` (NO `--container-id`)
/// and return its trimmed stdout. The container must be resolved purely from
/// the workspace folder + config — exactly the path that regressed in #187.
fn exec_echo(ws: &Path, marker: &str) -> String {
    let out = deacon()
        .args(["exec", "--workspace-folder"])
        .arg(ws)
        .args(["echo", marker])
        .stderr(Stdio::inherit())
        .output()
        .expect("spawn deacon exec");
    assert!(
        out.status.success(),
        "`deacon exec --workspace-folder {}` failed (#187 regression: exec could not \
         resolve up's container by workspace folder)",
        ws.display()
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn down(ws: &Path) {
    let _ = deacon()
        .args(["down", "--workspace-folder"])
        .arg(ws)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Control case: a plain `image` config already resolved correctly; keep a
/// guard so it can't silently regress alongside the dockerfile fix.
#[test]
fn up_then_exec_resolves_image_config_by_workspace_folder() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let ws = TempDir::new().unwrap();
    write(
        ws.path(),
        ".devcontainer/devcontainer.json",
        r#"{ "name": "identity-image", "image": "alpine:3.18", "overrideCommand": true }"#,
    );

    up(ws.path());
    let got = exec_echo(ws.path(), "IDENTITY_IMAGE_OK");
    down(ws.path());

    assert_eq!(got, "IDENTITY_IMAGE_OK");
}

/// The #187 regression: a `build.dockerfile` config. `up` builds it into a
/// `deacon-build:<hash>` image; `exec` must still resolve the container by
/// workspace folder despite that runtime image substitution.
#[test]
fn up_then_exec_resolves_dockerfile_config_by_workspace_folder() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let ws = TempDir::new().unwrap();
    write(
        ws.path(),
        ".devcontainer/devcontainer.json",
        r#"{ "name": "identity-dockerfile", "build": { "dockerfile": "Dockerfile" }, "overrideCommand": true }"#,
    );
    write(
        ws.path(),
        ".devcontainer/Dockerfile",
        "FROM alpine:3.18\nRUN echo built > /identity-marker\n",
    );

    up(ws.path());
    let got = exec_echo(ws.path(), "IDENTITY_DOCKERFILE_OK");
    down(ws.path());

    assert_eq!(got, "IDENTITY_DOCKERFILE_OK");
}
