//! Shared Tier-1 config-corpus runner skeleton for `parity_corpus_tier1` and
//! `parity_corpus_merged` (018-harden-parity-harness, research D4).
//!
//! Lives in a `tests/` SUBDIRECTORY module (not a top-level `tests/*.rs`) so
//! cargo/nextest do not treat it as its own test binary; each corpus binary
//! includes it via `mod corpus_runner;`. The two Tier-1 variants differ only in
//! the extra `read-configuration` argument and the normalization entry point, so
//! they share this one skeleton rather than duplicating the orchestration.

use std::collections::HashSet;
use std::path::Path;

use parity_harness::exec::{ExecKind, Invocation, exec_deacon, exec_oracle};
use parity_harness::oracle::Oracle;
use parity_harness::prereq::require_fixture;
use parity_harness::registry::{self, ParityRegistry};
use parity_harness::report::{
    CaseResult, Cause, OracleInfo, RawPaths, ReportFragment, now_rfc3339,
};
use parity_harness::waiver::{Scope, WaiverSet};
use parity_harness::{HarnessError, normalize, workspace_root};

/// The corpus id these Tier-1 runners drive (registry.json `corpora`).
const CORPUS: &str = "tier1";

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

/// Run the Tier-1 config corpus for `binary`. `base_args` are the leading
/// `read-configuration [--include-merged-configuration]` args (the
/// `--workspace-folder <case>` pair is appended per case); `normalizer` selects
/// `normalize::config` vs `normalize::merged_config`.
pub async fn run_config_corpus(
    binary: &str,
    kind: ExecKind,
    base_args: &[&str],
    normalizer: fn(&str, &str) -> Result<serde_json::Value, HarnessError>,
) {
    // Fail fast if the pinned oracle is absent/mismatched — never skip to pass.
    let oracle = ff(Oracle::acquire().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));
    let root = workspace_root();

    let registry = ParityRegistry::load().unwrap_or_else(|e| panic!("registry.json: {e}"));
    let corpus = registry
        .corpus(CORPUS)
        .unwrap_or_else(|| panic!("registry.json has no corpus `{CORPUS}`"));
    let corpus_root = root.join(&corpus.path);
    ff(require_fixture(&corpus_root));

    let waivers = ff(WaiverSet::load(&corpus_root));

    // Discover cases (immediate subdirs with `.devcontainer/`, excluding
    // errors/waivers/dot/pycache) and enforce the registry minimum.
    let cases = ff(registry::discover_tier1_cases(&corpus_root));
    ff(registry.check_corpus_min(corpus, cases.len()));

    let started = now_rfc3339();
    let mut case_results = Vec::new();
    let mut failures = Vec::new();
    let mut consumed_waivers: HashSet<String> = HashSet::new();

    for case_dir in &cases {
        let case = case_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let ws = case_dir.to_string_lossy().into_owned();

        let mut args: Vec<&str> = base_args.to_vec();
        args.push("--workspace-folder");
        args.push(&ws);

        // Run both CLIs (raw output always captured under target/parity/raw/).
        let deacon_inv = ff(exec_deacon(binary, &case, kind, deacon_bin, &args, case_dir).await);
        let oracle_inv = ff(exec_oracle(binary, &case, kind, &oracle.path, &args, case_dir).await);
        let raw = raw_paths(&deacon_inv, &oracle_inv);

        // A CLI expected to succeed but that failed is a process failure.
        if !deacon_inv.success || !oracle_inv.success {
            let which = if !deacon_inv.success {
                "deacon"
            } else {
                "oracle"
            };
            let summary = format!(
                "{which} exited unsuccessfully (deacon={:?} oracle={:?})",
                deacon_inv.exit_code, oracle_inv.exit_code
            );
            case_results.push(CaseResult::fail(
                &case,
                Cause::OracleFailure,
                Some(summary.clone()),
                raw,
            ));
            failures.push(format!("[{case}] {summary}"));
            continue;
        }

        // Normalize both sides through the single equivalence definition; a
        // normalization failure is a hard failure, never a raw-comparison fallback.
        let deacon_out = deacon_inv.stdout_string();
        let oracle_out = oracle_inv.stdout_string();
        let (deacon_norm, oracle_norm) = match (
            normalizer(&case, &deacon_out),
            normalizer(&case, &oracle_out),
        ) {
            (Ok(d), Ok(o)) => (d, o),
            (d, o) => {
                let cause = d
                    .err()
                    .or(o.err())
                    .map(|e| e.to_string())
                    .unwrap_or_default();
                case_results.push(CaseResult::fail(
                    &case,
                    Cause::Normalization,
                    Some(cause.clone()),
                    raw,
                ));
                failures.push(format!("[{case}] normalization failed: {cause}"));
                continue;
            }
        };

        let divergences = normalize::diff(&deacon_norm, &oracle_norm);
        if divergences.is_empty() {
            case_results.push(CaseResult::pass(&case, raw));
            continue;
        }

        // A corpus-case waiver may characterize this divergence.
        let summary = normalize::summarize(&divergences);
        match waivers.corpus_case(CORPUS, &case) {
            Some(w) => {
                consumed_waivers.insert(w.id.clone());
                case_results.push(CaseResult::pass_waived(&case, vec![w.id.clone()], raw));
            }
            None => {
                case_results.push(CaseResult::fail(
                    &case,
                    Cause::Divergence,
                    Some(summary.clone()),
                    raw,
                ));
                failures.push(format!("[{case}]\n{summary}"));
            }
        }
    }

    // Staleness: a corpus-case waiver for this corpus loaded but never consumed
    // (case gone, or its characterized divergence no longer observed) is stale and
    // fails the run naming it (FR-011).
    let stale = waivers.stale_among(
        |w| matches!(&w.scope, Scope::CorpusCase { corpus, .. } if corpus == CORPUS),
        &consumed_waivers,
    );
    for id in &stale {
        failures.push(format!(
            "stale waiver `{id}`: no matching {CORPUS} case observed its characterized divergence"
        ));
    }

    let finished = now_rfc3339();
    let fragment = ReportFragment::new(
        binary,
        OracleInfo::from(&oracle),
        started,
        finished,
        case_results,
        Vec::new(),
    );
    ff(fragment.write().await);

    assert!(
        failures.is_empty(),
        "{CORPUS} corpus parity divergence(s) vs oracle {} across {} case(s):\n{}",
        oracle.version,
        cases.len(),
        failures.join("\n\n"),
    );
}
