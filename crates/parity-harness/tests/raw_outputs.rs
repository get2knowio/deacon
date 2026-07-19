//! Raw-output preservation proof (018-harden-parity-harness, T042; FR-018, FR-020,
//! SC-006).
//!
//! Every compared case ALWAYS preserves four raw artifacts —
//! `deacon.{stdout,stderr}` + `oracle.{stdout,stderr}` — so any verdict is
//! reproducibly diagnosable from disk, and a report fragment's `raw` paths resolve
//! to those bytes verbatim. These hermetic tests drive the real `exec` capture core
//! (`run_and_capture`, the explicit-report-root seam that `exec_deacon`/`exec_oracle`
//! wrap), then the real `report` writer, and finally assert that an unwritable raw
//! directory FAILS the run with a `Report`-class error rather than silently passing.
//! No live oracle, Docker, or network is touched.
//!
//! Unix-only: the stub executables are `#!/bin/sh` scripts made executable via
//! `chmod`, and the read-only-directory fault uses POSIX mode bits (per the repo's
//! Windows notes on stub-script fault-injection tests).
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use parity_harness::HarnessError;
use parity_harness::exec::{Side, run_and_capture};
use parity_harness::oracle::OracleSource;
use parity_harness::report::{CaseResult, OracleInfo, RawPaths, ReportFragment, now_rfc3339};

/// Write an executable `#!/bin/sh` stub and return its path.
fn write_stub(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, body).expect("write stub");
    let mut perms = std::fs::metadata(&p).expect("stat stub").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&p, perms).expect("chmod stub");
    p
}

fn oracle_info() -> OracleInfo {
    OracleInfo {
        version: "0.87.0".into(),
        path: "/usr/local/bin/devcontainer".into(),
        source: OracleSource::PathLookup,
    }
}

#[tokio::test]
async fn preserves_all_four_raw_files_and_fragment_paths_resolve() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("report");
    let binary = "raw_case_bin";
    let case = "case-1";

    // Distinct known bytes per stream (including multibyte UTF-8) so a swap or loss
    // is detectable byte-for-byte.
    let deacon_out = "deacon-out-☑".as_bytes();
    let deacon_err = b"deacon-err".as_slice();
    let oracle_out = b"oracle-out".as_slice();
    let oracle_err = "oracle-err-☒".as_bytes();

    let deacon_stub = write_stub(
        dir.path(),
        "deacon_stub",
        "#!/bin/sh\nprintf 'deacon-out-☑'\nprintf 'deacon-err' 1>&2\nexit 0\n",
    );
    let oracle_stub = write_stub(
        dir.path(),
        "oracle_stub",
        "#!/bin/sh\nprintf 'oracle-out'\nprintf 'oracle-err-☒' 1>&2\nexit 0\n",
    );

    let d = run_and_capture(
        Side::Deacon,
        binary,
        case,
        &deacon_stub,
        &[],
        dir.path(),
        Duration::from_secs(30),
        &root,
    )
    .await
    .expect("deacon capture");
    let o = run_and_capture(
        Side::Oracle,
        binary,
        case,
        &oracle_stub,
        &[],
        dir.path(),
        Duration::from_secs(30),
        &root,
    )
    .await
    .expect("oracle capture");

    // 1. All four raw files exist under the single per-case dir with verbatim bytes.
    let raw_dir = root.join("raw").join(binary).join(case);
    assert_eq!(
        std::fs::read(raw_dir.join("deacon.stdout")).unwrap(),
        deacon_out
    );
    assert_eq!(
        std::fs::read(raw_dir.join("deacon.stderr")).unwrap(),
        deacon_err
    );
    assert_eq!(
        std::fs::read(raw_dir.join("oracle.stdout")).unwrap(),
        oracle_out
    );
    assert_eq!(
        std::fs::read(raw_dir.join("oracle.stderr")).unwrap(),
        oracle_err
    );

    // The invocation-reported paths point at the same preserved bytes.
    assert_eq!(std::fs::read(d.stdout_path()).unwrap(), deacon_out);
    assert_eq!(std::fs::read(o.stderr_path()).unwrap(), oracle_err);

    // 2. A fragment referencing those (report-root-relative) raw paths resolves to
    //    the preserved bytes end-to-end.
    let raw = RawPaths {
        deacon_stdout: d.stdout_rel.to_string_lossy().into_owned(),
        deacon_stderr: d.stderr_rel.to_string_lossy().into_owned(),
        oracle_stdout: o.stdout_rel.to_string_lossy().into_owned(),
        oracle_stderr: o.stderr_rel.to_string_lossy().into_owned(),
    };
    let frag = ReportFragment::new(
        binary,
        oracle_info(),
        now_rfc3339(),
        now_rfc3339(),
        vec![CaseResult::pass(case, raw.clone())],
        vec![],
    );
    let frag_path = frag.write_under(&root).await.expect("fragment write");
    assert!(frag_path.is_file(), "fragment must be written");

    for rel in [
        &raw.deacon_stdout,
        &raw.deacon_stderr,
        &raw.oracle_stdout,
        &raw.oracle_stderr,
    ] {
        let abs = root.join(rel);
        assert!(
            abs.is_file(),
            "fragment raw path must resolve to a file: {rel}"
        );
    }
    assert_eq!(
        std::fs::read(root.join(&raw.deacon_stdout)).unwrap(),
        deacon_out,
        "fragment raw path must resolve to the verbatim captured bytes"
    );
    assert_eq!(
        std::fs::read(root.join(&raw.oracle_stderr)).unwrap(),
        oracle_err
    );
}

/// Raw capture is preserved even for a nonzero-exit invocation: the four files must
/// still exist so a failure is diagnosable (FR-020). `require_success` then surfaces
/// the failure — the run does not silently pass.
#[tokio::test]
async fn nonzero_exit_still_preserves_raw_and_does_not_pass() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("report");
    let stub = write_stub(
        dir.path(),
        "boom",
        "#!/bin/sh\nprintf 'partial-out'\nprintf 'why-it-failed' 1>&2\nexit 3\n",
    );
    let inv = run_and_capture(
        Side::Deacon,
        "raw_fail_bin",
        "case-fail",
        &stub,
        &[],
        dir.path(),
        Duration::from_secs(30),
        &root,
    )
    .await
    .expect("capture completes even on nonzero exit");

    let raw_dir = root.join("raw").join("raw_fail_bin").join("case-fail");
    assert_eq!(
        std::fs::read(raw_dir.join("deacon.stdout")).unwrap(),
        b"partial-out"
    );
    assert_eq!(
        std::fs::read(raw_dir.join("deacon.stderr")).unwrap(),
        b"why-it-failed"
    );
    // The failure is not silently swallowed.
    assert!(
        matches!(
            inv.require_success(),
            Err(HarnessError::OracleFailure { .. })
        ),
        "a nonzero exit must surface as OracleFailure, not a pass"
    );
}

/// An unwritable raw directory must FAIL the run with a `Report`-class error rather
/// than silently passing without artifacts (FR-018). Pre-create the exact per-case
/// raw dir and make it read-only so the atomic temp write cannot land.
#[tokio::test]
async fn read_only_raw_dir_fails_the_run_not_pass() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("report");
    let binary = "ro_bin";
    let case = "ro-case";

    let raw_dir = root.join("raw").join(binary).join(case);
    std::fs::create_dir_all(&raw_dir).expect("precreate raw dir");
    let mut perms = std::fs::metadata(&raw_dir).unwrap().permissions();
    perms.set_mode(0o555); // read + execute, no write
    std::fs::set_permissions(&raw_dir, perms).unwrap();

    let stub = write_stub(dir.path(), "ok_stub", "#!/bin/sh\nprintf 'x'\nexit 0\n");
    let err = run_and_capture(
        Side::Deacon,
        binary,
        case,
        &stub,
        &[],
        dir.path(),
        Duration::from_secs(30),
        &root,
    )
    .await
    .expect_err("writing raw output into a read-only dir must fail the run, not pass");
    assert!(
        matches!(err, HarnessError::Report { .. }),
        "expected a Report-class write failure, got {err:?}"
    );

    // Restore write perms so the TempDir can clean itself up.
    let mut perms = std::fs::metadata(&raw_dir).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&raw_dir, perms).unwrap();
}
