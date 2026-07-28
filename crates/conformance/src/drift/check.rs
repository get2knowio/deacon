//! **V36 — drift-record integrity** (026, US4/US6; data-model.md §8).
//!
//! Hermetic by construction: everything here reads committed data and compares it to
//! itself. The network-facing half of drift detection lives in `parity_harness::drift`,
//! so an incomplete bundle or a mis-derived observation is caught on a pull request
//! without provisioning an upstream connection or two oracle versions.

use std::collections::BTreeSet;

use super::{DriftFile, DriftKind, ProposalSections, UpgradeProposal, run_is_complete};
use crate::validate::Violation;

/// Check the committed drift observations.
///
/// | Sub-case | Guards |
/// |---|---|
/// | derived id | an `id` that disagrees with `kind ‖ pinnedRevision ‖ observedRevision` |
/// | duplicate | two records with the same id |
/// | empty revision | a blank `pinnedRevision` or `observedRevision` |
/// | no-op | an observation whose observed revision equals its pin — that is not drift |
/// | incomplete run | a `lastCompletedRun` that omits a kind |
/// | missing artifact | an observation with no `reviewArtifact` (FR-027) |
pub fn check_drift(file: &DriftFile) -> Vec<Violation> {
    let mut violations = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();

    for record in &file.records {
        let derived = record.derived_id();
        if record.id != derived {
            violations.push(Violation::v36(
                &record.id,
                format!(
                    "id is not derived from its substance (expected `{derived}`). A \
                     substance-anchored id is what keeps a re-observation of the same \
                     drift from minting a second record every scan."
                ),
            ));
        }
        if !seen.insert(record.id.as_str()) {
            violations.push(Violation::v36(
                &record.id,
                "is a duplicate id. Two observations cannot share one identity.",
            ));
        }
        if record.pinned_revision.trim().is_empty() || record.observed_revision.trim().is_empty() {
            violations.push(Violation::v36(
                &record.id,
                "has a blank `pinnedRevision` or `observedRevision`. An observation with \
                 no revisions names no drift.",
            ));
        } else if record.pinned_revision == record.observed_revision {
            violations.push(Violation::v36(
                &record.id,
                "records an observed revision equal to its pin. That is not drift — \
                 remedy: drop the record rather than reporting a difference that is not one.",
            ));
        }
        if record.review_artifact.trim().is_empty() {
            violations.push(Violation::v36(
                &record.id,
                "names no `reviewArtifact`. Every drift signal must be traceable to the \
                 artifact it produced (FR-027).",
            ));
        }
    }

    if let Some(run) = &file.last_completed_run {
        let probed: BTreeSet<DriftKind> = run.kinds_probed.iter().copied().collect();
        for kind in DriftKind::ALL {
            if !probed.contains(kind) {
                violations.push(Violation::v36(
                    "observations.json",
                    format!(
                        "`lastCompletedRun` omits probed kind `{}`. A partial run must not \
                         be recorded as complete — an empty `records` alongside it would \
                         read as \"no drift\" when the truth is \"not looked at\" (FR-025).",
                        kind.as_str()
                    ),
                ));
            }
        }
        if run.date.trim().is_empty() {
            violations.push(Violation::v36(
                "observations.json",
                "`lastCompletedRun` has a blank date.",
            ));
        }
    } else if !file.records.is_empty() {
        violations.push(Violation::v36(
            "observations.json",
            "carries observations but no `lastCompletedRun`. A record cannot exist without \
             a run that produced it.",
        ));
    }

    violations
}

/// Whether the committed drift state should be read as "clean" rather than "unknown".
/// Exposed so a reporter can say which of the two it is instead of printing an empty list
/// either way.
pub fn drift_state_is_known(file: &DriftFile) -> bool {
    run_is_complete(file)
}

/// Check an upgrade-proposal bundle for completeness and determinism.
///
/// A bundle that fails to parse never reaches here — the type requires all seven sections,
/// so `serde` rejects a document missing one (FR-030). What remains checkable on a parsed
/// bundle is: a section explicitly marked `present: false`, an entry with no subject, an
/// unsorted entry list (which would break byte-reproducibility, FR-031), and an
/// informational-only entry in a section that is being cited as evidence.
pub fn check_proposal(proposal: &UpgradeProposal) -> Vec<Violation> {
    let mut violations = Vec::new();

    let sections: [(&str, &super::Section); 7] = [
        (ProposalSections::NAMES[0], &proposal.sections.schema_drift),
        (
            ProposalSections::NAMES[1],
            &proposal.sections.specification_drift,
        ),
        (
            ProposalSections::NAMES[2],
            &proposal.sections.cli_surface_drift,
        ),
        (
            ProposalSections::NAMES[3],
            &proposal.sections.reference_behavior_drift,
        ),
        (
            ProposalSections::NAMES[4],
            &proposal.sections.snapshot_differences,
        ),
        (
            ProposalSections::NAMES[5],
            &proposal.sections.newly_failing_cases,
        ),
        (
            ProposalSections::NAMES[6],
            &proposal.sections.affected_dispositions,
        ),
    ];

    for (name, section) in sections {
        if !section.present {
            violations.push(Violation::v36(
                name,
                "is marked `present: false`. A section that was not investigated cannot be \
                 read as clean, and an incomplete bundle cannot authorize an upgrade (FR-030).",
            ));
        }
        let subjects: Vec<&str> = section.entries.iter().map(|e| e.subject.as_str()).collect();
        let mut sorted = subjects.clone();
        sorted.sort_unstable();
        if subjects != sorted {
            violations.push(Violation::v36(
                name,
                "has entries that are not sorted by subject. Discovery-order output is not \
                 byte-reproducible, so the bundle could not be regenerated identically (FR-031).",
            ));
        }
        for entry in &section.entries {
            if entry.subject.trim().is_empty() {
                violations.push(Violation::v36(
                    name,
                    "has an entry with a blank subject, so it has no stable sort key.",
                ));
            }
        }
    }

    if proposal.from_oracle == proposal.to_oracle {
        violations.push(Violation::v36(
            "upgrade-proposal",
            "proposes an upgrade from a version to itself.",
        ));
    }
    if proposal.input_state.registry_digest.trim().is_empty() {
        violations.push(Violation::v36(
            "upgrade-proposal",
            "records no `inputState.registryDigest`, so what it was computed from cannot \
             be established (FR-027).",
        ));
    }

    violations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drift::{
        CompletedRun, DriftObservation, InputState, Section, SectionEntry, derive_observation_id,
    };

    fn observation(kind: DriftKind, pinned: &str, observed: &str) -> DriftObservation {
        DriftObservation {
            id: derive_observation_id(kind, pinned, observed),
            kind,
            pinned_revision: pinned.into(),
            observed_revision: observed.into(),
            affected_surfaces: vec![],
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

    #[test]
    fn a_well_formed_file_is_clean() {
        let file = DriftFile {
            schema_version: 1,
            records: vec![observation(DriftKind::SpecCommit, "113500f4", "9f21ab77")],
            last_completed_run: complete_run(),
        };
        assert_eq!(check_drift(&file), vec![]);
    }

    #[test]
    fn a_hand_edited_id_is_caught() {
        let mut file = DriftFile {
            schema_version: 1,
            records: vec![observation(DriftKind::SpecCommit, "113500f4", "9f21ab77")],
            last_completed_run: complete_run(),
        };
        file.records[0].id = "drf-spec-commit-00000000".into();
        let violations = check_drift(&file);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].code, "V36");
    }

    #[test]
    fn a_partial_run_is_reported_rather_than_reading_as_clean() {
        let file = DriftFile {
            schema_version: 1,
            records: vec![],
            last_completed_run: Some(CompletedRun {
                date: "2026-07-28".into(),
                kinds_probed: vec![DriftKind::SpecCommit],
            }),
        };
        let violations = check_drift(&file);
        assert_eq!(violations.len(), 4, "four kinds unprobed");
        assert!(!drift_state_is_known(&file));
    }

    #[test]
    fn an_observation_equal_to_its_pin_is_not_drift() {
        let file = DriftFile {
            schema_version: 1,
            records: vec![observation(DriftKind::SpecCommit, "113500f4", "113500f4")],
            last_completed_run: complete_run(),
        };
        assert!(
            check_drift(&file)
                .iter()
                .any(|v| v.message.contains("not drift"))
        );
    }

    fn proposal(sections: ProposalSections) -> UpgradeProposal {
        UpgradeProposal {
            schema_version: 1,
            from_oracle: "0.87.0".into(),
            to_oracle: "0.88.0".into(),
            input_state: InputState {
                registry_digest: "abc".into(),
                worktree_clean: true,
            },
            sections,
        }
    }

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

    #[test]
    fn a_complete_clean_bundle_passes() {
        assert_eq!(check_proposal(&proposal(clean_sections())), vec![]);
    }

    #[test]
    fn a_section_marked_absent_is_rejected() {
        let mut sections = clean_sections();
        sections.schema_drift.present = false;
        let violations = check_proposal(&proposal(sections));
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].record, "schemaDrift");
    }

    #[test]
    fn unsorted_entries_break_reproducibility_and_are_caught() {
        let mut sections = clean_sections();
        sections.schema_drift.entries = vec![
            SectionEntry {
                subject: "zzz".into(),
                detail: "d".into(),
                informational_only: false,
            },
            SectionEntry {
                subject: "aaa".into(),
                detail: "d".into(),
                informational_only: false,
            },
        ];
        assert!(
            check_proposal(&proposal(sections))
                .iter()
                .any(|v| v.message.contains("not sorted"))
        );
    }
}
