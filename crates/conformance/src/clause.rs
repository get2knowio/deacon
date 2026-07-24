//! Clause canonicalization, substance-anchored IDs, and atomic write
//! (021-normative-clause-inventory, research Decisions 1–4; T014/T015).
//!
//! [`generate_clauses`] drives the CI-facing pipeline: load + fingerprint-verify the
//! spec manifest, parse each vendored prose document, then **canonicalize** the committed
//! (human-authored) clause records against that prose — recompute each clause's
//! normalized-substance fingerprint and substance-anchored ID, verify the excerpt is
//! present in the pinned document under its recorded heading, cross-check the strength
//! label against the excerpt's RFC-2119 keywords, merge records that share a normalized
//! substance into one unit with combined `locations`, and sort canonically. It invents no
//! clauses and invokes no model or network — segmentation is out-of-band authoring work
//! (research Decision 1). [`render`] serializes canonically and [`write_clauses`] commits
//! atomically (temp file + rename), mirroring [`crate::inventory`].
//!
//! ## Stable ID scheme (research Decision 2)
//!
//! `clu-<doc>-<substance-slug>-<strength>-<hash8>`:
//! - `<doc>` — manifest document key;
//! - `<substance-slug>` — a bounded readable slug of the normalized substance's LEADING
//!   tokens (identity is NOT carried here);
//! - `<strength>` — a short strength code;
//! - `<hash8>` — the first 8 lowercase-hex chars of SHA-256 over
//!   `document ‖ normalize_substance(excerpt)` joined by the `0x1f` unit separator.
//!   **Location is excluded**, so a pure move preserves the ID (and its disposition); a
//!   material change (obligation or strength reworded ⇒ different normalized substance)
//!   mints a new ID (drift-forcing).

use std::collections::HashMap;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::load::{LoadError, load_clause_inventory, load_spec_manifest};
use crate::model::{
    ClauseInventory, ClauseLocation, ClauseUnit, SpecManifest, Strength, Testability,
};
use crate::prose::Document;
use crate::prose::normalize::{fingerprint, normalize_substance};
use crate::prose::strength::{has_family, hides_mandatory_keyword};

/// The clause-inventory schema version (data-model.md §2).
const SCHEMA_VERSION: u32 = 1;

/// The fixed separator byte for the ID hash inputs (`0x1f`), matching 020's discipline —
/// it cannot appear in a document key or in normalized substance text.
const HASH_SEPARATOR: u8 = 0x1f;

/// The maximum substance-slug length (readability only — identity lives in the hash).
const SLUG_MAX: usize = 48;

// ---------------------------------------------------------------------------
// Generation pipeline
// ---------------------------------------------------------------------------

/// Canonicalize the committed clause records under `clauses_file` against the vendored,
/// fingerprint-verified prose under `spec_dir`. Fail-loud on every integrity error
/// (fingerprint mismatch, missing excerpt at anchor, strength/keyword contradiction,
/// inconsistent merge); never produces a partial result. An absent `clauses_file` is
/// treated as an empty authored set (an empty inventory under the manifest revision).
pub fn generate_clauses(
    spec_dir: &Path,
    clauses_file: &Path,
) -> Result<ClauseInventory, LoadError> {
    let manifest = load_spec_manifest(spec_dir)?;
    let documents = parse_documents(spec_dir, &manifest)?;

    let committed = load_clause_inventory(clauses_file)?;
    let raw: Vec<RawClause> = committed.as_ref().map(flatten).unwrap_or_default();

    let units = canonicalize(raw, &documents, &manifest)?;
    Ok(ClauseInventory {
        schema_version: SCHEMA_VERSION,
        revision: manifest.revision,
        units,
    })
}

/// Parse every vendored prose document named by the manifest into a [`Document`], keyed by
/// document key. The manifest fingerprints are already verified by [`load_spec_manifest`].
fn parse_documents(
    spec_dir: &Path,
    manifest: &SpecManifest,
) -> Result<HashMap<String, Document>, LoadError> {
    let mut docs = HashMap::with_capacity(manifest.documents.len());
    for doc in &manifest.documents {
        let path = spec_dir.join(&doc.file);
        let raw = std::fs::read_to_string(&path).map_err(|e| LoadError::MalformedSchema {
            document: doc.key.clone(),
            cause: format!("could not read vendored prose {path:?}: {e}"),
        })?;
        docs.insert(doc.key.clone(), Document::parse(&raw));
    }
    Ok(docs)
}

/// One authored clause location flattened out of a committed unit — the atomic input to
/// canonicalization. Strength/testability/context are inherited from the owning unit.
struct RawClause {
    document: String,
    strength: Strength,
    testability: Testability,
    context: Option<serde_json::Value>,
    location: ClauseLocation,
}

/// Flatten a committed inventory into per-location raw clauses (canonicalization regroups
/// them by derived ID, which is idempotent for an already-canonical file).
fn flatten(inventory: &ClauseInventory) -> Vec<RawClause> {
    let mut out = Vec::new();
    for unit in &inventory.units {
        for location in &unit.locations {
            out.push(RawClause {
                document: unit.document.clone(),
                strength: unit.strength,
                testability: unit.testability,
                context: unit.context.clone(),
                location: location.clone(),
            });
        }
    }
    out
}

/// Canonicalize raw clauses into sorted, merged [`ClauseUnit`]s with the fail-loud
/// integrity checks (T015). Returns every unit sorted by `id`; identical inputs produce
/// byte-identical output.
fn canonicalize(
    raws: Vec<RawClause>,
    documents: &HashMap<String, Document>,
    manifest: &SpecManifest,
) -> Result<Vec<ClauseUnit>, LoadError> {
    let manifest_keys: std::collections::HashSet<&str> =
        manifest.documents.iter().map(|d| d.key.as_str()).collect();

    // Accumulate by derived ID, merging locations.
    let mut by_id: HashMap<String, ClauseUnit> = HashMap::new();
    // Track (strength, testability, context) per id so an inconsistent merge is fail-loud.
    for raw in raws {
        // The document key must be a real manifest document.
        if !manifest_keys.contains(raw.document.as_str()) {
            return Err(LoadError::MalformedSchema {
                document: raw.document.clone(),
                cause: format!(
                    "clause references document key {:?}, absent from the spec manifest",
                    raw.document
                ),
            });
        }
        let doc = documents
            .get(&raw.document)
            .ok_or_else(|| LoadError::MalformedSchema {
                document: raw.document.clone(),
                cause: "no parsed prose for this document key".to_string(),
            })?;

        let normalized = normalize_substance(&raw.location.excerpt);
        let id = derive_clause_id(&raw.document, raw.strength, &normalized);

        // Integrity: strength ↔ excerpt keyword agreement (V15, Decision 4).
        check_strength(&id, raw.strength, &raw.location.excerpt)?;

        // Integrity: excerpt present in the pinned document under its recorded heading (V15).
        if !doc.contains_excerpt_at(&raw.location.anchor, &raw.location.excerpt) {
            return Err(LoadError::ExcerptNotFoundAtAnchor {
                clause: id,
                heading: raw.location.heading.clone(),
            });
        }

        let fp = fingerprint(&raw.location.excerpt);
        match by_id.get_mut(&id) {
            Some(existing) => {
                // A merge is only sound when the merged records agree on strength,
                // testability, and context (identical substance ⇒ identical obligation).
                if existing.testability != raw.testability || existing.context != raw.context {
                    return Err(LoadError::MalformedSchema {
                        document: raw.document.clone(),
                        cause: format!(
                            "clause id {id:?} merges records with conflicting testability/context; \
                             identical substance must carry an identical disposition"
                        ),
                    });
                }
                existing.locations.push(raw.location);
            }
            None => {
                by_id.insert(
                    id.clone(),
                    ClauseUnit {
                        id,
                        document: raw.document,
                        strength: raw.strength,
                        testability: raw.testability,
                        fingerprint: fp,
                        locations: vec![raw.location],
                        context: raw.context,
                    },
                );
            }
        }
    }

    let mut units: Vec<ClauseUnit> = by_id.into_values().collect();
    for unit in &mut units {
        // Locations sorted by (anchor, ordinal); dedup byte-identical duplicates.
        unit.locations
            .sort_by(|a, b| a.anchor.cmp(&b.anchor).then(a.ordinal.cmp(&b.ordinal)));
        unit.locations.dedup();
    }
    units.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(units)
}

/// The strength ↔ excerpt keyword cross-check (V15, research Decision 4): a
/// `must`/`should`/`may` clause MUST carry the corresponding keyword family; a
/// `descriptive` clause MUST NOT hide a mandatory keyword.
fn check_strength(id: &str, strength: Strength, excerpt: &str) -> Result<(), LoadError> {
    match strength {
        Strength::Must | Strength::Should | Strength::May => {
            if !has_family(excerpt, strength) {
                return Err(LoadError::StrengthKeywordMismatch {
                    clause: id.to_string(),
                    labeled: strength_wire(strength).to_string(),
                    detected: crate::prose::strength::detect_strength(excerpt)
                        .map(|s| strength_wire(s).to_string())
                        .unwrap_or_else(|| "none".to_string()),
                });
            }
        }
        Strength::Descriptive => {
            if hides_mandatory_keyword(excerpt) {
                return Err(LoadError::StrengthKeywordMismatch {
                    clause: id.to_string(),
                    labeled: "descriptive".to_string(),
                    detected: "must".to_string(),
                });
            }
        }
        // `algorithm` / `io-contract` carry no single-keyword contract.
        Strength::Algorithm | Strength::IoContract => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Stable ID derivation (research Decision 2)
// ---------------------------------------------------------------------------

/// Derive the substance-anchored `clu-…` id: `clu-<doc>-<substance-slug>-<strength>-<hash8>`.
/// `normalized` MUST be `normalize_substance(excerpt)`.
pub fn derive_clause_id(document: &str, strength: Strength, normalized: &str) -> String {
    let slug = slugify_leading(normalized);
    let code = strength_code(strength);
    let hash8 = hash8(document, normalized);
    format!("clu-{document}-{slug}-{code}-{hash8}")
}

/// First 8 lowercase-hex chars of SHA-256 over `document ‖ normalized` (location excluded).
fn hash8(document: &str, normalized: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(document.as_bytes());
    hasher.update([HASH_SEPARATOR]);
    hasher.update(normalized.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(8);
    for b in &digest[..4] {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// A short, stable strength code for the ID (single ID segment — no internal hyphen).
fn strength_code(strength: Strength) -> &'static str {
    match strength {
        Strength::Must => "must",
        Strength::Should => "should",
        Strength::May => "may",
        Strength::Algorithm => "algo",
        Strength::IoContract => "io",
        Strength::Descriptive => "desc",
    }
}

/// The wire spelling of a strength (for diagnostics).
fn strength_wire(strength: Strength) -> &'static str {
    match strength {
        Strength::Must => "must",
        Strength::Should => "should",
        Strength::May => "may",
        Strength::Algorithm => "algorithm",
        Strength::IoContract => "io-contract",
        Strength::Descriptive => "descriptive",
    }
}

/// Slugify the LEADING tokens of the normalized substance for readability: keep
/// `[a-z0-9]`, collapse everything else to `-`, bound to [`SLUG_MAX`] at a clean hyphen
/// boundary. The empty/keyword-free case slugs to `clause`.
fn slugify_leading(normalized: &str) -> String {
    let mut slug = String::with_capacity(normalized.len().min(SLUG_MAX + 8));
    let mut prev_dash = false;
    for ch in normalized.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
        if slug.trim_matches('-').len() >= SLUG_MAX {
            break;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        return "clause".to_string();
    }
    if slug.len() <= SLUG_MAX {
        return slug.to_string();
    }
    // Keep the leading SLUG_MAX chars, trimmed to a clean trailing hyphen boundary.
    let head = &slug[..SLUG_MAX];
    let trimmed = match head.rfind('-') {
        Some(i) if i > 0 => &head[..i],
        _ => head,
    };
    trimmed.trim_matches('-').to_string()
}

// ---------------------------------------------------------------------------
// Serialization + atomic write
// ---------------------------------------------------------------------------

/// Render the clause inventory to its canonical string form: 2-space indent, LF endings,
/// trailing newline. Identical inputs render byte-identically (research Decision 3).
pub fn render(inventory: &ClauseInventory) -> String {
    let mut out = serde_json::to_string_pretty(inventory)
        .unwrap_or_else(|e| unreachable!("clause inventory serialization is infallible: {e}"));
    out.push('\n');
    out
}

/// Atomically write the rendered inventory to `path` (temp file + rename), creating the
/// parent directory if needed. Never leaves a partial file (mirrors
/// `inventory::write_inventory` / `cache/disk.rs::save_index`).
pub fn write_clauses(path: &Path, inventory: &ClauseInventory) -> std::io::Result<()> {
    let contents = render(inventory);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("clauses.json");

    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = parent.join(format!("{file_name}.tmp.{}.{}", std::process::id(), seq));

    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn id_is_substance_anchored_and_location_independent() {
        let a = derive_clause_id(
            "reference",
            Strength::Must,
            &normalize_substance("MUST do X."),
        );
        // Whitespace/markdown reflow → same normalized substance → same id (a pure move
        // keeps identity).
        let b = derive_clause_id(
            "reference",
            Strength::Must,
            &normalize_substance("MUST   do **X**."),
        );
        assert_eq!(a, b, "immaterial reflow keeps the id");
        // A material substance change mints a new id.
        let c = derive_clause_id(
            "reference",
            Strength::Must,
            &normalize_substance("MUST do Y."),
        );
        assert_ne!(a, c);
        assert!(a.starts_with("clu-reference-"));
        assert!(a.contains("-must-"));
        assert_eq!(a.rsplit('-').next().unwrap().len(), 8, "hash8 is 8 chars");
    }

    #[test]
    fn slugify_leading_is_bounded_and_uses_the_start() {
        let s = slugify_leading(
            "the oncreatecommand command must be run only once during the container lifecycle",
        );
        assert!(s.len() <= SLUG_MAX, "slug bounded: {s} ({})", s.len());
        assert!(
            s.starts_with("the-oncreatecommand"),
            "slug uses the start: {s}"
        );
        assert_eq!(slugify_leading(""), "clause");
    }

    #[test]
    fn strength_codes_are_single_segments() {
        for s in [
            Strength::Must,
            Strength::Should,
            Strength::May,
            Strength::Algorithm,
            Strength::IoContract,
            Strength::Descriptive,
        ] {
            assert!(!strength_code(s).contains('-'), "code {s:?} has no hyphen");
        }
    }

    #[test]
    fn render_is_deterministic_and_newline_terminated() {
        let inv = ClauseInventory {
            schema_version: 1,
            revision: "rev-spec-113500f4".to_string(),
            units: vec![ClauseUnit {
                id: "clu-reference-x-must-00000000".to_string(),
                document: "reference".to_string(),
                strength: Strength::Must,
                testability: Testability::DirectlyTestable,
                fingerprint: "ab".to_string(),
                locations: vec![ClauseLocation {
                    heading: "H".to_string(),
                    anchor: "h".to_string(),
                    ordinal: 1,
                    excerpt: "MUST x".to_string(),
                }],
                context: Some(json!({ "inCodeFence": true })),
            }],
        };
        let a = render(&inv);
        let b = render(&inv);
        assert_eq!(a, b);
        assert!(a.ends_with('\n'));
    }
}
