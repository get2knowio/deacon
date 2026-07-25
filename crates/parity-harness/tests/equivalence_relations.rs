//! T076 + T078 (US7, spec A-002 / FR-036): relations are classified on **outcome**, and a
//! `stricter` relation without a characterization fails.
//!
//! The comparison basis is the load-bearing detail. Two paths that both fail will word
//! the failure differently, order their findings differently, and format them differently
//! — none of which is a relation. Classifying on message text would make every wording
//! change look like a behavior change, and (worse) would let a cosmetic rewrite of the
//! legacy summary "prove" a difference that does not exist.
//!
//! Hermetic: pure functions only. No Docker, no network, no oracle.

use parity_harness::equivalence::{
    ComparisonOutcome, EquivalenceEntry, EquivalenceLedger, Relation, classify_relation,
};

fn entry(
    relation: Relation,
    detail: Option<&str>,
    characterized: Option<&str>,
) -> EquivalenceEntry {
    EquivalenceEntry {
        unit: "p::a".to_string(),
        carrier: "p".to_string(),
        legacy_outcome: "pass".to_string(),
        replacement_outcome: "diverge".to_string(),
        relation,
        detail: detail.map(str::to_string),
        characterized_as: characterized.map(str::to_string),
    }
}

#[test]
fn identical_outcomes_are_equivalent_whether_clean_or_diverging() {
    assert_eq!(
        classify_relation(ComparisonOutcome::Clean, ComparisonOutcome::Clean),
        Some(Relation::Equivalent)
    );
    assert_eq!(
        classify_relation(ComparisonOutcome::Difference, ComparisonOutcome::Difference),
        Some(Relation::Equivalent),
        "both detecting a difference is as equivalent as both detecting none — what \
         matters is whether a difference was DETECTED"
    );
}

#[test]
fn a_difference_only_the_replacement_sees_is_stricter() {
    assert_eq!(
        classify_relation(ComparisonOutcome::Clean, ComparisonOutcome::Difference),
        Some(Relation::Stricter)
    );
    assert!(
        Relation::Stricter.permits_deletion(),
        "stricter is permitted"
    );
}

#[test]
fn a_difference_only_the_legacy_path_sees_is_more_permissive_and_blocks() {
    assert_eq!(
        classify_relation(ComparisonOutcome::Difference, ComparisonOutcome::Clean),
        Some(Relation::MorePermissive)
    );
    assert!(
        !Relation::MorePermissive.permits_deletion(),
        "a replacement that misses a difference the legacy path catches must block \
         deletion (FR-035)"
    );
}

#[test]
fn an_incomplete_run_yields_no_relation_at_all() {
    // "We could not check" is not "it is fine". An unclassifiable unit stays unproven,
    // which fails deletion condition 1 rather than quietly permitting it.
    for other in [
        ComparisonOutcome::Clean,
        ComparisonOutcome::Difference,
        ComparisonOutcome::Error,
    ] {
        assert_eq!(classify_relation(ComparisonOutcome::Error, other), None);
        assert_eq!(classify_relation(other, ComparisonOutcome::Error), None);
    }
}

#[test]
fn classification_is_on_outcome_not_message_text() {
    // The property spec A-002 names. Every outcome name below reduces to the SAME
    // comparison state, so no relation can depend on which spelling a path used.
    for clean in ["pass", "pass-waived", "agree", "allowed-difference"] {
        assert_eq!(
            ComparisonOutcome::from_outcome_name(clean),
            ComparisonOutcome::Clean,
            "{clean} is a clean outcome"
        );
    }
    for difference in ["diverge", "divergence", "fail"] {
        assert_eq!(
            ComparisonOutcome::from_outcome_name(difference),
            ComparisonOutcome::Difference,
            "{difference} is a reported difference"
        );
    }
    // A tolerated difference is CLEAN, not a difference: the tolerance is a decision
    // already made and reviewed, not a difference left undetected.
    assert_eq!(
        classify_relation(
            ComparisonOutcome::from_outcome_name("pass-waived"),
            ComparisonOutcome::from_outcome_name("allowed-difference")
        ),
        Some(Relation::Equivalent)
    );
}

#[test]
fn an_unrecognized_outcome_is_an_error_not_a_pass() {
    // Fail-closed. A state we cannot interpret must never be read as agreement — that is
    // how an unproven unit becomes a deleted carrier.
    for unknown in [
        "",
        "ok",
        "success",
        "stale",
        "no-reference-for-platform",
        "error",
    ] {
        assert_eq!(
            ComparisonOutcome::from_outcome_name(unknown),
            ComparisonOutcome::Error,
            "{unknown:?} must not be read as clean"
        );
        assert!(!ComparisonOutcome::from_outcome_name(unknown).is_classifiable());
    }
}

// ---------------------------------------------------------------------------
// T078: a `stricter` relation without `characterizedAs` fails (FR-036)
// ---------------------------------------------------------------------------

#[test]
fn a_stricter_relation_without_a_characterization_is_a_defect() {
    let defects = entry(Relation::Stricter, Some("a new difference"), None).defects();
    assert_eq!(defects.len(), 1, "{defects:?}");
    assert!(
        defects[0].contains("characterizedAs"),
        "an uncharacterized improvement is suppression, not an improvement: {}",
        defects[0]
    );

    let blank = entry(Relation::Stricter, Some("a new difference"), Some("   ")).defects();
    assert!(
        !blank.is_empty(),
        "a blank characterization is as absent as a missing one"
    );

    assert!(
        entry(Relation::Stricter, Some("a new difference"), Some("wvr-x"))
            .defects()
            .is_empty(),
        "a characterized improvement is well-formed"
    );
}

#[test]
fn a_reported_difference_without_a_detail_is_a_defect() {
    for relation in [Relation::Stricter, Relation::MorePermissive] {
        let defects = entry(relation, None, Some("wvr-x")).defects();
        assert!(
            defects.iter().any(|d| d.contains("`detail`")),
            "{relation:?} must explain the difference it reports: {defects:?}"
        );
    }
    assert!(
        entry(Relation::Equivalent, None, None).defects().is_empty(),
        "equivalent reports no difference, so it needs no detail"
    );
}

#[test]
fn the_ledger_round_trips_and_rejects_unknown_fields() {
    let ledger = EquivalenceLedger {
        baseline_revision: "98c26a5".to_string(),
        entries: vec![entry(
            Relation::Stricter,
            Some("configFilePath is now compared"),
            Some("wvr-x"),
        )],
    };
    let json = serde_json::to_string(&ledger).expect("serializes");
    let back: EquivalenceLedger = serde_json::from_str(&json).expect("round-trips");
    assert_eq!(back, ledger);

    let bad = json.replace("\"detail\"", "\"surprise\"");
    assert!(
        serde_json::from_str::<EquivalenceLedger>(&bad).is_err(),
        "unknown fields must be rejected"
    );
}

// NOTE: there is deliberately NO test here that reads `target/parity/equivalence.json`.
// The ledger is a git-ignored artifact of a live parity run, so a hermetic test that read
// it would pass or fail depending on whether someone happened to produce one locally —
// and would report a malformed ledger as a unit-test failure on one machine and nothing
// at all on another. Ledger well-formedness is enforced where the ledger exists: the
// `equivalence-report` bin exits non-zero on any malformed entry, and `migration check
// --ledger <file>` folds it into the conservation report on demand.
