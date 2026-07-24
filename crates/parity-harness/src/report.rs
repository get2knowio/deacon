//! Per-binary run-report fragments with atomic writes (research D8; data-model §5).
//!
//! nextest runs test binaries in parallel with no ordering, so a shared report
//! file would race. Each live parity binary instead writes ONE fragment to
//! `<report_root>/report/<binary>.json`; the `parity-report` aggregator later
//! folds the fragments into the run report and checks completeness. Failure to
//! write a fragment is [`HarnessError::Report`], which the caller MUST propagate
//! as a test failure — a run whose result cannot be recorded is not a passing run.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::HarnessError;
use crate::oracle::{OracleSource, VerifiedOracle};

/// Report mode. Only `Live` exists today; the field is mandatory so any future
/// replay mode is visibly distinct in every fragment (FR-017).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Live,
}

/// The oracle a fragment was produced against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleInfo {
    pub version: String,
    pub path: String,
    pub source: OracleSource,
}

impl From<&VerifiedOracle> for OracleInfo {
    fn from(v: &VerifiedOracle) -> Self {
        OracleInfo {
            version: v.version.clone(),
            path: v.path.display().to_string(),
            source: v.source,
        }
    }
}

/// The outcome of one compared case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Pass,
    PassWaived,
    Fail,
}

/// Cause of a failing case (required iff `outcome == Fail`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Cause {
    Divergence,
    OracleFailure,
    OracleTimeout,
    MalformedOutput,
    Normalization,
    FixtureMissing,
    DockerMissing,
}

/// Report-relative paths to the four preserved raw outputs for a case (FR-020).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawPaths {
    pub deacon_stdout: String,
    pub deacon_stderr: String,
    pub oracle_stdout: String,
    pub oracle_stderr: String,
}

/// One compared case's result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseResult {
    pub case: String,
    pub outcome: Outcome,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cause: Option<Cause>,
    pub waivers_applied: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub diff_summary: Option<String>,
    pub raw: RawPaths,
}

impl CaseResult {
    /// A clean pass.
    pub fn pass(case: impl Into<String>, raw: RawPaths) -> Self {
        CaseResult {
            case: case.into(),
            outcome: Outcome::Pass,
            cause: None,
            waivers_applied: Vec::new(),
            diff_summary: None,
            raw,
        }
    }

    /// A pass justified by one or more active waivers.
    pub fn pass_waived(
        case: impl Into<String>,
        waivers_applied: Vec<String>,
        raw: RawPaths,
    ) -> Self {
        CaseResult {
            case: case.into(),
            outcome: Outcome::PassWaived,
            cause: None,
            waivers_applied,
            diff_summary: None,
            raw,
        }
    }

    /// A failure with a specific cause.
    pub fn fail(
        case: impl Into<String>,
        cause: Cause,
        diff_summary: Option<String>,
        raw: RawPaths,
    ) -> Self {
        CaseResult {
            case: case.into(),
            outcome: Outcome::Fail,
            cause: Some(cause),
            waivers_applied: Vec::new(),
            diff_summary,
            raw,
        }
    }

    /// Schema invariants: `fail` requires a cause; `pass-waived` requires at least
    /// one waiver id.
    fn validate(&self) -> Result<(), String> {
        match self.outcome {
            Outcome::Fail if self.cause.is_none() => {
                Err(format!("case `{}`: fail without a cause", self.case))
            }
            Outcome::PassWaived if self.waivers_applied.is_empty() => Err(format!(
                "case `{}`: pass-waived without any waiver id",
                self.case
            )),
            _ => Ok(()),
        }
    }
}

/// A registered case that was not run, with the reason (the aggregator treats an
/// unexplained omission as failure).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Omission {
    pub case: String,
    pub reason: String,
}

/// One test binary's run-report fragment (contracts/report-schema.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportFragment {
    pub binary: String,
    pub oracle: OracleInfo,
    pub mode: Mode,
    pub started: String,
    pub finished: String,
    pub cases: Vec<CaseResult>,
    pub omitted: Vec<Omission>,
}

impl ReportFragment {
    /// Build a live fragment.
    pub fn new(
        binary: impl Into<String>,
        oracle: OracleInfo,
        started: String,
        finished: String,
        cases: Vec<CaseResult>,
        omitted: Vec<Omission>,
    ) -> Self {
        ReportFragment {
            binary: binary.into(),
            oracle,
            mode: Mode::Live,
            started,
            finished,
            cases,
            omitted,
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.binary.is_empty() {
            return Err("fragment has an empty binary name".to_string());
        }
        for case in &self.cases {
            case.validate()?;
        }
        for omission in &self.omitted {
            if omission.reason.is_empty() {
                return Err(format!("omitted case `{}` has no reason", omission.case));
            }
        }
        Ok(())
    }

    /// Serialize and write this fragment atomically to
    /// `<report_root>/report/<binary>.json`, returning the absolute path written.
    /// A write failure is [`HarnessError::Report`] — the caller fails the test.
    pub async fn write(&self) -> Result<PathBuf, HarnessError> {
        self.write_under(&crate::report_root()).await
    }

    /// As [`write`], but under an explicit report root (for tests / custom dirs).
    pub async fn write_under(
        &self,
        report_root: &std::path::Path,
    ) -> Result<PathBuf, HarnessError> {
        self.validate()
            .map_err(|cause| HarnessError::Report { cause })?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| HarnessError::Report {
            cause: format!("could not serialize fragment for `{}`: {e}", self.binary),
        })?;
        let path = report_root
            .join("report")
            .join(format!("{}.json", self.binary));
        crate::atomic_write(&path, &bytes).await?;
        Ok(path)
    }
}

/// Current UTC time as an RFC3339 second-precision `Z` timestamp.
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

// ===========================================================================
// Declarative conformance runner verdict report (T024, contract runner-cli.md)
// ===========================================================================

/// The deterministic verdict report: a single JSON document on stdout listing every
/// case's per-channel verdict (contract runner-cli.md). It carries NO timestamps and NO
/// absolute paths (paths are tokenized by normalization), and records are in declaration
/// order (`Vec`, never `BTreeMap`) — so the body is byte-stable across runs (VI output
/// contract, T018). Logs/progress go to stderr via `tracing`, never into this document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerdictReport {
    /// Report schema version.
    pub schema_version: u32,
    /// The `NORMALIZER_VERSION` the verdicts were produced under (FR-030).
    pub normalizer_version: String,
    /// Per-case verdicts, in declaration order.
    pub cases: Vec<crate::evidence::CaseVerdict>,
}

impl VerdictReport {
    /// Build a report over `cases` at the current [`crate::normalize::NORMALIZER_VERSION`].
    pub fn new(cases: Vec<crate::evidence::CaseVerdict>) -> VerdictReport {
        VerdictReport {
            schema_version: 1,
            normalizer_version: crate::normalize::NORMALIZER_VERSION.to_string(),
            cases,
        }
    }

    /// Render the report to its deterministic, byte-stable JSON string (2-space indent,
    /// trailing newline). Ordering is fixed by struct/`Vec` order; there are no
    /// timestamps or absolute paths in the body.
    pub fn render(&self) -> Result<String, HarnessError> {
        let mut out = serde_json::to_string_pretty(self).map_err(|e| HarnessError::Report {
            cause: format!("could not serialize verdict report: {e}"),
        })?;
        out.push('\n');
        Ok(out)
    }

    /// Emit the report as the single JSON document on stdout (contract runner-cli.md).
    /// The caller writes all logs/progress to stderr via `tracing`.
    pub fn emit_stdout(&self) -> Result<(), HarnessError> {
        print!("{}", self.render()?);
        Ok(())
    }

    /// The process exit code the runner should use (contract runner-cli.md §"Runner exit
    /// codes"): 0 when every case is `agree`/`allowed-difference`; 1 on any `diverge`;
    /// 3 on any `stale`; 4 on any harness `error`. The worst wins.
    pub fn exit_code(&self) -> i32 {
        use crate::evidence::Outcome;
        let mut code = 0;
        for case in &self.cases {
            let this = match case.overall {
                Outcome::Agree | Outcome::AllowedDifference | Outcome::NoReferenceForPlatform => 0,
                Outcome::Diverge => 1,
                Outcome::Stale => 3,
                Outcome::Error => 4,
            };
            code = code.max(this);
        }
        code
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oracle::OracleSource;

    fn sample_raw() -> RawPaths {
        RawPaths {
            deacon_stdout: "raw/b/c/deacon.stdout".into(),
            deacon_stderr: "raw/b/c/deacon.stderr".into(),
            oracle_stdout: "raw/b/c/oracle.stdout".into(),
            oracle_stderr: "raw/b/c/oracle.stderr".into(),
        }
    }

    fn sample_oracle() -> OracleInfo {
        OracleInfo {
            version: "0.87.0".into(),
            path: "/usr/local/bin/devcontainer".into(),
            source: OracleSource::PathLookup,
        }
    }

    #[tokio::test]
    async fn writes_fragment_atomically_and_roundtrips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let frag = ReportFragment::new(
            "parity_corpus_tier1",
            sample_oracle(),
            now_rfc3339(),
            now_rfc3339(),
            vec![
                CaseResult::pass("case-a", sample_raw()),
                CaseResult::pass_waived("case-b", vec!["errors/x".into()], sample_raw()),
                CaseResult::fail(
                    "case-c",
                    Cause::Divergence,
                    Some("value mismatch at forwardPorts[1]".into()),
                    sample_raw(),
                ),
            ],
            vec![],
        );
        let path = frag.write_under(dir.path()).await.expect("write");
        assert!(path.ends_with("report/parity_corpus_tier1.json"));

        let text = std::fs::read_to_string(&path).expect("read back");
        assert!(text.contains("\"mode\": \"live\""));
        assert!(text.contains("\"outcome\": \"pass-waived\""));
        assert!(text.contains("\"cause\": \"divergence\""));

        let parsed: ReportFragment = serde_json::from_str(&text).expect("roundtrip");
        assert_eq!(parsed, frag);
    }

    #[tokio::test]
    async fn fail_without_cause_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bad = ReportFragment::new(
            "b",
            sample_oracle(),
            now_rfc3339(),
            now_rfc3339(),
            vec![CaseResult {
                case: "c".into(),
                outcome: Outcome::Fail,
                cause: None,
                waivers_applied: vec![],
                diff_summary: None,
                raw: sample_raw(),
            }],
            vec![],
        );
        assert!(matches!(
            bad.write_under(dir.path()).await,
            Err(HarnessError::Report { .. })
        ));
    }

    #[test]
    fn now_rfc3339_is_zulu() {
        let ts = now_rfc3339();
        assert!(ts.ends_with('Z'), "expected Z-suffixed UTC, got {ts}");
    }
}
