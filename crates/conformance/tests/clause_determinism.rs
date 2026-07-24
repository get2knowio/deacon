//! Determinism acceptance tests for the clause inventory (US1, T012; FR-011/FR-023,
//! SC-002). Canonicalization is byte-identical across runs, the committed inventory
//! byte-matches a fresh regeneration, IDs are stable, and no vendored/fixture prose
//! carries a CR byte. Hermetic — no Docker, no network, no model.

use deacon_conformance::clause::{generate_clauses, render};
use deacon_conformance::{default_clauses_file, default_pinned_spec_dir, workspace_root};

#[test]
fn committed_pinned_inventory_byte_matches_regeneration() {
    let spec = default_pinned_spec_dir();
    let clauses = default_clauses_file();
    let regenerated = generate_clauses(&spec, &clauses).expect("pinned clauses canonicalize");
    let committed = std::fs::read_to_string(&clauses).expect("committed clauses.json is readable");
    assert_eq!(
        committed,
        render(&regenerated),
        "committed clauses.json must byte-match a fresh `clause generate`"
    );
}

#[test]
fn two_regenerations_are_byte_identical() {
    let spec = default_pinned_spec_dir();
    let clauses = default_clauses_file();
    let a = render(&generate_clauses(&spec, &clauses).unwrap());
    let b = render(&generate_clauses(&spec, &clauses).unwrap());
    assert_eq!(a, b, "two in-memory regenerations must be byte-identical");
    assert!(a.ends_with('\n'), "canonical output is newline-terminated");
}

#[test]
fn ids_are_stable_across_runs() {
    let spec = default_pinned_spec_dir();
    let clauses = default_clauses_file();
    let ids1: Vec<String> = generate_clauses(&spec, &clauses)
        .unwrap()
        .units
        .into_iter()
        .map(|u| u.id)
        .collect();
    let ids2: Vec<String> = generate_clauses(&spec, &clauses)
        .unwrap()
        .units
        .into_iter()
        .map(|u| u.id)
        .collect();
    assert_eq!(
        ids1, ids2,
        "clause ids must be stable and in the same order"
    );
}

#[test]
fn fixture_inventory_regenerates_byte_identically() {
    let base = workspace_root().join("fixtures/conformance/prose");
    let clauses = base.join("clauses.json");
    let regenerated = generate_clauses(&base, &clauses).unwrap();
    let committed = std::fs::read_to_string(&clauses).unwrap();
    assert_eq!(committed, render(&regenerated));
}

#[test]
fn vendored_and_fixture_prose_have_no_cr_bytes() {
    // CR bytes would make cross-platform output non-byte-identical; the loader rejects
    // them, but assert it here directly over every vendored + fixture Markdown file.
    let roots = [
        default_pinned_spec_dir(),
        workspace_root().join("fixtures/conformance/prose"),
    ];
    for root in roots {
        for entry in std::fs::read_dir(&root)
            .expect("prose dir readable")
            .flatten()
        {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                let bytes = std::fs::read(&path).unwrap();
                assert!(
                    !bytes.contains(&b'\r'),
                    "{path:?} contains a CR byte; vendored/fixture prose must be LF-only"
                );
            }
        }
    }
}

#[test]
fn crate_has_no_network_or_llm_dependency_so_ci_paths_are_offline() {
    // SC-004 / T037: `generate`/`check`/`validate`/`diff`/`certify` read ONLY committed +
    // vendored inputs — no command can perform network IO or invoke a model, because the
    // dev-only `deacon-conformance` crate declares no HTTP client / model dependency at
    // all. Assert that structurally over its manifest (the strongest offline guarantee:
    // the capability is absent, not merely unused).
    let manifest = std::fs::read_to_string(workspace_root().join("crates/conformance/Cargo.toml"))
        .expect("conformance Cargo.toml is readable");
    // Inspect only dependency-declaration lines (`name = …`), so a word like "surface"
    // in a comment never trips the check — the guarantee is about actual dependencies.
    let dep_names: Vec<String> = manifest
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('=').map(|(name, _)| name.trim().to_string()))
        .collect();
    for forbidden in [
        "reqwest",
        "hyper",
        "ureq",
        "curl",
        "surf",
        "isahc",
        "tokio",
        "openai",
        "async-openai",
        "llm",
    ] {
        assert!(
            !dep_names.iter().any(|d| d == forbidden),
            "deacon-conformance must not depend on {forbidden:?}: its CI-facing clause \
             commands must be offline and LLM-free (SC-004)"
        );
    }
}
