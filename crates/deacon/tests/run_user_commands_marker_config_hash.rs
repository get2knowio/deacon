//! Integration test for #372: `run-user-commands` must not erase `up`'s
//! config-drift detection.
//!
//! Lifecycle phase markers are a deacon extension (the reference CLI has none)
//! and they key on the WORKSPACE hash alone, so `up` and `run-user-commands`
//! write the same `<user-data>/state/<workspace-hash>/<phase>.json` files. `up`
//! stamps the resolved `config_hash` on each marker and drops markers whose hash
//! differs from the current config, which is what makes a later
//! `up --override-config <changed>` re-run `postCreate` on the fresh container it
//! creates.
//!
//! `run-user-commands` used to write those markers with `config_hash: null`, and
//! `read_all_markers_for_config` treats an absent hash as "legacy, compatible
//! with any config" — so a single `run-user-commands` invocation permanently
//! disarmed the drift check and `postCreate` was silently skipped.
//!
//! Requires Docker; self-skips when unavailable.
#![cfg(unix)]

use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const IMAGE: &str = "alpine:3.18";

fn is_docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Removes every container this test's workspace produced, even if an assertion
/// panics mid-test. Scoped to the temp workspace's own `devcontainer.local_folder`
/// label so it can never reap a sibling test's container.
struct WorkspaceContainers(String);

impl WorkspaceContainers {
    fn new(workspace: &Path) -> Self {
        // The label carries the path as named (absolutized, never canonicalized — #665).
        let local_folder = deacon_core::workspace::absolutize(workspace);
        Self(format!(
            "label=devcontainer.local_folder={}",
            local_folder.display()
        ))
    }
}

impl Drop for WorkspaceContainers {
    fn drop(&mut self) {
        let listed = std::process::Command::new("docker")
            .args(["ps", "-aq", "--filter", &self.0])
            .output();
        if let Ok(out) = listed {
            for id in String::from_utf8_lossy(&out.stdout).lines() {
                let id = id.trim();
                if id.is_empty() {
                    continue;
                }
                let _ = std::process::Command::new("docker")
                    .args(["rm", "-f", id])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        }
    }
}

/// The single `postCreate.json` marker under `--user-data-folder`. Markers live at
/// `<user-data>/state/<workspace-hash>/postCreate.json`; the hash is computed by
/// deacon, so the test discovers the file rather than recomputing it.
fn post_create_marker(user_data: &Path) -> Option<PathBuf> {
    let state = user_data.join("state");
    for entry in fs::read_dir(state).ok()?.flatten() {
        let candidate = entry.path().join("postCreate.json");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn marker_config_hash(user_data: &Path) -> Option<String> {
    let path = post_create_marker(user_data)?;
    let raw = fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    value
        .get("config_hash")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

#[test]
fn run_user_commands_preserves_the_config_hash_up_recorded() {
    if !is_docker_available() {
        eprintln!("Skipping: Docker is not available");
        return;
    }

    let temp = TempDir::new().expect("temp dir");
    let workspace = temp.path().join("ws");
    let user_data = temp.path().join("udf");
    fs::create_dir_all(workspace.join(".devcontainer")).expect("workspace dirs");
    fs::create_dir_all(&user_data).expect("user data dir");

    fs::write(
        workspace.join(".devcontainer").join("devcontainer.json"),
        format!(
            r#"{{
  "name": "issue372-base",
  "image": "{IMAGE}",
  "postCreateCommand": "echo base-post-create"
}}"#
        ),
    )
    .expect("write devcontainer.json");

    let changed = temp.path().join("changed.json");
    fs::write(
        &changed,
        format!(
            r#"{{
  "name": "issue372-changed",
  "image": "{IMAGE}",
  "postCreateCommand": "echo changed-post-create"
}}"#
        ),
    )
    .expect("write override config");

    // The label carries the WORKSPACE folder, not the temp root — filtering on the
    // parent matches nothing and leaks both containers.
    let _cleanup = WorkspaceContainers::new(&workspace);

    // Step 1: `up` records a hash-stamped postCreate marker.
    Command::cargo_bin("deacon")
        .expect("deacon binary")
        .args(["up", "--workspace-folder"])
        .arg(&workspace)
        .arg("--user-data-folder")
        .arg(&user_data)
        .args(["--mount-workspace-git-root", "false"])
        .timeout(std::time::Duration::from_secs(300))
        .assert()
        .success();

    let after_up = marker_config_hash(&user_data)
        .expect("up must write a postCreate marker carrying a config_hash");

    // Step 2: `run-user-commands` rewrites the SAME marker. Before #372 it wrote
    // `config_hash: null`, which reads as "compatible with every config".
    Command::cargo_bin("deacon")
        .expect("deacon binary")
        .args(["run-user-commands", "--workspace-folder"])
        .arg(&workspace)
        .arg("--user-data-folder")
        .arg(&user_data)
        .timeout(std::time::Duration::from_secs(300))
        .assert()
        .success();

    let after_run_user_commands = marker_config_hash(&user_data)
        .expect("run-user-commands must not erase the marker's config_hash (#372)");
    assert_eq!(
        after_up, after_run_user_commands,
        "run-user-commands must stamp the same config hash `up` does (#372)"
    );

    // Step 3: the acceptance criterion — a changed config after a
    // `run-user-commands` still re-runs postCreate. `up` builds a fresh container
    // (different config hash → different identity) and the marker is rewritten
    // with the NEW hash; a skipped postCreate leaves the old hash in place.
    Command::cargo_bin("deacon")
        .expect("deacon binary")
        .args(["up", "--workspace-folder"])
        .arg(&workspace)
        .arg("--user-data-folder")
        .arg(&user_data)
        .args(["--mount-workspace-git-root", "false"])
        .arg("--override-config")
        .arg(&changed)
        .timeout(std::time::Duration::from_secs(300))
        .assert()
        .success();

    let after_drift =
        marker_config_hash(&user_data).expect("the drifted up must rewrite the postCreate marker");
    assert_ne!(
        after_run_user_commands, after_drift,
        "postCreate was skipped on a changed config: run-user-commands erased up's drift detection (#372)"
    );
}
