//! Deterministic revision diff between two clause inventories
//! (021-normative-clause-inventory, research Decision 9 / data-model.md §4).
//!
//! Unlike 020's `(document, pointer, kind)` match key, [`diff`] matches units on their
//! **substance-anchored ID** (⇔ the normalized-substance fingerprint, location excluded),
//! which is what makes **moves first-class**: a clause whose substance is unchanged but
//! whose heading moved keeps its ID and is reported as `moved` (old/new locations) —
//! non-blocking, disposition preserved. A unit present only on the right is `new`; only on
//! the left is `removed`. A material rewrite mints a new ID, so it surfaces as a `removed`
//! old-ID + a `new` new-ID sharing a heading — paired into `changed` for the reviewer. A
//! same-ID pair whose excerpt bytes differ at the same heading (whitespace/reflow) is
//! `nonMaterial`. Every bucket is deterministically sorted by `(document, heading, id)`;
//! both renderers mirror `diff.rs`'s byte-stable discipline.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Serialize;

use crate::model::{ClauseInventory, ClauseLocation, ClauseUnit, Strength};

/// The revision-diff schema version.
const SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Output model (camelCase on the wire)
// ---------------------------------------------------------------------------

/// The complete `clause diff` document (data-model.md §4). Command-only output, never
/// committed; deterministic for identical inputs.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClauseRevisionDiff {
    pub schema_version: u32,
    pub old: DiffSide,
    pub new: DiffSide,
    /// Clause IDs present only in `new`.
    pub new_clauses: Vec<ClauseEntry>,
    /// Clause IDs present only in `old` (that were not paired into `changed`).
    pub removed: Vec<ClauseEntry>,
    /// Same-ID pairs whose locations moved (heading changed) — disposition preserved.
    pub moved: Vec<MovedEntry>,
    /// A removed old-ID + a new new-ID sharing a heading (a material rewrite).
    pub changed: Vec<ChangedEntry>,
    /// Same-ID pairs whose excerpt bytes differ but the substance is unchanged.
    pub non_material: Vec<NonMaterialEntry>,
}

/// One side's revision identity.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiffSide {
    pub revision: String,
}

/// An added-or-removed clause — the full unit shape.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClauseEntry {
    pub id: String,
    pub document: String,
    pub strength: Strength,
    pub locations: Vec<ClauseLocation>,
}

/// A moved clause: same ID, different locations (old/new shown).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MovedEntry {
    pub id: String,
    pub document: String,
    pub old_locations: Vec<ClauseLocation>,
    pub new_locations: Vec<ClauseLocation>,
}

/// A material rewrite: a removed old-ID + a new new-ID sharing a heading.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedEntry {
    pub document: String,
    pub heading: String,
    pub old_id: String,
    pub new_id: String,
    pub old_excerpt: String,
    pub new_excerpt: String,
}

/// A same-ID immaterial excerpt change (whitespace/reflow at the same heading).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NonMaterialEntry {
    pub id: String,
    pub old_excerpt: String,
    pub new_excerpt: String,
}

// ---------------------------------------------------------------------------
// Diffing
// ---------------------------------------------------------------------------

/// Compute the deterministic diff of `new` against `old` (data-model.md §4).
pub fn diff(old: &ClauseInventory, new: &ClauseInventory) -> ClauseRevisionDiff {
    let old_by_id: BTreeMap<&str, &ClauseUnit> =
        old.units.iter().map(|u| (u.id.as_str(), u)).collect();
    let new_by_id: BTreeMap<&str, &ClauseUnit> =
        new.units.iter().map(|u| (u.id.as_str(), u)).collect();

    let mut new_clauses: Vec<ClauseEntry> = Vec::new();
    let mut removed: Vec<ClauseEntry> = Vec::new();
    let mut moved: Vec<MovedEntry> = Vec::new();
    let mut non_material: Vec<NonMaterialEntry> = Vec::new();

    // IDs in both: unchanged / moved / non-material.
    for (id, nu) in &new_by_id {
        let Some(ou) = old_by_id.get(id) else {
            new_clauses.push(clause_entry(nu));
            continue;
        };
        let old_anchors = anchor_set(ou);
        let new_anchors = anchor_set(nu);
        if old_anchors != new_anchors {
            moved.push(MovedEntry {
                id: (*id).to_string(),
                document: nu.document.clone(),
                old_locations: ou.locations.clone(),
                new_locations: nu.locations.clone(),
            });
        } else if ou.locations != nu.locations {
            // Same headings, excerpt bytes changed — immaterial (substance unchanged).
            non_material.push(NonMaterialEntry {
                id: (*id).to_string(),
                old_excerpt: first_excerpt(ou),
                new_excerpt: first_excerpt(nu),
            });
        }
        // else: fully unchanged.
    }

    // IDs only in old are removals (some pair into `changed` below).
    for (id, ou) in &old_by_id {
        if !new_by_id.contains_key(id) {
            removed.push(clause_entry(ou));
        }
    }

    // Pair removed old-IDs and new new-IDs that share a heading anchor into `changed`.
    let changed = pair_changed(&mut removed, &mut new_clauses);

    // Deterministic ordering.
    new_clauses.sort_by(clause_sort_key);
    removed.sort_by(clause_sort_key);
    moved.sort_by(|a, b| (&a.document, &a.id).cmp(&(&b.document, &b.id)));
    non_material.sort_by(|a, b| a.id.cmp(&b.id));

    ClauseRevisionDiff {
        schema_version: SCHEMA_VERSION,
        old: DiffSide {
            revision: old.revision.clone(),
        },
        new: DiffSide {
            revision: new.revision.clone(),
        },
        new_clauses,
        removed,
        moved,
        changed,
        non_material,
    }
}

impl ClauseRevisionDiff {
    /// Whether the two inventories are clause-identical (every bucket empty).
    pub fn is_empty(&self) -> bool {
        self.new_clauses.is_empty()
            && self.removed.is_empty()
            && self.moved.is_empty()
            && self.changed.is_empty()
            && self.non_material.is_empty()
    }
}

/// Pair a removed old clause with a new clause under the same leading heading anchor into a
/// `changed` entry (a material rewrite reads as "changed" for the reviewer). A pairing is
/// made ONLY when an anchor carries exactly one removed and one added clause — an
/// unambiguous 1:1 rewrite. When an anchor has more than one removed or added clause we
/// cannot tell which removal corresponds to which addition, so those clauses are left in
/// the `removed`/`new_clauses` buckets rather than paired arbitrarily: an arbitrary pairing
/// would both fabricate a spurious "change" and hide the genuine add/remove of unrelated
/// clauses that merely share a heading. Paired entries are drained from
/// `removed`/`new_clauses`; unpaired ones stay. Deterministic.
fn pair_changed(
    removed: &mut Vec<ClauseEntry>,
    new_clauses: &mut Vec<ClauseEntry>,
) -> Vec<ChangedEntry> {
    use std::collections::BTreeMap;

    // Group indices by leading anchor.
    let mut rem_by_anchor: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, e) in removed.iter().enumerate() {
        rem_by_anchor
            .entry(leading_anchor(e).to_string())
            .or_default()
            .push(i);
    }
    let mut new_by_anchor: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, e) in new_clauses.iter().enumerate() {
        new_by_anchor
            .entry(leading_anchor(e).to_string())
            .or_default()
            .push(i);
    }

    let mut changed: Vec<ChangedEntry> = Vec::new();
    let mut removed_taken: Vec<bool> = vec![false; removed.len()];
    let mut new_taken: Vec<bool> = vec![false; new_clauses.len()];

    for (anchor, rem_idx) in &rem_by_anchor {
        let Some(new_idx) = new_by_anchor.get(anchor) else {
            continue;
        };
        // Only an unambiguous 1:1 rewrite pairs. With more than one removed or added clause
        // under this anchor the correspondence is ambiguous, so leave every one of them as a
        // distinct removed/new entry (never mispaired, never hidden).
        if rem_idx.len() != 1 || new_idx.len() != 1 {
            continue;
        }
        let ri = rem_idx[0];
        let ni = new_idx[0];
        let old_e = &removed[ri];
        let new_e = &new_clauses[ni];
        changed.push(ChangedEntry {
            document: new_e.document.clone(),
            heading: heading_for_anchor(new_e, anchor),
            old_id: old_e.id.clone(),
            new_id: new_e.id.clone(),
            old_excerpt: excerpt_for_anchor(old_e, anchor),
            new_excerpt: excerpt_for_anchor(new_e, anchor),
        });
        removed_taken[ri] = true;
        new_taken[ni] = true;
    }

    // Drain paired entries.
    let mut i = 0;
    removed.retain(|_| {
        let keep = !removed_taken[i];
        i += 1;
        keep
    });
    let mut j = 0;
    new_clauses.retain(|_| {
        let keep = !new_taken[j];
        j += 1;
        keep
    });

    changed.sort_by(|a, b| {
        (&a.document, &a.heading, &a.new_id).cmp(&(&b.document, &b.heading, &b.new_id))
    });
    changed
}

fn clause_entry(unit: &ClauseUnit) -> ClauseEntry {
    ClauseEntry {
        id: unit.id.clone(),
        document: unit.document.clone(),
        strength: unit.strength,
        locations: unit.locations.clone(),
    }
}

fn anchor_set(unit: &ClauseUnit) -> std::collections::BTreeSet<&str> {
    unit.locations.iter().map(|l| l.anchor.as_str()).collect()
}

fn first_excerpt(unit: &ClauseUnit) -> String {
    unit.locations
        .first()
        .map(|l| l.excerpt.clone())
        .unwrap_or_default()
}

fn leading_anchor(e: &ClauseEntry) -> &str {
    e.locations.first().map(|l| l.anchor.as_str()).unwrap_or("")
}

fn heading_for_anchor(e: &ClauseEntry, anchor: &str) -> String {
    e.locations
        .iter()
        .find(|l| l.anchor == anchor)
        .map(|l| l.heading.clone())
        .unwrap_or_default()
}

fn excerpt_for_anchor(e: &ClauseEntry, anchor: &str) -> String {
    e.locations
        .iter()
        .find(|l| l.anchor == anchor)
        .map(|l| l.excerpt.clone())
        .unwrap_or_default()
}

fn clause_sort_key(a: &ClauseEntry, b: &ClauseEntry) -> std::cmp::Ordering {
    (&a.document, leading_anchor(a), &a.id).cmp(&(&b.document, leading_anchor(b), &b.id))
}

// ---------------------------------------------------------------------------
// Serialization — JSON + Markdown
// ---------------------------------------------------------------------------

/// Render the diff to its canonical JSON string (2-space indent, newline-terminated).
pub fn render_json(diff: &ClauseRevisionDiff) -> String {
    let mut out = serde_json::to_string_pretty(diff)
        .unwrap_or_else(|e| unreachable!("clause diff serialization is infallible: {e}"));
    out.push('\n');
    out
}

/// Render the diff to a human-review Markdown document (deterministic, LF, trailing
/// newline).
pub fn render_md(diff: &ClauseRevisionDiff) -> String {
    let mut md = String::new();
    md.push_str("# Clause Inventory Diff\n\n");
    let _ = writeln!(md, "**Old revision:** `{}`", diff.old.revision);
    let _ = writeln!(md, "**New revision:** `{}`\n", diff.new.revision);

    md.push_str("## Summary\n\n");
    md.push_str("| Bucket | Count |\n|--------|-------|\n");
    let _ = writeln!(md, "| New | {} |", diff.new_clauses.len());
    let _ = writeln!(md, "| Removed | {} |", diff.removed.len());
    let _ = writeln!(md, "| Moved | {} |", diff.moved.len());
    let _ = writeln!(md, "| Changed | {} |", diff.changed.len());
    let _ = writeln!(md, "| Non-material | {} |", diff.non_material.len());

    render_clause_section(&mut md, "New", &diff.new_clauses);
    render_clause_section(&mut md, "Removed", &diff.removed);

    let _ = write!(md, "\n## Moved\n\n");
    if diff.moved.is_empty() {
        md.push_str("None.\n");
    } else {
        md.push_str("| ID | Document | Old heading | New heading |\n");
        md.push_str("|----|----------|-------------|-------------|\n");
        for m in &diff.moved {
            let _ = writeln!(
                md,
                "| `{}` | `{}` | {} | {} |",
                m.id,
                m.document,
                headings_of(&m.old_locations),
                headings_of(&m.new_locations),
            );
        }
    }

    let _ = write!(md, "\n## Changed\n\n");
    if diff.changed.is_empty() {
        md.push_str("None.\n");
    } else {
        md.push_str("| Document | Heading | Old ID | New ID |\n");
        md.push_str("|----------|---------|--------|--------|\n");
        for c in &diff.changed {
            let _ = writeln!(
                md,
                "| `{}` | {} | `{}` | `{}` |",
                c.document, c.heading, c.old_id, c.new_id
            );
        }
    }

    let _ = write!(md, "\n## Non-material\n\n");
    if diff.non_material.is_empty() {
        md.push_str("None.\n");
    } else {
        for n in &diff.non_material {
            let _ = writeln!(md, "- `{}`", n.id);
        }
    }

    md
}

fn render_clause_section(md: &mut String, title: &str, entries: &[ClauseEntry]) {
    let _ = write!(md, "\n## {title}\n\n");
    if entries.is_empty() {
        md.push_str("None.\n");
        return;
    }
    md.push_str("| ID | Document | Strength | Heading |\n");
    md.push_str("|----|----------|----------|---------|\n");
    for e in entries {
        let _ = writeln!(
            md,
            "| `{}` | `{}` | `{}` | {} |",
            e.id,
            e.document,
            strength_wire(e.strength),
            headings_of(&e.locations),
        );
    }
}

fn headings_of(locations: &[ClauseLocation]) -> String {
    locations
        .iter()
        .map(|l| l.heading.clone())
        .collect::<Vec<_>>()
        .join("; ")
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Testability;

    fn unit(id: &str, anchor: &str, excerpt: &str) -> ClauseUnit {
        ClauseUnit {
            id: id.to_string(),
            document: "reference".to_string(),
            strength: Strength::Must,
            testability: Testability::DirectlyTestable,
            fingerprint: "fp".to_string(),
            locations: vec![ClauseLocation {
                heading: anchor.to_string(),
                anchor: anchor.to_string(),
                ordinal: 1,
                excerpt: excerpt.to_string(),
            }],
            context: None,
        }
    }

    fn inv(rev: &str, units: Vec<ClauseUnit>) -> ClauseInventory {
        ClauseInventory {
            schema_version: 1,
            revision: rev.to_string(),
            units,
        }
    }

    #[test]
    fn pure_add_and_remove() {
        let old = inv(
            "rev-spec-a",
            vec![unit("clu-a-x-must-11111111", "h1", "MUST a")],
        );
        let new = inv(
            "rev-spec-b",
            vec![unit("clu-a-y-must-22222222", "h2", "MUST b")],
        );
        // Different anchors → not paired into changed.
        let d = diff(&old, &new);
        assert_eq!(d.new_clauses.len(), 1);
        assert_eq!(d.removed.len(), 1);
        assert!(d.changed.is_empty() && d.moved.is_empty());
    }

    #[test]
    fn moved_keeps_id_and_is_not_changed() {
        let mut old_u = unit("clu-a-x-must-11111111", "old-heading", "MUST a");
        let mut new_u = old_u.clone();
        new_u.locations[0].heading = "new-heading".to_string();
        new_u.locations[0].anchor = "new-heading".to_string();
        old_u.locations[0].anchor = "old-heading".to_string();
        let d = diff(&inv("r-a", vec![old_u]), &inv("r-b", vec![new_u]));
        assert_eq!(d.moved.len(), 1, "same id, different anchor → moved: {d:?}");
        assert_eq!(d.moved[0].id, "clu-a-x-must-11111111");
        assert!(d.new_clauses.is_empty() && d.removed.is_empty() && d.changed.is_empty());
    }

    #[test]
    fn reworded_at_same_heading_is_changed_with_new_id() {
        let old = inv(
            "r-a",
            vec![unit("clu-a-x-must-11111111", "same", "MUST do X")],
        );
        let new = inv(
            "r-b",
            vec![unit("clu-a-x-must-99999999", "same", "MUST do Y")],
        );
        let d = diff(&old, &new);
        assert_eq!(
            d.changed.len(),
            1,
            "shared heading pairs into changed: {d:?}"
        );
        assert_eq!(d.changed[0].old_id, "clu-a-x-must-11111111");
        assert_eq!(d.changed[0].new_id, "clu-a-x-must-99999999");
        assert!(d.new_clauses.is_empty() && d.removed.is_empty());
    }

    #[test]
    fn ambiguous_multi_clause_heading_is_not_mispaired() {
        // Under a single heading anchor 'same', two clauses are removed and one UNRELATED
        // clause is added. The correspondence is ambiguous (2 vs 1), so nothing is paired
        // into `changed`: both genuine removals and the genuine addition are reported
        // truthfully instead of fabricating a rewrite and hiding an add/remove.
        let old = inv(
            "r-a",
            vec![
                unit("clu-a-x-must-11111111", "same", "MUST do X"),
                unit("clu-a-z-must-33333333", "same", "MUST do Z"),
            ],
        );
        let new = inv(
            "r-b",
            vec![unit("clu-a-y-must-22222222", "same", "MUST do Y")],
        );
        let d = diff(&old, &new);
        assert!(
            d.changed.is_empty(),
            "ambiguous multi-clause anchor must not pair: {d:?}"
        );
        assert_eq!(d.removed.len(), 2, "both removals reported: {d:?}");
        assert_eq!(d.new_clauses.len(), 1, "the addition reported: {d:?}");
        assert!(d.moved.is_empty() && d.non_material.is_empty());
    }

    #[test]
    fn immaterial_excerpt_change_at_same_heading_is_non_material() {
        let old = inv(
            "r-a",
            vec![unit("clu-a-x-must-11111111", "same", "MUST do X")],
        );
        // Same id (substance unchanged) but excerpt bytes differ.
        let mut new_u = unit("clu-a-x-must-11111111", "same", "MUST  do X");
        new_u.locations[0].excerpt = "MUST  do X".to_string();
        let d = diff(&old, &inv("r-b", vec![new_u]));
        assert_eq!(d.non_material.len(), 1, "reflow → nonMaterial: {d:?}");
        assert!(d.changed.is_empty() && d.moved.is_empty());
    }

    #[test]
    fn empty_diff_and_determinism() {
        let i = inv("r-a", vec![unit("clu-a-x-must-11111111", "h", "MUST a")]);
        let d = diff(&i, &i);
        assert!(d.is_empty());
        let j1 = render_json(&diff(&i, &i));
        let j2 = render_json(&diff(&i, &i));
        assert_eq!(j1, j2);
        assert!(j1.ends_with('\n'));
        assert!(render_md(&diff(&i, &i)).contains("# Clause Inventory Diff"));
    }
}
