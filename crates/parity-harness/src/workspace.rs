//! Isolated external workspaces + guaranteed resource cleanup for cases that must not run
//! against the committed fixture tree (research D10, T052, FR-036/037/039).
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
//!
//! An **`fs-heavy`** case gets the same isolated temp workspace through
//! [`DockerWorkspace::new_filesystem_only`], with Docker reclamation switched off. Its
//! group means "significant filesystem operations, no Docker" — and *significant
//! filesystem operations* is exactly the thing that must not happen inside
//! `parity/fixtures/`, which is version-controlled input shared by every other case.
//! Running such a case in place would leave the repository dirty and let one case's writes
//! become the next case's input. Reclaiming Docker for it, on the other hand, would make
//! the config-only lane shell out to a daemon it is defined not to need.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tempfile::TempDir;

/// Process-wide monotonic counter → the collision-resistant run-id suffix.
static RUN_SEQ: AtomicU64 = AtomicU64::new(0);

/// An isolated external temp workspace for a Docker-backed case, and the RAII guard that
/// reclaims its Docker resources on drop.
#[derive(Debug)]
pub struct DockerWorkspace {
    /// Held for its `Drop` alone: auto-removes the tree (after Docker cleanup). The
    /// workspace directory lives INSIDE it; nothing reads this field.
    _tempdir: TempDir,
    /// The lowercase-named workspace directory the case actually runs in (see `new` —
    /// Compose lowercases project names, so the basename must already be lowercase for
    /// the name-marker sweeps and the reclamation guard's predicate to match).
    workspace: PathBuf,
    /// Collision-resistant id, unique per process across concurrent cases.
    run_id: String,
    /// `deacon` binary path for `down` (best-effort); `None` skips the down call.
    deacon_path: Option<PathBuf>,
    /// Whether cleanup touches Docker at all (the label sweep + tracked resources).
    /// `false` for a filesystem-only workspace, whose lane has no daemon to talk to.
    reclaim_docker: bool,
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

        // The WORKSPACE is a lowercase-named child of the random tempdir, not the tempdir
        // itself (#442, the fourth and root cause of the leak family). Docker Compose
        // LOWERCASES its project name, and the reference CLI derives that project from the
        // workspace directory's basename — so a mixed-case `tempfile` basename
        // (`deacon-conf-0GWl5d`) yields Compose resources named for a basename that exists
        // nowhere (`deacon-conf-0gwl5d_default`), which every name-marker sweep here and
        // the reclamation guard's workspace-exists predicate then compare CASE-SENSITIVELY
        // and miss. Measured live: the leaked network's
        // `com.docker.compose.project.working_dir` was the mixed-case dir while every
        // resource name was its lowercase fold. A lowercase basename makes the project
        // name EQUAL the basename, and every existing comparison is correct as written.
        // The parent tempdir still provides collision-free randomness (its suffix is
        // folded into the child's name; a case-folded collision needs two live parents
        // agreeing on 6 alphanumerics modulo case) and removes the child on drop.
        let parent_name = tempdir
            .path()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let suffix = parent_name
            .strip_prefix("deacon-conf-")
            .unwrap_or(&parent_name)
            .to_lowercase();
        let workspace = tempdir.path().join(format!("deacon-conf-{suffix}-{seq}"));
        std::fs::create_dir(&workspace)?;

        Ok(DockerWorkspace {
            _tempdir: tempdir,
            workspace,
            run_id,
            deacon_path: deacon_path.map(Path::to_path_buf),
            reclaim_docker: true,
            images: Vec::new(),
            networks: Vec::new(),
            volumes: Vec::new(),
            reclaimed: false,
        })
    }

    /// An isolated temp workspace whose cleanup is the temp dir and nothing else — for an
    /// `fs-heavy` case, which writes to its workspace but creates no container.
    ///
    /// Deliberately a separate constructor rather than a boolean on [`new`](Self::new):
    /// the two differ in whether cleanup may shell out to `docker`, and that is a property
    /// of the lane a case runs in, not a tuning knob. The config-only lanes are defined
    /// to need no daemon, so a workspace they create must not try to reach one.
    pub fn new_filesystem_only() -> std::io::Result<DockerWorkspace> {
        let mut ws = DockerWorkspace::new(None)?;
        ws.reclaim_docker = false;
        Ok(ws)
    }

    /// The isolated workspace directory (the `--workspace-folder` for the case's ops).
    pub fn path(&self) -> &Path {
        &self.workspace
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
        copy_tree(fixture_dir, &self.workspace)
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
        let ws = self.workspace.to_string_lossy().into_owned();

        if let Some(deacon) = &self.deacon_path {
            let _ = std::process::Command::new(deacon)
                .args(["down", "--remove", "--workspace-folder", &ws])
                .current_dir(&self.workspace)
                .output();
        }

        if !self.reclaim_docker {
            return;
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

        // A Compose project the REFERENCE created in this workspace is not deacon's to tear
        // down. The reference derives its project name from the workspace DIRECTORY
        // (`<basename>_devcontainer`) while deacon derives `deacon_<workspaceHash>_<configHash>`,
        // so `deacon down` never sees the reference's project and neither does the container
        // label sweep above reach its NETWORK and VOLUMES — only its containers carry
        // `devcontainer.local_folder`.
        //
        // Left alone they accumulate across runs until the daemon answers a compose `up`
        // with "all predefined address pools have been fully subnetted", and the tier fails
        // in whichever compose case happened to run next. That is a leak presenting as a
        // flake in an unrelated case, which is the worst shape a leak can take.
        //
        // Swept by NAME rather than by label because a compose network carries only
        // `com.docker.compose.project`, not the working directory: every resource compose
        // creates is named `<project>_<resource>`, and this workspace's basename
        // (`deacon-conf-<unique>`) appears in the project name of any project rooted here and
        // in no other. Same collision-safety the container sweep relies on (FR-037).
        //
        // CONTAINERS must join the name sweep, not only networks and volumes (#442's third
        // and final half). A Compose SIDECAR (`<project>-db-1`) carries no
        // `devcontainer.local_folder` label — only the primary container does — so the
        // label sweep above never removes it, and while it exists its network's removal
        // fails with "active endpoints" on EVERY retry: a permanent leak no settle window
        // or retry loop can close, which is exactly the shape two nightly runs measured
        // after those two fixes landed. Containers go first for the same dependency
        // reason networks precede volumes below.
        self.sweep_containers_by_name_marker();
        self.sweep_compose_leftovers();

        for image in &self.images {
            let _ = std::process::Command::new("docker")
                .args(["rmi", "-f", image])
                .output();
        }
        for network in &self.networks {
            remove_docker_resource_with_retry("network", network, false);
        }
        for volume in &self.volumes {
            remove_docker_resource_with_retry("volume", volume, true);
        }
    }

    /// Remove every network and volume whose name carries this workspace's unique basename
    /// — the Compose resources the reference side leaves behind (see `reclaim`).
    ///
    /// Best-effort and never panics, like the rest of `reclaim`. Networks go first: a volume
    /// removal does not depend on a network, but a network removal fails while a container
    /// still attaches to it, and the container sweep has already run by this point.
    fn sweep_compose_leftovers(&self) {
        let Some(marker) = self
            .workspace
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
        else {
            return;
        };
        for kind in ["network", "volume"] {
            let list = std::process::Command::new("docker")
                .args([kind, "ls", "--format", "{{.Name}}"])
                .output();
            let Ok(out) = list else { continue };
            if !out.status.success() {
                continue;
            }
            let listing = String::from_utf8_lossy(&out.stdout);
            for name in compose_leftovers(&listing, &marker) {
                remove_docker_resource_with_retry(kind, name, kind == "volume");
            }
        }
    }

    /// Remove every CONTAINER whose name carries this workspace's unique basename — the
    /// Compose containers the reference side creates, which the label sweep cannot see
    /// (#442).
    ///
    /// Only the PRIMARY container of a devcontainer Compose project carries
    /// `devcontainer.local_folder`; a sidecar service's container
    /// (`<project>-<service>-1`) carries Compose's own labels and nothing of ours, so the
    /// only handle on it is its NAME — which embeds the project, which embeds this
    /// workspace's basename, unique by construction (FR-037). Without this sweep a
    /// surviving sidecar pins its network's endpoints and the network removal below fails
    /// on every retry: the permanent-leak shape two nightlies reproduced after the settle
    /// window and the removal retry had both landed.
    fn sweep_containers_by_name_marker(&self) {
        let Some(marker) = self
            .workspace
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
        else {
            return;
        };
        let list = std::process::Command::new("docker")
            .args(["ps", "-aq", "--filter", &format!("name={marker}")])
            .output();
        let Ok(out) = list else { return };
        if !out.status.success() {
            return;
        }
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

/// Remove a network/volume, retrying briefly on failure (#442's persistent half).
///
/// `docker rm -f <container>` returns before the daemon has detached the container's
/// endpoints, so a `network rm` issued right after the container sweep can fail with
/// "active endpoints" — and a swallowed single-shot failure is a PERMANENT leak: the
/// workspace directory is gone by the next observation, so the orphaned network survives
/// every later sweep keyed on it and eventually exhausts the daemon's address pools.
/// Bounded (a few attempts over ~5s), checks between attempts whether the resource is
/// already gone (an "rm" of a nonexistent name also fails, and retrying THAT would turn
/// the common already-clean path into a five-second stall), and never panics — this runs
/// on the RAII drop path, unwind included.
fn remove_docker_resource_with_retry(kind: &str, name: &str, force: bool) {
    for attempt in 0..5u64 {
        if attempt > 0 {
            // Only worth waiting if the resource still exists; a failed `rm` of a name
            // the daemon no longer knows is success in different clothes.
            let exists = std::process::Command::new("docker")
                .args([kind, "inspect", name])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if !exists {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(500 * attempt));
        }
        let mut cmd = std::process::Command::new("docker");
        cmd.arg(kind).arg("rm");
        if force {
            cmd.arg("-f");
        }
        if let Ok(out) = cmd.arg(name).output() {
            if out.status.success() {
                return;
            }
        }
    }
}

/// The names in a `docker <network|volume> ls` listing that belong to a Compose project
/// rooted in the workspace whose directory basename is `marker`.
///
/// Split out from the sweep so the SELECTION is testable without a daemon: matching too
/// widely here would delete a concurrent case's live resources, and matching too narrowly
/// restores the leak. Compose names every resource `<project>_<resource>`, and the
/// workspace basename is unique per run, so containment is both sufficient and safe.
///
/// **Matched case-INSENSITIVELY, which is not a nicety.** A Compose project name is
/// lowercased, and `tempfile` mixes case in its suffix (`deacon-conf-6HzJJt` becomes
/// project `deacon-conf-6hzjjt`), so an exact match silently selects nothing and leaves the
/// leak exactly where it was — with a sweep in place that looks like it is working.
fn compose_leftovers<'a>(listing: &'a str, marker: &str) -> Vec<&'a str> {
    let marker = marker.to_ascii_lowercase();
    listing
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && l.to_ascii_lowercase().contains(&marker))
        .collect()
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

    /// The reference's Compose project is named for the workspace DIRECTORY, so its network
    /// and volumes are reclaimed by name; a sibling workspace's resources are not touched.
    #[test]
    fn compose_leftovers_select_this_workspace_only() {
        let listing = "\
bridge
deacon-conf-aaaaaa_devcontainer_default
deacon-conf-bbbbbb_devcontainer_default
deacon-conf-aaaaaa_devcontainer_app-data
deacon_1444def5_d0eb6f3d_default
host

";
        let mine = compose_leftovers(listing, "deacon-conf-aaaaaa");
        assert_eq!(
            mine,
            vec![
                "deacon-conf-aaaaaa_devcontainer_default",
                "deacon-conf-aaaaaa_devcontainer_app-data",
            ],
            "both the network and the volume of THIS workspace's project are selected"
        );
        assert!(
            !mine.iter().any(|n| n.contains("bbbbbb")),
            "a concurrent workspace's live resources must never be swept (FR-037)"
        );
        assert!(
            compose_leftovers(listing, "deacon-conf-zzzzzz").is_empty(),
            "a workspace that created no Compose project sweeps nothing"
        );
        // The workspace directory keeps `tempfile`'s mixed-case suffix; the Compose project
        // derived from it is lowercased. An exact match here selects NOTHING and leaves the
        // leak in place behind a sweep that appears to work.
        assert_eq!(
            compose_leftovers(listing, "deacon-conf-AAAAAA").len(),
            2,
            "the project name is lowercased; the workspace basename is not"
        );
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
