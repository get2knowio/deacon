//! Acceptance tests for three-axis disposition recording (T025; US3, FR-012,
//! FR-014). Companion doc: `conformance/RULES.md`.
//!
//! Drives the shared library entry point [`validate_path`] against the checked-in
//! disposition fixtures under `fixtures/conformance/disposition/`:
//!
//! - **(a)** the `accepted` fixture — the four meaningful accepted axis combinations
//!   (US3 scenarios 1–3 plus nonconformant/divergent/intentional-divergence) —
//!   validates with ZERO violations, and each combination is actually present;
//! - **(b)** one `r1`…`r8` fixture per contradiction rule fails with EXACTLY one V8
//!   violation whose message names that specific rule and the offending behavior;
//! - **(c)** the `missing-axis` fixture (a behavior omitting `decision`) is rejected
//!   as a `SCHEMA` failure, never as a V-class contradiction (FR-012: fewer than all
//!   three axes → incomplete record).
//!
//! Hermetic (no Docker, no network); resolves paths via `workspace_root()` so it is
//! CWD-independent and selected by `dev-fast` automatically.

use std::path::PathBuf;

use deacon_conformance::load::Registry;
use deacon_conformance::model::{Decision, ReferenceStatus, SpecStatus};
use deacon_conformance::validate::{Violation, validate_path};
use deacon_conformance::workspace_root;

/// A fixed injected "today" so waiver-expiry (V6) is deterministic. The disposition
/// fixtures inherit the valid fixture's waiver, which expires 2027-01-19 —
/// comfortably in the future of this date, so V6 never fires here.
const TODAY: &str = "2026-07-19";

fn fixture(name: &str) -> PathBuf {
    workspace_root()
        .join("fixtures/conformance/disposition")
        .join(name)
}

fn validate(name: &str) -> Vec<Violation> {
    let root = fixture(name);
    validate_path(&root, TODAY, &workspace_root())
        .unwrap_or_else(|e| panic!("fixture {name:?} root is unreadable: {e}"))
}

fn load(name: &str) -> Registry {
    Registry::load(&fixture(name)).unwrap_or_else(|e| panic!("fixture {name:?} must load: {e}"))
}

// -- (a) accepted combinations validate with zero violations ------------------

#[test]
fn accepted_combos_validate_with_zero_violations() {
    let violations = validate("accepted");
    assert!(
        violations.is_empty(),
        "the accepted-combos fixture must have no violations, got: {violations:#?}"
    );
}

#[test]
fn accepted_fixture_actually_exercises_each_named_combo() {
    let registry = load("accepted");
    let has = |spec: SpecStatus, reference: ReferenceStatus, decision: Decision| {
        registry
            .behaviors
            .iter()
            .any(|b| b.spec == spec && b.reference == reference && b.decision == decision)
    };

    // US3 scenario 1: deacon follows the spec, the reference deviates.
    assert!(
        has(
            SpecStatus::Conformant,
            ReferenceStatus::Divergent,
            Decision::FollowSpec
        ),
        "expected a conformant/divergent/follow-spec behavior"
    );
    // US3 scenario 2: spec silent, deacon matches the reference.
    assert!(
        has(
            SpecStatus::Unspecified,
            ReferenceStatus::Aligned,
            Decision::AlignWithReference
        ),
        "expected an unspecified/aligned/align-with-reference behavior"
    );
    // US3 scenario 3: a deacon-only capability.
    assert!(
        has(
            SpecStatus::NotApplicable,
            ReferenceStatus::NotApplicable,
            Decision::DeaconExtension
        ),
        "expected a not-applicable/not-applicable/deacon-extension behavior"
    );
    // Deliberate, characterized divergence.
    assert!(
        has(
            SpecStatus::Nonconformant,
            ReferenceStatus::Divergent,
            Decision::IntentionalDivergence
        ),
        "expected a nonconformant/divergent/intentional-divergence behavior"
    );
}

// -- (b) one failing fixture per contradiction rule R1–R8 ---------------------

#[test]
fn each_contradiction_rule_fails_v8_naming_the_rule() {
    // (fixture dir, rule identifier, expected offending behavior ID)
    let cases = [
        ("r1", "R1", "bhv-readconfig-basic-parse"),
        ("r2", "R2", "bhv-secrets-dotenv-superset"),
        ("r3", "R3", "bhv-readconfig-malformed-jsonc-rejected"),
        ("r4", "R4", "bhv-readconfig-basic-parse"),
        ("r5", "R5", "bhv-readconfig-basic-parse"),
        ("r6", "R6", "bhv-readconfig-malformed-jsonc-rejected"),
        ("r7", "R7", "bhv-exec-podman-keep-id"),
        ("r8", "R8", "bhv-readconfig-remote-user-probe"),
    ];

    for (name, rule, record) in cases {
        let violations = validate(name);
        // Each fixture isolates exactly ONE contradiction: a single V8 violation.
        assert_eq!(
            violations.len(),
            1,
            "fixture {name:?} must report exactly one violation, got: {violations:#?}"
        );
        let v = &violations[0];
        assert_eq!(
            v.code, "V8",
            "fixture {name:?} must fail as V8, got {:?}",
            v.code
        );
        assert_eq!(
            v.record, record,
            "fixture {name:?} must name behavior {record:?}, got {:?}",
            v.record
        );
        // The message must NAME the specific rule (e.g. "R3"), not just say
        // "contradiction" — so contributors can predict validation from RULES.md.
        assert!(
            v.message.contains(rule),
            "fixture {name:?} V8 message must name rule {rule:?}, got {:?}",
            v.message
        );
    }
}

// -- (c) a record missing any axis is a SCHEMA failure (FR-012) ---------------

#[test]
fn record_missing_an_axis_is_a_schema_failure() {
    let violations = validate("missing-axis");
    assert_eq!(
        violations.len(),
        1,
        "the missing-axis fixture reports a single SCHEMA violation, got: {violations:#?}"
    );
    let v = &violations[0];
    // A dropped axis is caught at load (all three fields are mandatory), NOT as a
    // V8 contradiction: an incomplete record cannot even be modeled (FR-012).
    assert_eq!(
        v.code, "SCHEMA",
        "a missing axis must be SCHEMA, not a V-class rule, got {:?}",
        v.code
    );
    // The failure names the offending file and the dropped field.
    let record = v.record.replace('\\', "/"); // separator-agnostic for Windows.
    assert!(
        record.contains("behaviors/read-configuration.json"),
        "SCHEMA record must name the offending file, got {record:?}"
    );
    assert!(
        v.message.contains("decision"),
        "SCHEMA message must name the missing axis `decision`, got {:?}",
        v.message
    );
}
