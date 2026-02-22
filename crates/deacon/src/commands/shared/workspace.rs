//! Workspace folder derivation helpers.

use std::path::Path;

/// Derive the container workspace folder from configuration or the host workspace path.
///
/// If `config.workspace_folder` is set, returns that value directly.
/// Otherwise, derives it as `/workspaces/{dir_name}` where `dir_name` is the
/// last component of the host `workspace_folder` path (falling back to `"workspace"`
/// if the path has no final component).
pub fn derive_container_workspace_folder(
    config: &deacon_core::config::DevContainerConfig,
    workspace_folder: &Path,
) -> String {
    if let Some(ref folder) = config.workspace_folder {
        folder.clone()
    } else {
        let dir_name = workspace_folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace");
        format!("/workspaces/{}", dir_name)
    }
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

        let result = derive_container_workspace_folder(&config, &host_path);
        assert_eq!(result, "/custom/path");
    }

    #[test]
    fn test_derives_from_host_path() {
        let config = minimal_config();
        let host_path = PathBuf::from("/home/user/my-project");

        let result = derive_container_workspace_folder(&config, &host_path);
        assert_eq!(result, "/workspaces/my-project");
    }

    #[test]
    fn test_falls_back_to_workspace_for_root_path() {
        let config = minimal_config();
        let host_path = PathBuf::from("/");

        let result = derive_container_workspace_folder(&config, &host_path);
        assert_eq!(result, "/workspaces/workspace");
    }
}
