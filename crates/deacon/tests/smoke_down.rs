//! Smoke tests for down command behavior
//!
//! Scenarios covered:
//! - Down command before any up: should succeed or gracefully handle "no container"
//! - Down command after up: should successfully tear down (Docker-gated)
//! - Idempotent down behavior: subsequent down calls should not error
//!
//! NOTE: These tests assume Docker is available and running. They will fail
//! if Docker is not present or cannot start containers.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

/// The container runtime binary under test (honors `DEACON_CONTAINER_RUNTIME`,
/// the same env var deacon reads). Stale-container setup must use this so it
/// lands in the store deacon-under-podman actually sweeps.
fn runtime_bin() -> String {
    std::env::var("DEACON_CONTAINER_RUNTIME").unwrap_or_else(|_| "docker".to_string())
}

fn is_docker_available() -> bool {
    std::process::Command::new(runtime_bin())
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Test down command before any up: should succeed or gracefully handle "no container"
#[test]
fn test_down_before_up() {
    let temp_dir = TempDir::new().unwrap();

    // Create minimal devcontainer.json
    let devcontainer_config = r#"{
    "name": "Down Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test down command before any up
    let mut down_cmd = Command::cargo_bin("deacon").unwrap();
    let down_output = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let down_stderr = String::from_utf8_lossy(&down_output.stderr);
    // Expected: should succeed when no container to tear down
    // If CLI chooses to report "no container" as non-zero, allow known message
    if !down_output.status.success() {
        assert!(
            down_stderr.contains("No running containers")
                || down_stderr.contains("no container")
                || down_stderr.contains("not found"),
            "Down before up failed unexpectedly: {}",
            down_stderr
        );
    }
}

/// Test down command after up and idempotent behavior (Docker-gated)
#[test]
fn test_down_after_up_idempotent() {
    if !is_docker_available() {
        eprintln!("Skipping test_down_after_up_idempotent: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create minimal devcontainer.json
    let devcontainer_config = r#"{
    "name": "Down After Up Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // First: up command
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--skip-post-create")
        .arg("--skip-non-blocking-commands")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    assert!(
        up_output.status.success(),
        "Up command failed: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );

    // Second: down command (should succeed)
    let mut down_cmd = Command::cargo_bin("deacon").unwrap();
    let down_output = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let down_stderr = String::from_utf8_lossy(&down_output.stderr);

    assert!(
        down_output.status.success(),
        "Down command failed after up: {}",
        down_stderr
    );

    println!("Down after up succeeded");

    // Third: down command again (should be idempotent, not error)
    let mut down_cmd2 = Command::cargo_bin("deacon").unwrap();
    let down_output2 = down_cmd2
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let down_stderr2 = String::from_utf8_lossy(&down_output2.stderr);

    if !down_output2.status.success() {
        assert!(
            down_stderr2.contains("No running containers")
                || down_stderr2.contains("no container")
                || down_stderr2.contains("not found"),
            "Unexpected error in second down: {}",
            down_stderr2
        );
    }
}

/// Test `down --all` sweeps *every* container carrying this workspace's
/// `devcontainer.local_folder` label — including a stale container that was
/// NOT created by deacon (no `source`/hash labels) and whose config never
/// matched. Regression test for `--all` over-pinning on `config_hash`.
#[test]
fn test_down_all_sweeps_stale_by_local_folder() {
    if !is_docker_available() {
        eprintln!("Skipping test_down_all_sweeps_stale_by_local_folder: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();
    // Canonical workspace path — matches what deacon writes to the
    // devcontainer.local_folder label and what we filter on below.
    let workspace = temp_dir.path().canonicalize().unwrap();

    let devcontainer_config = r#"{
    "name": "Down All Stale Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace"
}"#;
    fs::create_dir(workspace.join(".devcontainer")).unwrap();
    fs::write(
        workspace.join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Bring up the deacon-managed container.
    let up_output = Command::cargo_bin("deacon")
        .unwrap()
        .arg("up")
        .arg("--skip-post-create")
        .arg("--skip-non-blocking-commands")
        .arg("--workspace-folder")
        .arg(&workspace)
        .output()
        .unwrap();
    assert!(
        up_output.status.success(),
        "Up failed: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );

    // Create a *stale* container that only carries the workspace's
    // local_folder label (simulating a container from an older deacon run
    // whose state file / config no longer matches).
    let local_folder_label = format!("devcontainer.local_folder={}", workspace.display());
    let stale = std::process::Command::new(runtime_bin())
        .args([
            "run",
            "-d",
            "--rm",
            "--label",
            &local_folder_label,
            "alpine:3.19",
            "sleep",
            "300",
        ])
        .output()
        .unwrap();
    assert!(
        stale.status.success(),
        "Failed to create stale container: {}",
        String::from_utf8_lossy(&stale.stderr)
    );
    let stale_id = String::from_utf8_lossy(&stale.stdout).trim().to_string();

    // Count containers with this workspace label before sweep (expect >= 2).
    let count_label = || -> usize {
        let out = std::process::Command::new(runtime_bin())
            .args([
                "ps",
                "-a",
                "--filter",
                &format!("label={}", local_folder_label),
                "-q",
            ])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count()
    };
    assert!(
        count_label() >= 2,
        "expected at least the deacon container + stale container before sweep"
    );

    // Sweep everything with --all --remove.
    let down_output = Command::cargo_bin("deacon")
        .unwrap()
        .arg("down")
        .arg("--workspace-folder")
        .arg(&workspace)
        .arg("--all")
        .arg("--remove")
        .arg("--force")
        .output()
        .unwrap();

    let remaining = count_label();
    // Best-effort cleanup of the stale container regardless of assertion outcome.
    let _ = std::process::Command::new(runtime_bin())
        .args(["rm", "-f", &stale_id])
        .output();

    assert!(
        down_output.status.success(),
        "down --all failed: {}",
        String::from_utf8_lossy(&down_output.stderr)
    );
    assert_eq!(
        remaining, 0,
        "down --all --remove must sweep ALL containers labeled for this workspace (including the stale, non-deacon one); {remaining} remained"
    );
}
