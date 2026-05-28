//! Spec parity (#67): config discovery uses `--workspace-folder` as
//! provided, even when the workspace lives inside a larger git repository.
//!
//! Before the fix, deacon walked up from the user's `--workspace-folder`
//! to the enclosing git root and used the git root for *both* mounting and
//! discovery. That made any sub-project inside a git repo silently load
//! the enclosing repo's `.devcontainer/devcontainer.json` instead of its
//! own. These tests pin discovery to the user's path.

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

/// Build a temp tree:
///   <root>/                ← initialized as a git repo
///     .devcontainer/
///       devcontainer.json  ← parent repo's config ("outer-config")
///     sub/                 ← the user's actual workspace
///       .devcontainer/
///         devcontainer.json← sub-project's config ("inner-config")
fn build_nested_workspace() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    StdCommand::new("git")
        .arg("init")
        .arg(root)
        .output()
        .expect("git init must succeed for this test");

    let outer_dc = root.join(".devcontainer");
    fs::create_dir_all(&outer_dc).unwrap();
    fs::write(
        outer_dc.join("devcontainer.json"),
        r#"{
  "name": "outer-config",
  "image": "alpine:3.18"
}
"#,
    )
    .unwrap();

    let sub = root.join("sub");
    let inner_dc = sub.join(".devcontainer");
    fs::create_dir_all(&inner_dc).unwrap();
    fs::write(
        inner_dc.join("devcontainer.json"),
        r#"{
  "name": "inner-config",
  "image": "alpine:3.18"
}
"#,
    )
    .unwrap();

    temp
}

fn run_read_configuration(sub: &std::path::Path, extra: &[&str]) -> Value {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--workspace-folder")
        .arg(sub);
    for a in extra {
        cmd.arg(a);
    }
    let output = cmd.output().expect("deacon must run");
    assert!(
        output.status.success(),
        "deacon read-configuration failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice::<Value>(&output.stdout).expect("output must be JSON")
}

#[test]
fn read_configuration_in_subdir_loads_inner_config_not_git_root() {
    let temp = build_nested_workspace();
    let sub = temp.path().join("sub");

    let json = run_read_configuration(&sub, &[]);

    // The example's config should win, not the parent repo's. The
    // `configuration.name` and `workspace.configFolderPath` fields both
    // surface the regression cleanly if the bug returns.
    assert_eq!(
        json["configuration"]["name"], "inner-config",
        "discovery must use the user's --workspace-folder (#67)"
    );
    let config_folder = json["workspace"]["configFolderPath"]
        .as_str()
        .expect("configFolderPath must be a string");
    assert!(
        config_folder.contains("/sub/.devcontainer")
            || config_folder.ends_with("sub/.devcontainer"),
        "configFolderPath must point at the sub-project's .devcontainer, got: {config_folder}"
    );
}

#[test]
fn read_configuration_in_subdir_with_explicit_git_root_flag_still_loads_inner() {
    // --mount-workspace-git-root=true (the default) controls the workspace
    // *mount*, not discovery (#67). The inner config must still win.
    let temp = build_nested_workspace();
    let sub = temp.path().join("sub");

    let json = run_read_configuration(&sub, &["--mount-workspace-git-root=true"]);

    assert_eq!(json["configuration"]["name"], "inner-config");
}
