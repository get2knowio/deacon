//! Workspace folder derivation helpers.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use deacon_core::config::DevContainerConfig;
use deacon_core::docker::Mount;

/// Resolve a subcommand's effective HOST workspace folder, defaulting to the
/// current directory when `--workspace-folder` is absent.
///
/// Reference parity ([#610], [#615]): with no `--workspace-folder`, the workspace is
/// the process's CURRENT DIRECTORY — the shape a developer types, `cd` into the
/// project and run `deacon <subcommand>`. That is what the reference CLI does, and
/// what `exec`, `build`, `down` and `run-user-commands` already did before either
/// issue (the first three inherit it from `shared::config_loader::load_config`,
/// which falls back to `current_dir()`). `up` (#610) and then `read-configuration`
/// (#615) were the two that demanded the flag and rejected the invocation with
/// `Missing required argument: …` before ever looking at the cwd.
///
/// Callers materialize the default at their OWN entry point rather than leaving it
/// to `load_config`, and that placement is the load-bearing part. A subcommand that
/// reads `args.workspace_folder` anywhere other than the loader — `up` does on five
/// further paths (argument validation, container identity hashing, the
/// workspace-trust gate, the `--mount-workspace-git-root` mount-source walk and
/// compose project naming); `read-configuration` does for
/// `resolve_workspace_configuration`, `ContainerIdentity`, feature-path anchoring and
/// every `SubstitutionContext` it builds — would otherwise see `None` on exactly the
/// paths that never consult the loader. Materializing once, up front, is what makes a
/// defaulted cwd indistinguishable from an explicit `--workspace-folder $(pwd)` on
/// every one of them.
///
/// Whether the path came in explicitly or from the cwd, it is **absolutized, never
/// canonicalized** ([#665]): the reference preserves the path the user named, and the
/// spec defines `${localWorkspaceFolder}` as the folder *that was opened*. `exec` and
/// `up` both route through here so their container identities agree on a symlinked
/// workspace — the earlier arrangement, where `read-configuration` kept an explicit
/// `--workspace-folder` verbatim while `up` canonicalized it, was the two halves of that
/// contract disagreeing.
///
/// [#610]: https://github.com/get2knowio/deacon/issues/610
/// [#615]: https://github.com/get2knowio/deacon/issues/615
/// [#665]: https://github.com/get2knowio/deacon/issues/665
pub(crate) fn resolve_workspace_folder(explicit: Option<PathBuf>) -> Result<PathBuf> {
    let ws = match explicit {
        Some(ws) => ws,
        None => std::env::current_dir()
            .context("Failed to resolve the current directory as the default workspace folder")?,
    };
    // Absolutized, never canonicalized: the reference preserves the path the user named and
    // the spec defines `${localWorkspaceFolder}` as the folder *that was opened* (#665).
    // The existence check the old `canonicalize()` gave for free is kept explicitly, since
    // a workspace that does not exist is still a user error worth failing on.
    if !ws.exists() {
        anyhow::bail!(
            "Failed to resolve workspace path '{}': path does not exist or cannot be accessed",
            ws.display()
        );
    }
    Ok(deacon_core::workspace::absolutize(&ws))
}

/// Recover the container workspace folder from a RUNNING container's actual
/// workspace bind-mount, instead of re-deriving it host-side from the
/// `--mount-workspace-git-root` flag.
///
/// Re-deriving host-side is fragile: subcommands disagree when their flags differ
/// (e.g. `up --mount-workspace-git-root false` then `exec`/`run-user-commands`
/// with the default), so the derived cwd doesn't match where `up` mounted and a
/// `chdir` into it fails. Reading the container's real mount is flag-independent —
/// it reflects exactly what `up` did, which is what the reference CLI's
/// `remoteWorkspaceFolder` encodes.
///
/// Precedence:
///   1. An explicit `config.workspaceFolder` — used verbatim (the reference does
///      the same; it's the authored value, independent of any mount).
///   2. The workspace bind mount: the bind mount whose host `source` is an
///      ancestor-or-equal of `host_workspace_folder` (the most specific one when
///      several match), joined with the source→workspace subpath onto its
///      container `destination`.
///
/// Returns `None` when neither applies (no explicit folder and no matching bind
/// mount — e.g. a volume-workspace or an unreadable container), so the caller can
/// fall back to [`derive_container_workspace_folder`].
pub fn container_workspace_folder_from_mounts(
    config: &DevContainerConfig,
    host_workspace_folder: &Path,
    mounts: &[Mount],
) -> Option<String> {
    if let Some(explicit) = config.workspace_folder.as_deref() {
        return Some(explicit.to_string());
    }

    let host = host_workspace_folder
        .canonicalize()
        .unwrap_or_else(|_| host_workspace_folder.to_path_buf());

    // Pick the bind mount with the LONGEST (most specific) source that contains
    // the host workspace, so nested mounts resolve to the innermost one.
    let mut best: Option<(&Mount, String)> = None;
    let mut best_len = 0usize;
    for m in mounts {
        if m.mount_type != "bind" {
            continue;
        }
        let Some(src) = m.source.as_deref() else {
            continue;
        };
        let src_canon = Path::new(src)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(src));
        if let Ok(sub) = host.strip_prefix(&src_canon) {
            let len = src_canon.as_os_str().len();
            if best.is_none() || len > best_len {
                best_len = len;
                // Container paths are POSIX; a Windows host subpath uses `\`.
                best = Some((m, sub.to_string_lossy().replace('\\', "/")));
            }
        }
    }

    let (m, sub) = best?;
    if sub.is_empty() {
        Some(m.destination.clone())
    } else {
        Some(format!("{}/{}", m.destination.trim_end_matches('/'), sub))
    }
}

/// Resolve the container working directory for `exec` / `run-user-commands` /
/// lifecycle, applying the full reference-matching precedence:
///   1. explicit `config.workspaceFolder`, or the running container's actual
///      workspace bind-mount (both via [`container_workspace_folder_from_mounts`]);
///   2. for a **Compose** config with no explicit `workspaceFolder`, `/` — the
///      reference's effective Compose workspace, and always a valid `chdir`
///      target (deacon previously used the single-container default
///      `/workspaces/<basename>`, which the Compose service doesn't mount, so
///      `exec`/lifecycle `chdir` failed with rc 127 — issues #294/#295);
///   3. the single-container host-side derivation
///      (`/workspaces/<basename(root)>[/<subpath>]`) — but ONLY when the container
///      corroborates it by actually having a mount there, which is what keeps the
///      volume-workspace case working;
///   4. otherwise `None`, meaning "this container has no workspace" — the caller
///      falls back to the container user's home folder.
///
/// Step 4 exists because step 3 is a claim about a container `up` created, and
/// `--container-id` can name one it did not (#655). A plain `docker run` target has
/// no mount at `/workspaces/<basename(host cwd)>`, so `chdir`-ing there fails the
/// exec outright with rc 127 — silently, in `run-user-commands`, where a
/// non-blocking phase still reports success. The reference's rule is
/// `remoteCwd = remoteWorkspaceFolder || homeFolder`, and returning `Option` is what
/// makes the second half of it reachable: before this, every config-bearing
/// invocation produced *some* path and the home-folder branch could never run.
/// Same failure mode as #294/#295, which is why step 2 exists; this is a second
/// trigger for it.
pub fn resolve_container_cwd(
    config: &DevContainerConfig,
    host_workspace_folder: &Path,
    mounts: &[Mount],
    mount_workspace_git_root: bool,
) -> Option<String> {
    if let Some(folder) =
        container_workspace_folder_from_mounts(config, host_workspace_folder, mounts)
    {
        return Some(folder);
    }
    if config.uses_compose() {
        return Some("/".to_string());
    }
    let derived =
        derive_container_workspace_folder(config, host_workspace_folder, mount_workspace_git_root);
    // A workspace mounted as a VOLUME leaves no bind mount for step 1 to match, so
    // the derived path is still right — the container has a mount whose destination
    // IS that path. A container deacon did not create has nothing there.
    mounts
        .iter()
        .any(|m| m.destination == derived)
        .then_some(derived)
}

/// Derive the container workspace folder (the lifecycle & exec working directory)
/// from configuration and the host workspace path.
///
/// Delegates to [`deacon_core::workspace::container_workspace_folder`] so the used
/// working dir matches `read-configuration` and the reference CLI (issue #309): an
/// explicit `workspaceFolder` wins verbatim, otherwise `/workspaces/<basename(root)>
/// [/<subpath>]` where `root` is the git root when `mount_workspace_git_root` is
/// set (else the workspace folder), with the root→workspace subpath appended. This
/// keeps the working dir on the actual mounted path for git-subdir workspaces
/// instead of a `/workspaces/<userFolderBasename>` that does not exist.
pub fn derive_container_workspace_folder(
    config: &deacon_core::config::DevContainerConfig,
    workspace_folder: &Path,
    mount_workspace_git_root: bool,
) -> String {
    deacon_core::workspace::container_workspace_folder(
        workspace_folder,
        config.workspace_folder.as_deref(),
        mount_workspace_git_root,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn minimal_config() -> deacon_core::config::DevContainerConfig {
        deacon_core::config::DevContainerConfig::default()
    }

    #[test]
    fn test_uses_config_workspace_folder_when_set() {
        let mut config = minimal_config();
        config.workspace_folder = Some("/custom/path".to_string());
        let host_path = PathBuf::from("/home/user/my-project");

        let result = derive_container_workspace_folder(&config, &host_path, true);
        assert_eq!(result, "/custom/path");
    }

    #[test]
    fn test_derives_from_host_path() {
        let config = minimal_config();
        let host_path = PathBuf::from("/home/user/my-project");

        let result = derive_container_workspace_folder(&config, &host_path, false);
        assert_eq!(result, "/workspaces/my-project");
    }

    #[test]
    fn test_falls_back_to_workspace_for_root_path() {
        let config = minimal_config();
        let host_path = PathBuf::from("/");

        let result = derive_container_workspace_folder(&config, &host_path, false);
        assert_eq!(result, "/workspaces/workspace");
    }

    // --- container_workspace_folder_from_mounts (mount-based recovery) ---
    // Synthetic (non-existent) paths canonicalize to themselves, so strip_prefix
    // works on the literal paths.

    fn bind(source: &str, dest: &str) -> Mount {
        Mount {
            mount_type: "bind".to_string(),
            source: Some(source.to_string()),
            destination: dest.to_string(),
            mode: None,
            rw: None,
            propagation: None,
            name: None,
            driver: None,
        }
    }

    #[test]
    fn from_mounts_explicit_workspace_folder_wins() {
        let mut config = minimal_config();
        config.workspace_folder = Some("/custom/wsf".to_string());
        // Even with a contradicting mount, the explicit folder is used verbatim.
        let mounts = vec![bind("/host/proj", "/workspaces/proj")];
        let got = container_workspace_folder_from_mounts(&config, Path::new("/host/proj"), &mounts);
        assert_eq!(got.as_deref(), Some("/custom/wsf"));
    }

    #[test]
    fn from_mounts_source_equals_workspace_returns_destination() {
        // Mirrors `up --mount-workspace-git-root false`: the workspace folder
        // itself is mounted, so the container cwd is the mount destination.
        let config = minimal_config();
        let mounts = vec![bind(
            "/host/examples/up-exec-down",
            "/workspaces/up-exec-down",
        )];
        let got = container_workspace_folder_from_mounts(
            &config,
            Path::new("/host/examples/up-exec-down"),
            &mounts,
        );
        assert_eq!(got.as_deref(), Some("/workspaces/up-exec-down"));
    }

    #[test]
    fn from_mounts_git_root_mount_appends_subpath() {
        // Mirrors the default (git-root) mount: the git root is mounted and the
        // workspace is a subdir, so the cwd is destination + subpath.
        let config = minimal_config();
        let mounts = vec![bind("/host/repo", "/workspaces/repo")];
        let got = container_workspace_folder_from_mounts(
            &config,
            Path::new("/host/repo/examples/up-exec-down"),
            &mounts,
        );
        assert_eq!(
            got.as_deref(),
            Some("/workspaces/repo/examples/up-exec-down")
        );
    }

    #[test]
    fn from_mounts_prefers_most_specific_source() {
        // A nested bind mount (deeper source) wins over the enclosing one.
        let config = minimal_config();
        let mounts = vec![
            bind("/host/repo", "/workspaces/repo"),
            bind("/host/repo/pkg", "/pkg"),
        ];
        let got =
            container_workspace_folder_from_mounts(&config, Path::new("/host/repo/pkg"), &mounts);
        assert_eq!(got.as_deref(), Some("/pkg"));
    }

    fn compose_config() -> DevContainerConfig {
        let mut c = minimal_config();
        c.docker_compose_file = Some(serde_json::json!("docker-compose.yml"));
        c.service = Some("app".to_string());
        c
    }

    #[test]
    fn cwd_compose_without_workspace_folder_is_root() {
        // Reference default for a Compose config without an explicit workspaceFolder
        // is `/` (a valid chdir target), NOT `/workspaces/<basename>` (#294/#295).
        let config = compose_config();
        assert!(config.uses_compose());
        let got = resolve_container_cwd(&config, Path::new("/host/my-project"), &[], false);
        assert_eq!(got.as_deref(), Some("/"));
    }

    #[test]
    fn cwd_compose_honors_explicit_workspace_folder() {
        let mut config = compose_config();
        config.workspace_folder = Some("/workspaces/compose-basic".to_string());
        let got = resolve_container_cwd(&config, Path::new("/host/my-project"), &[], false);
        assert_eq!(got.as_deref(), Some("/workspaces/compose-basic"));
    }

    #[test]
    fn cwd_single_container_uses_workspaces_basename_when_the_container_mounts_it() {
        // Non-compose without an explicit folder keeps the single-container default —
        // but only because the container corroborates it. A workspace mounted as a
        // VOLUME leaves no bind mount for the mount lookup to match, yet the derived
        // path is where `up` put it, so the destination check is what preserves it.
        let config = minimal_config();
        let mut vol = bind("workspace-volume", "/workspaces/my-project");
        vol.mount_type = "volume".to_string();
        let got = resolve_container_cwd(&config, Path::new("/host/my-project"), &[vol], false);
        assert_eq!(got.as_deref(), Some("/workspaces/my-project"));
    }

    #[test]
    fn cwd_is_none_for_a_container_that_has_no_workspace() {
        // #655: `--container-id` can name a container deacon did not create. Nothing
        // is mounted at the derived path, so there is no workspace and the caller
        // must fall back to the container user's home rather than `chdir`-ing into a
        // directory that does not exist (rc 127).
        let config = minimal_config();
        assert_eq!(
            resolve_container_cwd(&config, Path::new("/host/my-project"), &[], false),
            None
        );
        // A foreign container with unrelated mounts is still not this workspace.
        let mounts = vec![bind("/host/other", "/opt/other")];
        assert_eq!(
            resolve_container_cwd(&config, Path::new("/host/my-project"), &mounts, false),
            None
        );
    }

    #[test]
    fn cwd_authored_workspace_folder_wins_without_any_mount() {
        // An authored `workspaceFolder` is the caller's own claim and is honored
        // verbatim, mount or no mount — measured on both CLIs (#655).
        let mut config = minimal_config();
        config.workspace_folder = Some("/etc".to_string());
        let got = resolve_container_cwd(&config, Path::new("/host/my-project"), &[], false);
        assert_eq!(got.as_deref(), Some("/etc"));
    }

    #[test]
    fn cwd_prefers_workspace_mount_over_compose_root() {
        // A Compose service that DOES mount the workspace resolves from the mount,
        // not the `/` fallback.
        let config = compose_config();
        let mounts = vec![bind("/host/my-project", "/workspaces/my-project")];
        let got = resolve_container_cwd(&config, Path::new("/host/my-project"), &mounts, false);
        assert_eq!(got.as_deref(), Some("/workspaces/my-project"));
    }

    #[test]
    fn from_mounts_none_when_no_matching_bind() {
        let config = minimal_config();
        // A volume mount (not bind) and an unrelated bind mount → no match.
        let mut vol = bind("some-volume", "/data");
        vol.mount_type = "volume".to_string();
        let mounts = vec![vol, bind("/other/place", "/elsewhere")];
        let got = container_workspace_folder_from_mounts(&config, Path::new("/host/proj"), &mounts);
        assert_eq!(got, None);
    }
}
