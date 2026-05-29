//! Regression test: `up` reports the FULL container ID consistently on both
//! the initial create and on reconnect/reuse.
//!
//! `docker ps` emits the short 12-char ID by default, so the reconnect path
//! (which resolves the existing container via `list_containers`) used to report
//! a short `containerId` while the initial create reported the full 64-char ID.
//! `list_containers` now passes `--no-trunc`, so both are the full ID.
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

/// Parse `containerId` out of `deacon up`'s stdout (a single pretty-printed
/// JSON document).
fn container_id_from_stdout(stdout: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(stdout).ok()?;
    v.get("containerId")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

#[test]
fn test_up_reports_full_container_id_on_reconnect() {
    if !is_docker_available() {
        eprintln!("Skipping test_up_reports_full_container_id_on_reconnect: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join(".devcontainer")).unwrap();
    fs::write(
        root.join(".devcontainer/devcontainer.json"),
        r#"{
  "name": "Full ID Reconnect Test",
  "image": "alpine:3.18",
  "remoteUser": "root",
  "workspaceFolder": "/workspace"
}"#,
    )
    .unwrap();

    let run_up = || {
        Command::cargo_bin("deacon")
            .unwrap()
            .current_dir(root)
            .arg("up")
            .arg("--workspace-folder")
            .arg(root)
            .env("DEACON_LOG", "warn")
            .output()
            .expect("spawn deacon up")
    };

    // First up: create.
    let out1 = run_up();
    assert!(
        out1.status.success(),
        "first up failed: {}",
        String::from_utf8_lossy(&out1.stderr)
    );
    let id1 =
        container_id_from_stdout(&out1.stdout).expect("first up should emit containerId on stdout");
    let _guard = ContainerGuard(id1.clone());

    // Second up: reconnect/reuse.
    let out2 = run_up();
    assert!(
        out2.status.success(),
        "second up failed: {}",
        String::from_utf8_lossy(&out2.stderr)
    );
    let id2 = container_id_from_stdout(&out2.stdout)
        .expect("second up should emit containerId on stdout");

    // Both must be the same full 64-char container ID.
    assert_eq!(
        id1.len(),
        64,
        "create containerId should be the full 64-char id, got {} ({} chars)",
        id1,
        id1.len()
    );
    assert_eq!(
        id1, id2,
        "reconnect must report the same full containerId as create (id1={id1}, id2={id2})"
    );
}
