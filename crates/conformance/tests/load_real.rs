//! Hermetic guard that the real seed registry (`conformance/registry/`, T004) and
//! the valid test fixture (`fixtures/conformance/valid/`, T005) both parse cleanly
//! through the loader.
//!
//! This is the Phase-2 checkpoint ("`validate --registry fixtures/conformance/valid`
//! can load"). Full violation-class validation (V1–V10) and the PR-gate
//! `registry_valid` test land in User Story 1 (T013/T015); this only asserts the
//! shapes load without schema errors. No Docker, no network — runs on every lane,
//! including the Windows `dev-fast` lane.

use deacon_conformance::default_registry_dir;
use deacon_conformance::load::Registry;

#[test]
fn seed_registry_loads_cleanly() {
    let dir = default_registry_dir();
    let registry = Registry::load(&dir)
        .unwrap_or_else(|e| panic!("seed registry {} must load: {e}", dir.display()));
    // The four closed collections are seeded exactly (T004).
    assert_eq!(registry.revisions.len(), 4, "four pinned source revisions");
    assert_eq!(registry.dimensions.len(), 4, "os/arch/runtime/oracle");
    assert_eq!(registry.channels.len(), 6, "six observable channels");
    assert_eq!(registry.profiles.len(), 1, "one active profile");
    assert!(
        registry.profiles[0].active,
        "the seeded profile is the active one"
    );
    // Behaviors/sources/cases/waivers/extensions are seeded from the documented
    // divergence inventory in US4 (T027–T030); the closed collections above are
    // fixed, but these grow as the registry is populated.
    assert!(
        !registry.behaviors.is_empty(),
        "US4 seeds behaviors from the documented divergences"
    );
    assert!(!registry.sources.is_empty(), "US4 seeds source provenance");
    assert!(!registry.cases.is_empty(), "US4 seeds executable cases");
    assert!(!registry.waivers.is_empty(), "US4 migrates parity waivers");
    assert!(
        !registry.extensions.is_empty(),
        "US4 seeds deacon extensions"
    );
}

#[test]
fn valid_fixture_loads_and_exercises_every_record_type() {
    let dir = deacon_conformance::workspace_root().join("fixtures/conformance/valid");
    let registry = Registry::load(&dir)
        .unwrap_or_else(|e| panic!("valid fixture {} must load: {e}", dir.display()));

    // Every record type is present (T005 requirement).
    assert!(!registry.revisions.is_empty());
    assert!(!registry.dimensions.is_empty());
    assert!(!registry.channels.is_empty());
    assert_eq!(registry.profiles.len(), 1);
    assert!(!registry.cases.is_empty());
    assert!(!registry.gaps.is_empty());
    assert!(!registry.extensions.is_empty());
    assert!(!registry.waivers.is_empty());

    // All four source inventories appear.
    use deacon_conformance::model::Inventory;
    for inv in [
        Inventory::Schema,
        Inventory::Spec,
        Inventory::Cli,
        Inventory::Observed,
    ] {
        assert!(
            registry.sources.iter().any(|s| s.inventory == inv),
            "expected at least one {inv:?} source unit"
        );
    }

    // At least two behaviors, including one out-of-profile (applicability pins a
    // dimension to a value the active docker profile does not carry).
    assert!(registry.behaviors.len() >= 2);
    assert!(
        registry
            .behaviors
            .iter()
            .any(|b| !b.applicability.is_empty()),
        "expected at least one out-of-profile behavior with a non-empty applicability"
    );
}
