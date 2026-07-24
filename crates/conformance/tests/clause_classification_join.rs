//! Acceptance tests for the clause classification join and violation classes V11–V15
//! (US2, T021; FR-016/FR-024/FR-025, research Decisions 5/7/8). Builds an in-memory
//! registry (varying only the clause-classification records) pointed at the fixture
//! prose + committed fixture clause inventory, and asserts each class fires exactly when
//! it should. Hermetic — no Docker, no network, no model.

use std::collections::HashSet;
use std::path::PathBuf;

use deacon_conformance::clause::generate_clauses;
use deacon_conformance::load::Registry;
use deacon_conformance::model::{
    BehaviorUnit, ClauseClassification, Decision, Disposition, ReferenceStatus, RevisionKind,
    SourceRevision, SpecStatus,
};
use deacon_conformance::validate::{ClauseInputs, Violation, check_clause_inventory};
use deacon_conformance::workspace_root;

fn spec_dir() -> PathBuf {
    workspace_root().join("fixtures/conformance/prose")
}
fn clauses_file() -> PathBuf {
    spec_dir().join("clauses.json")
}

/// The canonical ids of the committed fixture clauses, keyed by an excerpt substring.
fn clause_id(needle: &str) -> String {
    generate_clauses(&spec_dir(), &clauses_file())
        .expect("fixture clauses canonicalize")
        .units
        .into_iter()
        .find(|u| u.locations.iter().any(|l| l.excerpt.contains(needle)))
        .unwrap_or_else(|| panic!("no fixture clause with excerpt {needle:?}"))
        .id
}

fn behavior(id: &str) -> BehaviorUnit {
    BehaviorUnit {
        id: id.to_string(),
        area: "fixture".to_string(),
        statement: "s".to_string(),
        applicability: vec![],
        spec: SpecStatus::Conformant,
        reference: ReferenceStatus::Aligned,
        decision: Decision::FollowSpec,
        notes: None,
    }
}

fn per_clause(
    clause: &str,
    disposition: Disposition,
    behaviors: Vec<&str>,
) -> ClauseClassification {
    let tail = clause.strip_prefix("clu-").unwrap();
    ClauseClassification {
        id: format!("clc-{tail}"),
        clause: Some(clause.to_string()),
        document: None,
        disposition,
        behaviors: behaviors.into_iter().map(String::from).collect(),
        rationale: matches!(
            disposition,
            Disposition::NonTestable | Disposition::NotApplicable
        )
        .then(|| "rationale".to_string()),
        notes: None,
    }
}

fn doc_default(document: &str) -> ClauseClassification {
    ClauseClassification {
        id: format!("clc-doc-{document}"),
        clause: None,
        document: Some(document.to_string()),
        disposition: Disposition::NotApplicable,
        behaviors: vec![],
        rationale: Some("authoring document; consumer-only scope".to_string()),
        notes: None,
    }
}

/// Run the clause join over an in-memory registry carrying `classifications`.
fn run(classifications: Vec<ClauseClassification>) -> Vec<Violation> {
    let registry = Registry {
        revisions: vec![SourceRevision {
            id: "rev-spec-113500f4".to_string(),
            kind: RevisionKind::Spec,
            pin: "113500f4".to_string(),
            url: "u".to_string(),
            verified_against: None,
        }],
        behaviors: vec![behavior("bhv-x")],
        clause_classifications: classifications,
        ..Default::default()
    };
    let spec = spec_dir();
    let clauses = clauses_file();
    check_clause_inventory(
        &registry,
        &ClauseInputs {
            spec_dir: &spec,
            clauses_file: &clauses,
        },
    )
}

/// A fully-classified registry: every consumer clause per-clause, the authoring document
/// covered by a document-scope default. This must be violation-free.
fn fully_classified() -> Vec<ClauseClassification> {
    vec![
        per_clause(
            &clause_id("run onCreateCommand exactly once"),
            Disposition::BehaviorMapped,
            vec!["bhv-x"],
        ),
        per_clause(
            &clause_id("MUST NOT run it again"),
            Disposition::BehaviorMapped,
            vec!["bhv-x"],
        ),
        per_clause(
            &clause_id("should generally complete quickly"),
            Disposition::NonTestable,
            vec![],
        ),
        per_clause(
            &clause_id("\"outcome\": \"success\""),
            Disposition::BehaviorMapped,
            vec!["bhv-x"],
        ),
        per_clause(
            &clause_id("purely descriptive"),
            Disposition::NonTestable,
            vec![],
        ),
        // The consumer install clause inside the authoring doc needs a per-clause override.
        per_clause(
            &clause_id("invoke the feature install script"),
            Disposition::BehaviorMapped,
            vec!["bhv-x"],
        ),
        doc_default("authoring-sample"),
    ]
}

fn codes(v: &[Violation]) -> HashSet<&str> {
    v.iter().map(|x| x.code.as_str()).collect()
}

#[test]
fn fully_classified_registry_is_clean() {
    let v = run(fully_classified());
    assert!(
        v.is_empty(),
        "fully-classified fixture must be clean, got: {v:?}"
    );
}

#[test]
fn ambiguous_clause_without_per_clause_record_blocks_as_v12() {
    // Drop the classification for the ambiguous clause → it is unclassified (V12).
    let mut cls = fully_classified();
    let amb = clause_id("should generally complete quickly");
    cls.retain(|c| c.clause.as_deref() != Some(amb.as_str()));
    let v = run(cls);
    assert!(
        v.iter().any(|x| x.code == "V12" && x.record == amb),
        "an unresolved ambiguous clause must block as V12, got: {v:?}"
    );
}

#[test]
fn unclassified_consumer_clause_blocks_as_v12() {
    let mut cls = fully_classified();
    let target = clause_id("run onCreateCommand exactly once");
    cls.retain(|c| c.clause.as_deref() != Some(target.as_str()));
    let v = run(cls);
    assert!(v.iter().any(|x| x.code == "V12" && x.record == target));
}

#[test]
fn stale_classification_blocks_as_v11() {
    let mut cls = fully_classified();
    let ghost = "clu-sample-ghost-must-00000000";
    cls.push(per_clause(ghost, Disposition::NonTestable, vec![]));
    let v = run(cls);
    assert!(
        v.iter().any(|x| x.code == "V11"
            && x.record == format!("clc-{}", ghost.strip_prefix("clu-").unwrap())),
        "a classification for a missing clause is V11-stale, got: {v:?}"
    );
}

#[test]
fn document_scope_default_on_a_consumer_document_is_v13() {
    let mut cls = fully_classified();
    cls.push(doc_default("sample")); // `sample` is consumer scope.
    let v = run(cls);
    assert!(
        codes(&v).contains("V13"),
        "a document-scope default on a consumer document must be V13, got: {v:?}"
    );
}

#[test]
fn behavior_mapped_with_empty_behaviors_is_v13() {
    let mut cls = fully_classified();
    let target = clause_id("run onCreateCommand exactly once");
    // Replace the good record with a behavior-mapped one that has no behaviors.
    for c in &mut cls {
        if c.clause.as_deref() == Some(target.as_str()) {
            c.disposition = Disposition::BehaviorMapped;
            c.behaviors.clear();
        }
    }
    let v = run(cls);
    assert!(
        codes(&v).contains("V13"),
        "empty behaviors on behavior-mapped is V13: {v:?}"
    );
}

#[test]
fn id_tail_mismatch_is_v13() {
    let mut cls = fully_classified();
    let target = clause_id("run onCreateCommand exactly once");
    for c in &mut cls {
        if c.clause.as_deref() == Some(target.as_str()) {
            c.id = "clc-wrong-tail".to_string();
        }
    }
    let v = run(cls);
    assert!(codes(&v).contains("V13"), "id-tail mismatch is V13: {v:?}");
}

#[test]
fn authoring_document_scope_default_is_non_blocking() {
    // The document-scope default covers every non-ambiguous authoring clause without a
    // per-clause record. Remove the install override → the doc default covers it, clean.
    let mut cls = fully_classified();
    let install = clause_id("invoke the feature install script");
    cls.retain(|c| c.clause.as_deref() != Some(install.as_str()));
    let v = run(cls);
    assert!(
        v.is_empty(),
        "the authoring doc-scope default covers its non-ambiguous clauses, got: {v:?}"
    );
}
