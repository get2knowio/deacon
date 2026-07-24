//! `conformance-snapshot` — the reviewed snapshot REFRESH bin (022-conformance-runner
//! US2, T037; contract runner-cli.md §2).
//!
//! `cargo run -p parity-harness --bin conformance-snapshot -- refresh [--case <id>]
//! [--platform <os-arch>]`
//!
//! Runs each `snapshot`-oracle case's operations against the VERIFIED pinned reference,
//! captures and normalizes the declared channels, records the 13-field provenance, and
//! writes `provenance.json` / `raw.json` / `normalized.json` ATOMICALLY under
//! `conformance/snapshots/<os-arch>/<case-id>/`. It requires the verified oracle, Docker,
//! and Node, and FAILS LOUD if any is absent (constitution IV — never a silent skip,
//! never a fabricated provenance field). It prints a review diff (old vs new) for the
//! reviewer; the git diff is the review surface. Ordinary test runs NEVER call this —
//! they only read/compare (FR-021).

use std::process::ExitCode;

use deacon_conformance::load::Registry;
use deacon_conformance::model::{CaseKind, OracleType, TestCase};
use deacon_conformance::{default_registry_dir, snapshot, workspace_root};

use parity_harness::exec::Side;
use parity_harness::oracle::Oracle;
use parity_harness::prereq::require_docker;
use parity_harness::runner::{RunConfig, capture_provenance, collect_evidence_on};
use parity_harness::{HarnessError, evidence, report_root};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("refresh") => run_refresh(&args[1..]),
        _ => {
            eprintln!("usage: conformance-snapshot refresh [--case <id>] [--platform <os-arch>]");
            ExitCode::from(2)
        }
    }
}

/// Parse `--case`/`--platform` and drive the refresh on a bounded tokio runtime.
fn run_refresh(args: &[String]) -> ExitCode {
    let mut case_filter: Option<String> = None;
    let mut platform: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--case" => {
                i += 1;
                match args.get(i) {
                    Some(v) => case_filter = Some(v.clone()),
                    None => return usage("--case requires a value"),
                }
            }
            "--platform" => {
                i += 1;
                match args.get(i) {
                    Some(v) => platform = Some(v.clone()),
                    None => return usage("--platform requires a value"),
                }
            }
            other => return usage(&format!("unknown argument {other:?}")),
        }
        i += 1;
    }

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: could not start async runtime: {e}");
            return ExitCode::from(4);
        }
    };
    match runtime.block_on(refresh(case_filter.as_deref(), platform.as_deref())) {
        Ok(count) => {
            eprintln!("refreshed {count} snapshot(s)");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(4)
        }
    }
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("error: {msg}");
    eprintln!("usage: conformance-snapshot refresh [--case <id>] [--platform <os-arch>]");
    ExitCode::from(2)
}

/// Record snapshots for the selected `snapshot`-oracle cases against the verified
/// reference. Returns the number of snapshots written.
async fn refresh(case_filter: Option<&str>, platform: Option<&str>) -> Result<usize, HarnessError> {
    // Fail-loud prerequisites: verified oracle + Docker + Node — never a silent skip.
    let oracle = Oracle::acquire().await?;
    require_docker().await?;
    let env = snapshot::probe_environment();
    if env.node_version.is_none() {
        return Err(HarnessError::NodeUnavailable {
            cause: "`node --version` did not report a version".to_string(),
        });
    }

    let root = workspace_root();
    let fixtures_root = root.join("conformance").join("fixtures");
    let snapshots_root = snapshot::default_snapshots_dir();
    let reports = report_root();
    let os_arch = platform
        .map(str::to_string)
        .unwrap_or_else(snapshot::current_os_arch);

    let registry =
        Registry::load(&default_registry_dir()).map_err(|e| HarnessError::FixtureMissing {
            path: default_registry_dir().join(format!("<load failed: {e}>")),
        })?;

    let cases: Vec<&TestCase> = registry
        .cases
        .iter()
        .filter(|c| matches!(c.classify(), Ok(CaseKind::Declarative)))
        .filter(|c| c.oracle_type == Some(OracleType::Snapshot))
        .filter(|c| case_filter.is_none_or(|id| c.id == id))
        .collect();

    if cases.is_empty() {
        return Err(HarnessError::FixtureMissing {
            path: default_registry_dir().join(match case_filter {
                Some(id) => format!("<no snapshot-oracle case {id:?}>"),
                None => "<no snapshot-oracle cases>".to_string(),
            }),
        });
    }

    let cfg = RunConfig {
        // The refresh records the REFERENCE; `deacon_path` is a placeholder (the oracle
        // path) since only the oracle side is collected here.
        deacon_path: &oracle.path,
        oracle: Some(&oracle),
        fixtures_root: &fixtures_root,
        report_root: &reports,
        snapshots_root: &snapshots_root,
    };

    let mut written = 0usize;
    for case in cases {
        let dir = snapshot::snapshot_case_dir(&snapshots_root, &os_arch, &case.id);
        // The prior snapshot (if any) is the review baseline.
        let old = snapshot::load_snapshot(&dir).ok();

        // Run the case against the reference and capture raw + normalized evidence.
        let case_evidence = collect_evidence_on(Side::Oracle, &oracle.path, case, &cfg).await?;
        let provenance = capture_provenance(case, &cfg, &oracle.version)?;

        // Print the review diff BEFORE writing (old vs new).
        print_review_diff(case, &old, &provenance, &case_evidence);

        evidence::write_snapshot(&dir, &provenance, &case_evidence).await?;
        eprintln!("wrote {}", dir.display());
        written += 1;
    }
    Ok(written)
}

/// Print a human review diff (old vs newly-recorded) for one case.
fn print_review_diff(
    case: &TestCase,
    old: &Option<snapshot::Snapshot>,
    provenance: &snapshot::Provenance,
    new_evidence: &evidence::CaseEvidence,
) {
    let new_normalized = serde_json::to_value(&new_evidence.normalized).unwrap_or_default();
    let new_raw = serde_json::to_value(&new_evidence.raw).unwrap_or_default();
    match old {
        None => eprintln!("[{}] NEW snapshot (no prior baseline)", case.id),
        Some(prev) => {
            let new_snap = snapshot::Snapshot {
                provenance: provenance.clone(),
                raw: new_raw,
                normalized: new_normalized,
            };
            let entries = snapshot::diff(prev, &new_snap);
            let material: Vec<_> = entries
                .iter()
                .filter(|e| !(e.artifact == "provenance" && e.path == "capturedAt"))
                .collect();
            if material.is_empty() {
                eprintln!(
                    "[{}] no material change (only capturedAt refreshed)",
                    case.id
                );
            } else {
                eprintln!("[{}] review diff:", case.id);
                for e in material {
                    eprintln!("  {} {}: {} -> {}", e.artifact, e.path, e.old, e.new);
                }
            }
        }
    }
}
