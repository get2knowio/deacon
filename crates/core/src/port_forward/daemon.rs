//! The detached forwarder supervisor loop.
//!
//! [`run`] is the entrypoint of the re-exec'd `__forward-daemon` process. It
//! eager-binds declared ports, polls the container's listening sockets (~1 s),
//! reconciles detected-vs-active forwards, writes the [`DaemonMarker`], and
//! self-exits when its container vanishes.
//!
//! Process detachment / signaling helpers ([`daemonize`], [`pid_alive`],
//! [`terminate_pid`]) wrap `nix` safe syscalls on Unix; on other platforms the
//! forwarder is unsupported and these return a clear error (Principle IV).
//!
//! [`DaemonMarker`]: crate::port_forward::DaemonMarker

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use tracing::{debug, info, warn};

use super::{
    DaemonMarker, DetectedPort, ForwardOrigin, ForwardSpec, PortForwardError,
    ResolvedPortAttributes, Result, detect, log_path, marker_path, registry, relay,
};
use crate::config::{ConfigLoader, DevContainerConfig, OnAutoForward, PortAttributes};
use crate::docker::{CliRuntime, Docker, ExecConfig};
use crate::ports::PortEvent;

/// Fixed detection poll interval (FR-004). Not configurable in v1.
pub const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1000);

/// Default in-container dial host for the relay (the container's own loopback).
pub const DEFAULT_DIAL_HOST: &str = "127.0.0.1";

/// Configuration handed to the detached forwarder process.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// Full id of the container to forward.
    pub container_id: String,
    /// Canonical workspace path of the owning devcontainer.
    pub workspace: PathBuf,
    /// User-data folder for registry/marker/log (default `~/.deacon`).
    pub user_data_folder: Option<PathBuf>,
    /// Raw declared-port specs (`PORT`, `HOST:CONTAINER`, `service:port`).
    pub declared_ports: Vec<String>,
    /// Path to the resolved devcontainer.json (for portsAttributes).
    pub config_path: Option<PathBuf>,
    /// Emit machine-readable `PORT_EVENT:` lines.
    pub ports_events: bool,
    /// Docker CLI path used for `exec`/`inspect`.
    pub docker_path: String,
    /// Resolved browser program for `onAutoForward: openBrowser` auto-open
    /// (`DEACON_BROWSER` > settings, resolved by the parent `up`); `None` ⇒ OS
    /// default opener. See [`crate::browser`].
    pub browser: Option<String>,
    /// Master gate for browser auto-open. The parent `up` sets this from the
    /// headless/CI/TTY check (the daemon's own stdio is `/dev/null`, so it can't
    /// evaluate `is_terminal()` itself).
    pub auto_open: bool,
}

// ---------------------------------------------------------------------------
// Process detachment / signaling (Unix)
// ---------------------------------------------------------------------------

/// Detach the current process from its controlling terminal and redirect the
/// standard fds onto the per-container log file.
///
/// Calls `setsid()` (new session — a closing terminal's SIGHUP no longer kills
/// us) then `dup2`s stdin from `/dev/null` and stdout/stderr onto `log`. Safe
/// `nix` wrappers keep this `unsafe`-free.
#[cfg(unix)]
pub fn daemonize(log_path: &Path) -> Result<()> {
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| PortForwardError::io(format!("create log dir {}", parent.display()), e))?;
    }

    // New session: detach from the controlling terminal. Fails with EPERM only
    // if we are already a process-group leader (we are not — we were just
    // spawned), so a failure here is non-fatal for detachment purposes.
    let _ = nix::unistd::setsid();

    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|e| PortForwardError::io(format!("open log {}", log_path.display()), e))?;
    let devnull = std::fs::OpenOptions::new()
        .read(true)
        .open("/dev/null")
        .map_err(|e| PortForwardError::io("open /dev/null", e))?;

    nix::unistd::dup2_stdin(&devnull)
        .map_err(|e| PortForwardError::io("dup2 stdin", std::io::Error::from(e)))?;
    nix::unistd::dup2_stdout(&log)
        .map_err(|e| PortForwardError::io("dup2 stdout", std::io::Error::from(e)))?;
    nix::unistd::dup2_stderr(&log)
        .map_err(|e| PortForwardError::io("dup2 stderr", std::io::Error::from(e)))?;
    Ok(())
}

/// Non-Unix builds do not support the detached forwarder (v1 is Unix-only).
#[cfg(not(unix))]
pub fn daemonize(_log_path: &Path) -> Result<()> {
    Err(PortForwardError::Docker {
        context: "daemonize".to_string(),
        message: "port forwarding is not supported on this platform (Unix-only in v1)".to_string(),
    })
}

/// Whether a process id is currently alive (`kill(pid, 0)` semantics).
#[cfg(unix)]
pub fn pid_alive(pid: u32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    // Signal `None` performs error checking without actually sending a signal.
    // Ok(()) ⇒ alive; ESRCH ⇒ gone; EPERM ⇒ exists but not ours (still alive).
    match kill(Pid::from_raw(pid as i32), None) {
        Ok(()) => true,
        Err(nix::errno::Errno::EPERM) => true,
        Err(_) => false,
    }
}

/// Non-Unix: liveness checks are unsupported; treat as not-alive.
#[cfg(not(unix))]
pub fn pid_alive(_pid: u32) -> bool {
    false
}

/// Send `SIGTERM` to a forwarder process so it can run its own cleanup.
#[cfg(unix)]
pub fn terminate_pid(pid: u32) -> Result<()> {
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;
    match kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
        Ok(()) => Ok(()),
        // Already gone — nothing to do.
        Err(nix::errno::Errno::ESRCH) => Ok(()),
        Err(e) => Err(PortForwardError::io(
            "kill SIGTERM",
            std::io::Error::from(e),
        )),
    }
}

/// Non-Unix: signaling is unsupported.
#[cfg(not(unix))]
pub fn terminate_pid(_pid: u32) -> Result<()> {
    Err(PortForwardError::Docker {
        context: "terminate_pid".to_string(),
        message: "port forwarding is not supported on this platform (Unix-only in v1)".to_string(),
    })
}

// ---------------------------------------------------------------------------
// Supervisor loop
// ---------------------------------------------------------------------------

/// A forward that currently has a host listener bound and a relay task running.
struct ActiveForward {
    spec: ForwardSpec,
    host_port: u16,
    task: tokio::task::JoinHandle<()>,
}

/// Build the `ExecConfig` used for silent probe execs.
fn silent_exec() -> ExecConfig {
    ExecConfig {
        user: None,
        working_dir: None,
        env: HashMap::new(),
        tty: false,
        interactive: false,
        detach: false,
        silent: true,
        stdout_to_stderr: false,
        terminal_size: None,
    }
}

/// Run the forwarder supervisor loop until the container vanishes or `SIGTERM`.
///
/// Eager-binds declared ports, polls listening sockets every [`POLL_INTERVAL`],
/// reconciles auto-detected ports, and cleans up (releases ports + removes the
/// marker) on exit (FR-002, FR-004, FR-015, FR-024).
pub async fn run(config: DaemonConfig) -> Result<()> {
    let udf = config.user_data_folder.as_deref();
    let runtime = CliRuntime::with_runtime_path(config.docker_path.clone());

    // Resolve port attributes from the devcontainer config (best-effort).
    let cfg = load_config(config.config_path.as_deref()).await;

    // Parse declared port specs (eager forwards).
    let declared: Vec<ForwardSpec> = config
        .declared_ports
        .iter()
        .filter_map(|s| parse_declared_spec(s, cfg.as_ref()))
        .collect();
    let declared_ports: HashSet<u16> = declared.iter().map(|s| s.container_port).collect();

    // Determine the relay strategy up front; fail fast & loud if none (FR-019).
    let relay_program = detect_relay_program(&runtime, &config.container_id).await?;

    // Record the marker only once we know we can forward.
    write_marker(&config)?;
    info!(
        container_id = %config.container_id,
        relay = ?relay_program,
        declared = declared.len(),
        "forwarder ready"
    );

    let mut active: HashMap<u16, ActiveForward> = HashMap::new();
    // Container ports already browser-opened in this daemon's lifetime. Never
    // cleared on withdraw, so `openBrowserOnce` holds across listener restarts
    // within the session (resets only when the daemon/container is recreated).
    let mut opened: HashSet<u16> = HashSet::new();

    // Eager-bind declared ports (Reserved → Active once the container listens).
    for spec in &declared {
        if spec.attributes.on_auto_forward == OnAutoForward::Ignore {
            continue;
        }
        match start_forward(spec.clone(), relay_program, &config, &mut opened).await {
            Ok(af) => {
                active.insert(spec.container_port, af);
            }
            Err(e) => warn!(port = spec.container_port, error = %e, "failed to bind declared port"),
        }
    }

    // Reconcile loop with SIGTERM handling.
    #[cfg(unix)]
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|e| PortForwardError::io("install SIGTERM handler", e))?;

    loop {
        #[cfg(unix)]
        {
            tokio::select! {
                _ = sigterm.recv() => {
                    info!("forwarder received SIGTERM; shutting down");
                    break;
                }
                _ = tokio::time::sleep(POLL_INTERVAL) => {}
            }
        }
        #[cfg(not(unix))]
        tokio::time::sleep(POLL_INTERVAL).await;

        // Self-exit when the container is gone (FR-015).
        match runtime.inspect_container(&config.container_id).await {
            Ok(None) => {
                info!("container gone; forwarder self-exiting");
                break;
            }
            Ok(Some(_)) => {}
            // Transient inspect failure — keep going rather than tearing down.
            Err(e) => {
                warn!(error = %e, "inspect failed; continuing");
                continue;
            }
        }

        let detected = match probe_ports(&runtime, &config.container_id).await {
            Ok(d) => d,
            Err(e) => {
                warn!(error = %e, "port probe failed; continuing");
                continue;
            }
        };

        reconcile(
            &mut active,
            &mut opened,
            &detected,
            &declared_ports,
            cfg.as_ref(),
            relay_program,
            &config,
        )
        .await;
    }

    // Cleanup: abort relays, release this container's ports, remove the marker.
    for (_, af) in active.drain() {
        af.task.abort();
    }
    if let Err(e) = registry::release_container(udf, &config.container_id) {
        warn!(error = %e, "failed to release registry entries on exit");
    }
    if let Err(e) = remove_marker(&config) {
        warn!(error = %e, "failed to remove marker on exit");
    }
    Ok(())
}

/// Reconcile detected-vs-active forwards: lazily bind newly-observed undeclared
/// ports; withdraw auto-detected forwards whose container port stopped
/// listening (FR-004, FR-024). Declared ports stay reserved regardless.
async fn reconcile(
    active: &mut HashMap<u16, ActiveForward>,
    opened: &mut HashSet<u16>,
    detected: &[DetectedPort],
    declared_ports: &HashSet<u16>,
    cfg: Option<&DevContainerConfig>,
    relay_program: relay::RelayProgram,
    config: &DaemonConfig,
) {
    let detected_ports: HashSet<u16> = detected.iter().map(|d| d.port).collect();

    // Add newly-observed undeclared ports.
    for d in detected {
        if active.contains_key(&d.port) || declared_ports.contains(&d.port) {
            continue;
        }
        let attributes = resolve_attributes(cfg, d.port);
        if attributes.on_auto_forward == OnAutoForward::Ignore {
            continue;
        }
        let spec = ForwardSpec {
            container_port: d.port,
            origin: ForwardOrigin::AutoDetected,
            service: None,
            attributes,
            eager: false,
        };
        match start_forward(spec, relay_program, config, opened).await {
            Ok(af) => {
                active.insert(d.port, af);
            }
            Err(e) => warn!(port = d.port, error = %e, "failed to forward detected port"),
        }
    }

    // Withdraw auto-detected forwards whose container port stopped listening.
    let stale: Vec<u16> = active
        .iter()
        .filter(|(port, af)| {
            af.spec.origin == ForwardOrigin::AutoDetected && !detected_ports.contains(port)
        })
        .map(|(port, _)| *port)
        .collect();
    for port in stale {
        if let Some(af) = active.remove(&port) {
            af.task.abort();
            let udf = config.user_data_folder.as_deref();
            if let Err(e) = registry::release_port(udf, af.host_port) {
                warn!(error = %e, "failed to release withdrawn port");
            }
            report_unforward(&af, config.ports_events);
        }
    }
}

/// Decide whether to open a browser for this action, given whether the
/// container port has already been opened in this daemon's lifetime.
/// `openBrowser` always opens; `openBrowserOnce` opens only the first time;
/// everything else (including `openPreview`, which has no CLI analog) does not.
fn should_open(action: &OnAutoForward, already_opened: bool) -> bool {
    match action {
        OnAutoForward::OpenBrowser => true,
        OnAutoForward::OpenBrowserOnce => !already_opened,
        _ => false,
    }
}

/// Best-effort browser auto-open for a freshly-live forward. Inserts the port
/// into `opened` BEFORE spawning so a broken browser binary can't retry on every
/// poll tick. Never fails the daemon.
async fn maybe_open_browser(
    spec: &ForwardSpec,
    host_port: u16,
    config: &DaemonConfig,
    opened: &mut HashSet<u16>,
) {
    if !config.auto_open
        || !should_open(
            &spec.attributes.on_auto_forward,
            opened.contains(&spec.container_port),
        )
    {
        return;
    }
    let scheme = spec.attributes.protocol.as_deref().unwrap_or("http");
    let url = format!("{scheme}://{DEFAULT_DIAL_HOST}:{host_port}");
    opened.insert(spec.container_port);
    match crate::browser::open_url(config.browser.as_deref(), &url).await {
        Ok(()) => info!(url = %url, "opened browser for forwarded port"),
        Err(e) => debug!(error = %e, url = %url, "browser auto-open failed (best-effort)"),
    }
}

/// Allocate + bind a host port, start the relay task, and report the mapping.
async fn start_forward(
    spec: ForwardSpec,
    relay_program: relay::RelayProgram,
    config: &DaemonConfig,
    opened: &mut HashSet<u16>,
) -> Result<ActiveForward> {
    let udf = config.user_data_folder.as_deref();
    let workspace = config.workspace.display().to_string();
    let alloc = registry::allocate(
        udf,
        &config.container_id,
        spec.container_port,
        &workspace,
        spec.attributes.label.as_deref(),
    )?;

    alloc
        .listener
        .set_nonblocking(true)
        .map_err(|e| PortForwardError::io("set listener nonblocking", e))?;
    let listener = tokio::net::TcpListener::from_std(alloc.listener)
        .map_err(|e| PortForwardError::io("convert listener to tokio", e))?;

    let dial_host = spec
        .service
        .clone()
        .unwrap_or_else(|| DEFAULT_DIAL_HOST.to_string());
    let relay_argv = relay::relay_args(relay_program, &dial_host, spec.container_port);

    let task = tokio::spawn(relay::serve(
        listener,
        config.docker_path.clone(),
        config.container_id.clone(),
        relay_argv,
    ));

    report_forward(&spec, alloc.host_port, alloc.remapped, config.ports_events);
    maybe_open_browser(&spec, alloc.host_port, config, opened).await;

    Ok(ActiveForward {
        spec,
        host_port: alloc.host_port,
        task,
    })
}

/// Probe `/proc/net/tcp{,6}` inside the container and parse listening ports.
async fn probe_ports(runtime: &CliRuntime, container_id: &str) -> Result<Vec<DetectedPort>> {
    let cmd = [
        "cat".to_string(),
        "/proc/net/tcp".to_string(),
        "/proc/net/tcp6".to_string(),
    ];
    // A missing /proc/net/tcp6 makes `cat` exit non-zero but still prints tcp;
    // parse whatever stdout we captured rather than failing the whole probe.
    let res = runtime
        .exec(container_id, &cmd, silent_exec())
        .await
        .map_err(|e| PortForwardError::Docker {
            context: "probe /proc/net/tcp".to_string(),
            message: e.to_string(),
        })?;
    Ok(detect::parse_proc_net_tcp(&res.stdout))
}

/// Detect the best available in-container relay program (FR-019).
async fn detect_relay_program(
    runtime: &CliRuntime,
    container_id: &str,
) -> Result<relay::RelayProgram> {
    let cmd = [
        "sh".to_string(),
        "-c".to_string(),
        relay::probe_script().to_string(),
    ];
    let res = runtime
        .exec(container_id, &cmd, silent_exec())
        .await
        .map_err(|e| PortForwardError::Docker {
            context: "probe relay program".to_string(),
            message: e.to_string(),
        })?;
    relay::select_relay_from_probe(&res.stdout)
        .ok_or_else(|| relay::no_relay_strategy(container_id))
}

/// Load the devcontainer config for port attributes (best-effort).
async fn load_config(config_path: Option<&Path>) -> Option<DevContainerConfig> {
    let path = config_path?;
    match ConfigLoader::load_with_extends(path).await {
        Ok(cfg) => Some(cfg),
        Err(e) => {
            warn!(path = %path.display(), error = %e, "failed to load config for port attributes");
            None
        }
    }
}

/// Parse a declared port spec (`PORT`, `HOST:CONTAINER`, or `service:port`).
fn parse_declared_spec(spec: &str, cfg: Option<&DevContainerConfig>) -> Option<ForwardSpec> {
    let (service, container_port) = match spec.rsplit_once(':') {
        Some((lhs, rhs)) => {
            let port = rhs.parse::<u16>().ok()?;
            if lhs.is_empty() || lhs.parse::<u16>().is_ok() {
                // "PORT:" or "HOST:CONTAINER" — forward the container port.
                (None, port)
            } else {
                // "service:port" — compose service-qualified.
                (Some(lhs.to_string()), port)
            }
        }
        None => (None, spec.parse::<u16>().ok()?),
    };
    let attributes = resolve_attributes(cfg, container_port);
    Some(ForwardSpec {
        container_port,
        origin: ForwardOrigin::Declared,
        service,
        attributes,
        eager: true,
    })
}

/// Resolve effective attributes: `portsAttributes[port]` else
/// `otherPortsAttributes` else implicit `Notify` (FR-017).
fn resolve_attributes(cfg: Option<&DevContainerConfig>, port: u16) -> ResolvedPortAttributes {
    let Some(cfg) = cfg else {
        return ResolvedPortAttributes::default();
    };
    let key_plain = port.to_string();
    let key_tcp = format!("{port}/tcp");
    if let Some(pa) = cfg
        .ports_attributes
        .get(&key_plain)
        .or_else(|| cfg.ports_attributes.get(&key_tcp))
    {
        return to_resolved(pa);
    }
    if let Some(other) = &cfg.other_ports_attributes {
        return to_resolved(other);
    }
    ResolvedPortAttributes::default()
}

/// Convert a [`PortAttributes`] into [`ResolvedPortAttributes`], preserving the
/// real `onAutoForward` action (so `openBrowser`/`openBrowserOnce` can drive
/// auto-open) and carrying the `protocol` hint for the URL/PORT_EVENT scheme.
fn to_resolved(pa: &PortAttributes) -> ResolvedPortAttributes {
    ResolvedPortAttributes {
        label: pa.label.clone(),
        on_auto_forward: pa.on_auto_forward.clone().unwrap_or(OnAutoForward::Notify),
        protocol: pa.protocol.clone(),
    }
}

/// Print the human-readable forward mapping to stderr (unless `silent`) and,
/// when `--ports-events` is set, emit a `PORT_EVENT:` line (FR-010, FR-020).
fn report_forward(spec: &ForwardSpec, host_port: u16, remapped: bool, ports_events: bool) {
    if spec.attributes.on_auto_forward != OnAutoForward::Silent {
        let mut parts: Vec<String> = Vec::new();
        if let Some(label) = &spec.attributes.label {
            parts.push(label.clone());
        }
        if remapped {
            if spec.container_port < 1024 {
                parts.push("remapped; privileged port".to_string());
            } else {
                parts.push(format!("remapped; host {} in use", spec.container_port));
            }
        }
        let suffix = if parts.is_empty() {
            String::new()
        } else {
            format!(" ({})", parts.join("; "))
        };
        eprintln!(
            "Forwarding container {} -> http://127.0.0.1:{}{}",
            spec.container_port, host_port, suffix
        );
    }
    if ports_events {
        emit_port_event(&port_event(spec, Some(host_port), true));
    }
}

/// Report a withdrawn forward (FR-004, FR-020).
fn report_unforward(af: &ActiveForward, ports_events: bool) {
    if af.spec.attributes.on_auto_forward != OnAutoForward::Silent {
        eprintln!(
            "Unforwarded container {} (server stopped)",
            af.spec.container_port
        );
    }
    if ports_events {
        emit_port_event(&port_event(&af.spec, Some(af.host_port), false));
    }
}

/// Build a [`PortEvent`] for a forward/unforward transition.
fn port_event(spec: &ForwardSpec, host_port: Option<u16>, forwarded: bool) -> PortEvent {
    PortEvent {
        port: spec.container_port,
        protocol: spec.attributes.protocol.clone(),
        label: spec.attributes.label.clone(),
        on_auto_forward: Some(spec.attributes.on_auto_forward.clone()),
        auto_forwarded: forwarded,
        local_port: host_port,
        host_ip: Some(DEFAULT_DIAL_HOST.to_string()),
        description: None,
        open_preview: None,
        require_local_port: None,
        elevate_if_needed: None,
    }
}

/// Emit a `PORT_EVENT:` line to stdout (the daemon's stdout is its log file).
fn emit_port_event(event: &PortEvent) {
    match serde_json::to_string(event) {
        Ok(json) => println!("PORT_EVENT: {json}"),
        Err(e) => warn!(error = %e, "failed to serialize port event"),
    }
}

/// Write the per-container marker atomically (temp file + rename).
fn write_marker(config: &DaemonConfig) -> Result<()> {
    let udf = config.user_data_folder.as_deref();
    let path = marker_path(udf, &config.container_id)?;
    let log = log_path(udf, &config.container_id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            PortForwardError::io(format!("create marker dir {}", parent.display()), e)
        })?;
    }
    let marker = DaemonMarker {
        pid: std::process::id(),
        container_id: config.container_id.clone(),
        workspace: config.workspace.display().to_string(),
        started_at: chrono::Utc::now().to_rfc3339(),
        log_path: log.display().to_string(),
    };
    let content =
        serde_json::to_string_pretty(&marker).map_err(|e| PortForwardError::serde("marker", e))?;
    let tmp = path.with_extension(format!("pid.tmp.{}", std::process::id()));
    std::fs::write(&tmp, content)
        .map_err(|e| PortForwardError::io(format!("write marker temp {}", tmp.display()), e))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| PortForwardError::io(format!("publish marker {}", path.display()), e))?;
    Ok(())
}

/// Remove the per-container marker (idempotent).
fn remove_marker(config: &DaemonConfig) -> Result<()> {
    let path = marker_path(config.user_data_folder.as_deref(), &config.container_id)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(PortForwardError::io(
            format!("remove marker {}", path.display()),
            e,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PortAttributes;

    fn attrs(label: Option<&str>, oaf: OnAutoForward) -> PortAttributes {
        PortAttributes {
            label: label.map(str::to_string),
            on_auto_forward: Some(oaf),
            protocol: None,
            open_preview: None,
            require_local_port: None,
            elevate_if_needed: None,
            description: None,
        }
    }

    fn config_with_attrs(
        ports: Vec<(&str, OnAutoForward)>,
        other: Option<OnAutoForward>,
    ) -> DevContainerConfig {
        let mut cfg = DevContainerConfig::default();
        for (key, oaf) in ports {
            cfg.ports_attributes
                .insert(key.to_string(), attrs(Some(&format!("label-{key}")), oaf));
        }
        cfg.other_ports_attributes = other.map(|oaf| attrs(None, oaf));
        cfg
    }

    #[test]
    fn declared_port_uses_ports_attributes() {
        let cfg = config_with_attrs(vec![("3000", OnAutoForward::Silent)], None);
        let attrs = resolve_attributes(Some(&cfg), 3000);
        assert_eq!(attrs.on_auto_forward, OnAutoForward::Silent);
        assert_eq!(attrs.label.as_deref(), Some("label-3000"));
    }

    #[test]
    fn ports_attributes_key_with_protocol_suffix_matches() {
        let cfg = config_with_attrs(vec![("8080/tcp", OnAutoForward::Ignore)], None);
        let attrs = resolve_attributes(Some(&cfg), 8080);
        assert_eq!(attrs.on_auto_forward, OnAutoForward::Ignore);
    }

    #[test]
    fn undeclared_falls_back_to_other_ports_attributes_then_notify() {
        let cfg = config_with_attrs(vec![], Some(OnAutoForward::Silent));
        assert_eq!(
            resolve_attributes(Some(&cfg), 9999).on_auto_forward,
            OnAutoForward::Silent
        );
        // No other_ports_attributes ⇒ implicit Notify.
        let cfg2 = config_with_attrs(vec![], None);
        assert_eq!(
            resolve_attributes(Some(&cfg2), 9999).on_auto_forward,
            OnAutoForward::Notify
        );
        // No config at all ⇒ Notify.
        assert_eq!(
            resolve_attributes(None, 9999).on_auto_forward,
            OnAutoForward::Notify
        );
    }

    #[test]
    fn resolve_preserves_open_actions_and_protocol() {
        // The real action is no longer collapsed to Notify (auto-open needs it).
        for action in [
            OnAutoForward::OpenBrowser,
            OnAutoForward::OpenBrowserOnce,
            OnAutoForward::OpenPreview,
        ] {
            let cfg = config_with_attrs(vec![("3000", action.clone())], None);
            assert_eq!(resolve_attributes(Some(&cfg), 3000).on_auto_forward, action);
        }
        // protocol hint is carried through for the URL/PORT_EVENT scheme.
        let mut cfg = DevContainerConfig::default();
        let mut pa = attrs(None, OnAutoForward::OpenBrowser);
        pa.protocol = Some("https".to_string());
        cfg.ports_attributes.insert("3000".to_string(), pa);
        assert_eq!(
            resolve_attributes(Some(&cfg), 3000).protocol.as_deref(),
            Some("https")
        );
    }

    #[test]
    fn should_open_semantics() {
        // openBrowser: always.
        assert!(should_open(&OnAutoForward::OpenBrowser, false));
        assert!(should_open(&OnAutoForward::OpenBrowser, true));
        // openBrowserOnce: only the first time.
        assert!(should_open(&OnAutoForward::OpenBrowserOnce, false));
        assert!(!should_open(&OnAutoForward::OpenBrowserOnce, true));
        // everything else (incl. openPreview): never opens.
        for a in [
            OnAutoForward::Notify,
            OnAutoForward::Silent,
            OnAutoForward::Ignore,
            OnAutoForward::OpenPreview,
        ] {
            assert!(!should_open(&a, false));
            assert!(!should_open(&a, true));
        }
    }

    #[test]
    fn parse_declared_specs_plain_host_and_service() {
        // Plain port.
        let s = parse_declared_spec("3000", None).unwrap();
        assert_eq!(s.container_port, 3000);
        assert_eq!(s.service, None);
        assert_eq!(s.origin, ForwardOrigin::Declared);
        assert!(s.eager);

        // HOST:CONTAINER → forward the container port, no service.
        let s = parse_declared_spec("8080:80", None).unwrap();
        assert_eq!(s.container_port, 80);
        assert_eq!(s.service, None);

        // service:port → compose service-qualified.
        let s = parse_declared_spec("db:5432", None).unwrap();
        assert_eq!(s.container_port, 5432);
        assert_eq!(s.service.as_deref(), Some("db"));

        // Garbage → None.
        assert!(parse_declared_spec("not-a-port", None).is_none());
    }

    #[test]
    fn ignore_attribute_is_preserved_for_skipping() {
        let cfg = config_with_attrs(vec![("3000", OnAutoForward::Ignore)], None);
        let spec = parse_declared_spec("3000", Some(&cfg)).unwrap();
        assert_eq!(spec.attributes.on_auto_forward, OnAutoForward::Ignore);
    }
}
