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
    /// Include all containers matching labels (stale containers)
    pub all: bool,
    /// Remove associated anonymous volumes
    pub volumes: bool,
    /// Force removal of running containers
    pub force: bool,
    /// Timeout in seconds for stopping containers (default: 30)
    pub timeout: Option<u32>,
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

    // Add structured tracing for container lifecycle
    let _span = tracing::info_span!(
        "container.down",
        all = args.all,
        volumes = args.volumes,
        force = args.force,
        timeout = args.timeout,
    )
    .entered();

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
            return execute_down_with_auto_discovery(workspace_folder, &args).await;
        }
    };

    let config = match config_result {
        Ok(config) => config,
        Err(e) => {
            warn!(
                "Failed to load configuration: {}, attempting auto-discovery",
                e
            );
            return execute_down_with_auto_discovery(workspace_folder, &args).await;
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

    let container_count = match &saved_state {
        Some(WorkspaceState::Container(_)) => 1,
        Some(WorkspaceState::Compose(compose_state)) => {
            // For compose, we count it as 1 project (could be multiple services)
            debug!("Found compose project: {}", compose_state.project_name);
            1
        }
        None => 0,
    };

    // Add container.count to span
    tracing::Span::current().record("container.count", container_count);

    match saved_state {
        Some(WorkspaceState::Container(container_state)) => {
            execute_container_down(
                &config,
                &container_state,
                &args,
                &mut state_manager,
                workspace_hash,
            )
            .await
        }
        Some(WorkspaceState::Compose(compose_state)) => {
            execute_compose_down(
                &config,
                &compose_state,
                &args,
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
#[instrument(skip(config, container_state, state_manager, args))]
async fn execute_container_down(
    config: &DevContainerConfig,
    container_state: &deacon_core::state::ContainerState,
    args: &DownArgs,
    state_manager: &mut StateManager,
    workspace_hash: &str,
) -> Result<()> {
    debug!("Shutting down container: {}", container_state.container_id);

    let docker = CliDocker::new();

    // Determine if we should remove based on flags
    let should_remove = args.remove || args.force;

    // Get timeout value (use provided or default to 30)
    let stop_timeout = args.timeout.or(Some(30));

    // Check if container is still running
    let container_info = docker
        .inspect_container(&container_state.container_id)
        .await?;
    if container_info.is_none() {
        debug!(
            "Container {} not found, removing from state",
            container_state.container_id
        );
        state_manager.remove_workspace_state(workspace_hash);
        return Ok(());
    }

    let container_info = container_info.unwrap();
    if container_info.state != "running" {
        debug!(
            "Container {} is already stopped",
            container_state.container_id
        );
        if should_remove || should_remove_container(config, container_state) {
            debug!("Removing stopped container");
            remove_container_with_options(&docker, &container_state.container_id, args).await?;
        }
        state_manager.remove_workspace_state(workspace_hash);
        return Ok(());
    }

    // Determine shutdown action - force flag overrides configuration
    let shutdown_action = if args.force {
        "stopContainer"
    } else {
        container_state
            .shutdown_action
            .as_deref()
            .or(config.shutdown_action.as_deref())
            .unwrap_or("stopContainer")
    };

    match shutdown_action {
        "none" => {
            debug!("Shutdown action is 'none', leaving container running");
            // Don't remove from state since container is still running
        }
        "stopContainer" => {
            debug!("Stopping container with timeout: {:?}", stop_timeout);
            docker
                .stop_container(&container_state.container_id, stop_timeout)
                .await?;

            if should_remove || should_remove_container(config, container_state) {
                debug!("Removing stopped container");
                remove_container_with_options(&docker, &container_state.container_id, args).await?;
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
                .stop_container(&container_state.container_id, stop_timeout)
                .await?;

            if should_remove {
                debug!("Removing stopped container");
                remove_container_with_options(&docker, &container_state.container_id, args).await?;
            }

            state_manager.remove_workspace_state(workspace_hash);
        }
    }

    Ok(())
}

/// Remove a container with optional volume removal
async fn remove_container_with_options(
    docker: &CliDocker,
    container_id: &str,
    args: &DownArgs,
) -> Result<()> {
    if args.volumes {
        debug!("Removing container with volumes");
        remove_container_with_volumes(container_id).await?;
    } else {
        docker.remove_container(container_id).await?;
    }
    Ok(())
}

/// Remove a container including its volumes
async fn remove_container_with_volumes(container_id: &str) -> Result<()> {
    use deacon_core::errors::DockerError;
    use std::process::Command;

    debug!("Removing container with volumes: {}", container_id);

    let container_id = container_id.to_string();

    tokio::task::spawn_blocking(move || -> std::result::Result<(), DockerError> {
        let output = Command::new("docker")
            .args(["rm", "-f", "-v", &container_id])
            .output()
            .map_err(|e| DockerError::CLIError(format!("Failed to remove container: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DockerError::CLIError(format!(
                "Remove command failed: {}",
                stderr
            )));
        }

        Ok(())
    })
    .await
    .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    .map_err(Into::into)
}

/// Execute down for compose configurations
#[instrument(skip(config, compose_state, state_manager, args))]
async fn execute_compose_down(
    config: &DevContainerConfig,
    compose_state: &deacon_core::state::ComposeState,
    args: &DownArgs,
    state_manager: &mut StateManager,
    workspace_hash: &str,
) -> Result<()> {
    debug!(
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
        debug!(
            "Compose project {} is not running, removing from state",
            project.name
        );
        state_manager.remove_workspace_state(workspace_hash);
        return Ok(());
    }

    // Determine shutdown action - force flag overrides configuration
    let shutdown_action = if args.force {
        "stopCompose"
    } else {
        compose_state
            .shutdown_action
            .as_deref()
            .or(config.shutdown_action.as_deref())
            .unwrap_or("stopCompose")
    };

    match shutdown_action {
        "none" => {
            debug!("Shutdown action is 'none', leaving compose project running");
            // Don't remove from state since project is still running
        }
        "stopCompose" => {
            // Use docker-compose down if remove or volumes flags are set
            let should_remove = args.remove || args.force;

            if should_remove {
                debug!("Stopping and removing compose project");
                if args.volumes {
                    debug!("Removing compose project with volumes");
                    compose_manager.down_project_with_volumes(&project)?;
                } else {
                    compose_manager.down_project(&project)?;
                }
            } else {
                debug!("Stopping compose project");
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
async fn execute_down_with_auto_discovery(workspace_folder: &Path, args: &DownArgs) -> Result<()> {
    debug!("Attempting auto-discovery of running containers/projects");

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
                args,
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
                args,
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
            all: false,
            volumes: false,
            force: false,
            timeout: None,
            workspace_folder: Some(PathBuf::from("/test")),
            config_path: None,
        };

        assert!(args.remove);
        assert!(!args.all);
        assert!(!args.volumes);
        assert!(!args.force);
        assert_eq!(args.timeout, None);
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
