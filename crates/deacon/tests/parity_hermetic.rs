//! The HERMETIC parity lane: `spec-expectation` cases in the config-only resource
//! groups. No Docker daemon, no pinned oracle, no network — and since #544 the last of
//! those is ENFORCED by [`hermetic_lane_runs_without_a_network`], not merely promised
//! here.
//!
//! These cases compare deacon against an assertion DECLARED in the record, so nothing
//! external has to exist for them to mean something. That is what makes them the one
//! parity lane that can gate every pull request, in every profile.
//!
//! **A registry is a prerequisite like any other.** `driver::lane_of` used to derive a
//! lane from `oracleType` and `resourceGroup` alone — an axis encoding the reference and
//! the daemon, on which "reaches `ghcr.io`" could not be said. Six cases that resolve OCI
//! Features at case time therefore sat here, in the lane whose promise is no network,
//! passing only because CI had some (#544). A case declares `needsRegistry` and lands in
//! `parity_registry` instead; nothing about what such a case ASSERTS changed in the move,
//! and nothing about it should — re-laning is a laning change, not a coverage change.
//!
//! **Every platform CI runs, and the case data is what earned that** (#441). The lane was
//! gated to Linux because its records pinned LINUX-MEASURED output rather than deacon's
//! behavior: `case-doctor-structured-*` asserted `host_os.name: "linux"` — the RUNNER's
//! operating system, which deacon does not choose — and a `templates apply` case pinned a
//! POSIX file mode, an observable that does not exist off Unix. Both are now claims deacon
//! can actually satisfy anywhere (a present `host_os` OBJECT; presence and bytes, not
//! permissions). The remaining two were never case-data problems at all: git's autocrlf
//! rewrote the `templates apply` fixture bytes on a Windows checkout, fixed by pinning
//! `parity/fixtures/** -text` in `.gitattributes`, and `TokenMap::workspace` registered
//! ONE spelling of the workspace path while deacon reports the canonicalized one
//! (`\\?\` + 8.3 expansion on Windows, `/private/var` on macOS), fixed in the normalizer.
//!
//! Nothing here is platform-conditional: there is no per-platform expected value, no
//! `cfg`-gated case and no skip. That was the constraint — making the DATA name the host
//! would have rebuilt the case machinery this suite deleted, and keeping the gate would
//! have been better than that.
//!
//! **No skips.** A missing fixture, a CLI failure or a normalization failure FAILS with a
//! cause-specific message (constitution IV). The lane never has to *skip* a case it cannot
//! run, because a case it cannot run is by definition in another lane —
//! [`driver::lane_of`] decides that from what the case needs, and the selection is visible
//! rather than discovered at run time. A skip and a pass look identical in a report; a
//! non-selection is written down.
//!
//! Its siblings: `parity_registry` (a registry, still no daemon and no oracle),
//! `parity_docker` (Docker-backed, still no oracle) and
//! `parity_differential` (live against the pinned reference).

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

/// The lane's "no network" promise, ENFORCED (#544).
///
/// Re-runs this binary's own driver tests inside a fresh user + network namespace, so a
/// case that reaches out cannot resolve anything. Before this existed the promise lived in
/// the docstring above and nowhere else, and six registry-reaching cases drifted into the
/// lane and stayed — passing on every CI run that happened to have network, and waiting to
/// present as a transient with the signature of #454. A property asserted only in prose is
/// a property nobody is checking.
///
/// **Linux-only, and deliberately so.** `unshare(2)` and unprivileged user namespaces are
/// a Linux facility; this lane runs on macOS and Windows too since #441, where the guard
/// is NOT SELECTED rather than skipped — the `#[cfg]` is visible in the source, whereas a
/// runtime skip would report as a pass. The lane's hermeticity is a property of the CASE
/// DATA, which is identical on every platform, so checking it on one platform checks it
/// everywhere; what would not be acceptable is a platform silently believing it had been
/// checked.
///
/// **Never a silent skip at run time either.** If the host cannot create a user+network
/// namespace the guard FAILS with that cause named, exactly as every other prerequisite
/// failure in this suite does (constitution IV). "Could not verify" is not "verified".
#[cfg(target_os = "linux")]
#[test]
fn hermetic_lane_runs_without_a_network() {
    use std::process::Command;

    /// This test's own name — the child selects everything BUT this, which is what keeps
    /// the re-exec from recursing. Kept next to the `--skip` that consumes it.
    const SELF: &str = "hermetic_lane_runs_without_a_network";

    // Probe the facility separately from using it, so "this host has no unprivileged user
    // namespaces" cannot be misread as "a hermetic case reached the network". The two
    // failures have completely different remedies and must not share a message.
    let probe = Command::new("unshare")
        .args(["--map-root-user", "--net", "--", "true"])
        .output();
    match probe {
        Err(e) => panic!(
            "the hermetic guard needs `unshare` (util-linux) to remove the network \
             namespace, and it could not be run: {e}. This is a MACHINERY failure, not a \
             parity divergence — install util-linux, or run this lane on a host that can \
             create namespaces. It is deliberately not a skip: a skip and a pass are \
             indistinguishable in a report."
        ),
        Ok(out) if !out.status.success() => panic!(
            "the hermetic guard could not create a user+network namespace \
             (`unshare --map-root-user --net` exited {}): {}\nThis is a MACHINERY failure, \
             not a parity divergence — unprivileged user namespaces must be enabled \
             (`sysctl kernel.unprivileged_userns_clone=1` on some distributions, \
             `/proc/sys/user/max_user_namespaces` non-zero). It is deliberately not a \
             skip.",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim(),
        ),
        Ok(_) => {}
    }

    // The child writes its own report fragments. Sharing `target/parity` with the outer
    // run would have two processes writing one path concurrently — nextest may schedule
    // this test alongside the drivers it re-runs.
    let evidence = tempfile::tempdir().expect("create a temp report root for the guard run");

    let self_exe = std::env::current_exe().expect("locate this test binary to re-exec it");
    let output = Command::new("unshare")
        .args(["--map-root-user", "--net", "--"])
        .arg(&self_exe)
        // Everything in this binary EXCEPT this test: a selection, not a skip. Running the
        // drivers themselves is the point — the guard must exercise the same case set the
        // lane does, or it would only be checking a copy of it.
        .args(["--skip", SELF, "--test-threads=1"])
        .env("DEACON_PARITY_REPORT_DIR", evidence.path())
        .output()
        .expect("re-exec this test binary inside the namespace");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "a `{BINARY}` case REACHED THE NETWORK. This lane promises none, gates every pull \
         request on a job with no registry token, and every case here compares deacon \
         against an assertion declared in its own record — so a case that needs `ghcr.io` \
         does not belong in it. Declare `\"needsRegistry\": true` on the case and it moves \
         to `parity_registry`, which runs on the jobs provisioned for a registry; do NOT \
         weaken what the case asserts to fit this lane.\n\
         --- namespaced re-run stdout ---\n{stdout}\n\
         --- namespaced re-run stderr ---\n{stderr}",
    );

    // A guard that selected nothing would pass forever. Assert it actually drove the
    // drivers rather than trusting an exit code that an empty selection also produces.
    assert!(
        stdout.contains("2 passed"),
        "the namespaced re-run reported no driver tests, so this guard checked nothing — \
         the `--skip {SELF}` selection is wrong or the lane's driver functions were \
         renamed.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
    );
}
