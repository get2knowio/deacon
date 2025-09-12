//! Exec command implementation for container execution
//!
//! This module provides container resolution and execution functionality
//! for the exec command, targeting the correct workspace container.

use anyhow::Result;
use deacon_core::config::{ConfigLoader, DevContainerConfig};
use deacon_core::container::{ContainerIdentity, ContainerOps};
use deacon_core::docker::CliDocker;
use deacon_core::errors::{ConfigError, DeaconError};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, instrument};

/// Arguments for the exec command
#[derive(Debug, Clone)]
pub struct ExecArgs {
    /// User to run the command as
    pub user: Option<String>,
    /// Disable TTY allocation
    pub no_tty: bool,
    /// Environment variables to set (KEY=VALUE format)
    pub env: Vec<String>,
    /// Command to execute
    pub command: Vec<String>,
    /// Workspace folder path
    pub workspace_folder: Option<std::path::PathBuf>,
    /// Configuration file path
    pub config_path: Option<std::path::PathBuf>,
}

/// Resolve the target container for the current workspace/config
#[instrument(skip(docker_client))]
pub async fn resolve_target_container(
    docker_client: &CliDocker,
    workspace_folder: &Path,
    config: &DevContainerConfig,
) -> Result<String> {
    debug!("Resolving target container for workspace");

    // Create container identity for this workspace/config
    let identity = ContainerIdentity::new(workspace_folder, config);
    debug!("Created container identity: {:?}", identity);

    // Find matching containers
    let matching_containers = docker_client.find_matching_containers(&identity).await?;

    match matching_containers.len() {
        0 => {
            let workspace_path = workspace_folder.display();
            let config_name = config.name.as_deref().unwrap_or("unnamed");
            Err(anyhow::anyhow!(
                "No running container found for workspace '{}' with config '{}'. \
                 Run 'deacon up' first to create the container.",
                workspace_path,
                config_name
            ))
        }
        1 => {
            let container_id = matching_containers[0].clone();
            debug!("Found unique matching container: {}", container_id);
            Ok(container_id)
        }
        multiple => {
            let workspace_path = workspace_folder.display();
            let config_name = config.name.as_deref().unwrap_or("unnamed");
            Err(anyhow::anyhow!(
                "Found {} running containers for workspace '{}' with config '{}'. \
                 This should not happen. Container IDs: {:?}",
                multiple,
                workspace_path,
                config_name,
                matching_containers
            ))
        }
    }
}

/// Determine the working directory inside the container
#[instrument(skip(config))]
pub fn determine_container_working_dir(
    config: &DevContainerConfig,
    workspace_folder: &Path,
) -> String {
    // Use containerWorkspaceFolder if specified in config
    if let Some(ref container_workspace_folder) = config.workspace_folder {
        debug!(
            "Using containerWorkspaceFolder from config: {}",
            container_workspace_folder
        );
        container_workspace_folder.clone()
    } else {
        // Default to /workspaces/{workspace_name}
        let workspace_name = workspace_folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace");
        let default_path = format!("/workspaces/{}", workspace_name);
        debug!(
            "Using default container working directory: {}",
            default_path
        );
        default_path
    }
}

/// Execute the exec command
#[instrument(skip(args))]
pub async fn execute_exec(args: ExecArgs) -> Result<()> {
    if args.command.is_empty() {
        return Err(anyhow::anyhow!("No command specified for exec"));
    }

    #[cfg(feature = "docker")]
    {
        use deacon_core::docker::{Docker, ExecConfig};

        tracing::info!("Executing command in container: {:?}", args.command);

        // Parse environment variables early to catch format errors
        let mut env_map = HashMap::new();
        for env_var in &args.env {
            if let Some((key, value)) = env_var.split_once('=') {
                env_map.insert(key.to_string(), value.to_string());
            } else {
                return Err(anyhow::anyhow!(
                    "Invalid environment variable format: '{}'. Expected KEY=VALUE",
                    env_var
                ));
            }
        }

        // Load configuration
        let workspace_folder = args.workspace_folder.as_deref().unwrap_or(Path::new("."));

        let config = if let Some(config_path) = args.config_path.as_ref() {
            ConfigLoader::load_from_path(config_path)?
        } else {
            let config_location = ConfigLoader::discover_config(workspace_folder)?;
            if !config_location.exists() {
                return Err(DeaconError::Config(ConfigError::NotFound {
                    path: config_location.path().to_string_lossy().to_string(),
                })
                .into());
            }
            ConfigLoader::load_from_path(config_location.path())?
        };

        debug!("Loaded configuration: {:?}", config.name);

        let docker_client = CliDocker::new();

        // Check Docker availability
        docker_client.ping().await?;

        // Resolve target container
        let container_id =
            resolve_target_container(&docker_client, workspace_folder, &config).await?;

        // Determine TTY allocation
        let should_use_tty = !args.no_tty && CliDocker::is_tty();

        // Determine working directory
        let working_dir = determine_container_working_dir(&config, workspace_folder);

        // Create exec config
        let exec_config = ExecConfig {
            user: args.user.clone(),
            working_dir: Some(working_dir),
            env: env_map,
            tty: should_use_tty,
            interactive: should_use_tty,
            detach: false,
        };

        match docker_client
            .exec(&container_id, &args.command, exec_config)
            .await
        {
            Ok(result) => {
                tracing::info!("Command completed with exit code: {}", result.exit_code);
                std::process::exit(result.exit_code);
            }
            Err(e) => {
                tracing::error!("Failed to execute command: {}", e);
                Err(e.into())
            }
        }
    }

    #[cfg(not(feature = "docker"))]
    {
        tracing::warn!("Docker support is disabled (compiled without 'docker' feature)");
        Err(DeaconError::Config(ConfigError::NotImplemented {
            feature: "exec command (docker support disabled)".to_string(),
        })
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::config::DevContainerConfig;
    use tempfile::TempDir;

    #[test]
    fn test_determine_container_working_dir_with_config() {
        let config = DevContainerConfig {
            workspace_folder: Some("/custom/workspace".to_string()),
            ..Default::default()
        };

        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let working_dir = determine_container_working_dir(&config, workspace_path);
        assert_eq!(working_dir, "/custom/workspace");
    }

    #[test]
    fn test_determine_container_working_dir_default() {
        let config = DevContainerConfig {
            workspace_folder: None,
            ..Default::default()
        };

        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();
        let workspace_name = workspace_path.file_name().unwrap().to_str().unwrap();

        let working_dir = determine_container_working_dir(&config, workspace_path);
        assert_eq!(working_dir, format!("/workspaces/{}", workspace_name));
    }

    #[test]
    fn test_determine_container_working_dir_fallback() {
        let config = DevContainerConfig {
            workspace_folder: None,
            ..Default::default()
        };

        // Use a path that might not have a proper file name
        let working_dir = determine_container_working_dir(&config, Path::new("/"));
        assert_eq!(working_dir, "/workspaces/workspace");
    }
}
