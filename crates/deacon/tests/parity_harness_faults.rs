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
//! - (g) injected differing documents → an untolerated divergence naming its path;
//! - (j) invalid input to `normalize::config` → `Normalization` (no raw fallback).
//!
//! Legs (h) and (i) — a matching waiver yields `pass-waived`, and a kept-but-unmatched
//! waiver is stale — retired with the corpus waiver model they exercised. Their
//! properties are asserted against the model that actually runs, in leg (o): a covered
//! difference is `allowed-difference` naming its backing id, and an unconsumed tolerance
//! is reported STALE.
//!
//! Hermetic: NO live oracle, NO real Docker, NO network — stub executables and env
//! overrides only. The oracle/docker/timeout legs rely on nextest's process-per-
//! test isolation (the mandated runner; this binary is selected only in hermetic
//! nextest lanes) so each `Oracle::acquire()` sees a fresh process-wide cache and
//! each `DEACON_PARITY_*` override is scoped to its own test process.
//!
//! Unix-only (whole file): the fault stubs are `#!/bin/sh` scripts made executable
//! via POSIX mode bits (per the repo's Windows notes on stub-script tests). The
//! pipeline legs (g–j) exercise pure `normalize`/`compare` code that is additionally
//! covered cross-platform by the harness crate's own unit tests.
#![cfg(unix)]

use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use parity_harness::HarnessError;
use parity_harness::compare::{Tolerances, verdict_differential};
use parity_harness::evidence::{NormalizedChannelEvidence, Outcome as DeclOutcome};
use parity_harness::exec::{Side, run_and_capture};
use parity_harness::model::CHAN_STRUCTURED_OUTPUT;
use parity_harness::normalize;
use parity_harness::oracle::{ORACLE_OVERRIDE_ENV, Oracle};
use parity_harness::prereq::{DOCKER_OVERRIDE_ENV, require_docker};
use parity_harness::report::{CaseResult, Cause, Outcome, RawPaths};

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

/// Placeholder raw-artifact paths for pipeline-only `CaseResult`s: those legs assert
/// classification, not artifact bytes (that is `raw_outputs.rs`).
fn sample_raw() -> RawPaths {
    RawPaths {
        deacon_stdout: "raw/parity_harness_faults/c/deacon.stdout".into(),
        deacon_stderr: "raw/parity_harness_faults/c/deacon.stderr".into(),
        oracle_stdout: "raw/parity_harness_faults/c/oracle.stdout".into(),
        oracle_stderr: "raw/parity_harness_faults/c/oracle.stderr".into(),
    }
}

/// The observable paths on which two normalized documents differ, reached through the
/// PRODUCTION comparison with no tolerances declared.
///
/// The difference-class legs below used to call a second, ranked differ that lived in
/// `normalize` and classified each difference as `ref-only` / `deacon-only` / `value`.
/// That differ is retired: the declarative comparison names the diverging PATH and leaves
/// both sides' values in the preserved evidence. What these legs must still prove is that
/// each SHAPE of difference is detected and named — most of all the deacon-only one, which
/// a comparison treating the reference as the truth would silently drop (FR-020).
fn diverging_paths(deacon: &serde_json::Value, reference: &serde_json::Value) -> Vec<String> {
    let side = |value: &serde_json::Value| NormalizedChannelEvidence {
        channel: CHAN_STRUCTURED_OUTPUT.to_string(),
        operation: "op-read".to_string(),
        present: true,
        value: value.clone(),
    };
    let no_tolerances = Tolerances::new(&[], &[]);
    let mut consumed = HashSet::new();
    let verdict = verdict_differential(
        CHAN_STRUCTURED_OUTPUT,
        &side(deacon),
        &side(reference),
        &no_tolerances,
        &mut consumed,
    );
    let prefix = format!("{CHAN_STRUCTURED_OUTPUT}.");
    match verdict.outcome {
        DeclOutcome::Agree => Vec::new(),
        DeclOutcome::Diverge => verdict
            .detail
            .as_ref()
            .and_then(|d| d.get("divergingPaths"))
            .and_then(|p| p.as_array())
            .expect("a differential divergence names its paths")
            .iter()
            .map(|p| {
                let p = p.as_str().expect("a diverging path is a string");
                p.strip_prefix(&prefix).unwrap_or(p).to_string()
            })
            .collect(),
        other => panic!("no tolerance was declared, so {other:?} is unreachable"),
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
// (g) Two fabricated documents differing in one key → an untolerated divergence that
//     the runner FAILS, naming the path it differed on.
// ---------------------------------------------------------------------------
#[test]
fn g_injected_difference_is_an_untolerated_divergence() {
    // deacon drops a key the reference keeps.
    let deacon =
        normalize::config("g", r#"{ "name": "demo" }"#, Side::Deacon).expect("normalize deacon");
    let reference = normalize::config(
        "g",
        r#"{ "name": "demo", "customizations": { "vscode": { "extensions": ["x"] } } }"#,
        Side::Deacon,
    )
    .expect("normalize reference");

    // With no tolerance declared, the difference is DETECTED and its path is NAMED —
    // a divergence reported without a path is one nobody can act on, and it is also one
    // no scoped tolerance could ever be written against.
    assert_eq!(
        diverging_paths(&deacon, &reference),
        vec!["customizations".to_string()],
        "the injected difference must be detected by the single comparison and named"
    );

    // Which the driver turns into a failure carrying its cause.
    let result = CaseResult::fail(
        "g",
        Cause::Divergence,
        Some("chan-structured-output.customizations".to_string()),
        sample_raw(),
    );
    assert_eq!(result.outcome, Outcome::Fail);
    assert_eq!(result.cause, Some(Cause::Divergence));
}

// ---------------------------------------------------------------------------
// (j) Invalid input to normalize::config → Normalization failure, with NO
//     fallback to raw comparison (the function returns Err, never a masquerading
//     Ok verdict).
// ---------------------------------------------------------------------------
#[test]
fn j_normalization_failure_has_no_raw_fallback() {
    let err = normalize::config("j", "this is not json", Side::Deacon)
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
            normalize::merged_config("j", "[1, 2, 3]", Side::Deacon),
            Err(HarnessError::Normalization { .. })
        ),
        "merged_config must reject a non-object top-level, never fall back"
    );
    // The only outcomes of normalization are Ok(normalized) or Err(Normalization);
    // there is no raw-byte comparison path a caller could take instead.
    assert!(normalize::config("j", "{ broken", Side::Deacon).is_err());
}

// ===========================================================================
// T055 (US4, FR-018 / FR-056): one hermetic case per SHAPE of difference.
//
// The fault-injection binary already proves the process-level causes (a–f) and the
// normalization pipeline (g, j). What it did not prove is that each *shape* of
// difference is detected and named — which is exactly what research D3 says was being
// lost: `deacon-only` was ranked last as "usually default noise" and `prune` deleted it
// outright when empty. Each leg below injects one shape synthetically and asserts it is
// reported with the path it occurred on.
//
// These legs used to assert a `DiffKind` classification stamped on the verdict by a
// second, ranked differ. That differ is retired; the class is now read off the preserved
// evidence — the two sides' values are both there — while the verdict names the path.
// The substance is unchanged: no shape may be silently absorbed.
// ===========================================================================

/// (k) The reference emits a key deacon does not. Historically framed as the
/// highest-signal shape ("deacon drops data").
#[test]
fn k_reference_only_difference_is_detected_and_named() {
    let deacon = normalize::config("k", r#"{ "name": "demo" }"#, Side::Deacon).expect("normalize");
    let reference = normalize::config(
        "k",
        r#"{ "name": "demo", "remoteUser": "vscode" }"#,
        Side::Deacon,
    )
    .expect("normalize");

    assert_eq!(
        diverging_paths(&deacon, &reference),
        vec!["remoteUser".to_string()]
    );
    // Both sides remain readable in the evidence, so the shape is diagnosable.
    assert!(deacon.get("remoteUser").is_none());
    assert_eq!(reference["remoteUser"], serde_json::json!("vscode"));
}

/// (l) deacon emits a key the reference does not. **This is the shape FR-020 protects.**
/// It must be reported at all — a comparison that took the reference as the truth, or a
/// blanket empty-value prune, would drop it — and (023 T065) it must not be treated as
/// less interesting than the others.
#[test]
fn l_deacon_only_difference_is_detected_and_not_deprioritized() {
    // `someNewProperty` is deliberately NOT on the enumerated `ABSENT_OPTIONAL_KEYS`
    // list, so it is compared — the property retiring `prune` restored (023 T062).
    let deacon = normalize::config(
        "l",
        r#"{ "name": "demo", "someNewProperty": {} }"#,
        Side::Deacon,
    )
    .expect("normalize");
    let reference =
        normalize::config("l", r#"{ "name": "demo" }"#, Side::Deacon).expect("normalize");

    assert_eq!(
        diverging_paths(&deacon, &reference),
        vec!["someNewProperty".to_string()],
        "an unlisted EMPTY deacon-only key must be REPORTED, not pruned away"
    );

    // FR-020 / 023 T065: with all three shapes present at once, every one is reported.
    // Ordering is by path — a deterministic display order, not a significance ranking, so
    // no shape can sort below another on the grounds of being noise.
    let mixed_deacon = normalize::config(
        "l",
        r#"{ "name": "a", "someNewProperty": 1 }"#,
        Side::Deacon,
    )
    .expect("normalize");
    let mixed_reference = normalize::config(
        "l",
        r#"{ "name": "b", "remoteUser": "vscode" }"#,
        Side::Deacon,
    )
    .expect("normalize");
    assert_eq!(
        diverging_paths(&mixed_deacon, &mixed_reference),
        vec![
            "name".to_string(),            // differing value
            "remoteUser".to_string(),      // reference-only
            "someNewProperty".to_string(), // deacon-only
        ],
        "no shape may be absorbed when several occur at once"
    );
}

/// (m) The same key with different values. Must not be collapsed into either one-sided
/// shape, and both sides must survive in the evidence so the difference is diagnosable.
#[test]
fn m_value_difference_is_named_with_both_sides_readable() {
    let deacon =
        normalize::config("m", r#"{ "name": "demo-a" }"#, Side::Deacon).expect("normalize");
    let reference =
        normalize::config("m", r#"{ "name": "demo-b" }"#, Side::Deacon).expect("normalize");

    assert_eq!(
        diverging_paths(&deacon, &reference),
        vec!["name".to_string()]
    );
    assert_eq!(
        (deacon["name"].clone(), reference["name"].clone()),
        (serde_json::json!("demo-a"), serde_json::json!("demo-b")),
        "a value difference must leave both sides in the evidence to be diagnosable"
    );
}

/// (n) accept-vs-reject — the decision-class difference the error-path cases turn on. It
/// lands on `chan-exit-code`, and it must be ADDRESSABLE: the channel carries one scalar,
/// so a bare channel id would be the diverging path, and a bare channel id is exactly
/// what `AllowedDifference::is_global_ignore` rejects (FR-032). Naming the observable
/// `chan-exit-code.exitCode` is what makes an exit-code difference expressible as a
/// scoped tolerance at all; four committed tolerances spelling it were inert before.
///
/// Direction (deacon rejected vs the reference rejected) used to live in a waiver's
/// `Expect` enum, which had to distinguish the two so one waiver could not excuse the
/// other. In the declarative model a case IS one direction — it pins one fixture and one
/// pair of expectations — so the direction is carried by the case, and what remains to
/// prove here is that both directions are reported and neither is absorbed.
#[test]
fn n_accept_vs_reject_difference_is_reported_and_addressable() {
    use parity_harness::model::{AllowedDifference, CHAN_EXIT_CODE};

    fn exit_code(code: i64) -> NormalizedChannelEvidence {
        NormalizedChannelEvidence {
            channel: CHAN_EXIT_CODE.to_string(),
            operation: "op-read".to_string(),
            present: true,
            value: serde_json::json!({ "exitCode": code }),
        }
    }

    let no_tolerances = Tolerances::new(&[], &[]);
    // deacon rejects, the reference accepts — and the inverse. Both are divergences.
    for (deacon_code, reference_code) in [(1, 0), (0, 1)] {
        let mut consumed = HashSet::new();
        let verdict = verdict_differential(
            CHAN_EXIT_CODE,
            &exit_code(deacon_code),
            &exit_code(reference_code),
            &no_tolerances,
            &mut consumed,
        );
        assert_eq!(
            verdict.outcome,
            DeclOutcome::Diverge,
            "deacon={deacon_code} reference={reference_code} must diverge"
        );
        let paths = verdict.detail.expect("a divergence names its paths");
        assert_eq!(
            paths["divergingPaths"],
            serde_json::json!(["chan-exit-code.exitCode"]),
            "the one observable of a scalar channel must be NAMED, or no scoped tolerance \
             can ever address it (FR-032)"
        );
    }

    // And a tolerance written against that named observable does cover it.
    let tolerance = AllowedDifference {
        behavior: "bhv-probe-decision".to_string(),
        context: Vec::new(),
        observable_path: "chan-exit-code.exitCode".to_string(),
        rationale: "fault-injection probe".to_string(),
        waiver_id: Some("wvr-probe-decision".to_string()),
        divergence_id: None,
    };
    let behaviors = vec!["bhv-probe-decision".to_string()];
    let tolerances = Tolerances::new(std::slice::from_ref(&tolerance), &behaviors);
    let mut consumed = HashSet::new();
    let verdict = verdict_differential(
        CHAN_EXIT_CODE,
        &exit_code(1),
        &exit_code(0),
        &tolerances,
        &mut consumed,
    );
    assert_eq!(verdict.outcome, DeclOutcome::AllowedDifference);

    // An unwaived decision difference is a hard failure carrying its own cause.
    let result = CaseResult::fail(
        "n",
        Cause::Divergence,
        Some("deacon rejected, the reference accepted".to_string()),
        sample_raw(),
    );
    assert_eq!(result.outcome, Outcome::Fail);
    assert_eq!(result.cause, Some(Cause::Divergence));
}

// ===========================================================================
// T056 (US4, FR-019 / FR-023 / FR-056): one hermetic case per DECLARATIVE
// process-level outcome not covered by (a)–(j).
//
// Legs (a)–(f) cover the legacy `report::Cause` vocabulary. The declarative runner
// has its own outcome vocabulary (`evidence::Outcome`), and three of its members —
// `AllowedDifference`, `NoReferenceForPlatform`, `Stale` — had no hermetic proof
// that they stay DISTINCT from `Agree` and from `Diverge`. Conflating any of them
// with `Agree` is precisely the "reported a pass when no comparison happened"
// failure FR-023 forbids.
// ===========================================================================

/// (o) `AllowedDifference` — a divergence fully covered by a scoped tolerance is its OWN
/// outcome, distinct from `Agree` (a real difference was found) and from `Diverge` (it is
/// characterized). The backing identity must be named, and an UNCOVERED path must keep
/// the verdict at `Diverge`.
#[test]
fn o_allowed_difference_is_distinct_from_agree_and_names_its_backing_id() {
    use parity_harness::compare::{Tolerances, verdict_differential};
    use parity_harness::evidence::{NormalizedChannelEvidence, Outcome as DeclOutcome};
    use parity_harness::model::AllowedDifference;

    fn evidence(value: serde_json::Value) -> NormalizedChannelEvidence {
        NormalizedChannelEvidence {
            channel: "chan-structured-output".to_string(),
            operation: "op-read".to_string(),
            present: true,
            value,
        }
    }

    let deacon = evidence(serde_json::json!({ "a": 1, "b": 1 }));
    let reference = evidence(serde_json::json!({ "a": 2, "b": 1 }));

    let tolerance = AllowedDifference {
        behavior: "bhv-probe".to_string(),
        context: Vec::new(),
        observable_path: "chan-structured-output.a".to_string(),
        rationale: "fault-injection probe".to_string(),
        waiver_id: Some("wvr-probe".to_string()),
        divergence_id: None,
    };
    let behaviors = vec!["bhv-probe".to_string()];

    // Covered → `allowed-difference`, naming the backing id. NOT `agree`: a difference
    // WAS found.
    let tolerances = Tolerances::new(std::slice::from_ref(&tolerance), &behaviors);
    let mut consumed = HashSet::new();
    let verdict = verdict_differential(
        "chan-structured-output",
        &deacon,
        &reference,
        &tolerances,
        &mut consumed,
    );
    assert_eq!(verdict.outcome, DeclOutcome::AllowedDifference);
    assert_ne!(
        verdict.outcome,
        DeclOutcome::Agree,
        "a tolerated difference is NOT agreement — conflating them reports a pass where a \
         difference exists (FR-023)"
    );
    let detail = verdict
        .detail
        .expect("an allowed difference carries its backing id");
    assert!(
        detail.to_string().contains("wvr-probe"),
        "the backing waiver id must appear in the detail: {detail}"
    );
    assert_eq!(consumed.len(), 1, "the tolerance is recorded as consumed");

    // UNCOVERED path → stays `diverge`. A tolerance is scoped, never global (FR-032).
    let elsewhere = AllowedDifference {
        observable_path: "chan-structured-output.zzz".to_string(),
        ..tolerance.clone()
    };
    let narrow = Tolerances::new(std::slice::from_ref(&elsewhere), &behaviors);
    let mut unconsumed = HashSet::new();
    let strict = verdict_differential(
        "chan-structured-output",
        &deacon,
        &reference,
        &narrow,
        &mut unconsumed,
    );
    assert_eq!(
        strict.outcome,
        DeclOutcome::Diverge,
        "a tolerance for a different path must not absorb this divergence"
    );

    // An unconsumed tolerance is reported STALE — the same self-invalidation as a waiver
    // (FR-034).
    assert_eq!(
        narrow.stale(&unconsumed).len(),
        1,
        "a tolerance whose difference did not reproduce must be reported stale"
    );
    assert!(
        tolerances.stale(&consumed).is_empty(),
        "a consumed tolerance is not stale"
    );
}

/// (p) `NoReferenceForPlatform` — no committed snapshot exists for this `os-arch`. It is a
/// COVERAGE GAP with its own outcome, and must never be reported as `Agree` (nothing was
/// compared) nor as `Stale` (nothing drifted).
#[test]
fn p_no_reference_for_platform_is_its_own_outcome() {
    use parity_harness::evidence::{ChannelVerdict, Outcome as DeclOutcome};

    let verdict = ChannelVerdict {
        channel: "chan-exit-code".to_string(),
        outcome: DeclOutcome::NoReferenceForPlatform,
        detail: Some(serde_json::json!({ "platform": "linux-aarch64" })),
        stderr_excerpt: None,
    };
    assert_ne!(
        verdict.outcome,
        DeclOutcome::Agree,
        "no reference means NO COMPARISON HAPPENED; reporting agreement would be the \
         silent pass FR-023 forbids"
    );
    assert_ne!(
        verdict.outcome,
        DeclOutcome::Stale,
        "absent is not stale — a missing snapshot is a coverage gap, a stale one is drift"
    );
    assert_ne!(verdict.outcome, DeclOutcome::Diverge);

    // The wire spelling is stable, so a report consumer can tell the three apart.
    for (outcome, wire) in [
        (DeclOutcome::Agree, "\"agree\""),
        (DeclOutcome::Diverge, "\"diverge\""),
        (DeclOutcome::AllowedDifference, "\"allowed-difference\""),
        (
            DeclOutcome::NoReferenceForPlatform,
            "\"no-reference-for-platform\"",
        ),
        (DeclOutcome::Stale, "\"stale\""),
        (DeclOutcome::Error, "\"error\""),
    ] {
        assert_eq!(
            serde_json::to_string(&outcome).expect("outcome serializes"),
            wire,
            "each declarative outcome needs a distinct, stable spelling"
        );
    }
}

/// (q) `Stale` — a committed snapshot whose evidence-determining provenance drifted. It
/// must be reported as `Stale`, naming the drifted field, and never silently replayed as
/// `Agree`.
#[test]
fn q_stale_snapshot_is_reported_naming_the_drifted_field() {
    use parity_harness::provenance::{Provenance, Staleness, compare_staleness};

    // Built by deserialization so this test needs no `indexmap` dependency of its own —
    // the shape is the committed `provenance.json` shape.
    let recorded: Provenance = serde_json::from_value(serde_json::json!({
        "caseHash": "aaaa",
        "fixtureHash": "bbbb",
        "oracleVersion": "0.87.0",
        "sourceRevision": "113500f4",
        "nodeVersion": "v22.0.0",
        "dockerVersion": "29.0.0",
        "composeVersion": "2.0.0",
        "imageDigests": {},
        "normalizerVersion": parity_harness::provenance::NORMALIZER_VERSION,
        "argv": ["read-configuration"],
        "platform": "linux",
        "arch": "x86_64",
        "capturedAt": "2026-01-01T00:00:00Z",
    }))
    .expect("provenance shape");

    // Fresh against itself.
    assert_eq!(
        compare_staleness(&recorded, &recorded),
        Staleness::Fresh,
        "identical provenance is not stale"
    );

    // An EVIDENCE-DETERMINING field drifting must be reported as stale, NAMING the field
    // — a boolean would leave the reviewer to guess what changed.
    let mut drifted = recorded.clone();
    drifted.case_hash = "cccc".to_string();
    match compare_staleness(&recorded, &drifted) {
        Staleness::Stale { field, .. } => assert_eq!(field, "caseHash"),
        Staleness::Fresh => panic!("a drifted caseHash must be stale, never replayed as agree"),
    }

    let mut normalizer_drift = recorded.clone();
    normalizer_drift.normalizer_version = "999".to_string();
    match compare_staleness(&recorded, &normalizer_drift) {
        Staleness::Stale { field, .. } => assert_eq!(
            field, "normalizerVersion",
            "a normalizer change invalidates recorded evidence — this is why retiring \
             `prune` and narrowing the id rule (023 T062/T063) bumps NORMALIZER_VERSION"
        ),
        Staleness::Fresh => panic!("a normalizer change must invalidate the snapshot"),
    }

    // A SELECTOR / informational field drifting is NOT staleness: gating on the host's
    // node/docker versions or the capture time would make every snapshot stale on every
    // machine but the recorder's, breaking cross-machine replay.
    let mut informational = recorded.clone();
    informational.node_version = "v20.0.0".to_string();
    informational.docker_version = "1.0.0".to_string();
    informational.compose_version = "9.9.9".to_string();
    informational.captured_at = "2020-01-01T00:00:00Z".to_string();
    assert_eq!(
        compare_staleness(&recorded, &informational),
        Staleness::Fresh,
        "host tool versions and capture time are informational, not evidence-determining"
    );
}
