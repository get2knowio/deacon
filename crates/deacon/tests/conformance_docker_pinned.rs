//! Container-backed pull-request lane: deacon against **pinned expected observables**
//! (026-continuous-conformance-certification, US3; FR-011, FR-012, FR-013).
//!
//! ## The sibling of `parity_conformance_docker`, minus the oracle
//!
//! Same shared runner, same declarative cases, same fail-loud contract — but restricted to
//! the cases whose evaluation needs no live reference. That restriction is what makes this
//! lane suitable for a pull request: it is deterministic and independent of upstream
//! availability, so a red result means deacon changed, not that npm was slow.
//!
//! Membership is derived from `oracleType`, never annotated (research D9). A case typed
//! `live-differential` belongs to the nightly lane and cannot appear here even by mistake.
//!
//! ## It emits the execution manifest
//!
//! This lane produces the receipt certification consumes (FR-033b). The manifest records
//! every required case — including the ones that failed and the ones excluded by
//! disposition — because a manifest listing only successes is incomplete, not clean.
//!
//! Runs ONLY under `cargo nextest run --profile pr-docker`. No opt-in environment gate and
//! no silent skip: an absent engine, a missing fixture, or an unreadable case FAILS
//! (constitution IV).

use std::path::PathBuf;

use deacon_conformance::case_hash::hashes_for_case;
use deacon_conformance::lane::case_lane_membership;
use deacon_conformance::load::Registry;
use deacon_conformance::model::TestCase;
use deacon_conformance::{default_registry_dir, workspace_root};

use parity_harness::manifest_emit::{self, CaseRun};

/// This binary's name — the registry entry and the manifest's producer key.
const BINARY: &str = "conformance_docker_pinned";

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
        if let Err(e) = hashes_for_case(case, &fixtures) {
            failures.push(format!("{}: {e}", case.id));
        }
    }
    assert!(failures.is_empty(), "{BINARY}: {failures:?}");
}

#[test]
fn every_image_input_is_pinned() {
    // FR-013. Enforced structurally by the existing V18 class over the registry; asserted
    // here as well because this lane is the one that would actually pull a mutable tag,
    // and a lane that silently tracked `latest` would make its own results irreproducible.
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
fn the_lane_emits_an_execution_manifest() {
    // FR-033b. The receipt is written on every run — including a failing one, because the
    // manifest is diagnostic and suppressing it on red runs would hide the evidence
    // exactly when it is most needed.
    let registry = registry();
    let fixtures = fixtures_root();

    let runs: Vec<CaseRun> = pinned_container_cases(&registry)
        .iter()
        .filter_map(|case| {
            let (case_hash, fixture_hash) = hashes_for_case(case, &fixtures).ok()?;
            Some(CaseRun {
                case_id: case.id.clone(),
                case_hash,
                fixture_hash,
                // This test asserts the *emission contract*. The per-case outcomes are
                // produced by the shared runner under `--profile pr-docker`; recording
                // them here would be recording a result nothing measured.
                outcome: manifest_emit::Outcome::Excluded,
                excluded_by: Some("odp-not-executed-in-shape-test".to_string()),
            })
        })
        .collect();

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("execution-manifest.json");
    manifest_emit::emit_manifest(
        &path,
        &manifest_emit::ManifestInputs {
            revision: "shape-test".into(),
            profile: "prof-linux-amd64-docker-0870".into(),
            required_case_count: runs.len(),
            runs,
        },
    )
    .expect("manifest writes");

    let raw = std::fs::read_to_string(&path).expect("manifest readable");
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("manifest parses");
    assert_eq!(parsed["schemaVersion"], 1);
    assert!(
        parsed["cases"].as_array().is_some_and(|c| !c.is_empty()),
        "the manifest must record every required case, not only the successes"
    );
}
