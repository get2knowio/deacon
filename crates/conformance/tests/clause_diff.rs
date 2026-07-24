//! Acceptance tests for the deterministic, move-aware clause diff (US3, T030;
//! FR-019/FR-020/FR-021, SC-005/SC-007). Diffs the two fixture revisions under
//! `fixtures/conformance/clause-drift/{old,new}` and asserts every change lands in the
//! correct bucket, moves keep their id, reworded clauses mint a new id, and immaterial
//! reflow is non-material. Hermetic — no Docker, no network, no model.

use deacon_conformance::clause_diff::{diff, render_json, render_md};
use deacon_conformance::model::ClauseInventory;
use deacon_conformance::workspace_root;

fn load(rel: &str) -> ClauseInventory {
    let path = workspace_root()
        .join("fixtures/conformance/clause-drift")
        .join(rel);
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {path:?}: {e}"))
}

#[test]
fn buckets_are_exactly_as_expected() {
    let d = diff(&load("old/clauses.json"), &load("new/clauses.json"));
    assert_eq!(
        d.new_clauses
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        vec!["clu-doc-added-must-66666666"],
        "exactly one added clause"
    );
    assert_eq!(
        d.removed.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        vec!["clu-doc-removed-must-22222222"],
        "exactly one removed clause"
    );
    assert_eq!(
        d.moved.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        vec!["clu-doc-moved-must-11111111"],
        "exactly one moved clause"
    );
    assert_eq!(d.changed.len(), 1, "exactly one materially changed clause");
    assert_eq!(
        d.non_material
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        vec!["clu-doc-nm-must-55555555"],
        "exactly one immaterial reflow"
    );
}

#[test]
fn a_move_keeps_its_id_and_shows_both_locations() {
    let d = diff(&load("old/clauses.json"), &load("new/clauses.json"));
    let m = &d.moved[0];
    assert_eq!(m.id, "clu-doc-moved-must-11111111");
    assert_eq!(m.old_locations[0].anchor, "old-location");
    assert_eq!(m.new_locations[0].anchor, "new-location");
}

#[test]
fn a_reword_yields_a_new_id_and_a_stale_old_id() {
    let d = diff(&load("old/clauses.json"), &load("new/clauses.json"));
    let c = &d.changed[0];
    assert_eq!(c.old_id, "clu-doc-old-must-33333333");
    assert_eq!(c.new_id, "clu-doc-new-must-44444444");
    assert_ne!(
        c.old_id, c.new_id,
        "a material change must carry distinct ids"
    );
    // No disposition is inherited by wording similarity: the old id is simply gone.
    assert!(!d.removed.iter().any(|e| e.id == c.new_id));
}

#[test]
fn immaterial_reflow_is_not_reported_as_material() {
    let d = diff(&load("old/clauses.json"), &load("new/clauses.json"));
    assert!(
        d.changed
            .iter()
            .all(|c| c.new_id != "clu-doc-nm-must-55555555"),
        "a reflow must never appear as a material change"
    );
}

#[test]
fn output_is_deterministic_in_both_forms() {
    let old = load("old/clauses.json");
    let new = load("new/clauses.json");
    assert_eq!(
        render_json(&diff(&old, &new)),
        render_json(&diff(&old, &new))
    );
    let md = render_md(&diff(&old, &new));
    assert_eq!(md, render_md(&diff(&old, &new)));
    assert!(md.contains("# Clause Inventory Diff"));
}

#[test]
fn identical_inventories_diff_to_nothing() {
    let old = load("old/clauses.json");
    assert!(diff(&old, &old).is_empty());
}
