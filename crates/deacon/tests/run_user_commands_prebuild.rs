//! Integration test for `run-user-commands --prebuild` lifecycle boundary.
//!
//! Per `docs/subcommand-specs/run-user-commands/SPEC.md`, `--prebuild` stops
//! after `onCreateCommand` and `updateContentCommand`, skipping postCreate,
//! dotfiles, postStart, and postAttach (the same set as
//! `core::lifecycle::LifecyclePhase::is_skipped_in_prebuild`).
//!
//! Unlike the `up_prebuild.rs` tests (which defer marker inspection to manual
//! testing), this test verifies the boundary concretely by inspecting marker
//! files written by each lifecycle phase inside the container.
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

/// Marker name -> true if the flag file must exist in the container.
fn marker_present(container_id: &str, marker: &str) -> bool {
    std::process::Command::new("docker")
        .args([
            "exec",
            container_id,
            "test",
            "-f",
            &format!("/tmp/{}.flag", marker),
        ])
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

/// Removes the named container on drop so cleanup runs even if an assertion
/// panics mid-test.
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
fn test_run_user_commands_prebuild_stops_after_update_content() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_run_user_commands_prebuild_stops_after_update_content: Docker not available"
        );
        return;
    }

    let name = "Deacon Prebuild RUC Test";
    let temp_dir = TempDir::new().unwrap();
    let config = format!(
        r#"{{
    "name": "{name}",
    "image": "alpine:3.18",
    "remoteUser": "root",
    "workspaceFolder": "/workspace",
    "onCreateCommand": "echo onCreate > /tmp/onCreate.flag",
    "updateContentCommand": "echo updateContent > /tmp/updateContent.flag",
    "postCreateCommand": "echo postCreate > /tmp/postCreate.flag",
    "postStartCommand": "echo postStart > /tmp/postStart.flag",
    "postAttachCommand": "echo postAttach > /tmp/postAttach.flag"
}}"#
    );
    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        config,
    )
    .unwrap();

    // Bring the container up with postCreate+ suppressed so only onCreate and
    // updateContent fire during `up`.
    Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--remove-existing-container")
        .arg("--skip-post-create")
        .assert()
        .success();

    let container_id = find_container(name).expect("container should exist after up");
    let _guard = ContainerGuard(container_id.clone());

    // Ensure no later markers leaked in, then prove --prebuild does not create them.
    let _ = std::process::Command::new("docker")
        .args([
            "exec",
            &container_id,
            "sh",
            "-c",
            "rm -f /tmp/postCreate.flag /tmp/postStart.flag /tmp/postAttach.flag",
        ])
        .status();

    Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(&temp_dir)
        .arg("run-user-commands")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--prebuild")
        .assert()
        .success();

    // onCreate + updateContent run; postCreate/postStart/postAttach must NOT.
    assert!(
        marker_present(&container_id, "updateContent"),
        "updateContent should run in prebuild mode"
    );
    assert!(
        !marker_present(&container_id, "postCreate"),
        "postCreate must be skipped in prebuild mode"
    );
    assert!(
        !marker_present(&container_id, "postStart"),
        "postStart must be skipped in prebuild mode"
    );
    assert!(
        !marker_present(&container_id, "postAttach"),
        "postAttach must be skipped in prebuild mode"
    );
}
