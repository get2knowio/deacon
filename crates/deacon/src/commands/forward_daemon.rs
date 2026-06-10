//! Entrypoint for the hidden `__forward-daemon` subcommand.
//!
//! `up --auto-forward` re-exec's the deacon binary with this subcommand to
//! spawn a detached forwarder. The process detaches from its controlling
//! terminal (`setsid`), reopens its stdio onto a per-container log file, and
//! then runs the core supervisor loop. It is **not** part of the user-facing
//! surface; end users must not invoke it directly.

use std::path::PathBuf;

use anyhow::Context;
use tracing::info;

use deacon_core::port_forward::daemon::{self, DaemonConfig};
use deacon_core::port_forward::log_path;

/// Parsed arguments for the hidden daemon subcommand.
#[derive(Debug, Clone)]
pub struct ForwardDaemonArgs {
    /// Full id of the container to forward.
    pub container_id: String,
    /// Canonical workspace path.
    pub workspace: PathBuf,
    /// User-data folder for registry/marker/log (default `~/.deacon`).
    pub user_data_folder: Option<PathBuf>,
    /// Raw declared-port specs to forward eagerly.
    pub declared_ports: Vec<String>,
    /// Path to the resolved devcontainer.json.
    pub config_path: Option<PathBuf>,
    /// Emit machine-readable `PORT_EVENT:` lines.
    pub ports_events: bool,
    /// Docker CLI path.
    pub docker_path: String,
}

/// Detach, redirect stdio to the per-container log, and run the supervisor loop.
pub async fn run_forward_daemon(args: ForwardDaemonArgs) -> anyhow::Result<()> {
    // Resolve the log file and detach. On a non-Unix build `daemonize` returns
    // a clear unsupported-platform error (Principle IV — no silent fallback).
    let log = log_path(args.user_data_folder.as_deref(), &args.container_id)
        .context("resolve forwarder log path")?;
    daemon::daemonize(&log).context("detach forwarder process")?;

    // After daemonize, stdout/stderr point at the log file, so existing
    // tracing (to stderr) and any PORT_EVENT lines (to stdout) land there.
    info!(
        container_id = %args.container_id,
        workspace = %args.workspace.display(),
        "forward daemon starting"
    );

    let config = DaemonConfig {
        container_id: args.container_id,
        workspace: args.workspace,
        user_data_folder: args.user_data_folder,
        declared_ports: args.declared_ports,
        config_path: args.config_path,
        ports_events: args.ports_events,
        docker_path: args.docker_path,
    };

    daemon::run(config)
        .await
        .context("forwarder supervisor loop")?;
    Ok(())
}
