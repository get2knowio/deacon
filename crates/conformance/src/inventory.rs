//! Constraint inventory generation, canonical serialization, and the
//! generate/check comparison (T013, research Decisions 6–7).
//!
//! [`generate_inventory`] drives the whole pipeline: load + fingerprint-verify the
//! schemas manifest, parse each vendored document, reject reference cycles, extract
//! every facet, derive a stable ID per unit, detect collisions, and sort by ID.
//! [`render`] serializes the result canonically (sorted object keys in substance,
//! 2-space indent, LF, trailing newline — the exact discipline `report.rs` follows),
//! and [`write_inventory`] commits it atomically (temp file + rename, matching
//! `cache/disk.rs::save_index`). [`compare`] powers `inventory check`.
//!
//! ## Stable ID scheme (Decision 6)
//!
//! `cst-<doc>-<slug>-<kind code>-<hash8>`:
//! - `<doc>` — manifest document key;
//! - `<slug>` — a readable slug of the pointer's trailing segments (identity is NOT
//!   carried here — see below);
//! - `<kind code>` — a short stable token per [`ConstraintKind`] ([`kind_code`]);
//! - `<hash8>` — the first 8 lowercase-hex chars of SHA-256 over
//!   `document ‖ pointer ‖ kind ‖ canonical(substance)`, joined by the `0x1f` unit
//!   separator (a byte that never appears in a pointer, a kind spelling, or canonical
//!   JSON text). Substance participates ⇒ a material change yields a new ID
//!   (drift-forcing); the slug does not, so slug churn cannot move identity.

use std::path::Path;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::load::{LoadError, load_schemas_manifest};
use crate::model::{ConstraintInventory, ConstraintKind, ConstraintUnit};
use crate::schema::extract::{ExtractedUnit, extract};
use crate::schema::resolve::check_ref_cycles;
use crate::schema::{DocumentSet, SchemaDocument};

/// The inventory schema version (data-model §2).
const SCHEMA_VERSION: u32 = 1;

/// The fixed separator byte for the ID hash inputs. `0x1f` (ASCII Unit Separator)
/// cannot appear in a JSON Pointer, a kebab-case kind spelling, or canonical JSON text
/// (JSON escapes all control characters inside strings), so the join is unambiguous.
const HASH_SEPARATOR: u8 = 0x1f;

/// The maximum slug length (readability only — identity lives in the hash). Slugs
/// longer than this are hard-truncated to their trailing 48 characters at a clean
/// hyphen boundary (Decision 6 / data-model §2).
const SLUG_MAX: usize = 48;

// ---------------------------------------------------------------------------
// Generation pipeline
// ---------------------------------------------------------------------------

/// Generate the constraint inventory from the vendored, pinned schemas under
/// `schemas_dir` (default `conformance/schemas/<pin>/`). Fail-loud on every malformed
/// / cyclic / unresolved input; never produces a partial result.
pub fn generate_inventory(schemas_dir: &Path) -> Result<ConstraintInventory, LoadError> {
    // 1. Manifest load + SHA-256 fingerprint verification (V14 on mismatch).
    let manifest = load_schemas_manifest(schemas_dir)?;

    // 2. Parse each vendored document (malformed JSON → MalformedSchema).
    let mut documents = Vec::with_capacity(manifest.documents.len());
    for doc in &manifest.documents {
        let path = schemas_dir.join(&doc.file);
        let raw = std::fs::read_to_string(&path).map_err(|e| LoadError::MalformedSchema {
            document: doc.key.clone(),
            cause: format!("could not read vendored schema {path:?}: {e}"),
        })?;
        let root: Value = serde_json::from_str(&raw).map_err(|e| LoadError::MalformedSchema {
            document: doc.key.clone(),
            cause: format!("invalid JSON at {}:{}: {e}", e.line(), e.column()),
        })?;
        documents.push(SchemaDocument {
            key: doc.key.clone(),
            file: doc.file.clone(),
            root,
        });
    }
    let docs = DocumentSet::new(documents);

    // 3. Reject unproductive pure-$ref cycles (Decision 5) before extraction.
    check_ref_cycles(&docs)?;

    // 4. Extract + derive IDs, detecting collisions.
    let mut units: Vec<ConstraintUnit> = Vec::new();
    let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for doc in docs.documents() {
        for ExtractedUnit {
            pointer,
            kind,
            substance,
            context,
        } in extract(doc, &docs)?
        {
            let substance = canonicalize(&substance);
            let id = derive_id(&doc.key, &pointer, kind, &substance);
            if let Some(first) = seen.get(&id) {
                return Err(LoadError::IdCollision {
                    id,
                    first: first.clone(),
                    second: pointer,
                });
            }
            seen.insert(id.clone(), pointer.clone());
            units.push(ConstraintUnit {
                id,
                document: doc.key.clone(),
                pointer,
                kind,
                substance,
                context,
            });
        }
    }

    // 5. Sort by ID (the committed order — data-model §2).
    units.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(ConstraintInventory {
        schema_version: SCHEMA_VERSION,
        revision: manifest.revision,
        units,
    })
}

// ---------------------------------------------------------------------------
// Stable ID derivation (Decision 6)
// ---------------------------------------------------------------------------

/// Derive the stable `cst-…` id for a unit. `substance` MUST already be canonicalized
/// so the hash is deterministic.
fn derive_id(document: &str, pointer: &str, kind: ConstraintKind, substance: &Value) -> String {
    let slug = slugify_pointer(pointer);
    let code = kind_code(kind);
    let hash8 = hash8(document, pointer, kind, substance);
    format!("cst-{document}-{slug}-{code}-{hash8}")
}

/// The first 8 lowercase-hex chars of SHA-256 over the identity inputs, joined by the
/// `0x1f` separator: `document ‖ pointer ‖ kind ‖ canonical(substance)`.
fn hash8(document: &str, pointer: &str, kind: ConstraintKind, substance: &Value) -> String {
    let kind_wire = serde_json::to_string(&kind)
        .unwrap_or_else(|e| unreachable!("kind serialization is infallible: {e}"));
    let kind_wire = kind_wire.trim_matches('"');
    // Compact canonical substance (sorted object keys, preserved array order).
    let substance = serde_json::to_string(substance)
        .unwrap_or_else(|e| unreachable!("canonical substance serialization is infallible: {e}"));

    let mut hasher = Sha256::new();
    hasher.update(document.as_bytes());
    hasher.update([HASH_SEPARATOR]);
    hasher.update(pointer.as_bytes());
    hasher.update([HASH_SEPARATOR]);
    hasher.update(kind_wire.as_bytes());
    hasher.update([HASH_SEPARATOR]);
    hasher.update(substance.as_bytes());
    let digest = hasher.finalize();

    let mut hex = String::with_capacity(8);
    for b in &digest[..4] {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// A short, stable kind code for the ID (kept as an explicit table so the mapping is
/// obviously stable across regenerations — Decision 6).
fn kind_code(kind: ConstraintKind) -> &'static str {
    match kind {
        ConstraintKind::PropertyExistence => "prop",
        ConstraintKind::Required => "req",
        ConstraintKind::Type => "type",
        ConstraintKind::Enum => "enum",
        ConstraintKind::Const => "const",
        ConstraintKind::Default => "def",
        ConstraintKind::UnionAlternative => "union",
        ConstraintKind::AllOf => "allof",
        ConstraintKind::Conditional => "cond",
        ConstraintKind::AdditionalProperties => "addprop",
        ConstraintKind::ArrayShape => "arr",
        ConstraintKind::ValueShape => "val",
        ConstraintKind::Reference => "ref",
        ConstraintKind::Annotation => "anno",
        ConstraintKind::UnmodeledKeyword => "unmod",
    }
}

/// Slugify a JSON Pointer for readability: lowercase, non-alphanumerics collapsed to
/// `-`. The result is bounded to [`SLUG_MAX`] by keeping its TRAILING characters (the
/// meaningful leaf lives at the end of a pointer) at a clean hyphen boundary. The empty
/// pointer (document root) slugs to `root`.
fn slugify_pointer(pointer: &str) -> String {
    let mut slug = String::with_capacity(pointer.len());
    let mut prev_dash = false;
    for ch in pointer.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        return "root".to_string();
    }
    if slug.len() <= SLUG_MAX {
        return slug.to_string();
    }
    // Keep the trailing SLUG_MAX chars, then trim to the next clean hyphen boundary so
    // we never start a slug mid-word.
    let tail = &slug[slug.len() - SLUG_MAX..];
    let trimmed = match tail.find('-') {
        Some(i) => &tail[i + 1..],
        None => tail,
    };
    let trimmed = trimmed.trim_matches('-');
    if trimmed.is_empty() {
        tail.trim_matches('-').to_string()
    } else {
        trimmed.to_string()
    }
}

// ---------------------------------------------------------------------------
// Canonical JSON (sorted object keys; array order preserved)
// ---------------------------------------------------------------------------

/// Recursively rebuild `value` with object keys in sorted order (arrays untouched —
/// their order is semantically significant, e.g. enum members, type unions). With
/// serde_json's `preserve_order` feature enabled this is what makes substance
/// byte-stable regardless of upstream key order.
fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let mut out = Map::new();
            for (k, v) in entries {
                out.insert(k.clone(), canonicalize(v));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize).collect()),
        other => other.clone(),
    }
}

// ---------------------------------------------------------------------------
// Serialization + atomic write
// ---------------------------------------------------------------------------

/// Render the inventory to its canonical string form: 2-space indent, LF endings,
/// trailing newline, no timestamps/absolute paths. Substance objects are already
/// canonicalized (sorted keys) during generation, so identical inputs render
/// byte-identically on every platform (Decision 7).
pub fn render(inventory: &ConstraintInventory) -> String {
    let mut out = serde_json::to_string_pretty(inventory)
        .unwrap_or_else(|e| unreachable!("inventory serialization is infallible: {e}"));
    out.push('\n');
    out
}

/// Atomically write the rendered inventory to `path` (temp file + rename), creating the
/// parent directory if needed. Never leaves a partial file (contracts/inventory-schema
/// §5). Mirrors `cache/disk.rs::save_index`.
pub fn write_inventory(path: &Path, inventory: &ConstraintInventory) -> std::io::Result<()> {
    let contents = render(inventory);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("constraints.json");

    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = parent.join(format!("{file_name}.tmp.{}.{}", std::process::id(), seq));

    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// generate/check comparison
// ---------------------------------------------------------------------------

/// A compact drift summary between a committed inventory and a fresh regeneration —
/// the `inventory check` mismatch report (contracts/cli-inventory.md).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InventoryDrift {
    /// IDs present in the regeneration but not the committed file.
    pub added: Vec<String>,
    /// IDs present in the committed file but not the regeneration.
    pub removed: Vec<String>,
    /// A shared `(document, pointer, kind)` location whose ID changed (substance
    /// drift): `(old id, new id)`.
    pub changed: Vec<(String, String)>,
}

impl InventoryDrift {
    /// Whether the two inventories are unit-identical (empty drift).
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

/// Compare a committed inventory against a regenerated one so a same-location substance
/// change reads as `changed` rather than an unrelated add/remove pair.
///
/// Delegates the matching to [`crate::diff::diff`] — the single implementation of
/// "which unit on the left corresponds to which on the right" — and flattens its richer
/// output into the compact id-only summary `inventory check` prints. `changed` and
/// `nonMaterial` both mean "same location, different substance", which is exactly this
/// summary's `changed`; the material/non-material split matters only to the human-facing
/// revision diff.
pub fn compare(
    committed: &ConstraintInventory,
    regenerated: &ConstraintInventory,
) -> InventoryDrift {
    let d = crate::diff::diff(committed, regenerated);

    let mut drift = InventoryDrift {
        added: d.added.iter().map(|e| e.id.clone()).collect(),
        removed: d.removed.iter().map(|e| e.id.clone()).collect(),
        changed: d
            .changed
            .iter()
            .chain(d.non_material.iter())
            .map(|c| (c.old_id.clone(), c.new_id.clone()))
            .collect(),
    };
    drift.added.sort();
    drift.removed.sort();
    drift.changed.sort();
    drift
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn slugify_uses_leaf_and_bounds_length() {
        assert_eq!(slugify_pointer(""), "root");
        assert_eq!(
            slugify_pointer("/definitions/devContainerCommon/properties/forwardPorts"),
            // trailing 48 chars, trimmed to a clean boundary — keeps the leaf.
            {
                let s = slugify_pointer("/definitions/devContainerCommon/properties/forwardPorts");
                assert!(s.ends_with("forwardports"), "slug keeps the leaf: {s}");
                assert!(s.len() <= SLUG_MAX, "slug bounded: {s}");
                s
            }
        );
        assert_eq!(slugify_pointer("/oneOf/2"), "oneof-2");
    }

    #[test]
    fn id_is_stable_and_substance_sensitive() {
        let a = derive_id(
            "base",
            "/x",
            ConstraintKind::Type,
            &canonicalize(&json!({ "type": "array" })),
        );
        let b = derive_id(
            "base",
            "/x",
            ConstraintKind::Type,
            &canonicalize(&json!({ "type": "array" })),
        );
        assert_eq!(a, b, "same inputs → same id");
        // Substance change moves the hash (drift-forcing).
        let c = derive_id(
            "base",
            "/x",
            ConstraintKind::Type,
            &canonicalize(&json!({ "type": "string" })),
        );
        assert_ne!(a, c);
        // Kind change moves the hash even at the same pointer/substance shape.
        let d = derive_id(
            "base",
            "/x",
            ConstraintKind::Enum,
            &canonicalize(&json!({ "type": "array" })),
        );
        assert_ne!(a, d);
        assert!(a.starts_with("cst-base-x-type-"));
        assert_eq!(
            a.rsplit('-').next().unwrap().len(),
            8,
            "hash8 is 8 hex chars"
        );
    }

    #[test]
    fn canonicalize_sorts_object_keys_but_preserves_arrays() {
        let v = json!({ "b": 1, "a": { "z": 2, "y": 3 }, "list": [3, 1, 2] });
        let c = canonicalize(&v);
        let s = serde_json::to_string(&c).unwrap();
        assert_eq!(s, r#"{"a":{"y":3,"z":2},"b":1,"list":[3,1,2]}"#);
    }

    #[test]
    fn compare_classifies_added_removed_changed() {
        let unit = |id: &str, ptr: &str, sub: Value| ConstraintUnit {
            id: id.into(),
            document: "base".into(),
            pointer: ptr.into(),
            kind: ConstraintKind::Type,
            substance: sub,
            context: None,
        };
        let committed = ConstraintInventory {
            schema_version: 1,
            revision: "rev-schema-x".into(),
            units: vec![
                unit(
                    "cst-base-a-type-11111111",
                    "/a",
                    json!({ "type": "string" }),
                ),
                unit(
                    "cst-base-b-type-22222222",
                    "/b",
                    json!({ "type": "string" }),
                ),
            ],
        };
        let regenerated = ConstraintInventory {
            schema_version: 1,
            revision: "rev-schema-x".into(),
            units: vec![
                // /a unchanged, /b substance changed (new id), /c added, /b's removal
                // via the changed pairing (not removed).
                unit(
                    "cst-base-a-type-11111111",
                    "/a",
                    json!({ "type": "string" }),
                ),
                unit(
                    "cst-base-b-type-33333333",
                    "/b",
                    json!({ "type": "integer" }),
                ),
                unit(
                    "cst-base-c-type-44444444",
                    "/c",
                    json!({ "type": "string" }),
                ),
            ],
        };
        let drift = compare(&committed, &regenerated);
        assert_eq!(drift.added, vec!["cst-base-c-type-44444444"]);
        assert!(drift.removed.is_empty());
        assert_eq!(
            drift.changed,
            vec![(
                "cst-base-b-type-22222222".to_string(),
                "cst-base-b-type-33333333".to_string()
            )]
        );
        assert!(!drift.is_empty());
        assert!(compare(&committed, &committed).is_empty());
    }
}
