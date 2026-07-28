//! Emit the **execution manifest** — the receipt the container-backed lane produces and
//! `certify` consumes (026-continuous-conformance-certification, US3;
//! contracts/execution-manifest.md).
//!
//! Lives on the live side because only a lane that actually ran cases can honestly report
//! what happened. The consuming side is hermetic (`deacon_conformance::manifest`), so the
//! data flows live → hermetic as committed-shape JSON and never the reverse: the release
//! gate never grows a live dependency.
//!
//! ## Producer obligations, and why each one matters
//!
//! - **Atomic write.** A truncated manifest read by a parallel job would parse as
//!   `V32-incomplete` and block a release for a reason that is not real.
//! - **Record every required case**, including failures and dispositioned exclusions. A
//!   manifest listing only successes is incomplete, not clean — omission must never read
//!   as absence of a problem.
//! - **Hashes computed at execution time**, from the definitions the run actually used.
//!   Re-deriving them later would mask a mid-run edit.
//! - **Emit on failure too.** The manifest is diagnostic; suppressing it on red runs hides
//!   the evidence exactly when it is most needed.

use std::path::Path;

use serde::Serialize;

use crate::HarnessError;

/// What a run recorded for one case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Pass,
    Fail,
    AllowedDifference,
    Excluded,
}

/// One case's recorded result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseRun {
    pub case_id: String,
    pub case_hash: String,
    pub fixture_hash: String,
    pub outcome: Outcome,
    /// Required iff `outcome` is [`Outcome::Excluded`]; the hermetic verifier rejects an
    /// exclusion that names nothing, because an unattributed skip is exactly FR-041(i).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excluded_by: Option<String>,
}

/// Everything the manifest needs beyond the per-case results.
#[derive(Debug, Clone)]
pub struct ManifestInputs {
    /// The revision under test. Pins the manifest to the commit it was produced for, so a
    /// manifest from another revision cannot be presented as evidence for this one.
    pub revision: String,
    pub profile: String,
    pub required_case_count: usize,
    pub runs: Vec<CaseRun>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestDocument<'a> {
    schema_version: u32,
    revision: &'a str,
    profile: &'a str,
    environment: Environment,
    required_case_count: usize,
    cases: &'a [CaseRun],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Environment {
    platform: String,
    arch: String,
    container_engine: String,
    container_engine_version: String,
    compose_version: String,
}

/// The environment identity of the running host.
///
/// Probed here — unlike in the certifier, which must run with no engine at all. This is
/// the right side of that split: the lane that ran the containers is the only one that can
/// truthfully say which engine ran them.
fn probe_environment() -> Environment {
    Environment {
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        container_engine: std::env::var("DEACON_CONTAINER_RUNTIME")
            .unwrap_or_else(|_| "docker".to_string()),
        container_engine_version: probe_version(&["docker", "--version"]),
        compose_version: probe_version(&["docker", "compose", "version", "--short"]),
    }
}

/// Best-effort version probe. A failure records `"unknown"` rather than aborting: the
/// version is an *informational* provenance field, not a staleness signal, so an
/// unavailable version must not cost the run its receipt.
fn probe_version(argv: &[&str]) -> String {
    let Some((program, args)) = argv.split_first() else {
        return "unknown".to_string();
    };
    match std::process::Command::new(program).args(args).output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "unknown".to_string(),
    }
}

/// Write the execution manifest atomically.
pub fn emit_manifest(path: &Path, inputs: &ManifestInputs) -> Result<(), HarnessError> {
    let document = ManifestDocument {
        schema_version: 1,
        revision: &inputs.revision,
        profile: &inputs.profile,
        environment: probe_environment(),
        required_case_count: inputs.required_case_count,
        cases: &inputs.runs,
    };
    let mut rendered =
        serde_json::to_string_pretty(&document).map_err(|e| HarnessError::Report {
            cause: format!("execution manifest `{}`: {e}", path.display()),
        })?;
    rendered.push('\n');

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| HarnessError::Report {
            cause: format!("execution manifest directory `{}`: {e}", parent.display()),
        })?;
    }
    // Temp file + rename: a shorter payload over a longer file would otherwise leave
    // trailing bytes, and a concurrent reader would see a document that parses as
    // incomplete.
    let temp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&temp, rendered).map_err(|e| HarnessError::Report {
        cause: format!("execution manifest temp file `{}`: {e}", temp.display()),
    })?;
    std::fs::rename(&temp, path).map_err(|e| HarnessError::Report {
        cause: format!("execution manifest rename to `{}`: {e}", path.display()),
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_records_failures_and_exclusions_not_only_successes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("execution-manifest.json");
        emit_manifest(
            &path,
            &ManifestInputs {
                revision: "abc".into(),
                profile: "prof".into(),
                required_case_count: 3,
                runs: vec![
                    CaseRun {
                        case_id: "case-a".into(),
                        case_hash: "h".into(),
                        fixture_hash: "f".into(),
                        outcome: Outcome::Pass,
                        excluded_by: None,
                    },
                    CaseRun {
                        case_id: "case-b".into(),
                        case_hash: "h".into(),
                        fixture_hash: "f".into(),
                        outcome: Outcome::Fail,
                        excluded_by: None,
                    },
                    CaseRun {
                        case_id: "case-c".into(),
                        case_hash: "h".into(),
                        fixture_hash: "f".into(),
                        outcome: Outcome::Excluded,
                        excluded_by: Some("odp-x".into()),
                    },
                ],
            },
        )
        .expect("write");

        let raw = std::fs::read_to_string(&path).expect("read");
        let doc: serde_json::Value = serde_json::from_str(&raw).expect("parse");
        assert_eq!(doc["cases"].as_array().expect("array").len(), 3);
        assert_eq!(doc["requiredCaseCount"], 3);
    }

    #[test]
    fn the_manifest_leaves_no_temp_file_behind() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("execution-manifest.json");
        emit_manifest(
            &path,
            &ManifestInputs {
                revision: "abc".into(),
                profile: "prof".into(),
                required_case_count: 0,
                runs: vec![],
            },
        )
        .expect("write");
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read dir")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries, vec!["execution-manifest.json".to_string()]);
    }
}
