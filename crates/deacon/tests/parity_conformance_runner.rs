//! Live differential run of the declarative conformance runner over the registry's
//! declarative cases (022-conformance-runner, US1 T026).
//!
//! Runs ONLY under `cargo nextest run --profile parity`. There is no opt-in env gate and
//! no silent skip: a missing/mismatched oracle, a missing fixture, a CLI failure, or a
//! normalization failure FAILS the test with a cause-specific message (constitution IV).
//! It drives the SHARED runner over every declarative `cases.json` record — spec-
//! expectation cases against deacon, live-differential cases against deacon + the pinned
//! oracle — so adding a case is a pure data edit (SC-001). The deterministic verdict
//! report is emitted on stdout; a run-report fragment is written to
//! `target/parity/report/parity_conformance_runner.json` for the aggregator.

use std::path::Path;

use deacon_conformance::default_registry_dir;
use deacon_conformance::load::Registry;
use deacon_conformance::model::CaseKind;

use parity_harness::evidence::{CaseVerdict, Outcome};
use parity_harness::oracle::Oracle;
use parity_harness::report::{
    CaseResult, Cause, OracleInfo, RawPaths, ReportFragment, VerdictReport, now_rfc3339,
};
use parity_harness::runner::{RUNNER_BINARY, RunConfig, run_case};
use parity_harness::{HarnessError, report_root, workspace_root};

/// This binary's name — the fragment key and raw-artifact subdirectory.
const BINARY: &str = "parity_conformance_runner";

/// Fail the test with the error's cause-specific `Display` message (never `Debug`) so an
/// oracle/prereq/normalization failure reads as its remedy.
fn ff<T>(r: Result<T, HarnessError>) -> T {
    r.unwrap_or_else(|e| panic!("{e}"))
}

/// The report-relative raw paths for a case's FIRST operation (diagnostic pointers for
/// the fragment). The runner writes raw capture under
/// `raw/<RUNNER_BINARY>/<case>__<op>/{deacon,oracle}.{stdout,stderr}`.
fn raw_paths(case_id: &str, first_op: &str) -> RawPaths {
    let base = format!("raw/{RUNNER_BINARY}/{case_id}__{first_op}");
    RawPaths {
        deacon_stdout: format!("{base}/deacon.stdout"),
        deacon_stderr: format!("{base}/deacon.stderr"),
        oracle_stdout: format!("{base}/oracle.stdout"),
        oracle_stderr: format!("{base}/oracle.stderr"),
    }
}

/// Map a case verdict to a report-fragment case result (agree/allowed-difference pass;
/// anything else fails with a cause).
fn case_result(verdict: &CaseVerdict, raw: RawPaths) -> CaseResult {
    match verdict.overall {
        // `no-reference-for-platform` is a NON-BLOCKING coverage gap (no snapshot recorded
        // for THIS platform yet), never a divergence — consistent with the runner's
        // exit-code contract (maps it to 0) and certify (surfaces it as non-blocking info).
        // It is logged in the main loop; here it passes so it never reddens the lane
        // (finding #2).
        Outcome::Agree | Outcome::AllowedDifference | Outcome::NoReferenceForPlatform => {
            CaseResult::pass(verdict.case_id.clone(), raw)
        }
        Outcome::Stale => CaseResult::fail(
            verdict.case_id.clone(),
            Cause::Divergence,
            Some("snapshot stale".to_string()),
            raw,
        ),
        Outcome::Diverge | Outcome::Error => CaseResult::fail(
            verdict.case_id.clone(),
            Cause::Divergence,
            Some(summarize(verdict)),
            raw,
        ),
    }
}

/// A compact, path-free summary of a case's diverging channels for the fragment.
fn summarize(verdict: &CaseVerdict) -> String {
    verdict
        .channels
        .iter()
        .filter(|c| c.outcome != Outcome::Agree && c.outcome != Outcome::AllowedDifference)
        .map(|c| format!("{}: {:?}", c.channel, c.outcome))
        .collect::<Vec<_>>()
        .join("; ")
}

#[tokio::test]
async fn parity_conformance_runner() {
    // Fail fast if the pinned oracle is absent/mismatched — never skip to pass. Every
    // declarative case may need it (live-differential does; spec-expectation ignores it).
    let oracle = ff(Oracle::acquire().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));
    let root = workspace_root();
    let fixtures_root = root.join("conformance").join("fixtures");
    let reports = report_root();

    let registry = Registry::load(&default_registry_dir())
        .unwrap_or_else(|e| panic!("conformance registry must load: {e}"));

    let declarative: Vec<_> = registry
        .cases
        .iter()
        .filter(|c| matches!(c.classify(), Ok(CaseKind::Declarative)))
        .collect();
    assert!(
        !declarative.is_empty(),
        "expected at least one declarative case to drive the runner"
    );

    let snapshots_root = root.join("conformance").join("snapshots");
    let cfg = RunConfig {
        deacon_path: deacon_bin,
        oracle: Some(&oracle),
        fixtures_root: &fixtures_root,
        report_root: &reports,
        snapshots_root: &snapshots_root,
    };

    let started = now_rfc3339();
    let mut verdicts = Vec::new();
    let mut results = Vec::new();
    let mut failures = Vec::new();

    for case in &declarative {
        let verdict = ff(run_case(case, &cfg).await);
        let first_op = case
            .operations
            .first()
            .map(|o| o.id.as_str())
            .unwrap_or("op");
        results.push(case_result(&verdict, raw_paths(&case.id, first_op)));
        match verdict.overall {
            Outcome::Agree | Outcome::AllowedDifference => {}
            // Non-blocking coverage gap: surface it (never a silent skip) but do NOT fail
            // the lane — a snapshot simply has not been recorded for this platform yet
            // (finding #2; consistent with certify + the runner exit-code contract).
            Outcome::NoReferenceForPlatform => eprintln!(
                "note: {} has no committed snapshot for this platform (no-reference-for-platform)",
                case.id
            ),
            _ => failures.push(format!("{}: {}", case.id, summarize(&verdict))),
        }
        verdicts.push(verdict);
    }

    // The deterministic verdict report on stdout (contract runner-cli.md).
    let report = VerdictReport::new(verdicts);
    ff(report.emit_stdout());

    // The run-report fragment for the aggregator's completeness gate.
    let finished = now_rfc3339();
    let fragment = ReportFragment::new(
        BINARY,
        OracleInfo::from(&oracle),
        started,
        finished,
        results,
        Vec::new(),
    );
    ff(fragment.write().await);

    assert!(
        failures.is_empty(),
        "declarative conformance divergence(s) vs the runner's expectations:\n{}",
        failures.join("\n"),
    );
}
