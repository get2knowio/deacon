//! PR-gate test (T015; research Decision 8): the REAL authoritative registry under
//! `conformance/registry/` MUST validate cleanly on every PR.
//!
//! This is the hermetic guard that keeps the registry from silently rotting. It runs
//! the full V1–V10 engine (via the shared [`validate_path`] entry) against the seed
//! skeleton (revisions/dimensions/channels/profiles today; behaviors/sources/cases
//! arrive in later phases and must keep this green). No Docker, no network, light
//! filesystem — so it lands in `dev-fast`/CI automatically with no nextest group
//! override (verify: `cargo nextest list -E 'binary(=registry_valid)'`).

use deacon_conformance::load::Registry;
use deacon_conformance::validate::validate_path;
use deacon_conformance::{default_registry_dir, workspace_root};

/// A fixed injected "today" so the gate never depends on the wall clock. The seed
/// registry has no waivers, so V6 cannot fire regardless — but pinning the date
/// keeps the test deterministic as waivers are added.
const TODAY: &str = "2026-07-19";

/// The number of case records the registry held when `cases.json` was split into
/// `cases/<area>.json` (024 T007). The split is a mechanical move whose one real risk
/// is silently dropping records — a per-area file that is never read, or a record that
/// falls out during a regroup, would just make the denominator smaller, and a smaller
/// denominator is invisible in every downstream count.
///
/// This constant is therefore a deliberate-edit gate, NOT a floor: raising it is how a
/// PR that genuinely adds cases records that it did so, and lowering it is how a PR that
/// genuinely retires cases records that it did so. An unexplained change to this number
/// is exactly the event the guard exists to surface.
const MIGRATED_CASE_COUNT: usize = 88;

#[test]
fn real_registry_is_structurally_valid() {
    let registry = default_registry_dir();
    let violations = validate_path(&registry, TODAY, &workspace_root()).unwrap_or_else(|e| {
        panic!(
            "the real registry at {} is unreadable: {e}",
            registry.display()
        )
    });
    assert!(
        violations.is_empty(),
        "conformance/registry/ must validate cleanly (V1–V10 + SCHEMA); violations:\n{violations:#?}"
    );
}

/// 024 T009: the per-area case split (T007) preserves every record, and the loader
/// (T008) still presents them in one deterministic, id-sorted sequence regardless of
/// how the areas are named or split.
#[test]
fn every_migrated_case_survives_the_per_area_split() {
    let registry_dir = default_registry_dir();
    let registry = Registry::load(&registry_dir).unwrap_or_else(|e| {
        panic!(
            "the real registry at {} is unreadable: {e}",
            registry_dir.display()
        )
    });

    let ids: Vec<&str> = registry.cases.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        ids.len(),
        MIGRATED_CASE_COUNT,
        "conformance/registry/cases/ must load exactly {MIGRATED_CASE_COUNT} case records; \
         loaded {} — if this change is intentional, update MIGRATED_CASE_COUNT in the same \
         commit and say why. Loaded ids: {ids:#?}",
        ids.len()
    );

    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(
        ids, sorted,
        "the concatenated case set must be id-sorted across per-area files, so aggregate \
         ordering is a property of the data and not of how the areas happen to be split"
    );

    sorted.dedup();
    assert_eq!(
        sorted.len(),
        ids.len(),
        "two case records claim the same id across per-area files"
    );
}
