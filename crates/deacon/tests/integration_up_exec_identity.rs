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

/// The container runtime binary under test (honors `DEACON_CONTAINER_RUNTIME`,
/// the same env var deacon reads). Setup/cleanup that bypasses deacon must use
/// this so images/containers land in the store deacon-under-podman actually reads.
fn runtime_bin() -> String {
    std::env::var("DEACON_CONTAINER_RUNTIME").unwrap_or_else(|_| "docker".to_string())
}

fn is_docker_available() -> bool {
    StdCommand::new(runtime_bin())
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

/// Run `exec --workspace-folder <ws> -- <args...>` and return trimmed stdout.
fn exec_cmd(ws: &Path, cmd: &[&str]) -> String {
    let out = deacon()
        .args(["exec", "--workspace-folder"])
        .arg(ws)
        .arg("--")
        .args(cmd)
        .stderr(Stdio::inherit())
        .output()
        .expect("spawn deacon exec");
    assert!(
        out.status.success(),
        "`deacon exec --workspace-folder {}` failed",
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

/// #223: when `remoteUser` comes only from the image's `devcontainer.metadata`
/// LABEL (not the user's devcontainer.json), `exec` must run as that user —
/// matching what `up` reports — instead of falling back to root. `up` merges
/// image metadata as a lower-precedence layer; `exec` previously loaded only the
/// raw config and so lost the metadata-derived user.
#[test]
fn exec_honors_remote_user_from_image_metadata() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }

    // Build a tiny image with a non-root `appuser` and a devcontainer.metadata
    // label that sets remoteUser. The devcontainer.json below does NOT set a
    // user, so the only source of `appuser` is the image label.
    let tag = "deacon-test-img-metadata-user:latest";
    let build_dir = TempDir::new().unwrap();
    std::fs::write(
        build_dir.path().join("Dockerfile"),
        "FROM alpine:3.18\n\
         RUN adduser -D appuser\n\
         LABEL devcontainer.metadata='[{\"remoteUser\":\"appuser\"}]'\n",
    )
    .unwrap();
    let built = StdCommand::new(runtime_bin())
        .args(["build", "-t", tag, "."])
        .current_dir(build_dir.path())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .expect("spawn docker build");
    assert!(built.success(), "failed to build metadata-label test image");

    let ws = TempDir::new().unwrap();
    write(
        ws.path(),
        ".devcontainer/devcontainer.json",
        &format!(r#"{{ "name": "md-user", "image": "{tag}", "overrideCommand": true }}"#),
    );

    // Mirror the issue repro: no UID remapping so the container user stays appuser.
    let up_status = deacon()
        .args(["up", "--workspace-folder"])
        .arg(ws.path())
        .args([
            "--remove-existing-container",
            "--update-remote-user-uid-default",
            "off",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .expect("spawn deacon up");
    assert!(up_status.success(), "`deacon up` failed");

    let got = exec_cmd(ws.path(), &["sh", "-lc", "id -un"]);
    down(ws.path());
    let _ = StdCommand::new(runtime_bin())
        .args(["rmi", "-f", tag])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    assert_eq!(
        got, "appuser",
        "exec must run as the image-metadata remoteUser (#223), got {got:?}"
    );
}
