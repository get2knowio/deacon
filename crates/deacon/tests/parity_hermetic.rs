//! The HERMETIC parity lane: `spec-expectation` cases in the config-only resource
//! groups. No Docker daemon, no pinned oracle, no network.
//!
//! These cases compare deacon against an assertion DECLARED in the record, so nothing
//! external has to exist for them to mean something. That is what makes them the one
//! parity lane that can gate every pull request, in every profile.
//!
//! **Linux only, and that is a selection rather than a skip.** The hermetic case data pins
//! LINUX-MEASURED output: `case-doctor-structured-*` assert `host_os.name: "linux"`, and the
//! `templates apply` / substitution cases pin path separators and line endings. Those are
//! assertions about the runner, so the lane runs only where its pins are valid — the same
//! reasoning that keeps `parity_differential` out of every profile that has no oracle. On
//! macOS and Windows the lane is truthfully NOT SELECTED rather than skipped or excused, and
//! deacon's own cross-platform coverage stays with the rest of `dev-fast`. Making the data
//! platform-conditional instead would rebuild the case machinery this suite deleted; the
//! four measured portability gaps are inventoried on #441.
//!
//! The inner `cfg` leaves an EMPTY test binary off Linux rather than an uncompiled one,
//! which is required: nextest compiles every test target before filtering, so this file
//! must still build on Windows.
//!
//! **No skips.** A missing fixture, a CLI failure or a normalization failure FAILS with a
//! cause-specific message (constitution IV). The lane never has to *skip* a case it cannot
//! run, because a case it cannot run is by definition in another lane —
//! [`driver::lane_of`] decides that from what the case needs, and the selection is visible
//! rather than discovered at run time. A skip and a pass look identical in a report; a
//! non-selection is written down.
//!
//! Its siblings: `parity_docker` (Docker-backed, still no oracle) and
//! `parity_differential` (live against the pinned reference).

#![cfg(target_os = "linux")]

use std::path::PathBuf;
use std::sync::Arc;

use parity_harness::driver::{self, DriverConfig, Lane};
use parity_harness::load::Registry;
use parity_harness::model::ResourceGroup;
use parity_harness::{HarnessError, parity_root, report_root, workspace_root};

/// This binary's name — the report-fragment key.
const BINARY: &str = "parity_hermetic";

/// Fail with the error's cause-specific `Display` message (never `Debug`) so a
/// prereq/normalization failure reads as its remedy.
fn ff<T>(r: Result<T, HarnessError>) -> T {
    r.unwrap_or_else(|e| panic!("{e}"))
}

/// Drive one config-only resource group's hermetic cases end to end.
///
/// A group with no cases is NOT a silent skip: it still writes its fragment (which is what
/// proves the binary ran) and says so on stderr. The case set as a whole is still asserted
/// non-empty, so a loader that stopped finding cases fails loudly instead of passing by
/// driving nothing.
async fn drive(group: ResourceGroup) {
    assert!(
        !driver::needs_docker(group),
        "`{BINARY}` is hermetic; `{}` is Docker-backed and belongs to parity_docker",
        driver::group_slug(group)
    );

    let registry = Registry::load(&parity_root())
        .unwrap_or_else(|e| panic!("the parity case set must load: {e}"));
    assert!(
        !registry.cases.is_empty(),
        "expected the parity case set to hold cases to drive"
    );
    let cases = driver::cases_in_lane_and_group(&registry.cases, Lane::Hermetic, group);
    if cases.is_empty() {
        eprintln!(
            "note: no hermetic case declares resourceGroup `{}` yet",
            driver::group_slug(group)
        );
    }

    let cfg = Arc::new(DriverConfig {
        binary: BINARY.to_string(),
        deacon_path: PathBuf::from(env!("CARGO_BIN_EXE_deacon")),
        // No oracle, and that is not a degraded mode: a hermetic case's expectation is in
        // its own record. `Oracle::acquire()` used to run unconditionally here, which is
        // precisely why this lane could not exist — it failed on every machine without
        // `@devcontainers/cli` installed, over cases that never invoke it.
        oracle: None,
        fixtures_root: workspace_root().join("parity").join("fixtures"),
        report_root: report_root(),
    });

    let run = ff(driver::drive_group(Arc::clone(&cfg), cases, group).await);
    ff(driver::emit(&run));
    assert!(
        run.failures.is_empty(),
        "hermetic parity failure(s) in resource group `{}`:\n{}",
        driver::group_slug(group),
        run.failures.join("\n"),
    );
}

/// Resource group `none` — the config-only cases (`read-configuration`, `doctor`) that
/// need no special resource. This is the bulk of the hermetic set.
#[tokio::test]
async fn hermetic_group_none() {
    drive(ResourceGroup::None).await;
}

/// Resource group `fs-heavy` — significant filesystem work, still no Docker. Its own
/// driver function is what lets nextest place it in the `fs-heavy` test group
/// independently of the `none` cases (FR-077).
#[tokio::test]
async fn hermetic_group_fs_heavy() {
    drive(ResourceGroup::FsHeavy).await;
}
