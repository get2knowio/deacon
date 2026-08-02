//! Prerequisite-fault guards for the container-backed error-path tier
//! (024-deterministic-conformance-coverage, US4 T097, FR-044).
//!
//! FR-044 states the rule negatively — "when the container runtime or the pinned reference
//! is unavailable, selection of this tier MUST fail with a cause-specific error; skipping
//! and passing are both forbidden" — and a negative rule needs both halves checked:
//!
//! 1. **The prerequisite failures are cause-specific.** An absent daemon, a daemon that
//!    cannot answer, an oracle reporting the wrong version, and an oracle that cannot
//!    report one at all are four different facts, and each must arrive as its own
//!    `HarnessError` naming what to do about it. `018-harden-parity-harness` established
//!    this for the *resolution* path and unit-tests it inside `oracle.rs`/`prereq.rs`;
//!    what is checked here is that the same failures reach a CALLER as errors, through the
//!    public seams the drivers use.
//! 2. **Nothing downstream can turn one into a green.** This is the half that is specific
//!    to the error-path tier, and the half a taxonomy test cannot see: the tier's cases
//!    must be *selected* by a driver that checks the prerequisites, its verdict vocabulary
//!    must have no word for "skipped", and a differential run without an oracle must fail
//!    rather than quietly compare deacon against nothing.
//!
//! Hermetic: pure functions, the real committed registry, and (Unix) POSIX-shell stub
//! binaries. No Docker, no oracle, no network — deliberately, because a guard that needed
//! the prerequisites could not observe their absence.

use std::path::{Path, PathBuf};
use std::time::Duration;

use parity_harness::load::Registry;
use parity_harness::model::{CaseKind, ResourceGroup, TestCase};
use parity_harness::{default_registry_dir, workspace_root};

use parity_harness::HarnessError;
use parity_harness::evidence::Outcome;
use parity_harness::oracle::{OraclePin, verify_binary};
use parity_harness::prereq::{DOCKER_PROBE_BOUND, probe_docker};
use parity_harness::runner::{RunConfig, run_case};

fn real_registry() -> Registry {
    Registry::load(&default_registry_dir()).expect("the real conformance registry loads")
}

fn error_path_cases(registry: &Registry) -> Vec<&TestCase> {
    registry
        .cases
        .iter()
        .filter(|c| c.error_path_tier)
        .collect()
}

// ---------------------------------------------------------------------------------
// (1) Each prerequisite failure is its own named cause.
// ---------------------------------------------------------------------------------

/// An absent container runtime is [`HarnessError::DockerMissing`], and its message names
/// both the missing thing and the remedy — not a boolean a caller could treat as "skip".
#[tokio::test]
async fn an_absent_container_runtime_is_a_named_cause() {
    let err = probe_docker(
        Path::new("/deacon-conformance/no-such-docker"),
        DOCKER_PROBE_BOUND,
    )
    .await
    .expect_err("an absent docker binary must fail, never report availability");
    assert!(
        matches!(err, HarnessError::DockerMissing),
        "expected DockerMissing, got {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("Docker") && msg.contains("Remedy"),
        "the failure must name the prerequisite and what to do about it: {msg}"
    );
}

/// A daemon that is *present but cannot answer* is the same fact as an absent one, and must
/// not be mistaken for availability. This is the shape a broken or unauthorized Docker
/// socket takes, which is far more common in practice than a missing binary.
#[cfg(unix)]
#[tokio::test]
async fn a_container_runtime_that_cannot_answer_is_unavailable_not_available() {
    let dir = tempfile::tempdir().expect("tempdir");
    let stub = stub(
        dir.path(),
        "docker",
        "printf 'cannot connect\\n' >&2\nexit 1\n",
    );
    let err = probe_docker(&stub, DOCKER_PROBE_BOUND)
        .await
        .expect_err("a failing docker CLI must be reported unavailable");
    assert!(matches!(err, HarnessError::DockerMissing), "{err:?}");
}

/// An oracle reporting a version other than the pin fails naming BOTH versions.
///
/// Naming both is what makes the failure actionable, and it is also what keeps the run
/// honest: a message saying only "wrong oracle" invites the reader to assume theirs is
/// close enough, and a parity verdict is only meaningful against exactly the pinned
/// reference.
#[cfg(unix)]
#[tokio::test]
async fn a_mismatched_oracle_version_fails_naming_both_versions() {
    let pin = OraclePin::load().expect("the embedded oracle pin parses");
    let wrong = "0.1.2";
    assert_ne!(pin.version, wrong, "the stub must not accidentally match");

    let dir = tempfile::tempdir().expect("tempdir");
    let stub = stub(
        dir.path(),
        "devcontainer",
        &format!("printf '{wrong}\\n'\n"),
    );
    let err = verify_binary(&stub, &pin, Duration::from_secs(30))
        .await
        .expect_err("a version other than the pin must fail verification");
    match &err {
        HarnessError::OracleVersionMismatch {
            found, required, ..
        } => {
            assert_eq!(found, wrong);
            assert_eq!(required, &pin.version);
        }
        other => panic!("expected OracleVersionMismatch, got {other:?}"),
    }
    let msg = err.to_string();
    assert!(msg.contains(wrong) && msg.contains(&pin.version), "{msg}");
}

/// An oracle that cannot report a version at all is `OracleUnverifiable` — a DIFFERENT
/// fact from a mismatch, because the remedies differ (repair the install vs install the
/// pin). Collapsing the two would send a reader to the wrong one.
#[cfg(unix)]
#[tokio::test]
async fn an_oracle_that_cannot_report_a_version_is_unverifiable() {
    let pin = OraclePin::load().expect("the embedded oracle pin parses");
    let dir = tempfile::tempdir().expect("tempdir");
    let stub = stub(
        dir.path(),
        "devcontainer",
        "printf 'broken\\n' >&2\nexit 7\n",
    );
    let err = verify_binary(&stub, &pin, Duration::from_secs(30))
        .await
        .expect_err("an oracle that cannot answer `--version` must fail verification");
    assert!(
        matches!(err, HarnessError::OracleUnverifiable { .. }),
        "expected OracleUnverifiable, got {err:?}"
    );
}

// ---------------------------------------------------------------------------------
// (2) Nothing downstream can turn a missing prerequisite into a pass.
// ---------------------------------------------------------------------------------

/// Every error-path case is selected by a driver that checks the prerequisites first.
///
/// `resourceGroup` is the ONLY discriminator partitioning cases between the two live
/// binaries, and only the Docker-backed driver calls `require_docker` + `Oracle::acquire`
/// before selecting anything. A tier case that drifted into `none` or `fs-heavy` would be
/// run by the binary that is *defined* to need no daemon — so the tier would execute (and
/// report) with neither prerequisite ever checked. That is FR-044's forbidden "pass"
/// arriving through scheduling rather than through an error being swallowed.
#[test]
fn every_error_path_case_is_selected_behind_the_prerequisite_checks() {
    let registry = real_registry();
    let cases = error_path_cases(&registry);
    assert!(
        cases.len() >= 9,
        "SC-007 needs the tier to span build, container creation, Feature installation, \
         lifecycle execution and teardown; found only {} case(s)",
        cases.len()
    );
    for case in &cases {
        let group = case.resource_group.unwrap_or(ResourceGroup::None);
        assert!(
            parity_harness::driver::needs_docker(group),
            "{}: is in the error-path tier but its resource group `{}` is driven by the \
             binary that needs no daemon, so neither Docker nor the pinned oracle would be \
             checked before it ran (FR-044)",
            case.id,
            parity_harness::driver::group_slug(group)
        );
    }
}

/// The verdict vocabulary has no word for "skipped".
///
/// Written over the closed [`Outcome`] set rather than as prose, so a future variant has to
/// justify itself against this test. `no-reference-for-platform` is the near miss and is
/// deliberately NOT a skip: it names a *coverage gap* (no snapshot recorded for this
/// platform), the driver surfaces it as a note, and it is reported rather than silently
/// dropped. A genuine skip would be an outcome that says nothing and costs nothing.
#[test]
fn the_verdict_vocabulary_cannot_express_a_skip() {
    const ALL: &[Outcome] = &[
        Outcome::Agree,
        Outcome::Diverge,
        Outcome::AllowedDifference,
        Outcome::NoReferenceForPlatform,
        Outcome::Stale,
        Outcome::Error,
    ];
    for outcome in ALL {
        let word = outcome.as_str();
        assert!(
            !word.contains("skip") && !word.contains("ignor") && !word.contains("pending"),
            "outcome {word:?} reads as a skip; FR-044 forbids one"
        );
    }
    // And an unavailable prerequisite maps to `Error`, the worst-ranked outcome, so it can
    // never be out-ranked into a pass by a channel that happened to agree.
    assert!(Outcome::Error.severity() > Outcome::Agree.severity());
    assert!(Outcome::Error.severity() > Outcome::AllowedDifference.severity());
    assert!(Outcome::Error.severity() > Outcome::NoReferenceForPlatform.severity());
}

/// A real error-path differential run WITHOUT the pinned oracle fails loud.
///
/// This is the prerequisite failure at its most dangerous, because the run *can* proceed:
/// deacon alone produces evidence, and a comparison against nothing has an obvious wrong
/// answer ("no differences found"). The runner refuses before executing anything, so the
/// case cannot report agreement it never established. Driven with the REAL registry's own
/// error-path cases so it cannot pass against a synthetic case shape that no longer ships.
#[tokio::test]
async fn an_error_path_differential_without_an_oracle_fails_loud() {
    use parity_harness::model::OracleType;

    let registry = real_registry();
    let differentials: Vec<&TestCase> = error_path_cases(&registry)
        .into_iter()
        .filter(|c| c.oracle_type == Some(OracleType::LiveDifferential))
        .filter(|c| matches!(c.classify(), Ok(CaseKind::Declarative)))
        .collect();
    assert!(
        !differentials.is_empty(),
        "the error-path tier must contain at least one live differential, or FR-044's \
         missing-oracle rule has nothing to bite on"
    );

    let root = workspace_root();
    let report_root = tempfile::tempdir().expect("tempdir");
    for case in differentials {
        let cfg = RunConfig {
            deacon_path: &PathBuf::from("/deacon-conformance/no-such-deacon"),
            oracle: None,
            fixtures_root: &root.join("conformance").join("fixtures"),
            report_root: report_root.path(),
            snapshots_root: &root.join("conformance").join("snapshots"),
        };
        match run_case(case, &cfg).await {
            Err(HarnessError::OracleMissing { .. }) => {}
            Ok(verdict) => panic!(
                "{}: produced a verdict ({}) with no oracle to compare against — a \
                 differential against nothing cannot find a difference",
                case.id,
                verdict.overall.as_str()
            ),
            Err(other) => panic!(
                "{}: expected a cause-specific OracleMissing before anything ran, got {other:?}",
                case.id
            ),
        }
    }
}

/// Write an executable `/bin/sh` stub with `body` and return its path.
#[cfg(unix)]
fn stub(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join(name);
    std::fs::write(&p, format!("#!/bin/sh\n{body}")).expect("write stub");
    let mut perms = std::fs::metadata(&p).expect("stat").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&p, perms).expect("chmod");
    p
}
