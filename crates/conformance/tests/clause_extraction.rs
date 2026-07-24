//! Acceptance tests for clause extraction/canonicalization (US1, T011; FR-004/FR-005/
//! FR-006). Drives `generate_clauses` over the purpose-built fixture prose under
//! `fixtures/conformance/prose/` and asserts the atomicity, strength-detection, and
//! ambiguity properties the spec mandates. Hermetic — no Docker, no network, no model.

use deacon_conformance::clause::generate_clauses;
use deacon_conformance::model::{ClauseUnit, Strength, Testability};
use deacon_conformance::workspace_root;

fn fixture_units() -> Vec<ClauseUnit> {
    let base = workspace_root().join("fixtures/conformance/prose");
    generate_clauses(&base, &base.join("clauses.json"))
        .expect("fixture clauses canonicalize")
        .units
}

fn find<'a>(units: &'a [ClauseUnit], needle: &str) -> &'a ClauseUnit {
    units
        .iter()
        .find(|u| u.locations.iter().any(|l| l.excerpt.contains(needle)))
        .unwrap_or_else(|| panic!("no clause with excerpt containing {needle:?}"))
}

#[test]
fn multi_requirement_paragraph_splits_into_distinct_clauses() {
    let units = fixture_units();
    // The single Lifecycle paragraph states two independent MUST obligations; each is its
    // own atomic clause with a distinct id.
    let once = find(&units, "run onCreateCommand exactly once");
    let not_again = find(&units, "MUST NOT run it again");
    assert_ne!(once.id, not_again.id, "two obligations → two clauses");
    assert_eq!(once.strength, Strength::Must);
    assert_eq!(not_again.strength, Strength::Must);
}

#[test]
fn strength_families_are_detected_and_recorded() {
    let units = fixture_units();
    assert_eq!(
        find(&units, "run onCreateCommand exactly once").strength,
        Strength::Must
    );
    // The fenced I/O contract is recorded as an io-contract clause.
    let io = find(&units, "\"outcome\": \"success\"");
    assert_eq!(io.strength, Strength::IoContract);
    assert_eq!(
        io.context.as_ref().and_then(|c| c.get("inCodeFence")),
        Some(&serde_json::Value::Bool(true)),
        "the I/O contract is flagged as living in a code fence"
    );
}

#[test]
fn hedged_language_is_surfaced_as_ambiguous_not_promoted_to_must() {
    let units = fixture_units();
    let hedged = find(&units, "should generally complete quickly");
    assert_eq!(
        hedged.testability,
        Testability::Ambiguous,
        "hedged language must surface as ambiguous"
    );
    assert_ne!(
        hedged.strength,
        Strength::Must,
        "ambiguous language must NEVER be auto-promoted to a strict MUST"
    );
}

#[test]
fn descriptive_text_is_recorded_as_informative_not_a_requirement() {
    let units = fixture_units();
    let descriptive = find(&units, "purely descriptive and");
    assert_eq!(descriptive.strength, Strength::Descriptive);
    assert_eq!(descriptive.testability, Testability::Informative);
}

#[test]
fn every_fixture_document_is_represented() {
    let units = fixture_units();
    assert!(units.iter().any(|u| u.document == "sample"));
    assert!(
        units.iter().any(|u| u.document == "authoring-sample"),
        "authoring-scope documents are inventoried in full, not skipped"
    );
}
