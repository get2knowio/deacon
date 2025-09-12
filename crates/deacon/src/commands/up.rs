//! Up command implementation
//!
//! Implements the `deacon up` subcommand for starting development containers.
//! Supports both traditional container workflows and Docker Compose workflows.

use anyhow::Result;
use deacon_core::compose::{ComposeManager, ComposeProject};
use deacon_core::config::{ConfigLoader, DevContainerConfig};
use deacon_core::container::ContainerIdentity;
use deacon_core::docker::{CliDocker, Docker, ExecConfig};
use deacon_core::errors::DeaconError;
use deacon_core::ports::PortForwardingManager;
use deacon_core::state::{ComposeState, ContainerState, StateManager};
use std::path::{Path, PathBuf};
use tracing::{debug, info, instrument, warn};

/// Up command arguments
#[derive(Debug, Clone)]
pub struct UpArgs {
    pub remove_existing_container: bool,
    pub skip_post_create: bool,
    #[allow(dead_code)] // TODO: Connect to container lifecycle execution
    pub skip_non_blocking_commands: bool,
    pub ports_events: bool,
    pub shutdown: bool,
    pub workspace_folder: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
}

/// Execute the up command
#[instrument(skip(args))]
pub async fn execute_up(args: UpArgs) -> Result<()> {
    info!("Starting up command execution");
    debug!("Up args: {:?}", args);

    // Load configuration
    let workspace_folder = args.workspace_folder.as_deref().unwrap_or(Path::new("."));

    let config = if let Some(config_path) = args.config_path.as_ref() {
        ConfigLoader::load_from_path(config_path)?
    } else {
        let config_location = ConfigLoader::discover_config(workspace_folder)?;
        if !config_location.exists() {
            return Err(
                DeaconError::Config(deacon_core::errors::ConfigError::NotFound {
                    path: config_location.path().to_string_lossy().to_string(),
                })
                .into(),
            );
        }
        ConfigLoader::load_from_path(config_location.path())?
    };

    debug!("Loaded configuration: {:?}", config.name);

    // Create container identity for state tracking
    let identity = ContainerIdentity::new(workspace_folder, &config);
    let workspace_hash = identity.workspace_hash.clone();

    // Initialize state manager
    let mut state_manager = StateManager::new()?;

    // Check if this is a compose-based configuration
    if config.uses_compose() {
        execute_compose_up(
            &config,
            workspace_folder,
            &args,
            &mut state_manager,
            &workspace_hash,
        )
        .await
    } else {
        execute_container_up(
            &config,
            workspace_folder,
            &args,
            &mut state_manager,
            &workspace_hash,
        )
        .await
    }
}

/// Execute up for Docker Compose configurations
#[instrument(skip(config, workspace_folder, args, state_manager))]
async fn execute_compose_up(
    config: &DevContainerConfig,
    workspace_folder: &Path,
    args: &UpArgs,
    state_manager: &mut StateManager,
    workspace_hash: &str,
) -> Result<()> {
    info!("Starting Docker Compose project");

    let compose_manager = ComposeManager::new();
    let project = compose_manager.create_project(config, workspace_folder)?;

    debug!("Created compose project: {:?}", project.name);

    // Check if project is already running
    if !args.remove_existing_container && compose_manager.is_project_running(&project)? {
        info!("Compose project {} is already running", project.name);

        // Get the primary container ID for potential exec operations
        if let Some(container_id) = compose_manager.get_primary_container_id(&project)? {
            info!("Primary service container ID: {}", container_id);
        }

        return Ok(());
    }

    // Stop existing containers if requested
    if args.remove_existing_container {
        info!("Stopping existing compose project");
        if let Err(e) = compose_manager.stop_project(&project) {
            warn!("Failed to stop existing project: {}", e);
        }
    }

    // Start the compose project
    compose_manager.start_project(&project)?;

    info!("Compose project {} started successfully", project.name);

    // Save compose state for shutdown tracking
    let compose_state = ComposeState {
        project_name: project.name.clone(),
        service_name: project.service.clone(),
        base_path: project.base_path.to_string_lossy().to_string(),
        compose_files: project
            .compose_files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
        shutdown_action: config.shutdown_action.clone(),
    };

    state_manager.save_compose_state(workspace_hash, compose_state)?;
    debug!("Saved compose state for workspace hash: {}", workspace_hash);

    // Execute post-create lifecycle if not skipped
    if !args.skip_post_create {
        execute_compose_post_create(&project, config).await?;
    }

    // Handle port forwarding and events
    if args.ports_events {
        handle_port_events(config, &project).await?;
    }

    // Handle shutdown if requested
    if args.shutdown {
        handle_compose_shutdown(config, &project, state_manager, workspace_hash).await?;
    }

    Ok(())
}

/// Execute up for traditional container configurations
#[instrument(skip_all)]
async fn execute_container_up(
    config: &DevContainerConfig,
    workspace_folder: &Path,
    args: &UpArgs,
    state_manager: &mut StateManager,
    workspace_hash: &str,
) -> Result<()> {
    info!("Starting traditional development container");

    // Create container identity for deterministic naming and labels
    let identity = ContainerIdentity::new(workspace_folder, config);
    debug!("Container identity: {:?}", identity);

    // Initialize Docker client
    let docker = CliDocker::new();

    // Check Docker availability
    docker.ping().await?;

    // Create container using DockerLifecycle trait
    use deacon_core::docker::DockerLifecycle;
    let container_result = docker
        .up(
            &identity,
            config,
            workspace_folder,
            args.remove_existing_container,
        )
        .await?;

    info!(
        "Container {} {} (image: {})",
        if container_result.reused {
            "reused"
        } else {
            "created"
        },
        container_result.container_id,
        container_result.image_id
    );

    // Save container state for shutdown tracking
    let container_state = ContainerState {
        container_id: container_result.container_id.clone(),
        container_name: identity.name.clone(),
        image_id: container_result.image_id.clone(),
        shutdown_action: config.shutdown_action.clone(),
    };

    state_manager.save_container_state(workspace_hash, container_state)?;
    debug!(
        "Saved container state for workspace hash: {}",
        workspace_hash
    );

    // Apply user mapping if configured
    if config.remote_user.is_some() || config.container_user.is_some() {
        apply_user_mapping(&container_result.container_id, config, workspace_folder).await?;
    }

    // Execute lifecycle commands if not skipped
    execute_lifecycle_commands(
        &container_result.container_id,
        config,
        workspace_folder,
        args,
    )
    .await?;

    // Handle port events if requested
    if args.ports_events {
        handle_container_port_events(&container_result.container_id, config).await?;
    }

    // Handle shutdown if requested
    if args.shutdown {
        handle_container_shutdown(
            config,
            &container_result.container_id,
            state_manager,
            workspace_hash,
        )
        .await?;
    }

    info!("Traditional container up completed successfully");
    Ok(())
}

/// Execute post-create lifecycle for compose projects
#[instrument(skip(project, config))]
async fn execute_compose_post_create(
    project: &ComposeProject,
    config: &DevContainerConfig,
) -> Result<()> {
    info!("Executing post-create lifecycle for compose project");

    // Get the primary container ID
    let compose_manager = ComposeManager::new();
    let container_id = match compose_manager.get_primary_container_id(project)? {
        Some(id) => id,
        None => {
            warn!("Primary service container not found, skipping post-create");
            return Ok(());
        }
    };

    debug!(
        "Running post-create commands in container: {}",
        container_id
    );

    // Execute postCreateCommand if specified
    if let Some(post_create_cmd) = &config.post_create_command {
        if let Some(cmd_str) = post_create_cmd.as_str() {
            info!("Executing postCreateCommand: {}", cmd_str);

            let docker = CliDocker::new();
            let result = docker
                .exec(
                    &container_id,
                    &["sh".to_string(), "-c".to_string(), cmd_str.to_string()],
                    ExecConfig {
                        user: None,
                        working_dir: None,
                        env: std::collections::HashMap::new(),
                        tty: false,
                        interactive: false,
                        detach: false,
                    },
                )
                .await;

            match result {
                Ok(_) => info!("postCreateCommand completed successfully"),
                Err(e) => warn!("postCreateCommand failed: {}", e),
            }
        }
    }

    Ok(())
}

/// Handle port events for compose projects
#[instrument(skip(config, project))]
async fn handle_port_events(config: &DevContainerConfig, project: &ComposeProject) -> Result<()> {
    info!("Processing port events for compose project");

    let compose_manager = ComposeManager::new();
    let container_id = match compose_manager.get_primary_container_id(project)? {
        Some(id) => id,
        None => {
            warn!("Primary service container not found, skipping port events");
            return Ok(());
        }
    };

    // Inspect the container to get port information
    let docker = CliDocker::new();
    let container_info = match docker.inspect_container(&container_id).await? {
        Some(info) => info,
        None => {
            warn!("Container {} not found, skipping port events", container_id);
            return Ok(());
        }
    };

    debug!(
        "Container {} has {} exposed ports and {} port mappings",
        container_id,
        container_info.exposed_ports.len(),
        container_info.port_mappings.len()
    );

    // Process ports and emit events
    let events = PortForwardingManager::process_container_ports(
        config,
        &container_info,
        true, // emit_events = true
    );

    info!("Emitted {} port events", events.len());

    Ok(())
}

/// Apply user mapping configuration to the container
#[instrument(skip(config))]
async fn apply_user_mapping(
    container_id: &str,
    config: &DevContainerConfig,
    workspace_folder: &Path,
) -> Result<()> {
    use deacon_core::user_mapping::{get_host_user_info, UserMappingConfig};

    info!("Applying user mapping configuration");

    // Create user mapping configuration
    let mut user_config = UserMappingConfig::new(
        config.remote_user.clone(),
        config.container_user.clone(),
        config.update_remote_user_uid.unwrap_or(false),
    );

    // Add host user information if updateRemoteUserUID is enabled
    if user_config.update_remote_user_uid {
        match get_host_user_info() {
            Ok((uid, gid)) => {
                user_config = user_config.with_host_user(uid, gid);
                debug!("Host user: UID={}, GID={}", uid, gid);
            }
            Err(e) => {
                warn!("Failed to get host user info, skipping UID mapping: {}", e);
            }
        }
    }

    // Set workspace path for ownership adjustments
    if let Some(container_workspace_folder) = &config.workspace_folder {
        user_config = user_config.with_workspace_path(container_workspace_folder.clone());
    } else {
        // Default container workspace folder
        let workspace_name = workspace_folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace");
        user_config = user_config.with_workspace_path(format!("/workspaces/{}", workspace_name));
    }

    // Apply user mapping if needed
    if user_config.needs_user_mapping() {
        debug!("User mapping required, applying configuration");

        // TODO: Implement user mapping application using UserMappingService
        // For now, log that user mapping would be applied
        info!(
            "User mapping configured: remote_user={:?}, container_user={:?}, update_uid={}",
            user_config.remote_user, user_config.container_user, user_config.update_remote_user_uid
        );
    }

    Ok(())
}

/// Execute lifecycle commands in the container
#[instrument(skip(config, args))]
async fn execute_lifecycle_commands(
    container_id: &str,
    config: &DevContainerConfig,
    workspace_folder: &Path,
    args: &UpArgs,
) -> Result<()> {
    use deacon_core::container_lifecycle::{
        execute_container_lifecycle, ContainerLifecycleCommands, ContainerLifecycleConfig,
    };
    use deacon_core::variable::SubstitutionContext;

    info!("Executing lifecycle commands in container");

    // Create substitution context
    let substitution_context = SubstitutionContext::new(workspace_folder)?;

    // Determine container workspace folder
    let container_workspace_folder = if let Some(ref workspace_folder) = config.workspace_folder {
        workspace_folder.clone()
    } else {
        let workspace_name = workspace_folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace");
        format!("/workspaces/{}", workspace_name)
    };

    // Create container lifecycle configuration
    let lifecycle_config = ContainerLifecycleConfig {
        container_id: container_id.to_string(),
        user: config
            .remote_user
            .clone()
            .or_else(|| config.container_user.clone()),
        container_workspace_folder,
        container_env: config.container_env.clone(),
        skip_post_create: args.skip_post_create,
        skip_non_blocking_commands: args.skip_non_blocking_commands,
    };

    // Build lifecycle commands from configuration
    let mut commands = ContainerLifecycleCommands::new();

    if let Some(ref on_create) = config.on_create_command {
        commands = commands.with_on_create(commands_from_json_value(on_create)?);
    }

    if let Some(ref post_create) = config.post_create_command {
        commands = commands.with_post_create(commands_from_json_value(post_create)?);
    }

    if let Some(ref post_start) = config.post_start_command {
        commands = commands.with_post_start(commands_from_json_value(post_start)?);
    }

    if let Some(ref post_attach) = config.post_attach_command {
        commands = commands.with_post_attach(commands_from_json_value(post_attach)?);
    }

    // Execute lifecycle commands
    let result =
        execute_container_lifecycle(&lifecycle_config, &commands, &substitution_context).await?;

    info!(
        "Lifecycle execution completed: {} phases executed",
        result.phases.len()
    );

    Ok(())
}

/// Convert JSON value to vector of command strings
fn commands_from_json_value(value: &serde_json::Value) -> Result<Vec<String>> {
    match value {
        serde_json::Value::String(cmd) => Ok(vec![cmd.clone()]),
        serde_json::Value::Array(cmds) => {
            let mut commands = Vec::new();
            for cmd_value in cmds {
                if let serde_json::Value::String(cmd) = cmd_value {
                    commands.push(cmd.clone());
                } else {
                    return Err(DeaconError::Config(
                        deacon_core::errors::ConfigError::Validation {
                            message: format!(
                                "Invalid command in array: expected string, got {:?}",
                                cmd_value
                            ),
                        },
                    )
                    .into());
                }
            }
            Ok(commands)
        }
        _ => Err(
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: format!(
                    "Invalid command format: expected string or array of strings, got {:?}",
                    value
                ),
            })
            .into(),
        ),
    }
}

/// Handle port events for the container
#[instrument(skip(config))]
async fn handle_container_port_events(
    container_id: &str,
    config: &DevContainerConfig,
) -> Result<()> {
    info!("Processing port events for container");

    // Inspect the container to get port information
    let docker = CliDocker::new();
    let container_info = match docker.inspect_container(container_id).await? {
        Some(info) => info,
        None => {
            warn!("Container {} not found, skipping port events", container_id);
            return Ok(());
        }
    };

    debug!(
        "Container {} has {} exposed ports and {} port mappings",
        container_id,
        container_info.exposed_ports.len(),
        container_info.port_mappings.len()
    );

    // Process ports and emit events
    let events = PortForwardingManager::process_container_ports(
        config,
        &container_info,
        true, // emit_events = true
    );

    info!("Emitted {} port events", events.len());

    Ok(())
}

/// Handle shutdown for container configurations
#[instrument(skip(config, state_manager))]
async fn handle_container_shutdown(
    config: &DevContainerConfig,
    container_id: &str,
    state_manager: &mut StateManager,
    workspace_hash: &str,
) -> Result<()> {
    info!("Handling shutdown for container: {}", container_id);

    let shutdown_action = config.shutdown_action.as_deref().unwrap_or("stopContainer");

    match shutdown_action {
        "none" => {
            info!("Shutdown action is 'none', leaving container running");
        }
        "stopContainer" => {
            info!("Stopping container due to shutdown action");
            let docker = CliDocker::new();
            docker.stop_container(container_id, Some(30)).await?;
            state_manager.remove_workspace_state(workspace_hash);
            info!("Container stopped and removed from state");
        }
        _ => {
            warn!(
                "Unknown shutdown action '{}', leaving container running",
                shutdown_action
            );
        }
    }

    Ok(())
}

/// Handle shutdown for compose configurations
#[instrument(skip(config, state_manager))]
async fn handle_compose_shutdown(
    config: &DevContainerConfig,
    project: &ComposeProject,
    state_manager: &mut StateManager,
    workspace_hash: &str,
) -> Result<()> {
    info!("Handling shutdown for compose project: {}", project.name);

    let shutdown_action = config.shutdown_action.as_deref().unwrap_or("stopCompose");

    match shutdown_action {
        "none" => {
            info!("Shutdown action is 'none', leaving compose project running");
        }
        "stopCompose" => {
            info!("Stopping compose project due to shutdown action");
            let compose_manager = ComposeManager::new();
            compose_manager.stop_project(project)?;
            state_manager.remove_workspace_state(workspace_hash);
            info!("Compose project stopped and removed from state");
        }
        _ => {
            warn!(
                "Unknown shutdown action '{}', leaving compose project running",
                shutdown_action
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::config::DevContainerConfig;
    use serde_json::json;

    #[test]
    fn test_up_args_creation() {
        let args = UpArgs {
            remove_existing_container: true,
            skip_post_create: false,
            skip_non_blocking_commands: false,
            ports_events: false,
            shutdown: false,
            workspace_folder: Some(PathBuf::from("/test")),
            config_path: None,
        };

        assert!(args.remove_existing_container);
        assert!(!args.skip_post_create);
        assert!(!args.skip_non_blocking_commands);
        assert!(!args.ports_events);
        assert!(!args.shutdown);
        assert_eq!(args.workspace_folder, Some(PathBuf::from("/test")));
        assert!(args.config_path.is_none());
    }

    #[test]
    fn test_commands_from_json_value_string() {
        let json_value = serde_json::Value::String("echo hello".to_string());
        let commands = commands_from_json_value(&json_value).unwrap();
        assert_eq!(commands, vec!["echo hello"]);
    }

    #[test]
    fn test_commands_from_json_value_array() {
        let json_value = serde_json::json!(["echo hello", "echo world"]);
        let commands = commands_from_json_value(&json_value).unwrap();
        assert_eq!(commands, vec!["echo hello", "echo world"]);
    }

    #[test]
    fn test_commands_from_json_value_invalid() {
        let json_value = serde_json::Value::Number(serde_json::Number::from(42));
        let result = commands_from_json_value(&json_value);
        assert!(result.is_err());
    }

    #[test]
    fn test_up_args_with_all_flags() {
        let args = UpArgs {
            remove_existing_container: true,
            skip_post_create: true,
            skip_non_blocking_commands: true,
            ports_events: true,
            shutdown: true,
            workspace_folder: Some(PathBuf::from("/test")),
            config_path: None,
        };

        assert!(args.remove_existing_container);
        assert!(args.skip_post_create);
        assert!(args.skip_non_blocking_commands);
        assert!(args.ports_events);
        assert!(args.shutdown);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_compose_config_detection() {
        let mut compose_config = DevContainerConfig::default();
        compose_config.name = Some("Test Compose".to_string());
        compose_config.docker_compose_file = Some(json!("docker-compose.yml"));
        compose_config.service = Some("app".to_string());
        compose_config.run_services = vec!["db".to_string()];
        compose_config.shutdown_action = Some("stopCompose".to_string());
        compose_config.post_create_command = Some(json!("echo 'Container ready'"));

        assert!(compose_config.uses_compose());
        assert!(compose_config.has_stop_compose_shutdown());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_traditional_config_detection() {
        let mut traditional_config = DevContainerConfig::default();
        traditional_config.name = Some("Test Traditional".to_string());
        traditional_config.image = Some("node:18".to_string());

        assert!(!traditional_config.uses_compose());
        assert!(!traditional_config.has_stop_compose_shutdown());
    }
}
