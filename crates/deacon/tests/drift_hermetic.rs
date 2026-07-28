//! Drift and upgrade-proposal acceptance tests (026, US4/US6).
//!
//! Hermetic: no network, no container engine, no reference implementation. The live half
//! (the five upstream probes) lives in `parity_harness::drift`; everything checkable
//! without an upstream connection is checked here, in the fast lane.

use deacon_conformance::drift::check::{check_drift, check_proposal, drift_state_is_known};
use deacon_conformance::drift::{
    CompletedRun, DriftFile, DriftKind, DriftObservation, InputState, ProposalSections, Section,
    SectionEntry, UpgradeProposal, derive_observation_id, load_drift, render_proposal_json,
    render_proposal_md,
};
use deacon_conformance::{default_registry_dir, drift_dir_for, workspace_root};

fn observation(kind: DriftKind, pinned: &str, observed: &str) -> DriftObservation {
    DriftObservation {
        id: derive_observation_id(kind, pinned, observed),
        kind,
        pinned_revision: pinned.into(),
        observed_revision: observed.into(),
        affected_surfaces: vec!["conformance/spec/113500f4/features.md".into()],
        observed_at: "2026-07-28".into(),
        review_artifact: "target/drift/scan.json".into(),
    }
}

fn complete_run() -> Option<CompletedRun> {
    Some(CompletedRun {
        date: "2026-07-28".into(),
        kinds_probed: DriftKind::ALL.to_vec(),
    })
}

// ---------------------------------------------------------------------------
// T080 — V36 observation integrity
// ---------------------------------------------------------------------------

#[test]
fn the_committed_observations_file_is_clean() {
    let file = load_drift(&drift_dir_for(&default_registry_dir())).expect("observations load");
    assert!(
        check_drift(&file).is_empty(),
        "the committed drift root must be clean: {:?}",
        check_drift(&file)
    );
}

#[test]
fn a_hand_edited_id_is_caught() {
    let mut file = DriftFile {
        schema_version: 1,
        records: vec![observation(DriftKind::SpecCommit, "113500f4", "9f21ab77")],
        last_completed_run: complete_run(),
    };
    file.records[0].id = "drf-spec-commit-deadbeef".into();
    let violations = check_drift(&file);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].code, "V36");
    assert!(
        violations[0]
            .message
            .contains("not derived from its substance")
    );
}

#[test]
fn a_duplicate_id_is_caught() {
    let record = observation(DriftKind::SchemaChange, "113500f4", "9f21ab77");
    let file = DriftFile {
        schema_version: 1,
        records: vec![record.clone(), record],
        last_completed_run: complete_run(),
    };
    assert!(
        check_drift(&file)
            .iter()
            .any(|v| v.message.contains("duplicate"))
    );
}

#[test]
fn an_observation_with_no_review_artifact_is_caught() {
    // FR-027: every signal must be traceable to the artifact it produced.
    let mut file = DriftFile {
        schema_version: 1,
        records: vec![observation(DriftKind::SpecCommit, "113500f4", "9f21ab77")],
        last_completed_run: complete_run(),
    };
    file.records[0].review_artifact = String::new();
    assert!(
        check_drift(&file)
            .iter()
            .any(|v| v.message.contains("reviewArtifact"))
    );
}

#[test]
fn an_unknown_kind_cannot_be_represented() {
    // The kind set is closed, so an invented kind fails at load rather than reaching a
    // check. That is stronger than validating it later: an observation over a source
    // nothing probes is not a drift signal at all.
    let raw = r#"{ "schemaVersion": 1, "records": [
        { "id": "drf-x-0", "kind": "invented-kind", "pinnedRevision": "a",
          "observedRevision": "b", "affectedSurfaces": [], "observedAt": "2026-07-28",
          "reviewArtifact": "x" }
    ], "lastCompletedRun": null }"#;
    assert!(serde_json::from_str::<DriftFile>(raw).is_err());
}

// ---------------------------------------------------------------------------
// T081 — "no drift" vs "did not run" (FR-025)
// ---------------------------------------------------------------------------

#[test]
fn an_empty_queue_without_a_completed_run_reads_as_unknown_not_clean() {
    // The distinction FR-025 exists for. Without `lastCompletedRun` both states are the
    // same empty array, and an empty array reads as reassurance.
    let never_ran = DriftFile {
        schema_version: 1,
        records: vec![],
        last_completed_run: None,
    };
    assert!(
        !drift_state_is_known(&never_ran),
        "an absent run must not read as a clean result"
    );

    let complete = DriftFile {
        schema_version: 1,
        records: vec![],
        last_completed_run: complete_run(),
    };
    assert!(
        drift_state_is_known(&complete),
        "an empty queue plus a complete run IS a clean result"
    );

    // Both have identical `records`. The only thing distinguishing them is the run record.
    assert_eq!(never_ran.records.len(), complete.records.len());
}

#[test]
fn a_partial_run_reports_every_unprobed_kind() {
    let file = DriftFile {
        schema_version: 1,
        records: vec![],
        last_completed_run: Some(CompletedRun {
            date: "2026-07-28".into(),
            kinds_probed: vec![DriftKind::SpecCommit, DriftKind::SchemaChange],
        }),
    };
    let violations = check_drift(&file);
    assert_eq!(violations.len(), 3, "three of five kinds unprobed");
    assert!(!drift_state_is_known(&file));
}

#[test]
fn an_observation_with_no_run_that_produced_it_is_caught() {
    let file = DriftFile {
        schema_version: 1,
        records: vec![observation(DriftKind::SpecCommit, "113500f4", "9f21ab77")],
        last_completed_run: None,
    };
    assert!(
        check_drift(&file)
            .iter()
            .any(|v| v.message.contains("no `lastCompletedRun`"))
    );
}

// ---------------------------------------------------------------------------
// T082 — the write path allow-list (FR-058, SC-015)
// ---------------------------------------------------------------------------

/// The only paths drift automation may write (FR-024a).
const PERMITTED_WRITE_PREFIXES: &[&str] = &["conformance/drift/", "target/drift/"];

#[test]
fn the_permitted_write_set_excludes_every_record_snapshot_and_pin_path() {
    for forbidden in [
        "conformance/registry/revisions.json",
        "conformance/registry/waivers/wvr-x.json",
        "conformance/registry/cases/up.json",
        "conformance/snapshots/linux-x86_64/case-a/provenance.json",
        "fixtures/parity-corpus/oracle.json",
        "conformance/discovery/canary.json",
    ] {
        assert!(
            !PERMITTED_WRITE_PREFIXES
                .iter()
                .any(|prefix| forbidden.starts_with(prefix)),
            "`{forbidden}` must be outside drift automation's write allow-list (FR-024a)"
        );
    }
    for permitted in [
        "conformance/drift/observations.json",
        "target/drift/scan.json",
        "target/drift/upgrade-proposal.json",
    ] {
        assert!(
            PERMITTED_WRITE_PREFIXES
                .iter()
                .any(|prefix| permitted.starts_with(prefix)),
            "`{permitted}` must be inside the allow-list"
        );
    }
}

/// Strip comments and `#[cfg(test)]` modules, leaving only production source.
///
/// Two exclusions, both necessary rather than convenient.
///
/// **Comments**, because the property under test is about *code*, not prose: every one of
/// these modules deliberately documents the boundary it must not cross — `drift/mod.rs`
/// explains at length that observations are not pins — and a scanner matching documentation
/// would punish exactly the comments that make the constraint legible.
///
/// **Test modules**, because a test that asserts a path is *refused* has to name that path.
/// `drift/mod.rs`'s own unit tests list every registry, snapshot, and pin location
/// precisely to prove the allow-list rejects them; scanning those literals would report the
/// guard as the violation it exists to prevent.
fn code_only(source: &str) -> String {
    let production = match source.find("#[cfg(test)]") {
        Some(idx) => &source[..idx],
        None => source,
    };
    production
        .lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn no_hermetic_drift_source_can_write_a_registry_record_or_pin() {
    // FR-024 / SC-015, asserted where it can be: the drift modules must contain no code
    // path to a pin, a disposition, or a snapshot. An abort-on-out-of-scope check is only
    // as good as the absence of a second way in.
    let base = workspace_root()
        .join("crates")
        .join("conformance")
        .join("src")
        .join("drift");
    for module in ["mod.rs", "check.rs"] {
        let source =
            code_only(&std::fs::read_to_string(base.join(module)).expect("module readable"));
        for forbidden in [
            "revisions.json",
            "waivers/",
            "conformance-snapshot",
            "write_provenance",
            "atomic_write",
        ] {
            assert!(
                !source.contains(forbidden),
                "`drift/{module}` has a code path referencing `{forbidden}`; drift \
                 automation must not be able to advance a pin, alter a disposition, or \
                 refresh a snapshot (FR-024)"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T107 / T108 / T110 — upgrade-proposal completeness and determinism
// ---------------------------------------------------------------------------

fn clean_sections() -> ProposalSections {
    ProposalSections {
        schema_drift: Section::clean(),
        specification_drift: Section::clean(),
        cli_surface_drift: Section::clean(),
        reference_behavior_drift: Section::clean(),
        snapshot_differences: Section::clean(),
        newly_failing_cases: Section::clean(),
        affected_dispositions: Section::clean(),
    }
}

fn proposal(sections: ProposalSections, clean_tree: bool) -> UpgradeProposal {
    UpgradeProposal {
        schema_version: 1,
        from_oracle: "0.87.0".into(),
        to_oracle: "0.88.0".into(),
        input_state: InputState {
            registry_digest: "abc123".into(),
            worktree_clean: clean_tree,
        },
        sections,
    }
}

#[test]
fn a_bundle_missing_any_of_the_seven_sections_is_rejected() {
    // FR-030 / FR-051 / SC-007 — 100% of the time, for each of the seven.
    let full = serde_json::to_value(proposal(clean_sections(), true)).expect("serialize");
    for section in ProposalSections::NAMES {
        let mut partial = full.clone();
        partial["sections"]
            .as_object_mut()
            .expect("sections is an object")
            .remove(*section);
        let parsed: Result<UpgradeProposal, _> = serde_json::from_value(partial);
        assert!(
            parsed.is_err(),
            "a bundle missing `{section}` must be rejected — an unrun analysis must never \
             read as a clean one"
        );
    }
}

#[test]
fn a_section_with_no_entries_is_clean_and_a_missing_one_is_not() {
    // The load-bearing distinction: `entries: []` means investigated and clean; an absent
    // key means not investigated.
    let bundle = proposal(clean_sections(), true);
    assert!(
        check_proposal(&bundle).is_empty(),
        "empty entries are clean"
    );

    let mut blanked = clean_sections();
    blanked.schema_drift.present = false;
    assert!(
        !check_proposal(&proposal(blanked, true)).is_empty(),
        "a section marked not-present must be rejected"
    );
}

#[test]
fn the_bundle_regenerates_byte_identically() {
    // FR-031 / SC-007.
    let bundle = proposal(clean_sections(), true);
    assert_eq!(render_proposal_json(&bundle), render_proposal_json(&bundle));
    assert_eq!(render_proposal_md(&bundle), render_proposal_md(&bundle));
    assert!(
        !render_proposal_md(&bundle).contains("/workspaces"),
        "no absolute path may appear in a bundle"
    );
}

#[test]
fn unsorted_entries_are_rejected_because_they_break_reproducibility() {
    let mut sections = clean_sections();
    sections.newly_failing_cases.entries = vec![
        SectionEntry {
            subject: "case-z".into(),
            detail: "fails".into(),
            informational_only: false,
        },
        SectionEntry {
            subject: "case-a".into(),
            detail: "fails".into(),
            informational_only: false,
        },
    ];
    assert!(
        check_proposal(&proposal(sections, true))
            .iter()
            .any(|v| v.message.contains("not sorted"))
    );
}

#[test]
fn a_bundle_built_on_a_dirty_tree_is_recognizable_as_such() {
    let dirty = proposal(clean_sections(), false);
    let rendered = render_proposal_md(&dirty);
    assert!(
        rendered.contains("dirty working tree"),
        "a proposal computed against uncommitted registry edits must say so"
    );
    let clean = proposal(clean_sections(), true);
    assert!(!render_proposal_md(&clean).contains("dirty working tree"));
}

// ---------------------------------------------------------------------------
// T109 / T111 — no pin writer, and canary evidence admissibility
// ---------------------------------------------------------------------------

#[test]
fn no_automated_path_can_advance_the_stable_pin() {
    // FR-028 / SC-006. The strongest form available hermetically: no drift or proposal
    // source may name the files that hold the pin.
    let roots = [
        workspace_root()
            .join("crates")
            .join("conformance")
            .join("src")
            .join("drift"),
        workspace_root()
            .join("crates")
            .join("parity-harness")
            .join("src")
            .join("drift"),
    ];
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(raw) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            let source = code_only(&raw);
            // A write to the pin needs both a write primitive and the pin's path in the
            // same module. Requiring both keeps the check meaningful: a module that
            // *reads* the pin to compare against it is not a module that can advance it.
            let writes = source.contains("atomic_write")
                || source.contains("fs::write")
                || source.contains("fs::rename");
            for pin_file in ["revisions.json", "oracle.json"] {
                assert!(
                    !(writes && source.contains(pin_file)),
                    "{} has both a write primitive and `{pin_file}` in its code; no \
                     automated path may advance the stable pin (FR-028, SC-006)",
                    entry.path().display()
                );
            }
        }
    }
}

#[test]
fn canary_evidence_is_marked_informational_unless_pinned_and_hermetic() {
    // FR-033. A canary result may inform the decision; it may not back it. The record
    // carries that judgement per entry, so a reviewer sees which findings are evidence and
    // which are merely suggestive.
    let mut sections = clean_sections();
    sections.reference_behavior_drift.entries = vec![SectionEntry {
        subject: "bhv-exec-argv".into(),
        detail: "observed against an unpinned canary revision".into(),
        informational_only: true,
    }];
    let bundle = proposal(sections, true);
    assert!(
        check_proposal(&bundle).is_empty(),
        "an informational entry is well-formed"
    );

    let rendered = render_proposal_md(&bundle);
    assert!(
        rendered.contains("informational only"),
        "an entry that is not pinned and hermetic must be visibly marked as such"
    );
}

#[test]
fn a_proposal_from_a_version_to_itself_is_rejected() {
    let mut bundle = proposal(clean_sections(), true);
    bundle.to_oracle = bundle.from_oracle.clone();
    assert!(!check_proposal(&bundle).is_empty());
}

// ---------------------------------------------------------------------------
// T083 — the scan's status reflects whether it ran, never what it found (FR-026)
// ---------------------------------------------------------------------------

#[test]
fn the_scan_binary_returns_success_regardless_of_findings() {
    // FR-026, asserted structurally over the binary's own source. A behavioral test would
    // need a real upstream, and the property is about the CONTROL FLOW: there must be no
    // path from "a probe found drift" to a non-zero exit. Reading the source is how that
    // is checkable in the fast lane.
    let source = std::fs::read_to_string(
        workspace_root()
            .join("crates")
            .join("parity-harness")
            .join("src")
            .join("bin")
            .join("drift-scan.rs"),
    )
    .expect("drift-scan source readable");
    let code = code_only(&source);

    // The only non-zero exits must be reachable from an error branch. Every `ExitCode::from`
    // in this binary sits under a `Err(...)` arm or an argument-parsing failure; a
    // `Drifted` arm that produced one would be the defect.
    let drifted_arm = code
        .split("Ok(ProbeResult::Drifted")
        .nth(1)
        .expect("the binary must handle a drifted probe");
    let arm_body = &drifted_arm[..drifted_arm.find("Err(e)").unwrap_or(drifted_arm.len())];
    assert!(
        !arm_body.contains("ExitCode::from"),
        "finding drift must not produce a non-zero exit — a finding-dependent status \
         becomes a gate the moment someone wires it into a required check (FR-026)"
    );
    assert!(
        code.contains("ExitCode::SUCCESS"),
        "the scan must have a success path it reaches after reporting findings"
    );
}

#[test]
fn the_scan_records_only_the_kinds_that_completed() {
    // The other half of FR-025: a probe that failed must not appear in `kindsProbed`, or a
    // partial run would record itself as complete and its empty `records` would read as
    // "no drift" when the truth is "not looked at".
    let source = std::fs::read_to_string(
        workspace_root()
            .join("crates")
            .join("parity-harness")
            .join("src")
            .join("bin")
            .join("drift-scan.rs"),
    )
    .expect("drift-scan source readable");
    let code = code_only(&source);
    let error_arm = code
        .split("Err(e) =>")
        .nth(1)
        .expect("the binary must handle a failed probe");
    let arm_body = &error_arm[..error_arm.find("}\n}").unwrap_or(error_arm.len().min(400))];
    assert!(
        !arm_body.contains("probed.push"),
        "a probe that could not run must NOT be recorded as probed (FR-025)"
    );
}
