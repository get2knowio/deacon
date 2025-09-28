//! Parity tests comparing deacon vs upstream devcontainer CLI for `exec` semantics.
//!
//! These tests verify that deacon's exec command behaves functionally equivalent to
//! the upstream devcontainer CLI in terms of working directory, user, TTY, and
//! environment variable handling.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

mod parity_utils;

fn upstream_bin() -> String {
    std::env::var("DEACON_PARITY_DEVCONTAINER").unwrap_or_else(|_| "devcontainer".to_string())
}

/// Test working directory parity with explicit workspaceFolder
#[test]
fn parity_exec_working_directory() {
    if !parity_utils::upstream_available() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();

    fs::create_dir(ws.join(".devcontainer")).unwrap();
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        r#"{
  "name": "ParityExecWorkingDir",
  "image": "alpine:3.19",
  "workspaceFolder": "/wsp"
}
"#,
    )
    .unwrap();

    // upstream: up then exec pwd
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
    ex1.arg("pwd");
    let e1 = ex1.output().unwrap();
    assert!(
        e1.status.success(),
        "upstream exec failed: {}",
        String::from_utf8_lossy(&e1.stderr)
    );
    let out1 = String::from_utf8_lossy(&e1.stdout).trim().to_string();

    // deacon: up then exec pwd
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
        .arg("pwd")
        .assert()
        .get_output()
        .to_owned();
    assert!(
        e2.status.success(),
        "deacon exec failed: {}",
        String::from_utf8_lossy(&e2.stderr)
    );
    let out2 = String::from_utf8_lossy(&e2.stdout).trim().to_string();

    // Both should print /wsp as the working directory
    assert_eq!(out1, "/wsp", "upstream should show /wsp as pwd");
    assert_eq!(out2, "/wsp", "deacon should show /wsp as pwd");
    assert_eq!(
        out1, out2,
        "working directory mismatch: upstream={}, deacon={}",
        out1, out2
    );
}

/// Test exec user parity with --user flag
#[test]
fn parity_exec_user() {
    if !parity_utils::upstream_available() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();

    fs::create_dir(ws.join(".devcontainer")).unwrap();
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        r#"{
  "name": "ParityExecUser",
  "image": "alpine:3.19"
}
"#,
    )
    .unwrap();

    // upstream: up then exec with --user root
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
    ex1.arg("--user");
    ex1.arg("root");
    ex1.arg("sh");
    ex1.arg("-lc");
    ex1.arg("id -u");
    let e1 = ex1.output().unwrap();
    assert!(
        e1.status.success(),
        "upstream exec failed: {}",
        String::from_utf8_lossy(&e1.stderr)
    );
    let out1 = String::from_utf8_lossy(&e1.stdout).trim().to_string();

    // deacon: up then exec with --user root
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
        .arg("--user")
        .arg("root")
        .arg("--")
        .arg("sh")
        .arg("-lc")
        .arg("id -u")
        .assert()
        .get_output()
        .to_owned();
    assert!(
        e2.status.success(),
        "deacon exec failed: {}",
        String::from_utf8_lossy(&e2.stderr)
    );
    let out2 = String::from_utf8_lossy(&e2.stdout).trim().to_string();

    // Both should show UID 0 (root)
    assert_eq!(out1, "0", "upstream should show UID 0 for root");
    assert_eq!(out2, "0", "deacon should show UID 0 for root");
    assert_eq!(
        out1, out2,
        "user ID mismatch: upstream={}, deacon={}",
        out1, out2
    );
}

/// Test exec TTY parity with --no-tty flag
#[test]
fn parity_exec_tty() {
    if !parity_utils::upstream_available() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();

    fs::create_dir(ws.join(".devcontainer")).unwrap();
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        r#"{
  "name": "ParityExecTTY",
  "image": "alpine:3.19"
}
"#,
    )
    .unwrap();

    // upstream: up then exec with --no-tty
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
    ex1.arg("--no-tty");
    ex1.arg("sh");
    ex1.arg("-lc");
    ex1.arg("test -t 1 && echo TTY || echo NOTTY");
    let e1 = ex1.output().unwrap();
    assert!(
        e1.status.success(),
        "upstream exec failed: {}",
        String::from_utf8_lossy(&e1.stderr)
    );
    let out1 = String::from_utf8_lossy(&e1.stdout).trim().to_string();

    // deacon: up then exec with --no-tty
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
        .arg("--no-tty")
        .arg("--")
        .arg("sh")
        .arg("-lc")
        .arg("test -t 1 && echo TTY || echo NOTTY")
        .assert()
        .get_output()
        .to_owned();
    assert!(
        e2.status.success(),
        "deacon exec failed: {}",
        String::from_utf8_lossy(&e2.stderr)
    );
    let out2 = String::from_utf8_lossy(&e2.stdout).trim().to_string();

    // Both should show NOTTY with --no-tty flag
    assert_eq!(out1, "NOTTY", "upstream should show NOTTY with --no-tty");
    assert_eq!(out2, "NOTTY", "deacon should show NOTTY with --no-tty");
    assert_eq!(
        out1, out2,
        "TTY behavior mismatch: upstream={}, deacon={}",
        out1, out2
    );
}

/// Test exec environment variable propagation with --env flag
#[test]
fn parity_exec_env_propagation() {
    if !parity_utils::upstream_available() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();

    fs::create_dir(ws.join(".devcontainer")).unwrap();
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        r#"{
  "name": "ParityExecEnv",
  "image": "alpine:3.19"
}
"#,
    )
    .unwrap();

    // upstream: up then exec with --env
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
    ex1.arg("--env");
    ex1.arg("FOO=BAR");
    ex1.arg("sh");
    ex1.arg("-lc");
    ex1.arg("echo $FOO");
    let e1 = ex1.output().unwrap();
    assert!(
        e1.status.success(),
        "upstream exec failed: {}",
        String::from_utf8_lossy(&e1.stderr)
    );
    let out1 = String::from_utf8_lossy(&e1.stdout).trim().to_string();

    // deacon: up then exec with --env
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
        .arg("--env")
        .arg("FOO=BAR")
        .arg("--")
        .arg("sh")
        .arg("-lc")
        .arg("echo $FOO")
        .assert()
        .get_output()
        .to_owned();
    assert!(
        e2.status.success(),
        "deacon exec failed: {}",
        String::from_utf8_lossy(&e2.stderr)
    );
    let out2 = String::from_utf8_lossy(&e2.stdout).trim().to_string();

    // Both should show BAR
    assert_eq!(out1, "BAR", "upstream should show BAR for FOO env var");
    assert_eq!(out2, "BAR", "deacon should show BAR for FOO env var");
    assert_eq!(
        out1, out2,
        "env propagation mismatch: upstream={}, deacon={}",
        out1, out2
    );
}
