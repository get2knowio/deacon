//! Parity: normalized observable-state parity between deacon and the pinned
//! `@devcontainers/cli` oracle.
//!
//! Each test brings a fixture UP with BOTH CLIs (or, for the intra-deacon case,
//! deacon twice) and diffs the resulting container's NORMALIZED observable state
//! (`docker inspect`: mounts, env, labels, user, working dir, ports) field by
//! field via `parity_harness::normalize::{container_state, diff_states}`. Any
//! divergence that is not caller-allowed fails the test with a per-field report.
//!
//! Runs ONLY under `cargo nextest run --profile parity`. There is no opt-in env
//! gate and no silent skip: a missing/mismatched oracle or an unavailable Docker
//! FAILS the test with a cause-specific message (018-harden-parity-harness). Both
//! CLIs' raw output is preserved under `target/parity/raw/` and a run-report
//! fragment is written to `target/parity/report/parity_state_diff.json`.
//!
//! Each test is a SEPARATE heavy container test (its own 15m nextest timeout);
//! they are deliberately not consolidated. Lives in the `parity` nextest group.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde_json::Value;
use tempfile::TempDir;

use parity_harness::exec::{ExecKind, Invocation, exec_deacon, exec_oracle};
use parity_harness::normalize::{StateSnapshot, container_state, diff_states};
use parity_harness::oracle::{Oracle, VerifiedOracle};
use parity_harness::prereq::require_docker;
use parity_harness::report::{
    CaseResult, Cause, OracleInfo, RawPaths, ReportFragment, now_rfc3339,
};
use parity_harness::waiver::{Scope, Waiver, WaiverSet, field_matches};
use parity_harness::{HarnessError, workspace_root};

/// This binary's name — the fragment key and raw-artifact subdirectory.
const BINARY: &str = "parity_state_diff";

/// Fail the test with the error's cause-specific `Display` message (never the
/// `Debug` form) so an oracle/prereq/normalization failure reads as its remedy.
fn ff<T>(r: Result<T, HarnessError>) -> T {
    r.unwrap_or_else(|e| panic!("{e}"))
}

// ---------------------------------------------------------------------------
// Local docker helpers (copied from parity_observable_state.rs, verbatim
// semantics) so this binary is self-contained under the harness crate.
// ---------------------------------------------------------------------------

fn docker_out_allow_fail(args: &[&str]) -> (bool, String, String) {
    let out = std::process::Command::new("docker")
        .args(args)
        .output()
        .expect("docker should run");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    )
}

/// Run `docker inspect <id>`, parse the JSON array, and return the first element.
fn docker_inspect_one(id: &str) -> Value {
    let out = std::process::Command::new("docker")
        .args(["inspect", id])
        .output()
        .expect("docker should run");
    assert!(
        out.status.success(),
        "docker inspect {} failed: {}",
        id,
        String::from_utf8_lossy(&out.stderr)
    );
    let raw = String::from_utf8_lossy(&out.stdout);
    let arr: Vec<Value> =
        serde_json::from_str(raw.trim()).expect("docker inspect returns a JSON array");
    arr.into_iter()
        .next()
        .expect("docker inspect returns at least one entry")
}

/// Extract a top-level string field from a `deacon up` JSON result on stdout,
/// tolerant of leading log lines before the JSON object.
fn json_field(stdout: &str, field: &str) -> Option<String> {
    let trimmed = stdout.trim();
    let value: Value = serde_json::from_str(trimmed).ok().or_else(|| {
        trimmed
            .rfind('{')
            .and_then(|i| serde_json::from_str(&trimmed[i..]).ok())
    })?;
    value
        .get(field)?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn deacon_compose_down_by_project(project_name: &str) {
    let _ = std::process::Command::new("docker")
        .args([
            "compose",
            "-p",
            project_name,
            "down",
            "--remove-orphans",
            "-v",
            "--rmi",
            "local",
        ])
        .output();
}

/// Best-effort deacon teardown, spawning the deacon binary directly (never
/// routed through the harness). Result ignored.
fn deacon_down(ws: &Path) {
    let _ = std::process::Command::new(std::path::Path::new(env!("CARGO_BIN_EXE_deacon")))
        .args([
            "down",
            "--workspace-folder",
            &ws.to_string_lossy(),
            "--remove",
        ])
        .output();
}

/// The canonicalized workspace path, matching the value both CLIs stamp into
/// the `devcontainer.local_folder` label. Filtering `docker ps` by the raw
/// (un-canonicalized) temp path misses the container on platforms where the
/// temp dir is symlinked (e.g. macOS `/tmp` -> `/private/tmp`), which would
/// make discovery spuriously return nothing.
fn canonical_ws_display(ws: &Path) -> String {
    ws.canonicalize()
        .unwrap_or_else(|_| ws.to_path_buf())
        .display()
        .to_string()
}

/// Discover the first running container for `ws` by its canonicalized
/// `devcontainer.local_folder` label — the reference-compatible discovery
/// label both CLIs stamp. Used to locate the upstream CLI's container (which,
/// unlike deacon, does not report a container id on stdout).
fn upstream_container_id(ws: &Path) -> Option<String> {
    let (ok, out, _) = docker_out_allow_fail(&[
        "ps",
        "--filter",
        &format!(
            "label=devcontainer.local_folder={}",
            canonical_ws_display(ws)
        ),
        "--format",
        "{{.ID}}",
    ]);
    if !ok {
        return None;
    }
    out.lines().find(|s| !s.is_empty()).map(|s| s.to_string())
}

/// Best-effort teardown of every container stamped with this workspace's
/// `devcontainer.local_folder` label (both CLIs stamp it), plus each
/// container's compose project read from its actual
/// `com.docker.compose.project` label. Robust to either CLI's project naming
/// and to the reference CLI not reporting a `composeProjectName` — a guessed
/// `<basename>_devcontainer` would miss upstream's real project because the
/// reference strips a `TempDir`'s leading `.` from the folder basename.
fn sweep_ws_containers(ws: &Path) {
    let (ok, out, _) = docker_out_allow_fail(&[
        "ps",
        "-a",
        "--filter",
        &format!(
            "label=devcontainer.local_folder={}",
            canonical_ws_display(ws)
        ),
        "--format",
        "{{.ID}}",
    ]);
    if !ok {
        return;
    }
    for id in out.lines().filter(|s| !s.is_empty()) {
        let (_, project, _) = docker_out_allow_fail(&[
            "inspect",
            "--format",
            "{{ index .Config.Labels \"com.docker.compose.project\" }}",
            id,
        ]);
        if !project.is_empty() {
            deacon_compose_down_by_project(&project);
        }
        let _ = docker_out_allow_fail(&["rm", "-f", id]);
    }
}

/// RAII cleanup: sweeps every container (and its compose project) for this
/// workspace when dropped — including during panic unwinding, so a failed
/// assertion can never leak Docker state. Declare it right after the
/// workspace path so it drops before the `TempDir` (whose directory must
/// still exist for the label canonicalization to resolve).
struct WsCleanup<'a>(&'a Path);
impl Drop for WsCleanup<'_> {
    fn drop(&mut self) {
        sweep_ws_containers(self.0);
    }
}

// ---------------------------------------------------------------------------
// Parity assertion + raw-paths plumbing.
// ---------------------------------------------------------------------------

/// The four preserved raw-output paths (report-relative) for one compared case.
fn raw_paths(deacon: &Invocation, oracle: &Invocation) -> RawPaths {
    RawPaths {
        deacon_stdout: deacon.stdout_rel.display().to_string(),
        deacon_stderr: deacon.stderr_rel.display().to_string(),
        oracle_stdout: oracle.stdout_rel.display().to_string(),
        oracle_stderr: oracle.stderr_rel.display().to_string(),
    }
}

/// Diff two snapshots, drop caller-allowed and waiver-characterized divergences,
/// write a report fragment, and (on any surviving divergence or stale waiver)
/// fail with a readable per-field report.
///
/// Two allowance mechanisms combine here (both EXACT by default; a trailing `*`
/// makes a matcher a prefix — so `mount:/workspace` must NOT match
/// `mount:/workspaces/sib`):
///
/// - `extra_allowed`: inline, genuinely test-structural allowances (e.g. two
///   distinct fixtures deliberately using different `containerEnv` KEYS) that are
///   NOT divergences versus the reference and thus need no recorded waiver.
/// - state-field waivers under `fixtures/parity-corpus/waivers/` scoped to this
///   binary and `fixture == case`: characterized observable-state divergences
///   that MIRROR the reference, routed through the single `waiver::load` loader
///   (018-harden-parity-harness, research D6). This replaces the retired
///   `KNOWN_INTENTIONAL_DIVERGENCES` / `KNOWN_GAPS` consts; the directory is
///   currently empty, so this consults zero records today. A loaded state-field
///   waiver that matches no observed divergence for this fixture is STALE and
///   fails the run naming its id (FR-011).
async fn assert_parity(
    case: &str,
    oracle: &VerifiedOracle,
    deacon: &StateSnapshot,
    upstream: &StateSnapshot,
    extra_allowed: &[&str],
    raw: RawPaths,
) {
    let started = now_rfc3339();
    let divs = diff_states(deacon, upstream);

    // State-field waivers scoped to this binary + fixture (the single loader
    // reads them from `fixtures/parity-corpus/waivers/`).
    let corpus_root = workspace_root().join("fixtures/parity-corpus");
    let waivers = ff(WaiverSet::load(&corpus_root));
    let field_waivers: Vec<&Waiver> = waivers
        .state_field_waivers(BINARY)
        .into_iter()
        .filter(|w| matches!(&w.scope, Scope::StateField { fixture, .. } if fixture == case))
        .collect();

    let mut consumed: HashSet<String> = HashSet::new();
    let unexpected: Vec<_> = divs
        .iter()
        .filter(|d| {
            if extra_allowed.iter().any(|m| field_matches(&d.field, m)) {
                return false;
            }
            // A matching state-field waiver characterizes this divergence.
            for w in &field_waivers {
                if let Scope::StateField { field, .. } = &w.scope {
                    if field_matches(&d.field, field) {
                        consumed.insert(w.id.clone());
                        return false;
                    }
                }
            }
            true
        })
        .collect();

    // Stale state-field waivers for this fixture that matched no divergence.
    let stale: Vec<String> = field_waivers
        .iter()
        .filter(|w| !consumed.contains(&w.id))
        .map(|w| w.id.clone())
        .collect();

    let mut panic_msg = None;
    let case_result = if !unexpected.is_empty() {
        let detail = unexpected
            .iter()
            .map(|d| format!("{}: {}", d.field, d.detail))
            .collect::<Vec<_>>()
            .join("\n");
        let mut msg = String::from("observable-state parity divergence(s) deacon vs upstream:\n");
        for d in &unexpected {
            msg.push_str(&format!("  - {}: {}\n", d.field, d.detail));
        }
        panic_msg = Some(msg);
        CaseResult::fail(case, Cause::Divergence, Some(detail), raw)
    } else if !stale.is_empty() {
        let detail = format!("stale state-field waiver(s): {}", stale.join(", "));
        panic_msg = Some(detail.clone());
        CaseResult::fail(case, Cause::Divergence, Some(detail), raw)
    } else if !consumed.is_empty() {
        let mut ids: Vec<String> = consumed.into_iter().collect();
        ids.sort();
        CaseResult::pass_waived(case, ids, raw)
    } else {
        CaseResult::pass(case, raw)
    };

    let finished = now_rfc3339();
    ff(ReportFragment::new(
        BINARY,
        OracleInfo::from(oracle),
        started,
        finished,
        vec![case_result],
        Vec::new(),
    )
    .write()
    .await);

    if let Some(msg) = panic_msg {
        panic!("{msg}");
    }
}

// ---------------------------------------------------------------------------
// Fixture writers
// ---------------------------------------------------------------------------

/// Single-container fixture: `containerEnv` + a `devcontainer.json` bind mount.
fn write_single_fixture(ws: &Path, label: &str) {
    let sib = ws.join("sib");
    fs::create_dir_all(&sib).unwrap();
    fs::write(sib.join("marker.txt"), "from-sib").unwrap();
    fs::create_dir_all(ws.join(".devcontainer")).unwrap();
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        format!(
            r#"{{
  "name": "StateDiffSingle-{label}",
  "image": "debian:bookworm-slim",
  "workspaceFolder": "/workspace",
  "workspaceMount": "source=${{localWorkspaceFolder}},target=/workspace,type=bind",
  "containerEnv": {{ "SC_ENV": "yes" }},
  "mounts": [
    "source=${{localWorkspaceFolder}}/sib,target=/workspaces/sib,type=bind"
  ]
}}
"#
        ),
    )
    .unwrap();
}

/// Compose fixture with `containerEnv`, a `devcontainer.json` bind mount, and a
/// local Feature declaring BOTH a `containerEnv` (positive control — baked into
/// the feature image, so present on both CLIs) and a volume `mount` (the #272
/// gap — dropped by deacon's compose path).
fn write_compose_feature_fixture(ws: &Path, label: &str) {
    let sib = ws.join("sib");
    fs::create_dir_all(&sib).unwrap();
    fs::write(sib.join("marker.txt"), "from-sib").unwrap();
    fs::write(
        ws.join("docker-compose.yml"),
        "services:\n  app:\n    image: debian:bookworm-slim\n    command: [\"sleep\", \"infinity\"]\n",
    )
    .unwrap();
    fs::create_dir_all(ws.join(".devcontainer/mountprobe")).unwrap();
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        format!(
            r#"{{
  "name": "StateDiffCompose-{label}",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace",
  "workspaceMount": "source=${{localWorkspaceFolder}},target=/workspace,type=bind",
  "containerEnv": {{ "CE_ENV": "yes" }},
  "mounts": [
    "source=${{localWorkspaceFolder}}/sib,target=/workspaces/sib,type=bind"
  ],
  "features": {{ "./mountprobe": {{}} }}
}}
"#
        ),
    )
    .unwrap();
    fs::write(
        ws.join(".devcontainer/mountprobe/devcontainer-feature.json"),
        r#"{
  "id": "mountprobe",
  "version": "1.0.0",
  "name": "Mount Probe",
  "containerEnv": { "FEATURE_ENV_CONTROL": "yes" },
  "mounts": [ { "source": "feat-probe-vol", "target": "/feat-mnt", "type": "volume" } ]
}
"#,
    )
    .unwrap();
    fs::write(
        ws.join(".devcontainer/mountprobe/install.sh"),
        "#!/bin/sh\nset -e\necho \"mountprobe feature installed\"\n",
    )
    .unwrap();
}

/// Compose fixture (feature-free) mirroring `write_single_fixture`'s
/// `containerEnv` + bind mount, for the intra-deacon single-vs-compose diff.
fn write_compose_plain_fixture(ws: &Path, label: &str) {
    let sib = ws.join("sib");
    fs::create_dir_all(&sib).unwrap();
    fs::write(sib.join("marker.txt"), "from-sib").unwrap();
    fs::write(
        ws.join("docker-compose.yml"),
        "services:\n  app:\n    image: debian:bookworm-slim\n    command: [\"sleep\", \"infinity\"]\n",
    )
    .unwrap();
    fs::create_dir_all(ws.join(".devcontainer")).unwrap();
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        format!(
            r#"{{
  "name": "StateDiffIntra-{label}",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace",
  "workspaceMount": "source=${{localWorkspaceFolder}},target=/workspace,type=bind",
  "containerEnv": {{ "IX_ENV": "yes" }},
  "mounts": [
    "source=${{localWorkspaceFolder}}/sib,target=/workspaces/sib,type=bind"
  ]
}}
"#
        ),
    )
    .unwrap();
}

/// Single-container fixture with `workspaceFolder` set but NO explicit
/// `workspaceMount`, to characterize the default-workspace-mount-target
/// divergence the differ surfaced (see
/// `state_diff_default_workspace_mount_target_divergence`).
fn write_default_mount_fixture(ws: &Path, label: &str) {
    fs::create_dir_all(ws.join(".devcontainer")).unwrap();
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        format!(
            r#"{{
  "name": "StateDiffDefaultMount-{label}",
  "image": "debian:bookworm-slim",
  "workspaceFolder": "/workspace",
  "containerEnv": {{ "DM_ENV": "yes" }}
}}
"#
        ),
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// Bring-up helpers
// ---------------------------------------------------------------------------

/// Bring `ws` up with deacon, capture the container state. Returns the bounded
/// invocation (for raw-artifact paths), the container id, and the snapshot.
async fn deacon_up_snapshot(
    ws: &Path,
    deacon_bin: &Path,
    case: &str,
) -> (Invocation, String, StateSnapshot) {
    let ws_str = ws.to_string_lossy().into_owned();
    let inv = ff(exec_deacon(
        BINARY,
        case,
        ExecKind::Lifecycle,
        deacon_bin,
        &["up", "--workspace-folder", &ws_str],
        ws,
    )
    .await);
    assert!(
        inv.success,
        "deacon up failed: {}",
        String::from_utf8_lossy(&inv.stderr)
    );
    let id = json_field(&inv.stdout_string(), "containerId")
        .expect("deacon up should report a containerId");
    let snap = ff(container_state(case, &docker_inspect_one(&id)));
    (inv, id, snap)
}

/// Bring `ws` up with the upstream oracle, capture the container state.
async fn upstream_up_snapshot(
    ws: &Path,
    oracle: &VerifiedOracle,
    case: &str,
) -> (Invocation, String, StateSnapshot) {
    let ws_str = ws.to_string_lossy().into_owned();
    let inv = ff(exec_oracle(
        BINARY,
        case,
        ExecKind::Lifecycle,
        &oracle.path,
        &["up", "--workspace-folder", &ws_str],
        ws,
    )
    .await);
    assert!(
        inv.success,
        "upstream up failed: {}",
        String::from_utf8_lossy(&inv.stderr)
    );
    let id = upstream_container_id(ws).expect("no upstream container found by label");
    let snap = ff(container_state(case, &docker_inspect_one(&id)));
    (inv, id, snap)
}

// ===========================================================================
// Test 1: single-container outcome parity.
// ===========================================================================

#[tokio::test]
async fn state_diff_single_container_parity() {
    const CASE: &str = "single-container-parity";
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));

    let up_tmp = TempDir::new().unwrap();
    let up_ws = up_tmp.path();
    let _up_clean = WsCleanup(up_ws);
    write_single_fixture(up_ws, "upstream");
    let (up_inv, up_id, up_snap) = upstream_up_snapshot(up_ws, &oracle, CASE).await;

    let d_tmp = TempDir::new().unwrap();
    let d_ws = d_tmp.path();
    let _d_clean = WsCleanup(d_ws);
    write_single_fixture(d_ws, "deacon");
    let (d_inv, _d_id, d_snap) = deacon_up_snapshot(d_ws, deacon_bin, CASE).await;

    // Tear down BEFORE asserting so a failed assertion never leaks state.
    let _ = docker_out_allow_fail(&["rm", "-f", &up_id]);
    deacon_down(d_ws);

    // Vacuity guards: the fixture's own markers must be captured, or the diff
    // would be comparing empty/incomplete snapshots.
    assert!(
        d_snap.env.contains("SC_ENV=yes"),
        "deacon snapshot missing fixture marker SC_ENV: {:?}",
        d_snap.env
    );
    assert!(
        up_snap.env.contains("SC_ENV=yes"),
        "upstream snapshot missing fixture marker SC_ENV: {:?}",
        up_snap.env
    );
    assert!(
        d_snap.mounts.contains_key("/workspaces/sib"),
        "deacon snapshot missing config mount /workspaces/sib: {:?}",
        d_snap.mounts
    );
    assert!(
        up_snap.mounts.contains_key("/workspaces/sib"),
        "upstream snapshot missing config mount /workspaces/sib: {:?}",
        up_snap.mounts
    );

    assert_parity(
        CASE,
        &oracle,
        &d_snap,
        &up_snap,
        &[],
        raw_paths(&d_inv, &up_inv),
    )
    .await;
}

// ===========================================================================
// Test 2: compose outcome parity — covers the (fixed) #272 feature-mount gap.
// `execute_compose_up` now resolves features before folding their `mounts`
// into `additional_mounts` (mirrors the single-container path), so the
// feature mount must now match upstream exactly; any compose divergence
// fails. The positive control (feature `containerEnv`, baked into the image)
// proves the Feature installed on deacon's compose path.
// ===========================================================================

#[tokio::test]
async fn state_diff_compose_parity_with_feature_mount_gap() {
    const CASE: &str = "compose-parity-with-feature-mount-gap";
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));

    let up_tmp = TempDir::new().unwrap();
    let up_ws = up_tmp.path();
    let _up_clean = WsCleanup(up_ws);
    write_compose_feature_fixture(up_ws, "upstream");
    let (up_inv, _up_id, up_snap) = upstream_up_snapshot(up_ws, &oracle, CASE).await;

    let d_tmp = TempDir::new().unwrap();
    let d_ws = d_tmp.path();
    let _d_clean = WsCleanup(d_ws);
    write_compose_feature_fixture(d_ws, "deacon");
    let (d_inv, _d_id, d_snap) = deacon_up_snapshot(d_ws, deacon_bin, CASE).await;

    // Tear down BEFORE asserting (compose projects + the shared feature volume).
    sweep_ws_containers(up_ws);
    sweep_ws_containers(d_ws);
    let _ = docker_out_allow_fail(&["volume", "rm", "-f", "feat-probe-vol"]);

    // Positive control: feature containerEnv baked into the image → present on
    // BOTH CLIs, proving the feature installed on deacon's compose path.
    assert!(
        d_snap.env.contains("FEATURE_ENV_CONTROL=yes"),
        "deacon compose snapshot missing baked feature env (feature did not install?): {:?}",
        d_snap.env
    );
    assert!(
        up_snap.env.contains("FEATURE_ENV_CONTROL=yes"),
        "upstream compose snapshot missing baked feature env: {:?}",
        up_snap.env
    );
    // #272 fixed: deacon now applies the feature-contributed mount, matching
    // upstream, on the compose path.
    assert!(
        up_snap.mounts.contains_key("/feat-mnt"),
        "upstream should apply the feature mount /feat-mnt: {:?}",
        up_snap.mounts
    );
    assert!(
        d_snap.mounts.contains_key("/feat-mnt"),
        "deacon should now apply the feature mount /feat-mnt (#272 fix): {:?}",
        d_snap.mounts
    );

    assert_parity(
        CASE,
        &oracle,
        &d_snap,
        &up_snap,
        &[],
        raw_paths(&d_inv, &up_inv),
    )
    .await;
}

// ===========================================================================
// Test 3: intra-deacon single-vs-compose parity. No upstream needed — diffs
// deacon's OWN single-container state against its compose state for the same
// logical config. This catches "compose drops X that single-container applies"
// bugs (the #266 / #272 class) directly, and would have caught #266.
// ===========================================================================

#[tokio::test]
async fn state_diff_intra_deacon_single_vs_compose() {
    const CASE: &str = "intra-deacon-single-vs-compose";
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));

    let single_tmp = TempDir::new().unwrap();
    let single_ws = single_tmp.path();
    let _single_clean = WsCleanup(single_ws);
    write_single_fixture(single_ws, "intra");
    // Reuse SC_ENV marker name is fine; single fixture already sets SC_ENV.
    let (single_inv, _s_id, single_snap) = deacon_up_snapshot(single_ws, deacon_bin, CASE).await;

    let compose_tmp = TempDir::new().unwrap();
    let compose_ws = compose_tmp.path();
    let _compose_clean = WsCleanup(compose_ws);
    write_compose_plain_fixture(compose_ws, "intra");
    let (compose_inv, _c_id, compose_snap) = deacon_up_snapshot(compose_ws, deacon_bin, CASE).await;

    // Tear down BEFORE asserting.
    deacon_down(single_ws);
    sweep_ws_containers(compose_ws);

    // Vacuity guards + normalize the differently-named env markers: the two
    // fixtures intentionally use different containerEnv KEYS (SC_ENV vs IX_ENV)
    // because they are distinct fixtures; allow those two so the diff focuses
    // on STRUCTURAL parity (mounts) rather than the deliberately-different key.
    assert!(
        single_snap.env.contains("SC_ENV=yes"),
        "single snapshot missing SC_ENV: {:?}",
        single_snap.env
    );
    assert!(
        compose_snap.env.contains("IX_ENV=yes"),
        "compose snapshot missing IX_ENV: {:?}",
        compose_snap.env
    );
    assert!(
        single_snap.mounts.contains_key("/workspaces/sib"),
        "single snapshot missing config mount: {:?}",
        single_snap.mounts
    );
    assert!(
        compose_snap.mounts.contains_key("/workspaces/sib"),
        "compose snapshot missing config mount (compose dropped it — #266 class regression): {:?}",
        compose_snap.mounts
    );

    // Allowed divergences:
    //  * env:SC_ENV / env:IX_ENV — the two fixtures deliberately use different
    //    containerEnv KEYS, so the diff focuses on STRUCTURAL parity (mounts).
    //  * mount:/workspace — deacon's compose path does not mount the workspace
    //    folder by default (only with `--workspace-mount-consistency`, and it
    //    ignores `workspaceMount`), whereas the single-container path honors the
    //    pinned `workspaceMount`. This single-vs-compose difference MIRRORS the
    //    reference CLI (verified: a plain compose devcontainer yields zero
    //    workspace binds on BOTH CLIs — the compose file owns the workspace
    //    mount), so it is the compose model, NOT a deacon parity gap.
    //
    // No upstream side here: both invocations are deacon, so the SECOND
    // (compose) invocation fills the oracle_* raw slots (four real artifact
    // paths, as the raw-paths invariant requires).
    assert_parity(
        CASE,
        &oracle,
        &single_snap,
        &compose_snap,
        &["env:SC_ENV", "env:IX_ENV", "mount:/workspace"],
        raw_paths(&single_inv, &compose_inv),
    )
    .await;
}

// ===========================================================================
// Test 4: default-workspace-mount-target divergence (surfaced by this differ
// on its first run). With `workspaceFolder` set but no explicit
// `workspaceMount`, deacon mounts the workspace AT `workspaceFolder`
// (`/workspace`), while the reference CLI mounts it at the spec default
// `/workspaces/<basename>` and uses `workspaceFolder` only as the working
// directory (containers.dev: `workspaceMount` overrides the mount; its default
// is `/workspaces/${localWorkspaceFolderBasename}`, independent of
// `workspaceFolder`). See `crates/core/src/docker.rs:2150-2162`.
//
// This test CHARACTERIZES the divergence so it is tracked, not silently
// allowlisted: if deacon changes its default to match the spec (mounting at
// `/workspaces/<basename>`), this test flips red and forces the decision to be
// made deliberately. The differ's other fixtures pin `workspaceMount`
// explicitly so both CLIs agree and the divergence does not mask real findings.
// ===========================================================================

#[tokio::test]
async fn state_diff_default_workspace_mount_target_divergence() {
    const CASE: &str = "default-workspace-mount-target-divergence";
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));

    let up_tmp = TempDir::new().unwrap();
    let up_ws = up_tmp.path();
    let _up_clean = WsCleanup(up_ws);
    write_default_mount_fixture(up_ws, "upstream");
    let (_up_inv, up_id, up_snap) = upstream_up_snapshot(up_ws, &oracle, CASE).await;

    let d_tmp = TempDir::new().unwrap();
    let d_ws = d_tmp.path();
    let _d_clean = WsCleanup(d_ws);
    write_default_mount_fixture(d_ws, "deacon");
    let (_d_inv, _d_id, d_snap) = deacon_up_snapshot(d_ws, deacon_bin, CASE).await;

    let _ = docker_out_allow_fail(&["rm", "-f", &up_id]);
    deacon_down(d_ws);

    // Vacuity guard: both fixtures actually launched with the marker env.
    assert!(
        d_snap.env.contains("DM_ENV=yes") && up_snap.env.contains("DM_ENV=yes"),
        "fixture marker DM_ENV missing (deacon={:?} upstream={:?})",
        d_snap.env,
        up_snap.env
    );

    let is_workspace_bind = |snap: &StateSnapshot, dest: &str| {
        snap.mounts
            .get(dest)
            .is_some_and(|m| m.mount_type == "bind")
    };

    // deacon: workspace mounted AT workspaceFolder.
    assert!(
        is_workspace_bind(&d_snap, "/workspace"),
        "deacon should mount the workspace at workspaceFolder (/workspace): {:?}",
        d_snap.mounts
    );
    // upstream: workspace mounted at the spec default /workspaces/<basename>,
    // NOT at workspaceFolder.
    assert!(
        !up_snap.mounts.contains_key("/workspace"),
        "reference CLI unexpectedly mounted the workspace at /workspace; the \
         default-mount-target divergence may be gone — reconcile deacon's \
         default (docker.rs) and update this test: {:?}",
        up_snap.mounts
    );
    assert!(
        up_snap
            .mounts
            .keys()
            .any(|d| d.starts_with("/workspaces/") && is_workspace_bind(&up_snap, d)),
        "reference CLI should mount the workspace under /workspaces/<basename>: {:?}",
        up_snap.mounts
    );
}

// ===========================================================================
// Test 5: Dockerfile build + non-root containerUser/remoteUser + Dockerfile ENV
// + containerEnv. Exercises the image-BUILD path (deacon `deacon-build:*` vs
// upstream `vsc-*`) and user parity beyond the empty≡root normalization.
// ===========================================================================

fn write_dockerfile_user_fixture(ws: &Path, label: &str) {
    fs::create_dir_all(ws.join(".devcontainer")).unwrap();
    fs::write(
        ws.join(".devcontainer/Dockerfile"),
        "FROM debian:bookworm-slim\n\
         RUN useradd -m -u 1000 dev\n\
         ENV DOCKERFILE_ENV=yes\n",
    )
    .unwrap();
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        format!(
            r#"{{
  "name": "StateDiffDockerfile-{label}",
  "build": {{ "dockerfile": "Dockerfile" }},
  "workspaceFolder": "/workspace",
  "workspaceMount": "source=${{localWorkspaceFolder}},target=/workspace,type=bind",
  "containerUser": "dev",
  "remoteUser": "dev",
  "containerEnv": {{ "DF_ENV": "yes" }}
}}
"#
        ),
    )
    .unwrap();
}

#[tokio::test]
async fn state_diff_dockerfile_build_and_nonroot_user() {
    const CASE: &str = "dockerfile-build-and-nonroot-user";
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));

    let up_tmp = TempDir::new().unwrap();
    let up_ws = up_tmp.path();
    let _up_clean = WsCleanup(up_ws);
    write_dockerfile_user_fixture(up_ws, "upstream");
    let (up_inv, up_id, up_snap) = upstream_up_snapshot(up_ws, &oracle, CASE).await;

    let d_tmp = TempDir::new().unwrap();
    let d_ws = d_tmp.path();
    let _d_clean = WsCleanup(d_ws);
    write_dockerfile_user_fixture(d_ws, "deacon");
    let (d_inv, _d_id, d_snap) = deacon_up_snapshot(d_ws, deacon_bin, CASE).await;

    let _ = docker_out_allow_fail(&["rm", "-f", &up_id]);
    deacon_down(d_ws);

    // Vacuity guards: Dockerfile ENV + containerEnv actually landed.
    for (who, snap) in [("deacon", &d_snap), ("upstream", &up_snap)] {
        assert!(
            snap.env.contains("DOCKERFILE_ENV=yes"),
            "{who} missing Dockerfile ENV: {:?}",
            snap.env
        );
        assert!(
            snap.env.contains("DF_ENV=yes"),
            "{who} missing containerEnv DF_ENV: {:?}",
            snap.env
        );
    }

    // #274 fixed: deacon now passes `--user <containerUser>` at container
    // create time, so `Config.User` matches the reference.
    assert_eq!(
        up_snap.user, "dev",
        "reference should set Config.User=dev from containerUser: {:?}",
        up_snap.user
    );
    assert_eq!(
        d_snap.user, "dev",
        "deacon should now set Config.User=dev from containerUser (#274 fix): {:?}",
        d_snap.user
    );
    assert_parity(
        CASE,
        &oracle,
        &d_snap,
        &up_snap,
        &[],
        raw_paths(&d_inv, &up_inv),
    )
    .await;
}

// ===========================================================================
// Test 6: appPort → PUBLISHED ports. Exercises `HostConfig.PortBindings`
// parity (a container port published to the host).
// ===========================================================================

fn write_appport_fixture(ws: &Path, label: &str) {
    fs::create_dir_all(ws.join(".devcontainer")).unwrap();
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        format!(
            r#"{{
  "name": "StateDiffAppPort-{label}",
  "image": "debian:bookworm-slim",
  "workspaceFolder": "/workspace",
  "workspaceMount": "source=${{localWorkspaceFolder}},target=/workspace,type=bind",
  "appPort": [3000],
  "containerEnv": {{ "AP_ENV": "yes" }}
}}
"#
        ),
    )
    .unwrap();
}

#[tokio::test]
async fn state_diff_appport_published_ports() {
    const CASE: &str = "appport-published-ports";
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));

    // `appPort: [3000]` publishes to the FIXED host port 3000 on BOTH CLIs (per
    // spec), so the two containers cannot coexist — bring up upstream, snapshot,
    // TEAR IT DOWN to free host:3000, then bring up deacon. (This is not a
    // parity difference; it is the shared host-port resource.)
    let up_tmp = TempDir::new().unwrap();
    let up_ws = up_tmp.path();
    let _up_clean = WsCleanup(up_ws);
    write_appport_fixture(up_ws, "upstream");
    let (up_inv, up_id, up_snap) = upstream_up_snapshot(up_ws, &oracle, CASE).await;
    let _ = docker_out_allow_fail(&["rm", "-f", &up_id]);

    let d_tmp = TempDir::new().unwrap();
    let d_ws = d_tmp.path();
    let _d_clean = WsCleanup(d_ws);
    write_appport_fixture(d_ws, "deacon");
    let (d_inv, _d_id, d_snap) = deacon_up_snapshot(d_ws, deacon_bin, CASE).await;
    deacon_down(d_ws);

    // Vacuity guard: the appPort actually published on at least one side, or the
    // test proves nothing about port parity.
    assert!(
        d_snap.published_ports.contains("3000/tcp") || up_snap.published_ports.contains("3000/tcp"),
        "neither CLI published appPort 3000/tcp (deacon={:?} upstream={:?}) — fixture broken?",
        d_snap.published_ports,
        up_snap.published_ports
    );

    assert_parity(
        CASE,
        &oracle,
        &d_snap,
        &up_snap,
        &[],
        raw_paths(&d_inv, &up_inv),
    )
    .await;
}

// ===========================================================================
// Test 7: mount variety — a read-only bind and a tmpfs mount. Exercises the
// mount read-only flag and the tmpfs mount type.
// ===========================================================================

fn write_mounts_variety_fixture(ws: &Path, label: &str) {
    let ro = ws.join("ro");
    fs::create_dir_all(&ro).unwrap();
    fs::write(ro.join("marker.txt"), "ro").unwrap();
    fs::create_dir_all(ws.join(".devcontainer")).unwrap();
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        format!(
            r#"{{
  "name": "StateDiffMounts-{label}",
  "image": "debian:bookworm-slim",
  "workspaceFolder": "/workspace",
  "workspaceMount": "source=${{localWorkspaceFolder}},target=/workspace,type=bind",
  "containerEnv": {{ "MV_ENV": "yes" }},
  "mounts": [
    "source=${{localWorkspaceFolder}}/ro,target=/ro,type=bind,readonly",
    "type=tmpfs,target=/tmpmnt"
  ]
}}
"#
        ),
    )
    .unwrap();
}

#[tokio::test]
async fn state_diff_mount_variety_readonly_and_tmpfs() {
    const CASE: &str = "mount-variety-readonly-and-tmpfs";
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));

    let up_tmp = TempDir::new().unwrap();
    let up_ws = up_tmp.path();
    let _up_clean = WsCleanup(up_ws);
    write_mounts_variety_fixture(up_ws, "upstream");
    let (up_inv, up_id, up_snap) = upstream_up_snapshot(up_ws, &oracle, CASE).await;

    let d_tmp = TempDir::new().unwrap();
    let d_ws = d_tmp.path();
    let _d_clean = WsCleanup(d_ws);
    write_mounts_variety_fixture(d_ws, "deacon");
    let (d_inv, _d_id, d_snap) = deacon_up_snapshot(d_ws, deacon_bin, CASE).await;

    let _ = docker_out_allow_fail(&["rm", "-f", &up_id]);
    deacon_down(d_ws);

    // Vacuity guard: the read-only bind landed and is actually read-only on at
    // least one side (so the ro-flag comparison is meaningful).
    assert!(
        d_snap.mounts.get("/ro").is_some_and(|m| m.ro)
            || up_snap.mounts.get("/ro").is_some_and(|m| m.ro),
        "neither CLI produced a read-only /ro bind (deacon={:?} upstream={:?})",
        d_snap.mounts.get("/ro"),
        up_snap.mounts.get("/ro")
    );

    assert_parity(
        CASE,
        &oracle,
        &d_snap,
        &up_snap,
        &[],
        raw_paths(&d_inv, &up_inv),
    )
    .await;
}

// ===========================================================================
// Test 8: compose with a db sidecar + a compose-declared named volume mounted
// into the primary service. Exercises multi-service compose and volume
// pass-through parity on the inspected (primary) container.
// ===========================================================================

fn write_compose_volume_fixture(ws: &Path, label: &str) {
    fs::write(
        ws.join("docker-compose.yml"),
        "services:\n  \
           app:\n    \
             image: debian:bookworm-slim\n    \
             command: [\"sleep\", \"infinity\"]\n    \
             volumes:\n      \
               - appdata:/data\n  \
           db:\n    \
             image: debian:bookworm-slim\n    \
             command: [\"sleep\", \"infinity\"]\n\
         volumes:\n  \
           appdata:\n",
    )
    .unwrap();
    fs::create_dir_all(ws.join(".devcontainer")).unwrap();
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        format!(
            r#"{{
  "name": "StateDiffComposeVol-{label}",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace",
  "containerEnv": {{ "CV_ENV": "yes" }}
}}
"#
        ),
    )
    .unwrap();
}

#[tokio::test]
async fn state_diff_compose_sidecar_and_named_volume() {
    const CASE: &str = "compose-sidecar-and-named-volume";
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));

    let up_tmp = TempDir::new().unwrap();
    let up_ws = up_tmp.path();
    let _up_clean = WsCleanup(up_ws);
    write_compose_volume_fixture(up_ws, "upstream");
    let (up_inv, _up_id, up_snap) = upstream_up_snapshot(up_ws, &oracle, CASE).await;

    let d_tmp = TempDir::new().unwrap();
    let d_ws = d_tmp.path();
    let _d_clean = WsCleanup(d_ws);
    write_compose_volume_fixture(d_ws, "deacon");
    let (d_inv, _d_id, d_snap) = deacon_up_snapshot(d_ws, deacon_bin, CASE).await;

    sweep_ws_containers(up_ws);
    sweep_ws_containers(d_ws);

    // Vacuity guards: the compose-declared named volume + containerEnv landed on
    // the inspected (primary) container.
    for (who, snap) in [("deacon", &d_snap), ("upstream", &up_snap)] {
        assert!(
            snap.mounts
                .get("/data")
                .is_some_and(|m| m.mount_type == "volume"),
            "{who} missing compose-declared named volume at /data: {:?}",
            snap.mounts
        );
        assert!(
            snap.env.contains("CV_ENV=yes"),
            "{who} missing containerEnv CV_ENV: {:?}",
            snap.env
        );
    }

    assert_parity(
        CASE,
        &oracle,
        &d_snap,
        &up_snap,
        &[],
        raw_paths(&d_inv, &up_inv),
    )
    .await;
}
