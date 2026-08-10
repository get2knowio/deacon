//! The REGISTRY parity lane: `spec-expectation` cases in the config-only resource groups
//! that REACH AN OCI REGISTRY. No Docker daemon, no pinned oracle — but network, and
//! `ghcr.io` specifically.
//!
//! **Why it exists** (#544). `driver::lane_of` used to derive a lane from `oracleType` and
//! `resourceGroup` only, an axis that encodes the reference and the daemon. "Needs a
//! registry" was not on it, so six cases that fetch `ghcr.io/devcontainers/features/git`
//! at case time sat in `parity_hermetic` — the lane whose docstring promises "no network"
//! and which gates every pull request on `Test (MVP fast)`, a job with no Docker setup, no
//! prepull and no GHCR token. They passed only because CI happened to have network. Each
//! was a standing dependency waiting to present as a transient, with a failure signature
//! byte-identical to the one that produced #454.
//!
//! **Why the reach is not removable.** These are not incidental registry touches a fixture
//! edit could swap for a local Feature:
//!
//! - `read-configuration --include-merged-configuration` resolves the effective Feature
//!   set to fold each Feature's metadata into the merged document, and the case data
//!   asserts VERSION-PINNED OCI keys. For `bhv-extends-feature-version-override` (#411)
//!   the pin *is* the behavior — `git:1.3.2` overridden to `git:1.3.8` across an extends
//!   chain — and a local Feature has no version in its id, so the claim cannot be
//!   expressed hermetically at all.
//! - `upgrade --dry-run` resolves Feature digests against the registry in order to
//!   regenerate the lockfile. That *is* the subcommand.
//!
//! So the cases moved lanes and NOTHING about what they claim changed. Re-laning is a
//! laning change, not a coverage change.
//!
//! **Where it runs, and why the six still gate every pull request.** `Test (MVP
//! integration)` is a required PR check and already acquires a read-only GHCR bearer token
//! (`.github/workflows/ci.yml`), so the `mvp-integration` profile selects this binary and
//! `dev-fast` excludes it. The release gate runs it too, alongside its two sibling
//! no-oracle lanes.
//!
//! **No skips.** Absent network the cases FAIL, loudly, naming the registry they could not
//! reach — the same discipline as every other lane (constitution IV). A lane that skipped
//! when the registry was unreachable would be the original defect wearing a different hat.
//!
//! Its siblings: `parity_hermetic` (nothing at all), `parity_docker` (Docker-backed, still
//! no oracle) and `parity_differential` (live against the pinned reference).

use std::path::PathBuf;
use std::sync::Arc;

use parity_harness::driver::{self, DriverConfig, Lane};
use parity_harness::load::Registry;
use parity_harness::model::ResourceGroup;
use parity_harness::{HarnessError, parity_root, report_root, workspace_root};

/// This binary's name — the report-fragment key.
const BINARY: &str = "parity_registry";

/// Fail with the error's cause-specific `Display` message (never `Debug`) so a
/// prereq/normalization failure reads as its remedy.
fn ff<T>(r: Result<T, HarnessError>) -> T {
    r.unwrap_or_else(|e| panic!("{e}"))
}

/// Drive one config-only resource group's registry-reaching cases end to end.
///
/// A group with no cases is NOT a silent skip: it still writes its fragment (which is what
/// proves the binary ran) and says so on stderr. The case set as a whole is still asserted
/// non-empty, so a loader that stopped finding cases fails loudly instead of passing by
/// driving nothing.
async fn drive(group: ResourceGroup) {
    assert!(
        !driver::needs_docker(group),
        "`{BINARY}` needs a registry, not a daemon; `{}` is Docker-backed and belongs to \
         parity_docker",
        driver::group_slug(group)
    );

    let registry = Registry::load(&parity_root())
        .unwrap_or_else(|e| panic!("the parity case set must load: {e}"));
    assert!(
        !registry.cases.is_empty(),
        "expected the parity case set to hold cases to drive"
    );
    let cases = driver::cases_in_lane_and_group(&registry.cases, Lane::Registry, group);
    if cases.is_empty() {
        eprintln!(
            "note: no registry case declares resourceGroup `{}` yet",
            driver::group_slug(group)
        );
    }

    let cfg = Arc::new(DriverConfig {
        binary: BINARY.to_string(),
        deacon_path: PathBuf::from(env!("CARGO_BIN_EXE_deacon")),
        // No oracle: like its hermetic sibling, every case here compares deacon against an
        // assertion declared in its own record. The registry is a prerequisite of the
        // SUBJECT, not of the comparison.
        oracle: None,
        fixtures_root: workspace_root().join("parity").join("fixtures"),
        report_root: report_root(),
    });

    let run = ff(driver::drive_group(Arc::clone(&cfg), cases, group).await);
    ff(driver::emit(&run));
    assert!(
        run.failures.is_empty(),
        "registry parity failure(s) in resource group `{}`:\n{}",
        driver::group_slug(group),
        run.failures.join("\n"),
    );
}

/// Resource group `none` — the config-only cases (`read-configuration`, `upgrade`) that
/// need no special local resource, only the registry. This is the whole registry set
/// today.
#[tokio::test]
async fn registry_group_none() {
    drive(ResourceGroup::None).await;
}

/// Resource group `fs-heavy` — significant filesystem work AND a registry reach, still no
/// Docker. Empty today; it exists so that a case declaring both stays a pure data edit,
/// and so `fs-heavy`'s nextest group can throttle it independently of the `none` cases
/// (FR-077).
#[tokio::test]
async fn registry_group_fs_heavy() {
    drive(ResourceGroup::FsHeavy).await;
}
