//! `--container-id` against a container deacon did NOT create.
//!
//! Every other Docker-gated test in this crate names a container `up` made, which
//! carries deacon's identity labels, a `devcontainer.metadata` label and a workspace
//! bind mount. `--container-id` can name a plain `docker run` container that has none
//! of those, and five separate behaviors were wrong there — each of them right for the
//! container `up` creates, which is why nothing had caught them. Mined from the
//! reference CLI's own e2e suite at v0.87.0 (#480, batch 13), `src/test/cli.set-up.test.ts`,
//! whose `exec` / `run-user-commands` / `read-configuration` arms all target exactly
//! this shape.
//!
//! - [#655](https://github.com/get2knowio/deacon/issues/655): the lifecycle/exec cwd was
//!   synthesized as `/workspaces/<basename(host cwd)>`, a path the container does not
//!   have, so the exec died with rc 127 — silently in `run-user-commands`, where a
//!   non-blocking phase still reported success.
//! - [#656](https://github.com/get2knowio/deacon/issues/656): `run-user-commands` demanded
//!   a config document instead of reading the container's metadata label.
//! - [#657](https://github.com/get2knowio/deacon/issues/657): `read-configuration
//!   --include-merged-configuration` treated the metadata label as required.
//! - [#658](https://github.com/get2knowio/deacon/issues/658): its `mergedConfiguration`
//!   applied the complete-record branch unconditionally, dropping the caller's own hooks.
//! - [#659](https://github.com/get2knowio/deacon/issues/659): it reported a `workspace`
//!   block derived from the `--config` path, for a workspace nobody named.
//!
//! **Why these are tests and not parity cases.** A parity case creates its container with
//! `op-up`, which stamps both label locations and mounts the workspace — the one shape on
//! which all five old behaviors are correct. The same reasoning already put #527's layered
//! branch here rather than in `parity/cases/` (see the `SPEC_STATUS.md` row). The one
//! claim that IS expressible over an `up` container — #659's empty workspace block — also
//! ships as a parity case, so it gates the release lane too.
//!
//! Every expectation was measured against the pinned reference CLI 0.87.0 on the same
//! container shapes before being written down.
//!
//! Requires Docker; self-skips when unavailable.
#![cfg(unix)]

use assert_cmd::Command;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

mod support;

/// Base image for every container here. Pinned, tiny, and — deliberately — has `root`
/// as its user with `/root` as its home, which is what the home-folder fallback lands in.
const BASE_IMAGE: &str = "alpine:3.17";

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

impl ContainerGuard {
    /// A container deacon knows nothing about: no identity labels, no workspace mount,
    /// and only the labels the caller asks for.
    fn run(labels: &[(&str, &str)]) -> Self {
        let mut cmd = std::process::Command::new("docker");
        cmd.args(["run", "-d"]);
        for (k, v) in labels {
            cmd.args(["--label", &format!("{}={}", k, v)]);
        }
        cmd.args([BASE_IMAGE, "sleep", "infinity"]);
        let out = cmd.output().expect("docker run should spawn");
        assert!(
            out.status.success(),
            "docker run failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert!(!id.is_empty(), "docker run printed no container id");
        Self(id)
    }

    fn id(&self) -> &str {
        &self.0
    }

    fn has_file(&self, path: &str) -> bool {
        std::process::Command::new("docker")
            .args(["exec", &self.0, "test", "-f", path])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", &self.0])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

/// Write a devcontainer document at `dir/devcontainer.json` and return its path.
fn write_config(dir: &Path, body: &str) -> std::path::PathBuf {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join("devcontainer.json");
    fs::write(&path, body).unwrap();
    path
}

fn deacon() -> Command {
    support::deacon_command()
}

// ---------------------------------------------------------------------------
// #655 — the working directory
// ---------------------------------------------------------------------------

/// A `--config` that authors no `workspaceFolder` leaves the target with no workspace,
/// so the cwd is the container user's home — the reference's
/// `remoteCwd = remoteWorkspaceFolder || homeFolder`. deacon used to synthesize
/// `/workspaces/<basename(host cwd)>` and die with rc 127.
#[test]
fn exec_on_a_foreign_container_runs_in_the_home_folder() {
    if !is_docker_available() {
        eprintln!("Skipping exec_on_a_foreign_container_runs_in_the_home_folder: no Docker");
        return;
    }
    let temp = TempDir::new().unwrap();
    let config = write_config(&temp.path().join("cfg"), r#"{ "remoteEnv": {} }"#);
    let container = ContainerGuard::run(&[]);

    let out = deacon()
        .args(["exec", "--container-id", container.id()])
        .arg("--config")
        .arg(&config)
        .arg("pwd")
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "exec should succeed against a foreign container; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "/root");
}

/// The other half of the pair, and what gives the one above its teeth: an AUTHORED
/// `workspaceFolder` is still honored verbatim on the very same container shape. A
/// deacon that simply always used the home folder would pass the test above and fail
/// this one.
#[test]
fn exec_on_a_foreign_container_honors_an_authored_workspace_folder() {
    if !is_docker_available() {
        eprintln!(
            "Skipping exec_on_a_foreign_container_honors_an_authored_workspace_folder: no Docker"
        );
        return;
    }
    let temp = TempDir::new().unwrap();
    let config = write_config(&temp.path().join("cfg"), r#"{ "workspaceFolder": "/etc" }"#);
    let container = ContainerGuard::run(&[]);

    let out = deacon()
        .args(["exec", "--container-id", container.id()])
        .arg("--config")
        .arg(&config)
        .arg("pwd")
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "exec should succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "/etc");
}

/// The same defect through `run-user-commands`, where it was invisible: the hook failed
/// with rc 127 and the non-blocking phase reported `{"outcome":"success"}` over it. The
/// artifact check is the assertion — the exit code alone was already 0 before the fix.
#[test]
fn run_user_commands_on_a_foreign_container_actually_runs_the_hook() {
    if !is_docker_available() {
        eprintln!(
            "Skipping run_user_commands_on_a_foreign_container_actually_runs_the_hook: no Docker"
        );
        return;
    }
    let temp = TempDir::new().unwrap();
    let config = write_config(
        &temp.path().join("cfg"),
        r#"{ "postAttachCommand": "touch /ran-post-attach.txt" }"#,
    );
    let container = ContainerGuard::run(&[]);

    let out = deacon()
        .args(["run-user-commands", "--container-id", container.id()])
        .arg("--config")
        .arg(&config)
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "run-user-commands should succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        container.has_file("/ran-post-attach.txt"),
        "the postAttachCommand did not run — this is the failure the success document hid"
    );
}

// ---------------------------------------------------------------------------
// #656 — where the configuration comes from
// ---------------------------------------------------------------------------

/// With a container selector and nothing naming a configuration, the container's own
/// `devcontainer.metadata` label IS the configuration. Both halves are asserted at once,
/// because the old behavior failed each differently: it exited 1 when the current
/// directory held no document, and silently used that document when it did.
#[test]
fn run_user_commands_takes_its_config_from_the_container_label() {
    if !is_docker_available() {
        eprintln!(
            "Skipping run_user_commands_takes_its_config_from_the_container_label: no Docker"
        );
        return;
    }
    let temp = TempDir::new().unwrap();
    // A current directory that DOES hold a devcontainer document, so "ignored it" is a
    // claim this test can actually make rather than an absence it cannot see.
    write_config(
        &temp.path().join(".devcontainer"),
        r#"{ "image": "alpine:3.17", "postCreateCommand": "touch /from-cwd-config.txt" }"#,
    );
    let container = ContainerGuard::run(&[(
        "devcontainer.metadata",
        r#"[{"postCreateCommand":"touch /from-label.txt"}]"#,
    )]);

    let out = deacon()
        .current_dir(temp.path())
        .args(["run-user-commands", "--container-id", container.id()])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "run-user-commands should succeed with no --config; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        container.has_file("/from-label.txt"),
        "the container label's postCreateCommand did not run"
    );
    assert!(
        !container.has_file("/from-cwd-config.txt"),
        "the current directory's document must be ignored when a container was named"
    );
}

// ---------------------------------------------------------------------------
// #657 / #658 / #659 — read-configuration
// ---------------------------------------------------------------------------

fn read_configuration_json(container_id: &str, config: &Path, extra: &[&str]) -> serde_json::Value {
    let mut cmd = deacon();
    cmd.args(["read-configuration", "--container-id", container_id])
        .arg("--config")
        .arg(config)
        .args(extra);
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "read-configuration should succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("stdout should be one JSON document")
}

/// A container with no `devcontainer.metadata` label at all is not an error: the label
/// is a source of configuration, not a precondition for having one. deacon used to exit
/// 1 with `does not have required 'devcontainer.metadata' label`.
#[test]
fn read_configuration_merges_without_a_container_metadata_label() {
    if !is_docker_available() {
        eprintln!(
            "Skipping read_configuration_merges_without_a_container_metadata_label: no Docker"
        );
        return;
    }
    let temp = TempDir::new().unwrap();
    let config = write_config(
        &temp.path().join("cfg"),
        r#"{ "postAttachCommand": "touch /post-attach.txt" }"#,
    );
    let container = ContainerGuard::run(&[]);

    let doc = read_configuration_json(container.id(), &config, &["--include-merged-configuration"]);
    assert_eq!(
        doc["mergedConfiguration"]["postAttachCommands"],
        serde_json::json!(["touch /post-attach.txt"]),
        "the supplied configuration is the whole merge when the container carries no label"
    );
}

/// The two branches of upstream's `Tr`/`Tt` split, on one container shape each. deacon
/// applied `Tr` unconditionally, which is correct only for the first.
#[test]
fn read_configuration_picks_the_merge_branch_from_the_containers_labels() {
    if !is_docker_available() {
        eprintln!(
            "Skipping read_configuration_picks_the_merge_branch_from_the_containers_labels: no Docker"
        );
        return;
    }
    let temp = TempDir::new().unwrap();
    let config = write_config(
        &temp.path().join("cfg"),
        r#"{ "postAttachCommand": "touch /post-attach.txt" }"#,
    );
    let label = (
        "devcontainer.metadata",
        r#"[{"postCreateCommand":"touch /post-create.txt"}]"#,
    );

    // No identity label → the label's entries are layers BENEATH the caller's config,
    // which contributes its own hooks.
    let foreign = ContainerGuard::run(&[label]);
    let doc = read_configuration_json(foreign.id(), &config, &["--include-merged-configuration"]);
    assert_eq!(
        doc["mergedConfiguration"]["postCreateCommands"],
        serde_json::json!(["touch /post-create.txt"])
    );
    assert_eq!(
        doc["mergedConfiguration"]["postAttachCommands"],
        serde_json::json!(["touch /post-attach.txt"]),
        "a foreign container's label does not displace the caller's own hooks"
    );

    // Identity label present → the label IS the complete lifecycle record, and the
    // caller's config contributes only remoteUser/userEnvProbe/remoteEnv, none of
    // which are collected. This half was already right and must stay right.
    let own = ContainerGuard::run(&[label, ("devcontainer.local_folder", "/host/workspace")]);
    let doc = read_configuration_json(own.id(), &config, &["--include-merged-configuration"]);
    assert_eq!(
        doc["mergedConfiguration"]["postCreateCommands"],
        serde_json::json!(["touch /post-create.txt"])
    );
    assert_eq!(
        doc["mergedConfiguration"]["postAttachCommands"],
        serde_json::json!([]),
        "on this workspace's own dev container the label is the complete record"
    );
}

/// Naming a container and no workspace means there is no workspace. The `--config`
/// path is a document location, not a workspace, and reporting a `workspaceMount` for
/// it describes a mount the container does not have and nobody asked for.
#[test]
fn read_configuration_reports_no_workspace_when_only_a_container_was_named() {
    if !is_docker_available() {
        eprintln!(
            "Skipping read_configuration_reports_no_workspace_when_only_a_container_was_named: no Docker"
        );
        return;
    }
    let temp = TempDir::new().unwrap();
    let config = write_config(&temp.path().join("cfg"), r#"{ "remoteEnv": {} }"#);
    let container = ContainerGuard::run(&[]);

    let doc = read_configuration_json(container.id(), &config, &[]);
    assert_eq!(
        doc["workspace"],
        serde_json::json!({}),
        "a container named directly has no workspace"
    );

    // And the teeth: naming one brings the block back. Same container, same `--config`
    // — the ONLY difference is `--workspace-folder`, so this is a claim about the
    // selector rather than about `read-configuration` having stopped reporting a
    // workspace at all.
    let out = deacon()
        .args(["read-configuration", "--container-id", container.id()])
        .arg("--config")
        .arg(&config)
        .arg("--workspace-folder")
        .arg(temp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "read-configuration with a workspace should succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        doc["workspace"]["workspaceFolder"].is_string(),
        "an explicitly named workspace is still reported: {}",
        doc["workspace"]
    );
}
