//! Hermetic tests for the `parity-report` aggregator + six-condition gate
//! (018-harden-parity-harness, T035; FR-018, contracts/report-schema.md).
//!
//! Every case fabricates fragments (and, where relevant, waivers) in a temp report
//! dir — NO live oracle, Docker, or network is touched. They prove the aggregator
//! passes a clean run and fails each gate: a missing fragment, a cross-fragment
//! oracle mismatch, a stale waiver, an unexplained omission, a corpus below its
//! minimum, and an unwritable report directory.

use std::fs;

use parity_harness::HarnessError;
use parity_harness::aggregate::{self, evaluate};
use parity_harness::oracle::{OraclePin, OracleSource};
use parity_harness::registry::{Corpus, LiveBinary, LiveKind, ParityRegistry};
use parity_harness::report::{
    CaseResult, Cause, Omission, OracleInfo, RawPaths, ReportFragment, now_rfc3339,
};
use parity_harness::waiver::WaiverSet;

const CORPUS_MIN: usize = 2;

fn pin() -> OraclePin {
    OraclePin {
        package: "@devcontainers/cli".into(),
        version: "0.87.0".into(),
    }
}

/// A two-binary registry: one scenario runner and one corpus runner whose corpus
/// requires at least [`CORPUS_MIN`] cases.
fn registry() -> ParityRegistry {
    ParityRegistry {
        live_binaries: vec![
            LiveBinary {
                name: "parity_read_configuration".into(),
                kind: LiveKind::Scenario,
                docker_required: false,
                corpus: None,
            },
            LiveBinary {
                name: "parity_corpus_tier1".into(),
                kind: LiveKind::Corpus,
                docker_required: false,
                corpus: Some("tier1".into()),
            },
        ],
        internal_consistency_binaries: vec![],
        corpora: vec![Corpus {
            id: "tier1".into(),
            path: "fixtures/parity-corpus".into(),
            min_cases: CORPUS_MIN,
        }],
    }
}

fn oracle_info(version: &str, path: &str) -> OracleInfo {
    OracleInfo {
        version: version.into(),
        path: path.into(),
        source: OracleSource::PathLookup,
    }
}

fn raw() -> RawPaths {
    RawPaths {
        deacon_stdout: "raw/b/c/deacon.stdout".into(),
        deacon_stderr: "raw/b/c/deacon.stderr".into(),
        oracle_stdout: "raw/b/c/oracle.stdout".into(),
        oracle_stderr: "raw/b/c/oracle.stderr".into(),
    }
}

/// A fragment with `n` clean passes against the pinned oracle at a fixed path.
fn passing(binary: &str, path: &str, n: usize) -> ReportFragment {
    let cases = (0..n)
        .map(|i| CaseResult::pass(format!("case-{i}"), raw()))
        .collect();
    ReportFragment::new(
        binary,
        oracle_info("0.87.0", path),
        now_rfc3339(),
        now_rfc3339(),
        cases,
        Vec::new(),
    )
}

const ORACLE_PATH: &str = "/usr/local/bin/devcontainer";

// --- 1. all-green: full read → evaluate → write pipeline via a temp report dir ---

#[tokio::test]
async fn all_green_run_certifies_and_writes_report() {
    let report_dir = tempfile::tempdir().expect("report dir");
    let corpus_dir = tempfile::tempdir().expect("corpus dir"); // no errors/ or waivers/

    // Fabricate one fragment per registered live binary.
    passing("parity_read_configuration", ORACLE_PATH, 2)
        .write_under(report_dir.path())
        .await
        .expect("write scenario fragment");
    passing("parity_corpus_tier1", ORACLE_PATH, CORPUS_MIN)
        .write_under(report_dir.path())
        .await
        .expect("write corpus fragment");

    let agg = aggregate::run(report_dir.path(), corpus_dir.path(), &registry(), &pin())
        .await
        .expect("aggregation runs");

    assert!(
        agg.violations.is_empty(),
        "clean run must certify, got: {:?}",
        agg.violations
    );
    assert!(agg.report.missing_fragments.is_empty());
    assert!(agg.report.stale_waivers.is_empty());
    assert_eq!(agg.report.oracle.verified_version, "0.87.0");
    assert_eq!(agg.report.totals.cases, 4);
    assert_eq!(agg.report.totals.passed, 4);
    assert!(
        report_dir.path().join("parity-report.json").is_file(),
        "gate 6: the aggregated report must be written"
    );
}

// --- 2. a registered live binary that produced no fragment ---

#[test]
fn missing_fragment_fails_gate_one() {
    let fragments = vec![passing("parity_read_configuration", ORACLE_PATH, 1)];
    let (report, violations) = evaluate(&registry(), &pin(), &fragments, &WaiverSet::default());

    assert_eq!(report.missing_fragments, vec!["parity_corpus_tier1"]);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("gate 1") && v.contains("parity_corpus_tier1")),
        "missing fragment must be enumerated, got: {violations:?}"
    );
}

// --- 3. present fragments disagree on the oracle across the run ---

#[test]
fn oracle_mismatch_across_fragments_fails_gate_two() {
    // Same pinned version, but two different resolved oracle paths.
    let fragments = vec![
        passing(
            "parity_read_configuration",
            "/usr/local/bin/devcontainer",
            1,
        ),
        passing("parity_corpus_tier1", "/opt/other/devcontainer", CORPUS_MIN),
    ];
    let (_report, violations) = evaluate(&registry(), &pin(), &fragments, &WaiverSet::default());
    assert!(
        violations.iter().any(|v| v.contains("oracle path")),
        "cross-fragment path disagreement must fail gate 2, got: {violations:?}"
    );

    // And a fragment whose version differs from the pin.
    let fragments = vec![
        passing("parity_read_configuration", ORACLE_PATH, 1),
        ReportFragment::new(
            "parity_corpus_tier1",
            oracle_info("0.86.0", ORACLE_PATH),
            now_rfc3339(),
            now_rfc3339(),
            vec![CaseResult::pass("a", raw()), CaseResult::pass("b", raw())],
            Vec::new(),
        ),
    ];
    let (_report, violations) = evaluate(&registry(), &pin(), &fragments, &WaiverSet::default());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("gate 2") && v.contains("0.86.0")),
        "version-vs-pin mismatch must fail gate 2, got: {violations:?}"
    );
}

// --- 4. a loaded waiver that no fragment applied ---

#[tokio::test]
async fn stale_waiver_fails_gate_four() {
    let report_dir = tempfile::tempdir().expect("report dir");
    let corpus_dir = tempfile::tempdir().expect("corpus dir");

    // A schema-valid waiver record that no fabricated case references.
    let waivers = corpus_dir.path().join("waivers");
    fs::create_dir_all(&waivers).unwrap();
    fs::write(
        waivers.join("stale.json"),
        r#"{
          "id": "wvr-state-unused",
          "behaviors": ["bhv-state-unused"],
          "scope": { "kind": "state_field", "binary": "parity_observable_state",
                     "fixture": "f", "field": "label:x" },
          "expect": { "kind": "field-divergence", "ours": "a", "reference": "b" },
          "rationale": "test fixture — intentionally never applied",
          "added": "2026-07-19", "expires": "2027-01-19"
        }"#,
    )
    .unwrap();

    // Otherwise-clean run: both live binaries present, correct oracle, min met.
    passing("parity_read_configuration", ORACLE_PATH, 1)
        .write_under(report_dir.path())
        .await
        .unwrap();
    passing("parity_corpus_tier1", ORACLE_PATH, CORPUS_MIN)
        .write_under(report_dir.path())
        .await
        .unwrap();

    let agg = aggregate::run(report_dir.path(), corpus_dir.path(), &registry(), &pin())
        .await
        .expect("aggregation runs");

    assert_eq!(agg.report.stale_waivers, vec!["wvr-state-unused"]);
    assert!(
        agg.violations
            .iter()
            .any(|v| v.contains("gate 4") && v.contains("wvr-state-unused")),
        "stale waiver must be enumerated, got: {:?}",
        agg.violations
    );
}

// --- 5. an omitted case without a reason ---

#[test]
fn unexplained_omission_fails_gate_three() {
    let with_omission = ReportFragment::new(
        "parity_read_configuration",
        oracle_info("0.87.0", ORACLE_PATH),
        now_rfc3339(),
        now_rfc3339(),
        vec![CaseResult::pass("ok", raw())],
        vec![Omission {
            case: "skipped".into(),
            reason: "   ".into(), // whitespace-only == unexplained
        }],
    );
    let fragments = vec![
        with_omission,
        passing("parity_corpus_tier1", ORACLE_PATH, CORPUS_MIN),
    ];
    let (_report, violations) = evaluate(&registry(), &pin(), &fragments, &WaiverSet::default());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("gate 3") && v.contains("without a reason")),
        "unexplained omission must fail gate 3, got: {violations:?}"
    );

    // An omission WITH a reason is explained and must NOT fail.
    let explained = ReportFragment::new(
        "parity_read_configuration",
        oracle_info("0.87.0", ORACLE_PATH),
        now_rfc3339(),
        now_rfc3339(),
        vec![CaseResult::pass("ok", raw())],
        vec![Omission {
            case: "skipped".into(),
            reason: "docker unavailable in this lane".into(),
        }],
    );
    let fragments = vec![
        explained,
        passing("parity_corpus_tier1", ORACLE_PATH, CORPUS_MIN),
    ];
    let (_report, violations) = evaluate(&registry(), &pin(), &fragments, &WaiverSet::default());
    assert!(
        violations.is_empty(),
        "explained omission must NOT fail, got: {violations:?}"
    );
}

// --- 6. a corpus fragment below its registered minimum ---

#[test]
fn corpus_below_minimum_fails_gate_five() {
    let fragments = vec![
        passing("parity_read_configuration", ORACLE_PATH, 1),
        passing("parity_corpus_tier1", ORACLE_PATH, CORPUS_MIN - 1), // one short
    ];
    let (_report, violations) = evaluate(&registry(), &pin(), &fragments, &WaiverSet::default());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("gate 5") && v.contains("below the registered minimum")),
        "corpus below minimum must fail gate 5, got: {violations:?}"
    );
}

// --- 6b. a failing case is enumerated (gate 3) ---

#[test]
fn failing_case_fails_gate_three() {
    let failing = ReportFragment::new(
        "parity_read_configuration",
        oracle_info("0.87.0", ORACLE_PATH),
        now_rfc3339(),
        now_rfc3339(),
        vec![CaseResult::fail(
            "diverging",
            Cause::Divergence,
            Some("value mismatch at forwardPorts[1]".into()),
            raw(),
        )],
        Vec::new(),
    );
    let fragments = vec![
        failing,
        passing("parity_corpus_tier1", ORACLE_PATH, CORPUS_MIN),
    ];
    let (report, violations) = evaluate(&registry(), &pin(), &fragments, &WaiverSet::default());
    assert_eq!(report.totals.failed, 1);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("gate 3") && v.contains("diverging")),
        "failing case must be enumerated, got: {violations:?}"
    );
}

// --- 7. the report directory cannot be written ---

#[cfg(unix)]
#[tokio::test]
async fn unwritable_report_dir_fails_gate_six() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let ro = dir.path().join("ro");
    fs::create_dir(&ro).unwrap();

    // A minimal but valid aggregated report to attempt to persist.
    let (report, _violations) = evaluate(&registry(), &pin(), &[], &WaiverSet::default());

    let mut perms = fs::metadata(&ro).unwrap().permissions();
    perms.set_mode(0o555); // read+execute, no write
    fs::set_permissions(&ro, perms).unwrap();

    let err = aggregate::write_report(&ro, &report)
        .await
        .expect_err("writing under a read-only dir must fail");
    assert!(
        matches!(err, HarnessError::Report { .. }),
        "expected a Report write failure (gate 6), got {err:?}"
    );

    // Restore so the TempDir can clean itself up.
    let mut perms = fs::metadata(&ro).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&ro, perms).unwrap();
}
