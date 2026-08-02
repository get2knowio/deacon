//! The DOCKER parity lane: Docker-backed cases whose expectation is PINNED in the record
//! (`spec-expectation` and `invariant-metamorphic`). Needs a daemon; needs no oracle.
//!
//! Its sibling `parity_hermetic` runs the pinned cases that need no daemon either;
//! `parity_differential` runs everything that must invoke the reference CLI. The split is
//! by what a case NEEDS, not by what it is about, which is why this lane can gate a pull
//! request on a runner that has Docker but not `@devcontainers/cli`.
//!
//! An `invariant-metamorphic` case belongs here rather than in the differential lane on
//! the same rule: it asserts a RELATIONSHIP across two or more of its own operations —
//! idempotence, first-create-versus-restart — so its answer comes from deacon alone.
//!
//! No opt-in env gate and no silent skip: absent Docker, a missing fixture, a CLI failure
//! or a normalization failure FAILS with a cause-specific message (constitution IV).
//!
//! **Two bounds, doing different jobs.** Each case is bounded at five minutes inside the
//! shared runner (FR-077b), so a wedged case is named and the rest of the group still
//! runs. The tier as a whole is bounded at thirty minutes (FR-077a) and asserted HERE,
//! explicitly, from its own measurements — not delegated to nextest's `slow-timeout`,
//! which would report only "the binary was slow" and name nothing to fix.

use std::path::PathBuf;
use std::sync::Arc;

use parity_harness::driver::{self, DriverConfig, Lane, TIER_BUDGET};
use parity_harness::load::Registry;
use parity_harness::model::ResourceGroup;
use parity_harness::prereq;
use parity_harness::{HarnessError, parity_root, report_root, workspace_root};

/// This binary's name — the fragment key and the tier-timing key.
const BINARY: &str = "parity_docker";

/// Fail with the error's cause-specific `Display` message (never `Debug`) so a
/// prereq/normalization failure reads as its remedy.
fn ff<T>(r: Result<T, HarnessError>) -> T {
    r.unwrap_or_else(|e| panic!("{e}"))
}

/// Drive one Docker-backed resource group's pinned cases, then assert the tier budget.
///
/// A group with no cases is NOT a silent skip: it still writes its fragment (which is what
/// proves the binary ran) and says so on stderr. Docker is still required and still
/// checked — this binary exists precisely because these cases need a daemon, and "no case
/// today" must not quietly become "no daemon needed".
async fn drive(group: ResourceGroup) {
    assert!(
        driver::needs_docker(group),
        "`{BINARY}` drives the Docker-backed groups; `{}` needs no daemon and belongs to \
         parity_hermetic",
        driver::group_slug(group)
    );

    ff(prereq::require_docker().await);

    let registry = Registry::load(&parity_root())
        .unwrap_or_else(|e| panic!("the parity case set must load: {e}"));
    let cases = driver::cases_in_lane_and_group(&registry.cases, Lane::Docker, group);
    if cases.is_empty() {
        eprintln!(
            "note: no pinned Docker case declares resourceGroup `{}` yet",
            driver::group_slug(group)
        );
    }

    let cfg = Arc::new(DriverConfig {
        binary: BINARY.to_string(),
        deacon_path: PathBuf::from(env!("CARGO_BIN_EXE_deacon")),
        // No oracle: every case in this lane carries its own expectation. A
        // `live-differential` case reaching this driver would fail loud rather than
        // silently compare against nothing — `oracle_type::live_differential` requires it.
        oracle: None,
        fixtures_root: workspace_root().join("parity").join("fixtures"),
        report_root: report_root(),
    });

    let run = ff(driver::drive_group(Arc::clone(&cfg), cases, group).await);
    ff(driver::emit(&run));

    // Record THIS group's wall clock before asserting, so the assertion below folds in a
    // complete picture of whatever has finished so far.
    ff(driver::record_timing(&cfg, &run).await);
    assert_tier_within_budget(&cfg);

    assert!(
        run.failures.is_empty(),
        "pinned Docker parity failure(s) in resource group `{}`:\n{}",
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
/// whole tier. So the tier can never pass by measuring less than happened, and the run
/// fails in at least one test when the budget is genuinely blown.
fn assert_tier_within_budget(cfg: &DriverConfig) {
    let summary = ff(driver::tier_summary(&cfg.report_root, &cfg.binary));
    eprintln!(
        "pinned Docker parity tier: {:?} elapsed across group(s) [{}], budget {:?}",
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
async fn docker_group_shared() {
    drive(ResourceGroup::DockerShared).await;
}

/// Resource group `docker-exclusive` — exclusive daemon access, driven serially.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn docker_group_exclusive() {
    drive(ResourceGroup::DockerExclusive).await;
}
