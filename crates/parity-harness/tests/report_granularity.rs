//! 024 D-1 (FR-047): a binary's per-case evidence survives concurrent writers, and the
//! aggregator sees every case the binary actually reported.
//!
//! The defect this guards: a `ReportFragment` used to be written to one
//! `report/<binary>.json`. A carrier like `parity_state_diff` has eight `#[tokio::test]`
//! functions, each building a fragment holding ONE case, and under nextest each runs in its
//! own process. Last writer won — the on-disk fragment held 1 case of 8, and the
//! aggregator's per-binary index collapsed same-binary fragments the same way. That is
//! precisely the reported-granularity-below-asserted-granularity defect spec 023 existed to
//! fix, alive and unguarded in the carriers 023 did not retire.
//!
//! Hermetic: tempdirs only. No Docker, no oracle, no network.

use parity_harness::aggregate::read_fragments;
use parity_harness::oracle::OracleSource;
use parity_harness::report::{CaseResult, Omission, OracleInfo, RawPaths, ReportFragment};

fn oracle() -> OracleInfo {
    OracleInfo {
        version: "0.87.0".to_string(),
        path: "/usr/local/bin/devcontainer".to_string(),
        source: OracleSource::PathLookup,
    }
}

fn raw() -> RawPaths {
    RawPaths {
        deacon_stdout: "raw/d.out".to_string(),
        deacon_stderr: "raw/d.err".to_string(),
        oracle_stdout: "raw/o.out".to_string(),
        oracle_stderr: "raw/o.err".to_string(),
    }
}

fn fragment(binary: &str, case: &str, started: &str, finished: &str) -> ReportFragment {
    ReportFragment::new(
        binary,
        oracle(),
        started.to_string(),
        finished.to_string(),
        vec![CaseResult::pass(case, raw())],
        vec![],
    )
}

/// The headline guard: eight independent single-case writers, as eight nextest processes
/// would do it, must yield eight reported cases — not one.
#[tokio::test]
async fn independent_writers_for_one_binary_all_survive() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    let cases = [
        "single-container-parity",
        "compose-parity-with-feature-mount-gap",
        "intra-deacon-single-vs-compose",
        "default-workspace-mount-target-parity",
        "dockerfile-build-and-nonroot-user",
        "appport-published-ports",
        "mount-variety-readonly-and-tmpfs",
        "compose-sidecar-and-named-volume",
    ];
    for case in cases {
        fragment(
            "parity_state_diff",
            case,
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:01:00Z",
        )
        .write_under(root)
        .await
        .expect("write");
    }

    let fragments = read_fragments(root).expect("fragments read");
    assert_eq!(
        fragments.len(),
        1,
        "the aggregator indexes one fragment per binary, so the pieces must merge"
    );
    let reported: Vec<&str> = fragments[0].cases.iter().map(|c| c.case.as_str()).collect();
    assert_eq!(
        reported.len(),
        cases.len(),
        "every independently reported case must survive; got {reported:?}"
    );
    for case in cases {
        assert!(
            reported.contains(&case),
            "case `{case}` was lost: {reported:?}"
        );
    }
}

/// Merging must be deterministic regardless of directory-read order.
#[tokio::test]
async fn merged_cases_are_sorted_deterministically() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    for case in ["zeta", "alpha", "mid"] {
        fragment(
            "parity_build",
            case,
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:01:00Z",
        )
        .write_under(root)
        .await
        .expect("write");
    }
    let reported: Vec<String> = read_fragments(root).expect("read")[0]
        .cases
        .iter()
        .map(|c| c.case.clone())
        .collect();
    assert_eq!(reported, vec!["alpha", "mid", "zeta"]);
}

/// Timestamps widen to cover the whole run, and omissions are preserved exactly once.
#[tokio::test]
async fn metadata_widens_and_omissions_survive() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    fragment(
        "parity_exec",
        "early",
        "2026-01-01T00:00:05Z",
        "2026-01-01T00:00:09Z",
    )
    .write_under(root)
    .await
    .expect("write");

    let mut late = fragment(
        "parity_exec",
        "late",
        "2026-01-01T00:00:20Z",
        "2026-01-01T00:00:30Z",
    );
    late.omitted = vec![Omission {
        case: "skipped".to_string(),
        reason: "the fixture needs a local registry service".to_string(),
    }];
    late.write_under(root).await.expect("write");

    let merged = &read_fragments(root).expect("read")[0];
    assert_eq!(
        merged.started, "2026-01-01T00:00:05Z",
        "earliest start wins"
    );
    assert_eq!(
        merged.finished, "2026-01-01T00:00:30Z",
        "latest finish wins"
    );
    assert_eq!(merged.cases.len(), 2);
    assert_eq!(
        merged.omitted.len(),
        1,
        "an omission is recorded once, not duplicated per case file"
    );
}

/// Two fragments of one binary certifying against different oracles is not something to
/// average over — the run is not comparable against a single pin.
#[tokio::test]
async fn disagreeing_oracle_versions_fail_loud() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    fragment(
        "parity_build",
        "a",
        "2026-01-01T00:00:00Z",
        "2026-01-01T00:01:00Z",
    )
    .write_under(root)
    .await
    .expect("write");

    let mut other = fragment(
        "parity_build",
        "b",
        "2026-01-01T00:00:00Z",
        "2026-01-01T00:01:00Z",
    );
    other.oracle = OracleInfo {
        version: "0.99.0".to_string(),
        path: "/usr/local/bin/devcontainer".to_string(),
        source: OracleSource::PathLookup,
    };
    other.write_under(root).await.expect("write");

    let err = read_fragments(root).expect_err("two oracle versions must fail loud");
    let message = err.to_string();
    assert!(
        message.contains("parity_build") && message.contains("0.99.0"),
        "the diagnosis must name the binary and the conflicting version: {message}"
    );
}

/// A case id containing a path separator must not escape the report directory.
#[tokio::test]
async fn a_case_id_cannot_escape_the_report_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    fragment(
        "parity_exec",
        "../../escaped",
        "2026-01-01T00:00:00Z",
        "2026-01-01T00:01:00Z",
    )
    .write_under(root)
    .await
    .expect("write");

    assert!(
        !root.join("escaped.json").exists()
            && !root.parent().unwrap().join("escaped.json").exists(),
        "a case id with path separators must be sanitized, never written outside the tree"
    );
    let merged = &read_fragments(root).expect("read")[0];
    assert_eq!(
        merged.cases[0].case, "../../escaped",
        "the real case id is carried INSIDE the file, so sanitizing the name loses nothing"
    );
}

// ---------------------------------------------------------------------------
// Gate 7 — reported granularity, against the REAL frozen baseline and mapping.
// ---------------------------------------------------------------------------

use parity_harness::aggregate::check_reported_granularity;

fn registry_root() -> std::path::PathBuf {
    deacon_conformance::default_registry_dir()
}

/// The pre-fix world: `parity_state_diff` asserts eight things and reports one. Gate 7 must
/// name each of the seven unwitnessed units — that is the whole point of the gate, and the
/// exact shape of the 3.6x under-count spec 023 was written to end.
#[test]
fn gate_7_catches_a_carrier_that_reports_fewer_cases_than_it_carries() {
    let under_reporting = ReportFragment::new(
        "parity_state_diff",
        oracle(),
        "2026-01-01T00:00:00Z".to_string(),
        "2026-01-01T00:01:00Z".to_string(),
        vec![CaseResult::pass("single-container-parity", raw())],
        vec![],
    );
    let violations =
        check_reported_granularity(&registry_root(), &[under_reporting]).expect("gate runs");

    assert!(
        violations.len() >= 7,
        "seven of eight units went unreported; gate 7 must name each: {violations:#?}"
    );
    for unit in [
        "appport-published-ports",
        "compose-sidecar-and-named-volume",
        "mount-variety-readonly-and-tmpfs",
    ] {
        assert!(
            violations.iter().any(|v| v.contains(unit)),
            "gate 7 must name the specific missing unit `{unit}`: {violations:#?}"
        );
    }
    assert!(
        violations.iter().all(|v| v.contains("gate 7")),
        "every violation must be attributable to its gate"
    );
}

/// A carrier reporting everything it still carries is clean.
#[test]
fn gate_7_passes_when_every_carried_unit_is_reported() {
    let cases = [
        "single-container-parity",
        "compose-parity-with-feature-mount-gap",
        "intra-deacon-single-vs-compose",
        "default-workspace-mount-target-parity",
        "dockerfile-build-and-nonroot-user",
        "appport-published-ports",
        "mount-variety-readonly-and-tmpfs",
        "compose-sidecar-and-named-volume",
    ];
    let complete = ReportFragment::new(
        "parity_state_diff",
        oracle(),
        "2026-01-01T00:00:00Z".to_string(),
        "2026-01-01T00:01:00Z".to_string(),
        cases.iter().map(|c| CaseResult::pass(*c, raw())).collect(),
        vec![],
    );
    let violations = check_reported_granularity(&registry_root(), &[complete]).expect("gate runs");
    assert!(
        violations.is_empty(),
        "a carrier reporting every unit it carries is clean: {violations:#?}"
    );
}

/// An EXPLAINED omission is not a silent loss — gate 3 already judges the reason, so gate 7
/// must not double-report it.
#[test]
fn gate_7_accepts_an_explained_omission() {
    let mut fragment = ReportFragment::new(
        "parity_state_diff",
        oracle(),
        "2026-01-01T00:00:00Z".to_string(),
        "2026-01-01T00:01:00Z".to_string(),
        vec![CaseResult::pass("single-container-parity", raw())],
        vec![],
    );
    fragment.omitted = [
        "compose-parity-with-feature-mount-gap",
        "intra-deacon-single-vs-compose",
        "default-workspace-mount-target-parity",
        "dockerfile-build-and-nonroot-user",
        "appport-published-ports",
        "mount-variety-readonly-and-tmpfs",
        "compose-sidecar-and-named-volume",
    ]
    .iter()
    .map(|c| Omission {
        case: (*c).to_string(),
        reason: "docker unavailable on this runner".to_string(),
    })
    .collect();

    let violations = check_reported_granularity(&registry_root(), &[fragment]).expect("gate runs");
    assert!(
        violations.is_empty(),
        "an explained omission is gate 3's business, not gate 7's: {violations:#?}"
    );
}

/// Extras are the transitional dual-path state, not a violation: a unit may be migrated to a
/// declarative case AND still exercised by its legacy carrier until that carrier is deleted.
#[test]
fn gate_7_tolerates_a_reported_case_the_baseline_no_longer_expects() {
    let fragment = ReportFragment::new(
        "parity_up_exec",
        oracle(),
        "2026-01-01T00:00:00Z".to_string(),
        "2026-01-01T00:01:00Z".to_string(),
        vec![CaseResult::pass("traditional", raw())],
        vec![],
    );
    let violations = check_reported_granularity(&registry_root(), &[fragment]).expect("gate runs");
    assert!(
        violations.is_empty(),
        "`traditional` is migrated, so the legacy carrier still reporting it is the \
         transitional state the migration passes through: {violations:#?}"
    );
}
