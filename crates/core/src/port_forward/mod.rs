//! Dynamic, user-space port forwarding for `deacon up --auto-forward`.
//!
//! A detached forwarder process polls a running container's TCP LISTEN sockets
//! (`/proc/net/tcp{,6}` via `docker exec`) and, for each detected or declared
//! port, opens a `127.0.0.1:<host-port>` listener on the host that relays bytes
//! into the container's network namespace over `docker exec -i`. This reaches
//! `127.0.0.1`-bound container servers that static `-p` publishing cannot.
//!
//! This is a **deacon consumer extension** — it is not mandated by the upstream
//! containers.dev spec, but reuses its vocabulary (`portsAttributes`,
//! `forwardPorts`, `appPort`, compose `"service:port"`). TCP-only, loopback-only,
//! Unix-only in v1.
//!
//! Module layout (Principle V — modular boundaries):
//! - [`detect`]: pure `/proc/net/tcp{,6}` parser.
//! - [`registry`]: host-global host-port allocation registry (flock + atomic write).
//! - [`relay`]: per-connection byte relay over `docker exec -i`.
//! - [`daemon`]: the detached supervisor loop.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::OnAutoForward;

pub mod daemon;
pub mod detect;
pub mod registry;
pub mod relay;

/// Errors raised by the port-forwarding subsystem.
#[derive(thiserror::Error, Debug)]
pub enum PortForwardError {
    /// An IO error with human context (file path / operation).
    #[error("port-forward IO error ({context}): {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    /// JSON (de)serialization error for the registry / marker files.
    #[error("port-forward JSON error ({context}): {source}")]
    Serde {
        context: String,
        #[source]
        source: serde_json::Error,
    },

    /// The user's home directory could not be resolved and no
    /// `--user-data-folder` was supplied.
    #[error("could not determine user home directory for the port-forward registry")]
    HomeDirUnavailable,

    /// No usable relay program is available inside the container.
    #[error(
        "no port-forward relay strategy available in container {container_id}: \
         need one of an embedded relay, socat, nc, or bash /dev/tcp"
    )]
    NoRelayStrategy { container_id: String },

    /// A `docker` invocation (probe / relay / inspect) failed.
    #[error("docker error ({context}): {message}")]
    Docker { context: String, message: String },
}

impl PortForwardError {
    /// Helper: build an [`PortForwardError::Io`] with context.
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        PortForwardError::Io {
            context: context.into(),
            source,
        }
    }

    /// Helper: build a [`PortForwardError::Serde`] with context.
    pub fn serde(context: impl Into<String>, source: serde_json::Error) -> Self {
        PortForwardError::Serde {
            context: context.into(),
            source,
        }
    }
}

/// Result alias for the port-forwarding subsystem.
pub type Result<T> = std::result::Result<T, PortForwardError>;

/// The interface a LISTEN socket is bound to inside the container.
///
/// Informational only — both loopback and any-interface binds are forwarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindScope {
    /// `127.0.0.1` / `::1`.
    Loopback,
    /// `0.0.0.0` / `::`.
    AnyInterface,
}

/// Address family of a detected socket. Used only for v4/v6 dedup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpFamily {
    /// IPv4.
    V4,
    /// IPv6.
    V6,
}

/// One LISTEN socket observed inside the container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectedPort {
    /// Container-side listening port.
    pub port: u16,
    /// Interface scope of the bind (informational).
    pub bind_addr: BindScope,
    /// Address family (for dedup).
    pub family: IpFamily,
}

/// Where a forward intent originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwardOrigin {
    /// From `forwardPorts` / `appPort` / `--forward-port` — bound eagerly.
    Declared,
    /// Observed listening at runtime — bound lazily on first observation.
    AutoDetected,
}

/// Effective per-port forwarding preferences (subset of `PortAttributes`).
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPortAttributes {
    /// Human label for reporting.
    pub label: Option<String>,
    /// The real `onAutoForward` action. `openBrowser`/`openBrowserOnce` drive
    /// browser auto-open in the daemon; `openPreview` (no CLI analog) and the
    /// rest are surfaced but do not open a browser.
    pub on_auto_forward: OnAutoForward,
    /// `protocol` hint (`http`/`https`) used to build the auto-open URL and the
    /// PORT_EVENT scheme. `None` ⇒ `http`.
    pub protocol: Option<String>,
}

impl Default for ResolvedPortAttributes {
    fn default() -> Self {
        ResolvedPortAttributes {
            label: None,
            on_auto_forward: OnAutoForward::Notify,
            protocol: None,
        }
    }
}

/// The reconciled decision that a container port should be forwarded.
#[derive(Debug, Clone, PartialEq)]
pub struct ForwardSpec {
    /// Source port inside the container.
    pub container_port: u16,
    /// Declared vs auto-detected.
    pub origin: ForwardOrigin,
    /// For compose `"service:port"` declared ports; `None` = primary service.
    pub service: Option<String>,
    /// Effective `portsAttributes[port]` / `otherPortsAttributes` default.
    pub attributes: ResolvedPortAttributes,
    /// `true` for declared (bind host listener at startup), `false` for
    /// auto-detected (bind on first observation).
    pub eager: bool,
}

/// One row in the host-global allocation registry.
///
/// **This JSON shape is a binding contract** (`contracts/registry.schema.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// Allocated loopback host port (always >= 1024). Unique across the file.
    pub host_port: u16,
    /// Full id of the owning container.
    pub container_id: String,
    /// Source TCP port inside the container.
    pub container_port: u16,
    /// Canonical workspace path of the owning devcontainer.
    pub workspace: String,
    /// Process id of the owning forwarder (for stale-pruning).
    pub pid: u32,
    /// Effective port label from `portsAttributes`, if any.
    pub label: Option<String>,
}

/// Single-owner record proving a live forwarder exists for a container.
///
/// **Binding contract** (`contracts/marker.schema.json`). Despite the `.pid`
/// filename suffix it holds this JSON object, not a bare pid integer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonMarker {
    /// Forwarder process id.
    pub pid: u32,
    /// The container this forwarder owns.
    pub container_id: String,
    /// Canonical workspace path.
    pub workspace: String,
    /// RFC3339 timestamp the forwarder started (diagnostics only).
    pub started_at: String,
    /// Absolute path to this forwarder's per-container log file.
    pub log_path: String,
}

/// Resolve the user-data folder base directory.
///
/// Mirrors `trust.rs::trust_store_path`: `--user-data-folder` if supplied,
/// else `~/.deacon`.
fn user_data_base(user_data_folder: Option<&Path>) -> Result<PathBuf> {
    match user_data_folder {
        Some(p) => Ok(p.to_path_buf()),
        None => {
            let dirs =
                directories_next::BaseDirs::new().ok_or(PortForwardError::HomeDirUnavailable)?;
            Ok(dirs.home_dir().join(".deacon"))
        }
    }
}

/// Path to the host-global registry file `{user_data_folder}/forwarded_ports.json`.
pub fn registry_path(user_data_folder: Option<&Path>) -> Result<PathBuf> {
    Ok(user_data_base(user_data_folder)?.join("forwarded_ports.json"))
}

/// Path to a container's marker file
/// `{user_data_folder}/forward_daemon_<container_id>.pid`.
pub fn marker_path(user_data_folder: Option<&Path>, container_id: &str) -> Result<PathBuf> {
    Ok(user_data_base(user_data_folder)?.join(format!("forward_daemon_{container_id}.pid")))
}

/// Path to a container's forwarder log file
/// `{user_data_folder}/forward_daemon_<container_id>.log`.
pub fn log_path(user_data_folder: Option<&Path>, container_id: &str) -> Result<PathBuf> {
    Ok(user_data_base(user_data_folder)?.join(format!("forward_daemon_{container_id}.log")))
}

/// Reap a container's forwarder: `SIGTERM` the marked pid (so it runs its own
/// cleanup), then force-remove the marker and release its registry entries.
///
/// Idempotent and best-effort — used by `down` (FR-013) and by
/// `up --remove-existing-container` (FR-014). A missing marker is a no-op.
pub fn reap(user_data_folder: Option<&Path>, container_id: &str) -> Result<()> {
    let marker = marker_path(user_data_folder, container_id)?;
    let had_marker = marker.exists();
    if had_marker {
        if let Ok(text) = std::fs::read_to_string(&marker) {
            if let Ok(m) = serde_json::from_str::<DaemonMarker>(&text) {
                // SIGTERM lets the daemon release ports + remove its own marker.
                let _ = daemon::terminate_pid(m.pid);
            }
        }
        // Force-remove the marker regardless (belt-and-suspenders).
        let _ = std::fs::remove_file(&marker);
    }

    // Only touch the registry when there's something to clean — a marker
    // existed, or a registry file is present. This keeps a normal `down`
    // (auto-forward never used) from creating the registry/lock files.
    if had_marker || registry_path(user_data_folder)?.exists() {
        registry::release_container(user_data_folder, container_id)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_use_override_folder() {
        let base = Path::new("/tmp/custom-deacon");
        assert_eq!(
            registry_path(Some(base)).unwrap(),
            base.join("forwarded_ports.json")
        );
        assert_eq!(
            marker_path(Some(base), "abc123").unwrap(),
            base.join("forward_daemon_abc123.pid")
        );
        assert_eq!(
            log_path(Some(base), "abc123").unwrap(),
            base.join("forward_daemon_abc123.log")
        );
    }

    #[test]
    fn registry_entry_round_trips() {
        let entry = RegistryEntry {
            host_port: 3001,
            container_id: "abc123".to_string(),
            container_port: 3000,
            workspace: "/home/dev/app".to_string(),
            pid: 4242,
            label: Some("web".to_string()),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: RegistryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, back);
        // null label round-trips as None.
        assert!(json.contains("\"label\":\"web\""));
        let no_label = RegistryEntry {
            label: None,
            ..entry
        };
        let json = serde_json::to_string(&no_label).unwrap();
        assert!(json.contains("\"label\":null"));
    }

    #[test]
    fn marker_round_trips() {
        let marker = DaemonMarker {
            pid: 4242,
            container_id: "abc123".to_string(),
            workspace: "/home/dev/app".to_string(),
            started_at: "2026-06-08T14:03:22Z".to_string(),
            log_path: "/home/dev/.deacon/forward_daemon_abc123.log".to_string(),
        };
        let json = serde_json::to_string(&marker).unwrap();
        let back: DaemonMarker = serde_json::from_str(&json).unwrap();
        assert_eq!(marker, back);
    }
}
