//! Docker-backed conformance-runner tests (022-conformance-runner US5, T049/T050/T051).
//!
//! These run REAL Docker (the `docker-shared` nextest group — unique resource names, so
//! concurrent execution is safe). They assert, via actual `docker` sweeps, that the four
//! Docker channel observers produce genuine evidence against a real container, that the
//! RAII cleanup guard leaves ZERO residual resources on success AND on unwind, and that
//! two concurrent cases allocate non-colliding names. Docker-only and hence
//! `#[cfg(unix)]`-guarded (the harness's Docker lanes are Linux/macOS).

#![cfg(unix)]

use std::process::Command;

use deacon_conformance::model::{
    CHAN_IMAGE, CHAN_INJECTED_PROCESS, CHAN_PROCESS_GRAPH, CHAN_TEMPORAL, Operation,
};
use parity_harness::normalize::{TokenMap, normalize_channel};
use parity_harness::observe::container_graph::ContainerGraphObserver;
use parity_harness::observe::image::ImageObserver;
use parity_harness::observe::injected_process::InjectedProcessObserver;
use parity_harness::observe::temporal::TemporalObserver;
use parity_harness::observe::{ChannelObserver, RunContext};
use parity_harness::workspace::DockerWorkspace;

const IMAGE: &str = "alpine:3.19";

/// Run `docker <args>` and return trimmed stdout, panicking on failure (fail-loud — these
/// tests only run in Docker lanes).
fn docker(args: &[&str]) -> String {
    let out = Command::new("docker")
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("could not run docker {args:?}: {e}"));
    assert!(
        out.status.success(),
        "docker {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// `docker` allowing failure (for best-effort teardown / existence probes).
fn docker_try(args: &[&str]) -> (bool, String) {
    match Command::new("docker").args(args).output() {
        Ok(out) => (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).trim().to_string(),
        ),
        Err(_) => (false, String::new()),
    }
}

/// Container ids labeled `devcontainer.local_folder=<ws>` (the cleanup sweep predicate).
fn containers_for(ws: &str) -> Vec<String> {
    let (ok, out) = docker_try(&[
        "ps",
        "-aq",
        "--filter",
        &format!("label=devcontainer.local_folder={ws}"),
    ]);
    if !ok {
        return Vec::new();
    }
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

fn ctx_for(container_id: &str) -> RunContext {
    let mut ctx = RunContext::new(std::path::PathBuf::from("/tmp"));
    ctx.container_id = Some(container_id.to_string());
    ctx
}

fn up_op() -> Operation {
    Operation {
        id: "op-up".to_string(),
        subcommand: "up".to_string(),
        ..Operation::default()
    }
}

// ---------------------------------------------------------------------------------
// T051: per-channel capture against a real container.
// ---------------------------------------------------------------------------------

#[test]
fn observers_capture_real_container_evidence() {
    // A DockerWorkspace guard makes the test LEAK-PROOF: even if an assertion below
    // panics, its Drop sweeps the labeled container + tracked network/volume.
    let mut ws = DockerWorkspace::new(None).expect("workspace");
    let ws_label = ws.path().to_string_lossy().into_owned();
    let network = ws.resource_name("net");
    let volume = ws.resource_name("vol");
    ws.track_network(&network);
    ws.track_volume(&volume);

    let _ = docker_try(&["network", "create", &network]);
    let _ = docker_try(&["volume", "create", &volume]);
    let id = docker(&[
        "run",
        "-d",
        "--label",
        &format!("devcontainer.local_folder={ws_label}"),
        "--label",
        "org.opencontainers.image.title=Probe",
        "--network",
        &network,
        "-v",
        &format!("{volume}:/data"),
        "-e",
        "FOO=bar",
        "-w",
        "/work",
        IMAGE,
        "sleep",
        "120",
    ]);

    let ctx = ctx_for(&id);
    let tokens = TokenMap::new();
    let op = up_op();

    // chan-image: labels + env, normalized (label_semantic keeps labels an object).
    let img_raw = ImageObserver.capture(&ctx, &op).expect("image capture");
    assert!(img_raw.present);
    let img = normalize_channel(CHAN_IMAGE, &img_raw, &tokens);
    assert_eq!(
        img.value["labels"]["org.opencontainers.image.title"], "Probe",
        "image labels captured + parsed semantically: {img:?}"
    );
    assert!(
        img.value["env"]
            .as_array()
            .is_some_and(|a| a.iter().any(|e| e == "FOO=bar")),
        "image env captured (nothing blanket-removed)"
    );

    // chan-process-graph: the mount graph + networks + volume.
    let graph_raw = ContainerGraphObserver
        .capture(&ctx, &op)
        .expect("graph capture");
    let graph = normalize_channel(CHAN_PROCESS_GRAPH, &graph_raw, &tokens);
    assert!(
        graph.value["mounts"]
            .as_array()
            .is_some_and(|a| a.iter().any(|m| m["target"] == "/data")),
        "the volume mount at /data is captured: {graph:?}"
    );
    assert!(
        graph.value["networks"]
            .as_array()
            .is_some_and(|a| a.iter().any(|n| n == &serde_json::json!(network))),
        "the custom network is captured"
    );
    assert!(
        graph.value["volumes"]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v == &serde_json::json!(volume))),
        "the named volume is captured"
    );

    // chan-injected-process: env/user/cwd/PATH (segmented)/tty.
    let inj_raw = InjectedProcessObserver
        .capture(&ctx, &op)
        .expect("injected capture");
    let inj = normalize_channel(CHAN_INJECTED_PROCESS, &inj_raw, &tokens);
    assert_eq!(inj.value["env"]["FOO"], "bar", "injected env captured");
    assert_eq!(inj.value["cwd"], "/work", "cwd captured");
    assert_eq!(inj.value["tty"], false, "tty captured");
    assert!(
        inj.value["path"].as_array().is_some_and(|a| !a.is_empty()),
        "PATH captured segment-wise (path_env_segmented): {inj:?}"
    );

    // chan-temporal: state markers (no wall-clock timestamps).
    let temp_raw = TemporalObserver
        .capture(&ctx, &op)
        .expect("temporal capture");
    let temp = normalize_channel(CHAN_TEMPORAL, &temp_raw, &tokens);
    assert_eq!(temp.value["status"], "running");
    assert_eq!(temp.value["running"], true);
    assert!(
        temp.value.get("startedAt").is_none() && temp.value.get("StartedAt").is_none(),
        "no wall-clock timestamp in temporal evidence (determinism)"
    );

    // `ws` drops here → sweeps the container + tracked network/volume (leak-proof).
    drop(ws);
}

#[test]
fn temporal_state_and_cleanup_transitions() {
    // Leak-proof: label the container with the workspace so the guard sweeps it even on
    // an assertion panic.
    let ws = DockerWorkspace::new(None).expect("workspace");
    let ws_label = ws.path().to_string_lossy().into_owned();
    let id = docker(&[
        "run",
        "-d",
        "--label",
        &format!("devcontainer.local_folder={ws_label}"),
        IMAGE,
        "sleep",
        "120",
    ]);
    let op = up_op();

    // Running → status=running.
    let running = TemporalObserver.capture(&ctx_for(&id), &op).unwrap();
    assert_eq!(running.value["status"], "running");
    assert_eq!(running.value["running"], true);

    // Stopped → status=exited (a real state transition, deterministic).
    docker(&["stop", "-t", "1", &id]);
    let stopped = TemporalObserver.capture(&ctx_for(&id), &op).unwrap();
    assert_eq!(
        stopped.value["status"], "exited",
        "stop → exited: {stopped:?}"
    );
    assert_eq!(stopped.value["running"], false);

    // Cleanup transition: a removed container is `present:false` (FR-018).
    docker(&["rm", "-f", &id]);
    let gone = TemporalObserver.capture(&ctx_for(&id), &op).unwrap();
    assert!(
        !gone.present,
        "a removed container is not-captured (cleanup transition)"
    );
}

// ---------------------------------------------------------------------------------
// T049: guaranteed cleanup — zero residual resources on success AND unwind.
// ---------------------------------------------------------------------------------

/// Start a container labeled with the workspace so the guard's sweep can find it.
fn start_labeled_container(ws: &str, name: &str) -> String {
    docker(&[
        "run",
        "-d",
        "--name",
        name,
        "--label",
        &format!("devcontainer.local_folder={ws}"),
        IMAGE,
        "sleep",
        "120",
    ])
}

#[test]
fn cleanup_leaves_zero_residual_on_success() {
    let mut ws = DockerWorkspace::new(None).expect("workspace");
    let ws_path = ws.path().to_string_lossy().into_owned();
    let name = format!("{}-c", ws.run_id());
    let _id = start_labeled_container(&ws_path, &name);
    assert_eq!(containers_for(&ws_path).len(), 1, "container is up");

    ws.cleanup_now();
    assert!(
        containers_for(&ws_path).is_empty(),
        "cleanup_now leaves zero residual containers (SC-009)"
    );
    // The temp workspace directory is removed when the guard drops.
    drop(ws);
}

#[test]
fn cleanup_runs_on_unwind_drop() {
    // Simulate a forced-failure / early-return: the guard reclaims on Drop, not just on
    // an explicit call. A scope exit (as an unwind would) must reclaim.
    let ws_path;
    let name;
    {
        let ws = DockerWorkspace::new(None).expect("workspace");
        ws_path = ws.path().to_string_lossy().into_owned();
        name = format!("{}-c", ws.run_id());
        let _id = start_labeled_container(&ws_path, &name);
        assert_eq!(containers_for(&ws_path).len(), 1, "container is up");
        // No explicit cleanup — the guard drops here (the unwind path).
    }
    assert!(
        containers_for(&ws_path).is_empty(),
        "Drop reclaims the container on unwind/early-return (SC-009)"
    );
    // Belt-and-suspenders teardown in case of assertion failure above.
    let _ = docker_try(&["rm", "-f", &name]);
}

// ---------------------------------------------------------------------------------
// T050: two concurrent Docker cases allocate non-colliding names.
// ---------------------------------------------------------------------------------

#[test]
fn concurrent_cases_do_not_collide_on_names() {
    let mut a = DockerWorkspace::new(None).expect("ws a");
    let mut b = DockerWorkspace::new(None).expect("ws b");
    assert_ne!(a.run_id(), b.run_id());
    assert_ne!(a.path(), b.path());

    let a_ws = a.path().to_string_lossy().into_owned();
    let b_ws = b.path().to_string_lossy().into_owned();
    let a_name = a.resource_name("c");
    let b_name = b.resource_name("c");
    assert_ne!(a_name, b_name, "resource names must be distinct");

    // Both run concurrently, each with its own labeled container.
    let _a_id = start_labeled_container(&a_ws, &a_name);
    let _b_id = start_labeled_container(&b_ws, &b_name);
    assert_eq!(containers_for(&a_ws).len(), 1);
    assert_eq!(containers_for(&b_ws).len(), 1);

    // Cleaning up A removes only A's container; B is untouched (scoped cleanup).
    a.cleanup_now();
    assert!(containers_for(&a_ws).is_empty(), "A cleaned");
    assert_eq!(containers_for(&b_ws).len(), 1, "B untouched by A's cleanup");

    b.cleanup_now();
    assert!(containers_for(&b_ws).is_empty(), "B cleaned");
}
