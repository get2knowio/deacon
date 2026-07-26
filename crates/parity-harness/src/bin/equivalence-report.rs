//! `equivalence-report` — the live equivalent-or-stricter ledger (023, US7, T082;
//! contracts/equivalence-ledger.md).
//!
//! `cargo run -p parity-harness --bin equivalence-report -- [--carrier <name>]`
//!
//! Runs the SUPERSEDED path and its REPLACEMENT over the same baseline units and records,
//! per unit, how the two outcomes relate. Deleting a superseded program is gated on this
//! ledger, because "the new thing passes" is not evidence — the new thing must not pass
//! where the old thing *failed* (FR-033–FR-038).
//!
//! # Which carriers this bin can judge, and why the rest are silent
//!
//! The legacy comparison for a **config corpus** carrier is expressible through the shared
//! harness API: run both CLIs, normalize through the single `normalize::config` /
//! `merged_config`, diff. This bin therefore judges those carriers for real. The
//! **Docker scenario** carriers (`parity_exec`, `parity_build`, `parity_up_exec`,
//! `parity_observable_state`, `parity_state_diff`) assert through bespoke orchestration
//! that lives inside their own test binaries; re-implementing it here would be a SECOND
//! comparison implementation, which is the thing FR-030 forbids — so this bin records NO
//! verdict for them.
//!
//! No verdict is not a pass. A carrier with no verdict fails deletion condition 1, which
//! is the correct and safe reading: unproven is not the same as safe.
//!
//! # Preconditions (constitution IV)
//!
//! The verified pinned oracle and the deacon binary under test are REQUIRED. A missing or
//! mismatched oracle fails the run with its cause-specific error; a ledger that cannot be
//! produced is not an empty ledger.

use std::collections::{BTreeMap, BTreeSet};
use std::process::ExitCode;

use deacon_conformance::baseline::{BaselineFile, UnitCategory, load_baseline};
use deacon_conformance::load::Registry;
use deacon_conformance::model::{CaseKind, TestCase};
use deacon_conformance::{default_baseline_file, default_registry_dir, workspace_root};

use parity_harness::equivalence::{
    ComparisonOutcome, EquivalenceEntry, EquivalenceLedger, Relation, classify_relation,
};
use parity_harness::oracle::Oracle;
use parity_harness::prereq::deacon_binary;
use parity_harness::registry::ParityRegistry;
use parity_harness::report::{CaseResult, Outcome as LegacyOutcome};
use parity_harness::runner::{RunConfig, run_case};
use parity_harness::{HarnessError, report_root};

/// The program every migrated case runs on: the migration's DESTINATION, never a
/// deletion candidate however its own units are judged. Mirrors
/// `deacon_conformance::conservation::SURVIVING_RUNNER`.
const SURVIVING_RUNNER: &str = "parity_conformance_runner";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut carrier: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--carrier" => {
                i += 1;
                match args.get(i) {
                    Some(v) => carrier = Some(v.clone()),
                    None => return usage("--carrier requires a value"),
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
    match runtime.block_on(produce(carrier.as_deref())) {
        Ok(ledger) => {
            let permissive: Vec<&EquivalenceEntry> = ledger
                .entries
                .iter()
                .filter(|e| e.relation == Relation::MorePermissive)
                .collect();
            for entry in &permissive {
                eprintln!(
                    "MORE-PERMISSIVE {}: legacy `{}` vs replacement `{}` — {}",
                    entry.unit,
                    entry.legacy_outcome,
                    entry.replacement_outcome,
                    entry.detail.as_deref().unwrap_or("<no detail>")
                );
            }
            let defects: Vec<String> = ledger.entries.iter().flat_map(|e| e.defects()).collect();
            for defect in &defects {
                eprintln!("MALFORMED {defect}");
            }
            if permissive.is_empty() && defects.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(4)
        }
    }
}

fn usage(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    eprintln!("usage: equivalence-report [--carrier <name>]");
    ExitCode::from(2)
}

/// Run the SUPERSEDED carrier's OWN test binary and read the per-case outcomes it
/// reports.
///
/// This re-implements nothing: every live parity binary already writes a
/// `ReportFragment` per case it compared, under `target/parity/report/<binary>/`. Reading
/// those fragments is therefore the legacy path's own verdict, in its own words — which is
/// the only way to compare against it without building a second comparison implementation
/// (FR-030).
///
/// Reading goes through [`aggregate::read_fragments`], the SAME reassembly the run report
/// uses, rather than a second hand-rolled path read. That is not tidiness: a carrier's
/// cases are written by separate nextest processes into separate files (024 D-1), so
/// "the fragment" is a merge, and a bin that opened one path would see one case. An
/// earlier revision of this bin did exactly that against the retired flat layout and
/// reported every carrier as having produced no verdict at all.
///
/// The nextest run is allowed to FAIL: a carrier reporting a difference is exactly the
/// state a relation is computed over. What is not tolerated is a missing fragment — that
/// means the binary never produced a verdict, and no verdict is not a pass.
fn run_legacy_carrier(carrier: &str) -> Result<Vec<CaseResult>, HarnessError> {
    // Stale evidence from an earlier run would silently stand in for this one, so clear
    // BOTH layouts: the per-case tree this carrier writes now, and the retired flat file
    // an older checkout may have left behind (`read_fragments` still merges it).
    let report_dir = report_root().join("report");
    let _ = std::fs::remove_dir_all(report_dir.join(carrier));
    let _ = std::fs::remove_file(report_dir.join(format!("{carrier}.json")));

    let status = std::process::Command::new("cargo")
        .args([
            "nextest",
            "run",
            "--profile",
            "parity",
            "-E",
            &format!("binary(={carrier})"),
        ])
        .current_dir(workspace_root())
        .status()
        .map_err(|e| HarnessError::Report {
            cause: format!("could not run the superseded carrier `{carrier}`: {e}"),
        })?;
    eprintln!("legacy `{carrier}` finished with {status} (a difference is a valid outcome here)");

    let fragments = parity_harness::aggregate::read_fragments(&report_root())?;
    let parsed = fragments
        .into_iter()
        .find(|f| f.binary == carrier)
        .ok_or_else(|| HarnessError::Report {
            cause: format!(
                "`{carrier}` produced no report fragment under {} — it reported no verdict, \
                 and no verdict is not a pass",
                report_dir.join(carrier).display()
            ),
        })?;
    Ok(parsed.cases)
}

/// The wire spelling of a legacy `CaseResult`'s outcome, in the vocabulary
/// [`ComparisonOutcome::from_outcome_name`] reduces.
fn legacy_outcome_name(result: &CaseResult) -> &'static str {
    match result.outcome {
        LegacyOutcome::Pass => "pass",
        LegacyOutcome::PassWaived => "pass-waived",
        LegacyOutcome::Fail => "fail",
    }
}

/// Produce the ledger, writing `target/parity/equivalence.json`.
async fn produce(carrier_filter: Option<&str>) -> Result<EquivalenceLedger, HarnessError> {
    // Fail loud on every precondition BEFORE any comparison runs.
    let oracle = Oracle::acquire().await?;
    let deacon = deacon_binary()?;
    let baseline: BaselineFile = load_baseline(&default_baseline_file())
        .map_err(|e| HarnessError::NormalizationFailed {
            channel: "baseline".to_string(),
            cause: e.to_string(),
        })?
        .ok_or_else(|| HarnessError::FixtureMissing {
            path: default_baseline_file(),
        })?;
    let registry =
        Registry::load(&default_registry_dir()).map_err(|e| HarnessError::WaiverInvalid {
            path: default_registry_dir(),
            cause: e.to_string(),
        })?;

    let parity_registry = ParityRegistry::load().map_err(|cause| HarnessError::Report {
        cause: format!("malformed parity registry: {cause}"),
    })?;
    let cases: BTreeMap<&str, &TestCase> =
        registry.cases.iter().map(|c| (c.id.as_str(), c)).collect();
    let destinations = destinations_by_unit(&registry);

    let fixtures_root = workspace_root().join("conformance").join("fixtures");
    let snapshots_root = workspace_root().join("conformance").join("snapshots");
    let reports = report_root();
    let cfg = RunConfig {
        deacon_path: &deacon,
        oracle: Some(&oracle),
        fixtures_root: &fixtures_root,
        report_root: &reports,
        snapshots_root: &snapshots_root,
    };

    // Every SUPERSEDED carrier: the live parity binaries, minus the declarative runner
    // (the migration's destination, never a deletion candidate).
    let carriers: Vec<String> = parity_registry
        .live_names()
        .into_iter()
        .filter(|name| *name != SURVIVING_RUNNER)
        .filter(|name| carrier_filter.is_none_or(|f| f == *name))
        .map(str::to_string)
        .collect();
    if carriers.is_empty() {
        return Err(HarnessError::Report {
            cause: format!(
                "no superseded carrier matched {:?} — the registry's live binaries are {:?}",
                carrier_filter,
                parity_registry.live_names()
            ),
        });
    }

    let mut entries: Vec<EquivalenceEntry> = Vec::new();
    for carrier in &carriers {
        let units: Vec<&deacon_conformance::baseline::BaselineUnit> = baseline
            .records
            .iter()
            .filter(|u| &u.program == carrier)
            .filter(|u| u.category != UnitCategory::ExternalCorpusEntry)
            .collect();
        if units.is_empty() {
            eprintln!("SKIP `{carrier}`: no baseline unit belongs to it");
            continue;
        }

        // --- the SUPERSEDED path: its OWN binary, its OWN verdict ---------
        let legacy_cases = run_legacy_carrier(carrier)?;
        let by_case: BTreeMap<&str, &CaseResult> =
            legacy_cases.iter().map(|c| (c.case.as_str(), c)).collect();

        for unit in units {
            let case_id = unit
                .id
                .split_once("::")
                .map(|(_, case)| case.to_string())
                .unwrap_or_default();

            let Some(legacy) = by_case.get(case_id.as_str()) else {
                eprintln!(
                    "UNVERDICTED {}: `{carrier}` reported no result for case `{case_id}`, so \
                     the unit stays unproven",
                    unit.id
                );
                continue;
            };

            // --- the REPLACEMENT path -------------------------------------
            let replacement = replacement_outcome(&unit.id, &destinations, &cases, &cfg).await?;

            let legacy_state = ComparisonOutcome::from_outcome_name(legacy_outcome_name(legacy));
            let replacement_state = ComparisonOutcome::from_outcome_name(&replacement.0);
            let Some(relation) = classify_relation(legacy_state, replacement_state) else {
                eprintln!(
                    "UNCLASSIFIABLE {}: legacy `{}` / replacement `{}` — at least one side \
                     did not complete, so no relation is recorded (the unit stays unproven)",
                    unit.id,
                    legacy_outcome_name(legacy),
                    replacement.0
                );
                continue;
            };

            // Both-diverge pairs are `equivalent` per the contract (outcome, not message
            // text) — but they are the one shape outcome-comparison is blind in, so the
            // detail carries BOTH summaries side by side for the reviewer rather than the
            // contract's "no detail needed".
            let detail = match (relation, legacy_state, replacement_state) {
                (Relation::Equivalent, ComparisonOutcome::Clean, _) => None,
                _ => Some(format!(
                    "legacy: {}; replacement: {}",
                    legacy
                        .diff_summary
                        .as_deref()
                        .unwrap_or("no difference reported"),
                    replacement.1.as_deref().unwrap_or("no difference reported")
                )),
            };
            let characterized_as = match relation {
                Relation::Stricter => {
                    characterization_for(&unit.id, &destinations, &cases, &registry)
                }
                _ => None,
            };

            entries.push(EquivalenceEntry {
                unit: unit.id.clone(),
                carrier: carrier.to_string(),
                legacy_outcome: legacy_outcome_name(legacy).to_string(),
                replacement_outcome: replacement.0,
                relation,
                detail,
                characterized_as,
            });
        }
    }

    entries.sort_by(|a, b| a.unit.cmp(&b.unit));
    let ledger = EquivalenceLedger {
        baseline_revision: baseline.revision.clone(),
        entries,
    };
    write_ledger(&ledger)?;

    let mut by_relation: BTreeMap<&str, usize> = BTreeMap::new();
    for entry in &ledger.entries {
        *by_relation.entry(entry.relation.as_str()).or_default() += 1;
    }
    eprintln!(
        "ledger: {} unit(s) judged — {:?}",
        ledger.entries.len(),
        by_relation
    );
    Ok(ledger)
}

/// Run the REPLACEMENT path for one unit: every declarative case it maps to. The worst
/// outcome across them is the unit's outcome — a unit whose rejection half diverges is
/// not covered just because its agreement half agrees.
async fn replacement_outcome(
    unit: &str,
    destinations: &BTreeMap<String, Vec<String>>,
    cases: &BTreeMap<&str, &TestCase>,
    cfg: &RunConfig<'_>,
) -> Result<(String, Option<String>), HarnessError> {
    let Some(case_ids) = destinations.get(unit) else {
        return Ok((
            "no-destination".to_string(),
            Some("the unit maps to no declarative case".to_string()),
        ));
    };

    let mut worst: Option<parity_harness::evidence::Outcome> = None;
    let mut details: Vec<String> = Vec::new();
    for case_id in case_ids {
        let Some(case) = cases.get(case_id.as_str()) else {
            continue;
        };
        if !matches!(case.classify(), Ok(CaseKind::Declarative)) {
            continue;
        }
        let verdict = run_case(case, cfg).await?;
        for channel in &verdict.channels {
            if channel.outcome != parity_harness::evidence::Outcome::Agree {
                details.push(format!(
                    "{}:{} {}",
                    case_id,
                    channel.channel,
                    channel.outcome.as_str()
                ));
            }
        }
        worst = Some(match worst {
            Some(current) if current.severity() >= verdict.overall.severity() => current,
            _ => verdict.overall,
        });
    }

    match worst {
        Some(outcome) => Ok((
            outcome.as_str().to_string(),
            (!details.is_empty()).then(|| details.join("; ")),
        )),
        None => Ok((
            "no-destination".to_string(),
            Some("no runnable declarative case".to_string()),
        )),
    }
}

/// Unit → the declarative case ids it migrated to.
fn destinations_by_unit(registry: &Registry) -> BTreeMap<String, Vec<String>> {
    registry
        .mapping
        .iter()
        .filter(|m| m.disposition.requires_cases())
        .map(|m| (m.unit.clone(), m.case_ids.clone()))
        .collect()
}

/// The registry identity characterizing a `stricter` relation, if one exists: a scoped
/// tolerance's backing id, else a `corpus_case` waiver covering the unit's case, else the
/// linked behavior when that behavior itself records the divergence.
fn characterization_for(
    unit: &str,
    destinations: &BTreeMap<String, Vec<String>>,
    cases: &BTreeMap<&str, &TestCase>,
    registry: &Registry,
) -> Option<String> {
    let case_ids = destinations.get(unit)?;
    for case_id in case_ids {
        // An id that resolves to no case is not an answer for THIS destination, but it
        // must not abort the search: the waiver and behavior fallbacks below are
        // independent sources, and skipping them would report a real `stricter` relation
        // as uncharacterized — which reads as suppression and blocks the deletion.
        let Some(case) = cases.get(case_id.as_str()) else {
            continue;
        };
        if let Some(ad) = case.allowed_differences.first() {
            if let Some(id) = ad.waiver_id.as_deref().or(ad.divergence_id.as_deref()) {
                return Some(id.to_string());
            }
        }
    }
    let case_name = unit.split_once("::").map(|(_, c)| c)?;
    if let Some(waiver) = registry.waivers.iter().find(|w| {
        matches!(&w.scope, deacon_conformance::model::Scope::CorpusCase { case, .. }
            if case == case_name)
    }) {
        return Some(waiver.id.clone());
    }
    // A behavior that records the divergence on its own axes IS the characterization.
    let behaviors: BTreeSet<&str> = case_ids
        .iter()
        .filter_map(|id| cases.get(id.as_str()))
        .flat_map(|c| c.behaviors.iter().map(String::as_str))
        .collect();
    registry
        .behaviors
        .iter()
        .find(|b| {
            behaviors.contains(b.id.as_str())
                && b.reference == deacon_conformance::model::ReferenceStatus::Divergent
        })
        .map(|b| b.id.clone())
}

/// Write `target/parity/equivalence.json` atomically.
fn write_ledger(ledger: &EquivalenceLedger) -> Result<(), HarnessError> {
    let dir = report_root();
    std::fs::create_dir_all(&dir).map_err(|e| HarnessError::Report {
        cause: format!("could not create {}: {e}", dir.display()),
    })?;
    let path = dir.join("equivalence.json");
    let mut body = serde_json::to_string_pretty(ledger).map_err(|e| HarnessError::Report {
        cause: format!("could not render the ledger: {e}"),
    })?;
    body.push('\n');
    deacon_conformance::atomic_write(&path, &body).map_err(|e| HarnessError::Report {
        cause: format!("could not write {}: {e}", path.display()),
    })?;
    eprintln!("wrote {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(case: &str, outcome: LegacyOutcome) -> CaseResult {
        CaseResult {
            case: case.to_string(),
            outcome,
            cause: None,
            waivers_applied: Vec::new(),
            diff_summary: None,
            raw: parity_harness::report::RawPaths {
                deacon_stdout: String::new(),
                deacon_stderr: String::new(),
                oracle_stdout: String::new(),
                oracle_stderr: String::new(),
            },
        }
    }

    #[test]
    fn a_legacy_outcome_reduces_to_the_shared_comparison_vocabulary() {
        // The bin never re-implements the legacy comparison: it reads the carrier's own
        // `CaseResult` and reduces it through the SAME classifier the declarative side
        // goes through, so a relation can never depend on which path produced the word.
        for (outcome, expected) in [
            (LegacyOutcome::Pass, ComparisonOutcome::Clean),
            (LegacyOutcome::PassWaived, ComparisonOutcome::Clean),
            (LegacyOutcome::Fail, ComparisonOutcome::Difference),
        ] {
            let name = legacy_outcome_name(&result("c", outcome));
            assert_eq!(ComparisonOutcome::from_outcome_name(name), expected);
        }
    }

    #[test]
    fn a_waived_legacy_pass_is_clean_not_a_difference() {
        // A characterized difference is a decision already made and reviewed, not a
        // difference left undetected — so it must not read as one.
        assert_eq!(
            ComparisonOutcome::from_outcome_name(legacy_outcome_name(&result(
                "c",
                LegacyOutcome::PassWaived
            ))),
            ComparisonOutcome::Clean
        );
    }
}
