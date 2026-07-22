//! Parity: deacon vs the pinned `@devcontainers/cli` oracle for `up` + `exec`.
//!
//! Runs ONLY under `cargo nextest run --profile parity`. There is no opt-in env
//! gate and no silent skip: a missing/mismatched oracle or an unavailable Docker
//! FAILS the test with a cause-specific message (018-harden-parity-harness,
//! FR-002, FR-004..FR-006). Both CLIs are driven through the bounded harness
//! executor, so their raw output is preserved under `target/parity/raw/` and a
//! run-report fragment is written to `target/parity/report/parity_up_exec.json`.
//!
//! Beyond the marker round-trip, this test inspects each launched container's
//! `devcontainer.*` labels. Those `docker` inspections are plain local commands —
//! they interrogate containers, not the two compared CLIs — and a best-effort
//! teardown runs before the label assertions so a failed assertion never leaks a
//! container.

use serde_json::Value;
use std::fs;
use tempfile::TempDir;

use parity_harness::HarnessError;
use parity_harness::exec::{ExecKind, Invocation, exec_deacon, exec_oracle};
use parity_harness::oracle::Oracle;
use parity_harness::prereq::require_docker;
use parity_harness::report::{CaseResult, OracleInfo, RawPaths, ReportFragment, now_rfc3339};

/// This binary's name — the fragment key and raw-artifact subdirectory.
const BINARY: &str = "parity_up_exec";

/// Fail the test with the error's cause-specific `Display` message (never the
/// `Debug` form) so an oracle/prereq failure reads as its remedy.
fn ff<T>(r: Result<T, HarnessError>) -> T {
    r.unwrap_or_else(|e| panic!("{e}"))
}

/// The four preserved raw-output paths (report-relative) for one compared case.
fn raw_paths(deacon: &Invocation, oracle: &Invocation) -> RawPaths {
    RawPaths {
        deacon_stdout: deacon.stdout_rel.display().to_string(),
        deacon_stderr: deacon.stderr_rel.display().to_string(),
        oracle_stdout: oracle.stdout_rel.display().to_string(),
        oracle_stderr: oracle.stderr_rel.display().to_string(),
    }
}

#[tokio::test]
async fn parity_up_and_exec_traditional() {
    // Fail fast if the pinned oracle is absent/mismatched or Docker is unavailable —
    // never skip to pass.
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = std::path::Path::new(env!("CARGO_BIN_EXE_deacon"));

    let started = now_rfc3339();

    // Workspace with an alpine image and a postCreate marker.
    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();
    let ws_str = ws.to_string_lossy().into_owned();

    fs::create_dir(ws.join(".devcontainer")).unwrap();
    // `containerEnv` + a lifecycle command that references `${containerEnv:*}`
    // exercises the #332 parity: the reference CLI does NOT substitute
    // `${containerEnv:VAR}` inside lifecycle command strings — it leaves the token
    // literal for the container shell (which expands it to empty). deacon aligns
    // (it used to inject the value, a command-injection hazard). Both CLIs must
    // produce the SAME empty marker (verified below).
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        r#"{
  "name": "ParityUpExec",
  "image": "alpine:3.19",
  "workspaceFolder": "/workspaces/${localWorkspaceFolderBasename}",
  "containerEnv": { "PARITY_TOKEN": "tkn-42" },
  "remoteEnv": { "REMOTE_ONLY": "from-devcontainer-json" },
  "postCreateCommand": "sh -lc 'echo ready > /tmp/parity_marker; printf \"env=[%s]\\n\" \"${containerEnv:PARITY_TOKEN}\" > /tmp/parity_env_marker'"
}
"#,
    )
    .unwrap();

    let up_args = ["up", "--workspace-folder", ws_str.as_str()];

    // Upstream (oracle): up + exec cat marker. Upstream takes the command WITHOUT a
    // `--` separator.
    let oracle_up_inv = ff(exec_oracle(
        BINARY,
        "traditional",
        ExecKind::Lifecycle,
        &oracle.path,
        &up_args,
        ws,
    )
    .await);
    ff(oracle_up_inv.require_success());

    let oracle_exec_args = [
        "exec",
        "--workspace-folder",
        ws_str.as_str(),
        "sh",
        "-lc",
        "cat /tmp/parity_marker && cat /tmp/parity_env_marker",
    ];
    let oracle_exec_inv = ff(exec_oracle(
        BINARY,
        "traditional",
        ExecKind::Lifecycle,
        &oracle.path,
        &oracle_exec_args,
        ws,
    )
    .await);
    ff(oracle_exec_inv.require_success());
    let out1 = oracle_exec_inv.stdout_string();
    assert!(out1.contains("ready"), "upstream marker missing: {}", out1);

    // Ours (deacon): up + exec cat marker. deacon takes the command AFTER a `--`.
    let deacon_up_inv = ff(exec_deacon(
        BINARY,
        "traditional",
        ExecKind::Lifecycle,
        deacon_bin,
        &up_args,
        ws,
    )
    .await);
    ff(deacon_up_inv.require_success());

    let deacon_exec_args = [
        "exec",
        "--workspace-folder",
        ws_str.as_str(),
        "--",
        "sh",
        "-lc",
        "cat /tmp/parity_marker && cat /tmp/parity_env_marker",
    ];
    let deacon_exec_inv = ff(exec_deacon(
        BINARY,
        "traditional",
        ExecKind::Lifecycle,
        deacon_bin,
        &deacon_exec_args,
        ws,
    )
    .await);
    ff(deacon_exec_inv.require_success());
    let out2 = deacon_exec_inv.stdout_string();
    assert!(out2.contains("ready"), "deacon marker missing: {}", out2);

    // #332: `${containerEnv:PARITY_TOKEN}` in the postCreateCommand must resolve
    // IDENTICALLY for both CLIs. The reference leaves it literal → the shell
    // expands it to empty → `env=[]`. deacon aligns, so both markers match. A
    // regression that made deacon inject the value would surface here as
    // `env=[tkn-42]` vs `env=[]`.
    fn env_marker_line(out: &str) -> &str {
        out.lines()
            .find(|l| l.starts_with("env="))
            .unwrap_or_else(|| panic!("env marker line missing in exec output: {out:?}"))
    }
    let oracle_env = env_marker_line(&out1);
    let deacon_env = env_marker_line(&out2);
    assert_eq!(
        deacon_env, oracle_env,
        "lifecycle `${{containerEnv:*}}` substitution diverged (deacon vs oracle)"
    );
    assert_eq!(
        deacon_env, "env=[]",
        "reference leaves `${{containerEnv:*}}` literal (shell → empty); deacon must match"
    );

    // --- Label parity checks (plain local docker inspection of the containers) ---
    fn docker_out(args: &[&str]) -> String {
        let out = std::process::Command::new("docker")
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "docker {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    // Find upstream container ID by matching labels and image.
    let upstream_id = {
        let format = "{{.ID}}";
        let list = docker_out(&[
            "ps",
            "--filter",
            &format!("label=devcontainer.local_folder={}", ws_str),
            "--filter",
            "ancestor=alpine:3.19",
            "--format",
            format,
        ]);
        assert!(
            !list.is_empty(),
            "no upstream container found with devcontainer.local_folder={}",
            ws_str
        );
        list.lines().next().unwrap().to_string()
    };
    // Capture labels while the container is still alive (before teardown).
    let upstream_labels_json =
        docker_out(&["inspect", "-f", "{{ json .Config.Labels }}", &upstream_id]);

    // Deacon container: identify by devcontainer.name and devcontainer.source=deacon.
    let deacon_id = {
        let format = "{{.ID}}";
        let list = docker_out(&[
            "ps",
            "--filter",
            "label=devcontainer.source=deacon",
            "--filter",
            "label=devcontainer.name=ParityUpExec",
            "--filter",
            "ancestor=alpine:3.19",
            "--format",
            format,
        ]);
        assert!(
            !list.is_empty(),
            "no deacon container found with devcontainer.name=ParityUpExec"
        );
        list.lines().next().unwrap().to_string()
    };
    let deacon_labels_json =
        docker_out(&["inspect", "-f", "{{ json .Config.Labels }}", &deacon_id]);

    // #322: `exec --container-id` (no --workspace-folder/--config) must recover the
    // config-only `remoteEnv` from the container's `devcontainer.metadata` label —
    // which `up` now stamps — identically to the reference. Runs while the
    // containers are still alive, before teardown below.
    let deacon_cid_inv = ff(exec_deacon(
        BINARY,
        "container-id",
        ExecKind::Lifecycle,
        deacon_bin,
        &[
            "exec",
            "--container-id",
            deacon_id.as_str(),
            "--",
            "sh",
            "-lc",
            "printf 'remote=[%s]' \"$REMOTE_ONLY\"",
        ],
        ws,
    )
    .await);
    ff(deacon_cid_inv.require_success());
    let oracle_cid_inv = ff(exec_oracle(
        BINARY,
        "container-id",
        ExecKind::Lifecycle,
        &oracle.path,
        &[
            "exec",
            "--container-id",
            upstream_id.as_str(),
            "sh",
            "-lc",
            "printf 'remote=[%s]' \"$REMOTE_ONLY\"",
        ],
        ws,
    )
    .await);
    ff(oracle_cid_inv.require_success());
    let deacon_cid_env = deacon_cid_inv.stdout_string();
    let oracle_cid_env = oracle_cid_inv.stdout_string();
    assert!(
        deacon_cid_env.contains("remote=[from-devcontainer-json]"),
        "deacon exec --container-id did not recover remoteEnv from the metadata label: {deacon_cid_env:?}"
    );
    assert_eq!(
        deacon_cid_env.trim(),
        oracle_cid_env.trim(),
        "exec --container-id remoteEnv recovery diverged (deacon vs oracle)"
    );

    // Best-effort teardown BEFORE the label assertions so a failed assertion never
    // leaks containers. Errors are intentionally ignored.
    let _ = std::process::Command::new("docker")
        .args(["rm", "-f", &upstream_id])
        .output();
    let _ = std::process::Command::new(deacon_bin)
        .args(["down", "--remove", "--workspace-folder", ws_str.as_str()])
        .current_dir(ws)
        .output();

    // Assert key upstream labels exist and match workspace.
    let upstream_labels: Value = serde_json::from_str(&upstream_labels_json).unwrap_or(Value::Null);
    let ul = upstream_labels.as_object().expect("upstream labels object");
    assert_eq!(
        ul.get("devcontainer.local_folder").and_then(|v| v.as_str()),
        Some(ws_str.as_str()),
        "upstream devcontainer.local_folder mismatch"
    );
    assert_eq!(
        ul.get("devcontainer.config_file").and_then(|v| v.as_str()),
        Some(
            ws.join(".devcontainer/devcontainer.json")
                .to_string_lossy()
                .as_ref()
        ),
        "upstream devcontainer.config_file mismatch"
    );
    assert!(
        ul.keys().any(|k| k.starts_with("devcontainer.")),
        "upstream labels missing devcontainer.* keys"
    );

    let deacon_labels: Value = serde_json::from_str(&deacon_labels_json).unwrap_or(Value::Null);
    let dl = deacon_labels.as_object().expect("deacon labels object");
    assert_eq!(
        dl.get("devcontainer.name").and_then(|v| v.as_str()),
        Some("ParityUpExec"),
        "deacon devcontainer.name mismatch"
    );
    assert_eq!(
        dl.get("devcontainer.source").and_then(|v| v.as_str()),
        Some("deacon"),
        "deacon devcontainer.source mismatch"
    );
    assert!(
        dl.keys().any(|k| k.starts_with("devcontainer.")),
        "deacon labels missing devcontainer.* keys"
    );

    // Note: We don't assert exact label key equality across CLIs because upstream and
    // deacon use different labeling schemes. We verify that each assigns the expected,
    // identifying devcontainer.* labels tied to the workspace/name semantics.

    // Record the successful case and write the per-binary report fragment. The raw
    // paths reference the primary compared outputs — the two `exec` invocations.
    let raw = raw_paths(&deacon_exec_inv, &oracle_exec_inv);
    let cases = vec![CaseResult::pass("traditional", raw)];
    let finished = now_rfc3339();
    let fragment = ReportFragment::new(
        BINARY,
        OracleInfo::from(&oracle),
        started,
        finished,
        cases,
        Vec::new(),
    );
    ff(fragment.write().await);
}
