//! Integration tests for the workspace-trust gate on host-side lifecycle hooks.
//!
//! Coverage matrix (per issue #52 Task 2 acceptance criteria):
//! - `deacon up` on a workspace with `initializeCommand` and no trust → fails
//!   with the WorkspaceUntrusted error (and the host shell never runs).
//! - `--trust-workspace` → host shell runs, side effects appear.
//! - `DEACON_NO_PROMPT=1` without trust → fails (fail-closed).
//!
//! These tests do not require Docker: they assert behavior at the point where
//! the trust gate fires, BEFORE any container operations. The host-side
//! initializeCommand either runs (and leaves a marker on disk) or doesn't.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

fn write_devcontainer_with_init(workspace: &std::path::Path, init_cmd: &str) {
    let dc_dir = workspace.join(".devcontainer");
    fs::create_dir_all(&dc_dir).unwrap();
    let config = format!(
        r#"{{
  "name": "trust-gate-test",
  "image": "alpine:3.19",
  "workspaceFolder": "/workspace",
  "initializeCommand": "{}"
}}"#,
        init_cmd
    );
    fs::write(dc_dir.join("devcontainer.json"), config).unwrap();
}

/// Default policy (no flag, no DEACON_NO_PROMPT) refuses to run the host hook.
///
/// The marker file MUST NOT exist after the run; the run MUST fail with a
/// message naming the workspace and `--trust-workspace`.
// Unix-only: verifies host `initializeCommand` execution via a POSIX shell
// command (`echo … > marker`). Windows host-hook execution is a separate
// concern; the trust-policy resolution itself is covered by unit tests in
// `deacon_core::trust`.
#[cfg(unix)]
#[test]
fn untrusted_workspace_refuses_initialize_command() {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path();
    let marker = workspace.join("should-not-exist.txt");
    write_devcontainer_with_init(workspace, &format!("echo trusted > {}", marker.display()));

    // Point user_data_folder at a fresh dir so we don't read the host's
    // real allowlist (which might have any path in it on a developer's box).
    let user_data = tmp.path().join("user-data");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .arg("--user-data-folder")
        .arg(&user_data)
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "deacon up MUST fail on an untrusted workspace; stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !marker.exists(),
        "initializeCommand MUST NOT have run; marker exists at {}",
        marker.display()
    );

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        combined.contains("not trusted") || combined.contains("WorkspaceUntrusted"),
        "error output should mention trust refusal: {}",
        combined
    );
    assert!(
        combined.contains("--trust-workspace"),
        "error output should advertise --trust-workspace opt-in: {}",
        combined
    );
}

/// `--trust-workspace` (one-shot) allows the host-side hook to run.
///
/// The marker file MUST exist after the run, even when the downstream
/// container creation fails (we don't have Docker in this hermetic test).
// Unix-only: see `untrusted_workspace_refuses_initialize_command` — exercises
// host `initializeCommand` execution via a POSIX shell.
#[cfg(unix)]
#[test]
fn trust_workspace_flag_allows_initialize_command() {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path();
    let marker = workspace.join("ran.txt");
    write_devcontainer_with_init(workspace, &format!("echo trusted > {}", marker.display()));

    let user_data = tmp.path().join("user-data");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let _ = cmd
        .arg("--user-data-folder")
        .arg(&user_data)
        .arg("--trust-workspace")
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .output()
        .unwrap();

    assert!(
        marker.exists(),
        "initializeCommand should have run with --trust-workspace; marker missing at {}",
        marker.display()
    );
}

/// `DEACON_NO_PROMPT=1` without an explicit `--trust-workspace*` flag forces
/// Deny (fail-closed in CI).
#[test]
fn deacon_no_prompt_denies_without_explicit_trust() {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path();
    let marker = workspace.join("should-not-exist.txt");
    write_devcontainer_with_init(workspace, &format!("echo trusted > {}", marker.display()));

    let user_data = tmp.path().join("user-data");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .env("DEACON_NO_PROMPT", "1")
        .arg("--user-data-folder")
        .arg(&user_data)
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "DEACON_NO_PROMPT=1 MUST cause untrusted workspaces to fail closed"
    );
    assert!(
        !marker.exists(),
        "initializeCommand MUST NOT have run with DEACON_NO_PROMPT=1; marker exists at {}",
        marker.display()
    );
}

/// `--trust-workspace-persist` records the workspace into the trust store so
/// subsequent runs without any flag are also allowed.
// Unix-only: see `untrusted_workspace_refuses_initialize_command` — exercises
// host `initializeCommand` execution via a POSIX shell.
#[cfg(unix)]
#[test]
fn trust_workspace_persist_remembers_for_next_run() {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path().join("ws");
    fs::create_dir_all(&workspace).unwrap();
    let marker1 = workspace.join("first.txt");
    let marker2 = workspace.join("second.txt");

    // Two devcontainers can't coexist, so we rewrite for each run.
    let user_data = tmp.path().join("user-data");

    // Run 1: persist trust + verify marker1.
    write_devcontainer_with_init(&workspace, &format!("echo first > {}", marker1.display()));
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let _ = cmd
        .arg("--user-data-folder")
        .arg(&user_data)
        .arg("--trust-workspace-persist")
        .arg("up")
        .arg("--workspace-folder")
        .arg(&workspace)
        .output()
        .unwrap();
    assert!(marker1.exists(), "first run should have created marker1");

    // Run 2: NO flag, NO DEACON_NO_PROMPT — the store should have remembered.
    write_devcontainer_with_init(&workspace, &format!("echo second > {}", marker2.display()));
    let mut cmd2 = Command::cargo_bin("deacon").unwrap();
    let _ = cmd2
        .arg("--user-data-folder")
        .arg(&user_data)
        .arg("up")
        .arg("--workspace-folder")
        .arg(&workspace)
        .output()
        .unwrap();
    assert!(
        marker2.exists(),
        "second run (no flag) should still trust persisted workspace; marker missing"
    );

    // Sanity check the on-disk store.
    let store_path = user_data.join("trusted_workspaces.json");
    assert!(
        store_path.exists(),
        "trust store should exist at {}",
        store_path.display()
    );
}
