//! T029 (US2, FR-024 / FR-027 / FR-028 / FR-051 / FR-047): every characterized
//! exception survives the migration, mapped to **exactly one** mechanism, tolerating
//! **no more** than it did before.
//!
//! Three failures, each with its own leg:
//!
//! - **zero mechanisms** — the exception was dropped. A tolerance that vanishes silently
//!   turns a characterized divergence into an unexplained pass or an unexplained fail;
//! - **more than one mechanism** — the exception was merged. Merging two tolerances
//!   widens both: each mechanism now covers the union of what they separately covered;
//! - **broadened direction or scope** — the mechanism survived but now lets more
//!   through. This is the quiet one: nothing is missing, the record is present, and the
//!   bar moved anyway.
//!
//! Breadth is a structural order, not a string comparison: an agreement expectation
//! tolerates no directional difference, a one-directional expectation tolerates one, and
//! a field divergence tolerates a value difference in either direction. Likewise a
//! single-case scope is narrower than a whole-corpus scope, which is narrower than a
//! whole-behavior scope.
//!
//! Hermetic: pure functions over synthetic records plus a read of the real registry.

use std::collections::{BTreeMap, BTreeSet};

use deacon_conformance::default_registry_dir;
use deacon_conformance::load::Registry;
use deacon_conformance::mapping::{
    ExceptionDisposition, ExceptionMapping, MechanismForm, check_exception_mappings,
    direction_breadth, scope_breadth,
};
use deacon_conformance::validate::check_mapping as check_registry_mapping;

fn known(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|s| (*s).to_string()).collect()
}

fn mechanism(id: &str, direction: &str, scope: &str) -> (String, MechanismForm) {
    (
        id.to_string(),
        MechanismForm {
            id: id.to_string(),
            direction: direction.to_string(),
            scope: scope.to_string(),
        },
    )
}

fn entry(exception: &str, mechanisms: &[&str], direction: &str, scope: &str) -> ExceptionMapping {
    ExceptionMapping {
        exception: exception.to_string(),
        disposition: ExceptionDisposition::Preserved,
        mechanisms: mechanisms.iter().map(|m| (*m).to_string()).collect(),
        preserved_direction: direction.to_string(),
        preserved_scope: scope.to_string(),
        rationale: "probe".to_string(),
    }
}

#[test]
fn a_well_formed_preserved_exception_passes() {
    let mechanisms: BTreeMap<_, _> = [mechanism(
        "wvr-a",
        "deacon-stricter",
        "corpus_case:errors/a",
    )]
    .into_iter()
    .collect();
    let problems = check_exception_mappings(
        &[entry(
            "wvr-a",
            &["wvr-a"],
            "deacon-stricter",
            "corpus_case:errors/a",
        )],
        &known(&["wvr-a"]),
        &mechanisms,
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn an_exception_mapped_to_zero_mechanisms_fails() {
    let problems = check_exception_mappings(
        &[entry(
            "wvr-a",
            &[],
            "deacon-stricter",
            "corpus_case:errors/a",
        )],
        &known(&["wvr-a"]),
        &BTreeMap::new(),
    );
    assert!(
        problems
            .iter()
            .any(|p| p.code == "V21" && p.message.contains("EXACTLY ONE mechanism, found 0")),
        "an exception with no mechanism is dropped coverage: {problems:#?}"
    );
}

#[test]
fn an_exception_mapped_to_two_mechanisms_fails() {
    let mechanisms: BTreeMap<_, _> = [
        mechanism("wvr-a", "deacon-stricter", "corpus_case:errors/a"),
        mechanism("wvr-b", "deacon-stricter", "corpus_case:errors/b"),
    ]
    .into_iter()
    .collect();
    let problems = check_exception_mappings(
        &[entry(
            "wvr-a",
            &["wvr-a", "wvr-b"],
            "deacon-stricter",
            "corpus_case:errors/a",
        )],
        &known(&["wvr-a"]),
        &mechanisms,
    );
    assert!(
        problems
            .iter()
            .any(|p| p.code == "V21" && p.message.contains("EXACTLY ONE mechanism, found 2")),
        "merging mechanisms widens both: {problems:#?}"
    );
}

#[test]
fn an_exception_with_no_mapping_entry_at_all_fails() {
    let problems = check_exception_mappings(&[], &known(&["wvr-orphan"]), &BTreeMap::new());
    assert!(
        problems
            .iter()
            .any(|p| p.record == "wvr-orphan" && p.message.contains("no mapping entry")),
        "every pre-migration exception must be explicitly dispositioned (FR-028): {problems:#?}"
    );
}

#[test]
fn a_broadened_direction_fails() {
    // Pre-migration the waiver tolerated only an agreement (`both-accept`); the
    // mechanism now tolerates an arbitrary value difference.
    let mechanisms: BTreeMap<_, _> = [mechanism(
        "wvr-a",
        "field-divergence",
        "corpus_case:errors/a",
    )]
    .into_iter()
    .collect();
    let problems = check_exception_mappings(
        &[entry(
            "wvr-a",
            &["wvr-a"],
            "both-accept",
            "corpus_case:errors/a",
        )],
        &known(&["wvr-a"]),
        &mechanisms,
    );
    assert!(
        problems
            .iter()
            .any(|p| p.code == "V21" && p.message.contains("BROADER than the recorded")),
        "a widened direction must fail (FR-027): {problems:#?}"
    );
}

#[test]
fn a_broadened_scope_fails() {
    // Pre-migration the waiver applied to ONE case; the mechanism now applies to every
    // case of the behavior.
    let mechanisms: BTreeMap<_, _> = [mechanism("wvr-a", "deacon-stricter", "behavior:bhv-x")]
        .into_iter()
        .collect();
    let problems = check_exception_mappings(
        &[entry(
            "wvr-a",
            &["wvr-a"],
            "deacon-stricter",
            "corpus_case:errors/a",
        )],
        &known(&["wvr-a"]),
        &mechanisms,
    );
    assert!(
        problems
            .iter()
            .any(|p| p.code == "V21" && p.message.contains("BROADER than the recorded")),
        "a widened scope must fail (FR-027): {problems:#?}"
    );
}

#[test]
fn a_narrowed_tolerance_is_permitted() {
    // The converse: narrowing is a strictness improvement and must NOT fail.
    let mechanisms: BTreeMap<_, _> = [mechanism("wvr-a", "both-accept", "corpus_case:errors/a")]
        .into_iter()
        .collect();
    let problems = check_exception_mappings(
        &[entry(
            "wvr-a",
            &["wvr-a"],
            "field-divergence",
            "corpus:errors",
        )],
        &known(&["wvr-a"]),
        &mechanisms,
    );
    assert!(
        problems.is_empty(),
        "narrowing a tolerance is always allowed: {problems:#?}"
    );
}

#[test]
fn an_unrecognized_direction_or_scope_is_treated_as_maximally_broad() {
    // Fail-closed: an unreviewed spelling must never slip through as "narrow".
    assert_eq!(direction_breadth("mystery"), u8::MAX);
    assert_eq!(scope_breadth("everything"), u8::MAX);
    assert!(direction_breadth("both-accept") < direction_breadth("deacon-stricter"));
    assert!(direction_breadth("deacon-stricter") < direction_breadth("field-divergence"));
    assert!(scope_breadth("corpus_case:a/b") < scope_breadth("corpus:a"));
    assert!(scope_breadth("corpus:a") < scope_breadth("behavior:bhv-x"));
}

#[test]
fn a_no_counterpart_exception_names_no_mechanism() {
    let mut no_counterpart = entry(
        "wvr-a",
        &["wvr-a"],
        "deacon-stricter",
        "corpus_case:errors/a",
    );
    no_counterpart.disposition = ExceptionDisposition::NoCounterpart;
    let problems =
        check_exception_mappings(&[no_counterpart], &known(&["wvr-a"]), &BTreeMap::new());
    assert!(
        problems
            .iter()
            .any(|p| p.message.contains("must name no mechanism")),
        "{problems:#?}"
    );
}

#[test]
fn every_real_exception_is_mapped_to_exactly_one_preserved_mechanism() {
    let registry =
        Registry::load(&default_registry_dir()).expect("the real registry loads cleanly");

    let expected = registry.waivers.len() + registry.extensions.len();
    assert_eq!(
        registry.mapping_exceptions.len(),
        expected,
        "all {expected} characterized exceptions ({} waivers + {} extensions) must be \
         mapped",
        registry.waivers.len(),
        registry.extensions.len()
    );
    for entry in &registry.mapping_exceptions {
        assert_eq!(
            entry.disposition,
            ExceptionDisposition::Preserved,
            "{} is not preserved; a no-counterpart disposition needs its own review",
            entry.exception
        );
        assert_eq!(
            entry.mechanisms,
            vec![entry.exception.clone()],
            "{} must be preserved in place, by exactly one mechanism",
            entry.exception
        );
    }

    let violations = check_registry_mapping(&registry);
    let exception_ids: BTreeSet<&str> = registry
        .mapping_exceptions
        .iter()
        .map(|e| e.exception.as_str())
        .collect();
    let hits: Vec<_> = violations
        .iter()
        .filter(|v| exception_ids.contains(v.record.as_str()))
        .collect();
    assert!(
        hits.is_empty(),
        "no exception correspondence may be broken:\n{hits:#?}"
    );
}
