//! Determinism + committed-artifact fidelity (T017, spec FR-023 / SC-002).
//!
//! Regenerating the inventory from the vendored, pinned schemas must be byte-identical
//! to the committed `conformance/inventory/constraints.json` and byte-identical across
//! repeated in-memory runs, with stable IDs. This is the hermetic twin of the
//! `inventory check` CLI. No Docker, no network.

use deacon_conformance::inventory::{generate_inventory, render};
use deacon_conformance::{default_inventory_file, default_pinned_schemas_dir};

#[test]
fn regeneration_matches_the_committed_inventory_byte_for_byte() {
    let regenerated = generate_inventory(&default_pinned_schemas_dir())
        .expect("vendored schemas extract cleanly");
    let rendered = render(&regenerated);

    let committed = std::fs::read_to_string(default_inventory_file())
        .expect("committed inventory exists (run `inventory generate` and commit it)");

    assert_eq!(
        rendered, committed,
        "regeneration differs from the committed inventory — run \
         `cargo run -p deacon-conformance -- inventory generate` and commit the result"
    );
    assert!(
        committed.ends_with('\n'),
        "committed inventory must be newline-terminated"
    );
}

#[test]
fn two_in_memory_regenerations_are_identical() {
    let a = render(&generate_inventory(&default_pinned_schemas_dir()).unwrap());
    let b = render(&generate_inventory(&default_pinned_schemas_dir()).unwrap());
    assert_eq!(
        a, b,
        "two regenerations from identical inputs must be byte-identical"
    );
}

#[test]
fn ids_are_stable_across_runs() {
    let a = generate_inventory(&default_pinned_schemas_dir()).unwrap();
    let b = generate_inventory(&default_pinned_schemas_dir()).unwrap();
    let ids_a: Vec<&String> = a.units.iter().map(|u| &u.id).collect();
    let ids_b: Vec<&String> = b.units.iter().map(|u| &u.id).collect();
    assert_eq!(
        ids_a, ids_b,
        "ids must be stable and in the same (sorted) order across runs"
    );

    // Units are sorted by id in the committed order.
    let mut sorted = ids_a.clone();
    sorted.sort();
    assert_eq!(ids_a, sorted, "committed units must be sorted by id");

    // Ids are unique (collision would have been a hard generation error).
    let unique: std::collections::HashSet<&&String> = ids_a.iter().collect();
    assert_eq!(unique.len(), ids_a.len(), "ids must be unique");
}
