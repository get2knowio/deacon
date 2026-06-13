//! Spawn / adopt the detached port forwarder for `up --auto-forward`.
//!
//! The forwarder is a separate process (the deacon binary re-exec'd with the
//! hidden `__forward-daemon` subcommand) so it can outlive `up` returning to
//! the shell (FR-002). It is single-owner per container: a live marker is
//! adopted rather than duplicated (FR-012). Forwarding is best-effort — any
//! failure here warns loudly but never fails `up` (FR-025, FR-019).

use std::io::IsTerminal;
use std::path::Path;
use std::process::Stdio;

use tracing::{info, warn};

use deacon_core::browser::{DEACON_BROWSER, resolve_browser};
use deacon_core::config::{DevContainerConfig, PortSpec};
use deacon_core::container::ContainerIdentity;
use deacon_core::docker::Docker;
use deacon_core::port_forward::daemon::pid_alive;
use deacon_core::port_forward::{DaemonMarker, marker_path};
use deacon_core::settings::Settings;

use crate::commands::up::args::UpArgs;

/// Reap any forwarders owning this workspace's existing container(s) before
/// `up --remove-existing-container` replaces them (FR-014). Best-effort.
pub async fn reap_existing_forwarders<D: Docker>(
    docker: &D,
    identity: &ContainerIdentity,
    user_data_folder: Option<&Path>,
) {
    let selector = identity
        .workspace_label_selector()
        .unwrap_or_else(|| identity.label_selector());
    match docker.list_containers(Some(&selector)).await {
        Ok(containers) => {
            for c in containers {
                if let Err(e) = deacon_core::port_forward::reap(user_data_folder, &c.id) {
                    warn!(container_id = %c.id, error = %e, "failed to reap forwarder before replace");
                }
            }
        }
        Err(e) => warn!(error = %e, "failed to list containers to reap forwarders before replace"),
    }
}

/// Collect declared port specs (`forwardPorts` + `appPort` + `--forward-port`)
/// to route to the forwarder, de-duplicated, in declaration order.
pub fn declared_port_specs(
    config: &DevContainerConfig,
    cli_forward_ports: &[String],
) -> Vec<String> {
    let mut specs: Vec<String> = Vec::new();
    let mut push = |s: String| {
        if !specs.contains(&s) {
            specs.push(s);
        }
    };
    for ps in &config.forward_ports {
        push(port_spec_to_string(ps));
    }
    if let Some(app) = &config.app_port {
        for ps in app.specs() {
            push(port_spec_to_string(ps));
        }
    }
    for s in cli_forward_ports {
        push(s.clone());
    }
    specs
}

fn port_spec_to_string(spec: &PortSpec) -> String {
    match spec {
        PortSpec::Number(n) => n.to_string(),
        PortSpec::String(s) => s.clone(),
    }
}

/// Spawn the forwarder for `container_id`, or adopt an existing live one.
///
/// `declared` is the already-collected declared-port spec set (captured before
/// the static `-p` ports were stripped for the create path). Best-effort:
/// returns even on failure (after warning) so `up` still succeeds with the
/// container running. On non-Unix builds it warns that forwarding is
/// unsupported and returns (the rest of `up` is unaffected).
pub async fn spawn_or_adopt(
    args: &UpArgs,
    container_id: &str,
    workspace_folder: &Path,
    config_path: &Path,
    declared: &[String],
) {
    if !cfg!(unix) {
        warn!("--auto-forward is not supported on this platform (Unix-only in v1); skipping");
        return;
    }

    // Adopt-or-reuse: a live marker means a forwarder already owns this
    // container — do not spawn a duplicate (FR-012).
    if let Some(pid) = live_forwarder_pid(args.user_data_folder.as_deref(), container_id) {
        info!(pid, container_id, "reusing existing forwarder");
        return;
    }

    let workspace = workspace_folder
        .canonicalize()
        .unwrap_or_else(|_| workspace_folder.to_path_buf());

    if let Err(e) = spawn_daemon(args, container_id, &workspace, config_path, declared) {
        warn!(
            container_id,
            error = %e,
            "failed to start port forwarder; container is up but ports are not forwarded"
        );
    }
}

/// Resolve the auto-open browser program (machine-owner only). Precedence:
/// `DEACON_BROWSER` env, then `settings.browser`, else `None` (OS default
/// opener). Best-effort — a settings read error degrades to "no configured
/// browser" and never fails `up`.
pub(crate) fn resolve_browser_cli(user_data_folder: Option<&Path>) -> Option<String> {
    let env = std::env::var(DEACON_BROWSER).ok();
    let settings = Settings::load(user_data_folder).unwrap_or_default();
    resolve_browser(env.as_deref(), &settings)
}

/// Whether to enable browser auto-open. Skip in CI / when `stderr` isn't a TTY,
/// UNLESS a browser is explicitly configured — an explicit machine-owner choice
/// signals intent (and is the lever hermetic tests use to force-enable).
pub(crate) fn auto_open_enabled(browser_configured: bool) -> bool {
    if browser_configured {
        return true;
    }
    let ci = std::env::var("CI").is_ok()
        || std::env::var("GITHUB_ACTIONS").is_ok()
        || std::env::var("CONTINUOUS_INTEGRATION").is_ok();
    !ci && std::io::stderr().is_terminal()
}

/// Read a live forwarder pid from the marker, if present and alive.
pub fn live_forwarder_pid(user_data_folder: Option<&Path>, container_id: &str) -> Option<u32> {
    let path = marker_path(user_data_folder, container_id).ok()?;
    let text = std::fs::read_to_string(&path).ok()?;
    let marker: DaemonMarker = serde_json::from_str(&text).ok()?;
    if pid_alive(marker.pid) {
        Some(marker.pid)
    } else {
        None
    }
}

/// Re-exec the deacon binary with the hidden `__forward-daemon` subcommand,
/// detached (no stdio inherited; the child reopens its fds onto the log).
fn spawn_daemon(
    args: &UpArgs,
    container_id: &str,
    workspace: &Path,
    config_path: &Path,
    declared: &[String],
) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);

    // Global flags first so they bind to the top-level parser.
    cmd.arg("--docker-path").arg(&args.docker_path);
    if let Some(udf) = &args.user_data_folder {
        cmd.arg("--user-data-folder").arg(udf);
    }
    cmd.arg("--config").arg(config_path);

    cmd.arg("__forward-daemon")
        .arg("--container-id")
        .arg(container_id)
        .arg("--workspace")
        .arg(workspace);
    for spec in declared {
        cmd.arg("--declared-port").arg(spec);
    }
    if args.ports_events {
        cmd.arg("--ports-events");
    }

    // Browser auto-open (onAutoForward: openBrowser) is machine-owner-resolved
    // here (env > settings) and the headless/CI gate is decided here too — the
    // daemon's stdio is /dev/null so it cannot evaluate `is_terminal()` itself.
    // Adoption returns before this, so an adopted daemon keeps its own setting.
    let browser = resolve_browser_cli(args.user_data_folder.as_deref());
    if let Some(b) = &browser {
        cmd.arg("--browser").arg(b);
    }
    if !auto_open_enabled(browser.is_some()) {
        cmd.arg("--no-auto-open");
    }

    // Detach: the child re-opens its own stdio onto the per-container log, so
    // it must not hold the parent's stdout (which carries the `up` JSON result).
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let child = cmd.spawn()?;
    info!(
        container_id,
        pid = child.id(),
        declared = declared.len(),
        "spawned port forwarder"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::config::AppPort;
    use deacon_core::port_forward::DaemonMarker;
    use tempfile::TempDir;

    fn write_marker(udf: &Path, container_id: &str, pid: u32) {
        let marker = DaemonMarker {
            pid,
            container_id: container_id.to_string(),
            workspace: "/ws".to_string(),
            started_at: "2026-06-08T00:00:00Z".to_string(),
            log_path: "/tmp/x.log".to_string(),
        };
        let path = marker_path(Some(udf), container_id).unwrap();
        std::fs::write(path, serde_json::to_string(&marker).unwrap()).unwrap();
    }

    #[test]
    fn adopt_reuses_live_pid_and_ignores_dead_or_missing() {
        let dir = TempDir::new().unwrap();
        let udf = dir.path();

        // Missing marker ⇒ no pid (spawn fresh).
        assert_eq!(live_forwarder_pid(Some(udf), "none"), None);

        // Live pid (our own process). On Unix a live pid is adopted/reused. On
        // non-Unix the forward daemon is unsupported and `pid_alive` is always
        // false (see `port_forward::daemon::pid_alive`), so even a live marker
        // yields None (spawn-fresh) — assert the platform-correct behavior so the
        // test runs on Windows instead of being skipped.
        let me = std::process::id();
        write_marker(udf, "live", me);
        #[cfg(unix)]
        assert_eq!(live_forwarder_pid(Some(udf), "live"), Some(me));
        #[cfg(not(unix))]
        assert_eq!(live_forwarder_pid(Some(udf), "live"), None);

        // Dead pid ⇒ spawn fresh (None).
        write_marker(udf, "dead", 2_000_000_000);
        assert_eq!(live_forwarder_pid(Some(udf), "dead"), None);
    }

    #[test]
    fn collects_and_dedups_declared_specs() {
        let config = DevContainerConfig {
            forward_ports: vec![
                PortSpec::Number(3000),
                PortSpec::String("db:5432".to_string()),
            ],
            app_port: Some(AppPort::Single(PortSpec::Number(8080))),
            ..DevContainerConfig::default()
        };
        let cli = vec!["3000".to_string(), "9000".to_string()];
        let specs = declared_port_specs(&config, &cli);
        assert_eq!(specs, vec!["3000", "db:5432", "8080", "9000"]);
    }
}
