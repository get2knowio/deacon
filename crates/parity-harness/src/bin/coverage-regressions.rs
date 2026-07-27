//! `coverage-regressions` — the injected-regression harness
//! (024-deterministic-conformance-coverage US6, T137; contract regression-harness.md).
//!
//! `cargo run -p parity-harness --bin coverage-regressions [--channel <id>]… [--record <id>]…`
//!
//! Proves each observable channel is **live**: that a difference visible on that channel
//! turns the suite red. For every `reg-` record in `conformance/registry/regressions.json`
//! it runs each candidate case twice — once clean, once with the record's perturbation
//! applied to the RAW captured evidence — and classifies the record `detected` when a case
//! that was clean before fails **on that record's channel** after.
//!
//! It writes the byte-stable `target/conformance/regressions.json` and **exits non-zero on
//! any inert channel** (FR-067). An inert channel is a failure, not a warning: this run is
//! the only thing standing between a dead channel and a trusted green suite.
//!
//! This is the ONLY program that can apply a regression. It takes out the process-level
//! injection capability ([`RegressionHarness::declare`]) that
//! [`parity_harness::inject::activate`] requires; the ordinary conformance drivers never
//! do, so `parity_conformance_runner` / `parity_conformance_docker` are structurally unable
//! to perturb their own evidence (FR-070, asserted in `injection_faults.rs`).
//!
//! Dev-only (Constitution II): never a shipped `deacon` subcommand.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use deacon_conformance::load::Registry;
use deacon_conformance::model::{CaseKind, TestCase};
use deacon_conformance::regression::RegressionRecord;
use deacon_conformance::{default_registry_dir, workspace_root};

use parity_harness::evidence::{CaseVerdict, Outcome};
use parity_harness::inject::{
    RecordResult, RegressionHarness, RegressionReport, RegressionVerdict, activate, detects,
};
use parity_harness::oracle::Oracle;
use parity_harness::prereq::{deacon_binary, require_docker};
use parity_harness::runner::{RunConfig, run_case};
use parity_harness::{HarnessError, report_root};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let selection = match Selection::parse(&args) {
        Ok(selection) => selection,
        Err(message) => return usage(&message),
    };

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("error: could not start async runtime: {e}");
            return ExitCode::from(4);
        }
    };
    match runtime.block_on(run(&selection)) {
        Ok(report) => finish(&report, &selection),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(4)
        }
    }
}

/// Emit the report and map it to the contract's exit code: `0` when every exercised
/// channel has ≥1 detected record, `1` when any is inert.
fn finish(report: &RegressionReport, selection: &Selection) -> ExitCode {
    for channel in &report.channels {
        let verdict = match channel.verdict {
            RegressionVerdict::Detected => "detected",
            RegressionVerdict::Inert => "INERT",
        };
        eprintln!("{:<28} {verdict}", channel.channel);
        for record in &channel.records {
            for note in &record.notes {
                eprintln!("    note ({}): {note}", record.id);
            }
        }
    }
    if report.inert_count == 0 {
        eprintln!(
            "every exercised channel is live ({} channel(s), inertCount: 0)",
            report.channels.len()
        );
    } else {
        eprintln!(
            "INERT CHANNEL(S): {}. A channel no regression can make fail proves nothing, and \
             a green suite that rests on it is unearned (FR-067). Fix the perturbation, or \
             strengthen the assertions of the cases that observe the channel.",
            report.inert_channels().join(", ")
        );
    }

    // A FILTERED run must not overwrite the full report: a partial document at the
    // canonical path would read as a complete run that happened to exercise one channel.
    if selection.is_filtered() {
        eprintln!(
            "note: this run was filtered, so `{}` was left untouched — only an unfiltered \
             run may write the canonical report",
            report_path().display()
        );
    } else if let Err(e) = write_report(report) {
        eprintln!("error: {e}");
        return ExitCode::from(4);
    }

    // The contract's exit rule lives on the report, so the acceptance test asserts the
    // same decision this bin makes rather than a restatement of it.
    ExitCode::from(report.exit_status())
}

/// `target/conformance/regressions.json` — git-ignored, byte-stable.
fn report_path() -> PathBuf {
    workspace_root()
        .join("target")
        .join("conformance")
        .join("regressions.json")
}

/// Write the report atomically (temp file + rename), like every other harness artifact.
fn write_report(report: &RegressionReport) -> Result<(), HarnessError> {
    let path = report_path();
    let rendered = report.render()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| HarnessError::Report {
            cause: format!("could not create {}: {e}", parent.display()),
        })?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, rendered.as_bytes()).map_err(|e| HarnessError::Report {
        cause: format!("could not write {}: {e}", temp.display()),
    })?;
    std::fs::rename(&temp, &path).map_err(|e| HarnessError::Report {
        cause: format!("could not rename {} into place: {e}", temp.display()),
    })?;
    eprintln!("wrote {}", path.display());
    Ok(())
}

/// Which records this invocation exercises.
#[derive(Debug, Default)]
struct Selection {
    channels: Vec<String>,
    records: Vec<String>,
}

impl Selection {
    fn parse(args: &[String]) -> Result<Selection, String> {
        let mut selection = Selection::default();
        let mut i = 0;
        while i < args.len() {
            let value = |i: usize| {
                args.get(i + 1)
                    .cloned()
                    .ok_or_else(|| format!("{} requires a value", args[i]))
            };
            match args[i].as_str() {
                "--channel" => selection.channels.push(value(i)?),
                "--record" => selection.records.push(value(i)?),
                other => return Err(format!("unknown argument {other:?}")),
            }
            i += 2;
        }
        Ok(selection)
    }

    fn is_filtered(&self) -> bool {
        !self.channels.is_empty() || !self.records.is_empty()
    }

    fn admits(&self, record: &RegressionRecord) -> bool {
        (self.channels.is_empty() || self.channels.contains(&record.channel))
            && (self.records.is_empty() || self.records.contains(&record.id))
    }
}

fn usage(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    eprintln!("usage: coverage-regressions [--channel <chan-id>]... [--record <reg-id>]...");
    ExitCode::from(2)
}

/// Run the selected records and produce the report.
async fn run(selection: &Selection) -> Result<RegressionReport, HarnessError> {
    // Fail-loud prerequisites BEFORE anything runs — never a silent skip (contract
    // regression-harness.md, "Isolation and safety").
    let oracle = Oracle::acquire().await?;
    require_docker().await?;
    let deacon = deacon_binary().await?;

    // Take out the injection capability. Everything below this line may perturb evidence;
    // nothing above it — and nothing in any other program — can (FR-070).
    let _capability = RegressionHarness::declare();

    let registry =
        Registry::load(&default_registry_dir()).map_err(|e| HarnessError::FixtureMissing {
            path: default_registry_dir().join(format!("<load failed: {e}>")),
        })?;

    let mut records: Vec<&RegressionRecord> = registry
        .regressions
        .iter()
        .filter(|r| selection.admits(r))
        .collect();
    records.sort_by(|a, b| a.id.cmp(&b.id));
    if records.is_empty() {
        return Err(HarnessError::Report {
            cause: format!(
                "no regression record matched the selection ({} record(s) declared) — a run \
                 that exercises nothing must not report success",
                registry.regressions.len()
            ),
        });
    }

    let cases: BTreeMap<&str, &TestCase> = registry
        .cases
        .iter()
        .filter(|c| matches!(c.classify(), Ok(CaseKind::Declarative)))
        .map(|c| (c.id.as_str(), c))
        .collect();

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

    // Baselines are per CASE, not per record: several records legitimately share a case
    // (one Docker `up` exercises four channels), and re-running it once per record would
    // multiply the tier's cost for no additional evidence.
    let mut baselines: BTreeMap<String, Vec<(String, Outcome)>> = BTreeMap::new();
    let mut results: Vec<(String, RecordResult)> = Vec::new();

    for record in records {
        eprintln!("regression {} ({})", record.id, record.channel);
        let mut detected_by: Vec<String> = Vec::new();
        let mut notes: Vec<String> = Vec::new();

        for case_id in &record.expected_detecting_cases {
            let Some(case) = cases.get(case_id.as_str()) else {
                // V30 refuses this statically; the harness still refuses to run past it
                // rather than quietly shrink the candidate set.
                return Err(HarnessError::FixtureMissing {
                    path: default_registry_dir().join(format!("<unknown case {case_id}>")),
                });
            };

            let baseline = match baselines.get(case_id) {
                Some(cached) => cached.clone(),
                None => {
                    let verdict = run_case(case, &cfg).await?;
                    let channels = channel_outcomes(&verdict);
                    baselines.insert(case_id.clone(), channels.clone());
                    channels
                }
            };
            let before = outcome_for(&baseline, &record.channel);
            if !matches!(before, Some(Outcome::Agree | Outcome::AllowedDifference)) {
                // Attribution matters (contract regression-harness.md, "Verdicts"): a case
                // that was already failing on this channel cannot show that WE caused the
                // failure. Surfaced rather than swallowed, so an undetected record is never
                // mysterious.
                notes.push(format!(
                    "{case_id}: baseline is {} on {} — a case that was already not clean on \
                     the channel cannot attribute a perturbed failure to the injection",
                    before.map_or("not verdicted", Outcome::as_str),
                    record.channel
                ));
                continue;
            }

            // Arm the regression for exactly one case run. The guard reverts every
            // filesystem change on the way out — success or unwind (FR-066).
            let guard = activate(record)?;
            let perturbed = run_case(case, &cfg).await;
            let applied = guard.applied_count();
            guard.finish()?;
            let perturbed = perturbed?;

            if applied == 0 {
                // A perturbation that never landed is a HARNESS fault, not an `inert`
                // channel: `inert` is a claim about the channel, and this says nothing
                // about it.
                return Err(HarnessError::InjectionInapplicable {
                    record: record.id.clone(),
                    cause: format!(
                        "the perturbation was never applied while running case {case_id:?}"
                    ),
                });
            }

            let after = outcome_for(&channel_outcomes(&perturbed), &record.channel);
            if detects(before, after) {
                detected_by.push(case_id.clone());
            } else {
                notes.push(format!(
                    "{case_id}: {} stayed {} under the perturbation — the case's assertions do \
                     not distinguish it",
                    record.channel,
                    after.map_or("not verdicted", Outcome::as_str)
                ));
            }
        }

        results.push((
            record.channel.clone(),
            RecordResult {
                id: record.id.clone(),
                detected_by,
                notes,
            },
        ));
    }

    Ok(RegressionReport::build(results))
}

/// A case verdict's `(channel, outcome)` pairs.
fn channel_outcomes(verdict: &CaseVerdict) -> Vec<(String, Outcome)> {
    verdict
        .channels
        .iter()
        .map(|c| (c.channel.clone(), c.outcome))
        .collect()
}

/// The WORST outcome recorded for `channel` in a verdict, or `None` when the case
/// verdicted nothing on it. Worst-of, because a case may declare the same channel for more
/// than one operation and a divergence on any of them is a divergence on the channel.
fn outcome_for(channels: &[(String, Outcome)], channel: &str) -> Option<Outcome> {
    channels
        .iter()
        .filter(|(id, _)| id == channel)
        .map(|(_, outcome)| *outcome)
        .max_by_key(|o| o.severity())
}
