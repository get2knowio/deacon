//! Container identity for a workspace reached through a symlink ([#665]).
//!
//! deacon canonicalized `--workspace-folder`, silently renaming such a workspace to its
//! real path. The reference preserves the spelling the caller used — deliberately, via
//! `git rev-parse --show-cdup` rather than `--show-toplevel` (`spec-common/git.ts:24`) —
//! and the spec defines `${localWorkspaceFolder}` as the folder *that was opened*
//! (`devcontainerjson-reference.md:157`).
//!
//! The reported half is pinned hermetically in `integration_read_configuration`. What
//! needs a container is the IDENTITY half, which is what the ruling on #665 actually
//! decided: the `devcontainer.local_folder` label carries the path as given, so `up`,
//! `exec` and `down` agree on one spelling and two spellings are two containers — exactly
//! as the reference behaves (measured at oracle 0.87.0, whose `exec` against the other
//! spelling fails to find the container too).
//!
//! Requires Docker; self-skips when unavailable.
//!
//! [#665]: https://github.com/get2knowio/deacon/issues/665
#![cfg(unix)]

use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const BASE_IMAGE: &str = "alpine:3.19";

fn is_docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Removes the container `up` created, whatever the test did afterwards.
struct WorkspaceContainer(PathBuf);

impl Drop for WorkspaceContainer {
    fn drop(&mut self) {
        let _ = Command::cargo_bin("deacon").map(|mut cmd| {
            cmd.arg("down")
                .arg("--workspace-folder")
                .arg(&self.0)
                .arg("--remove-volumes")
                .output()
        });
    }
}

/// A real workspace directory plus a symlink pointing at it.
fn linked_workspace(temp: &Path) -> (PathBuf, PathBuf) {
    let real = temp.join("real");
    fs::create_dir_all(real.join(".devcontainer")).unwrap();
    fs::write(
        real.join(".devcontainer").join("devcontainer.json"),
        format!(r#"{{"image": "{BASE_IMAGE}"}}"#),
    )
    .unwrap();
    let link = temp.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    (real, link)
}

fn up(workspace: &Path) -> serde_json::Value {
    let output = Command::cargo_bin("deacon")
        .unwrap()
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--remove-existing-container")
        .timeout(std::time::Duration::from_secs(300))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "up failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("up did not print JSON ({e}): {stdout}"))
}

fn exec_pwd(workspace: &Path) -> std::process::Output {
    Command::cargo_bin("deacon")
        .unwrap()
        .arg("exec")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--")
        .arg("pwd")
        .timeout(std::time::Duration::from_secs(120))
        .output()
        .unwrap()
}

fn label_of(container: &str, label: &str) -> String {
    let output = std::process::Command::new("docker")
        .args([
            "inspect",
            container,
            "--format",
            &format!("{{{{index .Config.Labels \"{label}\"}}}}"),
        ])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The label, the mount and the reported workspace folder all carry the link's spelling —
/// and `exec` through that same spelling reconnects, which is the symmetry the ruling
/// required be preserved.
#[test]
fn up_records_and_reconnects_by_the_path_that_was_named() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let temp = TempDir::new().unwrap();
    let (_real, link) = linked_workspace(temp.path());
    let _guard = WorkspaceContainer(link.clone());

    let result = up(&link);
    assert_eq!(result["remoteWorkspaceFolder"], "/workspaces/link");

    let container = result["containerId"].as_str().unwrap();
    assert_eq!(
        label_of(container, "devcontainer.local_folder"),
        link.display().to_string(),
        "the identity label carries the path as named"
    );

    let mounts = std::process::Command::new("docker")
        .args([
            "inspect",
            container,
            "--format",
            "{{range .Mounts}}{{.Source}} -> {{.Destination}}\n{{end}}",
        ])
        .output()
        .unwrap();
    let mounts = String::from_utf8_lossy(&mounts.stdout).into_owned();
    assert!(
        mounts.contains(&format!("{} -> /workspaces/link", link.display())),
        "the bind mount source is the link, not its target: {mounts}"
    );

    let probe = exec_pwd(&link);
    assert!(
        probe.status.success(),
        "exec through the same spelling must reconnect: {}",
        String::from_utf8_lossy(&probe.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&probe.stdout).trim(),
        "/workspaces/link"
    );
}

/// The other half of the ruling, and the control arm: the real path is a DIFFERENT
/// workspace, so it does not find the container the link's spelling created. Measured at
/// oracle 0.87.0, whose `exec` fails on the same pair. Without this arm, going back to
/// canonicalizing everything would still pass the test above.
#[test]
fn the_other_spelling_is_a_different_workspace() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let temp = TempDir::new().unwrap();
    let (real, link) = linked_workspace(temp.path());
    let _guard = WorkspaceContainer(link.clone());

    up(&link);

    let probe = exec_pwd(&real);
    assert!(
        !probe.status.success(),
        "the real path must not resolve the link's container: {}",
        String::from_utf8_lossy(&probe.stdout)
    );
    let stderr = String::from_utf8_lossy(&probe.stderr);
    assert!(
        stderr.contains("No running container found"),
        "expected a not-found diagnostic naming the workspace, got: {stderr}"
    );
    assert!(
        stderr.contains(&real.display().to_string()),
        "the diagnostic names the workspace as the caller spelled it: {stderr}"
    );
}
