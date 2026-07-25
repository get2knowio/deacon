//! T079 (US7, research D5): a legacy carrier's case count may only **decrease** once the
//! migration begins.
//!
//! Two comparison paths coexist during this migration — the legacy normalizers and the
//! declarative channel path. Constitution VIII forbids exactly that, and the exception is
//! granted only because FR-033 requires running BOTH paths over the full baseline to prove
//! the replacement is never more permissive. An exception with no bound is just a
//! violation with a rationale, so this test is the bound: the legacy path may shrink as
//! units migrate, and may never grow.
//!
//! Without it, the cheapest way to "finish" the migration would be to keep adding legacy
//! pointer cases — the transitional window would never close, and the second
//! implementation would become permanent by accretion rather than by decision.
//!
//! Hermetic: reads the real registry. No Docker, no network.

use std::collections::BTreeMap;

use deacon_conformance::default_registry_dir;
use deacon_conformance::load::Registry;
use deacon_conformance::model::CaseKind;

/// The legacy (binary-backed) case count per binary at the migration's branch point
/// (`98c26a5`), when `cases.json` held 31 records of which 25 were legacy pointers
/// (research §1g).
///
/// This is a frozen benchmark, like the baseline's assertions and
/// `PRE_MIGRATION_BEHAVIORS`. Raising an entry to accommodate a newly added legacy case
/// is the move the ratchet exists to prevent, and it is a conspicuous diff in this file.
const LEGACY_CASES_AT_BRANCH_POINT: &[(&str, usize)] = &[
    ("integration_auto_forward", 1),
    ("integration_host_ca_runtime", 1),
    ("integration_override_secrets", 1),
    ("integration_profiles", 1),
    ("integration_trust", 1),
    ("parity_build", 1),
    ("parity_corpus_errors", 9),
    ("parity_corpus_merged", 2),
    ("parity_corpus_tier1", 1),
    ("parity_exec", 1),
    ("parity_observable_state", 2),
    ("parity_read_configuration", 1),
    ("parity_state_diff", 1),
    ("parity_up_exec", 2),
];

/// Whether a test binary's source exists anywhere under `crates/*/tests/`.
///
/// Mirrors V1's resolution rather than assuming `crates/deacon/tests/`: legacy pointer
/// cases legitimately name binaries in other crates (`integration_override_secrets`
/// lives in `crates/core/tests/`), and a narrower rule here would report a deletion that
/// never happened.
fn binary_exists(binary: &str) -> bool {
    let crates_dir = deacon_conformance::workspace_root().join("crates");
    let Ok(entries) = std::fs::read_dir(&crates_dir) else {
        return false;
    };
    let file_name = format!("{binary}.rs");
    entries
        .flatten()
        .any(|entry| entry.path().join("tests").join(&file_name).is_file())
}

/// The legacy case count per binary in the registry as it stands.
fn current_legacy_counts() -> BTreeMap<String, usize> {
    let registry =
        Registry::load(&default_registry_dir()).expect("the real registry loads cleanly");
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    for case in &registry.cases {
        if !matches!(case.classify(), Ok(CaseKind::Legacy)) {
            continue;
        }
        if let Some(exe) = &case.executable {
            *out.entry(exe.binary.clone()).or_default() += 1;
        }
    }
    out
}

#[test]
fn no_legacy_carrier_gained_a_case() {
    let frozen: BTreeMap<&str, usize> = LEGACY_CASES_AT_BRANCH_POINT.iter().copied().collect();
    let current = current_legacy_counts();

    let mut grew: Vec<String> = Vec::new();
    for (binary, count) in &current {
        let before = frozen.get(binary.as_str()).copied().unwrap_or(0);
        if *count > before {
            grew.push(format!("{binary}: {before} -> {count}"));
        }
    }
    assert!(
        grew.is_empty(),
        "a legacy carrier's case count may only DECREASE once the migration begins \
         (research D5) — the transitional dual-path window must close, not widen: {grew:?}"
    );
}

#[test]
fn no_new_legacy_carrier_appeared() {
    let frozen: BTreeMap<&str, usize> = LEGACY_CASES_AT_BRANCH_POINT.iter().copied().collect();
    let current = current_legacy_counts();

    let newcomers: Vec<&String> = current
        .keys()
        .filter(|binary| !frozen.contains_key(binary.as_str()))
        .collect();
    assert!(
        newcomers.is_empty(),
        "a binary that carried no legacy case at the branch point may not acquire one — \
         new coverage belongs on the declarative path: {newcomers:?}"
    );
}

#[test]
fn the_ratchet_has_actually_moved() {
    // The bound is only meaningful if the window is genuinely closing. `parity_up_exec`
    // went 2 -> 1 in US3 (T054 merged two pointer cases that shared ONE reported
    // outcome), so this is a real decrease, not a no-op.
    let frozen_total: usize = LEGACY_CASES_AT_BRANCH_POINT.iter().map(|(_, n)| n).sum();
    let current_total: usize = current_legacy_counts().values().sum();
    assert!(
        current_total < frozen_total,
        "the legacy path should be shrinking: {current_total} now vs {frozen_total} at \
         the branch point"
    );
}

#[test]
fn a_deleted_carrier_drops_to_zero_and_stays_there() {
    // Deleting a carrier is the ratchet's terminal state. Whatever is gone must be gone
    // from `cases.json` too — a legacy case pointing at a deleted binary is a dangling
    // reference (V1), and this test states the direction so a half-deletion is caught
    // here with a clearer message than "missing executable".
    let current = current_legacy_counts();
    let mut dangling: Vec<String> = Vec::new();
    for binary in current.keys() {
        if !binary_exists(binary) {
            dangling.push(binary.clone());
        }
    }
    assert!(
        dangling.is_empty(),
        "these binaries were deleted but still carry legacy cases — a deletion must \
         remove the pointer cases in the same change (FR-031): {dangling:?}"
    );
}
