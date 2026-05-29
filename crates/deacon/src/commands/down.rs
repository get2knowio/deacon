//! Down command implementation
//!
//! Implements the `deacon down` subcommand for stopping development containers
//! and compose projects according to their shutdown actions.

use anyhow::Result;
use deacon_core::compose::{ComposeManager, ComposeProject};
use deacon_core::config::{ConfigLoader, DevContainerConfig, DiscoveryResult};
use deacon_core::container::{ContainerIdentity, ContainerOps};
use deacon_core::docker::{CliDocker, Docker};
use deacon_core::errors::{ConfigError, DeaconError};
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
    /// Path to docker executable
    #[allow(dead_code)] // Future: Will be used for custom docker executable path
    pub docker_path: String,
    /// Path to docker-compose executable (legacy standalone binary)
    #[allow(dead_code)] // Future: Will be used for standalone docker-compose binary support
    pub docker_compose_path: String,
}

/// Execute the down command
#[instrument(skip(args))]
pub async fn execute_down(args: DownArgs) -> Result<()> {
    debug!("Starting down command execution");
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
        ConfigLoader::load_from_path(config_path).await
    } else {
        match ConfigLoader::discover_config(workspace_folder).await? {
            DiscoveryResult::Single(path) => ConfigLoader::load_from_path(&path).await,
            DiscoveryResult::Multiple(paths) => {
                let display_paths: Vec<String> = paths
                    .iter()
                    .map(|p| {
                        p.strip_prefix(workspace_folder)
                            .unwrap_or(p)
                            .to_string_lossy()
                            .to_string()
                    })
                    .collect();
                return Err(DeaconError::Config(ConfigError::MultipleConfigs {
                    paths: display_paths,
                })
                .into());
            }
            DiscoveryResult::None(_) => {
                debug!("No configuration found, attempting auto-discovery from state");
                return execute_down_with_auto_discovery(workspace_folder, &args).await;
            }
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

    // If --all flag is set, find and remove all containers with matching labels
    if args.all {
        return execute_down_all(&identity, &args).await;
    }

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

/// Execute down for all containers with matching labels
#[instrument(skip(identity, args))]
async fn execute_down_all(identity: &ContainerIdentity, args: &DownArgs) -> Result<()> {
    debug!("Finding all containers with matching labels");

    let docker = CliDocker::new();

    // `--all` sweeps *every* container for this workspace, including stale
    // ones created under an older/different config. Match on the durable,
    // spec-canonical `devcontainer.local_folder` label (the workspace path)
    // rather than the config-pinned `source`+`workspace_hash`+`config_hash`
    // selector used to find the single current container — otherwise a
    // container whose config has drifted (different `config_hash`) would
    // never be swept, defeating the purpose of `--all`. Fall back to the
    // strict selector if the workspace path could not be canonicalized.
    let label_selector = identity
        .workspace_label_selector()
        .unwrap_or_else(|| identity.label_selector());

    debug!("Label selector: {}", label_selector);

    // List all containers with matching labels
    let containers = docker.list_containers(Some(&label_selector)).await?;

    if containers.is_empty() {
        info!("No containers found with matching labels");
        return Ok(());
    }

    let container_count = containers.len();
    info!(
        "Found {} container(s) with matching labels",
        container_count
    );
    tracing::Span::current().record("container.count", container_count);

    // Get timeout value (use provided or default to 30)
    let stop_timeout = args.timeout.or(Some(30));

    // Stop and optionally remove each container. `--all` is a best-effort
    // sweep across potentially many containers, so a per-container failure
    // must NOT abort the whole sweep — log it and keep going, then fail at
    // the end only if a container genuinely survived. Errors that just mean
    // "the container is already gone" (e.g. a `--rm` container auto-removed
    // on stop, or a concurrent removal) are treated as success.
    let mut failures = 0usize;
    for container in containers {
        debug!("Processing container: {}", container.id);

        // Only stop if container is running
        if container.state == "running" {
            debug!(
                "Stopping container {} with timeout: {:?}",
                container.id, stop_timeout
            );
            if let Err(e) = docker.stop_container(&container.id, stop_timeout).await {
                if is_already_gone(&e) {
                    debug!("Container {} already gone while stopping", container.id);
                } else {
                    warn!("Failed to stop container {}: {}", container.id, e);
                    failures += 1;
                    continue;
                }
            }
        } else {
            debug!(
                "Container {} is not running (state: {})",
                container.id, container.state
            );
        }

        // Remove if requested
        if args.remove || args.force || args.volumes {
            debug!("Removing container {}", container.id);
            if let Err(e) = remove_container_with_options(&docker, &container.id, args).await {
                if is_already_gone(&e) {
                    debug!("Container {} already removed", container.id);
                } else {
                    warn!("Failed to remove container {}: {}", container.id, e);
                    failures += 1;
                }
            }
        }
    }

    if failures > 0 {
        anyhow::bail!("{} container(s) could not be torn down by --all", failures);
    }

    // The current workspace's container was just swept (if it had one), so
    // drop its saved state — otherwise a subsequent plain `down` would read
    // state pointing at a now-removed container. Best-effort: a missing state
    // entry is fine.
    if args.remove || args.force || args.volumes {
        if let Ok(mut state_manager) = StateManager::new() {
            state_manager.remove_workspace_state(&identity.workspace_hash);
        }
    }

    info!("All matching containers processed successfully");
    Ok(())
}

/// Returns true if a Docker error simply means the container is already gone
/// (concurrently removed, auto-removed via `--rm` on stop, or never existed).
/// Such errors are benign for a teardown whose goal is the container's absence.
fn is_already_gone(err: &impl std::fmt::Display) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("no such container") || msg.contains("already in progress")
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
        docker.remove_container_with_volumes(container_id).await?;
    } else {
        docker.remove_container(container_id).await?;
    }
    Ok(())
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

    let compose_manager = ComposeManager::with_docker_path(args.docker_path.clone());

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
        env_files: Vec::new(),
        additional_mounts: Vec::new(), // Not needed for down operation
        profiles: Vec::new(),          // Not needed for down operation
        additional_env: deacon_core::IndexMap::new(),
        external_volumes: Vec::new(), // Not needed for down operation
        override_command: None,       // Not needed for down operation
        service_image_override: None,
        deacon_labels: deacon_core::IndexMap::new(), // Not needed for down operation
    };

    // Check if project is still running
    if !compose_manager.is_project_running(&project).await? {
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
                    compose_manager.down_project_with_volumes(&project).await?;
                } else {
                    compose_manager.down_project(&project).await?;
                }
            } else {
                debug!("Stopping compose project");
                compose_manager.stop_project(&project).await?;
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
            compose_manager.stop_project(&project).await?;
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
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
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
