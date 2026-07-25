//! Archival integrity of the frozen baseline (originally `baseline_enumeration.rs`,
//! T015; reshaped by T099).
//!
//! `conformance/migration/baseline.json` is retained as the evidence for the "no coverage
//! was lost" claim (FR-053). What it records is a **pre-migration** world, so once a
//! superseded carrier is deleted the enumeration can no longer reproduce it — the
//! discovery-driven legs that made that comparison, and the V25 gate they served, are
//! retired with it.
//!
//! What is still true, and still worth failing on, is the artifact's own integrity: the
//! frozen category totals, unique ids, a real assertion on every unit, and channels that
//! still resolve. These read the COMMITTED record rather than regenerating, so they hold
//! for as long as the record is retained.
//!
//! The off-by-one this file originally existed to guard (research D1: **24** Tier-1
//! cases, not the 25 a directory listing reports) is preserved where it now belongs — in
//! the frozen record itself, asserted below.
//!
//! Hermetic: no Docker, no network, no oracle.

use deacon_conformance::baseline::{BaselineFile, UnitCategory};
use deacon_conformance::default_baseline_file;

/// Frozen category totals. `live-per-case`, `internal-consistency` and
/// `external-corpus-entry` are the research §1 figures; `hermetic-guard` is 16 at the
/// branch point plus the 7 fault-injection units User Story 4 deliberately added.
const LIVE_PER_CASE: usize = 91;
const HERMETIC_GUARD: usize = 23;
const HERMETIC_GUARD_AT_FREEZE: usize = 16;
const US4_ADDED_GUARDS: usize = 7;
const INTERNAL_CONSISTENCY: usize = 4;
const EXTERNAL_CORPUS: usize = 33;
/// The Tier-1 corpus case count — the number research D1 corrected from 25.
const TIER1_CASES: usize = 24;
/// The error-corpus case count.
const ERROR_CASES: usize = 9;

fn committed() -> BaselineFile {
    let path = default_baseline_file();
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "the frozen baseline {} is RETAINED as evidence (FR-053) and must remain \
             readable: {e}",
            path.display()
        )
    });
    serde_json::from_str(&raw).expect("the committed baseline parses")
}

fn units_of(baseline: &BaselineFile, program: &str) -> Vec<String> {
    baseline
        .records
        .iter()
        .filter(|u| u.program == program)
        .map(|u| u.id.trim_start_matches(&format!("{program}::")).to_string())
        .collect()
}

#[test]
fn the_frozen_record_still_shows_24_tier1_cases_not_25() {
    // Research D1, preserved: a directory listing of the corpus reported 25 because it
    // counted the sibling `errors/` directory; the production discovery rule selected 24.
    // The frozen record carries the corrected figure, which is the whole reason the
    // baseline was enumerated rather than recalled.
    let baseline = committed();
    for program in ["parity_corpus_tier1", "parity_corpus_merged"] {
        assert_eq!(
            units_of(&baseline, program).len(),
            TIER1_CASES,
            "{program} must carry one unit per discovered Tier-1 case"
        );
    }
    assert_eq!(
        units_of(&baseline, "parity_corpus_errors").len(),
        ERROR_CASES
    );
    assert!(
        !units_of(&baseline, "parity_corpus_tier1").contains(&"errors".to_string()),
        "the sibling errors corpus is not a Tier-1 case — that WAS the off-by-one"
    );
}

#[test]
fn frozen_category_totals_hold() {
    let baseline = committed();

    assert_eq!(baseline.count(UnitCategory::LivePerCase), LIVE_PER_CASE);
    assert_eq!(baseline.count(UnitCategory::HermeticGuard), HERMETIC_GUARD);
    assert_eq!(
        baseline.count(UnitCategory::InternalConsistency),
        INTERNAL_CONSISTENCY
    );
    assert_eq!(
        baseline.count(UnitCategory::ExternalCorpusEntry),
        EXTERNAL_CORPUS
    );

    assert_eq!(
        HERMETIC_GUARD,
        HERMETIC_GUARD_AT_FREEZE + US4_ADDED_GUARDS,
        "the guard total must be the branch-point total plus exactly the US4 additions"
    );
    assert_eq!(
        LIVE_PER_CASE + HERMETIC_GUARD_AT_FREEZE + INTERNAL_CONSISTENCY,
        deacon_conformance::conservation::PRE_MIGRATION_EXECUTABLE_UNITS,
        "the pre-migration executable denominator stays 111 (research §1)"
    );
    assert_eq!(
        baseline.records.len(),
        LIVE_PER_CASE + HERMETIC_GUARD + INTERNAL_CONSISTENCY + EXTERNAL_CORPUS
    );
}

#[test]
fn guard_units_are_one_per_test_function() {
    let baseline = committed();
    for (program, expected) in [
        // 10 fault injections (a–j) at the branch point + the 7 US4 additions (k–q).
        ("parity_harness_faults", 17usize),
        ("parity_registry_check", 6),
        ("consistency_env_probe_flag", 2),
        ("consistency_remote_env_flags", 2),
    ] {
        assert_eq!(
            units_of(&baseline, program).len(),
            expected,
            "{program} contributes one unit per test function"
        );
    }
}

#[test]
fn every_unit_has_a_non_empty_assertion_and_a_unique_id() {
    let baseline = committed();
    let mut seen = std::collections::BTreeSet::new();
    for unit in &baseline.records {
        assert!(
            !unit.assertion.trim().is_empty(),
            "unit `{}` has no recorded assertion — it could not be proven conserved",
            unit.id
        );
        assert!(
            unit.id.contains("::"),
            "unit id `{}` must be `<program>::<case-or-fn>`",
            unit.id
        );
        assert!(
            seen.insert(unit.id.clone()),
            "duplicate unit id `{}`",
            unit.id
        );
    }
    let ids: Vec<&str> = baseline.records.iter().map(|u| u.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "the retained record must stay id-sorted");
}

#[test]
fn every_declared_channel_resolves_in_the_registry() {
    let baseline = committed();
    let registry =
        deacon_conformance::load::Registry::load(&deacon_conformance::default_registry_dir())
            .expect("the real registry loads");
    let known: std::collections::BTreeSet<&str> =
        registry.channels.iter().map(|c| c.id.as_str()).collect();

    for unit in &baseline.records {
        for channel in &unit.channels {
            assert!(
                known.contains(channel.as_str()),
                "unit `{}` names channel `{channel}`, which is not declared in \
                 channels.json — a retired carrier's channels must still resolve",
                unit.id
            );
        }
    }
}
