//! Container-backed pull-request lane: deacon against **pinned expected observables**
//! (026-continuous-conformance-certification, US3; FR-011, FR-012, FR-013, FR-033b).
//!
//! ## The sibling of `parity_conformance_docker`, minus the oracle
//!
//! Same shared runner, same declarative cases, same fail-loud contract — but restricted to
//! the cases whose evaluation needs no live reference. That restriction is what makes this
//! lane suitable for a pull request: it is deterministic and independent of upstream
//! availability, so a red result means deacon changed, not that npm was slow.
//!
//! Membership is derived from `oracleType`, never annotated (research D9). A case typed
//! `live-differential` belongs to the nightly lane and cannot appear here even by mistake —
//! [`driver::DriverConfig::oracle`] is `None`, so such a case would fail rather than run.
//!
//! ## It emits the execution manifest certification consumes
//!
//! The receipt goes to `target/conformance/execution-manifest.json` — the path
//! `conformance-docker.yml` uploads and `certify` reads (FR-033b). It records **real
//! outcomes from a real run**: every required case, including the ones that failed and the
//! ones a scoped tolerance covered, because a manifest listing only successes is
//! incomplete, not clean.
//!
//! The revision comes from `DEACON_CERTIFY_REVISION`. Absent, it is recorded empty, and
//! `certify` then blocks on `V35-revision` — deliberately: a receipt that cannot say which
//! commit it describes is not evidence, and silently inventing one would be worse than
//! having none.
//!
//! Runs ONLY under `cargo nextest run --profile pr-docker`. No opt-in environment gate and
//! no silent skip: an absent engine, a missing fixture, or a normalization fault FAILS
//! (constitution IV).

use std::path::PathBuf;
use std::sync::Arc;

use deacon_conformance::lane::case_lane_membership;
use deacon_conformance::load::Registry;
use deacon_conformance::model::{ResourceGroup, TestCase};
use deacon_conformance::{default_registry_dir, workspace_root};

use parity_harness::driver::{self, DriverConfig};
use parity_harness::evidence::Outcome;
use parity_harness::manifest_emit::{self, CaseRun};
use parity_harness::{HarnessError, prereq, report_root};

/// This binary's name — the report-fragment key and the lane's registry entry.
const BINARY: &str = "conformance_docker_pinned";

/// The profile this lane produces evidence for.
const PROFILE: &str = "prof-linux-amd64-docker-0870";

/// Fail with the error's cause-specific `Display` (never `Debug`) so a prerequisite or
/// normalization failure reads as its remedy.
fn ff<T>(r: Result<T, HarnessError>) -> T {
    r.unwrap_or_else(|e| panic!("{e}"))
}

fn registry() -> Registry {
    Registry::load(&default_registry_dir())
        .unwrap_or_else(|e| panic!("{BINARY}: registry did not load: {e}"))
}

fn fixtures_root() -> PathBuf {
    workspace_root().join("conformance").join("fixtures")
}

/// The cases this lane owns: container-backed and oracle-free.
fn pinned_container_cases(registry: &Registry) -> Vec<&TestCase> {
    registry
        .cases
        .iter()
        .filter(|case| {
            case_lane_membership(case).is_some_and(|m| m.needs_container && !m.needs_oracle)
        })
        .collect()
}

/// Map a runner verdict onto the manifest's closed outcome set.
///
/// `NoReferenceForPlatform` and `Stale` become `Fail` rather than a fourth state: both mean
/// the case produced no usable evidence on this platform, and certification must treat that
/// as a failure it can see. Folding them into `Excluded` would require a disposition that
/// does not exist, which `V35-unaccounted` would then — correctly — reject.
fn manifest_outcome(overall: Outcome) -> manifest_emit::Outcome {
    match overall {
        Outcome::Agree => manifest_emit::Outcome::Pass,
        Outcome::AllowedDifference => manifest_emit::Outcome::AllowedDifference,
        Outcome::Diverge | Outcome::Error | Outcome::Stale | Outcome::NoReferenceForPlatform => {
            manifest_emit::Outcome::Fail
        }
    }
}

/// Drive one Docker resource group without an oracle, and record what happened.
///
/// A group with no cases is NOT a silent skip: the engine is still required and still
/// probed, and the note reaches stderr. "No case today" must never quietly become "no
/// daemon needed" — that is how a lane stops proving anything without anyone noticing.
async fn drive(group: ResourceGroup) -> Vec<CaseRun> {
    assert!(
        driver::needs_docker(group),
        "`{BINARY}` drives the Docker-backed groups; `{}` needs no daemon",
        driver::group_slug(group)
    );

    // Fail-loud precondition. The lane declares `container-engine`, so an absent daemon
    // must fail it (FR-004), never skip it to a green.
    ff(prereq::require_docker().await);

    let root = workspace_root();
    let registry = registry();
    let cases: Vec<TestCase> = driver::cases_in_group(&registry.cases, group)
        .into_iter()
        .filter(|case| case_lane_membership(case).is_some_and(|m| !m.needs_oracle))
        .collect();

    if cases.is_empty() {
        eprintln!(
            "note: no oracle-free case declares resourceGroup `{}`",
            driver::group_slug(group)
        );
        return Vec::new();
    }

    let cfg = Arc::new(DriverConfig {
        binary: BINARY.to_string(),
        deacon_path: PathBuf::from(env!("CARGO_BIN_EXE_deacon")),
        // FR-012: this lane never resolves the reference. A `live-differential` case
        // reaching here would fail on the absent oracle rather than silently running
        // against nothing — which is the correct outcome, and why the filter above and
        // this `None` are both present.
        oracle: None,
        fixtures_root: fixtures_root(),
        report_root: report_root(),
        snapshots_root: root.join("conformance").join("snapshots"),
    });

    let fixtures = fixtures_root();
    let hashes: Vec<(String, String, String)> = cases
        .iter()
        .map(|case| {
            // Hashes are computed HERE, from the definitions this run is about to execute —
            // not re-derived afterwards, which would mask a mid-run edit.
            let (case_hash, fixture_hash) =
                deacon_conformance::case_hash::hashes_for_case(case, &fixtures)
                    .unwrap_or_else(|e| panic!("{BINARY}: hashes for `{}`: {e}", case.id));
            (case.id.clone(), case_hash, fixture_hash)
        })
        .collect();

    let run = ff(driver::drive_group(Arc::clone(&cfg), cases, group).await);
    ff(driver::emit(&run));

    let runs: Vec<CaseRun> = run
        .verdicts
        .iter()
        .map(|verdict| {
            let (case_hash, fixture_hash) = hashes
                .iter()
                .find(|(id, _, _)| *id == verdict.case_id)
                .map(|(_, c, f)| (c.clone(), f.clone()))
                .unwrap_or_default();
            CaseRun {
                case_id: verdict.case_id.clone(),
                case_hash,
                fixture_hash,
                outcome: manifest_outcome(verdict.overall),
                excluded_by: None,
            }
        })
        .collect();

    // The manifest is written before the divergence assertion below, so a RED run still
    // ships its receipt. The manifest is diagnostic; suppressing it exactly when the lane
    // fails would hide the evidence when it is most needed.
    write_manifest(&runs);

    assert!(
        run.failures.is_empty(),
        "pinned-observable divergence(s) in resource group `{}`:\n{}",
        driver::group_slug(group),
        run.failures.join("\n"),
    );
    runs
}

/// Write (or extend) the execution manifest at the path certification reads.
///
/// Merges rather than overwrites: nextest runs the group drivers as sibling processes, and
/// a driver that clobbered the file would leave a manifest describing one group while
/// `certify` requires every group's cases — reported as `V35-incomplete` for work that
/// actually ran.
fn write_manifest(new_runs: &[CaseRun]) {
    let path = workspace_root()
        .join("target")
        .join("conformance")
        .join("execution-manifest.json");

    let mut merged: Vec<CaseRun> = existing_runs(&path);
    for run in new_runs {
        merged.retain(|existing| existing.case_id != run.case_id);
        merged.push(run.clone());
    }
    merged.sort_by(|a, b| a.case_id.cmp(&b.case_id));

    let required = pinned_container_cases(&registry()).len();
    ff(manifest_emit::emit_manifest(
        &path,
        &manifest_emit::ManifestInputs {
            // From the environment the certifying job declares. Empty when unset, which
            // `certify` rejects as `V35-revision` — a receipt that cannot name its commit
            // is not evidence, and inventing one would be worse than having none.
            revision: std::env::var("DEACON_CERTIFY_REVISION").unwrap_or_default(),
            profile: PROFILE.to_string(),
            required_case_count: required,
            runs: merged,
        },
    ));
    eprintln!("{BINARY}: wrote {}", path.display());
}

/// The case runs already recorded in a manifest at `path`, if any.
fn existing_runs(path: &std::path::Path) -> Vec<CaseRun> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(existing) =
        serde_json::from_str::<deacon_conformance::manifest::ExecutionManifest>(&raw)
    else {
        // An unparseable leftover is discarded rather than propagated: this run's evidence
        // is real and current, and a corrupt prior file must not cost it.
        return Vec::new();
    };
    existing
        .cases
        .into_iter()
        .map(|case| CaseRun {
            case_id: case.case_id,
            case_hash: case.case_hash,
            fixture_hash: case.fixture_hash,
            outcome: match case.outcome {
                deacon_conformance::manifest::CaseOutcome::Pass => manifest_emit::Outcome::Pass,
                deacon_conformance::manifest::CaseOutcome::Fail => manifest_emit::Outcome::Fail,
                deacon_conformance::manifest::CaseOutcome::AllowedDifference => {
                    manifest_emit::Outcome::AllowedDifference
                }
                deacon_conformance::manifest::CaseOutcome::Excluded => {
                    manifest_emit::Outcome::Excluded
                }
            },
            excluded_by: case.excluded_by,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Drivers — one per Docker resource group, run as sibling nextest processes
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn docker_shared_group() {
    drive(ResourceGroup::DockerShared).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn docker_exclusive_group() {
    drive(ResourceGroup::DockerExclusive).await;
}

// ---------------------------------------------------------------------------
// Lane-membership invariants — hermetic, and true regardless of the daemon
// ---------------------------------------------------------------------------

#[test]
fn the_lane_owns_a_non_empty_case_set() {
    let registry = registry();
    assert!(
        !pinned_container_cases(&registry).is_empty(),
        "{BINARY}: a lane that selects nothing cannot fail and therefore proves nothing"
    );
}

#[test]
fn no_selected_case_requires_the_reference_implementation() {
    // FR-012. If a live-differential case reached this lane, a pull request would start
    // depending on upstream availability — the exact coupling pinning the observables buys
    // us out of.
    let registry = registry();
    for case in pinned_container_cases(&registry) {
        let membership = case_lane_membership(case).expect("declarative case");
        assert!(
            !membership.needs_oracle,
            "{BINARY}: case `{}` needs the reference implementation",
            case.id
        );
    }
}

#[test]
fn every_selected_case_has_computable_hashes() {
    // The manifest records hashes computed at execution time; a case whose hashes cannot
    // be computed could not be recorded honestly.
    let registry = registry();
    let fixtures = fixtures_root();
    let mut failures = Vec::new();
    for case in pinned_container_cases(&registry) {
        if let Err(e) = deacon_conformance::case_hash::hashes_for_case(case, &fixtures) {
            failures.push(format!("{}: {e}", case.id));
        }
    }
    assert!(failures.is_empty(), "{BINARY}: {failures:?}");
}

#[test]
fn every_image_input_is_pinned() {
    // FR-013. Enforced structurally by the existing V18 class over the registry; asserted
    // here as well because this lane is the one that would actually pull a mutable tag, and
    // a lane that silently tracked `latest` would make its own results irreproducible.
    let registry = registry();
    let mut mutable = Vec::new();
    for case in pinned_container_cases(&registry) {
        for operation in &case.operations {
            for arg in &operation.argv {
                if arg.ends_with(":latest") || arg == "latest" {
                    mutable.push(format!("{}: `{arg}`", case.id));
                }
            }
        }
    }
    assert!(
        mutable.is_empty(),
        "{BINARY}: every container input must be pinned by digest or concrete tag (FR-013): \
         {mutable:?}"
    );
}

#[test]
fn every_runner_outcome_maps_to_the_manifest_enumeration() {
    // The manifest's outcome set is closed so `V35-unaccounted` can catch a silent skip.
    // That only holds if every runner verdict has a mapping — an unmapped one would have to
    // become a fourth state, which is precisely what the closed set forbids.
    for outcome in [
        Outcome::Agree,
        Outcome::Diverge,
        Outcome::AllowedDifference,
        Outcome::NoReferenceForPlatform,
        Outcome::Stale,
        Outcome::Error,
    ] {
        let mapped = manifest_outcome(outcome);
        // `Excluded` is reserved for a disposition-backed exclusion, which the runner never
        // produces: it would need an `excludedBy` this lane has no basis to invent.
        assert_ne!(
            mapped,
            manifest_emit::Outcome::Excluded,
            "runner outcome {outcome:?} must not map to `excluded` — that state requires a \
             resolvable disposition id, and inventing one would be an unaccounted skip"
        );
    }
    assert_eq!(
        manifest_outcome(Outcome::Agree),
        manifest_emit::Outcome::Pass
    );
    assert_eq!(
        manifest_outcome(Outcome::AllowedDifference),
        manifest_emit::Outcome::AllowedDifference
    );
    assert_eq!(
        manifest_outcome(Outcome::Diverge),
        manifest_emit::Outcome::Fail
    );
}
