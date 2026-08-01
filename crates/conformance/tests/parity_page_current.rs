//! The published parity page must match a fresh render of the registry.
//!
//! `docs/PARITY.md` drifted twice within an hour of first publication: once because its
//! counts were taken from a working tree holding unmerged records, once because a PR
//! merged behind it. Both times the page claimed numbers the registry did not have,
//! while asserting in its own header that every claim traces to a committed record.
//!
//! Hand-maintenance cannot survive a repo where records land several times a day, so the
//! page is generated and this test is the gate. Hermetic — no Docker, no network, no
//! oracle — so it runs in the fast lane on every PR.

use deacon_conformance::load::Registry;
use deacon_conformance::{default_registry_dir, workspace_root};

#[test]
fn published_parity_page_matches_a_fresh_render() {
    let registry = Registry::load(&default_registry_dir()).expect("registry loads");
    let rendered = deacon_conformance::parity_page::render(&registry);

    let path = workspace_root().join("docs/PARITY.md");
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{} is unreadable ({e}). It is generated: \
             `cargo run -p deacon-conformance -- parity-page --write`",
            path.display()
        )
    });

    assert_eq!(
        committed, rendered,
        "docs/PARITY.md is stale — regenerate it with \
         `cargo run -p deacon-conformance -- parity-page --write`. \
         It is a GENERATED file: adding a behavior, a case, or a waiver changes it, and a \
         page that disagrees with the registry is worse than no page, because its header \
         promises that every claim traces to a committed record."
    );
}

/// The render must not depend on anything outside the registry — same input, same bytes.
#[test]
fn rendering_is_deterministic() {
    let registry = Registry::load(&default_registry_dir()).expect("registry loads");
    let a = deacon_conformance::parity_page::render(&registry);
    let b = deacon_conformance::parity_page::render(&registry);
    assert_eq!(a, b, "the page render is not deterministic");
}
