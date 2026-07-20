//! Acceptance tests for the constraint-inventory revision diff (T033, spec US3 /
//! FR-023 drift + SC-007 "no disposition inheritance").
//!
//! Two families of test:
//!
//! 1. **Diff correctness + determinism** — generate inventories from the committed
//!    `old/` and `new/` drift fixtures via the real extraction pipeline
//!    ([`generate_inventory`]), diff them, and assert the exact added / removed /
//!    changed / non-material sets the fixtures were designed to produce (see
//!    `fixtures/conformance/inventory-drift/README.md`). The changed entry carries
//!    `oldId != newId` with the correct old/new substance; the moved-but-identical
//!    constraint is reported as one removed + one added (never a "move"); the
//!    description reword is non-material. Both the JSON and Markdown renderings are
//!    asserted byte-deterministic across two independent runs.
//!
//! 2. **Drift workflow — no disposition inheritance (SC-007)** — author classifications
//!    against the OLD inventory, then run the `validate` V11–V14 join
//!    ([`check_inventory`]) against the NEW inventory and assert that units present
//!    only in `old` surface as V11 (stale classification) while units present only in
//!    `new` surface as V12 (unclassified) — nothing carries a disposition across by
//!    id-similarity, proving drift is fully non-inheriting.
//!
//! Fully hermetic — no Docker, no network. The fixtures are read-only; every generated
//! artifact lives in a `tempfile` scratch dir.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use deacon_conformance::diff::{ChangeEntry, UnitEntry, diff, render_json, render_md};
use deacon_conformance::inventory::{generate_inventory, write_inventory};
use deacon_conformance::load::Registry;
use deacon_conformance::model::{ConstraintInventory, ConstraintUnit};
use deacon_conformance::validate::{InventoryInputs, Violation, check_inventory};
use deacon_conformance::workspace_root;
use serde_json::{Value, json};

/// The committed drift fixture directory (`old/` and `new/` schema + manifest pairs).
fn drift_fixture(side: &str) -> PathBuf {
    workspace_root()
        .join("fixtures/conformance/inventory-drift")
        .join(side)
}

/// Generate the inventory for one drift-fixture side via the real extraction pipeline.
fn generate_side(side: &str) -> ConstraintInventory {
    generate_inventory(&drift_fixture(side))
        .unwrap_or_else(|e| panic!("drift fixture `{side}` must extract cleanly: {e}"))
}

/// The pointers of an added/removed bucket, for set assertions.
fn pointers(entries: &[UnitEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.pointer.as_str()).collect()
}

/// The pointers of a changed/non-material bucket.
fn change_pointers(entries: &[ChangeEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.pointer.as_str()).collect()
}

// ---------------------------------------------------------------------------
// 1. Diff correctness + determinism
// ---------------------------------------------------------------------------

#[test]
fn diff_reports_exactly_the_designed_drift_sets() {
    let old = generate_side("old");
    let new = generate_side("new");
    let d = diff(&old, &new);

    // Revisions carried through from each fixture manifest.
    assert_eq!(d.old.revision, "rev-schema-driftold");
    assert_eq!(d.new.revision, "rev-schema-driftnew");

    // Added: one pure addition (`newbie`) + the move-in (`leafmoved`). Sorted by key.
    assert_eq!(
        pointers(&d.added),
        vec!["/definitions/leafmoved", "/definitions/newbie"],
        "added must be exactly the move-in plus the pure add"
    );

    // Removed: one pure removal (`goner`) + the move-out (`leaf`). Sorted by key.
    assert_eq!(
        pointers(&d.removed),
        vec!["/definitions/goner", "/definitions/leaf"],
        "removed must be exactly the move-out plus the pure remove"
    );

    // Changed: exactly the type-widening at `/properties/widened`, with distinct ids
    // and the correct old/new substance.
    assert_eq!(change_pointers(&d.changed), vec!["/properties/widened"]);
    let c = &d.changed[0];
    assert_ne!(
        c.old_id, c.new_id,
        "a materially changed constraint gets a NEW id (drift-forcing)"
    );
    assert_eq!(c.old_substance, json!({ "type": "string" }));
    assert_eq!(
        c.new_substance,
        json!({ "type": ["string", "null"], "nullable": true }),
        "the widened type substance is shown verbatim"
    );

    // Non-material: exactly the description reword at `/properties/documented` — an
    // annotation-kind difference, segregated out of `changed`.
    assert_eq!(
        change_pointers(&d.non_material),
        vec!["/properties/documented"]
    );
    let nm = &d.non_material[0];
    assert_eq!(
        nm.old_substance,
        json!({ "keyword": "description", "value": "Original wording." })
    );
    assert_eq!(
        nm.new_substance,
        json!({ "keyword": "description", "value": "Revised wording." })
    );

    // Exact bucket cardinalities — nothing incidental leaks in.
    assert_eq!(
        (
            d.added.len(),
            d.removed.len(),
            d.changed.len(),
            d.non_material.len()
        ),
        (2, 2, 1, 1)
    );
}

#[test]
fn moved_but_identical_constraint_is_removed_plus_added_not_a_move() {
    // `leaf` (old) → `leafmoved` (new), both `{"type":"boolean"}`: identical substance,
    // different pointer. The spec Assumption requires this to be a removal + an addition
    // — never fuzzy move-tracking.
    let old = generate_side("old");
    let new = generate_side("new");
    let d = diff(&old, &new);

    let removed_leaf = d
        .removed
        .iter()
        .find(|e| e.pointer == "/definitions/leaf")
        .expect("the move-out is a removal");
    let added_leafmoved = d
        .added
        .iter()
        .find(|e| e.pointer == "/definitions/leafmoved")
        .expect("the move-in is an addition");

    // Substance-identical (that is what makes it a "move") but reported as two entries.
    assert_eq!(removed_leaf.substance, json!({ "type": "boolean" }));
    assert_eq!(added_leafmoved.substance, json!({ "type": "boolean" }));
    // The two ids differ only because the JSON Pointer participates in the stable-id
    // hash (research Decision 6); the SUBSTANCE is byte-identical, which is what makes
    // this a move — yet the diff reports it as a plain remove + add (match key includes
    // the pointer), never fuzzy move-tracking (spec Assumption).
    assert_ne!(removed_leaf.id, added_leafmoved.id);
    // It never appears in changed / non-material.
    assert!(
        !change_pointers(&d.changed).contains(&"/definitions/leaf")
            && !change_pointers(&d.changed).contains(&"/definitions/leafmoved")
    );
}

#[test]
fn diff_output_is_byte_deterministic_across_runs() {
    // Two fully independent generate → diff → render passes must be byte-identical, in
    // BOTH the JSON and Markdown forms (SC-002 discipline applied to the diff itself).
    let run = || {
        let old = generate_side("old");
        let new = generate_side("new");
        diff(&old, &new)
    };
    let a = run();
    let b = run();

    let json_a = render_json(&a);
    let json_b = render_json(&b);
    assert_eq!(
        json_a, json_b,
        "diff JSON must be byte-identical across runs"
    );
    assert!(json_a.ends_with('\n'), "diff JSON is newline-terminated");

    let md_a = render_md(&a);
    let md_b = render_md(&b);
    assert_eq!(
        md_a, md_b,
        "diff Markdown must be byte-identical across runs"
    );
    assert!(md_a.contains("# Constraint Inventory Diff"));

    // Determinism is not accidental emptiness.
    assert!(!a.is_empty(), "the drift fixtures produce a non-empty diff");
}

#[test]
fn identical_inventories_diff_to_an_empty_but_valid_document() {
    let inv = generate_side("new");
    let d = diff(&inv, &inv);
    assert!(d.is_empty(), "an inventory against itself has no drift");
    // Empty is still a well-formed, newline-terminated document (CLI exit 0).
    let json = render_json(&d);
    assert!(json.ends_with('\n'));
    assert!(json.contains("\"added\": []"));
}

// ---------------------------------------------------------------------------
// 2. Drift workflow — no disposition inheritance (SC-007)
// ---------------------------------------------------------------------------

/// Write `contents` to `path`, creating parent directories.
fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

/// The `cls-` id that mirrors a `cst-` constraint id (V13 id-tail rule).
fn cls_id(constraint: &str) -> String {
    format!("cls-{}", constraint.strip_prefix("cst-").expect("cst- id"))
}

/// A valid `non-testable` classification record (mirrored id, rationale, no behaviors)
/// pointing at `constraint`.
fn non_testable_cls(constraint: &str) -> Value {
    json!({
        "id": cls_id(constraint),
        "constraint": constraint,
        "disposition": "non-testable",
        "rationale": "authored against the OLD revision"
    })
}

/// The set of ids appearing on one side but not the other.
fn only_in<'a>(these: &'a [ConstraintUnit], not_in: &[ConstraintUnit]) -> Vec<&'a str> {
    let other: HashSet<&str> = not_in.iter().map(|u| u.id.as_str()).collect();
    these
        .iter()
        .map(|u| u.id.as_str())
        .filter(|id| !other.contains(id))
        .collect()
}

#[test]
fn regenerating_to_a_new_revision_inherits_no_disposition() {
    // Author classifications against the OLD inventory, switch the committed inventory to
    // the NEW revision, and run the V11–V14 join. Old-only units go V11-stale on their
    // classifications; new-only units go V12-unclassified. Nothing carries over by
    // id-similarity (SC-007) — the drift is fully non-inheriting.
    let old = generate_side("old");
    let new = generate_side("new");

    // Ids present on exactly one side (these are the drift review workload).
    let old_only = only_in(&old.units, &new.units);
    let new_only = only_in(&new.units, &old.units);
    // Sanity: the fixtures were designed so each side has exactly the drift deltas
    // (goner + leaf + old-widened-type + old-documented-annotation, and the new twins).
    assert_eq!(old_only.len(), 4, "old-only drift units: {old_only:?}");
    assert_eq!(new_only.len(), 4, "new-only drift units: {new_only:?}");

    // A temp scratch dir: the committed NEW inventory + a registry whose classifications
    // were authored against the OLD inventory ids.
    let tmp = tempfile::tempdir().unwrap();
    let inventory_file = tmp.path().join("inventory/constraints.json");
    write_inventory(&inventory_file, &new).expect("new inventory writes");

    // Registry: a `schema`-kind revision pinned to the NEW inventory's revision (so V14's
    // revision-pin check is clean) + one non-testable classification per OLD unit id.
    let registry_dir = tmp.path().join("registry");
    write(
        &registry_dir.join("revisions.json"),
        &format!(
            r#"{{ "schemaVersion": 1, "records": [
                {{ "id": "{}", "kind": "schema", "pin": "fixture",
                   "url": "https://example.invalid" }}
            ] }}"#,
            new.revision
        ),
    );
    let classifications: Vec<Value> = old.units.iter().map(|u| non_testable_cls(&u.id)).collect();
    write(
        &registry_dir.join("classifications/drift.json"),
        &serde_json::to_string_pretty(&json!({ "schemaVersion": 1, "records": classifications }))
            .unwrap(),
    );

    let reg = Registry::load(&registry_dir).expect("drift registry loads");
    let inputs = InventoryInputs {
        schemas_dir: &drift_fixture("new"),
        inventory_file: &inventory_file,
    };
    let violations = check_inventory(&reg, &inputs);

    // Every OLD-only unit's classification is now V11-stale (its constraint id is absent
    // from the committed NEW inventory).
    for old_id in &old_only {
        let expected_cls = cls_id(old_id);
        assert!(
            has_violation(&violations, "V11", &expected_cls),
            "expected V11 (stale) on classification {expected_cls:?} for old-only unit \
             {old_id:?}; got {violations:#?}"
        );
    }

    // Every NEW-only unit is V12-unclassified — proving no disposition was inherited from
    // the similarly-located OLD unit (e.g. the new widened `type` unit is UNCLASSIFIED
    // even though the old widened `type` unit was classified non-testable).
    for new_id in &new_only {
        assert!(
            has_violation(&violations, "V12", new_id),
            "expected V12 (unclassified) on new-only unit {new_id:?} — a disposition must \
             NOT carry over by id-similarity; got {violations:#?}"
        );
    }

    // Exactly the drift workload is flagged: 4 V11 (stale) + 4 V12 (unclassified). The
    // units shared by both revisions keep their (valid) classifications with no noise.
    let v11 = violations.iter().filter(|v| v.code == "V11").count();
    let v12 = violations.iter().filter(|v| v.code == "V12").count();
    assert_eq!((v11, v12), (4, 4), "drift workload: {violations:#?}");
}

/// Whether `violations` contains one with the given code and record id.
fn has_violation(violations: &[Violation], code: &str, record: &str) -> bool {
    violations
        .iter()
        .any(|v| v.code == code && v.record == record)
}
