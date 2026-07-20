//! Baseline assertions on the committed inventory (T019, spec FR-024,
//! contracts/inventory-schema.md "Baseline assertions").
//!
//! Pins observable facts about the real committed
//! `conformance/inventory/constraints.json` — the units superseding the two retired
//! hand-written schema source records, the base document's top-level container
//! variants, a known nullable union, and the EXACT per-document unit counts. The exact
//! counts are an intentional tripwire: any accidental extraction change breaks this
//! test loudly (updated consciously on re-vendoring). Hermetic.

use deacon_conformance::default_inventory_file;
use deacon_conformance::load::load_inventory;
use deacon_conformance::model::{ConstraintInventory, ConstraintKind, ConstraintUnit};
use serde_json::json;

/// Exact committed unit counts per document (029→ pinned tripwire — update ONLY on a
/// conscious re-vendoring of the pinned schemas).
const BASE_UNIT_COUNT: usize = 403;
const FEATURE_UNIT_COUNT: usize = 206;

fn committed() -> ConstraintInventory {
    load_inventory(&default_inventory_file())
        .expect("committed inventory loads")
        .expect("committed inventory exists")
}

fn find<'a>(
    inv: &'a ConstraintInventory,
    pointer: &str,
    kind: ConstraintKind,
) -> Option<&'a ConstraintUnit> {
    inv.units
        .iter()
        .find(|u| u.pointer == pointer && u.kind == kind)
}

#[test]
fn exact_unit_counts_per_document() {
    let inv = committed();
    let base = inv.units.iter().filter(|u| u.document == "base").count();
    let feature = inv.units.iter().filter(|u| u.document == "feature").count();
    assert_eq!(base, BASE_UNIT_COUNT, "base document unit count changed");
    assert_eq!(
        feature, FEATURE_UNIT_COUNT,
        "feature document unit count changed"
    );
    assert_eq!(
        inv.units.len(),
        BASE_UNIT_COUNT + FEATURE_UNIT_COUNT,
        "only base + feature documents are inventoried"
    );
    assert_eq!(inv.revision, "rev-schema-113500f4");
}

/// The pinned schemas must yield NO `unmodeled-keyword` units — a re-vendoring tripwire.
///
/// The extractor recurses into a fixed set of sub-schema positions; any keyword outside
/// that set is captured verbatim as a single `unmodeled-keyword` unit and is NOT
/// recursed into. That is deliberate (nothing is ever silently dropped) but coarse: a
/// keyword whose value CONTAINS sub-schemas collapses a whole subtree into one opaque
/// unit, which a human then classifies with one disposition instead of classifying the
/// constraints inside it.
///
/// The base document is draft 2019-09, so `dependentSchemas`, `dependentRequired`, and
/// `unevaluatedItems` are all live possibilities at the next pin bump (`dependencies`
/// likewise for the draft-07 feature document). None appear today, so this holds at
/// zero.
///
/// If a re-vendoring trips this, do NOT just bump the expectation: decide how the new
/// keyword should be modelled (its own `ConstraintKind`, folded into an existing one, or
/// legitimately opaque) and teach `schema::extract` to recurse into it if it carries
/// sub-schemas. Substance participates in the unit ID hash, so choosing the model late
/// is far cheaper than choosing it wrong and re-hashing every affected classification.
#[test]
fn pinned_schemas_yield_no_unmodeled_keywords() {
    let inv = committed();
    let unmodeled: Vec<(&str, String)> = inv
        .units
        .iter()
        .filter(|u| u.kind == ConstraintKind::UnmodeledKeyword)
        .map(|u| {
            let keyword = u
                .substance
                .get("keyword")
                .and_then(|k| k.as_str())
                .unwrap_or("<none>")
                .to_string();
            (u.pointer.as_str(), keyword)
        })
        .collect();

    assert!(
        unmodeled.is_empty(),
        "the pinned schemas introduced {} unmodeled keyword(s): {unmodeled:?}\n\
         Decide how to model each one (see this test's doc comment) — do not simply \
         update the expectation.",
        unmodeled.len()
    );
}

#[test]
fn forward_ports_array_type_unit_exists() {
    let inv = committed();
    let t = find(
        &inv,
        "/definitions/devContainerCommon/properties/forwardPorts",
        ConstraintKind::Type,
    )
    .expect("forwardPorts type unit must exist");
    assert_eq!(t.substance, json!({ "type": "array" }));
    assert_eq!(t.document, "base");
}

#[test]
fn features_object_and_additional_properties_units_exist() {
    let inv = committed();
    let ptr = "/definitions/devContainerCommon/properties/features";
    let t = find(&inv, ptr, ConstraintKind::Type).expect("features type unit");
    assert_eq!(t.substance, json!({ "type": "object" }));
    let ap = find(&inv, ptr, ConstraintKind::AdditionalProperties)
        .expect("features additional-properties unit");
    // `additionalProperties: true` → the open tri-state.
    assert_eq!(
        ap.substance,
        json!({ "keyword": "additionalProperties", "mode": "open" })
    );
}

#[test]
fn base_top_level_oneof_container_variants_present() {
    let inv = committed();
    // The base schema's top-level `oneOf` has two container variants.
    for index in 0..2 {
        let pointer = format!("/oneOf/{index}");
        let ua = find(&inv, &pointer, ConstraintKind::UnionAlternative)
            .unwrap_or_else(|| panic!("top-level union-alternative at {pointer} must exist"));
        assert_eq!(ua.substance, json!({ "branch": "oneOf", "index": index }));
        assert_eq!(ua.document, "base");
    }
}

#[test]
fn a_known_nullable_union_carries_the_flag() {
    let inv = committed();
    // remoteEnv values are `["string", "null"]` — the canonical nullable union.
    let t = find(
        &inv,
        "/definitions/devContainerCommon/properties/remoteEnv/additionalProperties",
        ConstraintKind::Type,
    )
    .expect("remoteEnv additionalProperties type unit");
    assert_eq!(
        t.substance,
        json!({ "type": ["string", "null"], "nullable": true })
    );

    // At least one nullable union exists across the inventory (FR-023 null-handling).
    let nullable_count = inv
        .units
        .iter()
        .filter(|u| {
            u.kind == ConstraintKind::Type && u.substance.get("nullable") == Some(&json!(true))
        })
        .count();
    assert!(
        nullable_count >= 1,
        "at least one nullable union must be present"
    );
}

#[test]
fn every_unit_id_is_grammar_valid_with_cst_prefix() {
    use deacon_conformance::model::{RecordType, parse_id};
    let inv = committed();
    for u in &inv.units {
        let ty = parse_id(&u.id).unwrap_or_else(|e| panic!("invalid id {}: {e}", u.id));
        assert_eq!(
            ty,
            RecordType::Constraint,
            "id {} is not a cst- record",
            u.id
        );
    }
}
