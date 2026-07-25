//! T025 (US2, FR-011 / FR-047): **V21** — the migration mapping has no orphans in
//! EITHER direction, and every migration destination is usable as evidence (T033).
//!
//! The two directions are genuinely different failures:
//!
//! - a baseline unit with **no mapping entry** is an orphan *test*: coverage that
//!   existed before the migration and now has nowhere to be;
//! - a declarative case **no mapping entry reaches** is an orphan *case*: a destination
//!   nothing was migrated into, which inflates the registry without conserving anything.
//!
//! Legacy (binary-backed) pointer cases are deliberately exempt from the second
//! direction — they are the pre-migration carriers, not destinations.
//!
//! Hermetic: pure functions over synthetic records plus a read of the real registry. No
//! Docker, no network, no oracle.

use std::collections::{BTreeMap, BTreeSet};

use deacon_conformance::mapping::{CaseFacts, Disposition, MigrationMapping, check_mapping};
use deacon_conformance::validate::{check_mapping as check_registry_mapping, check_residuals};
use deacon_conformance::{default_registry_dir, load::Registry};

fn behaviors() -> BTreeSet<String> {
    ["bhv-x".to_string()].into_iter().collect()
}

fn channels() -> BTreeSet<String> {
    ["chan-exit-code".to_string()].into_iter().collect()
}

fn good_case(id: &str) -> CaseFacts {
    CaseFacts {
        id: id.to_string(),
        behaviors: vec!["bhv-x".to_string()],
        channels: vec!["chan-exit-code".to_string()],
        fixtures: Vec::new(),
        declarative: true,
        context: Vec::new(),
        oracle: "SpecExpectation".to_string(),
        input_shape: format!("read-configuration [{id}]"),
    }
}

fn migrated(unit: &str, case_id: &str) -> MigrationMapping {
    MigrationMapping {
        unit: unit.to_string(),
        disposition: Disposition::Migrated,
        case_ids: vec![case_id.to_string()],
        residual_id: None,
        rationale: None,
        fixture_mapping: Vec::new(),
    }
}

fn run(
    units: &[&str],
    mapping: &[MigrationMapping],
    cases: &[CaseFacts],
) -> Vec<deacon_conformance::mapping::MappingProblem> {
    let unit_ids: Vec<String> = units.iter().map(|u| u.to_string()).collect();
    check_mapping(
        &unit_ids,
        mapping,
        cases,
        &BTreeSet::new(),
        &behaviors(),
        &channels(),
    )
}

#[test]
fn an_unmapped_baseline_unit_fails_naming_the_unit() {
    let problems = run(
        &["p::a", "p::b"],
        &[migrated("p::a", "case-a")],
        &[good_case("case-a")],
    );
    let hits: Vec<_> = problems.iter().filter(|p| p.record == "p::b").collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one problem naming p::b: {problems:#?}"
    );
    assert_eq!(hits[0].code, "V21");
    assert!(
        hits[0].message.contains("no mapping entry"),
        "the diagnosis must say the unit is unmapped, got: {}",
        hits[0].message
    );
}

#[test]
fn a_mapped_but_nonexistent_case_id_fails_naming_the_case() {
    let problems = run(
        &["p::a"],
        &[migrated("p::a", "case-ghost")],
        &[good_case("case-a")],
    );
    assert!(
        problems
            .iter()
            .any(|p| p.code == "V21" && p.message.contains("case-ghost")),
        "a mapping naming a nonexistent case must fail: {problems:#?}"
    );
}

#[test]
fn a_mapping_naming_a_nonexistent_baseline_unit_fails() {
    let problems = run(
        &["p::a"],
        &[migrated("p::a", "case-a"), migrated("p::ghost", "case-a")],
        &[good_case("case-a")],
    );
    assert!(
        problems
            .iter()
            .any(|p| p.record == "p::ghost" && p.message.contains("does not exist")),
        "a mapping naming a unit outside the baseline must fail: {problems:#?}"
    );
}

#[test]
fn a_declarative_case_no_unit_reaches_is_an_orphan_case() {
    let problems = run(
        &["p::a"],
        &[migrated("p::a", "case-a")],
        &[good_case("case-a"), good_case("case-unreached")],
    );
    let hits: Vec<_> = problems
        .iter()
        .filter(|p| p.record == "case-unreached")
        .collect();
    assert_eq!(hits.len(), 1, "{problems:#?}");
    assert!(hits[0].message.contains("orphan case"));
}

#[test]
fn a_legacy_pointer_case_is_exempt_from_the_orphan_case_direction() {
    let mut legacy = good_case("case-legacy-pointer");
    legacy.declarative = false;
    let problems = run(
        &["p::a"],
        &[migrated("p::a", "case-a")],
        &[good_case("case-a"), legacy],
    );
    assert!(
        !problems.iter().any(|p| p.record == "case-legacy-pointer"),
        "legacy pointer cases are pre-migration carriers, not destinations: {problems:#?}"
    );
}

#[test]
fn a_destination_case_without_a_behavior_or_channel_fails() {
    let mut no_behavior = good_case("case-nobhv");
    no_behavior.behaviors.clear();
    let mut no_channel = good_case("case-nochan");
    no_channel.channels.clear();

    let problems = run(
        &["p::a", "p::b"],
        &[
            migrated("p::a", "case-nobhv"),
            migrated("p::b", "case-nochan"),
        ],
        &[no_behavior, no_channel],
    );
    assert!(
        problems
            .iter()
            .any(|p| p.record == "case-nobhv" && p.message.contains("no behavior")),
        "T033: a destination case must resolve to at least one behavior: {problems:#?}"
    );
    assert!(
        problems
            .iter()
            .any(|p| p.record == "case-nochan" && p.message.contains("no observable channel")),
        "T033: a destination case must declare at least one channel: {problems:#?}"
    );
}

#[test]
fn dangling_behavior_and_channel_identifiers_are_rejected() {
    let mut dangling = good_case("case-dangling");
    dangling.behaviors = vec!["bhv-ghost".to_string()];
    dangling.channels = vec!["chan-ghost".to_string()];

    let problems = run(&["p::a"], &[migrated("p::a", "case-dangling")], &[dangling]);
    assert!(
        problems
            .iter()
            .any(|p| p.message.contains("unknown behavior") && p.message.contains("bhv-ghost")),
        "{problems:#?}"
    );
    assert!(
        problems
            .iter()
            .any(|p| p.message.contains("unknown channel") && p.message.contains("chan-ghost")),
        "{problems:#?}"
    );
}

#[test]
fn disposition_arity_rules_are_enforced() {
    let mut no_cases = migrated("p::a", "case-a");
    no_cases.case_ids.clear();

    let mut residual_with_cases = migrated("p::b", "case-a");
    residual_with_cases.disposition = Disposition::Residual;

    let mut retired_without_rationale = migrated("p::c", "case-a");
    retired_without_rationale.disposition = Disposition::Retired;
    retired_without_rationale.case_ids.clear();

    let problems = run(
        &["p::a", "p::b", "p::c"],
        &[no_cases, residual_with_cases, retired_without_rationale],
        &[good_case("case-a")],
    );
    assert!(
        problems
            .iter()
            .any(|p| p.record == "p::a" && p.message.contains("requires a non-empty `caseIds`"))
    );
    assert!(
        problems
            .iter()
            .any(|p| p.record == "p::b" && p.message.contains("must not name any case"))
    );
    assert!(
        problems
            .iter()
            .any(|p| p.record == "p::c" && p.message.contains("requires a `rationale`"))
    );
}

#[test]
fn the_real_registry_has_no_mapping_or_residual_violations() {
    let registry =
        Registry::load(&default_registry_dir()).expect("the real registry loads cleanly");
    let violations: Vec<_> = check_registry_mapping(&registry)
        .into_iter()
        .chain(check_residuals(&registry))
        .collect();
    assert!(
        violations.is_empty(),
        "conformance/registry + conformance/migration must be orphan-free:\n{violations:#?}"
    );
}

#[test]
fn every_baseline_unit_has_exactly_one_disposition_in_the_real_mapping() {
    let registry =
        Registry::load(&default_registry_dir()).expect("the real registry loads cleanly");
    let baseline = registry
        .baseline
        .as_ref()
        .expect("the committed baseline is present");

    let mut per_unit: BTreeMap<&str, usize> = BTreeMap::new();
    for record in &registry.mapping {
        *per_unit.entry(record.unit.as_str()).or_default() += 1;
    }
    assert_eq!(
        per_unit.len(),
        baseline.records.len(),
        "every baseline unit must be mapped exactly once"
    );
    assert!(
        per_unit.values().all(|n| *n == 1),
        "a unit is mapped more than once: {:?}",
        per_unit
            .iter()
            .filter(|(_, n)| **n != 1)
            .collect::<Vec<_>>()
    );
}
