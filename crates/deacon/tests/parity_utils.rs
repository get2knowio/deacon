#![allow(dead_code)]
#![allow(unused_imports)]

use assert_cmd::Command;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Environment gate: set to `1` to enable parity tests.
const ENV_ENABLE: &str = "DEACON_PARITY";

/// Optional override for upstream read-configuration invocation.
/// Template placeholders: {config}, {workspace}
const ENV_UPSTREAM_READ_CONFIG_TEMPLATE: &str = "DEACON_PARITY_UPSTREAM_READ_CONFIGURATION";
/// Optional override for upstream devcontainer binary path.
const ENV_UPSTREAM_BIN: &str = "DEACON_PARITY_DEVCONTAINER";

pub fn repo_root() -> PathBuf {
    // crates/deacon -> repo root is two levels up
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.parent()
        .and_then(|p| p.parent())
        .unwrap_or(&here)
        .to_path_buf()
}

pub fn parity_enabled() -> bool {
    std::env::var(ENV_ENABLE).map(|v| v == "1").unwrap_or(false)
}

pub fn docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn upstream_available() -> bool {
    if !parity_enabled() {
        return false;
    }
    if let Ok(bin) = std::env::var(ENV_UPSTREAM_BIN) {
        return std::path::Path::new(&bin).exists();
    }
    std::process::Command::new("sh")
        .arg("-lc")
        .arg("command -v devcontainer >/dev/null 2>&1")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn skip_reason() -> String {
    if !parity_enabled() {
        return "DEACON_PARITY not set to 1".to_string();
    }
    if let Ok(bin) = std::env::var(ENV_UPSTREAM_BIN) {
        return format!("devcontainer CLI not found at {}", bin);
    }
    if !upstream_available() {
        return "`devcontainer` CLI not found in PATH".to_string();
    }
    String::from("unknown")
}

/// Run our CLI and return stdout as String on success.
pub fn run_deacon_read_configuration(config_path: &Path) -> anyhow::Result<String> {
    // Align workspace with upstream: use the directory containing the devcontainer folder
    // (i.e., parent of the config file's directory), falling back to repo_root if unknown.
    let workspace: std::path::PathBuf = config_path
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(repo_root);

    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(&workspace)
        .arg("--config")
        .arg(config_path)
        .assert()
        .get_output()
        .to_owned();

    if !output.status.success() {
        anyhow::bail!(
            "deacon read-configuration failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Try to invoke the upstream CLI for read-configuration.
/// Attempts templates in order: env override, `--config`, `--workspace-folder`, positional path.
pub fn run_upstream_read_configuration(config_path: &Path) -> anyhow::Result<String> {
    let workspace = config_path
        .parent()
        .map(|p| p.parent().unwrap_or(p))
        .unwrap_or_else(|| Path::new("."));

    // 1) Env-provided template
    if let Ok(tpl) = std::env::var(ENV_UPSTREAM_READ_CONFIG_TEMPLATE) {
        if let Ok(s) = try_run_template(&tpl, config_path, workspace) {
            return Ok(s);
        }
    }

    // 2) Known attempt: --config <path>
    if let Ok(s) = run_devcontainer(
        &[
            "read-configuration",
            "--config",
            &config_path.to_string_lossy(),
        ],
        workspace,
    ) {
        return Ok(s);
    }

    // 3) Known attempt: --workspace-folder <workspace>
    if let Ok(s) = run_devcontainer(
        &[
            "read-configuration",
            "--workspace-folder",
            &workspace.to_string_lossy(),
        ],
        workspace,
    ) {
        return Ok(s);
    }

    // 4) Fallback: positional <workspace>
    if let Ok(s) = run_devcontainer(
        &["read-configuration", &workspace.to_string_lossy()],
        workspace,
    ) {
        return Ok(s);
    }

    anyhow::bail!("Unable to run upstream devcontainer read-configuration with known flag variants")
}

fn try_run_template(tpl: &str, config: &Path, workspace: &Path) -> anyhow::Result<String> {
    // Split a simple space-delimited template after substitution
    let cmdline = tpl
        .replace("{config}", &config.to_string_lossy())
        .replace("{workspace}", &workspace.to_string_lossy());
    let parts: Vec<String> = shell_words::split(&cmdline)?;
    run_devcontainer(&parts, workspace)
}

fn run_devcontainer<S: AsRef<str>>(args: &[S], cwd: &Path) -> anyhow::Result<String> {
    let bin = std::env::var(ENV_UPSTREAM_BIN).unwrap_or_else(|_| "devcontainer".to_string());
    let mut cmd = std::process::Command::new(bin);
    cmd.current_dir(cwd);
    for a in args {
        cmd.arg(a.as_ref());
    }
    let out = cmd.output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        anyhow::bail!(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

/// Normalize JSON produced by both CLIs for functional comparison.
/// Drops volatile fields and returns a pruned object with core keys.
pub fn normalize_config_json(raw: &str) -> anyhow::Result<Value> {
    let v: Value = serde_json::from_str(raw.trim())?;
    // Upstream devcontainer CLI returns an object with a top-level "configuration" object.
    // Our CLI returns the effective configuration object directly. Handle both.
    let obj = match v.as_object() {
        Some(o) => o,
        None => return Ok(v),
    };
    if let Some(conf) = obj.get("configuration") {
        return Ok(extract_core_config(conf));
    }
    Ok(extract_core_config(&v))
}

/// Extract a subset of keys that define the effective devcontainer behavior.
fn extract_core_config(v: &Value) -> Value {
    use serde_json::{Map, json};
    let mut out = Map::new();
    let obj = match v.as_object() {
        Some(o) => o,
        None => return v.clone(),
    };

    for k in [
        "name",
        "image",
        "workspaceFolder",
        "dockerFile",
        "build",
        "features",
        "containerEnv",
        "mounts",
        "onCreateCommand",
        "postCreateCommand",
        "postStartCommand",
        "postAttachCommand",
    ] {
        if let Some(val) = obj.get(k) {
            // Skip nulls to avoid mismatches when one side omits null-valued keys
            if !val.is_null() {
                out.insert(k.to_string(), val.clone());
            }
        }
    }

    // customizations.vscode.extensions (if present)
    if let Some(customizations) = obj.get("customizations").and_then(|c| c.as_object()) {
        if let Some(vscode) = customizations.get("vscode").and_then(|c| c.as_object()) {
            if let Some(ext) = vscode.get("extensions").cloned() {
                out.insert(
                    "customizations".to_string(),
                    json!({ "vscode": { "extensions": ext } }),
                );
            }
        }
    }

    let mut core = Value::Object(out);
    sanitize_dynamic_values(&mut core);
    core
}

/// Write a devcontainer.json file to the .devcontainer directory
pub fn write_devcontainer(ws: &Path, json: &str) -> anyhow::Result<()> {
    let dc_dir = ws.join(".devcontainer");
    std::fs::create_dir_all(&dc_dir)?;
    std::fs::write(dc_dir.join("devcontainer.json"), json)?;
    Ok(())
}

/// Recursively sanitize dynamic IDs and placeholders so outputs are comparable.
fn sanitize_dynamic_values(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (_k, val) in map.iter_mut() {
                sanitize_dynamic_values(val);
            }
        }
        Value::Array(arr) => {
            for val in arr.iter_mut() {
                sanitize_dynamic_values(val);
            }
        }
        Value::String(s) => {
            let mut replaced = s.replace("${devcontainerId}", "<ID>");
            replaced = replace_hex12(&replaced);
            *s = replaced;
        }
        _ => {}
    }
}

/// Replace any 12-character contiguous lowercase hex sequences with <ID>.
fn replace_hex12(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        // Attempt to match 12 hex chars starting at i
        if i + 12 <= bytes.len() {
            let slice = &bytes[i..i + 12];
            if is_hex_slice(slice) {
                out.push_str("<ID>");
                i += 12;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn is_hex_slice(slice: &[u8]) -> bool {
    slice.iter().all(|b| {
        matches!(b,
            b'0'..=b'9' | b'a'..=b'f'
        )
    })
}

/// Run upstream devcontainer command and return output
pub fn run_upstream(ws: &Path, args: &[&str]) -> anyhow::Result<std::process::Output> {
    let bin = std::env::var(ENV_UPSTREAM_BIN).unwrap_or_else(|_| "devcontainer".to_string());
    let mut cmd = std::process::Command::new(bin);
    cmd.current_dir(ws);
    for arg in args {
        cmd.arg(arg);
    }
    Ok(cmd.output()?)
}

/// Run deacon command and return output
pub fn run_deacon(ws: &Path, args: &[&str]) -> anyhow::Result<std::process::Output> {
    use assert_cmd::Command;
    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.current_dir(ws);
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.assert().get_output().to_owned();
    Ok(std::process::Output {
        status: output.status,
        stdout: output.stdout.clone(),
        stderr: output.stderr.clone(),
    })
}

/// Extract stdout as trimmed string
pub fn stdout_str(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

// ===========================================================================
// Shared Docker discovery / cleanup helpers (used by the observable-state
// parity binaries). Mirrors the private copies in
// `parity_observable_state.rs`; centralized here so the state differ
// (`parity_state_diff.rs`) can reuse them.
// ===========================================================================

/// Triple-gate a docker+upstream parity test. Returns `true` only when
/// `DEACON_PARITY=1`, Docker is reachable, and the `devcontainer` CLI is
/// available. Prints a skip reason and returns `false` otherwise (tests then
/// early-return — never panic-skip).
pub fn gated() -> bool {
    if !parity_enabled() {
        eprintln!("Skipping parity test: {}", skip_reason());
        return false;
    }
    if !docker_available() {
        eprintln!("Skipping parity test: Docker not available");
        return false;
    }
    if !upstream_available() {
        eprintln!("Skipping parity test: {}", skip_reason());
        return false;
    }
    true
}

/// Run `docker <args>`, returning (success, stdout, stderr) without panicking
/// on failure (for best-effort cleanup / discovery).
pub fn docker_out_allow_fail(args: &[&str]) -> (bool, String, String) {
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

/// Extract a top-level string field from a `deacon up`/`deacon build` JSON
/// result on stdout (tolerant of leading log lines before the JSON object).
pub fn json_field(output: &std::process::Output, field: &str) -> Option<String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
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

/// The canonicalized workspace path, matching the value both CLIs stamp into
/// the `devcontainer.local_folder` label. Filtering `docker ps` by the raw
/// (un-canonicalized) temp path misses the container where the temp dir is
/// symlinked (e.g. macOS `/tmp` -> `/private/tmp`).
pub fn canonical_ws_display(ws: &Path) -> String {
    ws.canonicalize()
        .unwrap_or_else(|_| ws.to_path_buf())
        .display()
        .to_string()
}

/// Discover the first running container for `ws` by its canonicalized
/// `devcontainer.local_folder` label (both CLIs stamp it). Used to locate the
/// upstream CLI's container, which does not report a container id on stdout.
pub fn upstream_container_id(ws: &Path) -> Option<String> {
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

/// `docker compose -p <project> down` (best-effort, volumes + local images).
pub fn deacon_compose_down_by_project(project_name: &str) {
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

/// `deacon down --remove` for a workspace (best-effort).
pub fn deacon_down(ws: &Path) {
    let _ = run_deacon(
        ws,
        &[
            "down",
            "--workspace-folder",
            &ws.to_string_lossy(),
            "--remove",
        ],
    );
}

/// Best-effort teardown of every container stamped with this workspace's
/// `devcontainer.local_folder` label (both CLIs stamp it), plus each
/// container's compose project read from its actual
/// `com.docker.compose.project` label.
pub fn sweep_ws_containers(ws: &Path) {
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
/// assertion can never leak Docker state. Declare it right after the workspace
/// path so it drops before the `TempDir`.
pub struct WsCleanup<'a>(pub &'a Path);
impl Drop for WsCleanup<'_> {
    fn drop(&mut self) {
        sweep_ws_containers(self.0);
    }
}

// ===========================================================================
// Normalized observable-state differ (#267 follow-up)
//
// The parity_* suites historically asserted "both CLIs launched" or checked a
// single hand-picked field. The bugs that actually bite are OUTCOME
// divergences on a successful launch: a missing mount (#266, #272), a missing
// env var, a colliding project name (#265). This differ snapshots the
// normalized observable state of a container (`docker inspect`) for each CLI
// and diffs it field-by-field, subtracting only:
//   * volatile-but-equivalent values (container ids, per-workspace temp paths,
//     compose-project-prefixed volume/network names),
//   * an explicit, documented allowlist of INTENTIONAL deacon divergences, and
//   * a documented KNOWN_GAPS list (open bugs, each tied to an issue).
// Anything left over is a real parity finding and fails the test.
// ===========================================================================

/// Env keys present in every container and/or runtime-injected; not meaningful
/// for cross-CLI outcome parity. Subtracted before diffing env.
pub const NOISE_ENV_KEYS: &[&str] = &["PATH", "HOME", "HOSTNAME", "TERM", "container"];

/// Label namespaces both CLIs stamp by design and differently (identity,
/// per-CLI metadata blob, compose bookkeeping, Docker Desktop). Subtracted
/// before diffing labels so only semantic image/config labels remain.
pub const INTENTIONAL_LABEL_PREFIXES: &[&str] = &[
    "devcontainer.",
    "com.docker.",
    "desktop.",
    "dev.containers.",
];

/// INTENTIONAL divergences deacon deliberately makes vs the reference CLI.
/// A raw divergence whose `field` starts with one of these matchers is dropped
/// (logged, never fails). Keep this list SMALL and each entry justified — it
/// is the reviewable record of where deacon is knowingly different. (Most
/// intentional divergences — project name, identity labels, keep-alive
/// command, project-prefixed networks — are already handled by normalization
/// or by fields the differ does not compare; entries here are only for
/// divergences that survive normalization.)
pub const KNOWN_INTENTIONAL_DIVERGENCES: &[(&str, &str)] = &[
    // (field-matcher, rationale)
];

/// A known, OPEN parity gap: deacon does not yet match the reference CLI here.
/// The differ reports it (so it stays visible) but does not fail, until the
/// bug is fixed — at which point the entry is removed and the divergence must
/// disappear (or the test flips red, catching a widened gap).
pub struct KnownGap {
    pub field_matcher: &'static str,
    pub issue: &'static str,
    pub note: &'static str,
}

pub const KNOWN_GAPS: &[KnownGap] = &[KnownGap {
    field_matcher: "mount:/feat-mnt",
    issue: "#272",
    note: "feature-contributed mounts dropped on deacon compose path",
}];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountState {
    pub mount_type: String,
    pub ro: bool,
    /// Normalized source descriptor for REPORTING only (bind: leaf component;
    /// volume: name with compose-project prefix stripped). NOT compared — bind
    /// sources are per-workspace temp paths that legitimately differ between
    /// the two CLIs' independent workspaces.
    pub source_tail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StateSnapshot {
    /// destination -> mount state
    pub mounts: BTreeMap<String, MountState>,
    /// `KEY=VALUE` entries, noise keys removed
    pub env: BTreeSet<String>,
    /// labels with CLI-namespaced keys stripped
    pub labels: BTreeMap<String, String>,
    pub user: String,
    pub working_dir: String,
    /// `Config.ExposedPorts` keys (image `EXPOSE` + declared), e.g. `3000/tcp`.
    pub exposed_ports: BTreeSet<String>,
    /// `HostConfig.PortBindings` keys — container ports actually PUBLISHED to
    /// the host (e.g. via `appPort`), e.g. `3000/tcp`. (`forwardPorts` is a
    /// runtime forward, not a publish, and does not appear here.)
    pub published_ports: BTreeSet<String>,
    /// Captured for debugging; NOT diffed (keep-alive strategy differs by CLI).
    pub entrypoint: Vec<String>,
    /// Captured for debugging; NOT diffed (keep-alive strategy differs by CLI).
    pub cmd: Vec<String>,
    /// Captured (compose-project-prefix-normalized) for debugging; NOT diffed.
    pub networks: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    /// Stable field identifier, e.g. `mount:/feat-mnt`, `env:FOO`, `user`.
    pub field: String,
    pub detail: String,
}

/// Snapshot a running container by id (`docker inspect`), normalized.
pub fn normalized_state(container_id: &str) -> StateSnapshot {
    let raw = docker_inspect_one(container_id);
    assert!(
        raw.get("Config").is_some(),
        "docker inspect for {} has no Config object; unexpected shape: {}",
        container_id,
        raw
    );
    snapshot_from_inspect(&raw)
}

fn docker_inspect_one(container_id: &str) -> Value {
    let out = std::process::Command::new("docker")
        .args(["inspect", container_id])
        .output()
        .expect("docker inspect should run");
    assert!(
        out.status.success(),
        "docker inspect {} failed: {}",
        container_id,
        String::from_utf8_lossy(&out.stderr)
    );
    let arr: Vec<Value> = serde_json::from_str(&String::from_utf8_lossy(&out.stdout))
        .expect("docker inspect returns a JSON array");
    arr.into_iter()
        .next()
        .expect("docker inspect returns at least one entry")
}

/// Build a normalized snapshot from a single `docker inspect` object. Pure —
/// unit-testable without Docker.
pub fn snapshot_from_inspect(raw: &Value) -> StateSnapshot {
    let project = raw["Config"]["Labels"]["com.docker.compose.project"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let mut mounts = BTreeMap::new();
    if let Some(arr) = raw["Mounts"].as_array() {
        for m in arr {
            let dest = m["Destination"].as_str().unwrap_or("").to_string();
            if dest.is_empty() {
                continue;
            }
            let mount_type = m["Type"].as_str().unwrap_or("").to_string();
            let ro = !m["RW"].as_bool().unwrap_or(true);
            let source_tail = if mount_type == "volume" {
                strip_project_prefix(m["Name"].as_str().unwrap_or(""), &project)
            } else if mount_type == "bind" {
                Path::new(m["Source"].as_str().unwrap_or(""))
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                String::new()
            };
            mounts.insert(
                dest,
                MountState {
                    mount_type,
                    ro,
                    source_tail,
                },
            );
        }
    }

    let env = str_array(&raw["Config"]["Env"])
        .into_iter()
        .filter(|e| {
            let key = e.split_once('=').map(|(k, _)| k).unwrap_or(e.as_str());
            !NOISE_ENV_KEYS.contains(&key)
        })
        .collect();

    let labels = raw["Config"]["Labels"]
        .as_object()
        .map(|o| {
            o.iter()
                .filter(|(k, _)| !INTENTIONAL_LABEL_PREFIXES.iter().any(|p| k.starts_with(p)))
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                .collect()
        })
        .unwrap_or_default();

    let exposed_ports = raw["Config"]["ExposedPorts"]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();

    let published_ports = raw["HostConfig"]["PortBindings"]
        .as_object()
        .map(|o| {
            o.iter()
                // A key maps to a non-null bindings array when actually published.
                .filter(|(_, v)| v.as_array().is_some_and(|a| !a.is_empty()))
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default();

    let networks = raw["NetworkSettings"]["Networks"]
        .as_object()
        .map(|o| {
            o.keys()
                .map(|k| strip_project_prefix(k, &project))
                .collect()
        })
        .unwrap_or_default();

    StateSnapshot {
        mounts,
        env,
        labels,
        user: raw["Config"]["User"].as_str().unwrap_or("").to_string(),
        working_dir: raw["Config"]["WorkingDir"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        exposed_ports,
        published_ports,
        entrypoint: str_array(&raw["Config"]["Entrypoint"]),
        cmd: str_array(&raw["Config"]["Cmd"]),
        networks,
    }
}

fn str_array(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn strip_project_prefix(name: &str, project: &str) -> String {
    if !project.is_empty() {
        if let Some(rest) = name.strip_prefix(&format!("{project}_")) {
            return rest.to_string();
        }
    }
    name.to_string()
}

/// Field-by-field diff of two normalized snapshots. Compares mounts (by
/// destination + type + read-only), env (by key), labels (by key), exposed
/// ports (set), and the scalar `user` / `working_dir`. Deliberately does NOT
/// compare mount SOURCES (per-workspace temp paths), cmd/entrypoint (keep-alive
/// strategy differs), or networks (compose-project-prefixed) — see the
/// `StateSnapshot` field docs.
pub fn diff_states(deacon: &StateSnapshot, upstream: &StateSnapshot) -> Vec<Divergence> {
    let mut out = Vec::new();

    let dests: BTreeSet<&String> = deacon.mounts.keys().chain(upstream.mounts.keys()).collect();
    for dest in dests {
        match (deacon.mounts.get(dest), upstream.mounts.get(dest)) {
            (Some(d), Some(u)) => {
                if d.mount_type != u.mount_type {
                    out.push(Divergence {
                        field: format!("mount:{dest}"),
                        detail: format!(
                            "type differs: deacon={} upstream={}",
                            d.mount_type, u.mount_type
                        ),
                    });
                }
                if d.ro != u.ro {
                    out.push(Divergence {
                        field: format!("mount:{dest}"),
                        detail: format!("read-only differs: deacon={} upstream={}", d.ro, u.ro),
                    });
                }
            }
            (Some(d), None) => out.push(Divergence {
                field: format!("mount:{dest}"),
                detail: format!("present on deacon ({}), absent upstream", d.mount_type),
            }),
            (None, Some(u)) => out.push(Divergence {
                field: format!("mount:{dest}"),
                detail: format!("present upstream ({}), absent deacon", u.mount_type),
            }),
            (None, None) => unreachable!(),
        }
    }

    diff_kv(
        "env",
        &env_map(&deacon.env),
        &env_map(&upstream.env),
        &mut out,
    );
    diff_kv("label", &deacon.labels, &upstream.labels, &mut out);

    for p in deacon.exposed_ports.difference(&upstream.exposed_ports) {
        out.push(Divergence {
            field: format!("port:{p}"),
            detail: "exposed on deacon, not upstream".to_string(),
        });
    }
    for p in upstream.exposed_ports.difference(&deacon.exposed_ports) {
        out.push(Divergence {
            field: format!("port:{p}"),
            detail: "exposed upstream, not deacon".to_string(),
        });
    }

    for p in deacon.published_ports.difference(&upstream.published_ports) {
        out.push(Divergence {
            field: format!("pubport:{p}"),
            detail: "published on deacon, not upstream".to_string(),
        });
    }
    for p in upstream.published_ports.difference(&deacon.published_ports) {
        out.push(Divergence {
            field: format!("pubport:{p}"),
            detail: "published upstream, not deacon".to_string(),
        });
    }

    if norm_user(&deacon.user) != norm_user(&upstream.user) {
        out.push(Divergence {
            field: "user".to_string(),
            detail: format!("deacon={:?} upstream={:?}", deacon.user, upstream.user),
        });
    }
    if deacon.working_dir != upstream.working_dir {
        out.push(Divergence {
            field: "workingdir".to_string(),
            detail: format!(
                "deacon={:?} upstream={:?}",
                deacon.working_dir, upstream.working_dir
            ),
        });
    }

    out
}

/// An empty `Config.User` means "image default", which for the Linux base
/// images used here is root. Treat "" and "root" as equivalent so the cosmetic
/// difference (deacon leaves it empty; the reference CLI stamps "root" on its
/// feature-image build) is not flagged, while a real `remoteUser` /
/// `containerUser` (a specific non-root user) still diverges.
fn norm_user(u: &str) -> &str {
    if u.is_empty() { "root" } else { u }
}

fn env_map(set: &BTreeSet<String>) -> BTreeMap<String, String> {
    set.iter()
        .map(|e| match e.split_once('=') {
            Some((k, v)) => (k.to_string(), v.to_string()),
            None => (e.clone(), String::new()),
        })
        .collect()
}

fn diff_kv(
    kind: &str,
    deacon: &BTreeMap<String, String>,
    upstream: &BTreeMap<String, String>,
    out: &mut Vec<Divergence>,
) {
    let keys: BTreeSet<&String> = deacon.keys().chain(upstream.keys()).collect();
    for k in keys {
        match (deacon.get(k), upstream.get(k)) {
            (Some(dv), Some(uv)) => {
                if dv != uv {
                    out.push(Divergence {
                        field: format!("{kind}:{k}"),
                        detail: format!("value differs: deacon={dv:?} upstream={uv:?}"),
                    });
                }
            }
            (Some(dv), None) => out.push(Divergence {
                field: format!("{kind}:{k}"),
                detail: format!("present on deacon ({dv:?}), absent upstream"),
            }),
            (None, Some(uv)) => out.push(Divergence {
                field: format!("{kind}:{k}"),
                detail: format!("present upstream ({uv:?}), absent deacon"),
            }),
            (None, None) => unreachable!(),
        }
    }
}

/// Classification of a raw divergence.
pub enum DivergenceClass {
    /// Matches `KNOWN_INTENTIONAL_DIVERGENCES` or the caller's `extra_allowed`.
    Intentional(String),
    /// Matches a `KNOWN_GAPS` entry (open bug).
    KnownGap(&'static KnownGap),
    /// A real, unexplained parity divergence — fails the test.
    Unexpected,
}

/// Match a divergence `field` against an allowlist/gap `matcher`. Matchers are
/// EXACT by default; a trailing `*` makes it a prefix match. Exact-by-default
/// matters for path-like fields where one destination is a string prefix of
/// another — e.g. `mount:/workspace` must NOT match `mount:/workspaces/sib`.
fn field_matches(field: &str, matcher: &str) -> bool {
    match matcher.strip_suffix('*') {
        Some(prefix) => field.starts_with(prefix),
        None => field == matcher,
    }
}

pub fn classify_divergence(field: &str, extra_allowed: &[&str]) -> DivergenceClass {
    if let Some(m) = extra_allowed.iter().find(|m| field_matches(field, m)) {
        return DivergenceClass::Intentional(format!("caller-allowed ({m})"));
    }
    if let Some((_, why)) = KNOWN_INTENTIONAL_DIVERGENCES
        .iter()
        .find(|(m, _)| field_matches(field, m))
    {
        return DivergenceClass::Intentional((*why).to_string());
    }
    if let Some(gap) = KNOWN_GAPS
        .iter()
        .find(|g| field_matches(field, g.field_matcher))
    {
        return DivergenceClass::KnownGap(gap);
    }
    DivergenceClass::Unexpected
}

/// Snapshot both containers and assert outcome parity. Fails with a readable
/// per-field report unless every divergence is intentional / caller-allowed /
/// a tracked known gap.
pub fn assert_state_parity(deacon_id: &str, upstream_id: &str, extra_allowed: &[&str]) {
    let deacon = normalized_state(deacon_id);
    let upstream = normalized_state(upstream_id);
    assert_snapshots_parity(&deacon, &upstream, extra_allowed);
}

/// As `assert_state_parity`, but on already-captured snapshots (so a test can
/// assert fixture-specific markers on the snapshot first — guarding against a
/// vacuous pass over empty state).
pub fn assert_snapshots_parity(
    deacon: &StateSnapshot,
    upstream: &StateSnapshot,
    extra_allowed: &[&str],
) {
    let divs = diff_states(deacon, upstream);
    let mut unexpected = Vec::new();
    for div in &divs {
        match classify_divergence(&div.field, extra_allowed) {
            DivergenceClass::Intentional(why) => {
                eprintln!(
                    "[state-diff] intentional: {} — {} ({})",
                    div.field, div.detail, why
                );
            }
            DivergenceClass::KnownGap(gap) => {
                eprintln!(
                    "[state-diff] KNOWN GAP {}: {} — {} ({})",
                    gap.issue, div.field, div.detail, gap.note
                );
            }
            DivergenceClass::Unexpected => unexpected.push(div),
        }
    }
    if !unexpected.is_empty() {
        let mut msg = String::from("observable-state parity divergence(s) deacon vs upstream:\n");
        for div in &unexpected {
            msg.push_str(&format!("  - {}: {}\n", div.field, div.detail));
        }
        panic!("{msg}");
    }
}
