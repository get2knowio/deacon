//! Acceptance tests for User Story 3 — "Deterministic coverage of the shared consumer
//! workflow" (024-deterministic-conformance-coverage, T073–T078).
//!
//! Hermetic by construction: every assertion reads the REAL committed registry, so none
//! of them can pass against a convenient synthetic model that no longer resembles what
//! ships. What these tests can and cannot see is worth stating, because the boundary is
//! easy to mistake for a gap:
//!
//! | Question | Answered here | Answered by the live tier |
//! |---|---|---|
//! | Can a case be skipped? | yes — the record and the driver partition | — |
//! | Does every stage span its input classes? | yes — from the same report a reviewer reads | — |
//! | Is a lenient case's DIRECTION pinned? | yes — the twin must exist | — |
//! | Does a case AGREE with the reference? | no | `parity_conformance_runner` / `_docker` |
//!
//! A hermetic test cannot run deacon, so "the case passes" is not something this file
//! asserts and must not pretend to. What it asserts is that the record set cannot express
//! the failures US3 is about: a case nobody runs, a stage nobody probed, a difference
//! whose direction nobody recorded, a verdict that moves between runs.

mod support;

use std::collections::{BTreeMap, BTreeSet};

use deacon_conformance::coverage::{ObligationBucket, evaluate_obligations, executable_case_ids};
use deacon_conformance::coverage_report::{CoverageReports, build_coverage_reports};
use deacon_conformance::load::Registry;
use deacon_conformance::model::{
    CONSUMER_SUBCOMMANDS, CaseKind, InputClass, OBSERVED_CHANNELS, OracleType, ResourceGroup,
    TestCase, differential_substitution,
};
use deacon_conformance::obligation::{generate_obligations, triple_obligation_id};
use deacon_conformance::scenario::OPERATION_DIMENSION;
use deacon_conformance::{default_registry_dir, workspace_root};

fn real_registry() -> Registry {
    Registry::load(&default_registry_dir()).expect("the real registry loads")
}

fn reports_for(registry: &Registry) -> CoverageReports {
    let inventory = generate_obligations(registry).expect("obligations generate");
    build_coverage_reports(registry, &inventory)
}

fn declarative(registry: &Registry) -> Vec<&TestCase> {
    registry
        .cases
        .iter()
        .filter(|c| matches!(c.classify(), Ok(CaseKind::Declarative)))
        .collect()
}

// ---------------------------------------------------------------------------
// T073 — scenario 1, SC-012
// ---------------------------------------------------------------------------

/// Every declarative case reaches a definite verdict: none can be skipped, ignored, or
/// conditionally excluded.
///
/// "Skipped" has three possible shapes, and the test refuses all three rather than the one
/// that is easiest to check:
///
/// 1. **No driver selects it.** `ResourceGroup` is the ONLY discriminator the two live
///    binaries partition on, and it is a closed set — so the check is that the partition is
///    total and disjoint, not that some list happens to mention every case.
/// 2. **Nothing can observe it.** A case declaring a channel with no observer validates
///    cleanly and then fails at run time; a case declaring NO channel is worse, because it
///    can only ever pass. Both would be a verdict nothing produced.
/// 3. **Its evidence cannot be gathered.** A `spec-expectation` case with an assertionless
///    channel has nothing to compare against, so its verdict would be vacuous.
///
/// The one thing deliberately NOT checked is a `#[ignore]`-style opt-out: the declarative
/// record has no field that could express one, and asserting the absence of a field the
/// schema cannot represent would be a test of `serde`.
#[test]
fn every_declarative_case_reaches_a_definite_verdict() {
    let registry = real_registry();
    let cases = declarative(&registry);
    assert!(
        cases.len() >= 100,
        "the declarative case set has collapsed to {} records — this test is only \
         meaningful over the real set",
        cases.len()
    );

    // (1) The resource-group partition is total: every case lands in exactly one driver.
    let mut by_driver: BTreeMap<&str, usize> = BTreeMap::new();
    for case in &cases {
        let group = case.resource_group.unwrap_or(ResourceGroup::None);
        let driver = match group {
            ResourceGroup::DockerShared | ResourceGroup::DockerExclusive => "docker",
            ResourceGroup::FsHeavy | ResourceGroup::None => "config-only",
        };
        *by_driver.entry(driver).or_default() += 1;
    }
    assert_eq!(
        by_driver.values().sum::<usize>(),
        cases.len(),
        "every declarative case must be selected by exactly one driver; the partition \
         covered {by_driver:?} of {} cases",
        cases.len()
    );
    assert!(
        by_driver.get("docker").copied().unwrap_or(0) > 0
            && by_driver.get("config-only").copied().unwrap_or(0) > 0,
        "both drivers must actually own cases, or one binary's selection is dead: \
         {by_driver:?}"
    );

    // (2) + (3) Every case declares an observable channel, and a spec-expectation case
    // declares what it expects on each one.
    let observable: BTreeSet<&str> = OBSERVED_CHANNELS.iter().copied().collect();
    for case in &cases {
        assert!(
            !case.expected.is_empty(),
            "{}: declares no expected channel, so no verdict can be produced from it",
            case.id
        );
        assert!(
            case.oracle_type.is_some(),
            "{}: declares no oracleType, so nothing decides its verdict",
            case.id
        );
        for expectation in &case.expected {
            assert!(
                observable.contains(expectation.channel.as_str()),
                "{}: declares channel {:?}, which no observer can capture",
                case.id,
                expectation.channel
            );
            if case.oracle_type == Some(OracleType::SpecExpectation) {
                assert!(
                    expectation.assertion.is_some(),
                    "{}: spec-expectation channel {:?} carries no assertion, so its \
                     verdict would be vacuous",
                    case.id,
                    expectation.channel
                );
            }
        }
        // Every operation invokes a real consumer subcommand — a case naming something
        // else could never run at all.
        for op in &case.operations {
            assert!(
                CONSUMER_SUBCOMMANDS.contains(&op.subcommand.as_str()),
                "{}: operation {:?} invokes {:?}, which is outside the consumer surface",
                case.id,
                op.id,
                op.subcommand
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T074 — scenario 2, FR-040 / SC-004
// ---------------------------------------------------------------------------

/// Every workflow stage carries at least one valid-behavior case, one case per permitted
/// input class, and one per permitted configuration source.
///
/// Read off the SAME report a reviewer reads, deliberately. A test that recomputed the
/// tallies would pass while the report said something else, and the report is the artifact
/// the coverage claim is actually made in.
#[test]
fn every_stage_spans_its_permitted_input_classes_and_config_sources() {
    let registry = real_registry();
    let reports = reports_for(&registry);

    let operations = registry
        .scenario
        .iter()
        .find(|d| d.id == OPERATION_DIMENSION)
        .expect("the operation dimension is declared");
    assert_eq!(
        reports.operations.operations.len(),
        operations.values.len(),
        "the report must carry a row for every declared operation, including any with no \
         cases — an operation that vanishes from the report cannot be seen to be missing"
    );

    for entry in &reports.operations.operations {
        assert!(entry.cases > 0, "`{}` has no cases at all", entry.operation);
        assert!(
            entry.input_classes.get("valid").copied().unwrap_or(0) > 0,
            "`{}` has no valid-behavior case; every other class describes a deviation \
             from a behavior nothing pins",
            entry.operation
        );
        assert!(
            entry.missing_input_classes.is_empty(),
            "`{}` is missing input classes {:?} (FR-040)",
            entry.operation,
            entry.missing_input_classes
        );
        assert!(
            entry.missing_config_sources.is_empty(),
            "`{}` is missing configuration sources {:?} (SC-004)",
            entry.operation,
            entry.missing_config_sources
        );
        // The report must also say WHICH classes it considers permitted, by carrying the
        // differential-availability fact the permitted set is derived from.
        assert_eq!(
            entry.differential_available,
            differential_substitution(&entry.operation).is_none(),
            "`{}` reports a differential availability that disagrees with the registry's \
             own account of the pinned reference",
            entry.operation
        );
    }
}

// ---------------------------------------------------------------------------
// T075 — scenario 3, FR-043 (the 023 T074 lesson)
// ---------------------------------------------------------------------------

/// Every `reference-lenient` case is paired with a `spec-expectation` twin that pins the
/// DIRECTION of the difference.
///
/// A differential alone asserts only that the two CLIs disagree. It stays green if both
/// sides start accepting the input, or both start rejecting it — the difference stops
/// reproducing, and the case that was supposed to record which side is right records
/// nothing. The twin is what makes the pair state a direction rather than a disagreement.
///
/// The twin is matched on a SHARED BEHAVIOR rather than on a name suffix: a naming
/// convention can be satisfied by a file rename, a shared behavior cannot.
#[test]
fn a_reference_lenient_case_is_paired_with_a_direction_pinning_twin() {
    let registry = real_registry();
    let cases = declarative(&registry);

    let lenient: Vec<&&TestCase> = cases
        .iter()
        .filter(|c| c.input_class == Some(InputClass::ReferenceLenient))
        .collect();
    assert!(
        lenient.len() >= 5,
        "the reference-lenient class must be exercised across the workflow, not once; \
         found {}",
        lenient.len()
    );

    for case in &lenient {
        assert_eq!(
            case.oracle_type,
            Some(OracleType::LiveDifferential),
            "{}: leniency is a claim about the REFERENCE, so it needs a differential",
            case.id
        );
        let twin = cases.iter().find(|other| {
            other.id != case.id
                && other.oracle_type == Some(OracleType::SpecExpectation)
                && other.behaviors.iter().any(|b| case.behaviors.contains(b))
        });
        assert!(
            twin.is_some(),
            "{}: no spec-expectation case shares a behavior with it, so the difference is \
             recorded without a direction — exactly the 023 T074 defect (FR-043). Linked \
             behaviors: {:?}",
            case.id,
            case.behaviors
        );
    }
}

// ---------------------------------------------------------------------------
// T076 — scenario 4, Assumption 5
// ---------------------------------------------------------------------------

/// An operation with no runnable differential uses `spec-expectation`, and the report
/// STATES the substitution.
///
/// Both halves matter. Without the first, a case would compare deacon against a usage
/// error and call the result a difference. Without the second, such an operation reads as
/// simply under-tested, and the reader has no way to tell "nothing to compare against"
/// from "nobody got to it".
#[test]
fn an_operation_with_no_reference_equivalent_substitutes_the_spec_expectation() {
    let registry = real_registry();
    let reports = reports_for(&registry);
    let cases = declarative(&registry);

    let substituted: Vec<&str> = reports
        .operations
        .operations
        .iter()
        .filter(|o| !o.differential_available)
        .map(|o| o.operation.as_str())
        .collect();
    assert!(
        !substituted.is_empty(),
        "at least one operation has no runnable differential against the pinned oracle \
         (`down` and `doctor` do not exist there at all); the report claims otherwise"
    );

    for entry in &reports.operations.operations {
        match differential_substitution(&entry.operation) {
            None => assert!(
                entry.differential_substitution.is_none(),
                "`{}` has a runnable differential but the report offers a substitution",
                entry.operation
            ),
            Some(expected) => {
                let stated = entry
                    .differential_substitution
                    .as_deref()
                    .unwrap_or_default();
                assert_eq!(
                    stated, expected,
                    "`{}` must state WHY it is not compared against the reference",
                    entry.operation
                );
                assert!(
                    stated.len() > 40,
                    "`{}`'s substitution note says nothing a reader could act on: {stated:?}",
                    entry.operation
                );
            }
        }
    }

    // And no case under such an operation claims a differential it could never run.
    for case in &cases {
        let Some(operation) = case.scenario_context.get(OPERATION_DIMENSION) else {
            continue;
        };
        if differential_substitution(operation).is_some() {
            assert_ne!(
                case.oracle_type,
                Some(OracleType::LiveDifferential),
                "{}: operation `{operation}` has no invocable reference command, so a \
                 differential would compare deacon against a usage error",
                case.id
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T077 — scenario 5, SC-011
// ---------------------------------------------------------------------------

/// A case whose observable output could vary between runs still produces the same verdict
/// every time — asserted **per case**, not in aggregate.
///
/// Repetition is the live tier's job; what is checkable here is the thing repetition would
/// expose. A verdict moves between runs when the case pinned something the run does not
/// control, so each case is inspected for the three ways that happens:
///
/// 1. a **snapshot** oracle over a channel whose evidence is not byte-stable (container
///    ids, compose project names, image ids — the reason `chan-container-state` and the
///    other Docker channels carry that warning in their own declaration);
/// 2. an assertion embedding a **run-varying literal** — an absolute temp path, a
///    container id, a timestamp — which is green exactly once;
/// 3. a Docker case with no isolated workspace, whose resources collide with a concurrent
///    case's and whose verdict then depends on scheduling.
///
/// Per case rather than in aggregate because an aggregate assertion names the suite when
/// it fails, and the suite is not what needs fixing.
#[test]
fn every_case_pins_only_run_invariant_evidence() {
    /// Channels whose evidence is not byte-stable across runs, so a committed snapshot of
    /// them would be stale the moment it was recorded on a second machine.
    const VOLATILE_CHANNELS: &[&str] = &[
        "chan-container-state",
        "chan-process-graph",
        "chan-injected-process",
        "chan-temporal",
        "chan-image",
    ];

    let registry = real_registry();
    for case in declarative(&registry) {
        if case.oracle_type == Some(OracleType::Snapshot) {
            for expectation in &case.expected {
                assert!(
                    !VOLATILE_CHANNELS.contains(&expectation.channel.as_str()),
                    "{}: snapshots channel {:?}, whose evidence carries container ids / \
                     project names / image ids and therefore differs on every machine — \
                     the verdict would move without the behavior changing",
                    case.id,
                    expectation.channel
                );
            }
        }

        for expectation in &case.expected {
            let Some(assertion) = &expectation.assertion else {
                continue;
            };
            let rendered = assertion.to_string();
            for marker in ["/tmp/", "/var/folders/", "deacon-conf-"] {
                assert!(
                    !rendered.contains(marker),
                    "{}: assertion on {:?} embeds {marker:?}, a path the run creates — it \
                     can be green at most once",
                    case.id,
                    expectation.channel
                );
            }
            // A bare 64-hex run is a container or image id; a 12-hex one is a short id.
            let hex_run = rendered
                .split(|c: char| !c.is_ascii_hexdigit())
                .map(str::len)
                .max()
                .unwrap_or(0);
            assert!(
                hex_run < 64,
                "{}: assertion on {:?} embeds a {hex_run}-character hex run, which reads \
                 as a container or image id and changes every run",
                case.id,
                expectation.channel
            );
        }

        // A case that touches the runtime must declare a Docker group: that field is the
        // only thing that gives it an isolated workspace, and without one two concurrent
        // cases share a `devcontainer.local_folder` label.
        let touches_runtime = case.operations.iter().any(|op| {
            matches!(
                op.subcommand.as_str(),
                "up" | "down" | "exec" | "build" | "run-user-commands"
            )
        });
        if touches_runtime {
            assert!(
                matches!(
                    case.resource_group,
                    Some(ResourceGroup::DockerShared) | Some(ResourceGroup::DockerExclusive)
                ),
                "{}: creates container resources with no Docker resourceGroup, so it runs \
                 against the committed fixture tree and collides with any concurrent case",
                case.id
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T078 — SC-003, FR-015
// ---------------------------------------------------------------------------

/// Every high-risk triple is covered by an EXECUTABLE case — never by a rationale, a
/// waiver, or a gap.
///
/// This is what makes selecting a triple mean something. V29 already refuses a rationale
/// or a waiver on one, so the failure this test exists to catch is the remaining one: a
/// triple dispositioned `gap`, which is honest, permitted by FR-015, and still leaves
/// SC-003 unmet.
#[test]
fn every_high_risk_triple_is_covered_by_an_executable_case() {
    let registry = real_registry();
    assert!(
        registry.triples.len() >= 12,
        "SC-003 requires at least twelve explicitly selected high-risk triples; found {}",
        registry.triples.len()
    );

    let executable = executable_case_ids(&registry);
    let inventory = generate_obligations(&registry).expect("obligations generate");
    let outcomes = evaluate_obligations(&registry, &inventory);
    let by_id: BTreeMap<&str, &_> = outcomes
        .iter()
        .map(|o| (o.obligation.id.as_str(), o))
        .collect();

    for triple in &registry.triples {
        assert!(
            triple.reason.split_whitespace().count() >= 20,
            "{}: the selection reason must be reviewable — a triple nobody can argue with \
             makes SC-003 a formality; got {:?}",
            triple.id,
            triple.reason
        );
        let obligation = triple_obligation_id(triple)
            .unwrap_or_else(|| panic!("{}: names no operation", triple.id));
        let outcome = by_id
            .get(obligation.as_str())
            .unwrap_or_else(|| panic!("{}: generated no obligation {obligation}", triple.id));
        assert_eq!(
            outcome.bucket,
            ObligationBucket::Covered,
            "{}: is `{}`, not covered by a case. Triples are selected precisely because \
             interaction defects hide in them, so an argument — or an admission — cannot \
             substitute for evidence (FR-015/SC-003)",
            triple.id,
            outcome.bucket.as_str()
        );
        for case_id in &outcome.by {
            assert!(
                executable.contains(case_id.as_str()),
                "{}: is covered by {case_id:?}, which is not executable — a pointer at a \
                 retired carrier is not evidence that the interaction behaves",
                triple.id
            );
        }
    }

    // And the report carries the same verdict, with the reason, so the selection is
    // reviewable from the artifact rather than only from the records.
    let reports = reports_for(&registry);
    assert_eq!(reports.triples.summary.selected, registry.triples.len());
    assert_eq!(reports.triples.summary.gap, 0);
    assert_eq!(reports.triples.summary.other, 0);
    assert_eq!(
        reports.triples.summary.covered,
        registry.triples.len(),
        "the triples report must agree with the obligation evaluation"
    );
    for row in &reports.triples.triples {
        assert!(
            !row.reason.trim().is_empty(),
            "{}: the report drops the selection reason, which is the whole point of \
             carrying it (FR-016)",
            row.id
        );
        assert!(!row.by.is_empty(), "{}: covered by nothing named", row.id);
    }

    // The rendered markdown is what a reviewer actually opens.
    let md = deacon_conformance::coverage_report::render_triples_md(&reports.triples);
    for triple in &registry.triples {
        assert!(
            md.contains(&triple.id),
            "coverage-triples.md omits {}",
            triple.id
        );
    }
    let _ = workspace_root();
}
