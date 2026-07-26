//! Live differential run of the declarative conformance runner over the registry's
//! DOCKER-BACKED declarative cases (024-deterministic-conformance-coverage T016/T018/T019,
//! research Decision 4 / Decision 10).
//!
//! The sibling of `parity_conformance_runner`: same shared runner, same data-driven cases,
//! same fail-loud contract — but the groups that touch the container runtime, and therefore
//! the tier that needs bounded concurrency and a wall-clock budget.
//!
//! Runs ONLY under `cargo nextest run --profile parity`. There is no opt-in env gate and no
//! silent skip: a missing/mismatched oracle, absent Docker, a missing fixture, or a
//! normalization failure FAILS the test with a cause-specific message (constitution IV).
//!
//! **One driver function per Docker `resourceGroup`** — `docker-shared` (safe concurrent
//! daemon use; four cases at a time) and `docker-exclusive` (serial). `fs-heavy` is NOT
//! here: per its model definition it is "significant filesystem operations, no Docker", so
//! it stays with the config-only binary. Adding a case with an existing group is a pure
//! data edit (SC-013).
//!
//! **Two bounds, doing different jobs.** Each case is bounded at five minutes inside the
//! shared runner (FR-077b), so a wedged case is named and the rest of the group still runs.
//! The tier as a whole is bounded at thirty minutes (FR-077a) and asserted HERE, explicitly,
//! from its own measurements — not delegated to nextest's `slow-timeout`, which would report
//! only "the binary was slow" and name nothing to fix.
//!
//! The error-path tier (US4) and the workflow/denormalized cases (US3/US5) land as DATA in
//! `conformance/registry/cases/<area>.json`; whatever their area, a Docker resource group
//! routes them here with no change to this file.

use std::path::PathBuf;
use std::sync::Arc;

use deacon_conformance::default_registry_dir;
use deacon_conformance::load::Registry;
use deacon_conformance::model::ResourceGroup;

use parity_harness::driver::{self, DriverConfig, TIER_BUDGET};
use parity_harness::oracle::Oracle;
use parity_harness::prereq;
use parity_harness::{HarnessError, report_root, workspace_root};

/// This binary's name — the fragment key, the registry entry, and the tier-timing key.
const BINARY: &str = "parity_conformance_docker";

/// Fail the test with the error's cause-specific `Display` message (never `Debug`) so an
/// oracle/prereq/normalization failure reads as its remedy.
fn ff<T>(r: Result<T, HarnessError>) -> T {
    r.unwrap_or_else(|e| panic!("{e}"))
}

/// Drive one Docker-backed resource group, then assert the tier budget.
///
/// A group with no cases is NOT a silent skip: it still writes its fragment (which is what
/// proves the binary ran) and says so on stderr. Docker is still required and still checked
/// — this binary exists precisely because these cases need a daemon, and "no case today"
/// must not quietly become "no daemon needed".
async fn drive(group: ResourceGroup) {
    assert!(
        driver::needs_docker(group),
        "`{BINARY}` drives the Docker-backed groups; `{}` needs no daemon and belongs to \
         parity_conformance_runner",
        driver::group_slug(group)
    );

    // Prerequisites first, both fail-loud: an absent daemon or a mismatched oracle must
    // fail this lane, never skip it to a green.
    ff(prereq::require_docker().await);
    let oracle = ff(Oracle::acquire().await);
    let root = workspace_root();

    let registry = Registry::load(&default_registry_dir())
        .unwrap_or_else(|e| panic!("conformance registry must load: {e}"));
    let cases = driver::cases_in_group(&registry.cases, group);
    if cases.is_empty() {
        eprintln!(
            "note: no declarative case declares resourceGroup `{}` yet",
            driver::group_slug(group)
        );
    }

    let cfg = Arc::new(DriverConfig {
        binary: BINARY.to_string(),
        deacon_path: PathBuf::from(env!("CARGO_BIN_EXE_deacon")),
        oracle: Some(oracle),
        fixtures_root: root.join("conformance").join("fixtures"),
        report_root: report_root(),
        snapshots_root: root.join("conformance").join("snapshots"),
    });

    let run = ff(driver::drive_group(Arc::clone(&cfg), cases, group).await);
    ff(driver::emit(&run));

    // Record THIS group's wall clock before asserting, so the assertion below folds in a
    // complete picture of whatever has finished so far.
    ff(driver::record_timing(&cfg, &run).await);
    assert_tier_within_budget(&cfg);

    assert!(
        run.failures.is_empty(),
        "declarative conformance divergence(s) in resource group `{}`:\n{}",
        driver::group_slug(group),
        run.failures.join("\n"),
    );
}

/// FR-077a / research Decision 10: the Docker tier's own measured wall clock must stay
/// within [`TIER_BUDGET`], and exceeding it fails with the NUMBER and the SLOWEST CASES.
///
/// The tier spans this binary's driver functions, which nextest runs as sibling processes.
/// Each records its group's span into a per-run artifact tree, and the fold below takes
/// `max(finished) - min(started)` across every artifact present — the true tier wall clock,
/// not the sum (which would over-count concurrent groups). A process that runs before its
/// siblings sees a partial fold, which can only UNDER-report; the last to finish sees the
/// whole tier. So the tier can never pass by measuring less than happened, and the run fails
/// in at least one test when the budget is genuinely blown.
fn assert_tier_within_budget(cfg: &DriverConfig) {
    let summary = ff(driver::tier_summary(&cfg.report_root, &cfg.binary));
    eprintln!(
        "docker conformance tier: {:?} elapsed across group(s) [{}], budget {:?}",
        summary.elapsed,
        summary.groups.join(", "),
        TIER_BUDGET
    );
    if let Some(violation) = driver::budget_violation(&summary, TIER_BUDGET) {
        panic!("{violation}");
    }
}

/// Resource group `docker-shared` — safe concurrent daemon use.
///
/// Every Docker case runs in its own isolated external temp workspace, so its
/// `devcontainer.local_folder` label (and therefore its container/network/volume names) is
/// unique; four cases run at a time without colliding.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn conformance_group_docker_shared() {
    drive(ResourceGroup::DockerShared).await;
}

/// Resource group `docker-exclusive` — exclusive daemon access, driven serially.
///
/// A case lands here when it manipulates state the daemon shares across containers, so its
/// driver runs one case at a time even though the tier as a whole is concurrent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conformance_group_docker_exclusive() {
    drive(ResourceGroup::DockerExclusive).await;
}
