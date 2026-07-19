//! Run-report aggregation + the six-condition completeness gate (research D8;
//! FR-016, FR-018, FR-022; contracts/report-schema.md).
//!
//! nextest writes ONE [`crate::report::ReportFragment`] per live parity binary
//! under `<report_root>/report/<binary>.json`. This module folds every fragment
//! into a single [`AggregatedReport`] (`<report_root>/parity-report.json`) and
//! decides whether the run *actually certified* parity against the pinned oracle.
//! It is the only place that can prove execution completeness: a test inside the
//! run cannot know the run's own final selection, but the aggregator can compare
//! the fragments that exist against the registry that enumerates what MUST exist.
//!
//! The gate (all must hold for a zero exit — contracts/report-schema.md):
//!
//! 1. a fragment exists for every registry `live_binaries` entry (proves
//!    execution, FR-022);
//! 2. every fragment's `oracle.version` equals the pin and all fragments agree on
//!    `oracle.path`;
//! 3. `totals.failed == 0` and every `omitted` case carries a reason;
//! 4. no loaded waiver went unused (`stale_waivers == []`, FR-011);
//! 5. every corpus met its registry `min_cases` (FR-024);
//! 6. the report file itself was written successfully (FR-018).
//!
//! Any gap is a human-readable violation string; the aggregator enumerates all of
//! them and exits nonzero. The structured summary is written regardless so the CI
//! artifact records the incomplete run.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::HarnessError;
use crate::oracle::OraclePin;
use crate::registry::{LiveKind, ParityRegistry};
use crate::report::{Outcome, ReportFragment};
use crate::waiver::WaiverSet;

/// The `oracle` block of the aggregated report (contracts/report-schema.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AggregatedOracle {
    /// The authoritative pin, echoed verbatim.
    pub pin: OraclePin,
    /// The version every fragment ran against (equal to the pin on a clean run).
    /// Empty when no fragment was produced.
    pub verified_version: String,
    /// The oracle binary path every fragment agreed on. Empty when no fragment
    /// was produced.
    pub path: String,
}

/// One live binary's rolled-up per-outcome counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BinarySummary {
    pub binary: String,
    pub cases: usize,
    pub passed: usize,
    pub waived: usize,
    pub failed: usize,
    pub omitted: usize,
}

/// Run-wide totals across every present live-binary fragment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Totals {
    pub cases: usize,
    pub passed: usize,
    pub waived: usize,
    pub failed: usize,
    pub omitted: usize,
}

/// The aggregated run report (`target/parity/parity-report.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AggregatedReport {
    pub oracle: AggregatedOracle,
    pub binaries: Vec<BinarySummary>,
    pub missing_fragments: Vec<String>,
    pub stale_waivers: Vec<String>,
    pub totals: Totals,
}

/// The outcome of a full aggregation pass: the structured report, the (possibly
/// empty) list of gate violations, and where the report was (or would be) written.
#[derive(Debug, Clone)]
pub struct Aggregation {
    pub report: AggregatedReport,
    /// Empty iff the run certified parity (zero exit); otherwise every gap is a
    /// human-readable, enumerated line for the failure message.
    pub violations: Vec<String>,
    pub report_path: PathBuf,
}

/// Evaluate the six gate conditions against already-loaded inputs (pure — no IO).
///
/// Returns the structured [`AggregatedReport`] and every gate violation found.
/// Gate 6 (report writability) is enforced by [`write_report`]/[`run`], not here.
pub fn evaluate(
    registry: &ParityRegistry,
    pin: &OraclePin,
    fragments: &[ReportFragment],
    waivers: &WaiverSet,
) -> (AggregatedReport, Vec<String>) {
    let mut violations = Vec::new();

    // Index the fragments actually produced by their binary name.
    let by_name: HashMap<&str, &ReportFragment> =
        fragments.iter().map(|f| (f.binary.as_str(), f)).collect();

    let mut binaries = Vec::new();
    let mut missing_fragments = Vec::new();
    let mut totals = Totals {
        cases: 0,
        passed: 0,
        waived: 0,
        failed: 0,
        omitted: 0,
    };
    let mut consumed_waivers: HashSet<&str> = HashSet::new();
    let mut versions: BTreeSet<String> = BTreeSet::new();
    let mut paths: BTreeSet<String> = BTreeSet::new();

    // Registry order gives the report a stable, reviewable binary ordering.
    for live in &registry.live_binaries {
        let name = live.name.as_str();
        let Some(fragment) = by_name.get(name) else {
            // Gate 1: a registered live binary produced no fragment → it never ran.
            missing_fragments.push(name.to_string());
            violations.push(format!(
                "gate 1 (execution completeness): no report fragment for live binary `{name}` \
                 — it was not executed under `--profile parity`"
            ));
            continue;
        };

        // Gate 2 inputs: record the oracle each fragment certified against.
        versions.insert(fragment.oracle.version.clone());
        paths.insert(fragment.oracle.path.clone());
        if fragment.oracle.version != pin.version {
            violations.push(format!(
                "gate 2 (oracle version): binary `{name}` ran against oracle version {found}, \
                 but the pin requires {required}",
                found = fragment.oracle.version,
                required = pin.version,
            ));
        }

        // Roll up per-case outcomes (gate 3) and collect consumed waivers (gate 4).
        let mut summary = BinarySummary {
            binary: name.to_string(),
            cases: fragment.cases.len(),
            passed: 0,
            waived: 0,
            failed: 0,
            omitted: fragment.omitted.len(),
        };
        for case in &fragment.cases {
            for id in &case.waivers_applied {
                consumed_waivers.insert(id.as_str());
            }
            match case.outcome {
                Outcome::Pass => summary.passed += 1,
                Outcome::PassWaived => summary.waived += 1,
                Outcome::Fail => {
                    summary.failed += 1;
                    let cause = case
                        .cause
                        .map(|c| format!("{c:?}"))
                        .unwrap_or_else(|| "unspecified".to_string());
                    let detail = case
                        .diff_summary
                        .as_deref()
                        .map(|d| format!(": {d}"))
                        .unwrap_or_default();
                    violations.push(format!(
                        "gate 3 (zero failures): `{name}` case `{case}` failed ({cause}){detail}",
                        case = case.case,
                    ));
                }
            }
        }

        // Gate 3: an omission without a reason is an unexplained gap.
        for omission in &fragment.omitted {
            if omission.reason.trim().is_empty() {
                violations.push(format!(
                    "gate 3 (explained omissions): `{name}` omitted case `{case}` without a reason",
                    case = omission.case,
                ));
            }
        }

        // Gate 5: a corpus binary must have discovered at least its registered
        // minimum number of cases (the fragment records one entry per case).
        if live.kind == LiveKind::Corpus {
            if let Some(corpus_id) = live.corpus.as_deref() {
                match registry.corpus(corpus_id) {
                    Some(corpus) if fragment.cases.len() < corpus.min_cases => {
                        violations.push(format!(
                            "gate 5 (corpus minimum): binary `{name}` reported {found} case(s) for \
                             corpus `{corpus_id}`, below the registered minimum of {min}",
                            found = fragment.cases.len(),
                            min = corpus.min_cases,
                        ));
                    }
                    Some(_) => {}
                    None => violations.push(format!(
                        "gate 5 (corpus minimum): binary `{name}` references corpus `{corpus_id}`, \
                         which is not declared in the registry"
                    )),
                }
            }
        }

        totals.cases += summary.cases;
        totals.passed += summary.passed;
        totals.waived += summary.waived;
        totals.failed += summary.failed;
        totals.omitted += summary.omitted;
        binaries.push(summary);
    }

    // Gate 4: any loaded waiver never referenced by a fragment case is stale —
    // it silently narrows coverage. (The per-runner staleness check catches the
    // in-scope case; this is the global cross-runner backstop.)
    let mut stale_waivers: Vec<String> = waivers
        .records()
        .iter()
        .filter(|w| !consumed_waivers.contains(w.id.as_str()))
        .map(|w| w.id.clone())
        .collect();
    stale_waivers.sort();
    for id in &stale_waivers {
        violations.push(format!(
            "gate 4 (no stale waivers): waiver `{id}` was loaded but never applied by any case \
             — remove or update the record"
        ));
    }

    // Gate 2: all fragments must agree on the oracle path.
    if paths.len() > 1 {
        violations.push(format!(
            "gate 2 (oracle path agreement): fragments disagree on the oracle binary path: {:?}",
            paths.iter().collect::<Vec<_>>()
        ));
    }

    // The reported verified version/path reflect reality: the agreed value when
    // fragments concur, else the lexicographically-first observed value (any
    // disagreement is already enumerated as a violation, and empty when no
    // fragment was produced).
    let verified_version = versions.iter().next().cloned().unwrap_or_default();
    let path = paths.iter().next().cloned().unwrap_or_default();

    let report = AggregatedReport {
        oracle: AggregatedOracle {
            pin: pin.clone(),
            verified_version,
            path,
        },
        binaries,
        missing_fragments,
        stale_waivers,
        totals,
    };
    (report, violations)
}

/// Read every `<report_root>/report/*.json` fragment. A missing `report/`
/// directory yields an empty vector (the aggregator then reports every live
/// binary as a missing fragment); a present-but-unparseable fragment is a hard
/// [`HarnessError::Report`] (a corrupt fragment cannot be silently ignored).
pub fn read_fragments(report_root: &Path) -> Result<Vec<ReportFragment>, HarnessError> {
    let dir = report_root.join("report");
    let rd = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(HarnessError::Report {
                cause: format!("could not read report fragment directory {dir:?}: {e}"),
            });
        }
    };

    let mut paths: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    paths.sort();

    let mut fragments = Vec::with_capacity(paths.len());
    for path in paths {
        let raw = std::fs::read_to_string(&path).map_err(|e| HarnessError::Report {
            cause: format!("could not read report fragment {path:?}: {e}"),
        })?;
        let fragment: ReportFragment =
            serde_json::from_str(&raw).map_err(|e| HarnessError::Report {
                cause: format!("malformed report fragment {path:?}: {e}"),
            })?;
        fragments.push(fragment);
    }
    Ok(fragments)
}

/// Atomically write the aggregated report to `<report_root>/parity-report.json`,
/// returning the path written. A write failure is [`HarnessError::Report`] and is
/// gate 6 — a run whose result cannot be recorded is not a passing run.
pub async fn write_report(
    report_root: &Path,
    report: &AggregatedReport,
) -> Result<PathBuf, HarnessError> {
    let mut bytes = serde_json::to_vec_pretty(report).map_err(|e| HarnessError::Report {
        cause: format!("could not serialize aggregated report: {e}"),
    })?;
    bytes.push(b'\n');
    let path = report_root.join("parity-report.json");
    crate::atomic_write(&path, &bytes).await?;
    Ok(path)
}

/// The full aggregation pipeline the `parity-report` bin runs: load waivers from
/// the conformance registry, read fragments from the report root, evaluate the
/// gate, and write the report. Gate 6 (report writability) is folded into the
/// returned violations so a write failure still fails the run with an enumerated
/// message.
pub async fn run(
    report_root: &Path,
    registry_root: &Path,
    registry: &ParityRegistry,
    pin: &OraclePin,
) -> Result<Aggregation, HarnessError> {
    let waivers = WaiverSet::load(registry_root)?;
    let fragments = read_fragments(report_root)?;
    let (report, mut violations) = evaluate(registry, pin, &fragments, &waivers);

    let report_path = match write_report(report_root, &report).await {
        Ok(path) => path,
        Err(e) => {
            violations.push(format!("gate 6 (report writable): {e}"));
            report_root.join("parity-report.json")
        }
    };

    Ok(Aggregation {
        report,
        violations,
        report_path,
    })
}
