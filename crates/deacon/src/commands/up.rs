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
use deacon_core::features::{FeatureMergeConfig, FeatureMerger};
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
    pub additional_features: Option<String>,
    pub prefer_cli_features: bool,
    pub feature_install_order: Option<String>,
    pub ignore_host_requirements: bool,
    pub progress_tracker:
        std::sync::Arc<std::sync::Mutex<Option<deacon_core::progress::ProgressTracker>>>,
}

/// Starts development containers for the current workspace according to the resolved devcontainer configuration.
///
/// This is the top-level entry point for the `up` command. It:
/// - Loads or discovers the devcontainer configuration (from `args.config_path` or `args.workspace_folder`).
/// - Validates host requirements (unless skipped via flags).
/// - Optionally merges CLI-provided feature modifications into the effective configuration.
/// - Creates a workspace container identity and initializes state tracking.
/// - Delegates to either the Docker Compose flow or the single-container flow depending on the configuration.
/// - Emits progress events to the optional shared progress tracker and logs a final metrics summary when available.
///
/// Returns Ok(()) on success; errors returned by configuration loading, host-requirements validation,
/// feature merging, container/compose operations, or state management are propagated.
///
/// # Examples
///
/// ```no_run
/// use std::path::PathBuf;
/// use tokio::runtime::Runtime;
///
/// // Construct minimal arguments for a workspace at the current directory.
/// let mut args = deacon::commands::up::UpArgs::default();
/// args.workspace_folder = Some(PathBuf::from("."));
///
/// let rt = Runtime::new().unwrap();
/// rt.block_on(async {
///     let _ = deacon::commands::up::execute_up(args).await;
/// });
/// ```
#[instrument(skip(args))]
pub async fn execute_up(args: UpArgs) -> Result<()> {
    info!("Starting up command execution");
    debug!("Up args: {:?}", args);

    // Initialize progress tracking
    let _emit_progress_event = |event: deacon_core::progress::ProgressEvent| -> Result<()> {
        if let Ok(mut tracker_guard) = args.progress_tracker.lock() {
            if let Some(ref mut tracker) = tracker_guard.as_mut() {
                tracker.emit_event(event)?;
            }
        }
        Ok(())
    };

    // Load configuration
    let workspace_folder = args.workspace_folder.as_deref().unwrap_or(Path::new("."));

    let mut config = if let Some(config_path) = args.config_path.as_ref() {
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

    // Validate host requirements if specified in configuration
    if let Some(host_requirements) = &config.host_requirements {
        info!("Validating host requirements");
        let mut evaluator = deacon_core::host_requirements::HostRequirementsEvaluator::new();

        match evaluator.validate_requirements(
            host_requirements,
            Some(workspace_folder),
            args.ignore_host_requirements,
        ) {
            Ok(evaluation) => {
                if evaluation.requirements_met {
                    info!("Host requirements validation passed");
                } else if args.ignore_host_requirements {
                    warn!("Host requirements not met, but proceeding due to --ignore-host-requirements flag");
                }
                debug!("Host evaluation: {:?}", evaluation);
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    } else {
        debug!("No host requirements specified in configuration");
    }

    // Apply feature merging if CLI features are provided
    if args.additional_features.is_some() || args.feature_install_order.is_some() {
        let merge_config = FeatureMergeConfig::new(
            args.additional_features.clone(),
            args.prefer_cli_features,
            args.feature_install_order.clone(),
        );

        // Merge features
        config.features = FeatureMerger::merge_features(&config.features, &merge_config)?;
        debug!("Applied feature merging");

        // Update override feature install order if provided
        if let Some(effective_order) = FeatureMerger::get_effective_install_order(
            config.override_feature_install_order.as_ref(),
            &merge_config,
        )? {
            config.override_feature_install_order = Some(effective_order);
            debug!("Updated feature install order");
        }
    }

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
        .await?;
    } else {
        execute_container_up(
            &config,
            workspace_folder,
            &args,
            &mut state_manager,
            &workspace_hash,
        )
        .await?;
    }

    // Output final metrics summary in debug mode
    if let Ok(tracker_guard) = args.progress_tracker.lock() {
        if let Some(tracker) = tracker_guard.as_ref() {
            if let Some(metrics_summary) = tracker.metrics_summary() {
                debug!("Final metrics summary: {:?}", metrics_summary);
            }
        }
    }

    Ok(())
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

/// Start and manage a single traditional development container for the given workspace.
///
/// This function ensures Docker is available, creates or reuses a container (deterministically
/// named from the workspace and config), emits progress events when a shared progress tracker
/// is provided, records timing metrics, saves runtime state for later shutdown handling, and
/// runs configured user-mapping and lifecycle commands. Optionally emits port events and
/// performs the configured shutdown action.
///
/// The function returns an error if Docker is unreachable, container creation/start fails,
/// state persistence fails, or any lifecycle/post-create actions fail; errors are propagated
/// through the returned `Result`.
///
/// Parameters:
/// - `workspace_hash`: identifier used to persist workspace-specific runtime state.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// // Setup `config`, `workspace_folder`, `args`, and `state_manager` according to your test harness.
/// // Then call the async function from a Tokio runtime:
/// // tokio::runtime::Runtime::new().unwrap().block_on(async {
/// //     execute_container_up(&config, &workspace_folder, &args, &mut state_manager, &workspace_hash).await.unwrap();
/// // });
/// ```
#[instrument(skip_all)]
async fn execute_container_up(
    config: &DevContainerConfig,
    workspace_folder: &Path,
    args: &UpArgs,
    state_manager: &mut StateManager,
    workspace_hash: &str,
) -> Result<()> {
    info!("Starting traditional development container");

    // Initialize progress tracking
    let emit_progress_event = |event: deacon_core::progress::ProgressEvent| -> Result<()> {
        if let Ok(mut tracker_guard) = args.progress_tracker.lock() {
            if let Some(ref mut tracker) = tracker_guard.as_mut() {
                tracker.emit_event(event)?;
            }
        }
        Ok(())
    };

    // Create container identity for deterministic naming and labels
    let identity = ContainerIdentity::new(workspace_folder, config);
    debug!("Container identity: {:?}", identity);

    // Initialize Docker client
    let docker = CliDocker::new();

    // Check Docker availability
    docker.ping().await?;

    // Emit container create begin event
    emit_progress_event(deacon_core::progress::ProgressEvent::ContainerCreateBegin {
        id: deacon_core::progress::ProgressTracker::next_event_id(),
        timestamp: deacon_core::progress::ProgressTracker::current_timestamp(),
        name: identity
            .name
            .clone()
            .unwrap_or_else(|| "unnamed".to_string()),
        image: config
            .image
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
    })?;

    let container_start_time = std::time::Instant::now();

    // Create container using DockerLifecycle trait
    use deacon_core::docker::DockerLifecycle;
    let container_result = docker
        .up(
            &identity,
            config,
            workspace_folder,
            args.remove_existing_container,
        )
        .await;

    let container_duration = container_start_time.elapsed();
    let container_success = container_result.is_ok();
    let container_id = container_result
        .as_ref()
        .ok()
        .map(|r| r.container_id.clone());

    // Emit container create end event
    emit_progress_event(deacon_core::progress::ProgressEvent::ContainerCreateEnd {
        id: deacon_core::progress::ProgressTracker::next_event_id(),
        timestamp: deacon_core::progress::ProgressTracker::current_timestamp(),
        name: identity
            .name
            .clone()
            .unwrap_or_else(|| "unnamed".to_string()),
        duration_ms: container_duration.as_millis() as u64,
        success: container_success,
        container_id,
    })?;

    // Record metrics
    if let Ok(tracker_guard) = args.progress_tracker.lock() {
        if let Some(tracker) = tracker_guard.as_ref() {
            tracker.record_duration("container.create", container_duration);
        }
    }

    let container_result = container_result?;

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

/// Execute configured lifecycle phases inside a running container.
///
/// This runs the lifecycle command phases defined in `config` (onCreate, postCreate,
/// postStart, postAttach) in that order, emitting per-phase progress events to
/// `args.progress_tracker` when present and recording an overall lifecycle duration metric.
///
/// Parameters:
/// - `container_id`: container identifier where commands will be executed.
/// - `config`: devcontainer configuration containing lifecycle command definitions and environment.
/// - `workspace_folder`: host path used to build the substitution context and to derive the container workspace path when not explicitly set in `config`.
/// - `args`: runtime flags that influence execution (e.g., skipping post-create, non-blocking behavior) and an optional progress tracker.
///
/// Behavior notes:
/// - Commands may be provided as a single string or an array in the config; non-string entries produce a configuration validation error.
/// - Emits LifecyclePhaseBegin for each phase before execution and LifecyclePhaseEnd for each phase after execution (end events contain an approximate per-phase duration).
/// - Records the total lifecycle duration under the metric name "lifecycle" if a progress tracker is available.
/// - Returns any error produced by the underlying lifecycle executor.
///
/// # Examples
///
/// ```no_run
/// # use std::path::Path;
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Prepare inputs (placeholders shown; real values come from your application)
/// let container_id = "container-123";
/// let config = deacon_core::config::DevContainerConfig::default();
/// let workspace_folder = Path::new("/path/to/workspace");
/// let args = crate::commands::up::UpArgs::default();
///
/// // Execute lifecycle phases inside the container
/// execute_lifecycle_commands(container_id, &config, workspace_folder, &args).await?;
/// # Ok(()) }
/// ```
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

    // Initialize progress tracking
    let emit_progress_event = |event: deacon_core::progress::ProgressEvent| -> Result<()> {
        if let Ok(mut tracker_guard) = args.progress_tracker.lock() {
            if let Some(ref mut tracker) = tracker_guard.as_mut() {
                tracker.emit_event(event)?;
            }
        }
        Ok(())
    };

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
    let mut phases_to_execute = Vec::new();

    if let Some(ref on_create) = config.on_create_command {
        let phase_commands = commands_from_json_value(on_create)?;
        commands = commands.with_on_create(phase_commands.clone());
        phases_to_execute.push(("onCreate".to_string(), phase_commands));
    }

    if let Some(ref post_create) = config.post_create_command {
        let phase_commands = commands_from_json_value(post_create)?;
        commands = commands.with_post_create(phase_commands.clone());
        phases_to_execute.push(("postCreate".to_string(), phase_commands));
    }

    if let Some(ref post_start) = config.post_start_command {
        let phase_commands = commands_from_json_value(post_start)?;
        commands = commands.with_post_start(phase_commands.clone());
        phases_to_execute.push(("postStart".to_string(), phase_commands));
    }

    if let Some(ref post_attach) = config.post_attach_command {
        let phase_commands = commands_from_json_value(post_attach)?;
        commands = commands.with_post_attach(phase_commands.clone());
        phases_to_execute.push(("postAttach".to_string(), phase_commands));
    }

    // Emit begin events for each phase
    for (phase_name, phase_commands) in &phases_to_execute {
        emit_progress_event(deacon_core::progress::ProgressEvent::LifecyclePhaseBegin {
            id: deacon_core::progress::ProgressTracker::next_event_id(),
            timestamp: deacon_core::progress::ProgressTracker::current_timestamp(),
            phase: phase_name.clone(),
            commands: phase_commands.clone(),
        })?;
    }

    let lifecycle_start_time = std::time::Instant::now();

    // Execute lifecycle commands
    let result =
        execute_container_lifecycle(&lifecycle_config, &commands, &substitution_context).await;

    let lifecycle_duration = lifecycle_start_time.elapsed();
    let lifecycle_success = result.is_ok();

    // Emit end events for each phase (in reverse order since execution is complete)
    for (phase_name, _) in phases_to_execute.iter().rev() {
        emit_progress_event(deacon_core::progress::ProgressEvent::LifecyclePhaseEnd {
            id: deacon_core::progress::ProgressTracker::next_event_id(),
            timestamp: deacon_core::progress::ProgressTracker::current_timestamp(),
            phase: phase_name.clone(),
            duration_ms: lifecycle_duration.as_millis() as u64 / phases_to_execute.len() as u64, // Approximate duration per phase
            success: lifecycle_success,
        })?;
    }

    // Record metrics
    if let Ok(tracker_guard) = args.progress_tracker.lock() {
        if let Some(tracker) = tracker_guard.as_ref() {
            tracker.record_duration("lifecycle", lifecycle_duration);
        }
    }

    let result = result?;

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
            additional_features: None,
            prefer_cli_features: false,
            feature_install_order: None,
            ignore_host_requirements: false,
            progress_tracker: std::sync::Arc::new(std::sync::Mutex::new(None)),
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
            additional_features: None,
            prefer_cli_features: false,
            feature_install_order: None,
            ignore_host_requirements: false,
            progress_tracker: std::sync::Arc::new(std::sync::Mutex::new(None)),
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
