//! Deterministic revision diff between two constraint inventories (T030, research
//! Decision 9 + data-model.md §4).
//!
//! [`diff`] compares an `old` and a `new` [`ConstraintInventory`], matching units on
//! the coarse key `(document, pointer, kind)` — deliberately NOT on `id`. The `id`
//! also encodes the substance hash (research Decision 6), so it is a FINER key than
//! the match key: matching on the coarser `(document, pointer, kind)` is what lets a
//! same-location substance change read as one `changed` entry (with `oldId`/`newId`
//! and both substances) rather than as an unrelated add/remove pair — the most
//! reviewable presentation of a pin bump. A unit present only on the right is `added`;
//! only on the left is `removed`.
//!
//! A matched pair whose canonical substance differs is a **material change**
//! (`changed`) UNLESS its kind is [`ConstraintKind::Annotation`], in which case the
//! difference is segregated into the separate `nonMaterial` list: annotation keywords
//! (titles, descriptions, examples, editor hints) carry no testable behavior (research
//! Decision 4), so their churn is non-material by construction (spec Assumption
//! "Descriptive metadata is non-testable, not invisible"). This is keyed on the
//! `annotation` KIND, not on any "description-wording-only" heuristic.
//!
//! A moved-but-identical constraint (same substance, different pointer) is therefore
//! reported as one `removed` + one `added` — the diff never attempts fuzzy
//! move-tracking (spec Assumption "A moved-but-identical constraint is reported as
//! removed + added"). Every bucket is deterministically sorted by the match key, and
//! both the JSON ([`render_json`]) and Markdown ([`render_md`]) renderers mirror
//! `report.rs`'s byte-stable discipline (sorted, no timestamps, LF, trailing newline)
//! so identical inputs produce byte-identical output on every platform.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Serialize;
use serde_json::Value;

use crate::model::{ConstraintInventory, ConstraintKind, ConstraintUnit};

/// The revision-diff schema version (data-model.md §4).
const SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Output model (field order IS the on-disk key order; camelCase on the wire)
// ---------------------------------------------------------------------------

/// The complete `inventory diff` document (data-model.md §4). Every bucket is sorted
/// by the `(document, pointer, kind)` match key, so the serialization is byte-stable
/// for identical inputs.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionDiff {
    pub schema_version: u32,
    /// The left (old) inventory's revision.
    pub old: DiffSide,
    /// The right (new) inventory's revision.
    pub new: DiffSide,
    /// Units present only in `new` (all kinds), sorted by match key.
    pub added: Vec<UnitEntry>,
    /// Units present only in `old` (all kinds), sorted by match key.
    pub removed: Vec<UnitEntry>,
    /// Matched-key pairs whose substance changed materially (kind ≠ `annotation`),
    /// sorted by match key.
    pub changed: Vec<ChangeEntry>,
    /// Matched-key pairs whose substance changed on an `annotation`-kind unit —
    /// non-material by construction (research Decision 9), sorted by match key.
    pub non_material: Vec<ChangeEntry>,
}

/// One side's revision identity in a [`RevisionDiff`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiffSide {
    pub revision: String,
}

/// An added-or-removed constraint unit — carries the full unit shape (data-model.md
/// §4 `added`/`removed` entries).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnitEntry {
    pub id: String,
    pub document: String,
    pub pointer: String,
    pub kind: ConstraintKind,
    pub substance: Value,
}

/// A matched-key change (material `changed` or non-material `nonMaterial`): the shared
/// location + kind, both IDs, and both substances (data-model.md §4 `changed` entries).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEntry {
    pub document: String,
    pub pointer: String,
    pub kind: ConstraintKind,
    pub old_id: String,
    pub new_id: String,
    pub old_substance: Value,
    pub new_substance: Value,
}

// ---------------------------------------------------------------------------
// Diffing
// ---------------------------------------------------------------------------

/// The match key `(document, pointer, kind wire spelling, facet discriminator)`
/// (research Decision 9). `kind` is folded in as its serde wire spelling so the key is
/// `Ord` for stable sorting without depending on the enum's `Ord` derivation.
///
/// The trailing facet discriminator is what makes the key identify ONE unit rather than
/// a group — see [`facet_discriminator`].
type MatchKey = (String, String, String, String);

/// Build the match key for a unit. `kind` serialization is infallible for a plain
/// closed enum.
fn match_key(unit: &ConstraintUnit) -> MatchKey {
    (
        unit.document.clone(),
        unit.pointer.clone(),
        kind_wire(&unit.kind),
        facet_discriminator(&unit.substance),
    )
}

/// Which facet of an object a unit describes, for kinds that emit MORE THAN ONE unit at
/// the same `(document, pointer, kind)`:
///
/// - `annotation` — one unit per annotation keyword (`title` AND `description` AND
///   `examples` all sit on the same schema object);
/// - `unmodeled-keyword` — one unit per unrecognised keyword;
/// - `additional-properties` — `additionalProperties` / `unevaluatedProperties` plus one
///   unit per `patternProperties` entry.
///
/// Each such substance names its own facet via `keyword` (plus `pattern` for a
/// `patternProperties` entry); the REST of the substance is the content that decides
/// changed-vs-unchanged. Folding only the facet name into the match key keeps siblings
/// from colliding while leaving the coarse `(document, pointer, kind)` grouping of
/// research Decision 9 intact for single-facet kinds (which discriminate to `""`).
///
/// Without this, two sibling facets share a key: a removed `examples` would be paired
/// against a surviving `description` and reported as a bogus reword instead of a
/// removal.
fn facet_discriminator(substance: &Value) -> String {
    let mut out = String::new();
    if let Some(keyword) = substance.get("keyword").and_then(Value::as_str) {
        out.push_str(keyword);
    }
    if let Some(pattern) = substance.get("pattern").and_then(Value::as_str) {
        // A separator that cannot occur in a keyword, so `keyword`/`pattern` can never
        // alias across a pair of units.
        out.push('\u{1f}');
        out.push_str(pattern);
    }
    out
}

/// Bucket units by match key, each bucket id-sorted.
///
/// Buckets (rather than a plain `BTreeMap<_, &ConstraintUnit>`) are a belt-and-braces
/// guarantee that no unit is ever silently dropped: should a future extractor emit two
/// units sharing even the discriminated key, they are paired positionally instead of one
/// overwriting the other in the map.
fn bucket_by_key(units: &[ConstraintUnit]) -> BTreeMap<MatchKey, Vec<&ConstraintUnit>> {
    let mut out: BTreeMap<MatchKey, Vec<&ConstraintUnit>> = BTreeMap::new();
    for unit in units {
        out.entry(match_key(unit)).or_default().push(unit);
    }
    for bucket in out.values_mut() {
        bucket.sort_by(|a, b| a.id.cmp(&b.id));
    }
    out
}

/// Compute the deterministic revision diff of `new` against `old` (data-model.md §4).
/// Matched on `(document, pointer, kind)`; substance decides `changed`;
/// annotation-kind changes are segregated to `nonMaterial`; every bucket is sorted by
/// the match key.
pub fn diff(old: &ConstraintInventory, new: &ConstraintInventory) -> RevisionDiff {
    // Index each side by match key, bucketed so no unit is ever lost to a key clash
    // (a `BTreeMap` also keeps a deterministic key order for free).
    let old_by_key = bucket_by_key(&old.units);
    let new_by_key = bucket_by_key(&new.units);

    let mut added: Vec<UnitEntry> = Vec::new();
    let mut removed: Vec<UnitEntry> = Vec::new();
    let mut changed: Vec<ChangeEntry> = Vec::new();
    let mut non_material: Vec<ChangeEntry> = Vec::new();

    // New side: added (right-only) and the changed/non-material matched pairs. Within a
    // bucket the two id-sorted sides are paired positionally; any surplus is a genuine
    // add.
    for (key, new_bucket) in &new_by_key {
        let old_bucket = old_by_key.get(key).map(Vec::as_slice).unwrap_or(&[]);
        for (i, nu) in new_bucket.iter().enumerate() {
            let Some(ou) = old_bucket.get(i) else {
                added.push(unit_entry(nu));
                continue;
            };
            if ou.substance != nu.substance {
                let entry = change_entry(ou, nu);
                // Annotation-kind substance differences are non-material by
                // construction (research Decision 9); every other kind is a
                // material `changed`.
                if nu.kind == ConstraintKind::Annotation {
                    non_material.push(entry);
                } else {
                    changed.push(entry);
                }
            }
        }
    }

    // Old side: removed — a left-only key, or a left surplus within a shared bucket.
    for (key, old_bucket) in &old_by_key {
        let new_len = new_by_key.get(key).map(Vec::len).unwrap_or(0);
        for ou in old_bucket.iter().skip(new_len) {
            removed.push(unit_entry(ou));
        }
    }

    // `BTreeMap` iteration already yields match-key order; sort explicitly so the
    // contract ("sorted by match key") holds regardless of construction path.
    added.sort_by_key(entry_key);
    removed.sort_by_key(entry_key);
    changed.sort_by_key(change_key);
    non_material.sort_by_key(change_key);

    RevisionDiff {
        schema_version: SCHEMA_VERSION,
        old: DiffSide {
            revision: old.revision.clone(),
        },
        new: DiffSide {
            revision: new.revision.clone(),
        },
        added,
        removed,
        changed,
        non_material,
    }
}

impl RevisionDiff {
    /// Whether the two inventories are unit-identical (every bucket empty).
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.changed.is_empty()
            && self.non_material.is_empty()
    }
}

fn unit_entry(unit: &ConstraintUnit) -> UnitEntry {
    UnitEntry {
        id: unit.id.clone(),
        document: unit.document.clone(),
        pointer: unit.pointer.clone(),
        kind: unit.kind,
        substance: unit.substance.clone(),
    }
}

fn change_entry(old: &ConstraintUnit, new: &ConstraintUnit) -> ChangeEntry {
    ChangeEntry {
        document: new.document.clone(),
        pointer: new.pointer.clone(),
        kind: new.kind,
        old_id: old.id.clone(),
        new_id: new.id.clone(),
        old_substance: old.substance.clone(),
        new_substance: new.substance.clone(),
    }
}

/// Match-key tuple for a [`UnitEntry`] (sort key).
fn entry_key(e: &UnitEntry) -> MatchKey {
    (
        e.document.clone(),
        e.pointer.clone(),
        kind_wire(&e.kind),
        facet_discriminator(&e.substance),
    )
}

/// Match-key tuple for a [`ChangeEntry`] (sort key). Both sides of a pair share a match
/// key by construction, so the new side's substance determines the discriminator.
fn change_key(e: &ChangeEntry) -> MatchKey {
    (
        e.document.clone(),
        e.pointer.clone(),
        kind_wire(&e.kind),
        facet_discriminator(&e.new_substance),
    )
}

/// A closed enum's serde wire spelling (kebab-case), quotes stripped. Infallible.
fn kind_wire(kind: &ConstraintKind) -> String {
    serde_json::to_string(kind)
        .unwrap_or_else(|e| unreachable!("kind serialization is infallible: {e}"))
        .trim_matches('"')
        .to_string()
}

// ---------------------------------------------------------------------------
// Serialization — JSON + Markdown (mirroring report.rs)
// ---------------------------------------------------------------------------

/// Render the diff to its canonical JSON string: pretty-printed (2-space indent,
/// field/declaration order), newline-terminated, byte-stable for identical inputs
/// (mirrors `report.rs::render_json`).
pub fn render_json(diff: &RevisionDiff) -> String {
    let mut out = serde_json::to_string_pretty(diff)
        .unwrap_or_else(|e| unreachable!("diff serialization is infallible: {e}"));
    out.push('\n');
    out
}

/// Render the diff to a human-review Markdown document — deterministic and derived
/// from the same sorted model as [`render_json`] (mirrors `report.rs::render_md`). No
/// timestamps; LF endings; trailing newline.
pub fn render_md(diff: &RevisionDiff) -> String {
    let mut md = String::new();

    md.push_str("# Constraint Inventory Diff\n\n");
    let _ = writeln!(md, "**Old revision:** `{}`", diff.old.revision);
    let _ = writeln!(md, "**New revision:** `{}`\n", diff.new.revision);

    md.push_str("## Summary\n\n");
    md.push_str("| Bucket | Count |\n|--------|-------|\n");
    let _ = writeln!(md, "| Added | {} |", diff.added.len());
    let _ = writeln!(md, "| Removed | {} |", diff.removed.len());
    let _ = writeln!(md, "| Changed | {} |", diff.changed.len());
    let _ = writeln!(md, "| Non-material | {} |", diff.non_material.len());

    render_unit_section(&mut md, "Added", &diff.added);
    render_unit_section(&mut md, "Removed", &diff.removed);
    render_change_section(&mut md, "Changed", &diff.changed);
    render_change_section(&mut md, "Non-material", &diff.non_material);

    md
}

/// Render an `added`/`removed` unit table (id, document, pointer, kind, substance).
fn render_unit_section(md: &mut String, title: &str, entries: &[UnitEntry]) {
    let _ = write!(md, "\n## {title}\n\n");
    if entries.is_empty() {
        md.push_str("None.\n");
        return;
    }
    md.push_str("| ID | Document | Pointer | Kind | Substance |\n");
    md.push_str("|----|----------|---------|------|-----------|\n");
    for e in entries {
        let _ = writeln!(
            md,
            "| `{}` | `{}` | `{}` | `{}` | `{}` |",
            e.id,
            e.document,
            e.pointer,
            kind_wire(&e.kind),
            compact(&e.substance),
        );
    }
}

/// Render a `changed`/`nonMaterial` table (location + kind, both ids, both substances).
fn render_change_section(md: &mut String, title: &str, entries: &[ChangeEntry]) {
    let _ = write!(md, "\n## {title}\n\n");
    if entries.is_empty() {
        md.push_str("None.\n");
        return;
    }
    md.push_str(
        "| Document | Pointer | Kind | Old ID | New ID | Old substance | New substance |\n",
    );
    md.push_str(
        "|----------|---------|------|--------|--------|---------------|---------------|\n",
    );
    for e in entries {
        let _ = writeln!(
            md,
            "| `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | `{}` |",
            e.document,
            e.pointer,
            kind_wire(&e.kind),
            e.old_id,
            e.new_id,
            compact(&e.old_substance),
            compact(&e.new_substance),
        );
    }
}

/// Compact one-line JSON for a substance value in a Markdown table cell.
fn compact(value: &Value) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|e| unreachable!("substance serialization is infallible: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Build a unit for tests (the id need not be a real derived hash — the diff never
    /// re-derives ids, it only carries them through).
    fn unit(id: &str, pointer: &str, kind: ConstraintKind, substance: Value) -> ConstraintUnit {
        ConstraintUnit {
            id: id.to_string(),
            document: "d".to_string(),
            pointer: pointer.to_string(),
            kind,
            substance,
            context: None,
        }
    }

    fn inventory(revision: &str, units: Vec<ConstraintUnit>) -> ConstraintInventory {
        ConstraintInventory {
            schema_version: 1,
            revision: revision.to_string(),
            units,
        }
    }

    #[test]
    fn pure_add_is_reported_as_added_only() {
        let old = inventory("rev-schema-a", vec![]);
        let new = inventory(
            "rev-schema-b",
            vec![unit(
                "cst-d-x-type-11111111",
                "/x",
                ConstraintKind::Type,
                json!({ "type": "string" }),
            )],
        );
        let d = diff(&old, &new);
        assert_eq!(d.added.len(), 1);
        assert_eq!(d.added[0].id, "cst-d-x-type-11111111");
        assert!(d.removed.is_empty());
        assert!(d.changed.is_empty());
        assert!(d.non_material.is_empty());
        assert!(!d.is_empty());
    }

    #[test]
    fn pure_remove_is_reported_as_removed_only() {
        let old = inventory(
            "rev-schema-a",
            vec![unit(
                "cst-d-x-type-11111111",
                "/x",
                ConstraintKind::Type,
                json!({ "type": "string" }),
            )],
        );
        let new = inventory("rev-schema-b", vec![]);
        let d = diff(&old, &new);
        assert_eq!(d.removed.len(), 1);
        assert_eq!(d.removed[0].id, "cst-d-x-type-11111111");
        assert!(d.added.is_empty());
        assert!(d.changed.is_empty());
        assert!(d.non_material.is_empty());
    }

    #[test]
    fn material_substance_change_is_changed_with_both_ids() {
        // Same (document, pointer, kind); substance widened → CHANGED, oldId != newId.
        let old = inventory(
            "rev-schema-a",
            vec![unit(
                "cst-d-x-type-aaaaaaaa",
                "/x",
                ConstraintKind::Type,
                json!({ "type": "string" }),
            )],
        );
        let new = inventory(
            "rev-schema-b",
            vec![unit(
                "cst-d-x-type-bbbbbbbb",
                "/x",
                ConstraintKind::Type,
                json!({ "type": ["string", "null"], "nullable": true }),
            )],
        );
        let d = diff(&old, &new);
        assert_eq!(d.changed.len(), 1);
        let c = &d.changed[0];
        assert_eq!(c.old_id, "cst-d-x-type-aaaaaaaa");
        assert_eq!(c.new_id, "cst-d-x-type-bbbbbbbb");
        assert_ne!(
            c.old_id, c.new_id,
            "a material change must carry distinct ids"
        );
        assert_eq!(c.old_substance, json!({ "type": "string" }));
        assert_eq!(
            c.new_substance,
            json!({ "type": ["string", "null"], "nullable": true })
        );
        // Not surfaced as an add/remove pair.
        assert!(d.added.is_empty());
        assert!(d.removed.is_empty());
        assert!(d.non_material.is_empty());
    }

    #[test]
    fn annotation_only_change_is_non_material() {
        // A description reword: same location + annotation kind, different substance.
        let old = inventory(
            "rev-schema-a",
            vec![unit(
                "cst-d-x-anno-aaaaaaaa",
                "/x",
                ConstraintKind::Annotation,
                json!({ "keyword": "description", "value": "old" }),
            )],
        );
        let new = inventory(
            "rev-schema-b",
            vec![unit(
                "cst-d-x-anno-bbbbbbbb",
                "/x",
                ConstraintKind::Annotation,
                json!({ "keyword": "description", "value": "new" }),
            )],
        );
        let d = diff(&old, &new);
        assert!(d.changed.is_empty(), "annotation change is NOT material");
        assert_eq!(d.non_material.len(), 1);
        let c = &d.non_material[0];
        assert_eq!(c.old_id, "cst-d-x-anno-aaaaaaaa");
        assert_eq!(c.new_id, "cst-d-x-anno-bbbbbbbb");
        assert!(d.added.is_empty());
        assert!(d.removed.is_empty());
    }

    #[test]
    fn moved_but_identical_is_removed_plus_added_not_a_move() {
        // Same substance, different pointer → the match key (which includes pointer)
        // differs, so it is one removed + one added, never a detected "move".
        let old = inventory(
            "rev-schema-a",
            vec![unit(
                "cst-d-leaf-type-cafef00d",
                "/definitions/leaf",
                ConstraintKind::Type,
                json!({ "type": "boolean" }),
            )],
        );
        let new = inventory(
            "rev-schema-b",
            vec![unit(
                // Substance-identical but at a new pointer; the real derived id would
                // differ (the pointer participates in the id hash), and the diff carries
                // whatever id it is given through verbatim.
                "cst-d-leafmoved-type-cafef00e",
                "/definitions/leafmoved",
                ConstraintKind::Type,
                json!({ "type": "boolean" }),
            )],
        );
        let d = diff(&old, &new);
        assert_eq!(d.added.len(), 1, "the move-in is an addition");
        assert_eq!(d.added[0].pointer, "/definitions/leafmoved");
        assert_eq!(d.removed.len(), 1, "the move-out is a removal");
        assert_eq!(d.removed[0].pointer, "/definitions/leaf");
        assert!(
            d.changed.is_empty() && d.non_material.is_empty(),
            "identical substance at a new location is never a change"
        );
    }

    #[test]
    fn sibling_facets_at_one_pointer_do_not_collide() {
        // `title`/`description`/`examples` all sit on the SAME schema object, so they
        // share (document, pointer, kind=annotation). Dropping one upstream must read as
        // exactly one removal — never as a bogus reword of a surviving sibling, and
        // never as silence.
        let anno = |id: &str, keyword: &str, value: &str| {
            unit(
                id,
                "/definitions/x/properties/capAdd",
                ConstraintKind::Annotation,
                json!({ "keyword": keyword, "value": value }),
            )
        };
        let old = inventory(
            "rev-schema-a",
            vec![
                anno("cst-d-capadd-anno-11111111", "description", "Passes caps."),
                anno("cst-d-capadd-anno-22222222", "examples", "SYS_PTRACE"),
            ],
        );
        let new = inventory(
            "rev-schema-b",
            vec![anno(
                "cst-d-capadd-anno-11111111",
                "description",
                "Passes caps.",
            )],
        );

        let d = diff(&old, &new);
        assert_eq!(
            d.removed.len(),
            1,
            "the dropped `examples` facet is a removal"
        );
        assert_eq!(d.removed[0].id, "cst-d-capadd-anno-22222222");
        assert!(
            d.non_material.is_empty() && d.changed.is_empty(),
            "the surviving `description` is untouched, not a reword: {d:?}"
        );
        assert!(d.added.is_empty());
    }

    #[test]
    fn sibling_facets_report_their_own_content_changes() {
        // Rewording ONE facet must not be attributed to its sibling.
        let anno = |id: &str, keyword: &str, value: &str| {
            unit(
                id,
                "/x",
                ConstraintKind::Annotation,
                json!({ "keyword": keyword, "value": value }),
            )
        };
        let old = inventory(
            "rev-schema-a",
            vec![
                anno("cst-d-x-anno-aaaaaaaa", "description", "old text"),
                anno("cst-d-x-anno-bbbbbbbb", "title", "Title"),
            ],
        );
        let new = inventory(
            "rev-schema-b",
            vec![
                anno("cst-d-x-anno-cccccccc", "description", "new text"),
                anno("cst-d-x-anno-bbbbbbbb", "title", "Title"),
            ],
        );
        let d = diff(&old, &new);
        assert_eq!(d.non_material.len(), 1, "exactly the reworded facet: {d:?}");
        assert_eq!(d.non_material[0].old_id, "cst-d-x-anno-aaaaaaaa");
        assert_eq!(d.non_material[0].new_id, "cst-d-x-anno-cccccccc");
        assert!(d.added.is_empty() && d.removed.is_empty());
    }

    #[test]
    fn additional_properties_siblings_are_distinguished_by_keyword_and_pattern() {
        // One object can carry `additionalProperties` AND several `patternProperties`
        // entries — all `additional-properties` kind at the same pointer.
        let mk = |id: &str, substance: Value| {
            unit(
                id,
                "/p/secrets",
                ConstraintKind::AdditionalProperties,
                substance,
            )
        };
        let closed = json!({ "keyword": "additionalProperties", "mode": "closed" });
        let pat_a = json!({ "keyword": "patternProperties", "pattern": "^[a-z]+$" });
        let pat_b = json!({ "keyword": "patternProperties", "pattern": "^[A-Z]+$" });

        let old = inventory(
            "rev-schema-a",
            vec![
                mk("cst-d-secrets-addprop-11111111", closed.clone()),
                mk("cst-d-secrets-addprop-22222222", pat_a.clone()),
                mk("cst-d-secrets-addprop-33333333", pat_b),
            ],
        );
        let new = inventory(
            "rev-schema-b",
            vec![
                mk("cst-d-secrets-addprop-11111111", closed),
                mk("cst-d-secrets-addprop-22222222", pat_a),
            ],
        );
        let d = diff(&old, &new);
        assert_eq!(d.removed.len(), 1, "only the dropped pattern: {d:?}");
        assert_eq!(d.removed[0].id, "cst-d-secrets-addprop-33333333");
        assert!(d.added.is_empty() && d.changed.is_empty() && d.non_material.is_empty());
    }

    #[test]
    fn empty_diff_of_identical_inventories() {
        let inv = inventory(
            "rev-schema-a",
            vec![
                unit(
                    "cst-d-x-type-11111111",
                    "/x",
                    ConstraintKind::Type,
                    json!({ "type": "string" }),
                ),
                unit(
                    "cst-d-y-anno-22222222",
                    "/y",
                    ConstraintKind::Annotation,
                    json!({ "keyword": "title", "value": "Y" }),
                ),
            ],
        );
        let d = diff(&inv, &inv);
        assert!(d.is_empty(), "identical inventories diff to nothing: {d:?}");
    }

    #[test]
    fn output_is_byte_deterministic_in_both_forms() {
        let old = inventory(
            "rev-schema-a",
            vec![
                unit(
                    "cst-d-b-type-22222222",
                    "/b",
                    ConstraintKind::Type,
                    json!({ "type": "string" }),
                ),
                unit(
                    "cst-d-a-type-11111111",
                    "/a",
                    ConstraintKind::Type,
                    json!({ "type": "string" }),
                ),
            ],
        );
        let new = inventory(
            "rev-schema-b",
            vec![unit(
                "cst-d-c-type-33333333",
                "/c",
                ConstraintKind::Type,
                json!({ "type": "string" }),
            )],
        );
        let j1 = render_json(&diff(&old, &new));
        let j2 = render_json(&diff(&old, &new));
        assert_eq!(j1, j2, "JSON must be byte-identical across runs");
        assert!(j1.ends_with('\n'));
        let m1 = render_md(&diff(&old, &new));
        let m2 = render_md(&diff(&old, &new));
        assert_eq!(m1, m2, "Markdown must be byte-identical across runs");
        assert!(m1.contains("# Constraint Inventory Diff"));
    }

    #[test]
    fn buckets_are_sorted_by_match_key() {
        // Removals at /a, /c, /b arrive out of order → must come out sorted by pointer.
        let old = inventory(
            "rev-schema-a",
            vec![
                unit("cst-d-c-type-33", "/c", ConstraintKind::Type, json!({})),
                unit("cst-d-a-type-11", "/a", ConstraintKind::Type, json!({})),
                unit("cst-d-b-type-22", "/b", ConstraintKind::Type, json!({})),
            ],
        );
        let new = inventory("rev-schema-b", vec![]);
        let d = diff(&old, &new);
        let pointers: Vec<&str> = d.removed.iter().map(|e| e.pointer.as_str()).collect();
        assert_eq!(pointers, vec!["/a", "/b", "/c"]);
    }
}
