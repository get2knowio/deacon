//! Per-binary run-report fragments with atomic writes (research D8; data-model §5).
//!
//! nextest runs test binaries in parallel with no ordering, so a shared report
//! file would race. Each live parity binary instead writes ONE fragment to
//! `<report_root>/report/<binary>.json`. Failure to write a fragment is
//! [`HarnessError::Report`], which the caller MUST propagate as a test failure — a run
//! whose result cannot be recorded is not a passing run.

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
    Fail,
}

/// Cause of a failing case (required iff `outcome == Fail`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Cause {
    Divergence,
    OracleFailure,
    /// One oracle CLI invocation exceeded its per-invocation bound.
    OracleTimeout,
    /// A whole case exceeded its per-case wall-clock bound (024 FR-077b).
    ///
    /// Distinct from [`OracleTimeout`](Self::OracleTimeout) on purpose: the per-case
    /// bound wraps deacon, the oracle, observation, AND teardown, so a wedged
    /// `deacon up` or a hung `docker inspect` lands here. Reporting those as
    /// `OracleTimeout` blamed the pinned reference for stalls it had no part in —
    /// exactly the unattributable signal the explicit per-case bound was added to fix.
    CaseTimeout,
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
            diff_summary,
            raw,
        }
    }

    /// Schema invariant: `fail` requires a cause.
    fn validate(&self) -> Result<(), String> {
        match self.outcome {
            Outcome::Fail if self.cause.is_none() => {
                Err(format!("case `{}`: fail without a cause", self.case))
            }
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

    /// Serialize and write this fragment atomically under
    /// `<report_root>/report/<binary>/`, returning the directory written.
    /// A write failure is [`HarnessError::Report`] — the caller fails the test.
    pub async fn write(&self) -> Result<PathBuf, HarnessError> {
        self.write_under(&crate::report_root()).await
    }

    /// As [`write`], but under an explicit report root (for tests / custom dirs).
    ///
    /// **One file per case** — reported (`<binary>/<case-id>.json`) *or* omitted
    /// (`<binary>/<case-id>.omitted.json`) — plus a `_meta.json` carrying only the run
    /// metadata.
    ///
    /// This shape exists because the previous one silently destroyed evidence. A fragment
    /// used to be written to a single `report/<binary>.json`, but a binary like
    /// `parity_state_diff` has EIGHT `#[tokio::test]` functions, each building a fragment
    /// holding ONE case — and under nextest each is a separate process. Last writer won, so
    /// the on-disk fragment held 1 case of 8, and the aggregator's per-binary index
    /// collapsed same-binary fragments the same way. That is the exact
    /// reported-granularity-below-asserted-granularity defect spec 023 existed to fix,
    /// living on unguarded in the carriers 023 did not retire (024 D-1).
    ///
    /// Per-case files make concurrent writers additive instead of destructive: two
    /// processes writing different cases of one binary touch different paths.
    ///
    /// **Omissions are per-case files too, and that is not incidental.** Parking them on a
    /// shared `_meta.json` reintroduced the very defect above one level down: every writer
    /// of a binary rewrites that file wholesale, so the last process to finish — typically
    /// one with nothing omitted — erased the omission lists of all the others. An omission
    /// is evidence (gate 3 requires each to carry a reason, and gate 7 reads them to tell a
    /// deliberate skip from an unreported unit), so losing it is losing evidence. What
    /// remains in `_meta.json` is only data every writer produces identically, or that
    /// merges monotonically: the binary name, the oracle (a disagreement is a hard error),
    /// and the run timestamps (which widen).
    pub async fn write_under(
        &self,
        report_root: &std::path::Path,
    ) -> Result<PathBuf, HarnessError> {
        self.validate()
            .map_err(|cause| HarnessError::Report { cause })?;
        let dir = report_root.join("report").join(&self.binary);

        // Metadata ONLY — no cases, no omissions. See the doc comment: anything
        // per-process stored here is destroyed by the next process to write.
        let meta = ReportFragment {
            cases: Vec::new(),
            omitted: Vec::new(),
            ..self.clone()
        };
        write_json(&dir.join("_meta.json"), &meta, &self.binary).await?;

        for case in &self.cases {
            let single = ReportFragment {
                cases: vec![case.clone()],
                omitted: Vec::new(),
                ..self.clone()
            };
            write_json(&dir.join(case_file_name(&case.case)), &single, &self.binary).await?;
        }
        for omission in &self.omitted {
            let single = ReportFragment {
                cases: Vec::new(),
                omitted: vec![omission.clone()],
                ..self.clone()
            };
            write_json(
                &dir.join(omitted_file_name(&omission.case)),
                &single,
                &self.binary,
            )
            .await?;
        }
        Ok(dir)
    }
}

/// Serialize `fragment` and write it atomically to `path`.
async fn write_json(
    path: &std::path::Path,
    fragment: &ReportFragment,
    binary: &str,
) -> Result<(), HarnessError> {
    let bytes = serde_json::to_vec_pretty(fragment).map_err(|e| HarnessError::Report {
        cause: format!("could not serialize fragment for `{binary}`: {e}"),
    })?;
    crate::atomic_write(path, &bytes).await
}

/// A filesystem-safe file name for a case id.
///
/// Case ids are authored slugs, but a path separator or `..` in one would escape the
/// report directory, so every character outside `[A-Za-z0-9.-]` is escaped.
///
/// **The escape is INJECTIVE, deliberately.** The obvious version — map every unsafe
/// character to `_` — is lossy, and lossy is not safe here: `exec/tty` and `exec:tty` both
/// become `exec_tty.json`, so the second write renames over the first and one case's
/// result vanishes. The aggregator then reports N-1 cases and gate 7 flags the missing
/// baseline unit as unreported. That is the same evidence loss the per-case layout was
/// introduced to fix, re-entering through the file name. Escaping each unsafe byte as
/// `_<hex>` (and `_` itself as `_5f`) keeps distinct ids in distinct files.
fn case_file_name(case: &str) -> String {
    let mut out = String::with_capacity(case.len() + 8);
    for byte in case.bytes() {
        let safe = byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-';
        if safe {
            out.push(byte as char);
        } else {
            // `_` is the escape character, so it must escape itself to stay injective.
            out.push_str(&format!("_{byte:02x}"));
        }
    }
    // `_meta` is reserved for the metadata file, and a leading dot would hide this one.
    // Neither can arise from the escaping above (`_` and `.` at position 0 are handled),
    // but guard explicitly so the reservation survives a future change to the safe set.
    if out == "_meta" || out.starts_with('.') {
        out.insert(0, 'c');
    }
    out.push_str(".json");
    out
}

/// The file name recording that a case was OMITTED rather than compared.
///
/// A distinct suffix rather than a shared name: a case is normally either reported or
/// omitted, but two test functions of one binary disagreeing about that is exactly the
/// kind of thing the report should surface, not resolve by overwrite.
fn omitted_file_name(case: &str) -> String {
    let mut out = case_file_name(case);
    out.truncate(out.len() - ".json".len());
    out.push_str(".omitted.json");
    out
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
            "parity_differential",
            sample_oracle(),
            now_rfc3339(),
            now_rfc3339(),
            vec![
                CaseResult::pass("case-a", sample_raw()),
                CaseResult::pass("case-b", sample_raw()),
                CaseResult::fail(
                    "case-c",
                    Cause::Divergence,
                    Some("value mismatch at forwardPorts[1]".into()),
                    sample_raw(),
                ),
            ],
            vec![],
        );
        // `write_under` writes ONE FILE PER CASE under `report/<binary>/` plus a
        // `_meta.json`, and returns that directory — the per-case granularity D-1 fixed
        // (a single shared file made concurrent test processes overwrite each other).
        let dir_path = frag.write_under(dir.path()).await.expect("write");
        assert!(
            dir_path.ends_with("report/parity_differential"),
            "write_under returns the per-binary DIRECTORY: {dir_path:?}"
        );
        assert!(
            dir_path.join("_meta.json").is_file(),
            "metadata is recorded"
        );

        // Every case is independently readable and round-trips.
        let mut round_tripped = Vec::new();
        for case in &frag.cases {
            let path = dir_path.join(format!("{}.json", case.case));
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read back {path:?}: {e}"));
            let parsed: ReportFragment = serde_json::from_str(&text).expect("roundtrip");
            assert_eq!(parsed.cases.len(), 1, "one case per file");
            assert_eq!(&parsed.cases[0], case, "the case round-trips verbatim");
            round_tripped.push(text);
        }
        let all = round_tripped.join("\n");
        assert!(all.contains("\"mode\": \"live\""));
        assert!(all.contains("\"outcome\": \"pass\""));
        assert!(all.contains("\"outcome\": \"fail\""));
        assert!(all.contains("\"cause\": \"divergence\""));
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
