//! T028 (US2, FR-007 / FR-050): a case's identity is **authored**, and annotating a
//! case changes neither its identity nor its committed snapshot provenance.
//!
//! Two properties, and they are different:
//!
//! 1. **id stability** — a case id is a hand-authored `case-*` slug (research D7), not a
//!    content hash. Editing `notes` cannot change it, because nothing derives it from
//!    content. A content-hash id would churn on every edit and break every committed
//!    snapshot's provenance — which is precisely why clause ids are hashed and case ids
//!    are not.
//! 2. **provenance stability** — `caseHash` is computed over the behavior-affecting
//!    inputs ONLY (`operations`, `oracleType`, `expected`, `fsAllowlist`, and the
//!    referenced fixture hashes). `notes`, `allowedDifferences`, and `behaviors` are
//!    excluded (research D3), so annotating a case — including attaching a scoped
//!    tolerance during this migration — never re-records a snapshot.
//!
//! Hermetic: reads the real registry and hashes committed fixtures. No Docker, no
//! network, no oracle.

use deacon_conformance::case_hash::hashes_for_case;
use deacon_conformance::load::Registry;
use deacon_conformance::model::{AllowedDifference, CaseKind, TestCase};
use deacon_conformance::{default_registry_dir, workspace_root};

fn fixtures_root() -> std::path::PathBuf {
    workspace_root().join("conformance").join("fixtures")
}

fn declarative_cases(registry: &Registry) -> Vec<&TestCase> {
    registry
        .cases
        .iter()
        .filter(|c| matches!(c.classify(), Ok(CaseKind::Declarative)))
        .collect()
}

#[test]
fn editing_notes_changes_neither_the_id_nor_the_case_hash() {
    let registry =
        Registry::load(&default_registry_dir()).expect("the real registry loads cleanly");
    let cases = declarative_cases(&registry);
    assert!(
        !cases.is_empty(),
        "there must be declarative cases to exercise"
    );

    for case in cases {
        let before = hashes_for_case(case, &fixtures_root())
            .unwrap_or_else(|e| panic!("hash {}: {e}", case.id));

        let mut annotated = case.clone();
        annotated.notes = Some(
            "A completely rewritten note that says something else entirely, added during \
             review."
                .to_string(),
        );
        let after = hashes_for_case(&annotated, &fixtures_root())
            .unwrap_or_else(|e| panic!("hash {}: {e}", case.id));

        assert_eq!(
            annotated.id, case.id,
            "a case id is authored, never derived from content"
        );
        assert_eq!(
            before, after,
            "annotating case {} must not change its caseHash — a committed snapshot's \
             provenance would go stale for a pure prose edit (research D3)",
            case.id
        );
    }
}

#[test]
fn attaching_a_scoped_tolerance_does_not_change_the_case_hash() {
    // US4 attaches `allowedDifferences` to characterized divergences. That is an
    // annotation of a KNOWN difference, not a change to what the case does, so it must
    // not re-record the snapshot either (research D3, FR-033).
    let registry =
        Registry::load(&default_registry_dir()).expect("the real registry loads cleanly");
    let case = declarative_cases(&registry)
        .into_iter()
        .next()
        .expect("at least one declarative case");

    let before = hashes_for_case(case, &fixtures_root()).expect("hash");

    let mut annotated = case.clone();
    annotated.allowed_differences.push(AllowedDifference {
        behavior: case
            .behaviors
            .first()
            .cloned()
            .unwrap_or_else(|| "bhv-x".to_string()),
        context: Vec::new(),
        observable_path: "chan-structured-output.configuration.appPort".to_string(),
        rationale: "probe".to_string(),
        waiver_id: Some("wvr-probe".to_string()),
        divergence_id: None,
    });
    let after = hashes_for_case(&annotated, &fixtures_root()).expect("hash");

    assert_eq!(
        before, after,
        "attaching a scoped tolerance is an annotation, not a behavior change"
    );
}

#[test]
fn changing_an_operation_does_change_the_case_hash() {
    // The converse: the hash MUST move when a behavior-affecting input moves, or the
    // staleness gate would be worthless.
    let registry =
        Registry::load(&default_registry_dir()).expect("the real registry loads cleanly");
    let case = declarative_cases(&registry)
        .into_iter()
        .find(|c| !c.operations.is_empty())
        .expect("a declarative case with an operation");

    let before = hashes_for_case(case, &fixtures_root()).expect("hash");
    let mut changed = case.clone();
    changed.operations[0]
        .argv
        .push("--a-new-flag-that-changes-behavior".to_string());
    let after = hashes_for_case(&changed, &fixtures_root()).expect("hash");

    assert_ne!(
        before, after,
        "a changed argv MUST change the caseHash — otherwise a stale snapshot would \
         replay against different inputs"
    );
}

#[test]
fn every_migrated_case_id_is_an_authored_slug() {
    // FR-050: identity is authored, so it must read as a slug — never a hash. A
    // hex-looking tail would mean someone reintroduced content-derived identity.
    let registry =
        Registry::load(&default_registry_dir()).expect("the real registry loads cleanly");
    for case in &registry.cases {
        assert!(
            case.id.starts_with("case-"),
            "case id {:?} must be an authored `case-*` slug",
            case.id
        );
        let tail = case.id.rsplit('-').next().unwrap_or_default();
        assert!(
            !(tail.len() >= 8 && tail.chars().all(|c| c.is_ascii_hexdigit())),
            "case id {:?} ends in what looks like a content hash; case identity is \
             authored, not derived (research D7)",
            case.id
        );
    }
}

#[test]
fn committed_snapshot_provenance_still_matches_its_case() {
    // The migration added cases around the snapshot-oracle case; its committed
    // provenance must still describe the case it was recorded from.
    let registry =
        Registry::load(&default_registry_dir()).expect("the real registry loads cleanly");
    let snapshots = workspace_root().join("conformance").join("snapshots");
    if !snapshots.is_dir() {
        return; // no committed snapshots on this checkout
    }

    for platform in std::fs::read_dir(&snapshots)
        .expect("read snapshots")
        .flatten()
    {
        if !platform.path().is_dir() {
            continue;
        }
        for case_dir in std::fs::read_dir(platform.path())
            .expect("read platform")
            .flatten()
        {
            let provenance_path = case_dir.path().join("provenance.json");
            if !provenance_path.is_file() {
                continue;
            }
            // The case id is the snapshot directory's name (the tree is
            // `<os-arch>/<case-id>/`).
            let case_id = case_dir.file_name().to_string_lossy().into_owned();
            let raw = std::fs::read_to_string(&provenance_path).expect("read provenance");
            let provenance: serde_json::Value =
                serde_json::from_str(&raw).expect("provenance parses");
            let case = registry
                .cases
                .iter()
                .find(|c| c.id == case_id)
                .unwrap_or_else(|| panic!("snapshot names unknown case {case_id:?}"));

            let recorded = provenance
                .get("caseHash")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let (case_hash, _fixture_hash) = hashes_for_case(case, &fixtures_root())
                .unwrap_or_else(|e| panic!("hash {case_id}: {e}"));
            assert_eq!(
                recorded, case_hash,
                "the migration must not have disturbed {case_id}'s committed snapshot \
                 provenance"
            );
        }
    }
}
