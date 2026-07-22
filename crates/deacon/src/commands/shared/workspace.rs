//! Workspace folder derivation helpers.

use std::path::Path;

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
}
