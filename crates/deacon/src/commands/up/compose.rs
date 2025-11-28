//! Docker Compose flow for the up command.
//!
//! This module contains:
//! - `execute_compose_up` - Main compose up execution
//! - `execute_compose_post_create` - Post-create lifecycle for compose
//! - `handle_compose_shutdown` - Shutdown handling for compose

use super::args::{MountType, NormalizedMount, UpArgs};
use super::lifecycle::{execute_initialize_command, resolve_force_pty};
use super::merged_config::{
    build_merged_configuration_with_options, inspect_for_merged_configuration,
};
use super::ports::handle_port_events;
use super::result::{EffectiveMount, UpContainerInfo};
use super::ENV_LOG_FORMAT;
use anyhow::{Context, Result};
use deacon_core::compose::{ComposeCommand, ComposeManager, ComposeProject};
use deacon_core::config::DevContainerConfig;
use deacon_core::docker::Docker;
use deacon_core::docker::ExecConfig;
use deacon_core::errors::{DeaconError, DockerError};
use deacon_core::runtime::ContainerRuntimeImpl;
use deacon_core::state::{ComposeState, StateManager};
use deacon_core::IndexMap;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

/// Execute up for Docker Compose configurations
#[allow(clippy::needless_borrows_for_generic_args)] // config borrowed twice for serialization
#[allow(clippy::too_many_arguments)]
#[instrument(skip(config, workspace_folder, args, state_manager, runtime))]
pub(crate) async fn execute_compose_up(
    config: &DevContainerConfig,
    workspace_folder: &Path,
    args: &UpArgs,
    state_manager: &mut StateManager,
    workspace_hash: &str,
    effective_env: &IndexMap<String, String>,
    config_path: &Path,
    runtime: &ContainerRuntimeImpl,
) -> Result<UpContainerInfo> {
    debug!("Starting Docker Compose project");

    let compose_manager = ComposeManager::with_docker_path(args.docker_path.clone());
    let mut project = compose_manager.create_project(config, workspace_folder)?;

    // Add env files from CLI args
    project.env_files = args.env_file.clone();

    // Apply default workspace mount for Compose when consistency is provided
    // Per FR-001: workspace_mount_consistency MUST apply to both Docker and Compose outputs
    // This mirrors the Docker behavior in execute_docker_up()
    let mut additional_mounts = Vec::new();
    if args.workspace_mount_consistency.is_some() {
        // Compute target path (container path) - same logic as Docker path
        let target_path = config.workspace_folder.clone().unwrap_or_else(|| {
            format!(
                "/workspaces/{}",
                workspace_folder
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("workspace")
            )
        });
        // Source path is the resolved workspace_folder (already canonicalized in execute_up)
        let source_path = workspace_folder.display().to_string();

        additional_mounts.push(deacon_core::compose::ComposeMount {
            mount_type: "bind".to_string(),
            source: source_path,
            target: target_path,
            read_only: false,
            consistency: args.workspace_mount_consistency.clone(),
        });
        debug!(
            "Added default workspace mount for Compose with consistency: {:?}",
            args.workspace_mount_consistency
        );
    }

    // Apply CLI mounts to compose project
    // Per CLAUDE.md: No silent fallbacks - fail fast on invalid mounts
    for mount_str in &args.mount {
        let mount = NormalizedMount::parse(mount_str)
            .with_context(|| format!("Invalid mount specification: {}", mount_str))?;
        additional_mounts.push(deacon_core::compose::ComposeMount {
            mount_type: match mount.mount_type {
                MountType::Bind => "bind".to_string(),
                MountType::Volume => "volume".to_string(),
            },
            source: mount.source.clone(),
            target: mount.target.clone(),
            read_only: mount.read_only,
            consistency: mount.consistency.clone(),
        });
    }
    if !additional_mounts.is_empty() {
        project.additional_mounts = additional_mounts;
    }

    // Apply remote env to compose services
    if !effective_env.is_empty() {
        project.additional_env = effective_env.clone();
    }

    // Per T006: Mount/env injection is now handled via ComposeManager::start_project()
    // which uses ComposeProject::generate_injection_override() to pipe YAML via stdin.
    // No temporary override files are created.

    // Populate external volumes from compose config.
    // This enables tracking which volumes are external for validation and preservation.
    // Per spec: external volumes must not be replaced or mutated by injection.
    // Note: This operation requires Docker - if unavailable, we continue without
    // external volume information as this is non-blocking for the core up workflow.
    if let Err(e) = compose_manager.populate_external_volumes(&mut project) {
        debug!(
            "Could not populate external volumes (Docker may be unavailable): {}",
            e
        );
    }

    debug!("Created compose project: {:?}", project.name);

    // If we expect an existing project, fail fast when it's not running.
    if args.expect_existing_container {
        match compose_manager.is_project_running(&project) {
            Ok(true) => { /* ok */ }
            Ok(false) => {
                return Err(DeaconError::Docker(DockerError::ContainerNotFound {
                    id: project.name.clone(),
                })
                .into());
            }
            Err(e) => return Err(e.into()),
        }
    }

    // Check if project is already running
    if !args.remove_existing_container {
        match compose_manager.is_project_running(&project) {
            Ok(true) => {
                debug!("Compose project {} is already running", project.name);
                // Get the primary container ID for potential exec operations
                let container_id = compose_manager
                    .get_primary_container_id(&project)?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Failed to get primary container ID for running compose project"
                        )
                    })?;
                debug!("Primary service container ID: {}", container_id);

                // Return container info for already-running project
                let remote_user = config
                    .remote_user
                    .clone()
                    .or_else(|| config.container_user.clone())
                    .unwrap_or_else(|| "root".to_string());

                let remote_workspace_folder = config
                    .workspace_folder
                    .clone()
                    .unwrap_or_else(|| "/workspaces".to_string());

                // Serialize configuration if requested
                let configuration = if args.include_configuration {
                    Some(serde_json::to_value(config)?)
                } else {
                    None
                };

                // Existing container reconnect - no resolved features available
                let merged_configuration = if args.include_merged_configuration {
                    // Use shared helper with injected runtime (respects --docker-path)
                    let options = inspect_for_merged_configuration(
                        runtime,
                        &container_id,
                        config.image.as_deref(),
                        Some(project.service.clone()),
                        None, // No resolved features for reconnect
                    )
                    .await;
                    Some(build_merged_configuration_with_options(
                        config,
                        config_path,
                        options,
                    )?)
                } else {
                    None
                };

                return Ok(UpContainerInfo {
                    container_id,
                    remote_user,
                    remote_workspace_folder,
                    compose_project_name: Some(project.name.clone()),
                    // For existing container reconnect, we don't have injection data
                    effective_mounts: None,
                    effective_env: None,
                    profiles_applied: None,
                    external_volumes_preserved: None,
                    configuration,
                    merged_configuration,
                });
            }
            Ok(false) => {
                // Not running, continue
            }
            Err(e) => {
                warn!(
                    "Failed to determine compose project state (continuing): {}",
                    e
                );
            }
        }
    }

    // Execute initializeCommand on host before starting compose operations
    if let Some(ref initialize) = config.initialize_command {
        execute_initialize_command(initialize, workspace_folder, &args.progress_tracker).await?;
    }

    // Stop existing containers if requested
    if args.remove_existing_container {
        debug!("Stopping existing compose project");
        if let Err(e) = compose_manager.stop_project(&project) {
            warn!("Failed to stop existing project: {}", e);
        }
    }

    // Start the compose project
    // First, warn about security options that cannot be applied dynamically
    ComposeCommand::warn_security_options_for_compose(config);

    // Log GPU mode application for compose
    if args.gpu_mode == deacon_core::gpu::GpuMode::All {
        info!("Applying GPU mode: all - requesting GPU access for compose services");
    } else if args.gpu_mode != deacon_core::gpu::GpuMode::None {
        debug!("GPU mode for compose: {:?}", args.gpu_mode);
    }

    compose_manager.start_project(&project, args.gpu_mode)?;

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
        // Resolve PTY preference for compose post-create (same logic as lifecycle commands)
        let json_mode = std::env::var(ENV_LOG_FORMAT)
            .map(|v| v == "json")
            .unwrap_or(false);
        let force_pty = resolve_force_pty(args.force_tty_if_json, json_mode);
        execute_compose_post_create(&project, config, &args.docker_path, force_pty).await?;
    }

    // Handle port forwarding and events
    if args.ports_events {
        handle_port_events(
            config,
            &project,
            &args.redaction_config,
            &args.secret_registry,
            &args.docker_path,
        )
        .await?;
    }

    // Handle shutdown if requested
    if args.shutdown {
        handle_compose_shutdown(
            config,
            &project,
            state_manager,
            workspace_hash,
            &args.docker_path,
        )
        .await?;
    }

    // Collect container information for JSON output
    // Retry getting container ID with exponential backoff to handle race conditions
    let container_id = {
        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 10;
        const INITIAL_DELAY_MS: u64 = 100;

        loop {
            match compose_manager.get_primary_container_id(&project)? {
                Some(id) => break id,
                None if attempts < MAX_ATTEMPTS => {
                    attempts += 1;
                    let delay = Duration::from_millis(INITIAL_DELAY_MS * 2u64.pow(attempts - 1));
                    debug!(
                        "Waiting for container to be ready, attempt {}/{}, waiting {:?}",
                        attempts, MAX_ATTEMPTS, delay
                    );
                    tokio::time::sleep(delay).await;
                }
                None => {
                    return Err(anyhow::anyhow!(
                        "Failed to get primary container ID after starting compose project (tried {} times)",
                        MAX_ATTEMPTS
                    ));
                }
            }
        }
    };

    let remote_user = config
        .remote_user
        .clone()
        .or_else(|| config.container_user.clone())
        .unwrap_or_else(|| "root".to_string());

    let remote_workspace_folder = config
        .workspace_folder
        .clone()
        .unwrap_or_else(|| "/workspaces".to_string());

    // Serialize configuration if requested
    let configuration = if args.include_configuration {
        Some(serde_json::to_value(&config)?)
    } else {
        None
    };

    let merged_configuration = if args.include_merged_configuration {
        // Use shared helper with injected runtime (respects --docker-path)
        let options = inspect_for_merged_configuration(
            runtime,
            &container_id,
            config.image.as_deref(),
            Some(project.service.clone()),
            None, // Features not yet supported for compose flow
        )
        .await;
        Some(build_merged_configuration_with_options(
            config,
            config_path,
            options,
        )?)
    } else {
        None
    };

    // Capture effective mounts from compose project
    let effective_mounts = if project.additional_mounts.is_empty() {
        None
    } else {
        Some(
            project
                .additional_mounts
                .iter()
                .map(|m| {
                    let mut options = Vec::new();
                    if m.read_only {
                        options.push("ro".to_string());
                    }
                    if let Some(ref consistency) = m.consistency {
                        options.push(format!("consistency={}", consistency));
                    }
                    EffectiveMount {
                        source: m.source.clone(),
                        target: m.target.clone(),
                        options,
                    }
                })
                .collect(),
        )
    };

    // Capture effective env from compose project
    let effective_env = if project.additional_env.is_empty() {
        None
    } else {
        Some(
            project
                .additional_env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<HashMap<_, _>>(),
        )
    };

    // Capture profiles applied
    let profiles_applied = if project.profiles.is_empty() {
        None
    } else {
        Some(project.profiles.clone())
    };

    // Capture external volumes preserved
    let external_volumes_preserved = if project.external_volumes.is_empty() {
        None
    } else {
        Some(project.external_volumes.clone())
    };

    Ok(UpContainerInfo {
        container_id,
        remote_user,
        remote_workspace_folder,
        compose_project_name: Some(project.name.clone()),
        effective_mounts,
        effective_env,
        profiles_applied,
        external_volumes_preserved,
        configuration,
        merged_configuration,
    })
}

/// Execute post-create lifecycle for compose projects
#[instrument(skip(project, config, docker_path))]
pub(crate) async fn execute_compose_post_create(
    project: &ComposeProject,
    config: &DevContainerConfig,
    docker_path: &str,
    force_pty: bool,
) -> Result<()> {
    debug!("Executing post-create lifecycle for compose project");

    // Get the primary container ID
    let compose_manager = ComposeManager::with_docker_path(docker_path.to_string());
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
            debug!("Executing postCreateCommand: {}", cmd_str);

            let docker = deacon_core::docker::CliDocker::new();
            let result = docker
                .exec(
                    &container_id,
                    &["sh".to_string(), "-c".to_string(), cmd_str.to_string()],
                    ExecConfig {
                        user: None,
                        working_dir: None,
                        env: std::collections::HashMap::new(),
                        tty: force_pty,
                        interactive: false,
                        detach: false,
                        silent: false,
                        terminal_size: None,
                    },
                )
                .await;

            match result {
                Ok(_) => debug!("postCreateCommand completed successfully"),
                Err(e) => warn!("postCreateCommand failed: {}", e),
            }
        }
    }

    Ok(())
}

/// Handle shutdown for compose configurations
#[instrument(skip(config, state_manager, docker_path))]
pub(crate) async fn handle_compose_shutdown(
    config: &DevContainerConfig,
    project: &ComposeProject,
    state_manager: &mut StateManager,
    workspace_hash: &str,
    docker_path: &str,
) -> Result<()> {
    debug!("Handling shutdown for compose project: {}", project.name);

    let shutdown_action = config.shutdown_action.as_deref().unwrap_or("stopCompose");

    match shutdown_action {
        "none" => {
            debug!("Shutdown action is 'none', leaving compose project running");
        }
        "stopCompose" => {
            debug!("Stopping compose project due to shutdown action");
            let compose_manager = ComposeManager::with_docker_path(docker_path.to_string());
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
