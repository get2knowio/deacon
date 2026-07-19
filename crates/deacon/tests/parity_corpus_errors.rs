//! Parity corpus (Tier 1c): the ERROR path, deacon vs the pinned
//! `@devcontainers/cli` oracle.
//!
//! Ported from the retired `fixtures/parity-corpus/run_tier1_errors.py` (018-
//! harden-parity-harness, research D4). Every case under `errors/<name>/` is an
//! invalid or edge-case config; exact error wording is expected to differ between
//! a Rust CLI and a Node CLI, so this runner diffs the accept/reject *decision*
//! (exit-code class) and, when both accept, the resolved configuration value
//! (after the shared `normalize::config` pruning). The decision matrix is driven
//! entirely by the schema-validated waiver records (`errors/<name>/expect.json`):
//!
//! - `both-reject`     — both CLIs must reject.
//! - `both-accept`     — both accept AND, after pruning, resolve to equal configs.
//! - `deacon-stricter` — deacon rejects, the reference leniently accepts.
//!
//! A stale or missing expectation, or a decision that no longer matches its
//! record, FAILS the run naming the case/record (FR-009, FR-011). There is no
//! opt-in gate and no silent skip.
//!
//! Runs ONLY under `cargo nextest run --profile parity`.

use std::collections::HashSet;
use std::path::Path;

use parity_harness::exec::{ExecKind, Invocation, exec_deacon, exec_oracle};
use parity_harness::oracle::Oracle;
use parity_harness::prereq::require_fixture;
use parity_harness::registry::{self, ParityRegistry};
use parity_harness::report::{
    CaseResult, Cause, OracleInfo, RawPaths, ReportFragment, now_rfc3339,
};
use parity_harness::waiver::{Expect, Scope, Waiver, WaiverSet};
use parity_harness::{HarnessError, normalize, workspace_root};

/// This binary's name — the fragment key and raw-artifact subdirectory.
const BINARY: &str = "parity_corpus_errors";

/// The corpus id this runner drives (registry.json `corpora`).
const CORPUS: &str = "errors";

/// Fail the test with the error's cause-specific `Display` message.
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

/// True if the two CLIs' resolved configs agree after the shared pruning. On a
/// normalization failure, fall back to trimmed-stdout equality (mirrors the
/// retired Python driver's `except` branch).
fn values_agree(case: &str, deacon_out: &str, oracle_out: &str) -> bool {
    match (
        normalize::config(case, deacon_out),
        normalize::config(case, oracle_out),
    ) {
        (Ok(d), Ok(o)) => d == o,
        _ => deacon_out.trim() == oracle_out.trim(),
    }
}

/// Evaluate reality against the waiver's expectation. Returns `Ok(())` on parity
/// as expected, else `Err(reason)`.
fn evaluate(
    waiver: &Waiver,
    d_accept: bool,
    r_accept: bool,
    case: &str,
    deacon_out: &str,
    oracle_out: &str,
) -> Result<(), String> {
    let decided = |accept: bool| if accept { "accept" } else { "reject" };
    match &waiver.expect {
        Expect::BothReject {} => {
            if !d_accept && !r_accept {
                Ok(())
            } else {
                Err(format!(
                    "expected both to reject (deacon {}, ref {})",
                    decided(d_accept),
                    decided(r_accept)
                ))
            }
        }
        Expect::BothAccept {} => {
            if !(d_accept && r_accept) {
                Err(format!(
                    "expected both to accept (deacon {}, ref {})",
                    decided(d_accept),
                    decided(r_accept)
                ))
            } else if !values_agree(case, deacon_out, oracle_out) {
                Err("both accept but resolved configuration differs after pruning".to_string())
            } else {
                Ok(())
            }
        }
        Expect::DeaconStricter { .. } => {
            if !d_accept && r_accept {
                Ok(())
            } else {
                Err(format!(
                    "expected deacon-reject / ref-accept, got deacon {} / ref {}",
                    decided(d_accept),
                    decided(r_accept)
                ))
            }
        }
        Expect::ReferenceStricter { .. } => {
            if d_accept && !r_accept {
                Ok(())
            } else {
                Err(format!(
                    "expected deacon-accept / ref-reject, got deacon {} / ref {}",
                    decided(d_accept),
                    decided(r_accept)
                ))
            }
        }
        Expect::FieldDivergence { .. } => {
            Err("field-divergence expectation is not applicable to the error corpus".to_string())
        }
    }
}

#[tokio::test]
async fn parity_corpus_errors() {
    // Fail fast if the pinned oracle is absent/mismatched — never skip to pass.
    let oracle = ff(Oracle::acquire().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));
    let root = workspace_root();

    let registry = ParityRegistry::load().unwrap_or_else(|e| panic!("registry.json: {e}"));
    let corpus = registry
        .corpus(CORPUS)
        .unwrap_or_else(|| panic!("registry.json has no corpus `{CORPUS}`"));
    let errors_root = root.join(&corpus.path);
    ff(require_fixture(&errors_root));

    // Waivers load from the corpus PARENT (which contains `errors/` and
    // `waivers/`); each `errors/<case>/expect.json` is a corpus-case record.
    let corpus_root = errors_root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| errors_root.clone());
    let waivers = ff(WaiverSet::load(&corpus_root));

    let cases = ff(registry::discover_error_cases(&errors_root));
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

        // Each discovered error case carries exactly one expectation record.
        let waiver = match waivers.corpus_case(CORPUS, &case) {
            Some(w) => w,
            None => {
                failures.push(format!(
                    "[{case}] no expectation record (errors/{case}/expect.json)"
                ));
                continue;
            }
        };
        consumed_waivers.insert(waiver.id.clone());

        let ws = case_dir.to_string_lossy().into_owned();
        let config_arg = waiver
            .config
            .as_ref()
            .map(|rel| case_dir.join(rel).to_string_lossy().into_owned());

        let mut args: Vec<&str> = vec!["read-configuration", "--workspace-folder", &ws];
        if let Some(cfg) = config_arg.as_deref() {
            args.push("--config");
            args.push(cfg);
        }

        // Run both CLIs (raw output always captured under target/parity/raw/).
        // Non-zero exit is the DECISION here, not a harness failure, so we do not
        // call `require_success`.
        let deacon_inv =
            ff(exec_deacon(BINARY, &case, ExecKind::Config, deacon_bin, &args, case_dir).await);
        let oracle_inv = ff(exec_oracle(
            BINARY,
            &case,
            ExecKind::Config,
            &oracle.path,
            &args,
            case_dir,
        )
        .await);
        let raw = raw_paths(&deacon_inv, &oracle_inv);

        let deacon_out = deacon_inv.stdout_string();
        let oracle_out = oracle_inv.stdout_string();

        match evaluate(
            waiver,
            deacon_inv.success,
            oracle_inv.success,
            &case,
            &deacon_out,
            &oracle_out,
        ) {
            Ok(()) => {
                // Every error case is governed by exactly one expectation record;
                // record it as consumed so the aggregator can prove completeness.
                case_results.push(CaseResult::pass_waived(&case, vec![waiver.id.clone()], raw));
            }
            Err(reason) => {
                case_results.push(CaseResult::fail(
                    &case,
                    Cause::Divergence,
                    Some(reason.clone()),
                    raw,
                ));
                failures.push(format!("[{case}] {reason}"));
            }
        }
    }

    // Staleness: an errors-scoped corpus-case waiver loaded but never consumed
    // (its case directory is gone) fails the run naming it (FR-011).
    let stale = waivers.stale_among(
        |w| matches!(&w.scope, Scope::CorpusCase { corpus, .. } if corpus == CORPUS),
        &consumed_waivers,
    );
    for id in &stale {
        failures.push(format!(
            "stale waiver `{id}`: its error case directory is gone"
        ));
    }

    let finished = now_rfc3339();
    let fragment = ReportFragment::new(
        BINARY,
        OracleInfo::from(&oracle),
        started,
        finished,
        case_results,
        Vec::new(),
    );
    ff(fragment.write().await);

    assert!(
        failures.is_empty(),
        "error-corpus decision parity divergence(s) vs oracle {} across {} case(s):\n{}",
        oracle.version,
        cases.len(),
        failures.join("\n"),
    );
}
