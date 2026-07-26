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
    let deacon =
        normalize::config("g", r#"{ "name": "demo" }"#, Side::Deacon).expect("normalize deacon");
    let reference = normalize::config(
        "g",
        r#"{ "name": "demo", "customizations": { "vscode": { "extensions": ["x"] } } }"#,
        Side::Deacon,
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
          "id": "wvr-injected-h",
          "behaviors": ["bhv-injected-h"],
          "scope": { "kind": "corpus_case", "corpus": "tier1", "case": "h" },
          "expect": { "kind": "field-divergence", "ours": "demo", "reference": "demo-ref" },
          "rationale": "acceptance fixture — characterized injected difference",
          "added": "2026-07-19",
          "expires": "2027-01-19"
        }"#,
    )
    .unwrap();

    let waivers = WaiverSet::load(corpus.path()).expect("load waivers");
    let w = waivers
        .corpus_case("tier1", "h")
        .expect("a matching waiver must be found for the injected case");
    assert_eq!(w.id, "wvr-injected-h");
    assert!(w.expect.is_divergence());

    // Mirror the corpus runner: divergence observed + waiver present → pass-waived.
    let result = CaseResult::pass_waived("h", vec![w.id.clone()], sample_raw());
    assert_eq!(result.outcome, Outcome::PassWaived);
    assert_eq!(result.waivers_applied, vec!["wvr-injected-h".to_string()]);

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
          "id": "wvr-injected-h",
          "behaviors": ["bhv-injected-h"],
          "scope": { "kind": "corpus_case", "corpus": "tier1", "case": "h" },
          "expect": { "kind": "field-divergence", "ours": "demo", "reference": "demo-ref" },
          "rationale": "acceptance fixture — characterized injected difference",
          "added": "2026-07-19",
          "expires": "2027-01-19"
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
        vec!["wvr-injected-h".to_string()],
        "a loaded-but-unconsumed waiver must be reported stale"
    );

    let err = HarnessError::WaiverStale {
        id: stale[0].clone(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("wvr-injected-h") && msg.contains("stale") && msg.contains("Remedy"),
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
// T055 (US4, FR-018 / FR-056): one hermetic case per DIFFERENCE class.
//
// The fault-injection binary already proves the process-level causes (a–f) and the
// waiver/normalization pipeline (g–j). What it did not prove is that each
// *difference* class is reported with its own classification — which is exactly
// what research D3 says was being lost: `deacon-only` was ranked last as "usually
// default noise" and `prune` deleted it outright when empty. Each leg below injects
// one class synthetically and asserts it is classified AS that class and named.
// ===========================================================================

/// (k) `ref-only` — the reference emits a key deacon does not. Historically framed as
/// the highest-signal class ("deacon drops data"), and it must stay reported as its own
/// class rather than folded into a generic "differs".
#[test]
fn k_reference_only_difference_is_classified_as_ref_only() {
    let deacon = normalize::config("k", r#"{ "name": "demo" }"#, Side::Deacon).expect("normalize");
    let reference = normalize::config(
        "k",
        r#"{ "name": "demo", "remoteUser": "vscode" }"#,
        Side::Deacon,
    )
    .expect("normalize");

    let divergences = normalize::diff(&deacon, &reference);
    assert_eq!(divergences.len(), 1, "{divergences:?}");
    assert_eq!(divergences[0].kind, normalize::DiffKind::RefOnly);
    assert_eq!(divergences[0].path, "remoteUser");
    assert!(divergences[0].deacon.is_none() && divergences[0].reference.is_some());

    let summary = normalize::summarize(&divergences);
    assert!(
        summary.contains("ref-only") && summary.contains("remoteUser"),
        "the class and the path must both be named: {summary}"
    );
}

/// (l) `deacon-only` — deacon emits a key the reference does not. **This is the class
/// FR-020 protects.** It must be reported with its own classification, and (023 T065) it
/// must no longer sort below `value` on the grounds of being noise.
#[test]
fn l_deacon_only_difference_is_classified_and_not_deprioritized() {
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

    let divergences = normalize::diff(&deacon, &reference);
    assert_eq!(
        divergences.len(),
        1,
        "an unlisted empty deacon-only key must be REPORTED, not pruned away: \
         {divergences:?}"
    );
    assert_eq!(divergences[0].kind, normalize::DiffKind::DeaconOnly);
    assert_eq!(divergences[0].path, "someNewProperty");

    let summary = normalize::summarize(&divergences);
    assert!(
        summary.contains("deacon-only") && summary.contains("the reference does not emit"),
        "deacon-only must read as a finding, not a shrug: {summary}"
    );

    // FR-020 / 023 T065: ordering must not place deacon-only last as "default noise".
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
    let kinds: Vec<_> = normalize::diff(&mixed_deacon, &mixed_reference)
        .iter()
        .map(|d| d.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            normalize::DiffKind::RefOnly,
            normalize::DiffKind::DeaconOnly,
            normalize::DiffKind::Value
        ],
        "deacon-only must not be ranked last as noise (FR-020)"
    );
}

/// (m) `value` — the same key, different values. Must not be collapsed into either
/// one-sided class, and must report BOTH sides so the difference is diagnosable.
#[test]
fn m_value_difference_is_classified_with_both_sides() {
    let deacon =
        normalize::config("m", r#"{ "name": "demo-a" }"#, Side::Deacon).expect("normalize");
    let reference =
        normalize::config("m", r#"{ "name": "demo-b" }"#, Side::Deacon).expect("normalize");

    let divergences = normalize::diff(&deacon, &reference);
    assert_eq!(divergences.len(), 1);
    assert_eq!(divergences[0].kind, normalize::DiffKind::Value);
    assert_eq!(
        (
            divergences[0].deacon.clone(),
            divergences[0].reference.clone()
        ),
        (
            Some(serde_json::json!("demo-a")),
            Some(serde_json::json!("demo-b"))
        ),
        "a value difference must carry both sides to be diagnosable"
    );

    let summary = normalize::summarize(&divergences);
    assert!(
        summary.contains("value") && summary.contains("demo-a") && summary.contains("demo-b"),
        "{summary}"
    );
}

/// (n) accept-vs-reject, WITH DIRECTION — the decision-class difference the error corpus
/// turns on. `deacon-stricter` and `reference-stricter` are distinct outcomes and a
/// waiver characterizing one must NOT waive the other; only the right direction applies.
#[test]
fn n_accept_vs_reject_difference_preserves_direction() {
    use parity_harness::waiver::{Expect, Waiver};

    fn waiver(id: &str, expect: Expect) -> Waiver {
        Waiver {
            id: id.to_string(),
            behaviors: vec!["bhv-readconfig-malformed-jsonc-rejected".to_string()],
            scope: Scope::CorpusCase {
                corpus: "errors".to_string(),
                case: "n".to_string(),
            },
            expect,
            rationale: "fault-injection probe".to_string(),
            added: "2026-07-25".to_string(),
            expires: "2027-07-25".to_string(),
            config: None,
        }
    }

    // The direction predicate every corpus runner applies, mirrored here so the
    // classification is asserted rather than assumed.
    fn applies(expect: &Expect, deacon_ok: bool, oracle_ok: bool) -> bool {
        match expect {
            Expect::ReferenceStricter { .. } => deacon_ok && !oracle_ok,
            Expect::DeaconStricter { .. } => !deacon_ok && oracle_ok,
            Expect::BothReject {} | Expect::BothAccept {} | Expect::FieldDivergence { .. } => false,
        }
    }

    let deacon_stricter = waiver("wvr-probe-deacon", Expect::DeaconStricter { signal: None });
    let reference_stricter = waiver(
        "wvr-probe-reference",
        Expect::ReferenceStricter { signal: None },
    );

    // deacon rejects, the reference accepts → only `deacon-stricter` characterizes it.
    assert!(applies(&deacon_stricter.expect, false, true));
    assert!(
        !applies(&reference_stricter.expect, false, true),
        "a reference-stricter waiver must NOT waive a deacon-stricter difference — the \
         direction IS the finding"
    );

    // The inverse direction, symmetrically.
    assert!(applies(&reference_stricter.expect, true, false));
    assert!(!applies(&deacon_stricter.expect, true, false));

    // An agreement expectation never characterizes a decision DIFFERENCE at all.
    for agreement in [Expect::BothReject {}, Expect::BothAccept {}] {
        let w = waiver("wvr-probe-agree", agreement);
        assert!(!applies(&w.expect, false, true));
        assert!(!applies(&w.expect, true, false));
    }

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
    use deacon_conformance::model::AllowedDifference;
    use parity_harness::compare::{Tolerances, verdict_differential};
    use parity_harness::evidence::{NormalizedChannelEvidence, Outcome as DeclOutcome};

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
    use deacon_conformance::snapshot::{Provenance, Staleness, compare_staleness};

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
        "normalizerVersion": deacon_conformance::snapshot::NORMALIZER_VERSION,
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
