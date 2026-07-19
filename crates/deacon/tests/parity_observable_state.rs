//! #267: observable-state parity suite between deacon and the pinned
//! reference `@devcontainers/cli` oracle.
//!
//! Locks in the interop fixes from #264 (lockfile manifest digest), #266
//! (compose `mounts` applied), and #265 (compose project name isolation), and
//! adds broader observable-state coverage (rendered compose state, labels,
//! merged-config-vs-runtime drift, cross-CLI handoff) so this class of
//! regression is caught going forward.
//!
//! Runs ONLY under `cargo nextest run --profile parity`. There is no opt-in env
//! gate and no silent skip: a missing/mismatched oracle or an unavailable Docker
//! FAILS the test with a cause-specific message (018-harden-parity-harness,
//! FR-002, FR-004..FR-006). Both CLIs' raw output is preserved under
//! `target/parity/raw/` and a run-report fragment is written to
//! `target/parity/report/parity_observable_state.json`. Lives in the `parity`
//! nextest group (docker-exclusive-adjacent, serialized) via the
//! `binary(#parity_*)` glob in `.config/nextest.toml`.

use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

use parity_harness::HarnessError;
use parity_harness::exec::{ExecKind, Invocation, exec_deacon, exec_oracle};
use parity_harness::oracle::Oracle;
use parity_harness::prereq::require_docker;
use parity_harness::report::{CaseResult, OracleInfo, RawPaths, ReportFragment, now_rfc3339};

/// This binary's name — the fragment key and raw-artifact subdirectory.
const BINARY: &str = "parity_observable_state";

/// Fail the test with the error's cause-specific `Display` message (never the
/// `Debug` form) so an oracle/prereq/normalization failure reads as its remedy.
fn ff<T>(r: Result<T, HarnessError>) -> T {
    r.unwrap_or_else(|e| panic!("{e}"))
}

/// The four preserved raw-output paths (report-relative) for one compared case
/// that exercised both CLIs.
fn raw_paths(deacon: &Invocation, oracle: &Invocation) -> RawPaths {
    RawPaths {
        deacon_stdout: deacon.stdout_rel.display().to_string(),
        deacon_stderr: deacon.stderr_rel.display().to_string(),
        oracle_stdout: oracle.stdout_rel.display().to_string(),
        oracle_stderr: oracle.stderr_rel.display().to_string(),
    }
}

/// Raw-output paths for a case that only invoked deacon; the oracle slots are
/// left empty (no compared oracle invocation for this case).
fn raw_paths_deacon_only(deacon: &Invocation) -> RawPaths {
    RawPaths {
        deacon_stdout: deacon.stdout_rel.display().to_string(),
        deacon_stderr: deacon.stderr_rel.display().to_string(),
        oracle_stdout: String::new(),
        oracle_stderr: String::new(),
    }
}

fn docker_out(args: &[&str]) -> String {
    let out = std::process::Command::new("docker")
        .args(args)
        .output()
        .expect("docker should run");
    assert!(
        out.status.success(),
        "docker {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

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

fn inspect_json(container_id: &str) -> Value {
    let raw = docker_out(&["inspect", container_id]);
    let arr: Vec<Value> = serde_json::from_str(&raw).expect("docker inspect returns a JSON array");
    arr.into_iter()
        .next()
        .expect("docker inspect returns at least one entry")
}

fn find_mount<'a>(inspect: &'a Value, target: &str) -> Option<&'a Value> {
    inspect["Mounts"]
        .as_array()?
        .iter()
        .find(|m| m["Destination"].as_str() == Some(target))
}

/// Extract a top-level string field from a `deacon up`/`deacon build` JSON
/// result. Tolerant of leading log lines via `rfind('{')`.
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

/// Best-effort teardown of a deacon-owned workspace. Spawns the deacon binary
/// under test directly — teardown is NOT a compared invocation, so it never
/// goes through the harness exec path.
fn deacon_down(ws: &Path) {
    let _ = std::process::Command::new(Path::new(env!("CARGO_BIN_EXE_deacon")))
        .arg("down")
        .arg("--workspace-folder")
        .arg(ws)
        .arg("--remove")
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

// ===========================================================================
// Area 1: Lockfile interop — deacon-generated lockfile is consumable by the
// reference CLI's `features resolve-dependencies`. Locks in #264: the
// lockfile must carry the OCI *manifest* digest, not the layer digest.
// ===========================================================================

#[tokio::test]
async fn parity_lockfile_manifest_digest_resolves_dependencies() {
    const CASE: &str = "lockfile-manifest-digest";
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));
    let started = now_rfc3339();

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();
    fs::create_dir(ws.join(".devcontainer")).unwrap();
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        r#"{
  "name": "ParityLockfile",
  "image": "debian:bookworm-slim",
  "features": {
    "ghcr.io/devcontainers/features/common-utils:2": {
      "installZsh": false,
      "installOhMyZsh": false,
      "upgradePackages": false
    }
  }
}
"#,
    )
    .unwrap();

    let ws_str = ws.to_string_lossy().into_owned();

    // deacon: regenerate the lockfile from the resolved feature set.
    let upgrade = ff(exec_deacon(
        BINARY,
        &format!("{CASE}-upgrade-deacon"),
        ExecKind::Lifecycle,
        deacon_bin,
        &["upgrade", "--workspace-folder", &ws_str],
        ws,
    )
    .await);
    assert!(
        upgrade.success,
        "deacon upgrade failed: {}",
        String::from_utf8_lossy(&upgrade.stderr)
    );

    let lockfile_path = ws.join(".devcontainer/devcontainer-lock.json");
    assert!(
        lockfile_path.exists(),
        "deacon upgrade should write a devcontainer-lock.json"
    );
    let lockfile: Value =
        serde_json::from_str(&fs::read_to_string(&lockfile_path).unwrap()).unwrap();
    let entry = lockfile["features"]["ghcr.io/devcontainers/features/common-utils:2"].clone();
    let resolved = entry["resolved"]
        .as_str()
        .expect("lockfile entry should have a resolved field");
    let integrity = entry["integrity"]
        .as_str()
        .expect("lockfile entry should have an integrity field");

    // #264 guard: the digest is the MANIFEST digest, not the layer digest —
    // cross-check against `docker manifest inspect`'s reported digest, and
    // confirm it does NOT match any layer digest in that manifest.
    let manifest_json = docker_out(&[
        "manifest",
        "inspect",
        "ghcr.io/devcontainers/features/common-utils:2",
    ]);
    let manifest: Value = serde_json::from_str(&manifest_json).unwrap_or(Value::Null);
    let mut checked_a_layer = false;
    if let Some(layers) = manifest["layers"].as_array() {
        for layer in layers {
            if let Some(layer_digest) = layer["digest"].as_str() {
                checked_a_layer = true;
                assert_ne!(
                    integrity, layer_digest,
                    "lockfile integrity must be the manifest digest, not a layer digest"
                );
            }
        }
    }
    // Guard against the check above passing vacuously: if `docker manifest
    // inspect` ever returns a shape without layer digests (e.g. a multi-arch
    // index under `manifests`), the layer-vs-manifest-digest invariant would
    // silently hold. Fail loudly so the test is updated rather than trusted.
    assert!(
        checked_a_layer,
        "manifest inspect returned no layer digests to cross-check #264 against; manifest was: {}",
        manifest_json
    );
    assert!(
        integrity.starts_with("sha256:"),
        "integrity should be sha256:-prefixed, got {}",
        integrity
    );
    assert!(
        resolved.ends_with(integrity),
        "resolved '{}' should end with the integrity digest '{}'",
        resolved,
        integrity
    );

    // The real interop check: the reference CLI's dependency resolver must
    // accept deacon's lockfile as-is.
    let resolve = ff(exec_oracle(
        BINARY,
        &format!("{CASE}-resolve-oracle"),
        ExecKind::Lifecycle,
        &oracle.path,
        &[
            "features",
            "resolve-dependencies",
            "--workspace-folder",
            &ws_str,
        ],
        ws,
    )
    .await);
    assert!(
        resolve.success,
        "devcontainer features resolve-dependencies rejected deacon's lockfile: {}",
        String::from_utf8_lossy(&resolve.stderr)
    );

    let finished = now_rfc3339();
    let raw = raw_paths(&upgrade, &resolve);
    ff(ReportFragment::new(
        BINARY,
        OracleInfo::from(&oracle),
        started,
        finished,
        vec![CaseResult::pass(CASE, raw)],
        Vec::new(),
    )
    .write()
    .await);
}

// ===========================================================================
// Area 2: Compose config mounts — `devcontainer.json` `mounts` applied by
// BOTH CLIs on the compose path. Locks in #266.
// ===========================================================================

#[tokio::test]
async fn parity_compose_config_mounts_applied_both_clis() {
    const CASE: &str = "compose-config-mounts";
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));
    let started = now_rfc3339();

    let mut deacon_up: Option<Invocation> = None;
    let mut upstream_up: Option<Invocation> = None;

    // Two independent workspaces (one per CLI) so their compose projects never collide.
    for (label, is_upstream) in [("upstream", true), ("deacon", false)] {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();
        let _cleanup = WsCleanup(ws);
        let sib = ws.join("sib");
        fs::create_dir_all(&sib).unwrap();
        fs::write(sib.join("marker.txt"), "from-sib").unwrap();

        fs::write(
            ws.join("docker-compose.yml"),
            "services:\n  app:\n    image: debian:bookworm-slim\n    command: [\"sleep\", \"infinity\"]\n",
        )
        .unwrap();
        fs::create_dir(ws.join(".devcontainer")).unwrap();
        fs::write(
            ws.join(".devcontainer/devcontainer.json"),
            format!(
                r#"{{
  "name": "ParityComposeMounts-{label}",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace",
  "mounts": [
    "source=${{localWorkspaceFolder}}/sib,target=/workspaces/sib,type=bind"
  ]
}}
"#
            ),
        )
        .unwrap();

        let ws_str = ws.to_string_lossy().into_owned();
        let up = if is_upstream {
            ff(exec_oracle(
                BINARY,
                &format!("{CASE}-up-upstream"),
                ExecKind::Lifecycle,
                &oracle.path,
                &["up", "--workspace-folder", &ws_str],
                ws,
            )
            .await)
        } else {
            ff(exec_deacon(
                BINARY,
                &format!("{CASE}-up-deacon"),
                ExecKind::Lifecycle,
                deacon_bin,
                &["up", "--workspace-folder", &ws_str],
                ws,
            )
            .await)
        };
        assert!(
            up.success,
            "{} up failed: {}",
            label,
            String::from_utf8_lossy(&up.stderr)
        );

        let container_id = if is_upstream {
            upstream_container_id(ws).expect("no upstream container found")
        } else {
            json_field(&up.stdout_string(), "containerId")
                .expect("deacon up should report a containerId")
        };

        let inspect = inspect_json(&container_id);
        let mount = find_mount(&inspect, "/workspaces/sib");

        if is_upstream {
            let (_, _, _) = docker_out_allow_fail(&[
                "compose",
                "-f",
                ws.join("docker-compose.yml").to_str().unwrap(),
                "--project-directory",
                ws.to_str().unwrap(),
                "down",
                "--remove-orphans",
                "-v",
            ]);
        } else {
            let project_name = json_field(&up.stdout_string(), "composeProjectName")
                .unwrap_or_else(|| "unknown".to_string());
            deacon_compose_down_by_project(&project_name);
        }

        let mount =
            mount.unwrap_or_else(|| panic!("{} config mount at /workspaces/sib missing", label));
        assert_eq!(mount["Type"].as_str(), Some("bind"));
        let source = mount["Source"].as_str().unwrap_or_default();
        assert!(
            source.ends_with("/sib"),
            "{} config mount source '{}' should resolve ${{localWorkspaceFolder}}/sib",
            label,
            source
        );

        if is_upstream {
            upstream_up = Some(up);
        } else {
            deacon_up = Some(up);
        }
    }

    let deacon_up = deacon_up.expect("deacon up captured");
    let upstream_up = upstream_up.expect("upstream up captured");
    let finished = now_rfc3339();
    let raw = raw_paths(&deacon_up, &upstream_up);
    ff(ReportFragment::new(
        BINARY,
        OracleInfo::from(&oracle),
        started,
        finished,
        vec![CaseResult::pass(CASE, raw)],
        Vec::new(),
    )
    .write()
    .await);
}

// ===========================================================================
// Area 5 (checked alongside 4/6 below): compose project name isolation.
// Locks in #265.
// ===========================================================================

#[tokio::test]
async fn parity_compose_project_name_isolated_from_reference() {
    const CASE: &str = "compose-project-name-isolated";
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));
    let started = now_rfc3339();

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();
    let _cleanup = WsCleanup(ws);
    fs::write(
        ws.join("docker-compose.yml"),
        "services:\n  app:\n    image: debian:bookworm-slim\n    command: [\"sleep\", \"infinity\"]\n",
    )
    .unwrap();
    fs::create_dir(ws.join(".devcontainer")).unwrap();
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        r#"{
  "name": "ParityProjectIsolation",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace"
}
"#,
    )
    .unwrap();

    let ws_str = ws.to_string_lossy().into_owned();
    let up = ff(exec_deacon(
        BINARY,
        &format!("{CASE}-up-deacon"),
        ExecKind::Lifecycle,
        deacon_bin,
        &["up", "--workspace-folder", &ws_str],
        ws,
    )
    .await);
    assert!(
        up.success,
        "deacon up failed: {}",
        String::from_utf8_lossy(&up.stderr)
    );
    let project_name = json_field(&up.stdout_string(), "composeProjectName")
        .expect("deacon up should report composeProjectName");

    deacon_compose_down_by_project(&project_name);

    // #265: deacon's project name must be namespaced (`deacon_*`), NOT the
    // reference CLI's own `<folder>_devcontainer` convention, so `devcontainer
    // up` never discovers (and then mismanages) a deacon-owned project.
    assert!(
        project_name.starts_with("deacon_"),
        "expected deacon-namespaced project name, got '{}'",
        project_name
    );
    let reference_form = format!(
        "{}_devcontainer",
        ws.file_name().unwrap().to_string_lossy().to_lowercase()
    );
    assert_ne!(
        project_name, reference_form,
        "deacon's project name must not collide with the reference CLI's own default"
    );

    let finished = now_rfc3339();
    let raw = raw_paths_deacon_only(&up);
    ff(ReportFragment::new(
        BINARY,
        OracleInfo::from(&oracle),
        started,
        finished,
        vec![CaseResult::pass(CASE, raw)],
        Vec::new(),
    )
    .write()
    .await);
}

// ===========================================================================
// Area 4: container & image labels — deacon's isolation contract observed
// via `docker inspect`.
// ===========================================================================

#[tokio::test]
async fn parity_container_and_image_labels_isolated() {
    const CASE: &str = "container-and-image-labels";
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));
    let started = now_rfc3339();

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();
    let _cleanup = WsCleanup(ws);
    fs::create_dir(ws.join(".devcontainer")).unwrap();
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        r#"{
  "name": "ParityLabels",
  "image": "debian:bookworm-slim"
}
"#,
    )
    .unwrap();

    let ws_str = ws.to_string_lossy().into_owned();
    let up = ff(exec_deacon(
        BINARY,
        &format!("{CASE}-up-deacon"),
        ExecKind::Lifecycle,
        deacon_bin,
        &["up", "--workspace-folder", &ws_str],
        ws,
    )
    .await);
    assert!(
        up.success,
        "deacon up failed: {}",
        String::from_utf8_lossy(&up.stderr)
    );
    let container_id = json_field(&up.stdout_string(), "containerId")
        .expect("deacon up should report containerId");
    let inspect = inspect_json(&container_id);
    let labels = inspect["Config"]["Labels"]
        .as_object()
        .expect("container should have labels");

    deacon_down(ws);

    assert_eq!(
        labels.get("devcontainer.source").and_then(|v| v.as_str()),
        Some("deacon"),
        "deacon must stamp devcontainer.source=deacon"
    );
    assert!(
        labels.contains_key("devcontainer.local_folder"),
        "devcontainer.local_folder label missing (reference-compatible discovery label)"
    );
    assert!(
        labels.contains_key("devcontainer.config_file"),
        "devcontainer.config_file label missing (reference-compatible discovery label)"
    );
    assert!(
        labels.contains_key("devcontainer.workspaceHash"),
        "devcontainer.workspaceHash label missing"
    );

    let finished = now_rfc3339();
    let raw = raw_paths_deacon_only(&up);
    ff(ReportFragment::new(
        BINARY,
        OracleInfo::from(&oracle),
        started,
        finished,
        vec![CaseResult::pass(CASE, raw)],
        Vec::new(),
    )
    .write()
    .await);
}

// ===========================================================================
// Area 3: rendered compose state — normalized compare of the primary
// service's image/volumes/env/labels between the two CLIs on equivalent
// input.
// ===========================================================================

#[tokio::test]
async fn parity_rendered_compose_state_comparable() {
    const CASE: &str = "rendered-compose-state";
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));
    let started = now_rfc3339();

    async fn bring_up(
        is_upstream: bool,
        oracle_path: &Path,
        deacon_bin: &Path,
        case: &str,
    ) -> (TempDir, Value, String, Invocation) {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();
        fs::write(
            ws.join("docker-compose.yml"),
            "services:\n  app:\n    image: debian:bookworm-slim\n    command: [\"sleep\", \"infinity\"]\n    environment:\n      - FOO=bar\n",
        )
        .unwrap();
        fs::create_dir(ws.join(".devcontainer")).unwrap();
        fs::write(
            ws.join(".devcontainer/devcontainer.json"),
            r#"{
  "name": "ParityRenderedState",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace",
  "containerEnv": { "BAZ": "qux" }
}
"#,
        )
        .unwrap();

        let ws_str = ws.to_string_lossy().into_owned();
        let up = if is_upstream {
            ff(exec_oracle(
                BINARY,
                &format!("{case}-up-upstream"),
                ExecKind::Lifecycle,
                oracle_path,
                &["up", "--workspace-folder", &ws_str],
                ws,
            )
            .await)
        } else {
            ff(exec_deacon(
                BINARY,
                &format!("{case}-up-deacon"),
                ExecKind::Lifecycle,
                deacon_bin,
                &["up", "--workspace-folder", &ws_str],
                ws,
            )
            .await)
        };
        assert!(
            up.success,
            "up failed (upstream={}): {}",
            is_upstream,
            String::from_utf8_lossy(&up.stderr)
        );

        let container_id = if is_upstream {
            upstream_container_id(ws).expect("no upstream container found")
        } else {
            json_field(&up.stdout_string(), "containerId")
                .expect("deacon up should report containerId")
        };
        let inspect = inspect_json(&container_id);
        (tmp, inspect, container_id, up)
    }

    let (upstream_tmp, upstream_inspect, upstream_id, upstream_up) =
        bring_up(true, &oracle.path, deacon_bin, CASE).await;
    // Guard the upstream container immediately so that a panic in the SECOND
    // bring_up (or any later assertion) still tears it down — it stays live
    // across the deacon setup below.
    let _upstream_cleanup = WsCleanup(upstream_tmp.path());
    let (deacon_tmp, deacon_inspect, deacon_id, deacon_up) =
        bring_up(false, &oracle.path, deacon_bin, CASE).await;
    let _deacon_cleanup = WsCleanup(deacon_tmp.path());

    // Tear down BEFORE asserting so a failed assertion never leaks containers.
    let _ = docker_out_allow_fail(&[
        "compose",
        "-f",
        upstream_tmp
            .path()
            .join("docker-compose.yml")
            .to_str()
            .unwrap(),
        "--project-directory",
        upstream_tmp.path().to_str().unwrap(),
        "down",
        "--remove-orphans",
        "-v",
    ]);
    deacon_down(deacon_tmp.path());
    let _ = (upstream_id, deacon_id);

    // Compare: base image, and both env vars present regardless of source
    // (compose `environment:` vs devcontainer.json `containerEnv`).
    let upstream_image = upstream_inspect["Config"]["Image"].as_str().unwrap_or("");
    let deacon_image = deacon_inspect["Config"]["Image"].as_str().unwrap_or("");
    assert!(
        upstream_image.contains("debian") && deacon_image.contains("debian"),
        "both should run on the debian base image; upstream={}, deacon={}",
        upstream_image,
        deacon_image
    );

    for (label, inspect) in [("upstream", &upstream_inspect), ("deacon", &deacon_inspect)] {
        let env: Vec<String> = inspect["Config"]["Env"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            env.iter().any(|e| e == "FOO=bar"),
            "{} container missing compose-declared FOO=bar env: {:?}",
            label,
            env
        );
        assert!(
            env.iter().any(|e| e == "BAZ=qux"),
            "{} container missing containerEnv-declared BAZ=qux env: {:?}",
            label,
            env
        );
    }

    let finished = now_rfc3339();
    let raw = raw_paths(&deacon_up, &upstream_up);
    ff(ReportFragment::new(
        BINARY,
        OracleInfo::from(&oracle),
        started,
        finished,
        vec![CaseResult::pass(CASE, raw)],
        Vec::new(),
    )
    .write()
    .await);
}

// ===========================================================================
// Area 6: handoff — bringing a workspace up with one CLI must not corrupt or
// be silently absorbed by the other. The reference CLI has no `down`
// command, so this covers the two directions that actually apply:
// deacon-first and upstream-first, each followed by the OTHER CLI's `up` on
// the SAME workspace.
//
// The compose file is deliberately CO-LOCATED inside `.devcontainer/` — the
// one layout where the reference CLI's own default project-naming
// (`Rp()` in devContainersSpecCLI.js) applies its `<folder>_devcontainer`
// suffix (verified against @devcontainers/cli 0.87.0 source; when the
// compose file lives outside `.devcontainer/`, upstream's default is just
// the bare folder basename, which never collided with deacon's naming
// either way). This is the exact layout under which deacon's OLD naming
// (`<folder>_devcontainer`, identical to upstream's) collided.
//
// Empirically (verified manually against @devcontainers/cli 0.87.0), a name
// collision does not make the second `up` error — it makes it silently
// ATTACH to and reuse the first CLI's container (matched by
// `com.docker.compose.project`+`com.docker.compose.service` labels), which
// is its own kind of cross-CLI corruption: switching tools would silently
// share state instead of each tool managing its own container. Deacon's
// project-name isolation (#265) guarantees the two CLIs' compose projects
// never collide, so the second `up` always provisions its OWN, distinct
// container — asserted here via reported container ID.
// ===========================================================================

#[tokio::test]
async fn parity_handoff_no_cross_cli_container_reuse() {
    const CASE: &str = "handoff-no-reuse";
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));
    let started = now_rfc3339();

    let mut deacon_inv: Option<Invocation> = None;
    let mut upstream_inv: Option<Invocation> = None;

    for deacon_first in [true, false] {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();
        let _cleanup = WsCleanup(ws);
        fs::create_dir(ws.join(".devcontainer")).unwrap();
        fs::write(
            ws.join(".devcontainer/docker-compose.yml"),
            "services:\n  app:\n    image: debian:bookworm-slim\n    command: [\"sleep\", \"infinity\"]\n",
        )
        .unwrap();
        fs::write(
            ws.join(".devcontainer/devcontainer.json"),
            r#"{
  "name": "ParityHandoff",
  "dockerComposeFile": "docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace"
}
"#,
        )
        .unwrap();

        let (first_label, second_label) = if deacon_first {
            ("deacon", "upstream")
        } else {
            ("upstream", "deacon")
        };

        let ws_str = ws.to_string_lossy().into_owned();
        let first_up = if deacon_first {
            ff(exec_deacon(
                BINARY,
                &format!("{CASE}-first-deacon"),
                ExecKind::Lifecycle,
                deacon_bin,
                &["up", "--workspace-folder", &ws_str],
                ws,
            )
            .await)
        } else {
            ff(exec_oracle(
                BINARY,
                &format!("{CASE}-first-upstream"),
                ExecKind::Lifecycle,
                &oracle.path,
                &["up", "--workspace-folder", &ws_str],
                ws,
            )
            .await)
        };
        let first_ok = first_up.success;
        let first_id = json_field(&first_up.stdout_string(), "containerId");

        let second_up = if first_ok {
            Some(if deacon_first {
                ff(exec_oracle(
                    BINARY,
                    &format!("{CASE}-second-upstream"),
                    ExecKind::Lifecycle,
                    &oracle.path,
                    &["up", "--workspace-folder", &ws_str],
                    ws,
                )
                .await)
            } else {
                ff(exec_deacon(
                    BINARY,
                    &format!("{CASE}-second-deacon"),
                    ExecKind::Lifecycle,
                    deacon_bin,
                    &["up", "--workspace-folder", &ws_str],
                    ws,
                )
                .await)
            })
        } else {
            None
        };
        let second_ok = second_up.as_ref().map(|o| o.success);
        let second_id = second_up
            .as_ref()
            .and_then(|o| json_field(&o.stdout_string(), "containerId"));
        let second_stderr = second_up
            .as_ref()
            .map(|o| String::from_utf8_lossy(&o.stderr).to_string());

        // Capture representative invocations for the fragment (clone so the
        // originals remain usable in the assertions below).
        if deacon_first {
            deacon_inv = Some(first_up.clone());
            upstream_inv = second_up.clone();
        } else {
            upstream_inv = Some(first_up.clone());
            deacon_inv = second_up.clone();
        }

        // Tear down BEFORE asserting so a failure never leaks containers.
        // Sweep by the `devcontainer.local_folder` label both CLIs stamp,
        // reading each container's real `com.docker.compose.project` label —
        // this catches upstream's project even though it reports no
        // `composeProjectName` on stdout and its folder-derived project name
        // strips the `TempDir`'s leading `.` (so a guessed
        // `<basename>_devcontainer` would not match). The `WsCleanup` guard
        // repeats this on panic/scope exit as a backstop.
        sweep_ws_containers(ws);

        assert!(
            first_ok,
            "{} up (first) failed: {}",
            first_label,
            String::from_utf8_lossy(&first_up.stderr)
        );
        assert_eq!(
            second_ok,
            Some(true),
            "{} up (second, after {} up on the same workspace) failed: {:?}",
            second_label,
            first_label,
            second_stderr
        );

        let first_id = first_id.expect("first up should report a containerId");
        let second_id = second_id.expect("second up should report a containerId");
        // deacon reports the short (12-char) container ID; the reference CLI
        // reports the full 64-char ID. Compare by common prefix so a literal
        // format mismatch never masks (or fakes) a real reuse.
        let common_len = first_id.len().min(second_id.len());
        assert_ne!(
            &first_id[..common_len],
            &second_id[..common_len],
            "{} up reused {}'s container (project-name collision) instead of provisioning its own: {} vs {}",
            second_label,
            first_label,
            first_id,
            second_id
        );
    }

    let deacon_inv = deacon_inv.expect("deacon invocation captured");
    let upstream_inv = upstream_inv.expect("upstream invocation captured");
    let finished = now_rfc3339();
    let raw = raw_paths(&deacon_inv, &upstream_inv);
    ff(ReportFragment::new(
        BINARY,
        OracleInfo::from(&oracle),
        started,
        finished,
        vec![CaseResult::pass(CASE, raw)],
        Vec::new(),
    )
    .write()
    .await);
}

// ===========================================================================
// Area 7: merged configuration vs runtime truth — for each CLI independently,
// `read-configuration --include-merged-configuration` must agree with what
// `docker inspect` actually shows on the running container (catches
// config-says / runtime-doesn't drift).
// ===========================================================================

#[tokio::test]
async fn parity_merged_config_matches_runtime_truth() {
    const CASE: &str = "merged-config-vs-runtime";
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));
    let started = now_rfc3339();

    let mut deacon_up: Option<Invocation> = None;
    let mut upstream_up: Option<Invocation> = None;

    for (label, is_upstream) in [("upstream", true), ("deacon", false)] {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();
        let _cleanup = WsCleanup(ws);
        fs::create_dir(ws.join(".devcontainer")).unwrap();
        fs::write(
            ws.join(".devcontainer/devcontainer.json"),
            r#"{
  "name": "ParityMergedVsRuntime",
  "image": "debian:bookworm-slim",
  "containerEnv": { "MERGED_TRUTH": "yes" }
}
"#,
        )
        .unwrap();

        let ws_str = ws.to_string_lossy().into_owned();
        let up = if is_upstream {
            ff(exec_oracle(
                BINARY,
                &format!("{CASE}-up-upstream"),
                ExecKind::Lifecycle,
                &oracle.path,
                &["up", "--workspace-folder", &ws_str],
                ws,
            )
            .await)
        } else {
            ff(exec_deacon(
                BINARY,
                &format!("{CASE}-up-deacon"),
                ExecKind::Lifecycle,
                deacon_bin,
                &["up", "--workspace-folder", &ws_str],
                ws,
            )
            .await)
        };
        assert!(
            up.success,
            "{} up failed: {}",
            label,
            String::from_utf8_lossy(&up.stderr)
        );

        let read_config = if is_upstream {
            ff(exec_oracle(
                BINARY,
                &format!("{CASE}-readconfig-upstream"),
                ExecKind::Lifecycle,
                &oracle.path,
                &[
                    "read-configuration",
                    "--workspace-folder",
                    &ws_str,
                    "--include-merged-configuration",
                ],
                ws,
            )
            .await)
        } else {
            ff(exec_deacon(
                BINARY,
                &format!("{CASE}-readconfig-deacon"),
                ExecKind::Lifecycle,
                deacon_bin,
                &[
                    "read-configuration",
                    "--workspace-folder",
                    &ws_str,
                    "--include-merged-configuration",
                ],
                ws,
            )
            .await)
        };
        assert!(
            read_config.success,
            "{} read-configuration failed: {}",
            label,
            String::from_utf8_lossy(&read_config.stderr)
        );
        let read_config_json: Value =
            serde_json::from_str(read_config.stdout_string().trim()).unwrap_or(Value::Null);
        let merged = read_config_json
            .get("mergedConfiguration")
            .or_else(|| read_config_json.get("configuration"))
            .cloned()
            .unwrap_or(read_config_json.clone());
        let container_env = merged["containerEnv"].clone();
        let config_says_truth = container_env["MERGED_TRUTH"].as_str() == Some("yes");

        let container_id = if is_upstream {
            upstream_container_id(ws).expect("no upstream container found")
        } else {
            json_field(&up.stdout_string(), "containerId")
                .expect("deacon up should report containerId")
        };
        let inspect = inspect_json(&container_id);
        let runtime_env: Vec<String> = inspect["Config"]["Env"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let runtime_says_truth = runtime_env.iter().any(|e| e == "MERGED_TRUTH=yes");

        if is_upstream {
            let _ = docker_out_allow_fail(&["rm", "-f", &container_id]);
        } else {
            deacon_down(ws);
        }

        assert!(
            config_says_truth,
            "{}: merged configuration should report containerEnv.MERGED_TRUTH=yes, got {:?}",
            label, container_env
        );
        assert!(
            runtime_says_truth,
            "{}: running container should have MERGED_TRUTH=yes in its env, got {:?}",
            label, runtime_env
        );
        assert_eq!(
            config_says_truth, runtime_says_truth,
            "{}: merged-configuration-says vs runtime-truth drift for containerEnv.MERGED_TRUTH",
            label
        );

        if is_upstream {
            upstream_up = Some(up);
        } else {
            deacon_up = Some(up);
        }
    }

    let deacon_up = deacon_up.expect("deacon up captured");
    let upstream_up = upstream_up.expect("upstream up captured");
    let finished = now_rfc3339();
    let raw = raw_paths(&deacon_up, &upstream_up);
    ff(ReportFragment::new(
        BINARY,
        OracleInfo::from(&oracle),
        started,
        finished,
        vec![CaseResult::pass(CASE, raw)],
        Vec::new(),
    )
    .write()
    .await);
}
