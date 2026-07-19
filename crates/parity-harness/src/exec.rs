//! Bounded oracle/deacon invocation with always-on raw-output capture
//! (research D3/D8, FR-007, FR-020).
//!
//! Every invocation streams the child's stdout/stderr to
//! `<report_root>/raw/<binary>/<case>/{deacon,oracle}.{stdout,stderr}` (atomic
//! temp+rename), whether it passes or fails, so any comparison is reproducibly
//! diagnosable from artifacts. Bounds are PER CLI INVOCATION — 2 min for
//! configuration-only work, 15 min for container-lifecycle work — and a breach
//! kills the child and preserves whatever partial output was produced.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::AsyncReadExt;

use crate::HarnessError;

/// Which CLI produced an invocation (fixes the raw-file name prefix).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Deacon,
    Oracle,
}

impl Side {
    fn prefix(self) -> &'static str {
        match self {
            Side::Deacon => "deacon",
            Side::Oracle => "oracle",
        }
    }
}

/// Per-invocation time bound class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecKind {
    /// Configuration-only commands (e.g. `read-configuration`): 2 minutes.
    Config,
    /// Container-lifecycle commands (e.g. `up`, `build`, `exec`): 15 minutes.
    Lifecycle,
}

impl ExecKind {
    /// The per-invocation bound for this kind.
    pub fn bound(self) -> Duration {
        match self {
            ExecKind::Config => Duration::from_secs(2 * 60),
            ExecKind::Lifecycle => Duration::from_secs(15 * 60),
        }
    }
}

/// The captured result of one CLI invocation. Raw bytes are held in memory AND
/// written verbatim to disk; `*_rel` are the report-relative artifact paths that
/// the report fragment references.
#[derive(Debug, Clone)]
pub struct Invocation {
    pub side: Side,
    pub case: String,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: Option<i32>,
    pub success: bool,
    /// Path to the stdout artifact, relative to the report root.
    pub stdout_rel: PathBuf,
    /// Path to the stderr artifact, relative to the report root.
    pub stderr_rel: PathBuf,
    /// The report root the artifacts were written under.
    report_root: PathBuf,
}

impl Invocation {
    /// stdout decoded lossily as UTF-8.
    pub fn stdout_string(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.stdout)
    }

    /// Parse this invocation's stdout as the JSON document the comparison requires.
    /// Non-JSON stdout from a CLI that nonetheless exited successfully is a
    /// [`HarnessError::MalformedOutput`] naming the case — the transport-level
    /// failure that precedes (and is distinct from) a normalization-rule failure
    /// ([`HarnessError::Normalization`]): the output never parsed as JSON at all.
    /// The verbatim raw stdout remains on disk for diagnosis regardless. There is
    /// no fallback to comparing raw bytes.
    pub fn stdout_json(&self) -> Result<serde_json::Value, HarnessError> {
        serde_json::from_str(self.stdout_string().trim()).map_err(|e| {
            HarnessError::MalformedOutput {
                case: self.case.clone(),
                cause: e.to_string(),
            }
        })
    }

    /// Absolute path to the written stdout artifact.
    pub fn stdout_path(&self) -> PathBuf {
        self.report_root.join(&self.stdout_rel)
    }

    /// Absolute path to the written stderr artifact.
    pub fn stderr_path(&self) -> PathBuf {
        self.report_root.join(&self.stderr_rel)
    }

    /// Assert this invocation succeeded; a non-zero exit where success was expected
    /// becomes [`HarnessError::OracleFailure`] referencing the preserved stderr.
    pub fn require_success(&self) -> Result<(), HarnessError> {
        if self.success {
            return Ok(());
        }
        let status = match self.exit_code {
            Some(code) => format!("exit code {code}"),
            None => "terminated by signal".to_string(),
        };
        Err(HarnessError::OracleFailure {
            case: self.case.clone(),
            status,
            stderr_path: self.stderr_path(),
        })
    }
}

/// Run the deacon binary under test. The caller supplies its path explicitly —
/// only the test crate can expand `env!("CARGO_BIN_EXE_deacon")`; the harness
/// never guesses a `target/…/deacon` path.
pub async fn exec_deacon(
    binary: &str,
    case: &str,
    kind: ExecKind,
    deacon_path: &Path,
    args: &[&str],
    cwd: &Path,
) -> Result<Invocation, HarnessError> {
    run_and_capture(
        Side::Deacon,
        binary,
        case,
        deacon_path,
        args,
        cwd,
        kind.bound(),
        &crate::report_root(),
    )
    .await
}

/// Run the verified oracle binary.
pub async fn exec_oracle(
    binary: &str,
    case: &str,
    kind: ExecKind,
    oracle_path: &Path,
    args: &[&str],
    cwd: &Path,
) -> Result<Invocation, HarnessError> {
    run_and_capture(
        Side::Oracle,
        binary,
        case,
        oracle_path,
        args,
        cwd,
        kind.bound(),
        &crate::report_root(),
    )
    .await
}

/// Core execution + capture, and the explicit-report-root seam that
/// [`exec_deacon`]/[`exec_oracle`] wrap (they default `report_root` to
/// [`crate::report_root`]). `bound` and `report_root` are explicit so callers —
/// unit tests and hermetic integration tests alike — can inject a short bound and
/// a temp artifact root instead of mutating process env (matching the crate's
/// other explicit-root seams, [`crate::report::ReportFragment::write_under`] and
/// [`crate::aggregate::run`]).
#[allow(clippy::too_many_arguments)]
pub async fn run_and_capture(
    side: Side,
    binary: &str,
    case: &str,
    program: &Path,
    args: &[&str],
    cwd: &Path,
    bound: Duration,
    report_root: &Path,
) -> Result<Invocation, HarnessError> {
    let prefix = side.prefix();
    let rel_dir = PathBuf::from("raw").join(binary).join(case);
    let stdout_rel = rel_dir.join(format!("{prefix}.stdout"));
    let stderr_rel = rel_dir.join(format!("{prefix}.stderr"));
    let stdout_abs = report_root.join(&stdout_rel);
    let stderr_abs = report_root.join(&stderr_rel);

    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            // Preserve the (empty) artifact paths so the raw-paths invariant holds.
            crate::atomic_write(&stdout_abs, b"").await?;
            crate::atomic_write(
                &stderr_abs,
                format!("failed to spawn {program:?}: {e}").as_bytes(),
            )
            .await?;
            return Err(HarnessError::OracleFailure {
                case: case.to_string(),
                status: format!("failed to spawn {program:?}: {e}"),
                stderr_path: stderr_abs,
            });
        }
    };

    // Drain both pipes concurrently so a chatty child cannot deadlock on a full
    // pipe while we wait for exit.
    let out_task = tokio::spawn(drain(child.stdout.take()));
    let err_task = tokio::spawn(drain(child.stderr.take()));

    match tokio::time::timeout(bound, child.wait()).await {
        // Bound exceeded: kill, then collect whatever partial output was produced.
        Err(_elapsed) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            let stdout = out_task.await.unwrap_or_default();
            let stderr = err_task.await.unwrap_or_default();
            crate::atomic_write(&stdout_abs, &stdout).await?;
            crate::atomic_write(&stderr_abs, &stderr).await?;
            Err(HarnessError::OracleTimeout {
                case: case.to_string(),
                bound,
                partial_paths: vec![stdout_abs, stderr_abs],
            })
        }
        Ok(Err(e)) => {
            let _ = child.start_kill();
            let stdout = out_task.await.unwrap_or_default();
            let stderr = err_task.await.unwrap_or_default();
            crate::atomic_write(&stdout_abs, &stdout).await?;
            crate::atomic_write(&stderr_abs, &stderr).await?;
            Err(HarnessError::OracleFailure {
                case: case.to_string(),
                status: format!("could not await child: {e}"),
                stderr_path: stderr_abs,
            })
        }
        Ok(Ok(status)) => {
            let stdout = out_task.await.unwrap_or_default();
            let stderr = err_task.await.unwrap_or_default();
            crate::atomic_write(&stdout_abs, &stdout).await?;
            crate::atomic_write(&stderr_abs, &stderr).await?;
            Ok(Invocation {
                side,
                case: case.to_string(),
                stdout,
                stderr,
                exit_code: status.code(),
                success: status.success(),
                stdout_rel,
                stderr_rel,
                report_root: report_root.to_path_buf(),
            })
        }
    }
}

/// Read an optional child pipe to EOF, tolerating read errors (best-effort raw
/// capture never fails the run for a partial read).
async fn drain<R: tokio::io::AsyncRead + Unpin>(pipe: Option<R>) -> Vec<u8> {
    let mut buf = Vec::new();
    if let Some(mut r) = pipe {
        let _ = r.read_to_end(&mut buf).await;
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_match_spec() {
        assert_eq!(ExecKind::Config.bound(), Duration::from_secs(120));
        assert_eq!(ExecKind::Lifecycle.bound(), Duration::from_secs(900));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stdout_json_parses_and_flags_garbage() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("report");

        let ok = write_stub(
            dir.path(),
            "json",
            "#!/bin/sh\nprintf '{\"a\":1}'\nexit 0\n",
        );
        let inv = run_and_capture(
            Side::Deacon,
            "bin_x",
            "case_json",
            &ok,
            &[],
            dir.path(),
            Duration::from_secs(30),
            &root,
        )
        .await
        .expect("capture");
        assert_eq!(
            inv.stdout_json().expect("valid json"),
            serde_json::json!({"a":1})
        );

        let garbage = write_stub(
            dir.path(),
            "garbage",
            "#!/bin/sh\nprintf 'not json'\nexit 0\n",
        );
        let inv = run_and_capture(
            Side::Oracle,
            "bin_x",
            "case_garbage",
            &garbage,
            &[],
            dir.path(),
            Duration::from_secs(30),
            &root,
        )
        .await
        .expect("capture");
        match inv.stdout_json() {
            Err(HarnessError::MalformedOutput { case, .. }) => assert_eq!(case, "case_garbage"),
            other => panic!("expected MalformedOutput, got {other:?}"),
        }
    }

    #[cfg(unix)]
    fn write_stub(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join(name);
        std::fs::write(&p, body).expect("write stub");
        let mut perms = std::fs::metadata(&p).expect("stat").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&p, perms).expect("chmod");
        p
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn captures_stdout_stderr_and_writes_raw_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("report");
        let stub = write_stub(
            dir.path(),
            "echoer",
            "#!/bin/sh\nprintf 'out-bytes' \nprintf 'err-bytes' 1>&2\nexit 0\n",
        );

        let inv = run_and_capture(
            Side::Deacon,
            "bin_x",
            "case_y",
            &stub,
            &[],
            dir.path(),
            Duration::from_secs(30),
            &root,
        )
        .await
        .expect("capture succeeds");

        assert!(inv.success);
        assert_eq!(inv.exit_code, Some(0));
        assert_eq!(inv.stdout, b"out-bytes");
        assert_eq!(inv.stderr, b"err-bytes");
        assert_eq!(
            inv.stdout_rel,
            PathBuf::from("raw/bin_x/case_y/deacon.stdout")
        );
        assert_eq!(
            std::fs::read(root.join("raw/bin_x/case_y/deacon.stdout")).unwrap(),
            b"out-bytes"
        );
        assert_eq!(
            std::fs::read(root.join("raw/bin_x/case_y/deacon.stderr")).unwrap(),
            b"err-bytes"
        );
        inv.require_success().expect("success");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn nonzero_exit_becomes_oracle_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("report");
        let stub = write_stub(dir.path(), "boom", "#!/bin/sh\necho nope 1>&2\nexit 7\n");
        let inv = run_and_capture(
            Side::Oracle,
            "bin_x",
            "case_z",
            &stub,
            &[],
            dir.path(),
            Duration::from_secs(30),
            &root,
        )
        .await
        .expect("capture completes even on nonzero exit");
        assert!(!inv.success);
        assert_eq!(inv.exit_code, Some(7));
        let err = inv.require_success().expect_err("must be failure");
        match err {
            HarnessError::OracleFailure {
                case, stderr_path, ..
            } => {
                assert_eq!(case, "case_z");
                assert!(stderr_path.ends_with("oracle.stderr"));
            }
            other => panic!("expected OracleFailure, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_kills_and_preserves_partial_output() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("report");
        // Emit a line, then hang past the injected bound.
        let stub = write_stub(
            dir.path(),
            "hang",
            "#!/bin/sh\nprintf 'partial' \nsleep 10\n",
        );
        let err = run_and_capture(
            Side::Deacon,
            "bin_x",
            "case_t",
            &stub,
            &[],
            dir.path(),
            Duration::from_millis(200),
            &root,
        )
        .await
        .expect_err("must time out");
        match err {
            HarnessError::OracleTimeout {
                case,
                partial_paths,
                ..
            } => {
                assert_eq!(case, "case_t");
                assert_eq!(partial_paths.len(), 2);
                let out = std::fs::read(root.join("raw/bin_x/case_t/deacon.stdout")).unwrap();
                assert_eq!(out, b"partial", "partial stdout must be preserved");
            }
            other => panic!("expected OracleTimeout, got {other:?}"),
        }
    }
}
