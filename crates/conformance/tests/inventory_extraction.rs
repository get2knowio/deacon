//! Constraint extraction acceptance tests (T016, spec FR-023).
//!
//! Exercises the extractor against the hand-authored fixtures under
//! `fixtures/conformance/schemas/*`: composition/nullable/required/additional-property
//! tri-states/conditional-context/unmodeled-keyword capture on the success path, and
//! each error fixture producing its EXACT typed [`LoadError`] (cycle chain, malformed
//! JSON, unresolved fragment, external URL). Also verifies manifest fingerprint
//! enforcement by tampering a copied fixture. Fully hermetic — no Docker, no network.

use std::path::PathBuf;

use deacon_conformance::inventory::generate_inventory;
use deacon_conformance::load::{LoadError, Registry};
use deacon_conformance::model::{ConstraintInventory, ConstraintKind, ConstraintUnit, UnitContext};
use deacon_conformance::report::build_report;
use deacon_conformance::workspace_root;
use serde_json::{Value, json};

/// Absolute path to a single-document fixture manifest directory.
fn fixture(name: &str) -> PathBuf {
    workspace_root()
        .join("fixtures/conformance/schemas")
        .join(name)
}

/// Generate a fixture's units, panicking with the error on failure.
fn units(name: &str) -> Vec<ConstraintUnit> {
    generate_inventory(&fixture(name))
        .unwrap_or_else(|e| panic!("fixture {name:?} should extract, got {e}"))
        .units
}

/// Find the single unit matching `(pointer, kind)`, or panic.
fn one<'a>(units: &'a [ConstraintUnit], pointer: &str, kind: ConstraintKind) -> &'a ConstraintUnit {
    let matches: Vec<&ConstraintUnit> = units
        .iter()
        .filter(|u| u.pointer == pointer && u.kind == kind)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one {kind:?} at {pointer:?}, got {}",
        matches.len()
    );
    matches[0]
}

fn has(units: &[ConstraintUnit], pointer: &str, kind: ConstraintKind) -> bool {
    units.iter().any(|u| u.pointer == pointer && u.kind == kind)
}

#[test]
fn nested_composition_resolves_each_arm() {
    let units = units("composition");

    // allOf on `combined` is one composition edge; its two arms are recursed.
    let allof = one(&units, "/properties/combined", ConstraintKind::AllOf);
    assert_eq!(allof.substance, json!({ "allOf": 2 }));

    // oneOf/anyOf arms each become their own union-alternative with the arm index,
    // AND their content is extracted at the arm pointer.
    for (branch, pointer_prefix) in [
        ("oneOf", "/properties/choice"),
        ("anyOf", "/properties/either"),
    ] {
        for index in 0..2 {
            let arm = format!("{pointer_prefix}/{branch}/{index}");
            let ua = one(&units, &arm, ConstraintKind::UnionAlternative);
            assert_eq!(ua.substance, json!({ "branch": branch, "index": index }));
            match &ua.context {
                Some(UnitContext::Branch(b)) => {
                    assert_eq!(b.branch, branch);
                    assert_eq!(b.index, index);
                }
                other => panic!("union arm {arm} missing branch context: {other:?}"),
            }
        }
    }
    // Arm content extracted at its own pointer (the integer arm's value-shape bounds).
    let bounds = one(
        &units,
        "/properties/choice/oneOf/0",
        ConstraintKind::ValueShape,
    );
    assert_eq!(bounds.substance, json!({ "minimum": 0, "maximum": 100 }));
    assert!(has(
        &units,
        "/properties/choice/oneOf/0",
        ConstraintKind::Type
    ));
    assert!(has(
        &units,
        "/properties/choice/oneOf/1",
        ConstraintKind::ValueShape
    ));
}

#[test]
fn ref_is_an_edge_never_inlined() {
    let units = units("composition");
    // The anyOf arm `#/definitions/leaf` is a single reference edge recording the
    // resolved target pointer; the leaf's own content is extracted once at the leaf.
    let edge = one(
        &units,
        "/properties/either/anyOf/1",
        ConstraintKind::Reference,
    );
    assert_eq!(
        edge.substance,
        json!({ "ref": "#/definitions/leaf", "targetPointer": "/definitions/leaf" })
    );
    // The leaf's `b` property is extracted at the definition site exactly once.
    assert!(has(
        &units,
        "/definitions/leaf/properties/b",
        ConstraintKind::PropertyExistence
    ));
    let inlined_at_usage = units
        .iter()
        .filter(|u| {
            u.pointer.starts_with("/properties/either/anyOf/1/")
                && u.kind == ConstraintKind::PropertyExistence
        })
        .count();
    assert_eq!(
        inlined_at_usage, 0,
        "ref target must not be inlined at the usage site"
    );
}

#[test]
fn nullable_flag_set_when_null_in_type_union() {
    let units = units("composition");
    let t = one(&units, "/properties/nullable", ConstraintKind::Type);
    assert_eq!(
        t.substance,
        json!({ "type": ["string", "null"], "nullable": true })
    );
    // A non-null single type carries no nullable flag.
    let plain = one(&units, "/properties/closed", ConstraintKind::Type);
    assert_eq!(plain.substance, json!({ "type": "object" }));
}

#[test]
fn required_captured_per_property() {
    let units = units("composition");
    // Top-level required names each become a distinct unit at their folded pointer.
    let closed = one(&units, "/required/closed", ConstraintKind::Required);
    assert_eq!(closed.substance, json!({ "required": "closed" }));
    assert_eq!(
        one(&units, "/required/choice", ConstraintKind::Required).substance,
        json!({ "required": "choice" })
    );
    // A required inside an allOf arm keeps its branch context.
    let nested = one(
        &units,
        "/properties/combined/allOf/0/required/a",
        ConstraintKind::Required,
    );
    assert_eq!(nested.substance, json!({ "required": "a" }));
    assert!(matches!(nested.context, Some(UnitContext::Branch(_))));
}

#[test]
fn additional_properties_tristate_is_distinguished() {
    let units = units("composition");
    let mode = |pointer: &str| -> String {
        one(&units, pointer, ConstraintKind::AdditionalProperties).substance["mode"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(mode("/properties/closed"), "closed"); // additionalProperties: false
    assert_eq!(mode("/properties/openMap"), "open"); // additionalProperties: true
    assert_eq!(mode("/properties/schemaMap"), "schema"); // additionalProperties: {schema}
    // The schema-valued case is ALSO recursed (its inner string type is extracted).
    assert!(has(
        &units,
        "/properties/schemaMap/additionalProperties",
        ConstraintKind::Type
    ));
}

#[test]
fn conditional_context_is_preserved() {
    let units = units("composition");
    let cond = one(
        &units,
        "/properties/conditional",
        ConstraintKind::Conditional,
    );
    assert_eq!(cond.substance, json!({ "clauses": ["if", "then", "else"] }));
    // then/else sub-schemas carry the condition's own pointer as context.
    for clause in ["then", "else"] {
        let name = if clause == "then" { "a" } else { "b" };
        let req = one(
            &units,
            &format!("/properties/conditional/{clause}/required/{name}"),
            ConstraintKind::Required,
        );
        match &req.context {
            Some(UnitContext::Condition(c)) => {
                assert_eq!(c.condition, "/properties/conditional/if");
            }
            other => panic!("{clause} required missing condition context: {other:?}"),
        }
    }
    // The `if` clause content is extracted (the const inside it).
    assert!(has(
        &units,
        "/properties/conditional/if/properties/kind",
        ConstraintKind::Const
    ));
}

#[test]
fn unmodeled_keyword_captured_verbatim() {
    let units = units("composition");
    let unmodeled: Vec<&ConstraintUnit> = units
        .iter()
        .filter(|u| u.kind == ConstraintKind::UnmodeledKeyword)
        .collect();
    // Both custom keywords on `encoded` are captured verbatim, nothing dropped.
    let vendor = unmodeled
        .iter()
        .find(|u| u.substance["keyword"] == json!("x-vendor-note"))
        .expect("x-vendor-note must be captured as unmodeled-keyword");
    assert_eq!(
        vendor.substance["value"],
        json!("an unmodeled keyword the extractor must capture verbatim")
    );
    let encoding = unmodeled
        .iter()
        .find(|u| u.substance["keyword"] == json!("contentEncoding"))
        .expect("contentEncoding must be captured as unmodeled-keyword");
    assert_eq!(encoding.substance["value"], json!("base64"));
}

#[test]
fn empty_schema_extracts_without_error() {
    // A trivial `{}` schema yields a sane, empty inventory (no keywords → no facets).
    let inv = generate_inventory(&fixture("empty")).expect("empty schema extracts");
    assert!(inv.units.is_empty(), "empty schema produces no units");
    assert_eq!(inv.revision, "rev-schema-fixture");
}

// ---- Error fixtures: each produces its exact typed error --------------------

#[test]
fn cycle_fixture_reports_the_full_chain() {
    let err = generate_inventory(&fixture("cycle")).unwrap_err();
    match err {
        LoadError::RefCycle { chain } => {
            assert_eq!(chain.first(), chain.last(), "chain returns to its start");
            assert!(chain.iter().any(|c| c.contains("/definitions/a")));
            assert!(chain.iter().any(|c| c.contains("/definitions/b")));
            // The rendered message lists the loop.
            let msg = LoadError::RefCycle { chain }.to_string();
            assert!(
                msg.contains("/definitions/a -> /definitions/b -> /definitions/a"),
                "{msg}"
            );
        }
        other => panic!("expected RefCycle, got {other:?}"),
    }
}

#[test]
fn malformed_fixture_reports_malformed_schema() {
    let err = generate_inventory(&fixture("malformed")).unwrap_err();
    assert!(
        matches!(err, LoadError::MalformedSchema { .. }),
        "got {err:?}"
    );
}

#[test]
fn unresolved_fragment_ref_reports_unresolved_ref() {
    let err = generate_inventory(&fixture("unresolved-ref")).unwrap_err();
    match err {
        LoadError::UnresolvedRef { target, .. } => assert_eq!(target, "/definitions/missing"),
        other => panic!("expected UnresolvedRef, got {other:?}"),
    }
}

#[test]
fn external_url_ref_reports_unresolved_external_ref_without_fetch() {
    let err = generate_inventory(&fixture("external-ref")).unwrap_err();
    assert!(
        matches!(err, LoadError::UnresolvedExternalRef { .. }),
        "got {err:?}"
    );
}

#[test]
fn recursive_ok_fixture_is_not_a_cycle() {
    // Productive self-reference through structural keywords extracts finitely.
    let inv = generate_inventory(&fixture("recursive-ok")).expect("productive recursion extracts");
    assert!(!inv.units.is_empty());
    // The self-ref edge is captured once at the definition site.
    assert!(
        inv.units
            .iter()
            .any(|u| u.pointer == "/definitions/node/properties/self"
                && u.kind == ConstraintKind::Reference)
    );
}

#[test]
fn manifest_fingerprint_mismatch_is_blocking() {
    // Copy the composition fixture into a temp dir, tamper one byte of the schema
    // WITHOUT updating the manifest's sha256 → the fingerprint check must fail.
    let src = fixture("composition");
    let tmp = tempfile::tempdir().unwrap();
    let manifest = std::fs::read_to_string(src.join("manifest.json")).unwrap();
    std::fs::write(tmp.path().join("manifest.json"), &manifest).unwrap();

    let mut schema = std::fs::read_to_string(src.join("composition.json")).unwrap();
    // Flip a byte in a way that keeps it valid JSON but changes the bytes.
    schema = schema.replacen("\"closed\"", "\"clssed\"", 1);
    // Sanity: the tamper actually changed the content and stays parseable.
    assert!(serde_json::from_str::<Value>(&schema).is_ok());
    std::fs::write(tmp.path().join("composition.json"), &schema).unwrap();

    let err = generate_inventory(tmp.path()).unwrap_err();
    match err {
        LoadError::ManifestFingerprintMismatch { file, .. } => {
            assert!(file.ends_with("composition.json"));
        }
        other => panic!("expected ManifestFingerprintMismatch, got {other:?}"),
    }
}

// ---- User Story 4: explicitly pinned external schema source (T035/T036) ------
//
// The `multi-source/` fixture is a genuinely separate two-document pinned set:
// `extra` cross-references `base-fixture` via a manifest-relative `$ref`
// (`./base-fixture.json#/definitions/Something`, research Decision 5). The
// `multi-source-missing/` sibling drops `base-fixture` from the pinned set so the
// same ref now points outside it — a hard `UnresolvedExternalRef`, never a fetch.

/// The full inventory (units + revision) for a schemas fixture directory.
fn inventory(name: &str) -> ConstraintInventory {
    generate_inventory(&fixture(name))
        .unwrap_or_else(|e| panic!("fixture {name:?} should extract, got {e}"))
}

/// Find the single unit matching `(document, pointer, kind)`, or panic. Unlike
/// [`one`], this disambiguates by document — the root pointer `""` collides across
/// documents in a multi-document set.
fn one_in<'a>(
    units: &'a [ConstraintUnit],
    document: &str,
    pointer: &str,
    kind: ConstraintKind,
) -> &'a ConstraintUnit {
    let matches: Vec<&ConstraintUnit> = units
        .iter()
        .filter(|u| u.document == document && u.pointer == pointer && u.kind == kind)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one {kind:?} at {document}#{pointer:?}, got {}",
        matches.len()
    );
    matches[0]
}

#[test]
fn cross_document_reference_carries_target_document_key() {
    // The `$ref` edge from `extra` to `base-fixture` is a single reference unit whose
    // substance records the RESOLVED target document key + pointer (never inlined).
    let units = inventory("multi-source").units;
    let edge = one_in(
        &units,
        "extra",
        "/properties/linked",
        ConstraintKind::Reference,
    );
    // The edge lives in `extra`…
    assert_eq!(edge.document, "extra");
    // …but points at `base-fixture`'s definition, and says so explicitly.
    assert_eq!(
        edge.substance,
        json!({
            "ref": "./base-fixture.json#/definitions/Something",
            "targetDocument": "base-fixture",
            "targetPointer": "/definitions/Something"
        })
    );
    assert_eq!(edge.substance["targetDocument"], json!("base-fixture"));
}

#[test]
fn multi_source_units_attributed_to_own_document() {
    // Each unit is attributed to whichever document actually DEFINES it — the two
    // pinned sources never conflate into one.
    let units = inventory("multi-source").units;

    // A unit that only exists in `base-fixture` (the referenced definition's required
    // name) is attributed to `base-fixture`.
    let base_required = one_in(
        &units,
        "base-fixture",
        "/definitions/Something/required/name",
        ConstraintKind::Required,
    );
    assert_eq!(base_required.document, "base-fixture");
    assert_eq!(base_required.substance, json!({ "required": "name" }));

    // A unit that only exists in `extra` (its own top-level property) is attributed to
    // `extra`.
    let extra_prop = one_in(
        &units,
        "extra",
        "/properties/linked",
        ConstraintKind::PropertyExistence,
    );
    assert_eq!(extra_prop.document, "extra");

    // Every unit belongs to exactly one of the two pinned document keys, and BOTH are
    // populated — separate-source attribution, not a merged blob.
    let base_count = units
        .iter()
        .filter(|u| u.document == "base-fixture")
        .count();
    let extra_count = units.iter().filter(|u| u.document == "extra").count();
    assert!(base_count > 0, "base-fixture must contribute units");
    assert!(extra_count > 0, "extra must contribute units");
    assert_eq!(base_count + extra_count, units.len());
    for u in &units {
        assert!(
            u.document == "base-fixture" || u.document == "extra",
            "unexpected document key {:?}",
            u.document
        );
    }
    // The referenced `base-fixture` definition's content is extracted at its own
    // definition site (in `base-fixture`), NOT inlined into `extra` at the usage site.
    assert!(
        !units
            .iter()
            .any(|u| u.document == "extra" && u.pointer.starts_with("/properties/linked/")),
        "the ref target must not be inlined into the referencing document"
    );
}

#[test]
fn missing_pinned_target_reports_unresolved_external_ref_without_fetch() {
    // The variant manifest omits `base-fixture` from the pinned set, so `extra`'s
    // relative ref now names an UNPINNED document. Resolution fails loud with a
    // cause-specific error that identifies BOTH the offending reference and the
    // document it lives in — and never touches the network (resolve.rs answers purely
    // from the in-memory pinned DocumentSet; there is no HTTP client on this path).
    let err = generate_inventory(&fixture("multi-source-missing")).unwrap_err();
    match &err {
        LoadError::UnresolvedExternalRef {
            document,
            reference,
        } => {
            assert_eq!(document, "extra", "error names the offending document");
            assert_eq!(
                reference, "./base-fixture.json#/definitions/Something",
                "error names the offending reference"
            );
        }
        other => panic!("expected UnresolvedExternalRef, got {other:?}"),
    }
    // The rendered message surfaces both the reference and the document.
    let msg = err.to_string();
    assert!(
        msg.contains("./base-fixture.json#/definitions/Something") && msg.contains("extra"),
        "message must name the ref and document: {msg}"
    );
}

#[test]
fn report_lists_multi_source_documents_separately() {
    // Separate-source attribution surfaces all the way through to the report's
    // inventory section: `unitsByDocument` shows the two pinned documents as distinct
    // keys with the counts the extractor attributed to each (T028 join).
    let inv = inventory("multi-source");
    let expected_base = inv
        .units
        .iter()
        .filter(|u| u.document == "base-fixture")
        .count();
    let expected_extra = inv.units.iter().filter(|u| u.document == "extra").count();

    // An empty registry is fine here — we are exercising the inventory join, not the
    // classification coverage. The section still tallies units by document.
    let report = build_report(&Registry::default(), Some(&inv));
    let by_doc = &report.inventory.units_by_document;

    assert_eq!(
        by_doc.get("base-fixture").copied(),
        Some(expected_base),
        "report attributes base-fixture units to their own document key"
    );
    assert_eq!(
        by_doc.get("extra").copied(),
        Some(expected_extra),
        "report attributes extra units to their own document key"
    );
    // Two distinct source keys, together accounting for every unit.
    assert_eq!(by_doc.len(), 2, "exactly the two pinned documents");
    assert_eq!(report.inventory.total_units, inv.units.len());
}
