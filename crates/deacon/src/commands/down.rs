//! Down command implementation
//!
//! Implements the `deacon down` subcommand for stopping development containers
//! and compose projects according to their shutdown actions.

use anyhow::Result;
use deacon_core::compose::{ComposeManager, ComposeProject};
use deacon_core::config::{ConfigLoader, DevContainerConfig};
use deacon_core::container::{ContainerIdentity, ContainerOps};
use deacon_core::docker::{CliDocker, Docker};
use deacon_core::state::{StateManager, WorkspaceState};
use std::path::{Path, PathBuf};
use tracing::{debug, info, instrument, warn};

/// Down command arguments
#[derive(Debug, Clone)]
pub struct DownArgs {
    /// Remove containers after stopping them
    pub remove: bool,
    /// Workspace folder path
    pub workspace_folder: Option<PathBuf>,
    /// Configuration file path
    pub config_path: Option<PathBuf>,
}

/// Execute the down command
#[instrument(skip(args))]
pub async fn execute_down(args: DownArgs) -> Result<()> {
    info!("Starting down command execution");
    debug!("Down args: {:?}", args);

    let workspace_folder = args.workspace_folder.as_deref().unwrap_or(Path::new("."));

    // Try to load configuration
    let config_result = if let Some(config_path) = args.config_path.as_ref() {
        ConfigLoader::load_from_path(config_path)
    } else {
        let config_location = ConfigLoader::discover_config(workspace_folder)?;
        if config_location.exists() {
            ConfigLoader::load_from_path(config_location.path())
        } else {
            // Config not found - we'll try to use saved state for auto-discovery
            debug!("No configuration found, attempting auto-discovery from state");
            return execute_down_with_auto_discovery(workspace_folder, args.remove).await;
        }
    };

    let config = match config_result {
        Ok(config) => config,
        Err(e) => {
            warn!(
                "Failed to load configuration: {}, attempting auto-discovery",
                e
            );
            return execute_down_with_auto_discovery(workspace_folder, args.remove).await;
        }
    };

    debug!("Loaded configuration: {:?}", config.name);

    // Get workspace hash for state lookup
    let identity = ContainerIdentity::new(workspace_folder, &config);
    let workspace_hash = &identity.workspace_hash;

    debug!("Workspace hash: {}", workspace_hash);

    // Load saved state
    let mut state_manager = StateManager::new()?;
    let saved_state = state_manager.get_workspace_state(workspace_hash);

    match saved_state {
        Some(WorkspaceState::Container(container_state)) => {
            execute_container_down(
                &config,
                &container_state,
                args.remove,
                &mut state_manager,
                workspace_hash,
            )
            .await
        }
        Some(WorkspaceState::Compose(compose_state)) => {
            execute_compose_down(
                &config,
                &compose_state,
                args.remove,
                &mut state_manager,
                workspace_hash,
            )
            .await
        }
        None => {
            info!("No running containers or compose projects found for workspace");
            Ok(())
        }
    }
}

/// Execute down for single container configurations
#[instrument(skip(config, container_state, state_manager))]
async fn execute_container_down(
    config: &DevContainerConfig,
    container_state: &deacon_core::state::ContainerState,
    force_remove: bool,
    state_manager: &mut StateManager,
    workspace_hash: &str,
) -> Result<()> {
    info!("Shutting down container: {}", container_state.container_id);

    let docker = CliDocker::new();

    // Check if container is still running
    let container_info = docker
        .inspect_container(&container_state.container_id)
        .await?;
    if container_info.is_none() {
        info!(
            "Container {} not found, removing from state",
            container_state.container_id
        );
        state_manager.remove_workspace_state(workspace_hash);
        return Ok(());
    }

    let container_info = container_info.unwrap();
    if container_info.state != "running" {
        info!(
            "Container {} is already stopped",
            container_state.container_id
        );
        if force_remove || should_remove_container(config, container_state) {
            info!("Removing stopped container");
            docker
                .remove_container(&container_state.container_id)
                .await?;
        }
        state_manager.remove_workspace_state(workspace_hash);
        return Ok(());
    }

    // Determine shutdown action
    let shutdown_action = container_state
        .shutdown_action
        .as_deref()
        .or(config.shutdown_action.as_deref())
        .unwrap_or("stopContainer");

    match shutdown_action {
        "none" => {
            info!("Shutdown action is 'none', leaving container running");
            // Don't remove from state since container is still running
        }
        "stopContainer" => {
            info!("Stopping container with timeout");
            docker
                .stop_container(&container_state.container_id, Some(30))
                .await?;

            if force_remove || should_remove_container(config, container_state) {
                info!("Removing stopped container");
                docker
                    .remove_container(&container_state.container_id)
                    .await?;
            }

            // Remove from state since container is stopped
            state_manager.remove_workspace_state(workspace_hash);
            info!("Container shutdown completed");
        }
        _ => {
            warn!(
                "Invalid shutdown action '{}' for container, defaulting to stopContainer",
                shutdown_action
            );
            docker
                .stop_container(&container_state.container_id, Some(30))
                .await?;

            if force_remove {
                info!("Removing stopped container");
                docker
                    .remove_container(&container_state.container_id)
                    .await?;
            }

            state_manager.remove_workspace_state(workspace_hash);
        }
    }

    Ok(())
}

/// Execute down for compose configurations
#[instrument(skip(config, compose_state, state_manager))]
async fn execute_compose_down(
    config: &DevContainerConfig,
    compose_state: &deacon_core::state::ComposeState,
    force_remove: bool,
    state_manager: &mut StateManager,
    workspace_hash: &str,
) -> Result<()> {
    info!(
        "Shutting down compose project: {}",
        compose_state.project_name
    );

    let compose_manager = ComposeManager::new();

    // Create project from saved state
    let project = ComposeProject {
        name: compose_state.project_name.clone(),
        base_path: PathBuf::from(&compose_state.base_path),
        compose_files: compose_state
            .compose_files
            .iter()
            .map(PathBuf::from)
            .collect(),
        service: compose_state.service_name.clone(),
        run_services: vec![], // We don't track run_services in state currently
    };

    // Check if project is still running
    if !compose_manager.is_project_running(&project)? {
        info!(
            "Compose project {} is not running, removing from state",
            project.name
        );
        state_manager.remove_workspace_state(workspace_hash);
        return Ok(());
    }

    // Determine shutdown action
    let shutdown_action = compose_state
        .shutdown_action
        .as_deref()
        .or(config.shutdown_action.as_deref())
        .unwrap_or("stopCompose");

    match shutdown_action {
        "none" => {
            info!("Shutdown action is 'none', leaving compose project running");
            // Don't remove from state since project is still running
        }
        "stopCompose" => {
            if force_remove {
                info!("Stopping and removing compose project");
                compose_manager.down_project(&project)?;
            } else {
                info!("Stopping compose project");
                compose_manager.stop_project(&project)?;
            }

            // Remove from state since project is stopped
            state_manager.remove_workspace_state(workspace_hash);
            info!("Compose project shutdown completed");
        }
        _ => {
            warn!(
                "Invalid shutdown action '{}' for compose project, defaulting to stopCompose",
                shutdown_action
            );
            compose_manager.stop_project(&project)?;
            state_manager.remove_workspace_state(workspace_hash);
        }
    }

    Ok(())
}

/// Execute down with auto-discovery when config is not available
#[instrument]
async fn execute_down_with_auto_discovery(
    workspace_folder: &Path,
    force_remove: bool,
) -> Result<()> {
    info!("Attempting auto-discovery of running containers/projects");

    // Create a minimal identity just for workspace hash generation
    let config = DevContainerConfig::default();
    let identity = ContainerIdentity::new(workspace_folder, &config);
    let workspace_hash = &identity.workspace_hash;

    debug!("Auto-discovery workspace hash: {}", workspace_hash);

    let mut state_manager = StateManager::new()?;
    let saved_state = state_manager.get_workspace_state(workspace_hash);

    match saved_state {
        Some(WorkspaceState::Container(container_state)) => {
            // Use default config for shutdown action
            let default_config = DevContainerConfig::default();
            execute_container_down(
                &default_config,
                &container_state,
                force_remove,
                &mut state_manager,
                workspace_hash,
            )
            .await
        }
        Some(WorkspaceState::Compose(compose_state)) => {
            // Use default config for shutdown action
            let default_config = DevContainerConfig::default();
            execute_compose_down(
                &default_config,
                &compose_state,
                force_remove,
                &mut state_manager,
                workspace_hash,
            )
            .await
        }
        None => {
            info!("No running containers or compose projects found for workspace");
            Ok(())
        }
    }
}

/// Determine if a container should be removed based on configuration
fn should_remove_container(
    config: &DevContainerConfig,
    _container_state: &deacon_core::state::ContainerState,
) -> bool {
    // For now, only remove if explicitly configured to do so
    // Future enhancement: could check for additional removal policies
    config.shutdown_action.as_deref() == Some("removeContainer")
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::state::ContainerState;

    #[test]
    fn test_down_args_creation() {
        let args = DownArgs {
            remove: true,
            workspace_folder: Some(PathBuf::from("/test")),
            config_path: None,
        };

        assert!(args.remove);
        assert_eq!(args.workspace_folder, Some(PathBuf::from("/test")));
        assert!(args.config_path.is_none());
    }

    #[test]
    fn test_should_remove_container() {
        let config = DevContainerConfig {
            shutdown_action: Some("removeContainer".to_string()),
            ..Default::default()
        };

        let container_state = ContainerState {
            container_id: "test123".to_string(),
            container_name: None,
            image_id: "image123".to_string(),
            shutdown_action: None,
        };

        assert!(should_remove_container(&config, &container_state));
    }

    #[test]
    fn test_should_not_remove_container_default() {
        let config = DevContainerConfig::default();

        let container_state = ContainerState {
            container_id: "test123".to_string(),
            container_name: None,
            image_id: "image123".to_string(),
            shutdown_action: None,
        };

        assert!(!should_remove_container(&config, &container_state));
    }
}
