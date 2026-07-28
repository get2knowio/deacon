//! `drift-scan` — observe the pinned upstream sources and record what they currently are
//! (026-continuous-conformance-certification, US4; contracts/cli-drift.md).
//!
//! ## Status reflects whether it ran, never what it found
//!
//! A scan that surfaces all five drift kinds exits `0`. Only an inability to run — an
//! unreachable upstream, an unresolvable pin, an unwritable artifact location, or an
//! attempted out-of-scope write — is non-zero (FR-026).
//!
//! This is the same rule the discovery lane follows, for the same reason: a
//! finding-dependent status becomes a gate the moment someone wires it into a required
//! check, and upstream moving is not a defect in this repository. A lane that went red
//! whenever the specification advanced would be a gate on someone else's release schedule.
//!
//! ## It blesses nothing
//!
//! The only writes are `conformance/drift/observations.json` and `target/drift/*`,
//! enforced by [`parity_harness::drift::write_drift_artifact`] before anything is
//! published. An observation records *what upstream looks like*; the pin — *what deacon is
//! pinned to* — stays in the registry and remains a human decision (FR-024, FR-028).

use std::process::ExitCode;

use deacon_conformance::drift::{CompletedRun, DriftFile, DriftKind, load_drift};
use deacon_conformance::{default_registry_dir, drift_dir_for, workspace_root};

use parity_harness::drift::scan::{Pins, ProbeResult, probe};
use parity_harness::drift::write_drift_artifact;

fn main() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("error: could not start the async runtime: {e}");
            return ExitCode::from(2);
        }
    };
    runtime.block_on(run())
}

async fn run() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let write = args.iter().any(|a| a == "--write");
    let kinds = match selected_kinds(&args) {
        Ok(kinds) => kinds,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::from(2);
        }
    };
    let today = match args.iter().position(|a| a == "--today") {
        Some(i) => args.get(i + 1).cloned().unwrap_or_default(),
        None => {
            // Injected rather than read from the clock wherever possible; the fallback
            // keeps a manual run usable without making the artifact non-reproducible for
            // the scheduled one, which always passes `--today`.
            eprintln!("note: no --today given; observations will carry an empty date");
            String::new()
        }
    };

    let root = workspace_root();
    let pins = match load_pins() {
        Ok(pins) => pins,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::from(2);
        }
    };

    let mut observations = Vec::new();
    let mut probed = Vec::new();
    for kind in &kinds {
        match probe(*kind, &pins, &root, &today).await {
            Ok(ProbeResult::Unchanged) => {
                probed.push(*kind);
                eprintln!("unchanged: {}", kind.as_str());
            }
            Ok(ProbeResult::Drifted(observation)) => {
                eprintln!(
                    "drift: {} `{}` → `{}`",
                    kind.as_str(),
                    observation.pinned_revision,
                    observation.observed_revision
                );
                probed.push(*kind);
                observations.push(observation);
            }
            Err(e) => {
                // A probe that could not run is a machinery failure. Reporting it as
                // "unchanged" would be the exact defect `lastCompletedRun` exists to make
                // impossible — silence read as reassurance.
                eprintln!("error: {kind:?} probe could not run: {e}");
                return ExitCode::from(1);
            }
        }
    }

    // `lastCompletedRun` is recorded ONLY over the kinds that actually completed. A partial
    // run therefore reads as partial (V33), and an empty `records` alongside it can never
    // be mistaken for "no drift" (FR-025).
    let file = DriftFile {
        schema_version: 1,
        records: observations,
        last_completed_run: Some(CompletedRun {
            date: today,
            kinds_probed: probed,
        }),
    };

    let rendered = match serde_json::to_string_pretty(&file) {
        Ok(mut text) => {
            text.push('\n');
            text
        }
        Err(e) => {
            eprintln!("error: could not serialize observations: {e}");
            return ExitCode::from(2);
        }
    };

    let scan_artifact = root.join("target").join("drift").join("scan.json");
    if let Err(e) = write_drift_artifact(&root, &scan_artifact, &rendered) {
        eprintln!("error: {e}");
        return ExitCode::from(1);
    }

    if write {
        let committed = drift_dir_for(&default_registry_dir()).join("observations.json");
        if let Err(e) = write_drift_artifact(&root, &committed, &rendered) {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
        eprintln!("wrote {}", committed.display());
    } else {
        eprintln!(
            "note: --write not given; committed observations at {} left untouched",
            drift_dir_for(&default_registry_dir())
                .join("observations.json")
                .display()
        );
        // Reading the committed file is harmless and tells the operator whether what they
        // just observed differs from what is recorded.
        if let Ok(existing) = load_drift(&drift_dir_for(&default_registry_dir())) {
            if existing.records.len() != file.records.len() {
                eprintln!(
                    "note: committed observations record {} signal(s); this scan found {}",
                    existing.records.len(),
                    file.records.len()
                );
            }
        }
    }

    // Zero regardless of what was found. Only the failures above are non-zero.
    ExitCode::SUCCESS
}

/// The kinds this run probes: every kind, or the `--kinds a,b` subset.
fn selected_kinds(args: &[String]) -> Result<Vec<DriftKind>, String> {
    let Some(index) = args.iter().position(|a| a == "--kinds") else {
        return Ok(DriftKind::ALL.to_vec());
    };
    let list = args
        .get(index + 1)
        .ok_or_else(|| "--kinds needs a comma-separated list".to_string())?;
    let mut kinds = Vec::new();
    for name in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let kind = DriftKind::ALL
            .iter()
            .find(|k| k.as_str() == name)
            .ok_or_else(|| {
                format!(
                    "unknown drift kind `{name}`; valid kinds: {}",
                    DriftKind::ALL
                        .iter()
                        .map(|k| k.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;
        kinds.push(*kind);
    }
    Ok(kinds)
}

/// Read the pins a scan compares against from the registry's revision records.
///
/// An unresolvable pin is a machinery failure, not a finding: a scan that cannot say what
/// it is comparing against has nothing to report.
fn load_pins() -> Result<Pins, String> {
    let registry = deacon_conformance::load::Registry::load(&default_registry_dir())
        .map_err(|e| format!("could not load the registry: {e}"))?;
    let pin = |kind: deacon_conformance::model::RevisionKind| -> Option<String> {
        registry
            .revisions
            .iter()
            .find(|r| r.kind == kind)
            .map(|r| r.pin.clone())
    };
    use deacon_conformance::model::RevisionKind;
    Ok(Pins {
        spec: pin(RevisionKind::Spec).ok_or("no `spec` revision record")?,
        schema: pin(RevisionKind::Schema).ok_or("no `schema` revision record")?,
        oracle: pin(RevisionKind::Oracle).ok_or("no `oracle` revision record")?,
        cli_surface: pin(RevisionKind::CliSurface).ok_or("no `cli-surface` revision record")?,
    })
}
