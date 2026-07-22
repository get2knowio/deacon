//! Workspace folder derivation helpers.

use std::path::{Path, PathBuf};

use deacon_core::config::DevContainerConfig;
use deacon_core::docker::Mount;

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
///   3. otherwise the single-container host-side derivation
///      (`/workspaces/<basename(root)>[/<subpath>]`).
pub fn resolve_container_cwd(
    config: &DevContainerConfig,
    host_workspace_folder: &Path,
    mounts: &[Mount],
    mount_workspace_git_root: bool,
) -> String {
    if let Some(folder) =
        container_workspace_folder_from_mounts(config, host_workspace_folder, mounts)
    {
        return folder;
    }
    if config.uses_compose() {
        return "/".to_string();
    }
    derive_container_workspace_folder(config, host_workspace_folder, mount_workspace_git_root)
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
        assert_eq!(got, "/");
    }

    #[test]
    fn cwd_compose_honors_explicit_workspace_folder() {
        let mut config = compose_config();
        config.workspace_folder = Some("/workspaces/compose-basic".to_string());
        let got = resolve_container_cwd(&config, Path::new("/host/my-project"), &[], false);
        assert_eq!(got, "/workspaces/compose-basic");
    }

    #[test]
    fn cwd_single_container_uses_workspaces_basename() {
        // Non-compose without an explicit folder keeps the single-container default.
        let config = minimal_config();
        let got = resolve_container_cwd(&config, Path::new("/host/my-project"), &[], false);
        assert_eq!(got, "/workspaces/my-project");
    }

    #[test]
    fn cwd_prefers_workspace_mount_over_compose_root() {
        // A Compose service that DOES mount the workspace resolves from the mount,
        // not the `/` fallback.
        let config = compose_config();
        let mounts = vec![bind("/host/my-project", "/workspaces/my-project")];
        let got = resolve_container_cwd(&config, Path::new("/host/my-project"), &mounts, false);
        assert_eq!(got, "/workspaces/my-project");
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
