//! `up` mount shapes that a comma or a git worktree makes non-obvious.
//!
//! Both are cases where the *reported* mount string and the mount Docker actually accepts
//! come apart, so a `read-configuration` assertion alone would not have caught them.
//! Mined from the reference CLI's own `src/test/workspaceConfiguration.test.ts` at v0.87.0
//! (#480, batch 14) and measured against that oracle before being written down.
//!
//! - [#663](https://github.com/get2knowio/deacon/issues/663): a `--mount` argument is CSV,
//!   so a comma in the workspace path has to be quoted. deacon quoted nowhere, and
//!   `docker create` rejected the argument outright — `up` could not start a devcontainer
//!   in such a workspace at all, while the reference could.
//! - [#664](https://github.com/get2knowio/deacon/issues/664):
//!   `--mount-git-worktree-common-dir` did not exist. Without it a devcontainer opened on a
//!   git worktree has a `.git` file pointing at a common dir that is not mounted, so every
//!   git command inside the container fails.
//!
//! Requires Docker; self-skips when unavailable.
#![cfg(unix)]

use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Pinned, tiny, and present on every lane that runs this file.
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

/// Removes the container `up` created for a workspace, whatever the test did afterwards.
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

fn write_config(workspace: &Path) {
    fs::create_dir_all(workspace.join(".devcontainer")).unwrap();
    fs::write(
        workspace.join(".devcontainer").join("devcontainer.json"),
        format!(r#"{{"image": "{BASE_IMAGE}"}}"#),
    )
    .unwrap();
}

/// Lay out `<root>/<repo>/.git/worktrees/<name>` and a worktree whose `.git` **file** holds
/// `gitdir`, plus the common dir's own `worktrees/<name>` directory so the resolution has
/// something real to land on inside the container.
///
/// Written by hand rather than with `git worktree add --relative-paths`, which needs git
/// 2.48 — no CI runner is guaranteed to have it, and deacon never shells out to git here.
fn write_worktree(root: &Path, repo: &str, worktree: &str, gitdir: &str) -> PathBuf {
    fs::create_dir_all(
        root.join(repo)
            .join(".git")
            .join("worktrees")
            .join(Path::new(worktree).file_name().unwrap()),
    )
    .unwrap();
    let worktree_path = root.join(worktree);
    fs::create_dir_all(&worktree_path).unwrap();
    fs::write(worktree_path.join(".git"), format!("gitdir: {gitdir}\n")).unwrap();
    write_config(&worktree_path);
    worktree_path
}

fn up(workspace: &Path, extra: &[&str]) -> std::process::Output {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--remove-existing-container");
    for arg in extra {
        cmd.arg(arg);
    }
    cmd.timeout(std::time::Duration::from_secs(300))
        .output()
        .unwrap()
}

fn exec(workspace: &Path, extra: &[&str], script: &str) -> std::process::Output {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("exec").arg("--workspace-folder").arg(workspace);
    for arg in extra {
        cmd.arg(arg);
    }
    cmd.arg("sh")
        .arg("-c")
        .arg(script)
        .timeout(std::time::Duration::from_secs(120))
        .output()
        .unwrap()
}

fn container_id(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("up did not print JSON ({e}): {stdout}"));
    parsed["containerId"]
        .as_str()
        .unwrap_or_else(|| panic!("up printed no containerId: {stdout}"))
        .to_string()
}

fn mounts_of(container: &str) -> String {
    let output = std::process::Command::new("docker")
        .args([
            "inspect",
            container,
            "--format",
            "{{range .Mounts}}{{.Source}} -> {{.Destination}}\n{{end}}",
        ])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// #663: before the fix this failed at `docker create` with
/// `invalid field 'ma' must be a key=value pair` — the unquoted comma split the `--mount`
/// argument. The reference started the same workspace fine.
#[test]
fn up_starts_in_a_workspace_whose_path_contains_a_comma() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let temp = TempDir::new().unwrap();
    let workspace = temp.path().join("com,ma");
    write_config(&workspace);
    let _guard = WorkspaceContainer(workspace.clone());

    let output = up(&workspace, &[]);
    assert!(
        output.status.success(),
        "up failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mounts = mounts_of(&container_id(&output));
    assert!(
        mounts.contains(&format!(
            "{} -> /workspaces/com,ma",
            workspace.to_string_lossy()
        )),
        "workspace mount missing from: {mounts}"
    );
}

/// #664: the worktree is mounted from the nearest ancestor it shares with the common dir,
/// and the common dir beside it — the only arrangement in which the relative `gitdir:`
/// still resolves container-side. Asserting the resolution rather than the mount list is
/// what makes this a test of the *point* of the flag.
#[test]
fn up_mounts_a_worktrees_common_git_dir_so_its_gitdir_resolves() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let temp = TempDir::new().unwrap();
    let workspace = write_worktree(
        temp.path(),
        "repos/main",
        "worktrees/feature",
        "../../repos/main/.git/worktrees/feature",
    );
    let _guard = WorkspaceContainer(workspace.clone());

    let output = up(&workspace, &["--mount-git-worktree-common-dir"]);
    assert!(
        output.status.success(),
        "up failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(
        parsed["remoteWorkspaceFolder"], "/workspaces/worktrees/feature",
        "the relocated workspace folder is what the reference reports: {stdout}"
    );

    let mounts = mounts_of(&container_id(&output));
    assert!(
        mounts.contains("-> /workspaces/repos/main/.git"),
        "common dir not mounted: {mounts}"
    );

    let probe = exec(
        &workspace,
        &["--mount-git-worktree-common-dir"],
        r#"p=$(cut -d' ' -f2 .git); test -d "$p" && echo RESOLVES"#,
    );
    assert!(
        String::from_utf8_lossy(&probe.stdout).contains("RESOLVES"),
        "the worktree's gitdir does not resolve inside the container: {} {}",
        String::from_utf8_lossy(&probe.stdout),
        String::from_utf8_lossy(&probe.stderr)
    );
}

/// #664 control, on the same fixture: without the flag the worktree keeps the ordinary
/// `/workspaces/<basename>` mount and the gitdir does NOT resolve. Without this arm,
/// relocating unconditionally would pass the test above.
#[test]
fn up_leaves_a_worktree_alone_without_the_flag() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let temp = TempDir::new().unwrap();
    let workspace = write_worktree(
        temp.path(),
        "repos/main",
        "worktrees/feature",
        "../../repos/main/.git/worktrees/feature",
    );
    let _guard = WorkspaceContainer(workspace.clone());

    let output = up(&workspace, &[]);
    assert!(
        output.status.success(),
        "up failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["remoteWorkspaceFolder"], "/workspaces/feature");

    let mounts = mounts_of(&container_id(&output));
    assert!(
        !mounts.contains("/workspaces/repos/main/.git"),
        "common dir mounted without the flag: {mounts}"
    );

    let probe = exec(
        &workspace,
        &[],
        r#"p=$(cut -d' ' -f2 .git); test -d "$p" && echo RESOLVES || echo MISSING"#,
    );
    assert!(
        String::from_utf8_lossy(&probe.stdout).contains("MISSING"),
        "unexpectedly resolved without the flag: {}",
        String::from_utf8_lossy(&probe.stdout)
    );
}
