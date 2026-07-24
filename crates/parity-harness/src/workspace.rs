//! Isolated external workspaces + guaranteed resource cleanup for Docker-backed cases
//! (research D10, T052, FR-036/037/039).
//!
//! Each Docker case runs in an isolated external temp workspace ([`tempfile`]) with a
//! collision-resistant run id. Because deacon derives its container identity (and the
//! `devcontainer.local_folder` label) from the workspace path, a unique temp workspace
//! yields non-colliding container/network/volume names for free — two concurrent cases
//! never collide (FR-037). [`DockerWorkspace`] is an RAII cleanup GUARD: its `Drop`
//! reclaims every resource — `deacon down`, a container sweep by the workspace label, and
//! any tracked images/networks/volumes — on success AND on unwind (panic / early return),
//! then the temp dir removes itself (FR-039). Cleanup is synchronous + best-effort (Drop
//! cannot be async and must never itself panic).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tempfile::TempDir;

/// Process-wide monotonic counter → the collision-resistant run-id suffix.
static RUN_SEQ: AtomicU64 = AtomicU64::new(0);

/// An isolated external temp workspace for a Docker-backed case, and the RAII guard that
/// reclaims its Docker resources on drop.
#[derive(Debug)]
pub struct DockerWorkspace {
    /// Auto-removed on drop (after Docker cleanup).
    tempdir: TempDir,
    /// Collision-resistant id, unique per process across concurrent cases.
    run_id: String,
    /// `deacon` binary path for `down` (best-effort); `None` skips the down call.
    deacon_path: Option<PathBuf>,
    /// Image tags to `docker rmi -f` on cleanup.
    images: Vec<String>,
    /// Network names to `docker network rm` on cleanup.
    networks: Vec<String>,
    /// Volume names to `docker volume rm -f` on cleanup.
    volumes: Vec<String>,
    /// Set once cleanup has run so `Drop` does not double-reclaim.
    reclaimed: bool,
}

impl DockerWorkspace {
    /// Create an isolated temp workspace with a collision-resistant run id. `deacon_path`
    /// is used for `deacon down` at cleanup; pass `None` to rely on the label sweep only.
    pub fn new(deacon_path: Option<&Path>) -> std::io::Result<DockerWorkspace> {
        let tempdir = tempfile::Builder::new().prefix("deacon-conf-").tempdir()?;
        let seq = RUN_SEQ.fetch_add(1, Ordering::Relaxed);
        let run_id = format!("dcr-{}-{seq}", std::process::id());
        Ok(DockerWorkspace {
            tempdir,
            run_id,
            deacon_path: deacon_path.map(Path::to_path_buf),
            images: Vec::new(),
            networks: Vec::new(),
            volumes: Vec::new(),
            reclaimed: false,
        })
    }

    /// The isolated workspace directory (the `--workspace-folder` for the case's ops).
    pub fn path(&self) -> &Path {
        self.tempdir.path()
    }

    /// The collision-resistant run id — unique across concurrent cases in this process.
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// A collision-resistant resource name of the form `<run-id>-<kind>` for any resource
    /// the case names explicitly (network, volume, built image tag).
    pub fn resource_name(&self, kind: &str) -> String {
        format!("{}-{kind}", self.run_id)
    }

    /// Materialize a fixture directory tree into the workspace (recursive copy). Repeated
    /// calls layer fixtures into the same workspace.
    pub fn materialize(&self, fixture_dir: &Path) -> std::io::Result<()> {
        copy_tree(fixture_dir, self.tempdir.path())
    }

    /// Track a built image tag for removal at cleanup.
    pub fn track_image(&mut self, tag: impl Into<String>) {
        self.images.push(tag.into());
    }

    /// Track a network name for removal at cleanup.
    pub fn track_network(&mut self, name: impl Into<String>) {
        self.networks.push(name.into());
    }

    /// Track a volume name for removal at cleanup.
    pub fn track_volume(&mut self, name: impl Into<String>) {
        self.volumes.push(name.into());
    }

    /// Explicitly reclaim all Docker resources now (idempotent). `Drop` calls this too,
    /// so a test can invoke it and then assert zero residual resources.
    pub fn cleanup_now(&mut self) {
        self.reclaim();
    }

    /// Best-effort synchronous resource reclamation (never panics). Order: `deacon down`
    /// (removes deacon's container + its network/volumes for this workspace), then a
    /// label sweep for any straggler containers, then tracked images/networks/volumes.
    fn reclaim(&mut self) {
        if self.reclaimed {
            return;
        }
        self.reclaimed = true;
        let ws = self.tempdir.path().to_string_lossy().into_owned();

        if let Some(deacon) = &self.deacon_path {
            let _ = std::process::Command::new(deacon)
                .args(["down", "--remove", "--workspace-folder", &ws])
                .current_dir(self.tempdir.path())
                .output();
        }

        // Sweep any container still labeled with THIS workspace (collision-safe — the
        // workspace path is unique).
        let list = std::process::Command::new("docker")
            .args([
                "ps",
                "-aq",
                "--filter",
                &format!("label=devcontainer.local_folder={ws}"),
            ])
            .output();
        if let Ok(out) = list {
            if out.status.success() {
                for id in String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                {
                    let _ = std::process::Command::new("docker")
                        .args(["rm", "-f", id])
                        .output();
                }
            }
        }

        for image in &self.images {
            let _ = std::process::Command::new("docker")
                .args(["rmi", "-f", image])
                .output();
        }
        for network in &self.networks {
            let _ = std::process::Command::new("docker")
                .args(["network", "rm", network])
                .output();
        }
        for volume in &self.volumes {
            let _ = std::process::Command::new("docker")
                .args(["volume", "rm", "-f", volume])
                .output();
        }
    }
}

impl Drop for DockerWorkspace {
    fn drop(&mut self) {
        // RAII guarantee: reclaim on success AND on unwind (panic / early return). The
        // TempDir field drops after this, removing the workspace directory (FR-039).
        self.reclaim();
    }
}

/// Recursively copy `src`'s contents into `dst` (creating `dst`), preserving the tree.
fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_ids_are_collision_resistant() {
        let a = DockerWorkspace::new(None).expect("workspace a");
        let b = DockerWorkspace::new(None).expect("workspace b");
        assert_ne!(a.run_id(), b.run_id(), "concurrent run ids must differ");
        assert_ne!(a.path(), b.path(), "temp workspaces must be distinct");
        // Names derived from run ids are also distinct.
        assert_ne!(a.resource_name("net"), b.resource_name("net"));
    }

    #[test]
    fn materialize_copies_the_fixture_tree() {
        let fixture = tempfile::tempdir().expect("fixture");
        std::fs::create_dir_all(fixture.path().join(".devcontainer")).unwrap();
        std::fs::write(fixture.path().join(".devcontainer/devcontainer.json"), "{}").unwrap();
        let ws = DockerWorkspace::new(None).expect("workspace");
        ws.materialize(fixture.path()).expect("materialize");
        assert!(ws.path().join(".devcontainer/devcontainer.json").is_file());
    }

    #[test]
    fn tempdir_is_removed_on_drop() {
        let path = {
            let ws = DockerWorkspace::new(None).expect("workspace");
            ws.path().to_path_buf()
        };
        assert!(!path.exists(), "the temp workspace is removed on drop");
    }
}
