//! Seeding-completeness acceptance test (T033; SC-001, FR-026, FR-028).
//!
//! Hermetic proof that the day-one seed inventory (research Decision 6) is fully
//! migrated into the authoritative registry AND that the legacy duplicate locations
//! are gone. It hard-codes the legacy divergence inventory — the one tier1 parity
//! waiver, the nine error-corpus cases, and the shipped-feature extensions — and
//! asserts each maps to EXACTLY ONE registry record whose disposition axes
//! (spec / reference / decision) are all present via its linked behavior(s). It
//! then asserts the migrated-from files no longer exist, so the single-source-of-
//! truth invariant cannot silently regress into the old two-file duplication.
//!
//! No Docker, no network, light filesystem — lands in `dev-fast`/CI automatically
//! like `registry_valid`.

use deacon_conformance::load::Registry;
use deacon_conformance::model::{BehaviorUnit, Decision, Waiver};
use deacon_conformance::{default_registry_dir, workspace_root};

/// The nine error-corpus cases (research Decision 6 item 2). Each was migrated from
/// `fixtures/parity-corpus/errors/<case>/expect.json` into a corpus-case-scoped
/// `wvr-<case>` record linking a `bhv-readconfig-*` behavior with all three axes.
const ERROR_CASES: &[&str] = &[
    "bad-config-path",
    "duplicate-keys",
    "extends-cycle",
    "extends-missing",
    "malformed-json",
    "missing-config",
    "unknown-field-preserved",
    "wrong-type-features",
    "wrong-type-forwardports",
];

/// The one tier1 parity waiver (research Decision 6 item 1), migrated from
/// `fixtures/parity-corpus/waivers/extends-child-merged.json`.
const TIER1_WAIVER: &str = "wvr-extends-child-merged";

/// The shipped-feature Deacon extensions (research Decision 6 items 3 & 5). Each is
/// an `ext-` record whose linked behaviors carry decision `deacon-extension`, so an
/// intentional capability is never misreported as a parity divergence.
const EXTENSIONS: &[&str] = &[
    "ext-auto-forward-ports",
    "ext-extends-resolution",
    "ext-host-ca-injection",
    "ext-secrets-file-env-format",
    "ext-user-profiles",
    "ext-workspace-trust-gate",
];

fn load_registry() -> Registry {
    let dir = default_registry_dir();
    Registry::load(&dir).unwrap_or_else(|e| panic!("the real registry at {dir:?} must load: {e}"))
}

/// Return the single waiver with `id`, panicking if it is absent or duplicated
/// (SC-001: each documented divergence maps to EXACTLY ONE registry record).
fn unique_waiver<'a>(registry: &'a Registry, id: &str) -> &'a Waiver {
    let matches: Vec<&Waiver> = registry.waivers.iter().filter(|w| w.id == id).collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one waiver `{id}`, found {}",
        matches.len()
    );
    matches[0]
}

/// Assert every behavior id links to exactly one behavior record whose three
/// disposition axes are present (they are mandatory fields, so a resolved record
/// structurally carries all three — this confirms the linkage resolves, FR-012).
fn assert_behaviors_have_all_axes(registry: &Registry, behaviors: &[String], context: &str) {
    assert!(
        !behaviors.is_empty(),
        "{context} must link at least one behavior (three-axis disposition)"
    );
    for bid in behaviors {
        let found: Vec<&BehaviorUnit> =
            registry.behaviors.iter().filter(|b| &b.id == bid).collect();
        assert_eq!(
            found.len(),
            1,
            "{context} links behavior `{bid}`, expected exactly one such record, found {}",
            found.len()
        );
        // spec / reference / decision are non-optional on BehaviorUnit, so a
        // successfully-loaded record has all three axes; reference them so the
        // requirement is explicit rather than implicit.
        let b = found[0];
        let _axes = (b.spec, b.reference, b.decision);
    }
}

#[test]
fn error_corpus_waivers_are_fully_migrated_with_three_axes() {
    let registry = load_registry();
    for case in ERROR_CASES {
        let id = format!("wvr-{case}");
        let waiver = unique_waiver(&registry, &id);
        assert_behaviors_have_all_axes(&registry, &waiver.behaviors, &id);
    }
}

#[test]
fn tier1_parity_waiver_is_migrated_with_three_axes() {
    let registry = load_registry();
    let waiver = unique_waiver(&registry, TIER1_WAIVER);
    assert_behaviors_have_all_axes(&registry, &waiver.behaviors, TIER1_WAIVER);
}

#[test]
fn shipped_feature_extensions_are_seeded_as_deacon_extensions() {
    let registry = load_registry();
    for ext_id in EXTENSIONS {
        let matches: Vec<_> = registry
            .extensions
            .iter()
            .filter(|e| &e.id == ext_id)
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "expected exactly one extension `{ext_id}`, found {}",
            matches.len()
        );
        let ext = matches[0];
        assert_behaviors_have_all_axes(&registry, &ext.behaviors, ext_id);
        // An extension's behaviors must all be recorded as deacon extensions, so an
        // intentional capability is never counted as a divergence (FR-012, R2).
        for bid in &ext.behaviors {
            let behavior = registry
                .behaviors
                .iter()
                .find(|b| &b.id == bid)
                .unwrap_or_else(|| panic!("extension `{ext_id}` links unknown behavior `{bid}`"));
            assert_eq!(
                behavior.decision,
                Decision::DeaconExtension,
                "behavior `{bid}` linked from extension `{ext_id}` must have decision \
                 deacon-extension"
            );
        }
    }
}

#[test]
fn legacy_waiver_locations_no_longer_exist() {
    let root = workspace_root();

    // The parity-corpus waivers/ directory was migrated wholesale into the registry.
    let legacy_waivers = root.join("fixtures/parity-corpus/waivers");
    assert!(
        !legacy_waivers.exists(),
        "legacy waiver directory {} must no longer exist (migrated into \
         conformance/registry/waivers/)",
        legacy_waivers.display()
    );

    // No error case may carry a per-case expect.json waiver file anymore.
    let errors_root = root.join("fixtures/parity-corpus/errors");
    assert!(
        errors_root.is_dir(),
        "the errors corpus directory must still exist: {}",
        errors_root.display()
    );
    let rd =
        std::fs::read_dir(&errors_root).unwrap_or_else(|e| panic!("read {errors_root:?}: {e}"));
    for entry in rd.filter_map(Result::ok) {
        let case_dir = entry.path();
        if case_dir.is_dir() {
            assert!(
                !case_dir.join("expect.json").is_file(),
                "legacy expect.json must be removed from error case {} (migrated to \
                 conformance/registry/waivers/)",
                case_dir.display()
            );
        }
    }
}
