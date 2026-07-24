//! Hermetic runner tests (022-conformance-runner, US1 T016/T017/T018).
//!
//! The runner is driven over DECLARATIVE cases (data) against a stub "deacon" binary, so
//! adding a case never adds a Rust test function (SC-001). The stub stands in for the
//! real binary — a parity-harness test cannot expand `CARGO_BIN_EXE_deacon`; the real
//! read-configuration case runs against real deacon in the parity lane
//! (`parity_conformance_runner`). The record/replay-equivalence + 13-field-provenance
//! assertions land in User Story 2 (T030).

use deacon_conformance::model::{
    CHAN_EXIT_CODE, CHAN_STRUCTURED_OUTPUT, ExpectedObservable, Operation, OracleType, TestCase,
};
use parity_harness::evidence::{CaseVerdict, ChannelVerdict, Outcome};
use parity_harness::report::VerdictReport;

/// A declarative spec-expectation case: read-configuration on `fx-x`, asserting exit 0
/// and a structured-output subset. Pure data — the same shape a `cases.json` record is.
fn spec_case() -> TestCase {
    TestCase {
        id: "case-runner-spec".to_string(),
        behaviors: vec!["bhv-x".to_string()],
        oracle_type: Some(OracleType::SpecExpectation),
        operations: vec![Operation {
            id: "op-read".to_string(),
            subcommand: "read-configuration".to_string(),
            argv: vec!["--workspace-folder".to_string(), "${WORKSPACE}".to_string()],
            fixtures: vec!["fx-x".to_string()],
            ..Operation::default()
        }],
        expected: vec![
            ExpectedObservable {
                channel: CHAN_EXIT_CODE.to_string(),
                operation: Some("op-read".to_string()),
                assertion: Some(serde_json::json!({ "equals": 0 })),
            },
            ExpectedObservable {
                channel: CHAN_STRUCTURED_OUTPUT.to_string(),
                operation: Some("op-read".to_string()),
                assertion: Some(
                    serde_json::json!({ "jsonSubset": { "configuration": { "customUnknownKey": "preserved" } } }),
                ),
            },
        ],
        ..TestCase::default()
    }
}

// ------------------------------------------------------------------------------------
// T018: report determinism (no exec — construct verdicts directly, cross-platform).
// ------------------------------------------------------------------------------------

#[test]
fn verdict_report_is_byte_stable_and_path_free() {
    let verdict = CaseVerdict {
        case_id: "case-runner-spec".to_string(),
        oracle_type: OracleType::SpecExpectation,
        behaviors: vec!["bhv-x".to_string()],
        channels: vec![
            ChannelVerdict {
                channel: CHAN_EXIT_CODE.to_string(),
                outcome: Outcome::Agree,
                detail: None,
            },
            ChannelVerdict {
                channel: CHAN_STRUCTURED_OUTPUT.to_string(),
                outcome: Outcome::Agree,
                detail: None,
            },
        ],
        overall: Outcome::Agree,
        stale_allowed_differences: Vec::new(),
    };
    let report = VerdictReport::new(vec![verdict]);
    let a = report.render().expect("render");
    let b = report.render().expect("render again");
    assert_eq!(
        a, b,
        "the verdict report must be byte-stable across renders"
    );

    // No timestamps and no absolute paths in the body (contract runner-cli.md, T018).
    assert!(
        !a.contains("T00:") && !a.to_lowercase().contains("capturedat") && !a.contains("/tmp/"),
        "report body must carry no timestamps or absolute paths:\n{a}"
    );
    // Declaration order: exit-code precedes structured-output as declared.
    let exit_at = a.find("chan-exit-code").expect("exit-code present");
    let struct_at = a
        .find("chan-structured-output")
        .expect("structured present");
    assert!(exit_at < struct_at, "channels must be in declaration order");
    assert_eq!(report.exit_code(), 0, "all-agree report exits 0");
}

// ------------------------------------------------------------------------------------
// T016/T017: spec-expectation agree/diverge + failure-phase (stub deacon, Unix only —
// the stub is a POSIX shell script).
// ------------------------------------------------------------------------------------

#[cfg(unix)]
mod spec_expectation {
    use super::*;

    use std::path::{Path, PathBuf};

    use parity_harness::runner::{RunConfig, collect_spec_evidence, run_case};

    /// Write an executable stub "deacon" that prints `stdout` and exits with `code`.
    fn write_stub(dir: &Path, name: &str, stdout: &str, code: i32) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join(name);
        // `printf '%s'` avoids a trailing newline mattering; the runner trims for JSON.
        let body = format!("#!/bin/sh\nprintf '%s' '{stdout}'\nexit {code}\n");
        std::fs::write(&p, body).expect("write stub");
        let mut perms = std::fs::metadata(&p).expect("stat").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&p, perms).expect("chmod");
        p
    }

    /// Write a stub that echoes its `--workspace-folder` arg ($3) into the structured
    /// JSON, so the raw evidence carries the real (temp) workspace path.
    fn write_echo_workspace_stub(dir: &Path, name: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join(name);
        let body = "#!/bin/sh\nprintf '{\"configuration\":{\"customUnknownKey\":\"preserved\"},\"root\":\"%s\"}' \"$3\"\nexit 0\n";
        std::fs::write(&p, body).expect("write stub");
        let mut perms = std::fs::metadata(&p).expect("stat").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&p, perms).expect("chmod");
        p
    }

    /// Create `<root>/fixtures/fx-x/` so `${WORKSPACE}` resolves for the op.
    fn make_fixtures(root: &Path) -> PathBuf {
        let fixtures = root.join("fixtures");
        std::fs::create_dir_all(fixtures.join("fx-x")).expect("mkdir fixture");
        fixtures
    }

    const GOOD_STDOUT: &str = r#"{"configuration":{"customUnknownKey":"preserved","name":"x"}}"#;

    /// T041: raw and normalized evidence are persisted SEPARATELY and independently
    /// retrievable — raw keeps the real temp workspace path, normalized shows the
    /// `<WORKSPACE>` token (real per-channel normalization, not pass-through).
    #[tokio::test]
    async fn raw_and_normalized_evidence_are_separate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let stub = write_echo_workspace_stub(dir.path(), "deacon-echo");
        let fixtures = make_fixtures(dir.path());
        let cfg = RunConfig {
            deacon_path: &stub,
            oracle: None,
            fixtures_root: &fixtures,
            report_root: &dir.path().join("report"),
            snapshots_root: &dir.path().join("snapshots"),
        };
        let evidence = collect_spec_evidence(&spec_case(), &cfg)
            .await
            .expect("collect");

        let raw = evidence
            .raw_for(CHAN_STRUCTURED_OUTPUT, "op-read")
            .expect("raw structured evidence");
        let norm = evidence
            .normalized_for(CHAN_STRUCTURED_OUTPUT, "op-read")
            .expect("normalized structured evidence");

        // The real workspace path is fixtures/fx-x — raw preserves it verbatim.
        let ws = fixtures.join("fx-x");
        assert_eq!(
            raw.value["root"],
            serde_json::json!(ws.to_string_lossy()),
            "raw evidence preserves the temp workspace path"
        );
        assert_eq!(
            norm.value["root"],
            serde_json::json!("<WORKSPACE>"),
            "normalized evidence tokenizes the workspace path (FR-024)"
        );
        assert_ne!(
            raw.value, norm.value,
            "raw and normalized are separate (FR-016)"
        );
    }

    #[tokio::test]
    async fn correct_output_agrees() {
        let dir = tempfile::tempdir().expect("tempdir");
        let stub = write_stub(dir.path(), "deacon-good", GOOD_STDOUT, 0);
        let fixtures = make_fixtures(dir.path());
        let cfg = RunConfig {
            deacon_path: &stub,
            oracle: None,
            fixtures_root: &fixtures,
            report_root: &dir.path().join("report"),
            snapshots_root: &dir.path().join("snapshots"),
        };
        let verdict = run_case(&spec_case(), &cfg).await.expect("run");
        assert_eq!(
            verdict.overall,
            Outcome::Agree,
            "matching output must agree: {verdict:?}"
        );
    }

    #[tokio::test]
    async fn wrong_exit_code_diverges() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Emits the right JSON but exits 1 → the exit-code assertion `{equals:0}` fails.
        let stub = write_stub(dir.path(), "deacon-badexit", GOOD_STDOUT, 1);
        let fixtures = make_fixtures(dir.path());
        let cfg = RunConfig {
            deacon_path: &stub,
            oracle: None,
            fixtures_root: &fixtures,
            report_root: &dir.path().join("report"),
            snapshots_root: &dir.path().join("snapshots"),
        };
        let verdict = run_case(&spec_case(), &cfg).await.expect("run");
        assert_eq!(
            verdict.overall,
            Outcome::Diverge,
            "wrong exit code must diverge: {verdict:?}"
        );
        let exit = verdict
            .channels
            .iter()
            .find(|c| c.channel == CHAN_EXIT_CODE)
            .expect("exit-code channel");
        assert_eq!(exit.outcome, Outcome::Diverge);
    }

    /// A negative case: read-configuration fails (exit 1, non-JSON stdout). The runner
    /// records PARTIAL evidence (exit-code captured; structured-output not captured) and
    /// the correct closed-set FailurePhase (config-resolution) on the exit-code channel.
    fn failure_case() -> TestCase {
        let mut case = spec_case();
        case.id = "case-runner-failure".to_string();
        // Expect the failure: exit is non-zero; structured output is still declared, so
        // its absence is observed as a divergence (partial capture).
        case.expected[0].assertion = Some(serde_json::json!({ "nonZero": true }));
        case
    }

    #[tokio::test]
    async fn failed_op_records_partial_evidence_and_failure_phase() {
        let dir = tempfile::tempdir().expect("tempdir");
        let stub = write_stub(dir.path(), "deacon-fail", "error: bad config", 1);
        let fixtures = make_fixtures(dir.path());
        let cfg = RunConfig {
            deacon_path: &stub,
            oracle: None,
            fixtures_root: &fixtures,
            report_root: &dir.path().join("report"),
            snapshots_root: &dir.path().join("snapshots"),
        };
        let verdict = run_case(&failure_case(), &cfg).await.expect("run");

        // Exit-code channel: the op failed (exit 1) → `nonZero` agrees, and the
        // failure phase is recorded in the detail.
        let exit = verdict
            .channels
            .iter()
            .find(|c| c.channel == CHAN_EXIT_CODE)
            .expect("exit-code channel");
        assert_eq!(exit.outcome, Outcome::Agree, "nonZero matched the failure");
        let phase = exit
            .detail
            .as_ref()
            .and_then(|d| d.get("failurePhase"))
            .and_then(|p| p.as_str());
        assert_eq!(
            phase,
            Some("config-resolution"),
            "read-configuration failure phase must be config-resolution: {exit:?}"
        );

        // Structured-output channel: stdout was NOT valid JSON → partial capture, the
        // declared assertion diverges (never a silent pass).
        let structured = verdict
            .channels
            .iter()
            .find(|c| c.channel == CHAN_STRUCTURED_OUTPUT)
            .expect("structured channel");
        assert_eq!(structured.outcome, Outcome::Diverge);
    }
}

// ------------------------------------------------------------------------------------
// T030: record/replay equivalence for a snapshot-oracle case (stub reference + stub
// deacon, Unix only). Records a snapshot to a temp tree, then replays via the snapshot
// dispatch and asserts the SAME verdict (agree) + 13-field provenance.
// ------------------------------------------------------------------------------------

#[cfg(unix)]
mod snapshot_replay {
    use deacon_conformance::model::{CHAN_EXIT_CODE, ExpectedObservable, OracleType};
    use deacon_conformance::snapshot;

    use parity_harness::evidence::write_snapshot;
    use parity_harness::exec::Side;
    use parity_harness::runner::{RunConfig, capture_provenance, collect_evidence_on, run_case};

    use super::*;
    use std::path::{Path, PathBuf};

    fn write_exit0_stub(dir: &Path, name: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join(name);
        std::fs::write(&p, "#!/bin/sh\nexit 0\n").expect("write stub");
        let mut perms = std::fs::metadata(&p).expect("stat").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&p, perms).expect("chmod");
        p
    }

    fn snapshot_case() -> TestCase {
        TestCase {
            id: "case-replay".to_string(),
            behaviors: vec!["bhv-x".to_string()],
            oracle_type: Some(OracleType::Snapshot),
            operations: vec![Operation {
                id: "op-read".to_string(),
                subcommand: "read-configuration".to_string(),
                argv: vec!["--workspace-folder".to_string(), "${WORKSPACE}".to_string()],
                fixtures: vec!["fx-x".to_string()],
                ..Operation::default()
            }],
            expected: vec![ExpectedObservable {
                channel: CHAN_EXIT_CODE.to_string(),
                operation: Some("op-read".to_string()),
                assertion: None,
            }],
            ..TestCase::default()
        }
    }

    #[tokio::test]
    async fn recorded_case_replays_to_the_same_verdict_with_full_provenance() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("fixtures").join("fx-x")).expect("fixture");
        let fixtures = dir.path().join("fixtures");
        let snapshots = dir.path().join("snapshots");
        let reports = dir.path().join("report");
        let case = snapshot_case();

        // A stub that exits 0 stands in for BOTH the reference (record) and deacon
        // (replay) — the recorded exit-code (0) must equal deacon's on replay.
        let stub = write_exit0_stub(dir.path(), "exit0");
        let cfg = RunConfig {
            deacon_path: &stub,
            oracle: None,
            fixtures_root: &fixtures,
            report_root: &reports,
            snapshots_root: &snapshots,
        };

        // RECORD: capture the reference evidence + provenance and write the snapshot for
        // the CURRENT os-arch (so the replay dispatch resolves it).
        let evidence = collect_evidence_on(Side::Oracle, &stub, &case, &cfg)
            .await
            .expect("record evidence");
        let provenance = capture_provenance(&case, &cfg, "0.87.0").expect("provenance");
        let os_arch = snapshot::current_os_arch();
        let case_dir = snapshot::snapshot_case_dir(&snapshots, &os_arch, &case.id);
        write_snapshot(&case_dir, &provenance, &evidence)
            .await
            .expect("write snapshot");

        // Provenance carries all 13 fields (SC-002) — reload and check the non-derived ones.
        let reloaded = snapshot::load_provenance(&case_dir).expect("provenance loads");
        assert_eq!(reloaded.oracle_version, "0.87.0");
        assert_eq!(reloaded.normalizer_version, snapshot::NORMALIZER_VERSION);
        assert_eq!(reloaded.source_revision, "113500f4");
        assert_eq!(reloaded.case_hash.len(), 64);
        assert_eq!(reloaded.fixture_hash.len(), 64);
        assert!(!reloaded.captured_at.is_empty());

        // REPLAY: the snapshot dispatch resolves the committed snapshot, gates on
        // provenance freshness (same machine → fresh), runs deacon (exit 0), and compares
        // to the recorded exit-code (0) → agree. Record/replay equivalence (SC-011).
        let verdict = run_case(&case, &cfg).await.expect("replay");
        assert_eq!(
            verdict.overall,
            Outcome::Agree,
            "a recorded case replays to the same (agree) verdict: {verdict:?}"
        );
    }
}

// ------------------------------------------------------------------------------------
// T066: one case under all four oracle types applies DISTINCT semantics, and re-pointing
// changes ONLY `oracleType` (not the operations/expected shape). Unix (stub deacon).
// ------------------------------------------------------------------------------------

#[cfg(unix)]
mod oracle_types {
    use super::*;

    use std::path::{Path, PathBuf};

    use parity_harness::oracle_type::evaluate;
    use parity_harness::runner::{RunConfig, run_case};

    fn exit0_stub(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("deacon-exit0");
        std::fs::write(&p, "#!/bin/sh\nexit 0\n").expect("write stub");
        let mut perms = std::fs::metadata(&p).expect("stat").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&p, perms).expect("chmod");
        p
    }

    /// A base case (read-configuration, no ${WORKSPACE}, exit-code assertion) whose
    /// operations + expected are IDENTICAL across every oracle type — only `oracleType`
    /// changes between the variants below (FR-007).
    fn base_case(oracle_type: OracleType) -> TestCase {
        TestCase {
            id: "case-repointed".to_string(),
            behaviors: vec!["bhv-x".to_string()],
            oracle_type: Some(oracle_type),
            operations: vec![Operation {
                id: "op".to_string(),
                subcommand: "read-configuration".to_string(),
                ..Operation::default()
            }],
            expected: vec![ExpectedObservable {
                channel: CHAN_EXIT_CODE.to_string(),
                operation: Some("op".to_string()),
                assertion: Some(serde_json::json!({ "equals": 0 })),
            }],
            ..TestCase::default()
        }
    }

    #[test]
    fn re_pointing_changes_only_oracle_type() {
        let a = base_case(OracleType::SpecExpectation);
        let b = base_case(OracleType::LiveDifferential);
        let c = base_case(OracleType::Snapshot);
        let d = base_case(OracleType::InvariantMetamorphic);
        // Every variant is identical EXCEPT `oracleType` — re-pointing is a one-field edit.
        for other in [&b, &c, &d] {
            assert_eq!(
                a.operations, other.operations,
                "operations must be identical"
            );
            assert_eq!(a.expected, other.expected, "expected must be identical");
            assert_eq!(a.fixtures_shape(), other.fixtures_shape());
            assert_ne!(a.oracle_type, other.oracle_type, "only oracleType differs");
        }
    }

    #[tokio::test]
    async fn four_oracle_types_apply_distinct_semantics() {
        let dir = tempfile::tempdir().expect("tempdir");
        let stub = exit0_stub(dir.path());
        let empty_snapshots = dir.path().join("snapshots"); // no committed snapshots

        // spec-expectation: runs deacon, evaluates the assertion → a real Agree verdict.
        let spec_cfg = RunConfig {
            deacon_path: &stub,
            oracle: None,
            fixtures_root: dir.path(),
            report_root: &dir.path().join("report"),
            snapshots_root: &empty_snapshots,
        };
        let spec = run_case(&base_case(OracleType::SpecExpectation), &spec_cfg)
            .await
            .expect("spec-expectation runs");
        assert_eq!(
            spec.overall,
            Outcome::Agree,
            "spec-expectation compares to the declared assertion: {spec:?}"
        );

        // live-differential WITHOUT an oracle → fail-loud OracleMissing (needs a reference).
        let live = evaluate(&base_case(OracleType::LiveDifferential), &spec_cfg).await;
        assert!(
            matches!(
                live,
                Err(parity_harness::HarnessError::OracleMissing { .. })
            ),
            "live-differential requires the reference oracle: {live:?}"
        );

        // snapshot with no committed snapshot for this platform → no-reference-for-platform
        // (a distinct coverage-gap outcome, not a verdict against a fixed value).
        let (snap_channels, _stale) = evaluate(&base_case(OracleType::Snapshot), &spec_cfg)
            .await
            .expect("snapshot resolves to a verdict, not an error");
        assert!(
            snap_channels
                .iter()
                .all(|c| c.outcome == Outcome::NoReferenceForPlatform),
            "snapshot with no committed reference is no-reference-for-platform: {snap_channels:?}"
        );

        // invariant-metamorphic with NO declared relationship → fail-loud (the relationship
        // IS the oracle; there is nothing to evaluate against a fixed value).
        let meta = evaluate(&base_case(OracleType::InvariantMetamorphic), &spec_cfg).await;
        assert!(
            meta.is_err(),
            "invariant-metamorphic needs a declared relationship: {meta:?}"
        );
    }
}

/// A shape fingerprint of a case's fixtures for the re-pointing assertion (helper trait).
trait FixturesShape {
    fn fixtures_shape(&self) -> Vec<Vec<String>>;
}
impl FixturesShape for TestCase {
    fn fixtures_shape(&self) -> Vec<Vec<String>> {
        self.operations.iter().map(|o| o.fixtures.clone()).collect()
    }
}
