//! Integration test for `run-user-commands` aggregating feature-contributed
//! lifecycle commands (T031, issue #137).
//!
//! Before T031, `run-user-commands` only executed the config's own lifecycle
//! commands. Now it resolves the declared features and aggregates their
//! lifecycle hooks too (feature commands in install order, then the config's),
//! matching `up`.
//!
//! This test installs a local feature that declares a `postCreateCommand`, runs
//! `up --skip-post-create` (so no postCreate fires yet), then runs
//! `run-user-commands` and asserts BOTH the feature's and the config's
//! postCreate markers appear.
//!
//! Requires Docker; self-skips when unavailable.
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

fn marker_present(container_id: &str, marker: &str) -> bool {
    std::process::Command::new("docker")
        .args(["exec", container_id, "test", "-f", marker])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn find_container(name: &str) -> Option<String> {
    let output = std::process::Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            "label=devcontainer.source=deacon",
            "--filter",
            &format!("label=devcontainer.name={}", name),
            "--format",
            "{{.ID}}",
        ])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
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
fn test_run_user_commands_runs_feature_lifecycle_commands() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_run_user_commands_runs_feature_lifecycle_commands: Docker not available"
        );
        return;
    }

    let name = "Deacon RUC Feature Lifecycle Test";
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Local feature that declares a postCreateCommand (and a trivial install).
    let feat = root.join(".devcontainer/features/lc");
    fs::create_dir_all(&feat).unwrap();
    fs::write(
        feat.join("devcontainer-feature.json"),
        r#"{
  "id": "lc",
  "version": "1.0.0",
  "name": "Lifecycle Feature",
  "postCreateCommand": "echo feat > /tmp/feat-postcreate.flag"
}"#,
    )
    .unwrap();
    fs::write(
        feat.join("install.sh"),
        "#!/usr/bin/env bash\nset -e\ntrue\n",
    )
    .unwrap();

    fs::write(
        root.join(".devcontainer/devcontainer.json"),
        format!(
            r#"{{
  "name": "{name}",
  "image": "debian:bookworm-slim",
  "remoteUser": "root",
  "workspaceFolder": "/workspace",
  "workspaceMount": "source=${{localWorkspaceFolder}},target=/workspace,type=bind",
  "features": {{ "./features/lc": {{}} }},
  "postCreateCommand": "echo cfg > /tmp/cfg-postcreate.flag"
}}"#
        ),
    )
    .unwrap();

    // Bring the container up with postCreate suppressed, so neither the
    // feature's nor the config's postCreate has fired yet.
    Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(root)
        .arg("up")
        .arg("--workspace-folder")
        .arg(root)
        .arg("--remove-existing-container")
        .arg("--skip-post-create")
        .assert()
        .success();

    let container_id = find_container(name).expect("container should exist after up");
    let _guard = ContainerGuard(container_id.clone());

    assert!(
        !marker_present(&container_id, "/tmp/feat-postcreate.flag"),
        "feature postCreate must not have run yet (up --skip-post-create)"
    );

    // Drive the remaining phases; postCreate should now run for BOTH the
    // feature and the config.
    Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(root)
        .arg("run-user-commands")
        .arg("--workspace-folder")
        .arg(root)
        .assert()
        .success();

    assert!(
        marker_present(&container_id, "/tmp/feat-postcreate.flag"),
        "run-user-commands must execute the FEATURE's postCreateCommand (T031)"
    );
    assert!(
        marker_present(&container_id, "/tmp/cfg-postcreate.flag"),
        "run-user-commands must still execute the config's postCreateCommand"
    );
}
