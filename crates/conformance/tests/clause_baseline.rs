//! Pinned-baseline acceptance tests for the committed clause inventory (US1, T013;
//! FR-028, SC-001). Asserts observable facts about the REAL extracted inventory of the
//! pinned `113500f4` prose — specific well-known clauses with their expected strength and
//! provenance, plus per-document coverage — so a regression in extraction/canonicalization
//! logic surfaces against real inputs, not only fixtures. Hermetic — no Docker, no
//! network, no model.

use std::collections::BTreeSet;

use deacon_conformance::model::{ClauseUnit, Strength};
use deacon_conformance::{default_clauses_file, default_pinned_spec_dir};

fn units() -> Vec<ClauseUnit> {
    deacon_conformance::clause::generate_clauses(
        &default_pinned_spec_dir(),
        &default_clauses_file(),
    )
    .expect("committed clause inventory canonicalizes")
    .units
}

fn find<'a>(units: &'a [ClauseUnit], document: &str, needle: &str) -> &'a ClauseUnit {
    units
        .iter()
        .find(|u| u.document == document && u.locations.iter().any(|l| l.excerpt.contains(needle)))
        .unwrap_or_else(|| panic!("no {document} clause with excerpt containing {needle:?}"))
}

#[test]
fn all_eighteen_pinned_documents_are_represented() {
    let units = units();
    let docs: BTreeSet<&str> = units.iter().map(|u| u.document.as_str()).collect();
    let expected = [
        "reference",
        "json-reference",
        "supporting-tools",
        "image-metadata",
        "lockfile",
        "devcontainer-id-variable",
        "parallel-lifecycle",
        "features-lifecycle-scripts",
        "features-user-env",
        "feature-dependencies",
        "gpu-host-requirement",
        "declarative-secrets",
        "secrets-support",
        "features-legacy-ids",
        "features",
        "features-distribution",
        "templates",
        "templates-distribution",
    ];
    for doc in expected {
        assert!(
            docs.contains(doc),
            "document {doc:?} is missing from the inventory"
        );
    }
    assert_eq!(
        docs.len(),
        18,
        "exactly the 18 ratified docs/specs documents"
    );
}

#[test]
fn the_feature_dependencies_install_order_must_is_present_with_must_strength() {
    // The one genuine uppercase RFC-2119 MUST in the Features authoring doc's dependency
    // discussion: `myfeature` MUST be installed after its declared dependencies.
    let units = units();
    let clause = find(&units, "features", "MUST be installed after");
    assert_eq!(
        clause.strength,
        Strength::Must,
        "an uppercase MUST is `must` strength"
    );
    assert!(
        clause.id.contains("-must-"),
        "the id encodes the strength code: {}",
        clause.id
    );
}

#[test]
fn the_devcontainer_id_hash_algorithm_is_recorded_as_an_algorithm() {
    // `${devcontainerId}` is computed by a defined SHA-256 → base-32 procedure.
    let units = units();
    let clause = find(&units, "devcontainer-id-variable", "SHA-256 hash");
    assert_eq!(clause.strength, Strength::Algorithm);
    assert_eq!(clause.locations[0].anchor, "label-based-computation");
}

#[test]
fn the_image_metadata_label_shape_is_recorded_as_an_io_contract() {
    // The `devcontainer.metadata` image label carries an array/object value contract.
    let units = units();
    let clause = find(&units, "image-metadata", "devcontainer.metadata");
    assert!(
        matches!(
            clause.strength,
            Strength::IoContract | Strength::Descriptive | Strength::Algorithm
        ),
        "the metadata label clause is a source-shape/merge clause, got {:?}",
        clause.strength
    );
}

#[test]
fn every_clause_carries_verifiable_provenance() {
    // FR-008 / SC-006: each clause resolves to a real document + a non-empty
    // heading/anchor/excerpt and a full-length fingerprint.
    for u in units() {
        assert!(!u.document.is_empty());
        assert_eq!(
            u.fingerprint.len(),
            64,
            "fingerprint is a full SHA-256: {}",
            u.id
        );
        assert!(!u.locations.is_empty(), "clause {} has no locations", u.id);
        for loc in &u.locations {
            assert!(
                !loc.anchor.is_empty(),
                "clause {} location has no anchor",
                u.id
            );
            assert!(
                !loc.excerpt.trim().is_empty(),
                "clause {} has an empty excerpt",
                u.id
            );
        }
    }
}

#[test]
fn the_inventory_covers_a_substantial_normative_surface() {
    // SC-001: the prose surface is large; assert a healthy floor so a regression that
    // silently drops most clauses is caught (well below the current ~250).
    assert!(
        units().len() >= 150,
        "the pinned prose inventory should carry a substantial number of clauses, got {}",
        units().len()
    );
}
