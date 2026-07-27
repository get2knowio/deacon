//! Acceptance tests for User Story 4 — "the container-backed error-path tier"
//! (024-deterministic-conformance-coverage, T100; FR-041–FR-046, SC-007).
//!
//! Hermetic by construction: every assertion reads the REAL committed registry, so none of
//! them can pass against a convenient synthetic model that no longer ships. The boundary is
//! worth stating, because it is easy to mistake for a gap:
//!
//! | Question | Answered here | Answered by the live tier |
//! |---|---|---|
//! | Could this case's verdict be reached at configuration read? | yes | — |
//! | Does the tier span all five later-stage failure points? | yes | — |
//! | Is a difference's DIRECTION pinned by something other than the differential? | yes | — |
//! | Do the two sides actually behave as the record says? | no | `parity_conformance_docker` |
//!
//! A hermetic test cannot run deacon, so "the case passes" is not something this file
//! asserts. What it asserts is that the record set cannot express the failure US4 exists to
//! prevent: a tier case that never gets past the stage where the reference is most lenient,
//! and therefore proves exactly what the pre-024 coverage already proved.

use std::collections::{BTreeMap, BTreeSet};

use deacon_conformance::default_registry_dir;
use deacon_conformance::load::Registry;
use deacon_conformance::model::{
    CaseKind, FailurePhase, OracleType, ResourceGroup, TestCase, phases_reachable_by,
};
use deacon_conformance::scenario::OPERATION_DIMENSION;

fn real_registry() -> Registry {
    Registry::load(&default_registry_dir()).expect("the real registry loads")
}

fn tier(registry: &Registry) -> Vec<&TestCase> {
    registry
        .cases
        .iter()
        .filter(|c| c.error_path_tier)
        .collect()
}

/// The five later-stage failure points SC-007 enumerates.
///
/// `build` and `feature-installation` share a [`FailurePhase`] and are separated by the
/// scenario context, deliberately: on BOTH implementations a Feature is installed by a
/// Docker build over the base image, so there is no distinct install phase for the closed
/// failure-phase set to name (022 data-model §8, reused rather than extended here). The
/// distinction SC-007 asks for is nonetheless real — one input has Features and the other
/// does not — and the scenario model is where that fact already lives.
const STAGES: &[&str] = &[
    "build",
    "container-creation",
    "feature-installation",
    "lifecycle-execution",
    "teardown",
];

/// Which of the five later-stage failure points a tier case exercises.
fn stage_of(case: &TestCase) -> Option<&'static str> {
    let phases: Vec<FailurePhase> = case
        .later_stage_failure_phases()
        .into_iter()
        .map(|(_, p)| p)
        .collect();
    let operation = case
        .scenario_context
        .get(OPERATION_DIMENSION)
        .map(String::as_str);
    let has_features = case
        .scenario_context
        .get("sdim-features")
        .map(|v| v != "none")
        .unwrap_or(false);

    if phases.iter().any(|p| p.is_lifecycle()) {
        return Some("lifecycle-execution");
    }
    if phases.contains(&FailurePhase::ContainerCreate) {
        return Some("container-creation");
    }
    if phases.contains(&FailurePhase::Build) {
        return Some(if has_features {
            "feature-installation"
        } else {
            "build"
        });
    }
    if operation == Some("down") {
        return Some("teardown");
    }
    None
}

// ---------------------------------------------------------------------------
// SC-007, first half — no error-path case reaches its verdict at configuration read
// ---------------------------------------------------------------------------

/// The tier's defining property, checked three ways because it can be lost three ways.
///
/// The premise of the tier (FR-041) is that configuration read ACCEPTS the input on both
/// sides, so that what is compared is a later stage. A case that fails at configuration
/// read still passes, still reports, and still counts — while proving exactly what the
/// pre-024 coverage already proved. Nothing at run time can tell the two apart: "the later
/// stage agreed" and "the later stage was never reached" produce the same green.
///
/// 1. **No declared phase is `config-resolution`.** V16 enforces it; asserted here too,
///    because the registry is the artifact the claim is made in.
/// 2. **The declared later stage is what the verdict is actually taken from.** Every
///    operation declaring a later-stage phase must be observed by at least one `expected`
///    entry, and at least one observation must come from an operation that can get past
///    configuration read at all. Without this a case can declare a late failure on an
///    operation nothing observes, and take its verdict entirely from something earlier —
///    the stage would be recorded and not compared. (Observing operations that only reach
///    configuration read is fine ALONGSIDE that: the teardown case asserts each `down`
///    exits 0 precisely so its later exec failure cannot be blamed on a broken teardown.)
/// 3. **No tier case is built on a fixture the registry uses to demonstrate a
///    configuration-read REJECTION.** This is the sharpest of the three: those fixtures are
///    the registry's own evidence that the document is not accepted, so a tier case reusing
///    one contradicts its own premise in a way neither of the first two checks can see.
#[test]
fn no_error_path_case_reaches_its_verdict_at_configuration_read() {
    let registry = real_registry();
    let cases = tier(&registry);
    assert!(
        cases.len() >= 9,
        "the error-path tier must span five later-stage failure points; found {} case(s)",
        cases.len()
    );

    // (3) The fixtures the registry itself uses to show a configuration-read rejection.
    let mut rejected_at_read: BTreeSet<&str> = BTreeSet::new();
    for case in &registry.cases {
        for op in &case.operations {
            if op.expect_failure_phase == Some(FailurePhase::ConfigResolution) {
                rejected_at_read.extend(op.fixtures.iter().map(String::as_str));
            }
        }
    }

    for case in &cases {
        // (1)
        for (op_id, phase) in case.declared_failure_phases() {
            assert!(
                !phase.is_configuration_read(),
                "{}: operation {op_id:?} declares `{}`, so its verdict is reached at the \
                 stage the tier exists to look past (FR-041)",
                case.id,
                phase.as_str()
            );
        }
        assert!(
            !case.later_stage_failure_phases().is_empty(),
            "{}: records no later-stage failure phase, so it states no stage at all (FR-042)",
            case.id
        );

        // (2)
        let mut observed_ops: BTreeSet<&str> = BTreeSet::new();
        let mut observes_a_later_stage = false;
        for expectation in &case.expected {
            let op = match &expectation.operation {
                Some(id) => case.operations.iter().find(|o| &o.id == id),
                None => case.operations.last(),
            }
            .unwrap_or_else(|| {
                panic!(
                    "{}: expectation on {:?} resolves to no operation",
                    case.id, expectation.channel
                )
            });
            observed_ops.insert(op.id.as_str());
            observes_a_later_stage |= phases_reachable_by(&op.subcommand).len() > 1;
        }
        assert!(
            observes_a_later_stage,
            "{}: every observation is taken from an operation that can only fail at \
             configuration read, so nothing it compares is evidence about a later stage",
            case.id
        );
        for (op_id, phase) in case.later_stage_failure_phases() {
            assert!(
                observed_ops.contains(op_id),
                "{}: operation {op_id:?} declares `{}`, but no `expected` observes it — the \
                 stage would be recorded and never compared (FR-042)",
                case.id,
                phase.as_str()
            );
        }

        // (3)
        for op in &case.operations {
            for fixture in &op.fixtures {
                assert!(
                    !rejected_at_read.contains(fixture.as_str()),
                    "{}: operation {:?} uses fixture {fixture:?}, which the registry \
                     elsewhere uses to demonstrate a configuration-read REJECTION — the \
                     tier's premise is an input BOTH sides accept at read time (FR-041)",
                    case.id,
                    op.id
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SC-007, second half — one case per later-stage failure point
// ---------------------------------------------------------------------------

/// Every later-stage failure point SC-007 names is exercised by at least one tier case, and
/// every tier case is attributable to one of them.
///
/// Both directions matter. Without the first, a stage can quietly go uncovered while the
/// tier looks healthy in aggregate. Without the second, a case can join the tier without
/// exercising any named stage — which is how a tier accumulates cases that are in it for no
/// stated reason and stops meaning anything.
#[test]
fn the_tier_spans_every_later_stage_failure_point() {
    let registry = real_registry();
    let mut by_stage: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for case in tier(&registry) {
        let stage = stage_of(case).unwrap_or_else(|| {
            panic!(
                "{}: is in the error-path tier but exercises none of the five later-stage \
                 failure points ({}); its declared phases are {:?}",
                case.id,
                STAGES.join(", "),
                case.declared_failure_phases()
            )
        });
        by_stage.entry(stage).or_default().push(&case.id);
    }
    for stage in STAGES {
        let covered = by_stage.get(stage).map(Vec::len).unwrap_or(0);
        assert!(
            covered > 0,
            "SC-007 requires at least one error-path case for `{stage}`; the tier covers \
             {by_stage:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// FR-045 / FR-046 / FR-041 — the tier's execution contract, as far as records show it
// ---------------------------------------------------------------------------

/// Every tier case is container-backed and declares full reclamation.
///
/// A tier case fails partway BY CONSTRUCTION, which is exactly the run in which resources
/// leak: the operation that would have torn something down never happened. The Docker
/// `resourceGroup` is what gives the case an isolated workspace and the RAII cleanup guard
/// (V16 rule 3), and `cleanup` is what the guard reclaims — so a tier case with either
/// missing is a case whose failure leaves residue on the daemon.
#[test]
fn every_error_path_case_is_container_backed_and_reclaims_what_it_creates() {
    let registry = real_registry();
    for case in tier(&registry) {
        assert!(
            matches!(
                case.resource_group,
                Some(ResourceGroup::DockerShared) | Some(ResourceGroup::DockerExclusive)
            ),
            "{}: is in the error-path tier with no Docker resource group, so it gets no \
             isolated workspace and no cleanup guard",
            case.id
        );
        let cleanup = case.cleanup.as_ref().unwrap_or_else(|| {
            panic!(
                "{}: declares no `cleanup`; a case that fails partway is precisely the one \
                 whose resources are left behind (FR-045)",
                case.id
            )
        });
        assert!(
            cleanup.containers && cleanup.networks && cleanup.volumes && cleanup.tempdir,
            "{}: must reclaim its container, network, volume and temp directory — {cleanup:?}",
            case.id
        );
        assert!(
            matches!(case.classify(), Ok(CaseKind::Declarative)),
            "{}: only a declarative case can be driven by the tier's runner",
            case.id
        );
    }
}

// ---------------------------------------------------------------------------
// FR-043 — a difference's direction is pinned by something other than the differential
// ---------------------------------------------------------------------------

/// Every tolerated difference in the tier has a `spec-expectation` case pinning WHICH SIDE
/// is right (the 023 T074 lesson, FR-043).
///
/// A differential carrying an `allowedDifference` asserts that the two CLIs disagree in a
/// characterized way. It stays green if BOTH sides start reporting success — the difference
/// simply stops reproducing, and the record that was supposed to say which side is correct
/// says nothing. (The tolerance is then reported stale, which is a prompt to re-review, not
/// a statement of direction.) The twin is what makes the pair state a direction.
///
/// The twin is matched on a SHARED BEHAVIOR rather than on a name suffix, for the same
/// reason `workflow_coverage.rs` does: a naming convention can be satisfied by a rename, a
/// shared behavior cannot.
#[test]
fn every_tolerated_difference_in_the_tier_has_a_direction_pinning_twin() {
    let registry = real_registry();
    let waivers: BTreeSet<&str> = registry.waivers.iter().map(|w| w.id.as_str()).collect();
    let extensions: BTreeSet<&str> = registry.extensions.iter().map(|e| e.id.as_str()).collect();

    let mut tolerated = 0usize;
    for case in tier(&registry) {
        for allowed in &case.allowed_differences {
            tolerated += 1;
            let backing = allowed.resolved_id().unwrap_or_else(|_| {
                panic!(
                    "{}: tolerance on {:?} names both or neither of waiverId/divergenceId",
                    case.id, allowed.observable_path
                )
            });
            assert!(
                waivers.contains(backing) || extensions.contains(backing),
                "{}: tolerance on {:?} is backed by {backing:?}, which resolves to no waiver \
                 or divergence record",
                case.id,
                allowed.observable_path
            );
            assert!(
                !allowed.is_global_ignore(),
                "{}: tolerance names {:?}, which is a bare channel or wildcard rather than a \
                 specific observable (FR-032)",
                case.id,
                allowed.observable_path
            );
            assert!(
                case.behaviors.contains(&allowed.behavior),
                "{}: tolerance is scoped to behavior {:?}, which the case does not link, so \
                 it can never apply",
                case.id,
                allowed.behavior
            );

            let twin = registry.cases.iter().find(|other| {
                other.id != case.id
                    && other.oracle_type == Some(OracleType::SpecExpectation)
                    && other.behaviors.contains(&allowed.behavior)
                    && other.expected.iter().any(|e| e.assertion.is_some())
            });
            assert!(
                twin.is_some(),
                "{}: tolerates a difference on behavior {:?} with no spec-expectation case \
                 asserting which side is right — the difference would be recorded without a \
                 direction, exactly the 023 T074 defect (FR-043)",
                case.id,
                allowed.behavior
            );
        }
    }
    assert!(
        tolerated > 0,
        "the tier is supposed to compare past the point where the reference is most lenient; \
         a tier that found NO tolerated difference at all has most likely stopped comparing \
         (or the tolerances were removed without the divergence being fixed)"
    );
}
