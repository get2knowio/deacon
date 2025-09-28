//! Parity tests comparing deacon vs upstream devcontainer CLI for `up` and `exec`.
//!
//! Assumes Docker is available and `devcontainer` is installed. No gating.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

mod parity_utils;

fn upstream_bin() -> String {
    std::env::var("DEACON_PARITY_DEVCONTAINER").unwrap_or_else(|_| "devcontainer".to_string())
}

#[test]
fn parity_up_and_exec_traditional() {
    if !parity_utils::upstream_available() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }
    // workspace with alpine image and a postCreate marker
    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();

    fs::create_dir(ws.join(".devcontainer")).unwrap();
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        r#"{
  "name": "ParityUpExec",
  "image": "alpine:3.19",
  "workspaceFolder": "/workspaces/${localWorkspaceFolderBasename}",
  "postCreateCommand": "sh -lc 'echo ready > /tmp/parity_marker'"
}
"#,
    )
    .unwrap();

    // upstream: up + exec cat marker
    let mut up1 = std::process::Command::new(upstream_bin());
    up1.current_dir(ws);
    up1.arg("up");
    up1.arg("--workspace-folder");
    up1.arg(ws);
    let st1 = up1.output().unwrap();
    assert!(
        st1.status.success(),
        "upstream up failed: {}",
        String::from_utf8_lossy(&st1.stderr)
    );

    let mut ex1 = std::process::Command::new(upstream_bin());
    ex1.current_dir(ws);
    ex1.arg("exec");
    ex1.arg("--workspace-folder");
    ex1.arg(ws);
    ex1.arg("sh");
    ex1.arg("-lc");
    ex1.arg("cat /tmp/parity_marker && pwd");
    let e1 = ex1.output().unwrap();
    assert!(
        e1.status.success(),
        "upstream exec failed: {}",
        String::from_utf8_lossy(&e1.stderr)
    );
    let out1 = String::from_utf8_lossy(&e1.stdout);
    assert!(out1.contains("ready"), "upstream marker missing: {}", out1);

    // ours: up + exec cat marker
    let mut up2 = Command::cargo_bin("deacon").unwrap();
    let st2 = up2
        .current_dir(ws)
        .arg("up")
        .arg("--workspace-folder")
        .arg(ws)
        .assert()
        .get_output()
        .to_owned();
    assert!(
        st2.status.success(),
        "deacon up failed: {}",
        String::from_utf8_lossy(&st2.stderr)
    );

    let mut ex2 = Command::cargo_bin("deacon").unwrap();
    let e2 = ex2
        .current_dir(ws)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(ws)
        .arg("--")
        .arg("sh")
        .arg("-lc")
        .arg("cat /tmp/parity_marker && pwd")
        .assert()
        .get_output()
        .to_owned();
    assert!(
        e2.status.success(),
        "deacon exec failed: {}",
        String::from_utf8_lossy(&e2.stderr)
    );
    let out2 = String::from_utf8_lossy(&e2.stdout);
    assert!(out2.contains("ready"), "deacon marker missing: {}", out2);
}
