//! The **execution manifest** — the receipt proving container-backed execution occurred
//! (026-continuous-conformance-certification, US2; contracts/execution-manifest.md).
//!
//! ## Why this type exists
//!
//! Certification is hermetic (FR-033a): it never installs, resolves, or invokes the
//! reference implementation, and needs no container engine or network. But FR-041(h)
//! requires a release to be blocked when the required container-backed execution did not
//! happen. A hermetic process cannot *observe* whether Docker ran — it can verify a
//! receipt. This module is that receipt and its verification.
//!
//! ## What makes the receipt non-forgeable in the ways that matter
//!
//! Two fields do the work. `revision` pins the manifest to the commit it was produced for,
//! so a manifest from another revision cannot be presented as evidence for this one
//! (FR-033c). Per-case `caseHash`/`fixtureHash` pin it to the case definitions that were
//! actually executed, so a manifest that predates a case edit is *stale* rather than
//! silently accepted (FR-033d).
//!
//! This is deliberately not cryptographic attestation. The failure being prevented is a
//! stale or mismatched manifest passing unnoticed, and recorded-hash comparison catches
//! that. An adversary with write access to the artifact store is outside the threat model,
//! and guarding against one would add key management for no gain against the real risk.
//!
//! ## What the manifest is *not*
//!
//! It is not committed evidence. Snapshots are reviewed artifacts under
//! `conformance/snapshots/`; a manifest is a per-run receipt, regenerated every run.
//! Conflating them would mean every continuous-integration run wants to write the reviewed
//! tree — the pressure FR-055 exists to remove. And it is not a substitute for snapshot
//! freshness: both obligations hold independently (FR-033e).

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::validate::Violation;

/// The execution manifest document (data-model.md §3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionManifest {
    pub schema_version: u32,
    /// The full commit the run was produced for (FR-033c).
    pub revision: String,
    /// The profile the run exercised.
    pub profile: String,
    pub environment: ManifestEnvironment,
    /// How many cases the producer considered required. Recorded so an over-narrow run is
    /// visible even when every listed case passed.
    pub required_case_count: usize,
    pub cases: Vec<ManifestCase>,
}

/// The environment identity a run was produced under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManifestEnvironment {
    pub platform: String,
    pub arch: String,
    pub container_engine: String,
    pub container_engine_version: String,
    pub compose_version: String,
}

/// One case's recorded outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManifestCase {
    pub case_id: String,
    pub case_hash: String,
    pub fixture_hash: String,
    pub outcome: CaseOutcome,
    /// The disposition id that excluded this case; required iff `outcome` is
    /// [`CaseOutcome::Excluded`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excluded_by: Option<String>,
}

/// The closed set of recorded outcomes.
///
/// Closed on purpose: FR-041(i) blocks a release on "a case whose result is neither pass,
/// fail, nor an explicitly dispositioned exclusion". A open-ended string would let a
/// producer invent a fourth state that reads as neither success nor failure, which is
/// precisely the silent skip the requirement forbids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaseOutcome {
    Pass,
    Fail,
    AllowedDifference,
    Excluded,
}

/// Why manifest verification rejected a manifest. Each variant is a distinct **V35**
/// sub-case so a failure names its own cause (FR-042).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestDefect {
    /// No manifest at the expected path.
    Absent { path: String },
    /// The file exists but does not parse.
    Malformed { path: String, cause: String },
    /// A required case id is missing from `cases`.
    Incomplete { case_id: String },
    /// `revision` names a different commit, or the environment does not match the profile.
    Revision { expected: String, found: String },
    /// A recorded hash no longer matches the currently computed one.
    Stale {
        case_id: String,
        field: &'static str,
        recorded: String,
        current: String,
    },
    /// An `excluded` outcome whose `excludedBy` does not resolve, or is absent.
    Unaccounted { case_id: String, cause: String },
}

impl ManifestDefect {
    /// The `V35-<sub-case>` code this defect reports under.
    pub fn code(&self) -> &'static str {
        match self {
            ManifestDefect::Absent { .. } => "V35-absent",
            ManifestDefect::Malformed { .. } => "V35-malformed",
            ManifestDefect::Incomplete { .. } => "V35-incomplete",
            ManifestDefect::Revision { .. } => "V35-revision",
            ManifestDefect::Stale { .. } => "V35-stale",
            ManifestDefect::Unaccounted { .. } => "V35-unaccounted",
        }
    }

    /// The offending record — a case id where one exists, else the manifest path.
    pub fn record(&self) -> String {
        match self {
            ManifestDefect::Absent { path } | ManifestDefect::Malformed { path, .. } => {
                path.clone()
            }
            ManifestDefect::Incomplete { case_id }
            | ManifestDefect::Stale { case_id, .. }
            | ManifestDefect::Unaccounted { case_id, .. } => case_id.clone(),
            ManifestDefect::Revision { .. } => "execution-manifest".to_string(),
        }
    }

    /// A precise, remedy-bearing message (constitution IV).
    pub fn message(&self) -> String {
        match self {
            ManifestDefect::Absent { path } => format!(
                "no execution manifest at `{path}`. The container-backed lane did not run, \
                 or its artifact was not made available. Remedy: run the `pr-docker` \
                 profile on this revision, or fetch the manifest artifact."
            ),
            ManifestDefect::Malformed { path, cause } => format!(
                "execution manifest `{path}` does not parse: {cause}. Remedy: re-run the \
                 container-backed lane — a malformed receipt is not evidence."
            ),
            ManifestDefect::Incomplete { case_id } => format!(
                "required case `{case_id}` is absent from the execution manifest. A \
                 manifest that lists only some required cases is incomplete, not clean — \
                 omission must never read as absence of a problem."
            ),
            ManifestDefect::Revision { expected, found } => format!(
                "execution manifest was produced for `{found}` but certification is for \
                 `{expected}`. A manifest from another revision is not evidence for this \
                 one (FR-033c). Remedy: re-run the container-backed lane on this revision."
            ),
            ManifestDefect::Stale {
                case_id,
                field,
                recorded,
                current,
            } => format!(
                "execution manifest recorded `{field}` `{recorded}` for case `{case_id}`, \
                 but the current value is `{current}`. The case changed after the run, so \
                 the recorded outcome describes a case that no longer exists. Remedy: \
                 re-run the container-backed lane."
            ),
            ManifestDefect::Unaccounted { case_id, cause } => format!(
                "case `{case_id}` has an unaccounted outcome: {cause}. A result that is \
                 neither pass, fail, nor an explicitly dispositioned exclusion is a silent \
                 skip (FR-041(i))."
            ),
        }
    }

    /// Render as a `V35` violation.
    pub fn to_violation(&self) -> Violation {
        Violation::v35(
            self.record(),
            format!("[{}] {}", self.code(), self.message()),
        )
    }
}

/// What certification needs to know about the run it is certifying.
#[derive(Debug, Clone)]
pub struct ManifestExpectation {
    /// The revision under certification.
    pub revision: String,
    /// The profile under certification.
    pub profile: String,
    /// The case ids the container-backed lane was required to execute, with their
    /// currently computed hashes.
    pub required: Vec<RequiredCase>,
    /// Disposition ids an `excluded` outcome may legitimately name.
    pub resolvable_dispositions: BTreeSet<String>,
}

/// One required case and its current hashes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredCase {
    pub case_id: String,
    pub case_hash: String,
    pub fixture_hash: String,
}

/// Load and verify an execution manifest.
///
/// **Evaluates every check rather than short-circuiting** (FR-043): a maintainer reading a
/// blocked release must see the whole list, not the first line. The only early return is
/// when there is no manifest to check at all.
pub fn verify_manifest(path: &Path, expected: &ManifestExpectation) -> Vec<ManifestDefect> {
    let display = path.display().to_string();
    if !path.is_file() {
        return vec![ManifestDefect::Absent { path: display }];
    }
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) => {
            return vec![ManifestDefect::Malformed {
                path: display,
                cause: e.to_string(),
            }];
        }
    };
    let manifest: ExecutionManifest = match serde_json::from_str(&raw) {
        Ok(m) => m,
        Err(e) => {
            return vec![ManifestDefect::Malformed {
                path: display,
                cause: e.to_string(),
            }];
        }
    };
    verify_loaded(&manifest, expected)
}

/// Verify an already-parsed manifest. Split out so tests can construct one directly.
pub fn verify_loaded(
    manifest: &ExecutionManifest,
    expected: &ManifestExpectation,
) -> Vec<ManifestDefect> {
    let mut defects = Vec::new();

    if manifest.revision != expected.revision {
        defects.push(ManifestDefect::Revision {
            expected: expected.revision.clone(),
            found: manifest.revision.clone(),
        });
    }
    if manifest.profile != expected.profile {
        defects.push(ManifestDefect::Revision {
            expected: expected.profile.clone(),
            found: manifest.profile.clone(),
        });
    }

    for required in &expected.required {
        let Some(recorded) = manifest
            .cases
            .iter()
            .find(|c| c.case_id == required.case_id)
        else {
            defects.push(ManifestDefect::Incomplete {
                case_id: required.case_id.clone(),
            });
            continue;
        };
        if recorded.case_hash != required.case_hash {
            defects.push(ManifestDefect::Stale {
                case_id: required.case_id.clone(),
                field: "caseHash",
                recorded: recorded.case_hash.clone(),
                current: required.case_hash.clone(),
            });
        }
        if recorded.fixture_hash != required.fixture_hash {
            defects.push(ManifestDefect::Stale {
                case_id: required.case_id.clone(),
                field: "fixtureHash",
                recorded: recorded.fixture_hash.clone(),
                current: required.fixture_hash.clone(),
            });
        }
    }

    for recorded in &manifest.cases {
        match (&recorded.outcome, &recorded.excluded_by) {
            (CaseOutcome::Excluded, None) => defects.push(ManifestDefect::Unaccounted {
                case_id: recorded.case_id.clone(),
                cause: "outcome is `excluded` but no `excludedBy` disposition is named".to_string(),
            }),
            (CaseOutcome::Excluded, Some(id)) if !expected.resolvable_dispositions.contains(id) => {
                defects.push(ManifestDefect::Unaccounted {
                    case_id: recorded.case_id.clone(),
                    cause: format!("`excludedBy` names `{id}`, which does not resolve"),
                })
            }
            (outcome, Some(id)) if !matches!(outcome, CaseOutcome::Excluded) => {
                defects.push(ManifestDefect::Unaccounted {
                    case_id: recorded.case_id.clone(),
                    cause: format!(
                        "names `excludedBy` `{id}` with outcome `{outcome:?}`; only an \
                         `excluded` outcome may carry one"
                    ),
                })
            }
            _ => {}
        }
    }

    defects
}

/// The case ids the manifest recorded as failing. These block certification as ordinary
/// failing cases, **not** as manifest-integrity defects.
///
/// Keeping the two distinct matters: "the evidence is malformed" and "the evidence says
/// deacon diverged" need different fixes, and a maintainer reading a blocked release must
/// be able to tell which they have.
pub fn failing_cases(manifest: &ExecutionManifest) -> Vec<String> {
    manifest
        .cases
        .iter()
        .filter(|c| c.outcome == CaseOutcome::Fail)
        .map(|c| c.case_id.clone())
        .collect()
}

/// The case ids the manifest accounted for in any way — the executed set the
/// runner-omission reconciliation compares against the applicable set (FR-041(d)).
pub fn accounted_cases(manifest: &ExecutionManifest) -> BTreeSet<String> {
    manifest.cases.iter().map(|c| c.case_id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> ManifestEnvironment {
        ManifestEnvironment {
            platform: "linux".into(),
            arch: "x86_64".into(),
            container_engine: "docker".into(),
            container_engine_version: "27.3.1".into(),
            compose_version: "2.29.7".into(),
        }
    }

    fn manifest(cases: Vec<ManifestCase>) -> ExecutionManifest {
        ExecutionManifest {
            schema_version: 1,
            revision: "abc123".into(),
            profile: "prof-linux-amd64-docker-0870".into(),
            environment: env(),
            required_case_count: cases.len(),
            cases,
        }
    }

    fn case(id: &str, outcome: CaseOutcome) -> ManifestCase {
        ManifestCase {
            case_id: id.into(),
            case_hash: "h1".into(),
            fixture_hash: "f1".into(),
            outcome,
            excluded_by: None,
        }
    }

    fn expectation(required: &[&str]) -> ManifestExpectation {
        ManifestExpectation {
            revision: "abc123".into(),
            profile: "prof-linux-amd64-docker-0870".into(),
            required: required
                .iter()
                .map(|id| RequiredCase {
                    case_id: (*id).to_string(),
                    case_hash: "h1".into(),
                    fixture_hash: "f1".into(),
                })
                .collect(),
            resolvable_dispositions: BTreeSet::from(["odp-known".to_string()]),
        }
    }

    #[test]
    fn a_complete_matching_manifest_is_clean() {
        let m = manifest(vec![case("case-a", CaseOutcome::Pass)]);
        assert_eq!(verify_loaded(&m, &expectation(&["case-a"])), vec![]);
    }

    #[test]
    fn absent_manifest_is_its_own_sub_case() {
        let defects = verify_manifest(Path::new("/nonexistent/manifest.json"), &expectation(&[]));
        assert_eq!(defects.len(), 1);
        assert_eq!(defects[0].code(), "V35-absent");
    }

    #[test]
    fn a_missing_required_case_is_incomplete_not_clean() {
        let m = manifest(vec![case("case-a", CaseOutcome::Pass)]);
        let defects = verify_loaded(&m, &expectation(&["case-a", "case-b"]));
        assert_eq!(defects.len(), 1);
        assert_eq!(defects[0].code(), "V35-incomplete");
        assert_eq!(defects[0].record(), "case-b");
    }

    #[test]
    fn a_manifest_from_another_revision_is_rejected() {
        let mut m = manifest(vec![case("case-a", CaseOutcome::Pass)]);
        m.revision = "deadbeef".into();
        let defects = verify_loaded(&m, &expectation(&["case-a"]));
        assert!(defects.iter().any(|d| d.code() == "V35-revision"));
    }

    #[test]
    fn a_drifted_hash_is_stale() {
        let mut m = manifest(vec![case("case-a", CaseOutcome::Pass)]);
        m.cases[0].case_hash = "old".into();
        let defects = verify_loaded(&m, &expectation(&["case-a"]));
        assert_eq!(defects.len(), 1);
        assert_eq!(defects[0].code(), "V35-stale");
    }

    #[test]
    fn an_excluded_outcome_needs_a_resolvable_disposition() {
        let mut m = manifest(vec![case("case-a", CaseOutcome::Excluded)]);
        let defects = verify_loaded(&m, &expectation(&["case-a"]));
        assert_eq!(defects[0].code(), "V35-unaccounted");

        m.cases[0].excluded_by = Some("odp-missing".into());
        let defects = verify_loaded(&m, &expectation(&["case-a"]));
        assert_eq!(defects[0].code(), "V35-unaccounted");

        m.cases[0].excluded_by = Some("odp-known".into());
        assert_eq!(verify_loaded(&m, &expectation(&["case-a"])), vec![]);
    }

    #[test]
    fn a_failing_case_is_not_a_manifest_integrity_defect() {
        // "the evidence is malformed" and "the evidence says deacon diverged" are
        // different problems with different fixes.
        let m = manifest(vec![case("case-a", CaseOutcome::Fail)]);
        assert_eq!(verify_loaded(&m, &expectation(&["case-a"])), vec![]);
        assert_eq!(failing_cases(&m), vec!["case-a".to_string()]);
    }

    #[test]
    fn every_defect_is_reported_not_just_the_first() {
        let mut m = manifest(vec![case("case-a", CaseOutcome::Pass)]);
        m.revision = "wrong".into();
        m.cases[0].case_hash = "old".into();
        let defects = verify_loaded(&m, &expectation(&["case-a", "case-b"]));
        let codes: BTreeSet<_> = defects.iter().map(|d| d.code()).collect();
        assert!(codes.contains("V35-revision"));
        assert!(codes.contains("V35-stale"));
        assert!(codes.contains("V35-incomplete"));
    }
}
