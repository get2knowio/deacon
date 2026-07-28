//! Upstream **drift observations** and **oracle upgrade proposals** — the hermetic half
//! (026-continuous-conformance-certification, US4/US6; contracts/cli-drift.md).
//!
//! ## Observations are not pins
//!
//! `conformance/drift/observations.json` records *what upstream currently looks like*. The
//! pin — *what deacon is pinned to* — stays in `conformance/registry/revisions.json` and
//! remains human-only (FR-028). That separation is the whole reason drift automation may
//! write anything at all: writing an observation blesses nothing, because nothing consumes
//! an observation as a claim about deacon.
//!
//! ## Why `lastCompletedRun` exists
//!
//! FR-025 requires "no drift" to be distinguishable from "drift detection did not run".
//! Without a completed-run record both states are the same empty array, and an empty array
//! reads as reassurance. With it, the reader can tell the difference: empty `records`
//! *plus* a `lastCompletedRun` covering all five kinds means clean; a missing or partial
//! one means unknown.
//!
//! ## Present-but-empty vs missing, in the upgrade proposal
//!
//! `"entries": []` means investigated and clean. A **missing section key** means not
//! investigated, and is rejected (FR-030). This distinction is the load-bearing property
//! of the bundle: the coverage model already found two cases where an assertion that could
//! never fail read as coverage, and an unrun analysis reading as a clean one is the same
//! defect at review scale.

pub mod check;

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The `observations.json` document (data-model.md §4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DriftFile {
    pub schema_version: u32,
    #[serde(default)]
    pub records: Vec<DriftObservation>,
    /// `None` means the scan has never completed. Distinguishing this from
    /// `records: []` is FR-025.
    #[serde(default)]
    pub last_completed_run: Option<CompletedRun>,
}

impl Default for DriftFile {
    fn default() -> Self {
        DriftFile {
            schema_version: 1,
            records: Vec::new(),
            last_completed_run: None,
        }
    }
}

/// The record of a scan that finished, and what it probed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompletedRun {
    /// Date only — no clock time, so the file stays byte-stable across a day's runs.
    pub date: String,
    pub kinds_probed: Vec<DriftKind>,
}

/// One observation that a pinned upstream source has moved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DriftObservation {
    /// `drf-<kind>-<pinned>-<hash8>`, derived from the substance so a re-observation of
    /// the same drift keeps its identity.
    pub id: String,
    pub kind: DriftKind,
    pub pinned_revision: String,
    pub observed_revision: String,
    #[serde(default)]
    pub affected_surfaces: Vec<String>,
    pub observed_at: String,
    /// The review artifact this observation produced (FR-027).
    pub review_artifact: String,
}

impl DriftObservation {
    /// The substance-anchored id this record must carry.
    pub fn derived_id(&self) -> String {
        derive_observation_id(self.kind, &self.pinned_revision, &self.observed_revision)
    }
}

/// Compute the derived id for an observation.
///
/// Anchored to `kind ‖ pinnedRevision ‖ observedRevision` and nothing else: re-observing
/// the same drift on a later day must not mint a new record, or the queue would grow one
/// entry per scan for a single unchanged fact.
pub fn derive_observation_id(kind: DriftKind, pinned: &str, observed: &str) -> String {
    // Reuses the length-prefixed `hash8` rather than a separator-joined one: an
    // `affectedSurfaces` path is ultimately built from upstream file names, so no
    // assumption about the input alphabet is safe here either.
    format!(
        "drf-{}-{}",
        kind.as_str(),
        crate::discovery::hash8(&[kind.as_str(), pinned, observed])
    )
}

/// The five upstream source kinds this system observes (FR-022).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DriftKind {
    SpecCommit,
    SchemaChange,
    ReferenceRelease,
    CliSurfaceChange,
    UpstreamTestOrChangelog,
}

impl DriftKind {
    /// The wire name, used in derived ids.
    pub fn as_str(self) -> &'static str {
        match self {
            DriftKind::SpecCommit => "spec-commit",
            DriftKind::SchemaChange => "schema-change",
            DriftKind::ReferenceRelease => "reference-release",
            DriftKind::CliSurfaceChange => "cli-surface-change",
            DriftKind::UpstreamTestOrChangelog => "upstream-test-or-changelog",
        }
    }

    /// Every kind — the set `lastCompletedRun.kindsProbed` must cover for a run to count
    /// as complete.
    pub const ALL: &'static [DriftKind] = &[
        DriftKind::SpecCommit,
        DriftKind::SchemaChange,
        DriftKind::ReferenceRelease,
        DriftKind::CliSurfaceChange,
        DriftKind::UpstreamTestOrChangelog,
    ];
}

/// Whether the scan has completed over every kind. `false` means "did not run" — the
/// state FR-025 requires to be distinguishable from "found nothing".
pub fn run_is_complete(file: &DriftFile) -> bool {
    match &file.last_completed_run {
        None => false,
        Some(run) => {
            let probed: BTreeSet<DriftKind> = run.kinds_probed.iter().copied().collect();
            DriftKind::ALL.iter().all(|k| probed.contains(k))
        }
    }
}

/// Load `observations.json`. A missing file yields the default (never-run) document, so a
/// fixture registry validates without one.
pub fn load_drift(drift_dir: &Path) -> Result<DriftFile, DriftLoadError> {
    let path = drift_dir.join("observations.json");
    if !path.is_file() {
        return Ok(DriftFile::default());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| DriftLoadError {
        path: path.display().to_string(),
        cause: e.to_string(),
    })?;
    serde_json::from_str(&raw).map_err(|e| DriftLoadError {
        path: path.display().to_string(),
        cause: e.to_string(),
    })
}

/// An `observations.json` that could not be read or parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "drift file `{path}` could not be loaded: {cause}. Remedy: fix the record — the drift \
     root is strict JSON and rejects unknown fields at load."
)]
pub struct DriftLoadError {
    pub path: String,
    pub cause: String,
}

// ---------------------------------------------------------------------------
// Upgrade proposal (US6)
// ---------------------------------------------------------------------------

/// The seven-section review bundle that authorizes advancing the stable oracle pin
/// (data-model.md §5, contracts/upgrade-proposal.md).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpgradeProposal {
    pub schema_version: u32,
    pub from_oracle: String,
    pub to_oracle: String,
    pub input_state: InputState,
    pub sections: ProposalSections,
}

/// What the bundle was computed from (FR-027).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputState {
    pub registry_digest: String,
    /// `false` marks a bundle computed against uncommitted registry edits, so a proposal
    /// built on a dirty tree is recognizable as such rather than trusted silently.
    pub worktree_clean: bool,
}

/// All seven sections. Every field is required — `serde` rejects a document missing one,
/// which is FR-030 enforced at the type level rather than by a later check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposalSections {
    pub schema_drift: Section,
    pub specification_drift: Section,
    pub cli_surface_drift: Section,
    pub reference_behavior_drift: Section,
    pub snapshot_differences: Section,
    pub newly_failing_cases: Section,
    pub affected_dispositions: Section,
}

impl ProposalSections {
    /// The seven section names, in bundle order.
    pub const NAMES: &'static [&'static str] = &[
        "schemaDrift",
        "specificationDrift",
        "cliSurfaceDrift",
        "referenceBehaviorDrift",
        "snapshotDifferences",
        "newlyFailingCases",
        "affectedDispositions",
    ];
}

/// One section's findings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Section {
    /// Always `true` in a well-formed bundle; carried explicitly so a hand-edited document
    /// that blanks a section is caught rather than read as clean.
    pub present: bool,
    /// Sorted by a stable key, never by discovery order (FR-031).
    pub entries: Vec<SectionEntry>,
}

impl Section {
    /// An investigated section with no findings — clean, and distinct from absent.
    pub fn clean() -> Section {
        Section {
            present: true,
            entries: Vec::new(),
        }
    }
}

/// One finding within a section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SectionEntry {
    /// The stable sort key — a document path, case id, disposition id, or flag name.
    pub subject: String,
    pub detail: String,
    /// `true` when this entry rests on evidence that was not fully pinned and hermetic —
    /// a canary run, for instance. Such an entry is recorded for information and MUST NOT
    /// be cited as evidence for the upgrade (FR-033).
    #[serde(default)]
    pub informational_only: bool,
}

/// Parse an upgrade-proposal document, keeping the *shape* failure distinguishable from a
/// read failure.
pub fn load_proposal(path: &Path) -> Result<UpgradeProposal, ProposalLoadError> {
    let raw = std::fs::read_to_string(path).map_err(|e| ProposalLoadError {
        path: path.display().to_string(),
        cause: e.to_string(),
    })?;
    serde_json::from_str(&raw).map_err(|e| ProposalLoadError {
        path: path.display().to_string(),
        cause: e.to_string(),
    })
}

/// An upgrade proposal that could not be read or parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "upgrade proposal `{path}` could not be loaded: {cause}. A bundle missing any of the \
     seven sections is incomplete and cannot authorize an upgrade (FR-030)."
)]
pub struct ProposalLoadError {
    pub path: String,
    pub cause: String,
}

/// Render the proposal as deterministic JSON — stable key order, no timestamps, no
/// absolute paths (FR-031).
pub fn render_proposal_json(proposal: &UpgradeProposal) -> String {
    let mut out = serde_json::to_string_pretty(proposal).unwrap_or_else(|_| "{}".to_string());
    out.push('\n');
    out
}

/// Render the proposal as deterministic Markdown for review.
pub fn render_proposal_md(proposal: &UpgradeProposal) -> String {
    let mut out = format!(
        "# Stable oracle upgrade proposal: {} → {}\n\n",
        proposal.from_oracle, proposal.to_oracle
    );
    if !proposal.input_state.worktree_clean {
        out.push_str(
            "> **Computed against a dirty working tree.** The registry had uncommitted \
             edits, so this bundle describes a state that is not in version control.\n\n",
        );
    }
    out.push_str(&format!(
        "Registry digest: `{}`\n\n",
        proposal.input_state.registry_digest
    ));

    let sections: [(&str, &Section); 7] = [
        ("Schema drift", &proposal.sections.schema_drift),
        (
            "Specification drift",
            &proposal.sections.specification_drift,
        ),
        ("CLI-surface drift", &proposal.sections.cli_surface_drift),
        (
            "Reference-behavior drift",
            &proposal.sections.reference_behavior_drift,
        ),
        (
            "Snapshot differences",
            &proposal.sections.snapshot_differences,
        ),
        (
            "Newly failing cases",
            &proposal.sections.newly_failing_cases,
        ),
        (
            "Affected dispositions",
            &proposal.sections.affected_dispositions,
        ),
    ];
    for (title, section) in sections {
        out.push_str(&format!("## {title}\n\n"));
        if section.entries.is_empty() {
            out.push_str("Investigated; nothing found.\n\n");
            continue;
        }
        for entry in &section.entries {
            let marker = if entry.informational_only {
                " *(informational only — not pinned and hermetic)*"
            } else {
                ""
            };
            out.push_str(&format!(
                "- `{}` — {}{}\n",
                entry.subject, entry.detail, marker
            ));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_anchored_to_substance_not_to_the_observation_date() {
        let a = derive_observation_id(DriftKind::SpecCommit, "113500f4", "9f21ab77");
        let b = derive_observation_id(DriftKind::SpecCommit, "113500f4", "9f21ab77");
        assert_eq!(a, b, "the same drift must keep one identity across scans");
        let c = derive_observation_id(DriftKind::SpecCommit, "113500f4", "deadbeef");
        assert_ne!(a, c);
    }

    #[test]
    fn no_drift_and_did_not_run_are_distinguishable() {
        let never_ran = DriftFile::default();
        assert!(
            !run_is_complete(&never_ran),
            "absent run must read as unknown"
        );

        let partial = DriftFile {
            last_completed_run: Some(CompletedRun {
                date: "2026-07-28".into(),
                kinds_probed: vec![DriftKind::SpecCommit],
            }),
            ..Default::default()
        };
        assert!(
            !run_is_complete(&partial),
            "a partial run is not a clean result"
        );

        let complete = DriftFile {
            last_completed_run: Some(CompletedRun {
                date: "2026-07-28".into(),
                kinds_probed: DriftKind::ALL.to_vec(),
            }),
            ..Default::default()
        };
        assert!(run_is_complete(&complete));
    }

    #[test]
    fn a_bundle_missing_a_section_key_does_not_parse() {
        // FR-030 enforced at the type level: `serde` rejects the document rather than
        // defaulting the section to empty, which would read as investigated-and-clean.
        let missing = r#"{
            "schemaVersion": 1, "fromOracle": "0.87.0", "toOracle": "0.88.0",
            "inputState": { "registryDigest": "d", "worktreeClean": true },
            "sections": {
                "schemaDrift": { "present": true, "entries": [] },
                "specificationDrift": { "present": true, "entries": [] },
                "cliSurfaceDrift": { "present": true, "entries": [] },
                "referenceBehaviorDrift": { "present": true, "entries": [] },
                "snapshotDifferences": { "present": true, "entries": [] },
                "newlyFailingCases": { "present": true, "entries": [] }
            }
        }"#;
        assert!(serde_json::from_str::<UpgradeProposal>(missing).is_err());
    }

    #[test]
    fn an_empty_entries_array_is_clean_not_missing() {
        let complete = r#"{
            "schemaVersion": 1, "fromOracle": "0.87.0", "toOracle": "0.88.0",
            "inputState": { "registryDigest": "d", "worktreeClean": true },
            "sections": {
                "schemaDrift": { "present": true, "entries": [] },
                "specificationDrift": { "present": true, "entries": [] },
                "cliSurfaceDrift": { "present": true, "entries": [] },
                "referenceBehaviorDrift": { "present": true, "entries": [] },
                "snapshotDifferences": { "present": true, "entries": [] },
                "newlyFailingCases": { "present": true, "entries": [] },
                "affectedDispositions": { "present": true, "entries": [] }
            }
        }"#;
        let parsed: UpgradeProposal =
            serde_json::from_str(complete).expect("complete bundle parses");
        assert!(parsed.sections.schema_drift.entries.is_empty());
        assert!(parsed.sections.schema_drift.present);
    }

    #[test]
    fn markdown_render_is_stable_across_calls() {
        let proposal = UpgradeProposal {
            schema_version: 1,
            from_oracle: "0.87.0".into(),
            to_oracle: "0.88.0".into(),
            input_state: InputState {
                registry_digest: "abc".into(),
                worktree_clean: true,
            },
            sections: ProposalSections {
                schema_drift: Section::clean(),
                specification_drift: Section::clean(),
                cli_surface_drift: Section::clean(),
                reference_behavior_drift: Section::clean(),
                snapshot_differences: Section::clean(),
                newly_failing_cases: Section::clean(),
                affected_dispositions: Section::clean(),
            },
        };
        assert_eq!(render_proposal_md(&proposal), render_proposal_md(&proposal));
    }
}
