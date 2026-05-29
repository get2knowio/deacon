//! Regression test: the container keep-alive command must not depend on the
//! image's `PATH`.
//!
//! `up` keeps the container alive with `/bin/sh -c "sleep infinity || tail -f
//! /dev/null"`. Some features (e.g. `node` via nvm) replace the image `PATH`
//! with their own bin dir, dropping `/usr/bin`+`/bin`; the bare `sleep`/`tail`
//! then resolve to "not found" (exit 127) and the container dies before any
//! lifecycle command runs. The keep-alive now prepends a standard `PATH`.
//!
//! This reproduces the class deterministically (no features/network) by
//! clobbering `PATH` via `containerEnv`. Requires Docker; self-skips otherwise.
#![cfg(unix)]

use assert_cmd::Command;
use std::fs;
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

struct ContainerGuard(String);
impl Drop for ContainerGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", &self.0])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

#[test]
fn test_keepalive_survives_path_clobbering_container_env() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_keepalive_survives_path_clobbering_container_env: Docker not available"
        );
        return;
    }

    let name = "Deacon Keepalive PATH Test";
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join(".devcontainer")).unwrap();
    fs::write(
        root.join(".devcontainer/devcontainer.json"),
        format!(
            r#"{{
  "name": "{name}",
  "image": "debian:bookworm-slim",
  "remoteUser": "root",
  "workspaceFolder": "/workspace",
  "containerEnv": {{ "PATH": "/nonexistent/bin" }}
}}"#
        ),
    )
    .unwrap();

    let output = Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(root)
        .arg("up")
        .arg("--workspace-folder")
        .arg(root)
        .arg("--remove-existing-container")
        .arg("--skip-post-create")
        .env("DEACON_LOG", "warn")
        .output()
        .expect("spawn deacon up");

    // Discover + guard the container regardless of outcome.
    let cid = String::from_utf8_lossy(
        &std::process::Command::new("docker")
            .args([
                "ps",
                "-a",
                "--filter",
                &format!("label=devcontainer.name={}", name),
                "--format",
                "{{.ID}}",
            ])
            .output()
            .map(|o| o.stdout)
            .unwrap_or_default(),
    )
    .lines()
    .next()
    .unwrap_or("")
    .trim()
    .to_string();
    let _guard = if cid.is_empty() {
        None
    } else {
        Some(ContainerGuard(cid.clone()))
    };

    assert!(
        output.status.success(),
        "up failed with a PATH-clobbering containerEnv: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!cid.is_empty(), "container should have been created");

    // The keep-alive must have survived: the container is still running.
    let state = String::from_utf8_lossy(
        &std::process::Command::new("docker")
            .args(["inspect", "-f", "{{.State.Status}}", &cid])
            .output()
            .map(|o| o.stdout)
            .unwrap_or_default(),
    )
    .trim()
    .to_string();
    assert_eq!(
        state, "running",
        "container should stay running despite a clobbered PATH (keep-alive must not depend on PATH)"
    );
}
