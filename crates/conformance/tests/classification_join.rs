//! Acceptance tests for the classification join classes V11–V14 (T023, spec US2,
//! contracts/classification-schema.md).
//!
//! Each test builds a SMALL, self-consistent fixture — a temp schemas directory
//! (manifest + one tiny schema) whose committed inventory is generated in-place via
//! [`generate_inventory`] + [`write_inventory`], plus a temp registry carrying only the
//! records the join needs (a `schema`-kind revision, optional behaviors, and the
//! classifications under test). It then drives the library entry point
//! [`check_inventory`] and asserts that exactly the intended class fires, naming the
//! offending ID. A fully-classified fixture passes clean; the scaffold sentinel
//! `"UNREVIEWED"` is rejected at LOAD time as a SCHEMA-class failure (not V11–V14).
//!
//! Fully hermetic — no Docker, no network. Mirrors the tiny-synthetic-registry pattern
//! in `load.rs`'s unit tests and the fixture-generation pattern in
//! `inventory_extraction.rs`.

use std::path::{Path, PathBuf};

use deacon_conformance::inventory::{generate_inventory, write_inventory};
use deacon_conformance::load::{LoadError, Registry};
use deacon_conformance::model::ConstraintUnit;
use deacon_conformance::validate::{InventoryInputs, Violation, check_inventory};
use serde_json::{Value, json};
use tempfile::TempDir;

/// A tiny object schema yielding a handful of facet units (property-existence, type,
/// additional-properties, …) — enough to classify several units and override one.
const SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "name": { "type": "string" },
    "count": { "type": "integer" }
  },
  "additionalProperties": false
}"#;

/// The fixture's schema-revision id (matches the `empty`/`composition` schema fixtures).
const REVISION: &str = "rev-schema-fixture";

/// Lowercase-hex SHA-256, matching the manifest fingerprint format the loader verifies.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Write `contents` to `path`, creating parent directories.
fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

/// A self-consistent schemas + committed-inventory pair (fingerprint verified, committed
/// == regeneration, revision pinned to [`REVISION`]) so that, absent an injected defect,
/// the V14 provenance checks are clean.
struct Fixture {
    _tmp: TempDir,
    schemas_dir: PathBuf,
    inventory_file: PathBuf,
    units: Vec<ConstraintUnit>,
}

impl Fixture {
    fn build() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let schemas_dir = tmp.path().join("schemas");
        write(&schemas_dir.join("schema.json"), SCHEMA);
        let sha = sha256_hex(SCHEMA.as_bytes());
        let manifest = format!(
            r#"{{ "schemaVersion": 1, "revision": "{REVISION}", "documents": [
                {{ "key": "fixture", "file": "schema.json",
                   "upstreamUrl": "https://example.invalid/s.json", "sha256": "{sha}" }}
            ] }}"#
        );
        write(&schemas_dir.join("manifest.json"), &manifest);

        let inventory = generate_inventory(&schemas_dir).expect("fixture schema extracts");
        assert!(
            inventory.units.len() >= 2,
            "fixture must yield ≥2 units, got {}",
            inventory.units.len()
        );
        let inventory_file = tmp.path().join("inventory/constraints.json");
        write_inventory(&inventory_file, &inventory).expect("committed inventory writes");

        Fixture {
            _tmp: tmp,
            schemas_dir,
            inventory_file,
            units: inventory.units,
        }
    }

    fn inputs(&self) -> InventoryInputs<'_> {
        InventoryInputs {
            schemas_dir: &self.schemas_dir,
            inventory_file: &self.inventory_file,
        }
    }
}

/// The `cls-` id that correctly mirrors a `cst-` constraint id.
fn cls_id(constraint: &str) -> String {
    format!("cls-{}", constraint.strip_prefix("cst-").expect("cst- id"))
}

/// A valid `non-testable` classification for a unit (mirrored id, rationale, no behaviors).
fn valid_cls(unit: &ConstraintUnit) -> Value {
    json!({
        "id": cls_id(&unit.id),
        "constraint": unit.id,
        "disposition": "non-testable",
        "rationale": "fixture rationale"
    })
}

/// Valid classifications for every unit EXCEPT the named constraint (so a test can inject
/// its own record for that one unit without any incidental V11/V12/V13 noise elsewhere).
fn valid_except(units: &[ConstraintUnit], except: &str) -> Vec<Value> {
    units
        .iter()
        .filter(|u| u.id != except)
        .map(valid_cls)
        .collect()
}

/// Build a temp registry with a `schema`-kind revision named `revision`, the given
/// `behaviors` (`follow-spec`/`conformant`), and `classifications` under test.
fn registry(revision: &str, behaviors: &[&str], classifications: &[Value]) -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    write(
        &dir.join("revisions.json"),
        &format!(
            r#"{{ "schemaVersion": 1, "records": [
                {{ "id": "{revision}", "kind": "schema", "pin": "fixture",
                   "url": "https://example.invalid" }}
            ] }}"#
        ),
    );
    if !behaviors.is_empty() {
        let records: Vec<Value> = behaviors
            .iter()
            .map(|b| {
                json!({
                    "id": b, "area": "fixture", "statement": "s",
                    "spec": "conformant", "reference": "aligned", "decision": "follow-spec"
                })
            })
            .collect();
        write(
            &dir.join("behaviors/fixture.json"),
            &serde_json::to_string_pretty(&json!({ "schemaVersion": 1, "records": records }))
                .unwrap(),
        );
    }
    write(
        &dir.join("classifications/fixture.json"),
        &serde_json::to_string_pretty(&json!({ "schemaVersion": 1, "records": classifications }))
            .unwrap(),
    );
    (tmp, dir)
}

/// The common case: registry pinned to [`REVISION`], no behaviors.
fn plain_registry(classifications: &[Value]) -> (TempDir, PathBuf) {
    registry(REVISION, &[], classifications)
}

/// Load the fixture registry and run the V11–V14 join against the fixture inventory.
fn join(fixture: &Fixture, registry_dir: &Path) -> Vec<Violation> {
    let reg = Registry::load(registry_dir).expect("fixture registry loads");
    check_inventory(&reg, &fixture.inputs())
}

/// Assert `violations` contains at least one of `code` naming `record`.
fn has(violations: &[Violation], code: &str, record: &str) -> bool {
    violations
        .iter()
        .any(|v| v.code == code && v.record == record)
}

/// Assert every violation is `code` (no other class leaked in).
fn only(violations: &[Violation], code: &str) -> bool {
    !violations.is_empty() && violations.iter().all(|v| v.code == code)
}

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

#[test]
fn fully_classified_fixture_passes_clean() {
    let fx = Fixture::build();
    // unit[0] behavior-mapped to a real behavior; the rest non-testable. Every unit
    // has exactly one correct classification; all rules satisfied.
    let mut classifications = vec![json!({
        "id": cls_id(&fx.units[0].id),
        "constraint": fx.units[0].id,
        "disposition": "behavior-mapped",
        "behaviors": ["bhv-fixture"]
    })];
    classifications.extend(fx.units[1..].iter().map(valid_cls));
    let (_reg, dir) = registry(REVISION, &["bhv-fixture"], &classifications);

    let violations = join(&fx, &dir);
    assert!(
        violations.is_empty(),
        "a fully-classified, fully-valid fixture must have zero violations, got: {violations:#?}"
    );
}

// ---------------------------------------------------------------------------
// V11 — stale classification
// ---------------------------------------------------------------------------

#[test]
fn v11_fires_for_a_stale_classification_naming_it() {
    let fx = Fixture::build();
    let ghost = "cst-fixture-ghost-type-00000000";
    let mut classifications: Vec<Value> = fx.units.iter().map(valid_cls).collect();
    classifications.push(json!({
        "id": cls_id(ghost),
        "constraint": ghost,
        "disposition": "non-testable",
        "rationale": "points at a unit no longer in the inventory"
    }));
    let (_reg, dir) = plain_registry(&classifications);

    let violations = join(&fx, &dir);
    assert!(
        has(&violations, "V11", &cls_id(ghost)),
        "V11 must name the stale classification, got: {violations:#?}"
    );
    assert!(
        only(&violations, "V11"),
        "only V11 should fire (every real unit is classified once), got: {violations:#?}"
    );
}

// ---------------------------------------------------------------------------
// V12 — unclassified / duplicated
// ---------------------------------------------------------------------------

#[test]
fn v12_fires_for_an_unclassified_unit_naming_the_constraint() {
    let fx = Fixture::build();
    let target = fx.units[0].id.clone();
    // Classify every unit EXCEPT the first → the first is unclassified.
    let classifications = valid_except(&fx.units, &target);
    let (_reg, dir) = plain_registry(&classifications);

    let violations = join(&fx, &dir);
    assert!(
        has(&violations, "V12", &target),
        "V12 must name the unclassified constraint {target:?}, got: {violations:#?}"
    );
    assert!(
        only(&violations, "V12"),
        "only V12 should fire, got: {violations:#?}"
    );
}

#[test]
fn v12_fires_for_duplicate_classifications_naming_both() {
    let fx = Fixture::build();
    let target = fx.units[0].id.clone();
    // Two classifications point at the same unit; name both offending ids.
    let dup_a = cls_id(&target);
    let dup_b = "cls-fixture-duplicate-extra".to_string();
    let mut classifications = valid_except(&fx.units, &target);
    classifications.push(json!({
        "id": dup_a, "constraint": target,
        "disposition": "non-testable", "rationale": "first"
    }));
    classifications.push(json!({
        "id": dup_b, "constraint": target,
        "disposition": "non-testable", "rationale": "second"
    }));
    let (_reg, dir) = plain_registry(&classifications);

    let violations = join(&fx, &dir);
    let v12 = violations
        .iter()
        .find(|v| v.code == "V12" && v.record == target)
        .unwrap_or_else(|| panic!("expected a V12 for {target:?}, got: {violations:#?}"));
    // The V12 message names BOTH offending classification ids.
    assert!(
        v12.message.contains(&dup_a) && v12.message.contains(&dup_b),
        "V12 duplicate message must name both classification ids, got: {:?}",
        v12.message
    );
}

// ---------------------------------------------------------------------------
// V13 — shape / linkage (one arity rule per test)
// ---------------------------------------------------------------------------

#[test]
fn v13_fires_on_id_tail_mismatch() {
    let fx = Fixture::build();
    let target = fx.units[0].id.clone();
    let wrong_id = "cls-fixture-wrong-tail";
    let mut classifications = valid_except(&fx.units, &target);
    classifications.push(json!({
        "id": wrong_id, "constraint": target,
        "disposition": "non-testable", "rationale": "id does not mirror the constraint tail"
    }));
    let (_reg, dir) = plain_registry(&classifications);

    let violations = join(&fx, &dir);
    assert!(
        has(&violations, "V13", wrong_id) && only(&violations, "V13"),
        "id-tail mismatch must fire only V13 naming {wrong_id:?}, got: {violations:#?}"
    );
}

#[test]
fn v13_fires_when_behavior_mapped_has_empty_behaviors() {
    let fx = Fixture::build();
    let target = fx.units[0].id.clone();
    let id = cls_id(&target);
    let mut classifications = valid_except(&fx.units, &target);
    classifications.push(json!({
        "id": id, "constraint": target, "disposition": "behavior-mapped", "behaviors": []
    }));
    let (_reg, dir) = plain_registry(&classifications);

    let violations = join(&fx, &dir);
    assert!(
        has(&violations, "V13", &cls_id(&fx.units[0].id)) && only(&violations, "V13"),
        "behavior-mapped with empty behaviors must fire only V13, got: {violations:#?}"
    );
}

#[test]
fn v13_fires_when_behavior_mapped_references_a_nonexistent_behavior() {
    let fx = Fixture::build();
    let target = fx.units[0].id.clone();
    let id = cls_id(&target);
    let mut classifications = valid_except(&fx.units, &target);
    classifications.push(json!({
        "id": id, "constraint": target,
        "disposition": "behavior-mapped", "behaviors": ["bhv-does-not-exist"]
    }));
    // No behaviors registered → the referenced behavior does not exist.
    let (_reg, dir) = plain_registry(&classifications);

    let violations = join(&fx, &dir);
    assert!(
        has(&violations, "V13", &cls_id(&fx.units[0].id)) && only(&violations, "V13"),
        "behavior-mapped referencing a missing behavior must fire only V13, got: {violations:#?}"
    );
}

#[test]
fn v13_fires_when_non_behavior_mapped_has_behaviors() {
    let fx = Fixture::build();
    let target = fx.units[0].id.clone();
    let id = cls_id(&target);
    let mut classifications = valid_except(&fx.units, &target);
    // non-testable must NOT carry behaviors (existence is irrelevant to this rule).
    classifications.push(json!({
        "id": id, "constraint": target, "disposition": "non-testable",
        "rationale": "r", "behaviors": ["bhv-fixture"]
    }));
    let (_reg, dir) = registry(REVISION, &["bhv-fixture"], &classifications);

    let violations = join(&fx, &dir);
    assert!(
        has(&violations, "V13", &cls_id(&fx.units[0].id)) && only(&violations, "V13"),
        "non-behavior-mapped with a non-empty behaviors list must fire only V13, got: \
         {violations:#?}"
    );
}

#[test]
fn v13_fires_when_non_testable_lacks_rationale() {
    let fx = Fixture::build();
    let target = fx.units[0].id.clone();
    let id = cls_id(&target);
    let mut classifications = valid_except(&fx.units, &target);
    classifications.push(json!({
        "id": id, "constraint": target, "disposition": "non-testable"
    }));
    let (_reg, dir) = plain_registry(&classifications);

    let violations = join(&fx, &dir);
    assert!(
        has(&violations, "V13", &cls_id(&fx.units[0].id)) && only(&violations, "V13"),
        "non-testable without a rationale must fire only V13, got: {violations:#?}"
    );
}

#[test]
fn v13_fires_when_not_applicable_lacks_rationale() {
    let fx = Fixture::build();
    let target = fx.units[0].id.clone();
    let id = cls_id(&target);
    let mut classifications = valid_except(&fx.units, &target);
    classifications.push(json!({
        "id": id, "constraint": target, "disposition": "not-applicable"
    }));
    let (_reg, dir) = plain_registry(&classifications);

    let violations = join(&fx, &dir);
    assert!(
        has(&violations, "V13", &cls_id(&fx.units[0].id)) && only(&violations, "V13"),
        "not-applicable without a rationale must fire only V13, got: {violations:#?}"
    );
}

// ---------------------------------------------------------------------------
// V14 — provenance (one rule per test)
// ---------------------------------------------------------------------------

#[test]
fn v14_fires_on_manifest_fingerprint_mismatch() {
    let fx = Fixture::build();
    // Tamper the vendored schema WITHOUT updating the manifest's sha256.
    let tampered = SCHEMA.replacen("\"string\"", "\"number\"", 1);
    assert_ne!(tampered, SCHEMA, "tamper must change the bytes");
    write(&fx.schemas_dir.join("schema.json"), &tampered);

    // Every unit is validly classified so ONLY the provenance breakage surfaces.
    let classifications: Vec<Value> = fx.units.iter().map(valid_cls).collect();
    let (_reg, dir) = plain_registry(&classifications);

    let violations = join(&fx, &dir);
    assert!(
        only(&violations, "V14")
            && violations
                .iter()
                .any(|v| v.code == "V14" && v.message.contains("fingerprint")),
        "a tampered vendored schema must fire only a V14 fingerprint mismatch, got: {violations:#?}"
    );
}

#[test]
fn v14_fires_on_inventory_revision_pin_mismatch() {
    let fx = Fixture::build();
    let classifications: Vec<Value> = fx.units.iter().map(valid_cls).collect();
    // Registry's schema revision id differs from the inventory's `revision`.
    let (_reg, dir) = registry("rev-schema-other", &[], &classifications);

    let violations = join(&fx, &dir);
    assert!(
        has(&violations, "V14", REVISION) && only(&violations, "V14"),
        "a revision-pin mismatch must fire only V14 naming the inventory revision, got: \
         {violations:#?}"
    );
}

#[test]
fn v14_fires_when_committed_inventory_differs_from_regeneration() {
    let fx = Fixture::build();
    // Corrupt the committed BYTES while keeping the JSON (and thus the units) parseable:
    // an extra trailing newline is still valid JSON but no longer byte-matches `render`.
    let committed = std::fs::read_to_string(&fx.inventory_file).unwrap();
    std::fs::write(&fx.inventory_file, format!("{committed}\n")).unwrap();

    let classifications: Vec<Value> = fx.units.iter().map(valid_cls).collect();
    let (_reg, dir) = plain_registry(&classifications);

    let violations = join(&fx, &dir);
    let record = fx.inventory_file.display().to_string();
    assert!(
        has(&violations, "V14", &record) && only(&violations, "V14"),
        "a committed-vs-regenerated byte mismatch must fire only V14, got: {violations:#?}"
    );
}

// ---------------------------------------------------------------------------
// Scaffold sentinel — rejected at LOAD time as a SCHEMA-class failure
// ---------------------------------------------------------------------------

#[test]
fn scaffold_sentinel_is_rejected_at_load_as_schema_not_v11_v14() {
    // The `inventory scaffold` sentinel `"UNREVIEWED"` is not a member of the closed
    // `Disposition` enum, so it fails to deserialize — a SCHEMA-class load failure,
    // caught long before any V11–V14 join.
    let (_reg, dir) = plain_registry(&[json!({
        "id": "cls-fixture-x", "constraint": "cst-fixture-x-type-00000000",
        "disposition": "UNREVIEWED"
    })]);
    let err = Registry::load(&dir).expect_err("the sentinel disposition must fail to load");
    match err {
        LoadError::Schema(errors) => {
            assert!(
                errors.iter().any(|e| e.file.ends_with("fixture.json")),
                "the SCHEMA failure must name the classifications file, got: {errors:#?}"
            );
        }
        other => panic!("expected a SCHEMA-class load failure, got {other:?}"),
    }
}
