//! Parity: deacon vs the pinned `@devcontainers/cli` oracle for `exec` semantics.
//!
//! Runs ONLY under `cargo nextest run --profile parity`. There is no opt-in env
//! gate and no silent skip: a missing/mismatched oracle, an unavailable Docker, a
//! CLI failure, or a stdout divergence FAILS the test with a cause-specific message
//! (018-harden-parity-harness, FR-002, FR-004..FR-006). Both CLIs' raw output is
//! preserved under `target/parity/raw/` and a single run-report fragment is written
//! to `target/parity/report/parity_exec.json`.
//!
//! The four historical exec checks (working directory, user, TTY, env propagation)
//! run as sequential CASES of one test because the harness design is ONE report
//! fragment per binary. Each case uses its own `TempDir` and tears down its
//! containers (best-effort) before comparison so a failure never leaks state.

use std::path::Path;

use parity_harness::HarnessError;
use parity_harness::exec::{ExecKind, Invocation, exec_deacon, exec_oracle};
use parity_harness::oracle::Oracle;
use parity_harness::prereq::require_docker;
use parity_harness::report::{
    CaseResult, Cause, OracleInfo, RawPaths, ReportFragment, now_rfc3339,
};

/// This binary's name — the fragment key and raw-artifact subdirectory.
const BINARY: &str = "parity_exec";

/// Fail the test with the error's cause-specific `Display` message (never the
/// `Debug` form) so an oracle/prereq/CLI failure reads as its remedy.
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

/// Write a `devcontainer.json` into `<ws>/.devcontainer/`.
fn write_devcontainer(ws: &Path, contents: &str) {
    let dir = ws.join(".devcontainer");
    std::fs::create_dir_all(&dir).expect("create .devcontainer directory");
    std::fs::write(dir.join("devcontainer.json"), contents).expect("write devcontainer.json");
}

/// Best-effort teardown so containers don't leak across cases. Removes the deacon
/// container via `deacon down` and the upstream container discovered by its
/// `devcontainer.local_folder` label. All errors are ignored.
fn teardown(deacon_bin: &Path, ws_str: &str, label: &str) {
    let _ = std::process::Command::new(deacon_bin)
        .args(["down", "--remove", "--workspace-folder", ws_str])
        .output();

    if let Ok(out) = std::process::Command::new("docker")
        .args([
            "ps",
            "--filter",
            &format!("label={label}"),
            "--format",
            "{{.ID}}",
        ])
        .output()
    {
        let ids = String::from_utf8_lossy(&out.stdout);
        for id in ids.split_whitespace() {
            let _ = std::process::Command::new("docker")
                .args(["rm", "-f", id])
                .output();
        }
    }
}

/// Run one exec parity case end to end and return its `CaseResult` plus an optional
/// human-readable failure line (present iff the case diverged).
///
/// `command` is the `sh -lc` argument run in both containers. `oracle_id_label`
/// targets the upstream container explicitly via its `--id-label` (the working-dir
/// case needs it). `oracle_extra`/`deacon_extra` carry the side-specific exec flags
/// (e.g. upstream `--remote-env` vs deacon `--env`). `expected`, when set, is the
/// literal stdout both CLIs must print.
#[allow(clippy::too_many_arguments)]
async fn run_case(
    oracle_path: &Path,
    deacon_bin: &Path,
    case: &str,
    config_json: &str,
    command: &str,
    oracle_id_label: bool,
    oracle_extra: &[&str],
    deacon_extra: &[&str],
    expected: Option<&str>,
) -> (CaseResult, Option<String>) {
    let tmp = tempfile::TempDir::new().expect("create tempdir");
    let ws = tmp.path();
    write_devcontainer(ws, config_json);

    let ws_str = ws.to_string_lossy().into_owned();
    let canonical_ws = std::fs::canonicalize(ws).unwrap_or_else(|_| ws.to_path_buf());
    let id_label = format!(
        "devcontainer.local_folder={}",
        canonical_ws.to_string_lossy()
    );

    let up_args = ["up", "--workspace-folder", ws_str.as_str()];

    // Oracle: up, then exec.
    let oracle_up =
        ff(exec_oracle(BINARY, case, ExecKind::Lifecycle, oracle_path, &up_args, ws).await);
    let mut oracle_args: Vec<&str> = vec!["exec", "--workspace-folder", ws_str.as_str()];
    if oracle_id_label {
        oracle_args.push("--id-label");
        oracle_args.push(id_label.as_str());
    }
    oracle_args.extend_from_slice(oracle_extra);
    oracle_args.extend_from_slice(&["--", "sh", "-lc", command]);
    let oracle_exec = ff(exec_oracle(
        BINARY,
        case,
        ExecKind::Lifecycle,
        oracle_path,
        &oracle_args,
        ws,
    )
    .await);

    // Deacon: up, then exec.
    let deacon_up =
        ff(exec_deacon(BINARY, case, ExecKind::Lifecycle, deacon_bin, &up_args, ws).await);
    let mut deacon_args: Vec<&str> = vec!["exec", "--workspace-folder", ws_str.as_str()];
    deacon_args.extend_from_slice(deacon_extra);
    deacon_args.extend_from_slice(&["--", "sh", "-lc", command]);
    let deacon_exec = ff(exec_deacon(
        BINARY,
        case,
        ExecKind::Lifecycle,
        deacon_bin,
        &deacon_args,
        ws,
    )
    .await);

    // Tear down BEFORE any comparison/require_success panic so state never leaks.
    teardown(deacon_bin, &ws_str, &id_label);

    // Both `up`s and both `exec`s were expected to succeed.
    ff(oracle_up.require_success());
    ff(oracle_exec.require_success());
    ff(deacon_up.require_success());
    ff(deacon_exec.require_success());

    let raw = raw_paths(&deacon_exec, &oracle_exec);
    let out_oracle = oracle_exec.stdout_string().trim().to_string();
    let out_deacon = deacon_exec.stdout_string().trim().to_string();

    let mut detail: Vec<String> = Vec::new();
    if let Some(exp) = expected {
        if out_oracle != exp {
            detail.push(format!("oracle stdout {out_oracle:?} != expected {exp:?}"));
        }
        if out_deacon != exp {
            detail.push(format!("deacon stdout {out_deacon:?} != expected {exp:?}"));
        }
    }
    if out_oracle != out_deacon {
        detail.push(format!(
            "stdout mismatch: oracle={out_oracle:?} deacon={out_deacon:?}"
        ));
    }

    if detail.is_empty() {
        (CaseResult::pass(case, raw), None)
    } else {
        let summary = detail.join("; ");
        (
            CaseResult::fail(case, Cause::Divergence, Some(summary.clone()), raw),
            Some(format!("[{case}] {summary}")),
        )
    }
}

#[tokio::test]
async fn parity_exec() {
    // Fail fast if the pinned oracle or Docker is absent — never skip to pass.
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));

    let started = now_rfc3339();

    // working-directory: explicit workspaceFolder → both print /root.
    let working_directory = run_case(
        &oracle.path,
        deacon_bin,
        "working-directory",
        r#"{
  "name": "ParityExecWorkingDir",
  "image": "alpine:3.19",
  "workspaceFolder": "/root"
}
"#,
        "pwd",
        true,
        &[],
        &[],
        Some("/root"),
    )
    .await;

    // user: containerUser root → both print UID 0.
    let user = run_case(
        &oracle.path,
        deacon_bin,
        "user",
        r#"{
  "name": "ParityExecUser",
  "image": "alpine:3.19",
  "containerUser": "root"
}
"#,
        "id -u",
        false,
        &[],
        &[],
        Some("0"),
    )
    .await;

    // tty: no literal expectation, just identical TTY behavior.
    let tty = run_case(
        &oracle.path,
        deacon_bin,
        "tty",
        r#"{
  "name": "ParityExecTTY",
  "image": "alpine:3.19"
}
"#,
        "test -t 1 && echo TTY || echo NOTTY",
        false,
        &[],
        &[],
        None,
    )
    .await;

    // env-propagation: upstream `--remote-env` vs deacon `--env` → both print BAR.
    let env_propagation = run_case(
        &oracle.path,
        deacon_bin,
        "env-propagation",
        r#"{
  "name": "ParityExecEnv",
  "image": "alpine:3.19"
}
"#,
        "echo $FOO",
        false,
        &["--remote-env", "FOO=BAR"],
        &["--env", "FOO=BAR"],
        Some("BAR"),
    )
    .await;

    let mut cases = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    for (result, failure) in [working_directory, user, tty, env_propagation] {
        cases.push(result);
        if let Some(f) = failure {
            failures.push(f);
        }
    }

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

    assert!(
        failures.is_empty(),
        "exec parity divergence(s) vs oracle {}:\n{}",
        oracle.version,
        failures.join("\n"),
    );
}
