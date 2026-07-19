//! Parity: deacon vs the pinned `@devcontainers/cli` oracle for `read-configuration`.
//!
//! Runs ONLY under `cargo nextest run --profile parity`. There is no opt-in env
//! gate and no silent skip: a missing/mismatched oracle, a missing fixture, a CLI
//! failure, or a normalization failure FAILS the test with a cause-specific
//! message (018-harden-parity-harness, FR-002, FR-004..FR-006). Both CLIs' raw
//! output is preserved under `target/parity/raw/` and a run-report fragment is
//! written to `target/parity/report/parity_read_configuration.json`.

use std::path::Path;

use parity_harness::exec::{ExecKind, Invocation, exec_deacon, exec_oracle};
use parity_harness::normalize;
use parity_harness::oracle::Oracle;
use parity_harness::prereq::require_fixture;
use parity_harness::report::{
    CaseResult, Cause, OracleInfo, RawPaths, ReportFragment, now_rfc3339,
};
use parity_harness::{HarnessError, workspace_root};

/// This binary's name — the fragment key and raw-artifact subdirectory.
const BINARY: &str = "parity_read_configuration";

/// Config-only comparison cases: (case id, repo-relative config path).
const CASES: &[(&str, &str)] = &[
    ("basic", "fixtures/config/basic/devcontainer.jsonc"),
    (
        "with-variables",
        "fixtures/config/with-variables/devcontainer.jsonc",
    ),
];

/// Fail the test with the error's cause-specific `Display` message (never the
/// `Debug` form) so an oracle/prereq/normalization failure reads as its remedy.
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
async fn parity_read_configuration() {
    // Fail fast if the pinned oracle is absent/mismatched — never skip to pass.
    let oracle = ff(Oracle::acquire().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));
    let root = workspace_root();

    let started = now_rfc3339();
    let mut cases = Vec::new();
    let mut failures = Vec::new();

    for (case, config_rel) in CASES {
        let config = root.join(config_rel);
        ff(require_fixture(&config));

        // Workspace = parent of the config file's directory, matching both CLIs'
        // `--workspace-folder` expectations.
        let workspace = config
            .parent()
            .and_then(Path::parent)
            .unwrap_or(root.as_path())
            .to_path_buf();
        let config_str = config.to_string_lossy().into_owned();
        let workspace_str = workspace.to_string_lossy().into_owned();
        let args = [
            "read-configuration",
            "--workspace-folder",
            &workspace_str,
            "--config",
            &config_str,
        ];

        let deacon_inv = ff(exec_deacon(
            BINARY,
            case,
            ExecKind::Config,
            deacon_bin,
            &args,
            &workspace,
        )
        .await);
        ff(deacon_inv.require_success());
        let oracle_inv = ff(exec_oracle(
            BINARY,
            case,
            ExecKind::Config,
            &oracle.path,
            &args,
            &workspace,
        )
        .await);
        ff(oracle_inv.require_success());

        let raw = raw_paths(&deacon_inv, &oracle_inv);
        let deacon_out = deacon_inv.stdout_string();
        let oracle_out = oracle_inv.stdout_string();
        let deacon_norm = ff(normalize::config(case, &deacon_out));
        let oracle_norm = ff(normalize::config(case, &oracle_out));
        let divergences = normalize::diff(&deacon_norm, &oracle_norm);

        if divergences.is_empty() {
            cases.push(CaseResult::pass(*case, raw));
        } else {
            let summary = normalize::summarize(&divergences);
            cases.push(CaseResult::fail(
                *case,
                Cause::Divergence,
                Some(summary.clone()),
                raw,
            ));
            failures.push(format!("[{case}]\n{summary}"));
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
        "read-configuration parity divergence(s) vs oracle {}:\n{}",
        oracle.version,
        failures.join("\n\n"),
    );
}
