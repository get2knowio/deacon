//! T050 (US3, FR-016): two behaviors that are indistinguishable are **reported** as
//! suspected duplicates, for merge or explicit differentiation.
//!
//! This is deliberately a report and not a violation. A validator cannot decide whether
//! two similar-reading claims are one claim written twice or two real claims that need
//! better prose — but it can surface the pair so a human decides. Blocking would push
//! authors toward padding statements with noise words to escape the check, which
//! produces worse prose AND keeps the duplication.
//!
//! The detector compares *substance*, not text: a statement's lowercased word bag minus
//! structural filler. Two statements that differ only in wording or word order are the
//! same claim; two that differ in a single content word are not.
//!
//! Hermetic: reads the real registry and evaluates pure functions.

use deacon_conformance::conservation::{DuplicateReason, suspected_duplicate_behaviors};
use deacon_conformance::default_registry_dir;
use deacon_conformance::load::Registry;
use deacon_conformance::model::{BehaviorUnit, Decision, ReferenceStatus, SpecStatus, TestCase};

fn behavior(id: &str, statement: &str) -> BehaviorUnit {
    BehaviorUnit {
        id: id.to_string(),
        area: "probe".to_string(),
        statement: statement.to_string(),
        applicability: Vec::new(),
        spec: SpecStatus::Conformant,
        reference: ReferenceStatus::Aligned,
        decision: Decision::FollowSpec,
        notes: None,
    }
}

fn case_for(id: &str, behaviors: &[&str]) -> TestCase {
    TestCase {
        id: id.to_string(),
        behaviors: behaviors.iter().map(|b| (*b).to_string()).collect(),
        ..TestCase::default()
    }
}

/// A registry carrying only the behaviors/cases under test, so the detector's verdict is
/// attributable to them and nothing else.
fn probe_registry(behaviors: Vec<BehaviorUnit>, cases: Vec<TestCase>) -> Registry {
    Registry {
        behaviors,
        cases,
        ..Registry::default()
    }
}

#[test]
fn two_behaviors_with_the_same_substance_and_coverage_are_reported() {
    let registry = probe_registry(
        vec![
            behavior("bhv-a", "deacon rejects a cyclic extends chain."),
            behavior("bhv-b", "A cyclic extends chain is rejected by deacon!"),
        ],
        vec![case_for("case-x", &["bhv-a", "bhv-b"])],
    );

    let duplicates = suspected_duplicate_behaviors(&registry);
    assert_eq!(duplicates.len(), 1, "{duplicates:#?}");
    assert_eq!(
        duplicates[0].behaviors,
        ("bhv-a".to_string(), "bhv-b".to_string())
    );
    assert_eq!(
        duplicates[0].reason,
        DuplicateReason::IdenticalStatementAndCoverage,
        "identical substance AND identical coverage leaves nothing to tell them apart"
    );
}

#[test]
fn the_same_substance_with_different_coverage_is_reported_more_weakly() {
    let registry = probe_registry(
        vec![
            behavior("bhv-a", "deacon rejects a cyclic extends chain"),
            behavior("bhv-b", "a cyclic extends chain deacon rejects"),
        ],
        vec![
            case_for("case-x", &["bhv-a"]),
            case_for("case-y", &["bhv-b"]),
        ],
    );

    let duplicates = suspected_duplicate_behaviors(&registry);
    assert_eq!(duplicates.len(), 1);
    assert_eq!(
        duplicates[0].reason,
        DuplicateReason::IdenticalStatement,
        "different coverage is a weaker signal than identical coverage"
    );
    assert_eq!(
        duplicates[0].reason.as_str(),
        "identical-statement",
        "the reason has a stable wire spelling for the report"
    );
}

#[test]
fn behaviors_differing_by_a_content_word_are_not_duplicates() {
    let registry = probe_registry(
        vec![
            behavior("bhv-a", "deacon rejects a cyclic extends chain"),
            behavior("bhv-b", "deacon rejects a missing extends target"),
        ],
        vec![case_for("case-x", &["bhv-a", "bhv-b"])],
    );
    assert!(
        suspected_duplicate_behaviors(&registry).is_empty(),
        "a single differing content word makes two distinct claims"
    );
}

#[test]
fn structural_filler_alone_never_differentiates() {
    // Padding a statement with filler must NOT be a way to escape the detector — that is
    // the gaming move this normalization exists to defeat.
    let registry = probe_registry(
        vec![
            behavior("bhv-a", "build produces an image"),
            behavior(
                "bhv-b",
                "the build is that which produces an image, and so on, for it",
            ),
        ],
        vec![case_for("case-x", &["bhv-a", "bhv-b"])],
    );
    assert_eq!(
        suspected_duplicate_behaviors(&registry).len(),
        1,
        "filler words must not differentiate two identical claims"
    );
}

#[test]
fn three_indistinguishable_behaviors_report_every_pair() {
    let registry = probe_registry(
        vec![
            behavior("bhv-a", "exec runs a command in the container"),
            behavior("bhv-b", "a command runs in the container via exec"),
            behavior("bhv-c", "in the container, exec runs a command"),
        ],
        vec![case_for("case-x", &["bhv-a", "bhv-b", "bhv-c"])],
    );
    let duplicates = suspected_duplicate_behaviors(&registry);
    assert_eq!(duplicates.len(), 3, "three behaviors → three pairs");
    let pairs: Vec<(String, String)> = duplicates.iter().map(|d| d.behaviors.clone()).collect();
    assert_eq!(
        pairs,
        vec![
            ("bhv-a".to_string(), "bhv-b".to_string()),
            ("bhv-a".to_string(), "bhv-c".to_string()),
            ("bhv-b".to_string(), "bhv-c".to_string()),
        ],
        "pairs are reported deterministically, ID-sorted"
    );
}

#[test]
fn the_real_registry_has_no_suspected_duplicate_behaviors() {
    let registry =
        Registry::load(&default_registry_dir()).expect("the real registry loads cleanly");
    let duplicates = suspected_duplicate_behaviors(&registry);
    assert!(
        duplicates.is_empty(),
        "the 25 behaviors must each make a distinct claim; suspected duplicates:\n{duplicates:#?}"
    );
}
