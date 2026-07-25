//! T026 (US2, FR-010 / FR-012 / FR-047): **V22** — fixture correspondence is strictly
//! one-to-one, nothing is silently dropped, and no migrated fixture is left
//! unreferenced.
//!
//! Four distinct silent-loss modes, each with its own leg:
//!
//! - **merge**: two source fixtures land on one destination — one input disappears;
//! - **split**: one source lands on two destinations — the correspondence is ambiguous
//!   and a later deletion cannot know which one is authoritative;
//! - **drop**: a migrated unit consumed a fixture that no mapping accounts for;
//! - **orphan**: a fixture was moved into `conformance/fixtures/` but no case runs
//!   against it, so the move conserved nothing.
//!
//! Hermetic: pure functions over synthetic records plus a read of the real registry.

use std::collections::BTreeMap;

use deacon_conformance::mapping::{
    CaseFacts, Disposition, FixtureMapping, MigrationMapping, check_fixture_mappings,
};
use deacon_conformance::{default_registry_dir, load::Registry, workspace_root};

/// A synthetic `migrated` unit declaring the given `(from, to)` fixture
/// correspondences. What the unit ACTUALLY consumed is supplied separately by
/// [`declared`], exactly as the real check reads it from the frozen baseline.
fn unit(name: &str, mappings: &[(&str, &str)]) -> MigrationMapping {
    MigrationMapping {
        unit: name.to_string(),
        disposition: Disposition::Migrated,
        case_ids: vec!["case-x".to_string()],
        residual_id: None,
        rationale: None,
        fixture_mapping: mappings
            .iter()
            .map(|(from, to)| FixtureMapping {
                from: (*from).to_string(),
                to: (*to).to_string(),
            })
            .collect(),
    }
}

fn declared(entries: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
    entries
        .iter()
        .map(|(unit, fixtures)| {
            (
                (*unit).to_string(),
                fixtures.iter().map(|f| (*f).to_string()).collect(),
            )
        })
        .collect()
}

fn case_using(fixtures: &[&str]) -> CaseFacts {
    CaseFacts {
        id: "case-x".to_string(),
        behaviors: vec!["bhv-x".to_string()],
        channels: vec!["chan-exit-code".to_string()],
        fixtures: fixtures.iter().map(|f| (*f).to_string()).collect(),
        declarative: true,
        context: Vec::new(),
        oracle: "SpecExpectation".to_string(),
        input_shape: "read-configuration [probe]".to_string(),
    }
}

#[test]
fn two_sources_merged_into_one_destination_fails() {
    let mapping = vec![
        unit("p::a", &[("src/a", "conformance/fixtures/fx-shared")]),
        unit("p::b", &[("src/b", "conformance/fixtures/fx-shared")]),
    ];
    let problems = check_fixture_mappings(
        &mapping,
        &declared(&[("p::a", &["src/a"]), ("p::b", &["src/b"])]),
        &[case_using(&["fx-shared"])],
    );
    let hit = problems
        .iter()
        .find(|p| p.record == "conformance/fixtures/fx-shared")
        .unwrap_or_else(|| panic!("a merge must be reported: {problems:#?}"));
    assert_eq!(hit.code, "V22");
    assert!(hit.message.contains("fed by 2 sources"), "{}", hit.message);
}

#[test]
fn one_source_split_across_two_destinations_fails() {
    let mapping = vec![unit(
        "p::a",
        &[
            ("src/a", "conformance/fixtures/fx-one"),
            ("src/a", "conformance/fixtures/fx-two"),
        ],
    )];
    let problems = check_fixture_mappings(
        &mapping,
        &declared(&[("p::a", &["src/a"])]),
        &[case_using(&["fx-one", "fx-two"])],
    );
    let hit = problems
        .iter()
        .find(|p| p.record == "src/a")
        .unwrap_or_else(|| panic!("a split must be reported: {problems:#?}"));
    assert_eq!(hit.code, "V22");
    assert!(hit.message.contains("split across 2"), "{}", hit.message);
}

#[test]
fn a_dropped_fixture_fails_naming_the_unit_and_the_fixture() {
    // The unit consumed two fixtures but accounts for only one.
    let mapping = vec![unit("p::a", &[("src/a", "conformance/fixtures/fx-a")])];
    let problems = check_fixture_mappings(
        &mapping,
        &declared(&[("p::a", &["src/a", "src/b"])]),
        &[case_using(&["fx-a"])],
    );
    assert!(
        problems.iter().any(|p| p.code == "V22"
            && p.record == "p::a"
            && p.message.contains("src/b")
            && p.message.contains("silently dropped")),
        "a dropped fixture must be reported: {problems:#?}"
    );
}

#[test]
fn a_migrated_fixture_referenced_by_no_case_is_an_orphan() {
    let mapping = vec![unit("p::a", &[("src/a", "conformance/fixtures/fx-unused")])];
    // No case references `fx-unused`.
    let problems = check_fixture_mappings(
        &mapping,
        &declared(&[("p::a", &["src/a"])]),
        &[case_using(&["fx-something-else"])],
    );
    assert!(
        problems.iter().any(|p| p.code == "V22"
            && p.record == "conformance/fixtures/fx-unused"
            && p.message.contains("referenced by no case")),
        "an unreferenced migrated fixture must be reported: {problems:#?}"
    );
}

#[test]
fn a_from_that_is_not_a_baseline_fixture_of_the_unit_fails() {
    let mapping = vec![unit(
        "p::a",
        &[("src/elsewhere", "conformance/fixtures/fx-a")],
    )];
    let problems = check_fixture_mappings(
        &mapping,
        &declared(&[("p::a", &["src/a"])]),
        &[case_using(&["fx-a"])],
    );
    assert!(
        problems
            .iter()
            .any(|p| p.code == "V22" && p.message.contains("not one of the unit's baseline")),
        "{problems:#?}"
    );
}

#[test]
fn the_same_correspondence_declared_by_two_units_is_not_a_merge() {
    // The tier-1 and merged-mode units legitimately share ONE fixture directory: the
    // SAME (from, to) pair declared twice is still a one-to-one correspondence.
    let mapping = vec![
        unit("p::tier1", &[("src/a", "conformance/fixtures/fx-a")]),
        unit("p::merged", &[("src/a", "conformance/fixtures/fx-a")]),
    ];
    let problems = check_fixture_mappings(
        &mapping,
        &declared(&[("p::tier1", &["src/a"]), ("p::merged", &["src/a"])]),
        &[case_using(&["fx-a"])],
    );
    assert!(
        problems.is_empty(),
        "sharing one fixture between two modes of the same workspace is correct: {problems:#?}"
    );
}

#[test]
fn the_real_registry_has_one_to_one_fixture_correspondence() {
    let registry =
        Registry::load(&default_registry_dir()).expect("the real registry loads cleanly");
    let baseline = registry
        .baseline
        .as_ref()
        .expect("the committed baseline is present");
    let unit_fixtures: BTreeMap<String, Vec<String>> = baseline
        .records
        .iter()
        .map(|u| (u.id.clone(), u.fixtures.clone()))
        .collect();
    let cases: Vec<CaseFacts> = registry
        .cases
        .iter()
        .map(|c| CaseFacts {
            id: c.id.clone(),
            behaviors: c.behaviors.clone(),
            channels: c.expected.iter().map(|e| e.channel.clone()).collect(),
            fixtures: c
                .operations
                .iter()
                .flat_map(|op| op.fixtures.iter().cloned())
                .collect(),
            declarative: matches!(
                c.classify(),
                Ok(deacon_conformance::model::CaseKind::Declarative)
            ),
            context: Vec::new(),
            oracle: format!("{:?}", c.oracle_type),
            input_shape: c.id.clone(),
        })
        .collect();

    let problems = check_fixture_mappings(&registry.mapping, &unit_fixtures, &cases);
    assert!(
        problems.is_empty(),
        "the real fixture correspondence must be one-to-one:\n{problems:#?}"
    );
}

#[test]
fn every_migrated_fixture_destination_exists_on_disk() {
    let registry =
        Registry::load(&default_registry_dir()).expect("the real registry loads cleanly");
    let root = workspace_root();
    let mut missing: Vec<String> = Vec::new();
    for record in &registry.mapping {
        for fm in &record.fixture_mapping {
            if fm.to.starts_with("inline:") {
                continue;
            }
            if !root.join(&fm.to).is_dir() {
                missing.push(fm.to.clone());
            }
        }
    }
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "fixtureMapping destinations must exist: {missing:?}"
    );
}
