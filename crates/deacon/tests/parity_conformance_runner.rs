//! Live differential run of the declarative conformance runner over the registry's
//! CONFIG-ONLY declarative cases (022-conformance-runner US1 T026; reshaped by
//! 024-deterministic-conformance-coverage T015, research Decision 4).
//!
//! Runs ONLY under `cargo nextest run --profile parity`. There is no opt-in env gate and
//! no silent skip: a missing/mismatched oracle, a missing fixture, a CLI failure, or a
//! normalization failure FAILS the test with a cause-specific message (constitution IV).
//! It drives the SHARED runner over declarative cases — spec-expectation cases against
//! deacon, live-differential cases against deacon + the pinned oracle — so adding a case is
//! a pure data edit (SC-001). The deterministic verdict report is emitted on stdout; a
//! run-report fragment is written under `target/parity/report/parity_conformance_runner/`
//! for the aggregator.
//!
//! **One driver function per `resourceGroup`.** This binary owns the groups that need no
//! container runtime — `none` and `fs-heavy` (which is significant *filesystem* work, not
//! Docker). The Docker-backed groups live in `parity_conformance_docker`, so this binary's
//! registry entry can truthfully say `docker_required: false` (024 T020).
//!
//! Adding a case with an EXISTING resource group stays a pure data edit (SC-013): all four
//! `ResourceGroup` variants already have a driver, split across the two binaries.

use std::path::PathBuf;
use std::sync::Arc;

use deacon_conformance::default_registry_dir;
use deacon_conformance::load::Registry;
use deacon_conformance::model::ResourceGroup;

use parity_harness::driver::{self, DriverConfig};
use parity_harness::oracle::Oracle;
use parity_harness::{HarnessError, report_root, workspace_root};

/// This binary's name — the fragment key and the registry entry.
const BINARY: &str = "parity_conformance_runner";

/// Fail the test with the error's cause-specific `Display` message (never `Debug`) so an
/// oracle/prereq/normalization failure reads as its remedy.
fn ff<T>(r: Result<T, HarnessError>) -> T {
    r.unwrap_or_else(|e| panic!("{e}"))
}

/// Drive one config-only resource group end to end.
///
/// A group with no cases is NOT a silent skip: it still writes its fragment (which is what
/// proves the binary ran) and says so on stderr. The registry as a whole is still asserted
/// non-empty, so a loader that stopped finding cases fails loudly instead of passing by
/// driving nothing.
async fn drive(group: ResourceGroup) {
    assert!(
        !driver::needs_docker(group),
        "`{BINARY}` drives the config-only groups; `{}` is Docker-backed and belongs to \
         parity_conformance_docker",
        driver::group_slug(group)
    );

    // Fail fast if the pinned oracle is absent/mismatched — never skip to pass. Every
    // declarative case may need it (live-differential does; spec-expectation ignores it).
    let oracle = ff(Oracle::acquire().await);
    let root = workspace_root();

    let registry = Registry::load(&default_registry_dir())
        .unwrap_or_else(|e| panic!("conformance registry must load: {e}"));
    let cases = driver::cases_in_group(&registry.cases, group);
    assert!(
        !registry.cases.is_empty(),
        "expected the conformance registry to hold cases to drive"
    );
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
    assert!(
        run.failures.is_empty(),
        "declarative conformance divergence(s) in resource group `{}`:\n{}",
        driver::group_slug(group),
        run.failures.join("\n"),
    );
}

/// Resource group `none` — the config-only cases (`read-configuration`, `doctor`) that
/// need no special resource. This is the bulk of the declarative set.
#[tokio::test]
async fn conformance_group_none() {
    drive(ResourceGroup::None).await;
}

/// Resource group `fs-heavy` — significant filesystem work, no Docker.
///
/// It lives here, not in the Docker binary, because the group means filesystem pressure
/// rather than a daemon. Its own driver function is what makes nextest able to place it in
/// the `fs-heavy` test group independently of the `none` cases (FR-077).
#[tokio::test]
async fn conformance_group_fs_heavy() {
    drive(ResourceGroup::FsHeavy).await;
}
