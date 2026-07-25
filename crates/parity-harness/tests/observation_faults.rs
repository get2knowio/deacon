//! Observation-fault guards for the live comparison path (024 Phase 3, D-2/D-3, FR-047).
//!
//! Two silent-pass defects the declarative runner must not have:
//!
//! - **D-2 (vacuity).** A differential where BOTH sides observed nothing agrees with
//!   itself. Every Docker observer returns `not_captured` when there is no container
//!   inspect, and the container lookup used to swallow a `docker ps` fault into an empty
//!   vec — so a daemon hiccup produced a green pass. A case that observed nothing has
//!   proven nothing: it must fail loud (constitution IV, no silent fallbacks).
//! - **D-3 (side sequencing).** deacon's container/network/volumes must be released
//!   BEFORE the reference side runs, or any case publishing a fixed host port collides.
//!
//! Plus the channel/observer lockstep: a case may only declare a channel the harness can
//! actually observe (V16 mirror, same pattern as `normalization_rules.rs`).
//!
//! Hermetic: pure functions, synthetic cases, and (Unix) POSIX-shell stub binaries. No
//! Docker, no network, no oracle.

use deacon_conformance::model::{
    CHAN_CONTAINER_STATE, CHAN_IMAGE, CHAN_TEMPORAL, ExpectedObservable, OBSERVED_CHANNELS,
    Operation, OracleType, ResourceGroup, TestCase,
};
use parity_harness::HarnessError;
use parity_harness::observe::observer_for;

// ---------------------------------------------------------------------------------
// Channel ↔ observer lockstep (V16 mirror).
// ---------------------------------------------------------------------------------

#[test]
fn every_observed_channel_has_an_observer() {
    for channel in OBSERVED_CHANNELS {
        assert!(
            observer_for(channel).is_some(),
            "`OBSERVED_CHANNELS` claims {channel:?} is observable, but `observer_for` has no \
             observer for it — validation would then admit a case that fails at runtime"
        );
    }
}

#[test]
fn a_channel_with_no_observer_is_not_declared_observable() {
    // `chan-container-state` is a legacy channel with no declarative observer (its
    // observer lands in 024 Phase 4). It must NOT be in the observable set, or a case
    // naming it would validate and then fail at runtime — exactly the gap V16 closes.
    assert!(
        observer_for(CHAN_CONTAINER_STATE).is_none(),
        "chan-container-state has no observer yet"
    );
    assert!(
        !OBSERVED_CHANNELS.contains(&CHAN_CONTAINER_STATE),
        "a channel with no observer must not be listed as observable"
    );
}

/// The lockstep in BOTH directions over the registry's whole channel universe: a channel
/// declared in `channels.json` is in `OBSERVED_CHANNELS` **iff** `observer_for` resolves
/// it, and every listed id is a real registry channel. A new channel added to the registry
/// therefore forces an explicit observable/not-observable decision here.
#[test]
fn observed_channels_is_exactly_the_set_with_an_observer() {
    let registry =
        deacon_conformance::load::Registry::load(&deacon_conformance::default_registry_dir())
            .expect("the committed conformance registry loads");
    let declared: Vec<&str> = registry.channels.iter().map(|c| c.id.as_str()).collect();
    for id in &declared {
        assert_eq!(
            observer_for(id).is_some(),
            OBSERVED_CHANNELS.contains(id),
            "channel {id:?}: `observer_for` and `OBSERVED_CHANNELS` disagree — either wire \
             the observer or drop the id from the observable set (V16 is computed from it)"
        );
    }
    for id in OBSERVED_CHANNELS {
        assert!(
            declared.contains(id),
            "`OBSERVED_CHANNELS` names {id:?}, which is not a channel in channels.json"
        );
    }
}

// ---------------------------------------------------------------------------------
// D-2: the container lookup is fallible — a `docker ps` fault is never an empty vec.
// ---------------------------------------------------------------------------------

#[test]
fn a_docker_ps_spawn_failure_is_an_error_not_an_empty_result() {
    let err = parity_harness::runner::containers_for_workspace_with(
        "deacon-no-such-docker-binary-xyz",
        std::path::Path::new("/tmp/ws"),
    )
    .expect_err("an unspawnable docker CLI must fail loud, never yield `no containers`");
    assert!(
        matches!(err, HarnessError::DockerUnavailable { .. }),
        "expected a cause-specific DockerUnavailable, got {err:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_nonzero_docker_ps_is_an_error_not_an_empty_result() {
    let dir = tempfile::tempdir().expect("tempdir");
    let failing = unix_stub::write(
        dir.path(),
        "docker-failing",
        "printf 'boom\\n' >&2\nexit 3\n",
    );
    let err = parity_harness::runner::containers_for_workspace_with(
        &failing.to_string_lossy(),
        std::path::Path::new("/tmp/ws"),
    )
    .expect_err("a non-zero `docker ps` must fail loud, never yield `no containers`");
    let msg = err.to_string();
    assert!(
        msg.contains("docker ps") && msg.contains("boom"),
        "the error must name the failing probe and carry its stderr: {msg}"
    );
}

#[cfg(unix)]
#[test]
fn a_successful_docker_ps_yields_sorted_deduped_ids() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ok = unix_stub::write(
        dir.path(),
        "docker-ok",
        "printf 'b\\na\\nb\\n\\n'\nexit 0\n",
    );
    let ids = parity_harness::runner::containers_for_workspace_with(
        &ok.to_string_lossy(),
        std::path::Path::new("/tmp/ws"),
    )
    .expect("a successful probe yields the ids");
    assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn a_successful_docker_op_that_discovered_no_container_is_an_observation_fault() {
    // A successful `up` that produced no discoverable container means the OBSERVATION is
    // broken, not that there is nothing to see.
    let err = parity_harness::runner::require_observed_container(
        "case-x",
        "op-up",
        &[],
        std::path::Path::new("/tmp/ws"),
    )
    .expect_err("a successful docker op with no container must fail loud");
    assert!(
        matches!(err, HarnessError::ObservationFault { .. }),
        "expected an ObservationFault, got {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("case-x") && msg.contains("op-up"),
        "the fault must name the case and the operation: {msg}"
    );
    assert!(
        parity_harness::runner::require_observed_container(
            "case-x",
            "op-up",
            &["c1".to_string()],
            std::path::Path::new("/tmp/ws"),
        )
        .is_ok(),
        "a discovered container is not a fault"
    );
}

// ---------------------------------------------------------------------------------
// D-2 + D-3 over the real runner, driven by stub binaries (Unix: POSIX-shell stubs).
// ---------------------------------------------------------------------------------

#[cfg(unix)]
mod unix_stub {
    use std::path::{Path, PathBuf};

    /// Write an executable `/bin/sh` stub with `body` and return its path.
    pub fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}")).expect("write stub");
        let mut perms = std::fs::metadata(&p).expect("stat").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&p, perms).expect("chmod");
        p
    }

    /// A stub that appends `<tag> <argv…>` to `log` and exits 0.
    pub fn logging(dir: &Path, name: &str, tag: &str, log: &Path) -> PathBuf {
        write(
            dir,
            name,
            &format!(
                "printf '{tag} %s\\n' \"$*\" >> '{}'\nexit 0\n",
                log.to_string_lossy()
            ),
        )
    }
}

#[cfg(unix)]
mod runner_faults {
    use super::*;

    use parity_harness::oracle::{OracleSource, VerifiedOracle};
    use parity_harness::runner::{RunConfig, run_case};

    /// A live-differential case whose only declared channels are Docker channels that
    /// cannot be observed (no container) — the vacuity shape of D-2.
    fn unobservable_case() -> TestCase {
        TestCase {
            id: "case-unobservable".to_string(),
            oracle_type: Some(OracleType::LiveDifferential),
            operations: vec![Operation {
                id: "op-read".to_string(),
                subcommand: "read-configuration".to_string(),
                argv: vec!["--workspace-folder".to_string(), "${WORKSPACE}".to_string()],
                fixtures: vec!["fx-x".to_string()],
                ..Operation::default()
            }],
            expected: vec![
                ExpectedObservable {
                    channel: CHAN_IMAGE.to_string(),
                    operation: Some("op-read".to_string()),
                    assertion: None,
                },
                ExpectedObservable {
                    channel: CHAN_TEMPORAL.to_string(),
                    operation: Some("op-read".to_string()),
                    assertion: None,
                },
            ],
            ..TestCase::default()
        }
    }

    fn oracle_at(path: &std::path::Path) -> VerifiedOracle {
        VerifiedOracle {
            path: path.to_path_buf(),
            source: OracleSource::Override,
            version: "0.0.0-stub".to_string(),
        }
    }

    #[tokio::test]
    async fn a_differential_that_observed_nothing_fails_loud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let stub = unix_stub::write(dir.path(), "stub-cli", "exit 0\n");
        let fixtures = dir.path().join("fixtures");
        std::fs::create_dir_all(fixtures.join("fx-x")).expect("mkdir fixture");
        let oracle = oracle_at(&stub);
        let cfg = RunConfig {
            deacon_path: &stub,
            oracle: Some(&oracle),
            fixtures_root: &fixtures,
            report_root: &dir.path().join("report"),
            snapshots_root: &dir.path().join("snapshots"),
        };

        let err = run_case(&unobservable_case(), &cfg)
            .await
            .expect_err("both sides observed nothing: that is a fault, not agreement");
        assert!(
            matches!(err, HarnessError::ObservationFault { .. }),
            "expected an ObservationFault, got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("case-unobservable")
                && msg.contains(CHAN_IMAGE)
                && msg.contains(CHAN_TEMPORAL),
            "the fault must name the case and every unobserved channel: {msg}"
        );
    }

    #[tokio::test]
    async fn a_spec_expectation_that_observed_nothing_fails_loud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let stub = unix_stub::write(dir.path(), "stub-cli", "exit 0\n");
        let fixtures = dir.path().join("fixtures");
        std::fs::create_dir_all(fixtures.join("fx-x")).expect("mkdir fixture");
        let cfg = RunConfig {
            deacon_path: &stub,
            oracle: None,
            fixtures_root: &fixtures,
            report_root: &dir.path().join("report"),
            snapshots_root: &dir.path().join("snapshots"),
        };
        let mut case = unobservable_case();
        case.id = "case-unobservable-spec".to_string();
        case.oracle_type = Some(OracleType::SpecExpectation);
        for exp in &mut case.expected {
            exp.assertion = Some(serde_json::json!({ "jsonSubset": {} }));
        }

        let err = run_case(&case, &cfg)
            .await
            .expect_err("a spec-expectation case that observed nothing must fail loud");
        assert!(
            matches!(err, HarnessError::ObservationFault { .. }),
            "expected an ObservationFault, got {err:?}"
        );
    }

    /// One observed channel is enough: not-captured-matches-not-captured stays a legal
    /// PER-CHANNEL verdict (FR-018). Only the all-channels-empty case is rejected.
    #[tokio::test]
    async fn one_observed_channel_keeps_the_per_channel_not_captured_semantics() {
        let dir = tempfile::tempdir().expect("tempdir");
        let stub = unix_stub::write(dir.path(), "stub-cli", "exit 0\n");
        let fixtures = dir.path().join("fixtures");
        std::fs::create_dir_all(fixtures.join("fx-x")).expect("mkdir fixture");
        let oracle = oracle_at(&stub);
        let cfg = RunConfig {
            deacon_path: &stub,
            oracle: Some(&oracle),
            fixtures_root: &fixtures,
            report_root: &dir.path().join("report"),
            snapshots_root: &dir.path().join("snapshots"),
        };
        let mut case = unobservable_case();
        case.id = "case-partly-observable".to_string();
        case.expected.push(ExpectedObservable {
            channel: deacon_conformance::model::CHAN_EXIT_CODE.to_string(),
            operation: Some("op-read".to_string()),
            assertion: None,
        });

        let verdict = run_case(&case, &cfg)
            .await
            .expect("one observed channel makes the case verifiable");
        assert_eq!(
            verdict.overall,
            parity_harness::evidence::Outcome::Agree,
            "the observed channel agrees and the unobservable ones stay not-captured on \
             BOTH sides (FR-018): {verdict:?}"
        );
    }

    /// D-3: deacon's resources are released BEFORE the reference side runs. The stub
    /// `deacon` logs every invocation — including the cleanup `down` the workspace guard
    /// issues — so the log's ORDER is the observable.
    #[tokio::test]
    async fn deacon_side_is_released_before_the_oracle_side_runs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log = dir.path().join("invocations.log");
        let deacon = unix_stub::logging(dir.path(), "stub-deacon", "DEACON", &log);
        let oracle_bin = unix_stub::logging(dir.path(), "stub-oracle", "ORACLE", &log);
        let fixtures = dir.path().join("fixtures");
        std::fs::create_dir_all(&fixtures).expect("mkdir fixtures");
        let oracle = oracle_at(&oracle_bin);
        let cfg = RunConfig {
            deacon_path: &deacon,
            oracle: Some(&oracle),
            fixtures_root: &fixtures,
            report_root: &dir.path().join("report"),
            snapshots_root: &dir.path().join("snapshots"),
        };
        // A Docker-backed case (isolated workspace + RAII guard), whose op is config-only
        // so the case needs no daemon: the sequencing under test is the guard's, not
        // Docker's.
        let case = TestCase {
            id: "case-sequencing".to_string(),
            oracle_type: Some(OracleType::LiveDifferential),
            resource_group: Some(ResourceGroup::DockerShared),
            operations: vec![Operation {
                id: "op-read".to_string(),
                subcommand: "read-configuration".to_string(),
                argv: vec!["--workspace-folder".to_string(), "${WORKSPACE}".to_string()],
                ..Operation::default()
            }],
            expected: vec![ExpectedObservable {
                channel: deacon_conformance::model::CHAN_EXIT_CODE.to_string(),
                operation: Some("op-read".to_string()),
                assertion: None,
            }],
            ..TestCase::default()
        };

        let verdict = run_case(&case, &cfg).await.expect("run");
        assert_eq!(
            verdict.overall,
            parity_harness::evidence::Outcome::Agree,
            "both stubs exit 0: {verdict:?}"
        );

        let text = std::fs::read_to_string(&log).expect("the stubs logged their invocations");
        let lines: Vec<&str> = text.lines().collect();
        let first_oracle = lines
            .iter()
            .position(|l| l.starts_with("ORACLE "))
            .unwrap_or_else(|| panic!("the oracle side must have run:\n{text}"));
        let first_down = lines
            .iter()
            .position(|l| l.starts_with("DEACON down"))
            .unwrap_or_else(|| panic!("deacon's workspace must be reclaimed:\n{text}"));
        assert!(
            first_down < first_oracle,
            "deacon's side must be RELEASED before the oracle side runs (D-3) — otherwise a \
             fixed-host-port case collides. Log:\n{text}"
        );
    }
}
