//! T069 (US5, FR-045): lowering the baseline to satisfy the report surfaces as a **V25**
//! failure, not as a green report.
//!
//! This is the load-bearing anti-gaming property. Every other check in this feature
//! measures the migration against the baseline — so if the baseline itself can be edited,
//! every one of them can be satisfied by editing it, and "no coverage was lost" becomes
//! unfalsifiable.
//!
//! The guard is that the report **reads** the baseline and never writes it: lowering the
//! bar is a visible diff in a version-controlled file, reviewed like any other, and never
//! an invisible side effect of running a tool.
//!
//! **What changed at T099**: the second guard used to be V25 — `baseline check`
//! recomputing the inventory from the repository tree and byte-comparing, so a lowered
//! baseline failed validation even when the report went green. That gate is retired
//! (FR-053): once a superseded carrier is deleted, regeneration cannot reproduce a
//! pre-migration record by construction, and a permanent gate would forbid ever retiring
//! the machinery this migration exists to retire. The baseline is retained as evidence;
//! the automated recomputation is gone.
//!
//! So the property these tests now state is narrower and honest about it: a lowered
//! baseline still shows up as a diff, the report still cannot cause one, and the report
//! going green on a lowered baseline is DEMONSTRATED here rather than assumed — which is
//! exactly why the artifact is version-controlled and reviewed.
//!
//! Hermetic: mutates copies of the real baseline.

mod support;

use support::Fixture;

/// The unit a saboteur would delete to make an orphan disappear.
const TARGET: &str = "parity_corpus_tier1::node-ts";

#[test]
fn lowering_the_baseline_does_hide_the_orphan_which_is_why_it_is_reviewed() {
    // Step 1: orphan a unit. The report fails, as T066 proves.
    let orphaned = Fixture::real().without_mapping_entry(TARGET);
    let report = orphaned.report().expect("report computes");
    assert!(
        report
            .accounting
            .unaccounted
            .iter()
            .any(|u| u.unit == TARGET),
        "precondition: the orphan is reported"
    );

    // Step 2: the gaming move — delete the unit from the BASELINE so the orphan has
    // nothing to be an orphan of. The report goes green.
    let gamed = orphaned.edit_baseline(|doc| {
        if let Some(records) = doc.get_mut("records").and_then(|v| v.as_array_mut()) {
            records.retain(|r| r.get("id").and_then(|v| v.as_str()) != Some(TARGET));
        }
    });
    let gamed_report = gamed.report().expect("report computes");
    assert!(
        !gamed_report
            .accounting
            .unaccounted
            .iter()
            .any(|u| u.unit == TARGET),
        "lowering the baseline DOES hide the orphan from the accounting"
    );

    // …which is stated here, not hidden, because with V25 retired (T099) the remaining
    // guard is review, not automation: the deleted unit is a one-line removal from a
    // version-controlled artifact whose whole purpose is to be diffed. A test that
    // pretended otherwise would be the more dangerous artifact.
    let before = Fixture::real();
    let baseline = before
        .baseline_unit(TARGET)
        .expect("the unit is in the committed baseline");
    assert!(
        !baseline.assertion.trim().is_empty(),
        "the record a reviewer would see removed carries what the unit asserted, so the \
         removal is legible in the diff"
    );
}

#[test]
fn the_report_never_writes_the_baseline() {
    // FR-045: the report reads the baseline. If it could write it, "lowering the
    // baseline" would stop being a reviewable diff and become a side effect.
    let fixture = Fixture::real();
    let baseline_path = fixture
        .registry_dir()
        .parent()
        .expect("conformance dir")
        .join("migration")
        .join("baseline.json");
    let before = std::fs::read(&baseline_path).expect("baseline exists");

    let _ = fixture.report().expect("report computes");
    let _ = fixture.report().expect("report computes again");

    let after = std::fs::read(&baseline_path).expect("baseline still exists");
    assert_eq!(
        before, after,
        "computing the report must leave the baseline byte-identical"
    );
}
