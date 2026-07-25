//! T077 (US7, FR-034 / FR-037 / FR-047): the deletion predicate blocks on a single
//! `more-permissive` unit or a residual naming the carrier, and NAMES the unsatisfied
//! condition.
//!
//! This is the last gate before an irreversible act. "Not deletable" without a reason is
//! indistinguishable from "nobody looked", so every blocked verdict must say which
//! condition failed and which item failed it — otherwise the next person's cheapest path
//! forward is to assume it was noise.
//!
//! Hermetic: pure functions only. No Docker, no network, no oracle.

use parity_harness::equivalence::{DeletionVerdict, EquivalenceEntry, Relation, deletion_verdict};

const CARRIER: &str = "parity_corpus_tier1";

fn units(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| format!("{CARRIER}::{n}")).collect()
}

fn entry(case: &str, relation: Relation) -> EquivalenceEntry {
    EquivalenceEntry {
        unit: format!("{CARRIER}::{case}"),
        carrier: CARRIER.to_string(),
        legacy_outcome: "pass".to_string(),
        replacement_outcome: "agree".to_string(),
        relation,
        detail: (relation != Relation::Equivalent).then(|| "a difference".to_string()),
        characterized_as: (relation == Relation::Stricter).then(|| "wvr-x".to_string()),
    }
}

fn verdict(
    units: &[String],
    ledger: &[EquivalenceEntry],
    residuals: &[String],
    unaccounted: &[String],
) -> DeletionVerdict {
    deletion_verdict(CARRIER, units, ledger, residuals, unaccounted)
}

#[test]
fn a_fully_cleared_carrier_is_deletable() {
    let v = verdict(
        &units(&["a", "b"]),
        &[
            entry("a", Relation::Equivalent),
            entry("b", Relation::Stricter),
        ],
        &[],
        &[],
    );
    assert!(v.deletable, "{:#?}", v.unsatisfied);
    assert!(v.unsatisfied.is_empty());
}

#[test]
fn a_single_more_permissive_unit_blocks_and_names_it() {
    let v = verdict(
        &units(&["a", "b"]),
        &[
            entry("a", Relation::Equivalent),
            entry("b", Relation::MorePermissive),
        ],
        &[],
        &[],
    );
    assert!(!v.deletable, "one more-permissive unit is enough to block");
    let hit = v
        .unsatisfied
        .iter()
        .find(|u| u.contains("parity_corpus_tier1::b"))
        .unwrap_or_else(|| panic!("the offending unit must be named: {:#?}", v.unsatisfied));
    assert!(hit.starts_with("condition 2"), "{hit}");
    assert!(
        hit.contains("more-permissive"),
        "the relation must be named: {hit}"
    );
}

#[test]
fn a_residual_naming_the_carrier_blocks_and_names_it() {
    let v = verdict(
        &units(&["a"]),
        &[entry("a", Relation::Equivalent)],
        &["res-something".to_string()],
        &[],
    );
    assert!(!v.deletable);
    let hit = v
        .unsatisfied
        .iter()
        .find(|u| u.contains("res-something"))
        .unwrap_or_else(|| panic!("the residual must be named: {:#?}", v.unsatisfied));
    assert!(hit.starts_with("condition 3"), "{hit}");
}

#[test]
fn a_unit_with_no_verdict_blocks_because_unproven_is_not_safe() {
    // Condition 1. The failure mode this exists to stop: a carrier looking deletable
    // because nobody produced a verdict for half its units.
    let v = verdict(
        &units(&["a", "b"]),
        &[entry("a", Relation::Equivalent)],
        &[],
        &[],
    );
    assert!(!v.deletable);
    let hit = v
        .unsatisfied
        .iter()
        .find(|u| u.contains("parity_corpus_tier1::b"))
        .unwrap_or_else(|| panic!("{:#?}", v.unsatisfied));
    assert!(hit.starts_with("condition 1"), "{hit}");
    assert!(
        hit.contains("unproven is not the same as safe"),
        "the reason must be explicit: {hit}"
    );
}

#[test]
fn an_unaccounted_unit_blocks_via_condition_four() {
    let unit = format!("{CARRIER}::a");
    let v = verdict(
        &units(&["a"]),
        &[entry("a", Relation::Equivalent)],
        &[],
        std::slice::from_ref(&unit),
    );
    assert!(!v.deletable);
    assert!(
        v.unsatisfied
            .iter()
            .any(|u| u.starts_with("condition 4") && u.contains(&unit)),
        "{:#?}",
        v.unsatisfied
    );
}

#[test]
fn a_malformed_stricter_entry_blocks_deletion() {
    // A `stricter` relation with no characterization cannot support a deletion: the
    // improvement was suppressed rather than characterized (FR-036), so the evidence is
    // incomplete.
    let mut uncharacterized = entry("a", Relation::Stricter);
    uncharacterized.characterized_as = None;
    let v = verdict(&units(&["a"]), &[uncharacterized], &[], &[]);
    assert!(!v.deletable);
    assert!(
        v.unsatisfied.iter().any(|u| u.contains("characterizedAs")),
        "{:#?}",
        v.unsatisfied
    );
}

#[test]
fn every_unsatisfied_condition_is_reported_not_just_the_first() {
    // A blocked deletion that reports only the first problem sends the reader round the
    // loop once per problem.
    let v = verdict(
        &units(&["a", "b"]),
        &[entry("a", Relation::MorePermissive)],
        &["res-one".to_string(), "res-two".to_string()],
        &[format!("{CARRIER}::a")],
    );
    assert!(!v.deletable);
    assert!(
        v.unsatisfied.len() >= 4,
        "expected the more-permissive unit, the unverdicted unit, both residuals and the \
         unaccounted unit: {:#?}",
        v.unsatisfied
    );
    for expected in ["condition 1", "condition 2", "condition 3", "condition 4"] {
        assert!(
            v.unsatisfied.iter().any(|u| u.starts_with(expected)),
            "{expected} must be reported: {:#?}",
            v.unsatisfied
        );
    }
}

#[test]
fn an_unknown_carrier_is_not_trivially_deletable() {
    // An empty unit list means the caller asked about a program the baseline does not
    // know — a wiring error. Returning "deletable" for it would make a typo look like a
    // clearance.
    let v = verdict(&[], &[], &[], &[]);
    assert!(!v.deletable);
    assert!(
        v.unsatisfied
            .iter()
            .any(|u| u.contains("carries no baseline unit")),
        "{:#?}",
        v.unsatisfied
    );
}

#[test]
fn a_verdict_for_another_carrier_does_not_clear_this_one() {
    // Ledger entries are carrier-scoped: a cleared unit belonging to a DIFFERENT carrier
    // must not satisfy this carrier's condition 1.
    let mut foreign = entry("a", Relation::Equivalent);
    foreign.carrier = "some_other_binary".to_string();
    let v = verdict(&units(&["a"]), &[foreign], &[], &[]);
    assert!(
        !v.deletable,
        "a verdict recorded against another carrier proves nothing here"
    );
}
