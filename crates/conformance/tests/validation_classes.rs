//! Acceptance tests for the violation-class engine (T013; SC-002, FR-019).
//!
//! Drives the shared library entry point [`validate_path`] against the checked-in
//! fixture registries under `fixtures/conformance/`:
//!
//! - the `valid` fixture passes with zero violations;
//! - each `invalid-v*` fixture fails naming EXACTLY its class and the offending
//!   record ID (no other violation class leaks in);
//! - `schema-error` reports the `SCHEMA` class with the file + `line:column`;
//! - the `multi-violation` fixture reports ALL violations in a single run.
//!
//! Hermetic (no Docker, no network); resolves paths via `workspace_root()` so it is
//! CWD-independent and selected by `dev-fast` automatically.

use std::path::PathBuf;

use deacon_conformance::validate::{Violation, validate_path};
use deacon_conformance::workspace_root;

/// A fixed injected "today" so waiver-expiry (V6) is deterministic. The valid
/// fixture's waiver expires 2027-01-19, comfortably in the future of this date, so
/// the valid registry has zero violations at this instant.
const TODAY: &str = "2026-07-19";

fn fixture(name: &str) -> PathBuf {
    workspace_root().join("fixtures/conformance").join(name)
}

fn validate(name: &str) -> Vec<Violation> {
    let root = fixture(name);
    validate_path(&root, TODAY, &workspace_root())
        .unwrap_or_else(|e| panic!("fixture {name:?} root is unreadable: {e}"))
}

#[test]
fn valid_fixture_has_zero_violations() {
    let violations = validate("valid");
    assert!(
        violations.is_empty(),
        "the valid fixture must have no violations, got: {violations:#?}"
    );
}

#[test]
fn each_invalid_fixture_fails_with_exactly_its_class_and_record() {
    // (fixture dir, expected violation code, expected offending record ID)
    let cases = [
        ("invalid-v1", "V1", "case-readconfig-surface"),
        ("invalid-v2", "V2", "chan-stdout"),
        ("invalid-v3", "V3", "case-orphan"),
        ("invalid-v4", "V4", "src-schema-image-required"),
        ("invalid-v5", "V5", "bhv-readconfig-uncovered"),
        ("invalid-v6", "V6", "wvr-readconfig-malformed-jsonc"),
        ("invalid-v7", "V7", "rev-oracle-0-87-0"),
        ("invalid-v8", "V8", "bhv-readconfig-basic-parse"),
        ("invalid-v9", "V9", "case-readconfig-surface"),
        ("invalid-v10", "V10", "case-exec-docker-conflict"),
    ];

    for (name, code, record) in cases {
        let violations = validate(name);
        assert!(
            !violations.is_empty(),
            "fixture {name:?} must report at least one violation"
        );
        // "exactly its class": every reported violation is the expected class.
        assert!(
            violations.iter().all(|v| v.code == code),
            "fixture {name:?} must report ONLY {code}, got: {violations:#?}"
        );
        // The offending record is named.
        assert!(
            violations.iter().any(|v| v.record == record),
            "fixture {name:?} must name record {record:?}, got: {violations:#?}"
        );
    }
}

#[test]
fn schema_error_fixture_reports_schema_class_with_location() {
    let violations = validate("schema-error");
    assert_eq!(
        violations.len(),
        1,
        "schema-error fixture reports a single SCHEMA violation, got: {violations:#?}"
    );
    let v = &violations[0];
    assert_eq!(v.code, "SCHEMA", "expected SCHEMA class, got {:?}", v.code);
    // The record carries the file path and a `line:column` location.
    let record = v.record.replace('\\', "/"); // separator-agnostic for Windows.
    assert!(
        record.contains("behaviors/read-configuration.json"),
        "SCHEMA record must name the offending file, got {record:?}"
    );
    assert!(
        record
            .rsplit('/')
            .next()
            .is_some_and(|leaf| leaf.contains(':')),
        "SCHEMA record must carry a line:column location, got {record:?}"
    );
    assert!(
        v.message.contains("totally-invalid-enum"),
        "SCHEMA message must explain the malformed value, got {:?}",
        v.message
    );
}

#[test]
fn multi_violation_fixture_reports_all_violations_in_one_run() {
    let violations = validate("multi-violation");
    // Both an orphan case (V3) and an expired waiver (V6) are reported together —
    // validation never stops at the first failure (FR-019).
    assert!(
        violations
            .iter()
            .any(|v| v.code == "V3" && v.record == "case-orphan"),
        "expected the V3 orphan case, got: {violations:#?}"
    );
    assert!(
        violations
            .iter()
            .any(|v| v.code == "V6" && v.record == "wvr-readconfig-malformed-jsonc"),
        "expected the V6 expired waiver, got: {violations:#?}"
    );
    // Sorted output: V3 (rank 3) precedes V6 (rank 6).
    let codes: Vec<&str> = violations.iter().map(|v| v.code.as_str()).collect();
    let v3 = codes.iter().position(|c| *c == "V3");
    let v6 = codes.iter().position(|c| *c == "V6");
    assert!(v3 < v6, "violations must be sorted by code: {codes:?}");
}
