//! Assemble the seven-section **oracle upgrade proposal**
//! (026-continuous-conformance-certification, US6; contracts/upgrade-proposal.md).
//!
//! ## Why assembly is live and validation is hermetic
//!
//! Reference-behavior drift and newly-failing cases cannot be determined without running
//! the candidate reference, so assembly needs network and Docker. Completeness is a
//! property of the *document*, so validation must not — otherwise rejecting an incomplete
//! bundle (FR-030) would require provisioning two oracles, and the rejection would leave
//! the pull-request lane where it belongs (research D10).
//!
//! ## Present-but-empty vs missing
//!
//! Every section this module emits is `present: true`. `entries: []` means investigated
//! and clean. There is no code path that emits a section marked absent, because "not
//! investigated" is a state a *generator* must never claim — it is what a missing key in a
//! hand-edited bundle means, and the type system already rejects that.
//!
//! ## Canary evidence is admitted, not trusted
//!
//! An entry resting on a run that was not fully pinned and hermetic is marked
//! `informationalOnly` (FR-033). It appears in the bundle — a reviewer should see it — but
//! it is visibly not evidence, so it cannot quietly back the decision.

use std::path::Path;

use deacon_conformance::drift::{
    InputState, ProposalSections, Section, SectionEntry, UpgradeProposal,
};

use crate::HarnessError;

/// What the builder needs beyond the two versions.
#[derive(Debug, Clone)]
pub struct ProposalInputs {
    pub from_oracle: String,
    pub to_oracle: String,
    /// The repository root the pinned sources are read from.
    pub repo_root: std::path::PathBuf,
    /// Findings observed against the candidate reference. Each carries whether its run was
    /// fully pinned and hermetic; the ones that were not become `informationalOnly`.
    pub reference_findings: Vec<ReferenceFinding>,
    /// Cases that pass today and fail against the candidate.
    pub newly_failing: Vec<String>,
}

/// One observation about how the candidate reference behaves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceFinding {
    pub subject: String,
    pub detail: String,
    /// `true` only when every input was pinned by immutable identifier and the run was
    /// hermetic. Anything else is informational (FR-033).
    pub pinned_and_hermetic: bool,
}

/// Build the complete bundle.
pub fn build_proposal(inputs: &ProposalInputs) -> Result<UpgradeProposal, HarnessError> {
    if inputs.from_oracle == inputs.to_oracle {
        return Err(HarnessError::Report {
            cause: format!(
                "cannot propose an upgrade from `{}` to itself",
                inputs.from_oracle
            ),
        });
    }
    Ok(UpgradeProposal {
        schema_version: 1,
        from_oracle: inputs.from_oracle.clone(),
        to_oracle: inputs.to_oracle.clone(),
        input_state: input_state(&inputs.repo_root),
        sections: ProposalSections {
            schema_drift: schema_drift(&inputs.repo_root),
            specification_drift: specification_drift(&inputs.repo_root),
            cli_surface_drift: cli_surface_drift(&inputs.from_oracle, &inputs.to_oracle),
            reference_behavior_drift: reference_behavior_drift(&inputs.reference_findings),
            snapshot_differences: snapshot_differences(&inputs.repo_root),
            newly_failing_cases: newly_failing_cases(&inputs.newly_failing),
            affected_dispositions: affected_dispositions(&inputs.repo_root),
        },
    })
}

/// Record what the bundle was computed from (FR-027).
///
/// A dirty working tree is recorded rather than refused: a maintainer exploring an upgrade
/// against local edits is doing something reasonable, and what matters is that the
/// resulting bundle says so instead of reading as a statement about `HEAD`.
fn input_state(repo_root: &Path) -> InputState {
    let digest = registry_digest(repo_root);
    let worktree_clean = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["status", "--porcelain", "--", "conformance/registry"])
        .output()
        .map(|o| o.status.success() && o.stdout.is_empty())
        .unwrap_or(false);
    InputState {
        registry_digest: digest,
        worktree_clean,
    }
}

/// A content digest over the registry, so two bundles computed from the same records are
/// visibly computed from the same records.
fn registry_digest(repo_root: &Path) -> String {
    use sha2::{Digest, Sha256};
    let mut files = Vec::new();
    collect(&repo_root.join("conformance").join("registry"), &mut files);
    files.sort();
    let mut hasher = Sha256::new();
    for file in files {
        if let Ok(bytes) = std::fs::read(&file) {
            hasher.update(
                file.strip_prefix(repo_root)
                    .unwrap_or(&file)
                    .to_string_lossy()
                    .as_bytes(),
            );
            hasher.update(&bytes);
        }
    }
    format!("{:x}", hasher.finalize())
}

fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// The vendored documents a pin change would put at stake.
fn documents_under(dir: &Path) -> Vec<String> {
    let Ok(raw) = std::fs::read_to_string(dir.join("manifest.json")) else {
        return Vec::new();
    };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Vec::new();
    };
    let mut keys: Vec<String> = doc
        .get("documents")
        .and_then(|d| d.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

fn schema_drift(repo_root: &Path) -> Section {
    let dir = repo_root
        .join("conformance")
        .join("schemas")
        .join(deacon_conformance::CURRENT_SCHEMA_PIN);
    section(
        documents_under(&dir)
            .into_iter()
            .map(|doc| SectionEntry {
                subject: doc.clone(),
                detail: format!(
                    "vendored at schema pin `{}`; re-vendor and re-run `inventory diff` \
                     before accepting",
                    deacon_conformance::CURRENT_SCHEMA_PIN
                ),
                informational_only: false,
            })
            .collect(),
    )
}

fn specification_drift(repo_root: &Path) -> Section {
    let dir = repo_root
        .join("conformance")
        .join("spec")
        .join(deacon_conformance::CURRENT_SPEC_PIN);
    section(
        documents_under(&dir)
            .into_iter()
            .map(|doc| SectionEntry {
                subject: doc.clone(),
                detail: format!(
                    "vendored at spec pin `{}`; re-vendor and re-run `clause diff` before \
                     accepting",
                    deacon_conformance::CURRENT_SPEC_PIN
                ),
                informational_only: false,
            })
            .collect(),
    )
}

fn cli_surface_drift(from: &str, to: &str) -> Section {
    section(vec![SectionEntry {
        subject: "cli-surface".to_string(),
        detail: format!(
            "compare the `--help` surface of {from} and {to}; a changed flag, subcommand, \
             or output shape needs a behavior record before the pin moves"
        ),
        informational_only: false,
    }])
}

fn reference_behavior_drift(findings: &[ReferenceFinding]) -> Section {
    section(
        findings
            .iter()
            .map(|f| SectionEntry {
                subject: f.subject.clone(),
                detail: f.detail.clone(),
                // FR-033: a run that was not fully pinned and hermetic informs the
                // decision but cannot back it.
                informational_only: !f.pinned_and_hermetic,
            })
            .collect(),
    )
}

fn newly_failing_cases(cases: &[String]) -> Section {
    section(
        cases
            .iter()
            .map(|case| SectionEntry {
                subject: case.clone(),
                detail: "passes against the current pin and fails against the candidate"
                    .to_string(),
                informational_only: false,
            })
            .collect(),
    )
}

fn snapshot_differences(repo_root: &Path) -> Section {
    // Every committed snapshot records the oracle version it was captured against, so a
    // pin move makes each one stale by construction. Listing them is the reviewer's work
    // item: each needs re-recording through the reviewed record path (FR-032).
    let snapshots = repo_root.join("conformance").join("snapshots");
    let mut entries = Vec::new();
    if let Ok(platforms) = std::fs::read_dir(&snapshots) {
        for platform in platforms.flatten() {
            let platform_name = platform.file_name().to_string_lossy().to_string();
            let Ok(cases) = std::fs::read_dir(platform.path()) else {
                continue;
            };
            for case in cases.flatten() {
                if !case.path().join("provenance.json").is_file() {
                    continue;
                }
                entries.push(SectionEntry {
                    subject: format!("{platform_name}/{}", case.file_name().to_string_lossy()),
                    detail: "records the current oracle version in its provenance; a pin \
                             move makes it stale and it must be re-recorded through the \
                             reviewed record path"
                        .to_string(),
                    informational_only: false,
                });
            }
        }
    }
    section(entries)
}

fn affected_dispositions(repo_root: &Path) -> Section {
    // A waiver characterizes a divergence against a specific reference version; moving the
    // pin invalidates that characterization until someone re-confirms it.
    let waivers = repo_root
        .join("conformance")
        .join("registry")
        .join("waivers");
    let mut entries = Vec::new();
    if let Ok(files) = std::fs::read_dir(&waivers) {
        for file in files.flatten() {
            let name = file.file_name().to_string_lossy().to_string();
            let Some(id) = name.strip_suffix(".json") else {
                continue;
            };
            entries.push(SectionEntry {
                subject: id.to_string(),
                detail: "characterizes a divergence against the current pin; re-confirm it \
                         reproduces against the candidate before accepting"
                    .to_string(),
                informational_only: false,
            });
        }
    }
    section(entries)
}

/// Build a section with entries sorted by subject.
///
/// Sorting here rather than at each call site is what makes FR-031's byte-reproducibility
/// a property of the type rather than a discipline seven builders have to remember.
fn section(mut entries: Vec<SectionEntry>) -> Section {
    entries.sort_by(|a, b| a.subject.cmp(&b.subject));
    Section {
        present: true,
        entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(root: &Path) -> ProposalInputs {
        ProposalInputs {
            from_oracle: "0.87.0".into(),
            to_oracle: "0.88.0".into(),
            repo_root: root.to_path_buf(),
            reference_findings: vec![],
            newly_failing: vec![],
        }
    }

    #[test]
    fn every_section_is_present_even_when_empty() {
        let dir = tempfile::tempdir().expect("temp dir");
        let bundle = build_proposal(&inputs(dir.path())).expect("builds");
        let s = &bundle.sections;
        for present in [
            s.schema_drift.present,
            s.specification_drift.present,
            s.cli_surface_drift.present,
            s.reference_behavior_drift.present,
            s.snapshot_differences.present,
            s.newly_failing_cases.present,
            s.affected_dispositions.present,
        ] {
            assert!(present, "a generator must never emit an absent section");
        }
    }

    #[test]
    fn entries_are_sorted_so_the_bundle_regenerates_identically() {
        let sorted = section(vec![
            SectionEntry {
                subject: "z".into(),
                detail: "d".into(),
                informational_only: false,
            },
            SectionEntry {
                subject: "a".into(),
                detail: "d".into(),
                informational_only: false,
            },
        ]);
        assert_eq!(
            sorted
                .entries
                .iter()
                .map(|e| e.subject.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "z"]
        );
    }

    #[test]
    fn a_non_hermetic_finding_is_marked_informational_only() {
        let section = reference_behavior_drift(&[
            ReferenceFinding {
                subject: "bhv-a".into(),
                detail: "observed against a pinned, hermetic run".into(),
                pinned_and_hermetic: true,
            },
            ReferenceFinding {
                subject: "bhv-b".into(),
                detail: "observed against an unpinned canary".into(),
                pinned_and_hermetic: false,
            },
        ]);
        assert!(!section.entries[0].informational_only);
        assert!(
            section.entries[1].informational_only,
            "canary evidence must not silently back an upgrade (FR-033)"
        );
    }

    #[test]
    fn an_upgrade_to_the_same_version_is_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut i = inputs(dir.path());
        i.to_oracle = i.from_oracle.clone();
        assert!(build_proposal(&i).is_err());
    }

    #[test]
    fn the_bundle_is_byte_identical_across_builds() {
        let dir = tempfile::tempdir().expect("temp dir");
        let a = build_proposal(&inputs(dir.path())).expect("builds");
        let b = build_proposal(&inputs(dir.path())).expect("builds");
        assert_eq!(
            deacon_conformance::drift::render_proposal_json(&a),
            deacon_conformance::drift::render_proposal_json(&b)
        );
    }
}
