//! Resource-group drivers for the declarative conformance runner
//! (024-deterministic-conformance-coverage, research Decision 4 / Decision 10).
//!
//! The declarative runner used to be driven by ONE `#[tokio::test]` function iterating
//! every case serially. Three requirements are unsatisfiable against that shape:
//!
//! 1. **FR-077b** — a per-case bound. With one test, a single hung case consumes the whole
//!    lane and reports as one failure naming nothing. (The bound itself lives on
//!    [`crate::runner::run_case`]; this module reports it per case.)
//! 2. **FR-077** — `resourceGroup` is declared data on real cases, and nextest cannot act
//!    on it, because nextest groups per test BINARY/FUNCTION. One function makes the
//!    declaration inert.
//! 3. **FR-077a** — a 30-minute Docker-tier budget needs concurrency, and concurrency needs
//!    to know which cases may share a daemon — which is exactly what `resourceGroup` says.
//!
//! So the case set is partitioned by [`ResourceGroup`] and each group gets its own driver
//! function, across two binaries: `parity_conformance_runner` owns the config-only groups
//! ([`ResourceGroup::None`], [`ResourceGroup::FsHeavy`] — the latter is significant
//! filesystem work, explicitly *not* Docker) and `parity_conformance_docker` owns the
//! Docker-backed ones ([`ResourceGroup::DockerShared`], [`ResourceGroup::DockerExclusive`]).
//!
//! **SC-013 is preserved.** `ResourceGroup` is a CLOSED set and every variant already has a
//! driver, so adding a case with an existing group stays a pure data edit. Only introducing
//! a genuinely new group would need a new function, and that is a deliberate infrastructure
//! change, not case authoring.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::model::{CaseKind, ResourceGroup, TestCase};

use crate::HarnessError;
use crate::evidence::{CaseVerdict, Outcome};
use crate::oracle::VerifiedOracle;
use crate::report::{
    CaseResult, Cause, OracleInfo, RawPaths, ReportFragment, VerdictReport, now_rfc3339,
};
use crate::runner::{CASE_TIMEOUT, RUNNER_BINARY, RunConfig, run_case};

/// The Docker tier's wall-clock budget (FR-077a).
///
/// Research Decision 10: this is asserted EXPLICITLY by the tier's driver, not delegated to
/// nextest's `slow-timeout`. A `slow-timeout` failure reports "the binary was slow", which
/// is indistinguishable from a wedged daemon and names nothing to fix; an explicit assertion
/// reports the number and the case list that produced it, which is what a maintainer needs
/// in order to choose between tightening applicability rules and splitting the tier.
/// Exceeding it is a failure of acceptance, not a reason to widen it — so the number has to
/// be visible to be argued with.
pub const TIER_BUDGET: Duration = Duration::from_secs(30 * 60);

/// How many slow cases a budget-violation message names.
const SLOWEST_REPORTED: usize = 10;

/// The environment variable nextest stamps with a per-RUN identity, shared by every test
/// process of that run. It is what lets two driver functions — separate processes under
/// nextest — measure ONE tier wall clock without a stale artifact from yesterday's run
/// widening the span to a day.
const NEXTEST_RUN_ID_ENV: &str = "NEXTEST_RUN_ID";

/// A case's effective resource group: the declared one, else [`ResourceGroup::None`]
/// (absence means "no special group", data-model §1).
pub fn group_of(case: &TestCase) -> ResourceGroup {
    case.resource_group.unwrap_or(ResourceGroup::None)
}

/// The stable kebab-case slug for a group — the artifact file name and the label in
/// diagnostics. Matches the serialized form in `cases/<area>.json`.
pub fn group_slug(group: ResourceGroup) -> &'static str {
    match group {
        ResourceGroup::DockerShared => "docker-shared",
        ResourceGroup::DockerExclusive => "docker-exclusive",
        ResourceGroup::FsHeavy => "fs-heavy",
        ResourceGroup::None => "none",
    }
}

/// Whether a group needs the container runtime — i.e. whether its driver belongs in
/// `parity_conformance_docker` rather than `parity_conformance_runner`.
///
/// `fs-heavy` is deliberately NOT Docker: per its model definition it is "significant
/// filesystem operations, no Docker", so it stays with the config-only binary and gets the
/// filesystem group's parallelism, not a daemon.
pub fn needs_docker(group: ResourceGroup) -> bool {
    matches!(
        group,
        ResourceGroup::DockerShared | ResourceGroup::DockerExclusive
    )
}

/// The bounded concurrency a group's driver runs at, mirroring the semantics of the
/// same-named nextest test group in `.config/nextest.toml`:
///
/// - `docker-exclusive` is exclusive daemon access → 1.
/// - `docker-shared` is safe concurrent Docker usage → 4, matching the group's
///   `parallel-4`. Every Docker case runs in its own isolated temp workspace with a unique
///   `devcontainer.local_folder` label, so concurrent cases cannot collide on container,
///   network or volume names.
/// - The config-only groups run at 1. Concurrency there buys little (a
///   `read-configuration` case is a sub-second CLI invocation) and no requirement asks for
///   it, so the existing serial behaviour of that lane is left exactly as it was rather
///   than perturbed as a side effect of this reshape.
pub fn default_concurrency(group: ResourceGroup) -> usize {
    match group {
        ResourceGroup::DockerShared => 4,
        ResourceGroup::DockerExclusive => 1,
        ResourceGroup::FsHeavy | ResourceGroup::None => 1,
    }
}

/// Every declarative case belonging to `group`, in the registry's id-sorted order.
pub fn cases_in_group(cases: &[TestCase], group: ResourceGroup) -> Vec<TestCase> {
    let mut selected: Vec<TestCase> = cases
        .iter()
        .filter(|c| matches!(c.classify(), Ok(CaseKind::Declarative)))
        .filter(|c| group_of(c) == group)
        .cloned()
        .collect();
    selected.sort_by(|a, b| a.id.cmp(&b.id));
    selected
}

/// Owned, `'static` inputs a driver hands to each spawned case task.
///
/// [`RunConfig`] borrows; a `JoinSet` task must be `'static`. Rather than fight the
/// lifetimes, the driver owns one of these behind an [`Arc`] and each task rebuilds the
/// borrowed [`RunConfig`] from it.
#[derive(Debug, Clone)]
pub struct DriverConfig {
    /// The test binary's name — the report-fragment key.
    pub binary: String,
    /// The deacon binary under test (only the test crate can expand `CARGO_BIN_EXE_deacon`).
    pub deacon_path: PathBuf,
    /// The verified pinned oracle (required by `live-differential` cases).
    pub oracle: Option<VerifiedOracle>,
    /// Root under which a fixture id resolves.
    pub fixtures_root: PathBuf,
    /// Root the raw stdout/stderr artifacts and report fragments are written under.
    pub report_root: PathBuf,
    /// Committed-snapshots root.
    pub snapshots_root: PathBuf,
}

impl DriverConfig {
    /// Borrow this owned config as the [`RunConfig`] the runner takes.
    ///
    /// Public so a live test binary can drive a hand-picked slice of the case set through
    /// [`crate::runner::run_case`] directly — the error-path tier's acceptance tests (024
    /// US4) assert per-case properties that [`drive_group`]'s aggregate `GroupRun` does not
    /// expose, and rebuilding an equivalent `RunConfig` there would fork the field list.
    pub fn run_config(&self) -> RunConfig<'_> {
        RunConfig {
            deacon_path: &self.deacon_path,
            oracle: self.oracle.as_ref(),
            fixtures_root: &self.fixtures_root,
            report_root: &self.report_root,
            snapshots_root: &self.snapshots_root,
        }
    }
}

/// What one spawned case task hands back to the driver loop.
struct CaseTaskResult {
    case_id: String,
    /// The case's first operation id — the raw-capture directory suffix.
    first_op: String,
    elapsed: Duration,
    verdict: Result<CaseVerdict, HarnessError>,
}

/// One case's wall clock, recorded so a budget violation can name what produced it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseTiming {
    pub case: String,
    pub millis: u128,
}

/// One group's run: the persisted timing record plus everything the driver reports.
#[derive(Debug)]
pub struct GroupRun {
    /// The group driven.
    pub group: ResourceGroup,
    /// Per-case verdicts, id-sorted (so the emitted report stays deterministic regardless
    /// of the order concurrent tasks completed in).
    pub verdicts: Vec<CaseVerdict>,
    /// Per-case wall clocks, id-sorted.
    pub timings: Vec<CaseTiming>,
    /// Human-readable failures — a non-empty list fails the driver's test.
    pub failures: Vec<String>,
    /// Non-blocking observations surfaced to stderr (never a silent skip).
    pub notes: Vec<String>,
    /// The group's own wall clock.
    pub elapsed: Duration,
}

/// The persisted per-group timing artifact (`<report_root>/tier/<binary>/<run>/<group>.json`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TierTiming {
    pub binary: String,
    pub group: String,
    /// Unix-epoch milliseconds. Epoch integers, not RFC3339 strings, so the span is plain
    /// arithmetic with no date parsing and no ambiguity about clock formats.
    pub started_ms: u128,
    pub finished_ms: u128,
    pub cases: Vec<CaseTiming>,
}

/// The tier's measured wall clock, folded across every group of one binary in one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TierSummary {
    /// `max(finished) - min(started)` across the groups observed — the true tier wall
    /// clock, not the sum of the groups (which would over-count concurrent processes).
    pub elapsed: Duration,
    /// The groups whose timing artifacts were folded in.
    pub groups: Vec<String>,
    /// Every case timing, slowest first.
    pub slowest: Vec<CaseTiming>,
}

/// Drive every declarative case of `group` and produce its [`GroupRun`].
///
/// Concurrency is bounded by a semaphore over a [`tokio::task::JoinSet`]
/// ([`default_concurrency`]). Results are collected in completion order and then id-sorted,
/// so the emitted verdict report is byte-stable regardless of scheduling.
///
/// **Error handling is deliberately two-tier.** A [`HarnessError::CaseTimeout`] is recorded
/// as that case's failure and the group keeps going: that is the entire point of a per-case
/// bound (FR-077b) — one wedged case must not cost the tier its remaining coverage. Every
/// other `HarnessError` (missing oracle, missing fixture, normalization failure) aborts the
/// group, preserving the fail-loud contract, because those recur for every case and
/// continuing would just restate one environmental fault N times.
pub async fn drive_group(
    cfg: Arc<DriverConfig>,
    cases: Vec<TestCase>,
    group: ResourceGroup,
) -> Result<GroupRun, HarnessError> {
    let started = Instant::now();
    let concurrency = default_concurrency(group).max(1);
    let permits = Arc::new(tokio::sync::Semaphore::new(concurrency));
    let mut set: tokio::task::JoinSet<CaseTaskResult> = tokio::task::JoinSet::new();

    for case in cases {
        let cfg = Arc::clone(&cfg);
        let permits = Arc::clone(&permits);
        set.spawn(async move {
            let case_id = case.id.clone();
            // The raw-capture directory a case's FIRST operation wrote to — the fragment's
            // diagnostic pointer. Resolved here, where the case is still in hand.
            let first_op = case
                .operations
                .first()
                .map(|o| o.id.clone())
                .unwrap_or_else(|| "op".to_string());
            let permit = match permits.acquire_owned().await {
                Ok(p) => p,
                // The semaphore is owned by this function and never closed, so this is
                // unreachable in practice — reported as a fault rather than unwrapped,
                // because a runtime path must not panic on an expected-fallible API.
                Err(e) => {
                    return CaseTaskResult {
                        case_id: case_id.clone(),
                        first_op,
                        elapsed: Duration::ZERO,
                        verdict: Err(HarnessError::DockerUnavailable {
                            cause: format!(
                                "the driver's concurrency semaphore closed before case \
                                 {case_id:?} could acquire a permit: {e}"
                            ),
                        }),
                    };
                }
            };
            let case_started = Instant::now();
            let verdict = run_case(&case, &cfg.run_config()).await;
            drop(permit);
            CaseTaskResult {
                case_id,
                first_op,
                elapsed: case_started.elapsed(),
                verdict,
            }
        });
    }

    let mut verdicts: Vec<CaseVerdict> = Vec::new();
    let mut timings: Vec<CaseTiming> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut results: Vec<CaseResult> = Vec::new();

    while let Some(joined) = set.join_next().await {
        let CaseTaskResult {
            case_id,
            first_op,
            elapsed,
            verdict,
        } = joined.map_err(|e| HarnessError::Report {
            cause: format!("a conformance case task failed to complete: {e}"),
        })?;
        timings.push(CaseTiming {
            case: case_id.clone(),
            millis: elapsed.as_millis(),
        });
        match verdict {
            Ok(verdict) => {
                results.push(case_result(
                    &verdict,
                    raw_paths(&verdict.case_id, &first_op),
                ));
                match verdict.overall {
                    Outcome::Agree | Outcome::AllowedDifference => {}
                    // Non-blocking coverage gap: surfaced (never a silent skip) but not a
                    // failure — no snapshot has been recorded for this platform yet.
                    Outcome::NoReferenceForPlatform => notes.push(format!(
                        "{}: no committed snapshot for this platform (no-reference-for-platform)",
                        verdict.case_id
                    )),
                    _ => failures.push(format!("{}: {}", verdict.case_id, summarize(&verdict))),
                }
                verdicts.push(verdict);
            }
            // FR-077b: attributed to the case, reported as that case's failure, group
            // continues.
            Err(HarnessError::CaseTimeout { case, bound }) => {
                let detail = format!("exceeded the {bound:?} per-case bound");
                results.push(CaseResult::fail(
                    case.clone(),
                    Cause::CaseTimeout,
                    Some(detail.clone()),
                    raw_paths(&case, &first_op),
                ));
                failures.push(format!("{case}: {detail}"));
            }
            // Environmental / authoring faults abort the group fail-loud.
            Err(other) => return Err(other),
        }
    }

    verdicts.sort_by(|a, b| a.case_id.cmp(&b.case_id));
    timings.sort_by(|a, b| a.case.cmp(&b.case));
    results.sort_by(|a, b| a.case.cmp(&b.case));
    failures.sort();
    notes.sort();

    let elapsed = started.elapsed();
    let group_run = GroupRun {
        group,
        verdicts,
        timings,
        failures,
        notes,
        elapsed,
    };
    write_fragment(&cfg, results).await?;
    Ok(group_run)
}

/// Emit the deterministic verdict report on stdout (contract runner-cli.md) and the
/// non-blocking notes on stderr.
pub fn emit(run: &GroupRun) -> Result<(), HarnessError> {
    for note in &run.notes {
        eprintln!("note: {note}");
    }
    eprintln!(
        "resource group `{}`: {} case(s) in {:?}",
        group_slug(run.group),
        run.verdicts.len(),
        run.elapsed
    );
    let report = VerdictReport::new(run.verdicts.clone());
    report.emit_stdout()
}

/// Write this group's slice of the binary's run-report fragment.
///
/// Fragments are per-CASE files under `report/<binary>/`, so two driver functions of one
/// binary — separate processes under nextest — write disjoint paths and the aggregator
/// merges them. A group with no cases still writes its `_meta.json`, which is what proves
/// the binary RAN: a registered live binary that produced no fragment at all fails the
/// aggregator's execution-completeness gate, and that must mean "it never ran", not "its
/// groups happened to be empty".
async fn write_fragment(cfg: &DriverConfig, results: Vec<CaseResult>) -> Result<(), HarnessError> {
    // A fragment is the PARITY AGGREGATOR's input, and its identity is the oracle the run
    // compared against — that is what makes two fragments comparable at all. A lane that
    // resolves no reference (026's container pull-request lane, `oracle: None`) has no such
    // identity, and is deliberately absent from `fixtures/parity-corpus/registry.json`, so
    // no execution-completeness gate is waiting on a fragment from it.
    //
    // Skipping is therefore correct, but it is announced rather than silent: a lane that
    // stopped writing a fragment it OWED would otherwise look identical to one that never
    // owed one, and the aggregator's whole value is that a missing fragment means "it never
    // ran". The `oracle: None` in the caller's `DriverConfig` is the declaration; this note
    // is the receipt.
    let Some(oracle) = cfg.oracle.as_ref().map(OracleInfo::from) else {
        eprintln!(
            "note: `{}` resolved no reference oracle, so it writes no parity report \
             fragment — it is not a registered parity binary and no aggregator gate awaits \
             one. Its evidence is the execution manifest instead.",
            cfg.binary
        );
        return Ok(());
    };
    let now = now_rfc3339();
    let fragment = ReportFragment::new(
        cfg.binary.clone(),
        oracle,
        now.clone(),
        now,
        results,
        Vec::new(),
    );
    fragment.write_under(&cfg.report_root).await.map(|_| ())
}

/// Persist this group's timing record so the tier assertion can fold it together with the
/// binary's other groups (which run in sibling nextest processes).
pub async fn record_timing(cfg: &DriverConfig, run: &GroupRun) -> Result<PathBuf, HarnessError> {
    let finished_ms = epoch_millis();
    let record = TierTiming {
        binary: cfg.binary.clone(),
        group: group_slug(run.group).to_string(),
        started_ms: finished_ms.saturating_sub(run.elapsed.as_millis()),
        finished_ms,
        cases: run.timings.clone(),
    };
    let mut bytes = serde_json::to_vec_pretty(&record).map_err(|e| HarnessError::Report {
        cause: format!("could not serialize the tier timing record: {e}"),
    })?;
    bytes.push(b'\n');
    let path = tier_dir(&cfg.report_root, &cfg.binary).join(format!("{}.json", record.group));
    crate::atomic_write(&path, &bytes).await?;
    Ok(path)
}

/// The per-run tier-timing directory for `binary`.
///
/// Scoped by the nextest RUN id so a timing artifact left by an earlier run cannot widen
/// this run's measured span. Outside nextest (the profile is the only sanctioned entry
/// point) the process id is used instead, which means a process folds in only its OWN
/// group — a floor on the tier wall clock, never an inflation of it.
fn tier_dir(report_root: &Path, binary: &str) -> PathBuf {
    let run = std::env::var(NEXTEST_RUN_ID_ENV)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| format!("pid-{}", std::process::id()));
    report_root.join("tier").join(binary).join(sanitize(&run))
}

/// Keep a run id to path-safe characters (it is an external value used as a directory name).
fn sanitize(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Fold every timing artifact this run has written for `binary` into the tier's measured
/// wall clock.
pub fn tier_summary(report_root: &Path, binary: &str) -> Result<TierSummary, HarnessError> {
    let dir = tier_dir(report_root, binary);
    let mut records: BTreeMap<String, TierTiming> = BTreeMap::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let raw = std::fs::read_to_string(&path).map_err(|e| HarnessError::Report {
                cause: format!("could not read the tier timing record {path:?}: {e}"),
            })?;
            let record: TierTiming =
                serde_json::from_str(&raw).map_err(|e| HarnessError::Report {
                    cause: format!("tier timing record {path:?} is malformed: {e}"),
                })?;
            records.insert(record.group.clone(), record);
        }
    }
    Ok(summarize_timings(records.into_values()))
}

/// The pure fold behind [`tier_summary`] — separated so it is unit-testable without a
/// filesystem.
fn summarize_timings(records: impl IntoIterator<Item = TierTiming>) -> TierSummary {
    let mut earliest: Option<u128> = None;
    let mut latest: Option<u128> = None;
    let mut groups: Vec<String> = Vec::new();
    let mut slowest: Vec<CaseTiming> = Vec::new();

    for record in records {
        earliest = Some(earliest.map_or(record.started_ms, |e: u128| e.min(record.started_ms)));
        latest = Some(latest.map_or(record.finished_ms, |l: u128| l.max(record.finished_ms)));
        groups.push(record.group);
        slowest.extend(record.cases);
    }
    groups.sort();
    // Slowest first; ties broken by case id so the message is deterministic.
    slowest.sort_by(|a, b| b.millis.cmp(&a.millis).then_with(|| a.case.cmp(&b.case)));

    let span = match (earliest, latest) {
        (Some(start), Some(end)) => u64::try_from(end.saturating_sub(start)).unwrap_or(u64::MAX),
        _ => 0,
    };
    TierSummary {
        elapsed: Duration::from_millis(span),
        groups,
        slowest,
    }
}

/// The budget-violation message for `summary`, or `None` when the tier is within budget.
///
/// The message carries the measured elapsed time, the groups it was measured across, and
/// the slowest cases — the three things a maintainer needs to decide between tightening
/// applicability rules and splitting the tier (research Decision 10). A bare "too slow"
/// would name none of them.
pub fn budget_violation(summary: &TierSummary, budget: Duration) -> Option<String> {
    if summary.elapsed <= budget {
        return None;
    }
    let slowest = summary
        .slowest
        .iter()
        .take(SLOWEST_REPORTED)
        .map(|t| {
            format!(
                "  {} — {:?}",
                t.case,
                Duration::from_millis(t.millis as u64)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Some(format!(
        "the Docker conformance tier took {:?}, exceeding its {:?} budget (FR-077a).\n\
         Measured across group(s): {}.\n\
         Slowest case(s):\n{}\n\
         Exceeding the budget is a failure of acceptance, not a reason to widen it: tighten \
         the applicability rules that admit these cases, or split the tier.",
        summary.elapsed,
        budget,
        summary.groups.join(", "),
        if slowest.is_empty() {
            "  (no per-case timings recorded)".to_string()
        } else {
            slowest
        },
    ))
}

/// Unix-epoch milliseconds. A clock before the epoch yields 0 rather than panicking.
fn epoch_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// The report-relative raw paths for a case's FIRST operation (diagnostic pointers for the
/// fragment). The runner writes raw capture under
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
        // exit-code contract and with certify (which surfaces it as non-blocking info).
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

/// The per-case bound the drivers run under, re-exported so a driver's diagnostics can
/// name the same number the runner enforces.
pub const fn case_bound() -> Duration {
    CASE_TIMEOUT
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Operation, OracleType};

    fn declarative(id: &str, group: Option<ResourceGroup>) -> TestCase {
        TestCase {
            id: id.to_string(),
            oracle_type: Some(OracleType::SpecExpectation),
            operations: vec![Operation {
                id: "op-1".to_string(),
                subcommand: "read-configuration".to_string(),
                ..Operation::default()
            }],
            resource_group: group,
            ..TestCase::default()
        }
    }

    /// Every `ResourceGroup` variant is owned by exactly one binary, and the two sets
    /// partition the enum. If a variant were added with no driver, cases carrying it would
    /// be silently driven by nobody — the inert-declaration defect this reshape exists to
    /// fix, reintroduced one variant later.
    #[test]
    fn every_resource_group_is_owned_by_exactly_one_binary() {
        const ALL: &[ResourceGroup] = &[
            ResourceGroup::None,
            ResourceGroup::FsHeavy,
            ResourceGroup::DockerShared,
            ResourceGroup::DockerExclusive,
        ];
        let docker: Vec<_> = ALL.iter().copied().filter(|g| needs_docker(*g)).collect();
        let config: Vec<_> = ALL.iter().copied().filter(|g| !needs_docker(*g)).collect();
        assert_eq!(
            docker,
            vec![ResourceGroup::DockerShared, ResourceGroup::DockerExclusive]
        );
        assert_eq!(config, vec![ResourceGroup::None, ResourceGroup::FsHeavy]);
        assert_eq!(docker.len() + config.len(), ALL.len());
    }

    /// `fs-heavy` is filesystem work, NOT Docker — so it belongs to the config-only binary.
    /// Getting this backwards would put a non-Docker group behind a Docker prerequisite and
    /// make the config lane require a daemon it never touches.
    #[test]
    fn fs_heavy_is_not_a_docker_group() {
        assert!(!needs_docker(ResourceGroup::FsHeavy));
        assert!(needs_docker(ResourceGroup::DockerShared));
        assert!(needs_docker(ResourceGroup::DockerExclusive));
    }

    /// An absent `resourceGroup` means `none`, so a case that declares nothing is driven by
    /// the config-only binary rather than dropped.
    #[test]
    fn an_undeclared_group_defaults_to_none_and_is_still_driven() {
        let cases = vec![
            declarative("case-b", None),
            declarative("case-a", Some(ResourceGroup::DockerShared)),
            declarative("case-c", Some(ResourceGroup::None)),
        ];
        let none = cases_in_group(&cases, ResourceGroup::None);
        assert_eq!(
            none.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["case-b", "case-c"],
            "selection is id-sorted and includes the undeclared case"
        );
        assert_eq!(
            cases_in_group(&cases, ResourceGroup::DockerShared)
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            vec!["case-a"]
        );
        // The four groups partition the declarative set — no case is driven twice, and none
        // is driven by nobody.
        let total: usize = [
            ResourceGroup::None,
            ResourceGroup::FsHeavy,
            ResourceGroup::DockerShared,
            ResourceGroup::DockerExclusive,
        ]
        .iter()
        .map(|g| cases_in_group(&cases, *g).len())
        .sum();
        assert_eq!(total, cases.len());
    }

    /// A legacy (binary-backed) record is never driven declaratively, whatever group it
    /// carries.
    #[test]
    fn legacy_cases_are_never_selected() {
        let mut legacy = declarative("case-legacy", Some(ResourceGroup::DockerShared));
        legacy.operations.clear();
        legacy.oracle_type = None;
        legacy.executable = Some(crate::model::Executable {
            binary: "parity_build".to_string(),
            test: None,
            corpus: None,
            case: None,
        });
        assert_eq!(
            cases_in_group(&[legacy], ResourceGroup::DockerShared).len(),
            0
        );
    }

    /// `docker-exclusive` must be serial; `docker-shared` may run four at a time.
    #[test]
    fn concurrency_mirrors_the_group_semantics() {
        assert_eq!(default_concurrency(ResourceGroup::DockerExclusive), 1);
        assert_eq!(default_concurrency(ResourceGroup::DockerShared), 4);
        assert_eq!(default_concurrency(ResourceGroup::None), 1);
        assert_eq!(default_concurrency(ResourceGroup::FsHeavy), 1);
    }

    fn timing(group: &str, started: u128, finished: u128, cases: &[(&str, u128)]) -> TierTiming {
        TierTiming {
            binary: "parity_conformance_docker".to_string(),
            group: group.to_string(),
            started_ms: started,
            finished_ms: finished,
            cases: cases
                .iter()
                .map(|(c, m)| CaseTiming {
                    case: (*c).to_string(),
                    millis: *m,
                })
                .collect(),
        }
    }

    /// The tier wall clock is the SPAN across groups, not their sum: two groups running
    /// concurrently in sibling nextest processes each take 20 minutes but the tier took 20,
    /// and summing would fail a tier that met its budget.
    #[test]
    fn tier_elapsed_is_the_span_not_the_sum() {
        let summary = summarize_timings([
            timing("docker-shared", 1_000, 1_201_000, &[]),
            timing("docker-exclusive", 2_000, 1_200_000, &[]),
        ]);
        assert_eq!(summary.elapsed, Duration::from_millis(1_200_000));
        assert_eq!(summary.groups, vec!["docker-exclusive", "docker-shared"]);
    }

    /// An empty tier directory yields a zero span rather than a panic or a bogus epoch-sized
    /// duration.
    #[test]
    fn an_empty_tier_measures_zero() {
        let summary = summarize_timings([]);
        assert_eq!(summary.elapsed, Duration::ZERO);
        assert!(budget_violation(&summary, TIER_BUDGET).is_none());
    }

    /// A violation names the elapsed time, the groups, and the slowest cases — not just
    /// "too slow" (research Decision 10).
    #[test]
    fn a_budget_violation_names_the_number_and_the_slowest_cases() {
        let summary = summarize_timings([timing(
            "docker-shared",
            0,
            31 * 60 * 1_000,
            &[("case-fast", 1_000), ("case-slow", 900_000)],
        )]);
        let message = budget_violation(&summary, TIER_BUDGET)
            .expect("31 minutes exceeds the 30-minute budget");
        assert!(message.contains("case-slow"), "{message}");
        assert!(message.contains("docker-shared"), "{message}");
        assert!(message.contains("FR-077a"), "{message}");
        // Slowest first.
        assert_eq!(
            summary.slowest.first().map(|t| t.case.as_str()),
            Some("case-slow")
        );
        // And exactly at the budget is NOT a violation.
        let at_budget = summarize_timings([timing("docker-shared", 0, 30 * 60 * 1_000, &[])]);
        assert!(budget_violation(&at_budget, TIER_BUDGET).is_none());
    }

    /// The per-case bound the drivers advertise is the one the runner enforces — a second
    /// constant here would drift from it silently.
    #[test]
    fn the_advertised_case_bound_is_the_runners_bound() {
        assert_eq!(case_bound(), CASE_TIMEOUT);
        assert_eq!(case_bound(), Duration::from_secs(300));
    }
}
