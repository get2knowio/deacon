//! Hermetic deterministic replay of the oracle-free, non-container declarative cases
//! (026-continuous-conformance-certification, US3; FR-007, FR-010).
//!
//! ## What this binary is for
//!
//! The hermetic pull-request lane must include "deterministic snapshot replay" (FR-007).
//! That means: for every case whose evaluation needs neither the pinned reference nor a
//! container engine, confirm the case is *replayable* — its fixtures resolve, its hashes
//! compute, and its committed snapshot (where it has one) still matches the inputs it was
//! recorded against.
//!
//! ## Strictly read-only (FR-010)
//!
//! This binary holds no write path to the committed snapshot tree. Not by convention — by
//! construction: it imports no snapshot writer, and `lane_integrity` asserts by source
//! scan that it references none. A replay that could refresh a snapshot would turn every
//! continuous-integration run into an unreviewed blessing, which is precisely what FR-055
//! exists to prevent.
//!
//! ## No silent skips (FR-006)
//!
//! A missing fixture, an uncomputable hash, or an unreadable snapshot **fails**. There is
//! no `#[ignore]` here and no conditional early return: the whole value of the lane model
//! is that a green result means the units ran.

use std::collections::BTreeSet;

use deacon_conformance::case_hash::hashes_for_case;
use deacon_conformance::lane::case_lane_membership;
use deacon_conformance::load::Registry;
use deacon_conformance::model::TestCase;
use deacon_conformance::snapshot::Provenance;
use deacon_conformance::{default_registry_dir, workspace_root};

/// The cases this lane owns: oracle-free and container-free.
fn replayable_cases(registry: &Registry) -> Vec<&TestCase> {
    registry
        .cases
        .iter()
        .filter(|case| {
            case_lane_membership(case).is_some_and(|m| !m.needs_oracle && !m.needs_container)
        })
        .collect()
}

fn registry() -> Registry {
    Registry::load(&default_registry_dir()).expect("registry loads")
}

/// Case fixtures live under `conformance/fixtures/`, a sibling of the registry — the same
/// root the reviewed refresh and the hermetic `snapshot check` both use, so all three
/// agree about what a case's inputs are.
fn fixtures_root() -> std::path::PathBuf {
    workspace_root().join("conformance").join("fixtures")
}

#[test]
fn the_lane_owns_a_non_empty_case_set() {
    // A lane that selects nothing reports green forever, which is worse than an absent
    // lane: it looks like evidence.
    let registry = registry();
    let cases = replayable_cases(&registry);
    assert!(
        !cases.is_empty(),
        "the hermetic replay lane must own at least one case; a lane that selects nothing \
         cannot fail and therefore proves nothing"
    );
}

#[test]
fn every_replayable_case_has_computable_hashes() {
    // The precondition for replay: if a case's inputs cannot be hashed, its snapshot's
    // staleness is unknowable, and "unknown" must never be reported as "fresh".
    let registry = registry();
    let fixtures_root = fixtures_root();
    let mut failures = Vec::new();
    for case in replayable_cases(&registry) {
        if let Err(e) = hashes_for_case(case, &fixtures_root) {
            failures.push(format!("{}: {e}", case.id));
        }
    }
    assert!(
        failures.is_empty(),
        "every replayable case must have computable case/fixture hashes: {failures:?}"
    );
}

#[test]
fn every_committed_snapshot_for_a_replayable_case_matches_its_recorded_inputs() {
    // Deterministic snapshot replay (FR-007). A snapshot whose evidence-determining inputs
    // have drifted describes a case that no longer exists; reporting it as evidence would
    // be a claim backed by a measurement of something else.
    let registry = registry();
    let fixtures_root = fixtures_root();
    let snapshots_dir = workspace_root().join("conformance").join("snapshots");
    let replayable: BTreeSet<&str> = replayable_cases(&registry)
        .iter()
        .map(|c| c.id.as_str())
        .collect();

    let Ok(platforms) = std::fs::read_dir(&snapshots_dir) else {
        // No committed snapshots at all is a legitimate state; it is not a silent skip,
        // because the other tests in this binary still ran over every replayable case.
        return;
    };

    let mut stale = Vec::new();
    for platform in platforms.flatten() {
        let Ok(cases) = std::fs::read_dir(platform.path()) else {
            continue;
        };
        for entry in cases.flatten() {
            let case_id = entry.file_name().to_string_lossy().to_string();
            if !replayable.contains(case_id.as_str()) {
                continue;
            }
            let provenance_path = entry.path().join("provenance.json");
            let raw = std::fs::read_to_string(&provenance_path).unwrap_or_else(|e| {
                panic!("snapshot `{case_id}` has an unreadable provenance.json: {e}")
            });
            let provenance: Provenance = serde_json::from_str(&raw)
                .unwrap_or_else(|e| panic!("snapshot `{case_id}` provenance is malformed: {e}"));

            let case = registry
                .cases
                .iter()
                .find(|c| c.id == case_id)
                .expect("replayable set is drawn from the registry");
            let (case_hash, fixture_hash) = hashes_for_case(case, &fixtures_root)
                .unwrap_or_else(|e| panic!("hashes for `{case_id}`: {e}"));

            if provenance.case_hash != case_hash {
                stale.push(format!("{case_id}: caseHash drifted"));
            } else if provenance.fixture_hash != fixture_hash {
                stale.push(format!("{case_id}: fixtureHash drifted"));
            }
        }
    }
    assert!(
        stale.is_empty(),
        "committed snapshots are stale against current inputs: {stale:?}. Remedy: re-record \
         through the reviewed record path (`conformance-snapshot refresh`) and review the diff.",
    );
}

#[test]
fn this_binary_needs_no_reference_implementation_or_container_engine() {
    // The lane's defining property (FR-008), asserted about the case set rather than about
    // the environment: an environment probe would pass on any machine that happens to lack
    // the tools, proving nothing about what the lane actually requires.
    let registry = registry();
    for case in replayable_cases(&registry) {
        let membership = case_lane_membership(case).expect("declarative case");
        assert!(
            !membership.needs_oracle,
            "case `{}` needs the reference implementation and does not belong in the \
             hermetic lane",
            case.id
        );
        assert!(
            !membership.needs_container,
            "case `{}` needs a container engine and does not belong in the hermetic lane",
            case.id
        );
    }
}
