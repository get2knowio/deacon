//! Cross-SHAPE supersede: a workspace whose document changes between compose and
//! single-container (#551, follow-up to #371 / #550).
//!
//! #550 established the invariant: after any successful `up`, exactly one container for
//! the workspace is live. `stop_superseded_containers` holds it by sweeping deacon's own
//! labels and, for a superseded compose generation, expanding that project through
//! `com.docker.compose.project` — because compose's `depends_on` sidecars carry NONE of
//! deacon's labels and a label-only sweep leaves them running with the project network
//! still referenced.
//!
//! That expansion was gated on the CURRENT `up` being compose, so it never fired when the
//! document changed shape from compose to a plain `image`: the superseded project's
//! primary was stopped (it has deacon's labels) and its sidecars were stranded. This
//! binary observes that transition end to end.
//!
//! Why here and not on `case-up-stale-config-reentry`: that case's `workspaceContainers`
//! census is a `docker ps --filter label=devcontainer.local_folder=<ws>` count, and the
//! stranded sidecar carries no such label. The census is blind to the exact container this
//! is about, so the evidence has to come from a probe that can see the project.
//!
//! Docker-gated: skips cleanly when Docker is unavailable. Every container and network it
//! creates is torn down by an RAII guard, so a failing assertion still cleans up rather
//! than leaking toward address-pool exhaustion.

use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use tempfile::TempDir;

/// The container runtime binary under test (honors `DEACON_CONTAINER_RUNTIME`, the same
/// env var deacon reads), so probes read the store deacon-under-podman actually writes.
fn runtime_bin() -> String {
    std::env::var("DEACON_CONTAINER_RUNTIME").unwrap_or_else(|_| "docker".to_string())
}

fn is_docker_available() -> bool {
    StdCommand::new(runtime_bin())
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn runtime(args: &[&str]) -> String {
    let out = StdCommand::new(runtime_bin())
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn {} {:?}: {e}", runtime_bin(), args));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn write(ws: &Path, rel: &str, body: &str) {
    let path = ws.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// `deacon up --workspace-folder <ws>`, returning its result JSON. `up` writes the result
/// document to stdout and everything else to stderr, so stdout parses on its own.
fn up_json(ws: &Path) -> serde_json::Value {
    let out = StdCommand::new(env!("CARGO_BIN_EXE_deacon"))
        .args(["up", "--workspace-folder"])
        .arg(ws)
        .stderr(Stdio::inherit())
        .output()
        .expect("spawn deacon up");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "`deacon up` failed for {}: {stdout}",
        ws.display()
    );
    serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("`up` stdout is not JSON ({e}): {stdout}"))
}

/// Ids of the containers compose created for `project`, in whatever state.
/// `com.docker.compose.project` is the one label compose stamps on EVERY container it
/// creates, which is precisely why the sweep has to use it.
fn project_containers(project: &str, running_only: bool) -> Vec<String> {
    let filter = format!("label=com.docker.compose.project={project}");
    let mut args: Vec<&str> = vec!["ps"];
    if !running_only {
        args.push("-a");
    }
    args.extend(["-q", "--filter", &filter]);
    runtime(&args)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn container_state(id: &str) -> String {
    runtime(&["inspect", id, "--format", "{{.State.Status}}"])
}

fn label(id: &str, key: &str) -> String {
    runtime(&[
        "inspect",
        id,
        "--format",
        &format!("{{{{index .Config.Labels \"{key}\"}}}}"),
    ])
}

/// Removes exactly what the test created — the compose project's containers and network,
/// plus the single container the shape change produced. Scoped to recorded names; never a
/// global prune (a sweep of daemon-global state races sibling cases).
#[derive(Default)]
struct Cleanup {
    project: Option<String>,
    containers: Vec<String>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        let mut ids = self.containers.clone();
        if let Some(p) = &self.project {
            ids.extend(project_containers(p, false));
        }
        for id in ids {
            let _ = StdCommand::new(runtime_bin())
                .args(["rm", "-f", &id])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        if let Some(p) = &self.project {
            // Compose names the default network `<project>_default`; removing it is what
            // keeps a cancelled run from accumulating toward address-pool exhaustion.
            let _ = StdCommand::new(runtime_bin())
                .args(["network", "rm", &format!("{p}_default")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

const COMPOSE_YML: &str = "services:\n  \
    app:\n    \
      image: alpine:3.18\n    \
      command: sleep infinity\n    \
      depends_on:\n      \
        - db\n  \
    db:\n    \
      image: alpine:3.18\n    \
      command: sleep infinity\n";

const COMPOSE_CONFIG: &str = r#"{
  "name": "SupersedeShapeCompose",
  "dockerComposeFile": "docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace"
}"#;

const SINGLE_CONFIG: &str = r#"{
  "name": "SupersedeShapeSingle",
  "image": "alpine:3.18",
  "overrideCommand": true
}"#;

/// #551. Compose first, then the same workspace re-`up`ped with a plain `image` document.
/// The superseded generation is a whole PROJECT, and it must go down whole — the primary
/// (deacon-labelled) and the `depends_on` sidecar (labelled by compose only) alike.
/// Stopped, not removed: the 2026-08-07 ruling keeps a superseded generation recoverable.
#[test]
fn compose_project_superseded_by_a_single_container_up_goes_down_whole() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let ws = TempDir::new().unwrap();
    let mut cleanup = Cleanup::default();

    write(ws.path(), ".devcontainer/docker-compose.yml", COMPOSE_YML);
    write(ws.path(), ".devcontainer/devcontainer.json", COMPOSE_CONFIG);

    let first = up_json(ws.path());
    let project = first["composeProjectName"]
        .as_str()
        .expect("compose `up` reports its project name")
        .to_string();
    cleanup.project = Some(project.clone());

    let before = project_containers(&project, true);
    assert_eq!(
        before.len(),
        2,
        "the compose generation is a primary plus its `depends_on` sidecar: {before:?}"
    );

    // The load-bearing fact behind the whole project expansion: at least one member of the
    // project carries NONE of deacon's labels, so a sweep that only matched deacon's own
    // labels could never reach it.
    assert!(
        before
            .iter()
            .any(|id| label(id, "devcontainer.local_folder").is_empty()
                && label(id, "devcontainer.source").is_empty()),
        "expected an unlabelled compose sidecar among {before:?}"
    );

    // The shape change: same workspace, same config PATH, a document that is no longer
    // compose. deacon's identity includes a configHash, so this provisions a new container
    // rather than reattaching, and the whole compose project it replaces is superseded.
    write(ws.path(), ".devcontainer/devcontainer.json", SINGLE_CONFIG);
    let second = up_json(ws.path());
    let single_id = second["containerId"]
        .as_str()
        .expect("single-container `up` reports its container id")
        .to_string();
    cleanup.containers.push(single_id.clone());
    assert!(
        second["composeProjectName"].is_null(),
        "the second `up` must NOT be a compose run: {second}"
    );

    let still_running = project_containers(&project, true);
    assert!(
        still_running.is_empty(),
        "#551: every container of the superseded project must be stopped — the sidecars \
         carry no deacon labels, so leaving them up strands them AND keeps the project \
         network referenced. Still running: {:?}",
        still_running
            .iter()
            .map(|id| (id.clone(), container_state(id)))
            .collect::<Vec<_>>()
    );

    // Stopped, NOT removed (maintainer ruling 2026-08-07): the generation's state and
    // volumes survive so it stays recoverable. `is_empty()` above would also pass if the
    // sweep had deleted the project, which is the behaviour the ruling rejected.
    let survivors = project_containers(&project, false);
    assert_eq!(
        survivors.len(),
        2,
        "superseded containers are stopped, not removed: {survivors:?}"
    );

    assert_eq!(
        container_state(&single_id),
        "running",
        "the container this `up` settled on must be spared"
    );
}

/// The other direction, which already worked and must keep working: a plain `image`
/// generation superseded by a compose one. The widened inspect must not turn a
/// project-less superseded container into something the sweep skips, and the project this
/// `up` just brought up — sidecar included — must be spared in full.
#[test]
fn single_container_superseded_by_a_compose_up_is_stopped_and_the_new_project_spared() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let ws = TempDir::new().unwrap();
    let mut cleanup = Cleanup::default();

    write(ws.path(), ".devcontainer/docker-compose.yml", COMPOSE_YML);
    write(ws.path(), ".devcontainer/devcontainer.json", SINGLE_CONFIG);

    let first = up_json(ws.path());
    let single_id = first["containerId"]
        .as_str()
        .expect("single-container `up` reports its container id")
        .to_string();
    cleanup.containers.push(single_id.clone());
    assert_eq!(container_state(&single_id), "running");

    write(ws.path(), ".devcontainer/devcontainer.json", COMPOSE_CONFIG);
    let second = up_json(ws.path());
    let project = second["composeProjectName"]
        .as_str()
        .expect("compose `up` reports its project name")
        .to_string();
    cleanup.project = Some(project.clone());

    assert_eq!(
        container_state(&single_id),
        "exited",
        "the superseded single-container generation must be stopped"
    );

    let live = project_containers(&project, true);
    assert_eq!(
        live.len(),
        2,
        "nothing in the project this `up` just brought up may be swept — not the primary, \
         and not the sidecar the sweep now inspects on every path: {live:?}"
    );
}
