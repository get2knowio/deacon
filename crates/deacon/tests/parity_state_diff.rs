//! #267 follow-up: normalized observable-state parity between deacon and the
//! reference `@devcontainers/cli`.
//!
//! Unlike the launch-parity checks (both CLIs exit 0) and the per-bug field
//! probes in `parity_observable_state.rs`, this binary brings a fixture up with
//! BOTH CLIs and diffs the resulting container's NORMALIZED observable state
//! (`docker inspect`: mounts, env, labels, user, working dir, ports) field by
//! field via the differ in `parity_utils`. Any divergence that is not an
//! explicit intentional divergence or a tracked known gap (`KNOWN_GAPS`, e.g.
//! #272) fails the test — catching outcome drift that a launch check misses.
//!
//! Triple-gated like the rest of the `parity_*` suite (`DEACON_PARITY=1`,
//! Docker, `devcontainer` CLI). Cleanly skips when any gate is unmet. Lives in
//! the `parity` nextest group (15m slow-timeout) via `.config/nextest.toml`.

use std::fs;
use std::path::Path;
use tempfile::TempDir;

mod parity_utils;
use parity_utils::{
    StateSnapshot, WsCleanup, assert_snapshots_parity, deacon_down, docker_out_allow_fail, gated,
    json_field, normalized_state, run_deacon, run_upstream, sweep_ws_containers,
    upstream_container_id,
};

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

fn deacon_up_snapshot(ws: &Path) -> (std::process::Output, String, StateSnapshot) {
    let out = run_deacon(ws, &["up", "--workspace-folder", &ws.to_string_lossy()]).unwrap();
    assert!(
        out.status.success(),
        "deacon up failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let id = json_field(&out, "containerId").expect("deacon up should report a containerId");
    let snap = normalized_state(&id);
    (out, id, snap)
}

fn upstream_up_snapshot(ws: &Path) -> (String, StateSnapshot) {
    let out = run_upstream(ws, &["up", "--workspace-folder", &ws.to_string_lossy()]).unwrap();
    assert!(
        out.status.success(),
        "upstream up failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let id = upstream_container_id(ws).expect("no upstream container found by label");
    let snap = normalized_state(&id);
    (id, snap)
}

// ===========================================================================
// Test 1: single-container outcome parity.
// ===========================================================================

#[test]
fn state_diff_single_container_parity() {
    if !gated() {
        return;
    }

    let up_tmp = TempDir::new().unwrap();
    let up_ws = up_tmp.path();
    let _up_clean = WsCleanup(up_ws);
    write_single_fixture(up_ws, "upstream");
    let (up_id, up_snap) = upstream_up_snapshot(up_ws);

    let d_tmp = TempDir::new().unwrap();
    let d_ws = d_tmp.path();
    let _d_clean = WsCleanup(d_ws);
    write_single_fixture(d_ws, "deacon");
    let (_d_out, _d_id, d_snap) = deacon_up_snapshot(d_ws);

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

    assert_snapshots_parity(&d_snap, &up_snap, &[]);
}

// ===========================================================================
// Test 2: compose outcome parity — exercises the #272 feature-mount gap. The
// gap classifies as a tracked KNOWN_GAP (passes today); any OTHER compose
// divergence fails. The positive control (feature `containerEnv`, baked into
// the image) proves the Feature installed on deacon's compose path, so a
// missing /feat-mnt is specifically the runtime mount being dropped.
// ===========================================================================

#[test]
fn state_diff_compose_parity_with_feature_mount_gap() {
    if !gated() {
        return;
    }

    let up_tmp = TempDir::new().unwrap();
    let up_ws = up_tmp.path();
    let _up_clean = WsCleanup(up_ws);
    write_compose_feature_fixture(up_ws, "upstream");
    let (_up_id, up_snap) = upstream_up_snapshot(up_ws);

    let d_tmp = TempDir::new().unwrap();
    let d_ws = d_tmp.path();
    let _d_clean = WsCleanup(d_ws);
    write_compose_feature_fixture(d_ws, "deacon");
    let (_d_out, _d_id, d_snap) = deacon_up_snapshot(d_ws);

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
    // The #272 gap itself: upstream has the feature mount, deacon does not.
    // Asserting this shape keeps the KNOWN_GAP honest — if deacon starts
    // applying it, this assertion flips and the gap entry should be removed.
    assert!(
        up_snap.mounts.contains_key("/feat-mnt"),
        "upstream should apply the feature mount /feat-mnt: {:?}",
        up_snap.mounts
    );
    assert!(
        !d_snap.mounts.contains_key("/feat-mnt"),
        "deacon unexpectedly applied /feat-mnt — #272 may be FIXED; remove the KNOWN_GAP entry and drop this assertion: {:?}",
        d_snap.mounts
    );

    // Everything except the tracked #272 gap must match.
    assert_snapshots_parity(&d_snap, &up_snap, &[]);
}

// ===========================================================================
// Test 3: intra-deacon single-vs-compose parity. No upstream needed — diffs
// deacon's OWN single-container state against its compose state for the same
// logical config. This catches "compose drops X that single-container applies"
// bugs (the #266 / #272 class) directly, and would have caught #266.
// ===========================================================================

#[test]
fn state_diff_intra_deacon_single_vs_compose() {
    if !gated() {
        return;
    }

    let single_tmp = TempDir::new().unwrap();
    let single_ws = single_tmp.path();
    let _single_clean = WsCleanup(single_ws);
    write_single_fixture(single_ws, "intra");
    // Reuse SC_ENV marker name is fine; single fixture already sets SC_ENV.
    let (_s_out, _s_id, single_snap) = deacon_up_snapshot(single_ws);

    let compose_tmp = TempDir::new().unwrap();
    let compose_ws = compose_tmp.path();
    let _compose_clean = WsCleanup(compose_ws);
    write_compose_plain_fixture(compose_ws, "intra");
    let (_c_out, _c_id, compose_snap) = deacon_up_snapshot(compose_ws);

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
    assert_snapshots_parity(
        &single_snap,
        &compose_snap,
        &["env:SC_ENV", "env:IX_ENV", "mount:/workspace"],
    );
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

#[test]
fn state_diff_default_workspace_mount_target_divergence() {
    if !gated() {
        return;
    }

    let up_tmp = TempDir::new().unwrap();
    let up_ws = up_tmp.path();
    let _up_clean = WsCleanup(up_ws);
    write_default_mount_fixture(up_ws, "upstream");
    let (up_id, up_snap) = upstream_up_snapshot(up_ws);

    let d_tmp = TempDir::new().unwrap();
    let d_ws = d_tmp.path();
    let _d_clean = WsCleanup(d_ws);
    write_default_mount_fixture(d_ws, "deacon");
    let (_d_out, _d_id, d_snap) = deacon_up_snapshot(d_ws);

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

#[test]
fn state_diff_dockerfile_build_and_nonroot_user() {
    if !gated() {
        return;
    }

    let up_tmp = TempDir::new().unwrap();
    let up_ws = up_tmp.path();
    let _up_clean = WsCleanup(up_ws);
    write_dockerfile_user_fixture(up_ws, "upstream");
    let (up_id, up_snap) = upstream_up_snapshot(up_ws);

    let d_tmp = TempDir::new().unwrap();
    let d_ws = d_tmp.path();
    let _d_clean = WsCleanup(d_ws);
    write_dockerfile_user_fixture(d_ws, "deacon");
    let (_d_out, _d_id, d_snap) = deacon_up_snapshot(d_ws);

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

    // #274: deacon does not apply `containerUser` to the created container's
    // `Config.User` (it runs as root, applying the user only at exec time),
    // whereas the reference sets `Config.User=dev`. Assert the gap's shape so
    // this test flips red when #274 is fixed, then allow `user` in the diff.
    assert_eq!(
        up_snap.user, "dev",
        "reference should set Config.User=dev from containerUser: {:?}",
        up_snap.user
    );
    assert!(
        d_snap.user.is_empty() || d_snap.user == "root",
        "deacon unexpectedly set Config.User ({:?}) — #274 may be FIXED; drop \
         the `user` allowance and this shape assertion",
        d_snap.user
    );
    assert_snapshots_parity(&d_snap, &up_snap, &["user"]);
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

#[test]
fn state_diff_appport_published_ports() {
    if !gated() {
        return;
    }

    // `appPort: [3000]` publishes to the FIXED host port 3000 on BOTH CLIs (per
    // spec), so the two containers cannot coexist — bring up upstream, snapshot,
    // TEAR IT DOWN to free host:3000, then bring up deacon. (This is not a
    // parity difference; it is the shared host-port resource.)
    let up_tmp = TempDir::new().unwrap();
    let up_ws = up_tmp.path();
    let _up_clean = WsCleanup(up_ws);
    write_appport_fixture(up_ws, "upstream");
    let (up_id, up_snap) = upstream_up_snapshot(up_ws);
    let _ = docker_out_allow_fail(&["rm", "-f", &up_id]);

    let d_tmp = TempDir::new().unwrap();
    let d_ws = d_tmp.path();
    let _d_clean = WsCleanup(d_ws);
    write_appport_fixture(d_ws, "deacon");
    let (_d_out, _d_id, d_snap) = deacon_up_snapshot(d_ws);
    deacon_down(d_ws);

    // Vacuity guard: the appPort actually published on at least one side, or the
    // test proves nothing about port parity.
    assert!(
        d_snap.published_ports.contains("3000/tcp") || up_snap.published_ports.contains("3000/tcp"),
        "neither CLI published appPort 3000/tcp (deacon={:?} upstream={:?}) — fixture broken?",
        d_snap.published_ports,
        up_snap.published_ports
    );

    assert_snapshots_parity(&d_snap, &up_snap, &[]);
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

#[test]
fn state_diff_mount_variety_readonly_and_tmpfs() {
    if !gated() {
        return;
    }

    let up_tmp = TempDir::new().unwrap();
    let up_ws = up_tmp.path();
    let _up_clean = WsCleanup(up_ws);
    write_mounts_variety_fixture(up_ws, "upstream");
    let (up_id, up_snap) = upstream_up_snapshot(up_ws);

    let d_tmp = TempDir::new().unwrap();
    let d_ws = d_tmp.path();
    let _d_clean = WsCleanup(d_ws);
    write_mounts_variety_fixture(d_ws, "deacon");
    let (_d_out, _d_id, d_snap) = deacon_up_snapshot(d_ws);

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

    assert_snapshots_parity(&d_snap, &up_snap, &[]);
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

#[test]
fn state_diff_compose_sidecar_and_named_volume() {
    if !gated() {
        return;
    }

    let up_tmp = TempDir::new().unwrap();
    let up_ws = up_tmp.path();
    let _up_clean = WsCleanup(up_ws);
    write_compose_volume_fixture(up_ws, "upstream");
    let (_up_id, up_snap) = upstream_up_snapshot(up_ws);

    let d_tmp = TempDir::new().unwrap();
    let d_ws = d_tmp.path();
    let _d_clean = WsCleanup(d_ws);
    write_compose_volume_fixture(d_ws, "deacon");
    let (_d_out, _d_id, d_snap) = deacon_up_snapshot(d_ws);

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

    assert_snapshots_parity(&d_snap, &up_snap, &[]);
}
