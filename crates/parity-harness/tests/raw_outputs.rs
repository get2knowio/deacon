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
        None,
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
        None,
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
    // `write_under` returns the per-binary DIRECTORY: one file per case plus `_meta.json`
    // (024 D-1 — a single per-binary file let concurrent test processes overwrite each
    // other's evidence).
    let frag_dir = frag.write_under(&root).await.expect("fragment write");
    assert!(frag_dir.is_dir(), "fragment directory must be written");
    assert!(
        frag_dir.join("_meta.json").is_file(),
        "run metadata must be recorded"
    );
    assert!(
        frag_dir.join(format!("{case}.json")).is_file(),
        "the case must have its OWN file, so a sibling test process cannot clobber it"
    );

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
        None,
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
        None,
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

// ===========================================================================
// T058 (US4, FR-022): raw and normalized evidence are BOTH preserved and
// SEPARATELY locatable for every compared case.
// ===========================================================================

/// FR-022: normalization must never be destructive of the record. Whatever a rule
/// rewrote, dropped or reshaped, the verbatim capture stays available beside it — so a
/// reviewer can always ask "what did the CLI actually emit?" independently of "what did
/// we compare?".
///
/// This matters most exactly where US4 tightens the rules: `path_token` rewrites a host
/// path out of the COMPARISON, and the raw evidence must still show what deacon actually
/// emitted. If normalization overwrote the raw record, retiring a rule later would be
/// unreviewable — which is precisely what #398 then did to `drop_absent_optional` on the
/// `configuration` block.
#[test]
fn raw_and_normalized_evidence_are_both_preserved_and_separately_locatable() {
    use parity_harness::evidence::{CaseEvidence, RawChannelEvidence};
    use parity_harness::normalize::{self, TokenMap};

    let workspace = Path::new("/tmp/some-run-1234/ws");
    let tokens = TokenMap::workspace(workspace);

    // A structured-output document exercising a path that `path_token` rewrites plus
    // values no rule touches, so the raw-vs-normalized separation is visible.
    let raw_value = serde_json::json!({
        "configuration": {
            "name": "demo",
            "workspaceMount": null,
            "initializeCommand": null,
            "unlistedProperty": {},
            "mounts": ["source=/tmp/some-run-1234/ws,target=/w,type=bind"],
        }
    });
    let raw = RawChannelEvidence {
        channel: "chan-structured-output".to_string(),
        operation: "op-read".to_string(),
        present: true,
        value: raw_value.clone(),
    };
    let normalized =
        normalize::normalize_channel("chan-structured-output", &raw, &tokens, Side::Deacon);

    let mut evidence = CaseEvidence::new();
    evidence.push(raw.clone(), normalized.clone());

    // BOTH are present, in separate collections, addressable by channel.
    assert_eq!(evidence.raw.len(), 1);
    assert_eq!(evidence.normalized.len(), 1);
    let stored_raw = evidence
        .raw
        .iter()
        .find(|e| e.channel == "chan-structured-output")
        .expect("raw evidence is locatable by channel");
    let stored_norm = evidence
        .normalized
        .iter()
        .find(|e| e.channel == "chan-structured-output")
        .expect("normalized evidence is locatable by channel");

    // The raw record is VERBATIM — the temp path is intact and the elided optionals are
    // still there. Normalization did not overwrite the capture.
    assert_eq!(
        stored_raw.value, raw_value,
        "raw evidence must be byte-faithful to what the CLI emitted"
    );
    assert_eq!(
        stored_raw.value["configuration"]["workspaceMount"],
        serde_json::Value::Null,
        "an authored null must remain visible in the RAW record"
    );
    assert!(
        stored_raw.value["configuration"]["mounts"][0]
            .as_str()
            .is_some_and(|s| s.contains("/tmp/some-run-1234/ws")),
        "the raw record keeps the un-tokenized path"
    );

    // The normalized record is the compared form: path tokenized, everything else
    // preserved. Since #398 the `configuration` block loses NOTHING to
    // `drop_absent_optional` — deacon emits only what the author wrote, so an authored
    // null is the author's and is compared.
    assert_eq!(
        stored_norm.value["configuration"]["workspaceMount"],
        serde_json::Value::Null,
        "an authored null survives into the comparison — eliding it is what made an \
         authored null and an omission the same observation (FR-055)"
    );
    assert_eq!(
        stored_norm.value["configuration"]["unlistedProperty"],
        serde_json::json!({}),
        "an UNLISTED empty value is preserved in the comparison (023 T062)"
    );
    assert!(
        stored_norm.value["configuration"]["mounts"][0]
            .as_str()
            .is_some_and(|s| !s.contains("/tmp/some-run-1234")),
        "the normalized record has the path rewritten to a stable token"
    );

    // And the two are genuinely DIFFERENT documents — the separation is real, not a
    // pair of aliases to one value.
    assert_ne!(
        stored_raw.value, stored_norm.value,
        "raw and normalized must be stored separately, not aliased (FR-016/FR-022)"
    );
    assert_eq!(
        stored_raw.present, stored_norm.present,
        "`present` is preserved"
    );
}

/// A not-captured channel stays not-captured on both sides — `present:false` is never
/// laundered into a captured-empty value by normalization (FR-018).
#[test]
fn not_captured_evidence_stays_not_captured_through_normalization() {
    use parity_harness::evidence::RawChannelEvidence;
    use parity_harness::normalize::{self, TokenMap};

    let raw = RawChannelEvidence {
        channel: "chan-image".to_string(),
        operation: "op-up".to_string(),
        present: false,
        value: serde_json::Value::Null,
    };
    let normalized = normalize::normalize_channel(
        "chan-image",
        &raw,
        &TokenMap::workspace(Path::new("/w")),
        Side::Deacon,
    );
    assert!(
        !normalized.present,
        "a channel that could not be observed must stay distinguishable from one \
         observed as empty"
    );
}

/// #586: `Operation::stdinFile` must reach the child process with its bytes intact.
///
/// The field it replaced was declared in the case schema and dropped by the executor,
/// so a case could have asserted a stdin-dependent behavior while the child read
/// `/dev/null` — and passed, because the assertion was on something else. This test is
/// the guard against that returning: it pipes all 256 byte values (NUL and invalid
/// UTF-8 included, which is why the payload is a FILE and not a JSON string) through a
/// stub that copies stdin to stdout, and compares the captured artifact byte for byte.
#[tokio::test]
async fn a_stdin_payload_reaches_the_child_with_its_bytes_intact() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("report");
    let binary = "stdin_bin";
    let case = "case-stdin";

    let payload: Vec<u8> = (0u8..=255).collect();
    let payload_path = dir.path().join("payload.bin");
    std::fs::write(&payload_path, &payload).expect("write payload");

    let stub = write_stub(dir.path(), "cat_stub", "#!/bin/sh\nexec cat\n");

    let inv = run_and_capture(
        Side::Deacon,
        binary,
        case,
        &stub,
        &[],
        dir.path(),
        Some(payload_path.as_path()),
        Duration::from_secs(30),
        &root,
    )
    .await
    .expect("capture with stdin");

    assert_eq!(inv.exit_code, Some(0));
    let captured = std::fs::read(inv.stdout_path()).expect("read captured stdout");
    assert_eq!(
        captured,
        payload,
        "stdout must be byte-identical to the stdin payload; got {} bytes",
        captured.len()
    );
}

/// The other half: with no payload declared, stdin stays `null`. A child that reads
/// stdin must see EOF immediately rather than inheriting the test runner's, which would
/// make a case's behavior depend on how the suite was invoked.
#[tokio::test]
async fn without_a_payload_stdin_is_null() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("report");
    let stub = write_stub(dir.path(), "cat_stub", "#!/bin/sh\nexec cat\n");

    let inv = run_and_capture(
        Side::Deacon,
        "stdin_bin",
        "case-null-stdin",
        &stub,
        &[],
        dir.path(),
        None,
        Duration::from_secs(30),
        &root,
    )
    .await
    .expect("capture without stdin");

    assert_eq!(inv.exit_code, Some(0));
    let captured = std::fs::read(inv.stdout_path()).expect("read captured stdout");
    assert!(captured.is_empty(), "expected EOF, got {captured:?}");
}

/// A payload that cannot be opened FAILS the invocation. The predecessor field's whole
/// defect was failing silently, so the replacement must not have a quiet path either.
#[tokio::test]
async fn a_missing_stdin_payload_fails_the_invocation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("report");
    let stub = write_stub(dir.path(), "cat_stub", "#!/bin/sh\nexec cat\n");

    let err = run_and_capture(
        Side::Deacon,
        "stdin_bin",
        "case-missing-stdin",
        &stub,
        &[],
        dir.path(),
        Some(&dir.path().join("does-not-exist.bin")),
        Duration::from_secs(30),
        &root,
    )
    .await
    .expect_err("a missing payload must not be silently ignored");

    assert!(
        format!("{err}").contains("could not open stdin payload"),
        "the diagnostic must name the cause: {err}"
    );
}
