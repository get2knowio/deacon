//! Host-global host-port allocation registry (`forwarded_ports.json`).
//!
//! Allocations live in `{user_data_folder}/forwarded_ports.json`, shared by
//! every forwarder on the host. The allocate-and-bind critical section is
//! serialized with an advisory `fs2` `flock` on a sibling lock file, and
//! writes use the atomic temp-file + `fs::rename` pattern (mirrors
//! `cache/disk.rs::save_index`). Every `host_port` is unique file-wide
//! (FR-008 / SC-004).

use std::fs::File;
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use super::daemon::pid_alive;
use super::{PortForwardError, RegistryEntry, Result, registry_path};

/// Lowest unprivileged host port we will allocate (never require host root).
const MIN_HOST_PORT: u16 = 1024;

/// On-disk shape of `forwarded_ports.json` (binding contract —
/// `contracts/registry.schema.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegistryFile {
    version: u32,
    entries: Vec<RegistryEntry>,
}

impl Default for RegistryFile {
    fn default() -> Self {
        RegistryFile {
            version: 1,
            entries: Vec::new(),
        }
    }
}

/// The result of allocating (and binding) a host port for a forward.
#[derive(Debug)]
pub struct Allocation {
    /// The allocated loopback host port (always >= 1024).
    pub host_port: u16,
    /// `true` if `host_port != container_port` (drives the remap report).
    pub remapped: bool,
    /// The reserved listener, already bound to `127.0.0.1:host_port`.
    pub listener: TcpListener,
}

/// Acquire the advisory lock guarding the allocate-and-bind critical section.
///
/// The lock auto-releases when `lock` is dropped (or the process dies), which
/// is the crash-safety the multi-container requirement needs.
fn lock(user_data_folder: Option<&Path>) -> Result<File> {
    let reg = registry_path(user_data_folder)?;
    if let Some(parent) = reg.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            PortForwardError::io(format!("create registry dir {}", parent.display()), e)
        })?;
    }
    let lock_path = reg.with_extension("lock");
    let file = File::create(&lock_path)
        .map_err(|e| PortForwardError::io(format!("create lock {}", lock_path.display()), e))?;
    FileExt::lock_exclusive(&file).map_err(|e| PortForwardError::io("acquire registry lock", e))?;
    Ok(file)
}

/// Load the registry file (empty default when absent).
fn load(user_data_folder: Option<&Path>) -> Result<RegistryFile> {
    let path = registry_path(user_data_folder)?;
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| PortForwardError::serde(format!("parse {}", path.display()), e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(RegistryFile::default()),
        Err(e) => Err(PortForwardError::io(format!("read {}", path.display()), e)),
    }
}

/// Atomically persist the registry (temp file + `fs::rename`).
fn save(user_data_folder: Option<&Path>, registry: &RegistryFile) -> Result<()> {
    let path = registry_path(user_data_folder)?;
    let dir = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let content = serde_json::to_string_pretty(registry)
        .map_err(|e| PortForwardError::serde("serialize registry", e))?;

    // Unique temp name (pid + counter) so concurrent writers don't clobber each
    // other's staging file before the rename.
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = dir.join(format!(
        "forwarded_ports.json.tmp.{}.{}",
        std::process::id(),
        seq
    ));
    std::fs::write(&tmp, content)
        .map_err(|e| PortForwardError::io(format!("write temp {}", tmp.display()), e))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| PortForwardError::io(format!("publish {}", path.display()), e))?;
    Ok(())
}

/// Remove entries whose owning forwarder process is dead (FR-016).
///
/// Returns `true` if any entry was removed. Container-existence pruning is the
/// daemon's responsibility (it has the Docker handle); here we prune by pid so
/// a crashed forwarder's ports become reusable host-wide.
fn prune_dead(registry: &mut RegistryFile) -> bool {
    let before = registry.entries.len();
    registry.entries.retain(|e| pid_alive(e.pid));
    registry.entries.len() != before
}

/// Try to reserve `port` on loopback: fail if already in the registry or not
/// bindable. On success returns the held listener.
fn try_reserve(registry: &RegistryFile, port: u16) -> Option<TcpListener> {
    if registry.entries.iter().any(|e| e.host_port == port) {
        return None;
    }
    TcpListener::bind(("127.0.0.1", port)).ok()
}

/// Allocate (and bind) a collision-free loopback host port for a forward.
///
/// Prefers the same number as `container_port` when free in both the registry
/// and an actual bind probe; otherwise the next free port. Privileged
/// container ports (<1024) always remap to >= 1024 (FR-009a). The whole
/// allocate-and-bind sequence runs under the advisory lock so concurrent
/// forwarders never collide (FR-008, FR-011).
pub fn allocate(
    user_data_folder: Option<&Path>,
    container_id: &str,
    container_port: u16,
    workspace: &str,
    label: Option<&str>,
) -> Result<Allocation> {
    let _guard = lock(user_data_folder)?;
    let mut registry = load(user_data_folder)?;
    let dirty = prune_dead(&mut registry);

    // Prefer the same number when unprivileged and free.
    let mut chosen: Option<(u16, TcpListener)> = None;
    if container_port >= MIN_HOST_PORT {
        if let Some(listener) = try_reserve(&registry, container_port) {
            chosen = Some((container_port, listener));
        }
    }
    // Otherwise scan upward for the next free, bindable, unregistered port.
    if chosen.is_none() {
        let start = container_port.max(MIN_HOST_PORT);
        for candidate in start..=u16::MAX {
            if candidate < MIN_HOST_PORT {
                continue;
            }
            if let Some(listener) = try_reserve(&registry, candidate) {
                chosen = Some((candidate, listener));
                break;
            }
        }
    }

    let (host_port, listener) = chosen.ok_or_else(|| PortForwardError::Docker {
        context: "allocate".to_string(),
        message: format!("no free host port available for container port {container_port}"),
    })?;

    registry.entries.push(RegistryEntry {
        host_port,
        container_id: container_id.to_string(),
        container_port,
        workspace: workspace.to_string(),
        pid: std::process::id(),
        label: label.map(str::to_string),
    });
    let _ = dirty; // entries always change here, so we always save.
    save(user_data_folder, &registry)?;

    Ok(Allocation {
        host_port,
        remapped: host_port != container_port,
        listener,
    })
}

/// Release a single host-port entry (used when a forward is withdrawn).
pub fn release_port(user_data_folder: Option<&Path>, host_port: u16) -> Result<()> {
    let _guard = lock(user_data_folder)?;
    let mut registry = load(user_data_folder)?;
    let before = registry.entries.len();
    registry.entries.retain(|e| e.host_port != host_port);
    if registry.entries.len() != before {
        save(user_data_folder, &registry)?;
    }
    Ok(())
}

/// Release all of a container's entries (reap / self-exit, FR-013 / FR-015).
pub fn release_container(user_data_folder: Option<&Path>, container_id: &str) -> Result<()> {
    let _guard = lock(user_data_folder)?;
    let mut registry = load(user_data_folder)?;
    let before = registry.entries.len();
    registry.entries.retain(|e| e.container_id != container_id);
    if registry.entries.len() != before {
        save(user_data_folder, &registry)?;
    }
    Ok(())
}

/// Prune entries whose pid is dead **or** whose container is reported gone by
/// `container_alive` (FR-016). Runs under the lock.
pub fn prune(
    user_data_folder: Option<&Path>,
    container_alive: impl Fn(&str) -> bool,
) -> Result<()> {
    let _guard = lock(user_data_folder)?;
    let mut registry = load(user_data_folder)?;
    let before = registry.entries.len();
    registry
        .entries
        .retain(|e| pid_alive(e.pid) && container_alive(&e.container_id));
    if registry.entries.len() != before {
        save(user_data_folder, &registry)?;
    }
    Ok(())
}

/// Read the current entries (test/diagnostic helper; takes the lock briefly).
pub fn entries(user_data_folder: Option<&Path>) -> Result<Vec<RegistryEntry>> {
    let _guard = lock(user_data_folder)?;
    Ok(load(user_data_folder)?.entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn udf(dir: &TempDir) -> Option<&Path> {
        Some(dir.path())
    }

    /// An ephemeral port that is currently free (the listener is dropped
    /// immediately, so `allocate` can re-bind the same number).
    fn free_port() -> u16 {
        TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    #[test]
    fn prefers_same_number_when_free() {
        let dir = TempDir::new().unwrap();
        let port = free_port();
        let alloc = allocate(udf(&dir), "c1", port, "/ws/a", Some("web")).unwrap();
        assert_eq!(alloc.host_port, port);
        assert!(!alloc.remapped);
        let entries = entries(udf(&dir)).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].host_port, port);
        assert_eq!(entries[0].label.as_deref(), Some("web"));
    }

    #[test]
    fn collision_triggers_next_free_remap() {
        let dir = TempDir::new().unwrap();
        let port = free_port();
        // First container takes the natural port.
        let a = allocate(udf(&dir), "c1", port, "/ws/a", None).unwrap();
        assert_eq!(a.host_port, port);
        // Second container also wants it → remapped to next free.
        let b = allocate(udf(&dir), "c2", port, "/ws/b", None).unwrap();
        assert_ne!(b.host_port, port);
        assert!(b.remapped);
        assert!(b.host_port >= MIN_HOST_PORT);
        // Both registered, host ports unique.
        let entries = entries(udf(&dir)).unwrap();
        assert_eq!(entries.len(), 2);
        assert_ne!(entries[0].host_port, entries[1].host_port);
        // Keep listeners alive until here so the ports stay reserved.
        drop(a);
        drop(b);
    }

    #[test]
    fn privileged_port_is_remapped() {
        let dir = TempDir::new().unwrap();
        let a = allocate(udf(&dir), "c1", 80, "/ws/a", None).unwrap();
        assert!(a.host_port >= MIN_HOST_PORT);
        assert!(a.remapped);
    }

    #[test]
    fn release_removes_entries() {
        let dir = TempDir::new().unwrap();
        let a = allocate(udf(&dir), "c1", free_port(), "/ws/a", None).unwrap();
        drop(a); // free the listener so the port is reusable
        release_container(udf(&dir), "c1").unwrap();
        assert!(entries(udf(&dir)).unwrap().is_empty());
    }

    #[test]
    fn stale_dead_pid_entries_are_pruned_on_allocate() {
        let dir = TempDir::new().unwrap();
        let port = free_port();
        // Seed a registry with an entry owned by a definitely-dead pid, holding
        // the natural port.
        let stale = RegistryFile {
            version: 1,
            entries: vec![RegistryEntry {
                host_port: port,
                container_id: "ghost".to_string(),
                container_port: port,
                workspace: "/ws/ghost".to_string(),
                pid: 2_000_000_000, // not a live pid
                label: None,
            }],
        };
        save(udf(&dir), &stale).unwrap();
        // Allocating the same container port should prune the stale entry and
        // reuse the natural port.
        let a = allocate(udf(&dir), "c1", port, "/ws/a", None).unwrap();
        assert_eq!(a.host_port, port);
        let entries = entries(udf(&dir)).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].container_id, "c1");
    }

    #[test]
    fn prune_drops_missing_containers() {
        let dir = TempDir::new().unwrap();
        let a = allocate(udf(&dir), "alive", free_port(), "/ws/a", None).unwrap();
        // Prune treating the container as gone → entry removed.
        prune(udf(&dir), |_| false).unwrap();
        assert!(entries(udf(&dir)).unwrap().is_empty());
        drop(a);
    }
}
