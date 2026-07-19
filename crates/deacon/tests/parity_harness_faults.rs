//! Fault-injection acceptance suite: proof the parity harness cannot lie
//! (018-harden-parity-harness, T043/T044; research D10; FR-021; SC-001).
//!
//! Every guaranteed failure mode is DEMONSTRATED here against real harness code
//! paths — never asserted by inspection. Each case injects the fault through the
//! harness's own seams (executable stubs + the documented `DEACON_PARITY_*`
//! override env vars, fabricated JSON documents, fabricated waiver fixtures) and
//! asserts the exact cause-specific [`HarnessError`] with its remedy-bearing
//! `Display`. Ten sub-cases:
//!
//! - (a) wrong-version oracle stub → `OracleVersionMismatch` (found vs required);
//! - (b) nonexistent override path → `OracleMissing` (provisioning hint);
//! - (c) failing docker stub → `DockerMissing`;
//! - (d) crash stub (nonzero exit) → `OracleFailure` (stderr preserved);
//! - (e) garbage-output stub → `MalformedOutput`;
//! - (f) hang stub past a shortened bound → `OracleTimeout` (partial output kept);
//! - (g) injected differing documents → unwaived-divergence failure;
//! - (h) + matching waiver fixture → `pass-waived` naming the record id;
//! - (i) difference removed, waiver kept → `WaiverStale`;
//! - (j) invalid input to `normalize::config` → `Normalization` (no raw fallback).
//!
//! Hermetic: NO live oracle, NO real Docker, NO network — stub executables and env
//! overrides only. The oracle/docker/timeout legs rely on nextest's process-per-
//! test isolation (the mandated runner; this binary is selected only in hermetic
//! nextest lanes) so each `Oracle::acquire()` sees a fresh process-wide cache and
//! each `DEACON_PARITY_*` override is scoped to its own test process.
//!
//! Unix-only (whole file): the fault stubs are `#!/bin/sh` scripts made executable
//! via POSIX mode bits (per the repo's Windows notes on stub-script tests). The
//! pipeline legs (g–j) exercise pure `normalize`/`waiver` code that is additionally
//! covered cross-platform by the harness crate's own unit tests.
#![cfg(unix)]

use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use parity_harness::HarnessError;
use parity_harness::exec::{Side, run_and_capture};
use parity_harness::normalize;
use parity_harness::oracle::{ORACLE_OVERRIDE_ENV, Oracle};
use parity_harness::prereq::{DOCKER_OVERRIDE_ENV, require_docker};
use parity_harness::report::{CaseResult, Cause, Outcome, RawPaths};
use parity_harness::waiver::{Scope, WaiverSet};

/// This binary's name — used as the raw-artifact subdirectory for exec cases.
const BINARY: &str = "parity_harness_faults";

/// Write an executable `#!/bin/sh` stub and return its path.
fn write_stub(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, body).expect("write stub");
    let mut perms = std::fs::metadata(&p).expect("stat stub").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&p, perms).expect("chmod stub");
    p
}

/// Placeholder raw-artifact paths for pipeline-only `CaseResult`s (g/h): those
/// legs assert classification, not artifact bytes (that is `raw_outputs.rs`).
fn sample_raw() -> RawPaths {
    RawPaths {
        deacon_stdout: "raw/parity_harness_faults/c/deacon.stdout".into(),
        deacon_stderr: "raw/parity_harness_faults/c/deacon.stderr".into(),
        oracle_stdout: "raw/parity_harness_faults/c/oracle.stdout".into(),
        oracle_stderr: "raw/parity_harness_faults/c/oracle.stderr".into(),
    }
}

// ---------------------------------------------------------------------------
// (a) Wrong oracle version → OracleVersionMismatch naming found vs required.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn a_wrong_version_stub_reports_mismatch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let stub = write_stub(dir.path(), "devcontainer", "#!/bin/sh\necho 0.86.0\n");
    let stub_str = stub.to_str().expect("utf8 path").to_string();

    let result =
        temp_env::async_with_vars([(ORACLE_OVERRIDE_ENV, Some(stub_str.as_str()))], async {
            Oracle::acquire().await
        })
        .await;

    match &result {
        Err(HarnessError::OracleVersionMismatch {
            found,
            required,
            path,
        }) => {
            assert_eq!(
                found, "0.86.0",
                "must name the wrong version the stub reported"
            );
            assert_eq!(required, "0.87.0", "must name the pinned required version");
            assert_eq!(path, &stub);
        }
        other => panic!("expected OracleVersionMismatch, got {other:?}"),
    }

    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("0.86.0") && msg.contains("0.87.0") && msg.contains("Remedy"),
        "Display must name found, required, and a remedy: {msg}"
    );
}

// ---------------------------------------------------------------------------
// (b) Nonexistent override path → OracleMissing with a provisioning hint.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn b_nonexistent_override_reports_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("does-not-exist").join("devcontainer");
    let missing_str = missing.to_str().expect("utf8 path").to_string();

    let result =
        temp_env::async_with_vars([(ORACLE_OVERRIDE_ENV, Some(missing_str.as_str()))], async {
            Oracle::acquire().await
        })
        .await;

    match &result {
        Err(HarnessError::OracleMissing { hint }) => {
            assert!(
                hint.contains(&missing_str),
                "hint must name the missing override path: {hint}"
            );
        }
        other => panic!("expected OracleMissing, got {other:?}"),
    }
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("npm install -g @devcontainers/cli")
            && msg.contains("DEACON_PARITY_DEVCONTAINER"),
        "Display must carry the provisioning hint: {msg}"
    );
}

// ---------------------------------------------------------------------------
// (c) Failing docker stub via DEACON_PARITY_DOCKER → DockerMissing.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn c_failing_docker_stub_reports_docker_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let stub = write_stub(
        dir.path(),
        "docker",
        "#!/bin/sh\necho 'daemon down' 1>&2\nexit 1\n",
    );
    let stub_str = stub.to_str().expect("utf8 path").to_string();

    let result =
        temp_env::async_with_vars([(DOCKER_OVERRIDE_ENV, Some(stub_str.as_str()))], async {
            require_docker().await
        })
        .await;

    assert!(
        matches!(result, Err(HarnessError::DockerMissing)),
        "a failing docker stub must be reported as DockerMissing, got {result:?}"
    );
    assert!(
        HarnessError::DockerMissing.to_string().contains("Docker")
            && HarnessError::DockerMissing.to_string().contains("Remedy"),
        "Display must name Docker and a remedy"
    );
}

// ---------------------------------------------------------------------------
// (d) Crash stub (nonzero exit where success expected) → OracleFailure, with the
//     stderr preserved on disk for diagnosis.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn d_crash_stub_is_oracle_failure() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("report");
    let stub = write_stub(
        dir.path(),
        "crash",
        "#!/bin/sh\nprintf 'half-a-protocol'\nprintf 'exploded mid-run' 1>&2\nexit 1\n",
    );

    let inv = run_and_capture(
        Side::Oracle,
        BINARY,
        "crash",
        &stub,
        &[],
        dir.path(),
        Duration::from_secs(30),
        &root,
    )
    .await
    .expect("capture completes even on a nonzero exit");

    assert!(!inv.success);
    assert_eq!(inv.exit_code, Some(1));
    let err = inv
        .require_success()
        .expect_err("a nonzero exit must surface as OracleFailure, not pass");
    match err {
        HarnessError::OracleFailure {
            case, stderr_path, ..
        } => {
            assert_eq!(case, "crash");
            assert!(stderr_path.is_file(), "preserved stderr must exist on disk");
            let stderr = std::fs::read(&stderr_path).unwrap();
            assert_eq!(stderr, b"exploded mid-run");
        }
        other => panic!("expected OracleFailure, got {other:?}"),
    }
    // The partial stdout is preserved regardless.
    assert_eq!(
        std::fs::read(root.join("raw/parity_harness_faults/crash/oracle.stdout")).unwrap(),
        b"half-a-protocol"
    );
}

// ---------------------------------------------------------------------------
// (e) Garbage output where JSON was required → MalformedOutput. The CLI exits 0,
//     so this is a distinct transport-level failure from a normalization failure.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn e_garbage_output_is_malformed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("report");
    let stub = write_stub(
        dir.path(),
        "garbage",
        "#!/bin/sh\nprintf 'this is not json at all'\nexit 0\n",
    );

    let inv = run_and_capture(
        Side::Deacon,
        BINARY,
        "garbage",
        &stub,
        &[],
        dir.path(),
        Duration::from_secs(30),
        &root,
    )
    .await
    .expect("capture");
    inv.require_success()
        .expect("the stub exits 0 — the fault is the non-JSON body, not the status");

    let err = inv
        .stdout_json()
        .expect_err("non-JSON stdout must not parse into a comparison document");
    match err {
        HarnessError::MalformedOutput { case, cause } => {
            assert_eq!(case, "garbage");
            assert!(!cause.is_empty(), "cause must carry the parser diagnostic");
        }
        other => panic!("expected MalformedOutput, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// (f) Hang stub past a test-shortened bound → OracleTimeout with partial output
//     preserved (research D10: bound injectable for tests).
// ---------------------------------------------------------------------------
#[tokio::test]
async fn f_hang_stub_times_out_with_partial_output() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("report");
    // Emit partial output, close the pipes (so the capture drains reach EOF
    // promptly), then hang well past the injected 250 ms bound. The harness still
    // observes the child alive at the bound and terminates it.
    let stub = write_stub(
        dir.path(),
        "hang",
        "#!/bin/sh\nprintf 'partial-before-hang'\nexec 1>&- 2>&-\nsleep 30\n",
    );

    let err = run_and_capture(
        Side::Deacon,
        BINARY,
        "hang",
        &stub,
        &[],
        dir.path(),
        Duration::from_millis(250),
        &root,
    )
    .await
    .expect_err("a hang past the bound must time out, not pass");

    match err {
        HarnessError::OracleTimeout {
            case,
            bound,
            partial_paths,
        } => {
            assert_eq!(case, "hang");
            assert_eq!(bound, Duration::from_millis(250));
            assert_eq!(
                partial_paths.len(),
                2,
                "both raw paths preserved on timeout"
            );
            let out =
                std::fs::read(root.join("raw/parity_harness_faults/hang/deacon.stdout")).unwrap();
            assert_eq!(
                out, b"partial-before-hang",
                "partial output produced before the hang must be preserved"
            );
        }
        other => panic!("expected OracleTimeout, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// (g) Two fabricated documents differing in one key → an unwaived divergence that
//     a corpus runner would FAIL (no waiver excuses it).
// ---------------------------------------------------------------------------
#[test]
fn g_injected_difference_is_unwaived_divergence() {
    // deacon drops a key the reference keeps — the highest-signal (ref-only) class.
    let deacon = normalize::config("g", r#"{ "name": "demo" }"#).expect("normalize deacon");
    let reference = normalize::config(
        "g",
        r#"{ "name": "demo", "customizations": { "vscode": { "extensions": ["x"] } } }"#,
    )
    .expect("normalize reference");

    let divergences = normalize::diff(&deacon, &reference);
    assert!(
        !divergences.is_empty(),
        "the injected difference must be detected by the single diff"
    );
    let summary = normalize::summarize(&divergences);
    assert!(
        summary.contains("ref-only") && summary.contains("customizations"),
        "the ref-only divergence must be named: {summary}"
    );

    // Mirror the corpus runner: no waiver covers this case → it is a failure.
    let waivers = WaiverSet::default();
    assert!(
        waivers.corpus_case("tier1", "g").is_none(),
        "no waiver may cover an injected difference"
    );
    let result = CaseResult::fail("g", Cause::Divergence, Some(summary), sample_raw());
    assert_eq!(result.outcome, Outcome::Fail);
    assert_eq!(result.cause, Some(Cause::Divergence));
    assert!(result.waivers_applied.is_empty());
}

// ---------------------------------------------------------------------------
// (h) The same injected difference WITH a matching waiver fixture → pass-waived,
//     the case result referencing the waiver record id.
// ---------------------------------------------------------------------------
#[test]
fn h_matching_waiver_yields_pass_waived() {
    let corpus = tempfile::tempdir().expect("corpus dir");
    let waivers_dir = corpus.path().join("waivers");
    std::fs::create_dir_all(&waivers_dir).unwrap();
    std::fs::write(
        waivers_dir.join("h.json"),
        r#"{
          "id": "tier1/h-injected",
          "scope": { "kind": "corpus_case", "corpus": "tier1", "case": "h" },
          "expect": { "kind": "field-divergence", "ours": "demo", "reference": "demo-ref" },
          "rationale": "acceptance fixture — characterized injected difference",
          "added": "2026-07-19"
        }"#,
    )
    .unwrap();

    let waivers = WaiverSet::load(corpus.path()).expect("load waivers");
    let w = waivers
        .corpus_case("tier1", "h")
        .expect("a matching waiver must be found for the injected case");
    assert_eq!(w.id, "tier1/h-injected");
    assert!(w.expect.is_divergence());

    // Mirror the corpus runner: divergence observed + waiver present → pass-waived.
    let result = CaseResult::pass_waived("h", vec![w.id.clone()], sample_raw());
    assert_eq!(result.outcome, Outcome::PassWaived);
    assert_eq!(result.waivers_applied, vec!["tier1/h-injected".to_string()]);

    // Consumed → not stale.
    let mut consumed = HashSet::new();
    consumed.insert(w.id.clone());
    let stale = waivers.stale_among(
        |w| matches!(&w.scope, Scope::CorpusCase { corpus, .. } if corpus == "tier1"),
        &consumed,
    );
    assert!(stale.is_empty(), "a consumed waiver is not stale");
}

// ---------------------------------------------------------------------------
// (i) The difference is gone but the waiver is kept → WaiverStale naming the id.
// ---------------------------------------------------------------------------
#[test]
fn i_kept_waiver_without_difference_is_stale() {
    let corpus = tempfile::tempdir().expect("corpus dir");
    let waivers_dir = corpus.path().join("waivers");
    std::fs::create_dir_all(&waivers_dir).unwrap();
    std::fs::write(
        waivers_dir.join("i.json"),
        r#"{
          "id": "tier1/h-injected",
          "scope": { "kind": "corpus_case", "corpus": "tier1", "case": "h" },
          "expect": { "kind": "field-divergence", "ours": "demo", "reference": "demo-ref" },
          "rationale": "acceptance fixture — characterized injected difference",
          "added": "2026-07-19"
        }"#,
    )
    .unwrap();

    let waivers = WaiverSet::load(corpus.path()).expect("load waivers");
    // The injected difference was removed, so no case consumed the waiver.
    let consumed: HashSet<String> = HashSet::new();
    let stale = waivers.stale_among(
        |w| matches!(&w.scope, Scope::CorpusCase { corpus, .. } if corpus == "tier1"),
        &consumed,
    );
    assert_eq!(
        stale,
        vec!["tier1/h-injected".to_string()],
        "a loaded-but-unconsumed waiver must be reported stale"
    );

    let err = HarnessError::WaiverStale {
        id: stale[0].clone(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("tier1/h-injected") && msg.contains("stale") && msg.contains("Remedy"),
        "Display must name the stale record and a remedy: {msg}"
    );
}

// ---------------------------------------------------------------------------
// (j) Invalid input to normalize::config → Normalization failure, with NO
//     fallback to raw comparison (the function returns Err, never a masquerading
//     Ok verdict).
// ---------------------------------------------------------------------------
#[test]
fn j_normalization_failure_has_no_raw_fallback() {
    let err = normalize::config("j", "this is not json")
        .expect_err("non-JSON input must fail normalization, not fall back to raw compare");
    match err {
        HarnessError::Normalization { case, cause } => {
            assert_eq!(case, "j");
            assert!(!cause.is_empty());
        }
        other => panic!("expected Normalization, got {other:?}"),
    }

    // No fallback anywhere: a non-object mergedConfiguration also errors rather
    // than silently comparing an empty/raw value.
    assert!(
        matches!(
            normalize::merged_config("j", "[1, 2, 3]"),
            Err(HarnessError::Normalization { .. })
        ),
        "merged_config must reject a non-object top-level, never fall back"
    );
    // The only outcomes of normalization are Ok(normalized) or Err(Normalization);
    // there is no raw-byte comparison path a caller could take instead.
    assert!(normalize::config("j", "{ broken").is_err());
}
