//! Docker Compose flow for the up command.
//!
//! This module contains:
//! - `execute_compose_up` - Main compose up execution
//! - `execute_compose_post_create` - Post-create lifecycle for compose
//! - `handle_compose_shutdown` - Shutdown handling for compose

use super::ENV_LOG_FORMAT;
use super::args::{MountType, NormalizedMount, UpArgs};
use super::features_build::{
    FeatureBuildOutput, build_image_with_features, build_image_with_features_from_dockerfile,
};
use super::helpers::handle_lockfile_post_build;
use super::lifecycle::{HostTrustArgs, execute_initialize_command, resolve_force_pty};
use super::merged_config::{
    build_merged_configuration_with_options, inspect_for_merged_configuration,
};
use super::ports::handle_port_events;
use super::result::{EffectiveMount, UpContainerInfo};
use anyhow::{Context, Result};
use deacon_core::IndexMap;
use deacon_core::compose::{ComposeCommand, ComposeManager, ComposeProject, ServiceShape};
use deacon_core::config::DevContainerConfig;
use deacon_core::container::ContainerIdentity;
use deacon_core::docker::Docker;
use deacon_core::docker::ExecConfig;
use deacon_core::errors::{DeaconError, DockerError};
use deacon_core::runtime::ContainerRuntimeImpl;
use deacon_core::state::{ComposeState, StateManager};
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
    identity: &ContainerIdentity,
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

    // Spec parity (#100): stamp the same deacon identity labels the
    // single-container path uses onto every compose service. Without these, VS
    // Code Dev Containers reconnect / `docker ps --filter
    // label=devcontainer.local_folder=<abs>` / `deacon exec --id-label` all
    // miss compose-managed containers. The identity is the canonical
    // (as-loaded) one computed by the caller (#187) — it must NOT be rebuilt
    // from the post-substitution `config` here, or the stamped `configHash`
    // could drift from what `exec`/`down` compute.
    for (key, value) in identity.labels() {
        project.deacon_labels.insert(key, value);
    }

    // Apply default workspace mount for Compose when consistency is provided
    // Per FR-001: workspace_mount_consistency MUST apply to both Docker and Compose outputs
    // This mirrors the Docker behavior in execute_docker_up().
    //
    // Spec parity (#67): when `--mount-workspace-git-root` is true, the
    // mount *source* walks up to the enclosing git root so git operations
    // inside the container work; otherwise the user's workspace folder.
    // Discovery has already used the user's path by this point.
    let mut additional_mounts = Vec::new();
    if args.workspace_mount_consistency.is_some() {
        let mount_source = if args.mount_workspace_git_root {
            deacon_core::workspace::resolve_workspace_root(workspace_folder)?
        } else {
            workspace_folder.to_path_buf()
        };
        let target_path = config.workspace_folder.clone().unwrap_or_else(|| {
            format!(
                "/workspaces/{}",
                mount_source
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("workspace")
            )
        });

        additional_mounts.push(deacon_core::compose::ComposeMount {
            mount_type: "bind".to_string(),
            source: mount_source.display().to_string(),
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

    // Populate profiles from compose config.
    // Per spec §7: detect profiles for services in runServices and pass them
    // via --profile flags to all compose commands.
    // Uses `docker compose config --format json` (same pattern as external volumes).
    if let Err(e) = compose_manager.populate_profiles(&mut project).await {
        debug!(
            "Could not detect compose profiles (Docker may be unavailable): {}",
            e
        );
    }

    // Populate external volumes from compose config.
    // This enables tracking which volumes are external for validation and preservation.
    // Per spec: external volumes must not be replaced or mutated by injection.
    // Note: This operation requires Docker - if unavailable, we continue without
    // external volume information as this is non-blocking for the core up workflow.
    if let Err(e) = compose_manager
        .populate_external_volumes(&mut project)
        .await
    {
        debug!(
            "Could not populate external volumes (Docker may be unavailable): {}",
            e
        );
    }

    debug!("Created compose project: {:?}", project.name);

    // If we expect an existing project, fail fast when it's not running.
    if args.expect_existing_container {
        match compose_manager.is_project_running(&project).await {
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
        match compose_manager.is_project_running(&project).await {
            Ok(true) => {
                debug!("Compose project {} is already running", project.name);
                // Get the primary container ID for potential exec operations
                let container_id = compose_manager
                    .get_primary_container_id(&project)
                    .await?
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
        let trust_args = HostTrustArgs {
            trust_workspace: args.trust_workspace,
            trust_workspace_persist: args.trust_workspace_persist,
            user_data_folder: args.user_data_folder.as_deref(),
        };
        execute_initialize_command(
            initialize,
            workspace_folder,
            &args.progress_tracker,
            trust_args,
        )
        .await?;
    }

    // Stop existing containers if requested
    if args.remove_existing_container {
        debug!("Stopping existing compose project");
        if let Err(e) = compose_manager.stop_project(&project).await {
            warn!("Failed to stop existing project: {}", e);
        }
    }

    // Bead 14a + 14b: when features are declared, install them by building a
    // feature-extended image and rewriting the target service's `image:` via
    // the existing injection override. Both the `image:` shape (14a) and the
    // `build:` shape (14b — user-authored Dockerfile + context) are supported.
    // Future work (per spec): thread resolved_features into merged_configuration.
    let feature_build = install_features_for_compose(
        config,
        &compose_manager,
        &mut project,
        workspace_folder,
        config_path,
        workspace_hash,
    )
    .await?;

    // Lockfile graduation (PR-4b): mirror the single-container flow — write
    // the lockfile to disk, or byte-compare it in `--frozen-lockfile` mode.
    // Only runs when features were actually built (the compose path returns
    // `None` when no features are declared).
    if let Some(ref fb) = feature_build {
        handle_lockfile_post_build(args, config_path, &fb.lockfile).await?;
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

    compose_manager
        .start_project(&project, args.gpu_mode)
        .await?;

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
            match compose_manager.get_primary_container_id(&project).await? {
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

    // Start the detached port forwarder for the primary service container if
    // requested. Declared `"service:port"` specs relay over the compose network
    // to the named service; auto-detection stays scoped to the primary service
    // (FR-023). Best-effort (FR-002, FR-025).
    if args.auto_forward {
        let declared = super::forward::declared_port_specs(config, &args.forward_ports);
        super::forward::spawn_or_adopt(
            args,
            &container_id,
            workspace_folder,
            config_path,
            &declared,
        )
        .await;
    }

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
    let container_id = match compose_manager.get_primary_container_id(project).await? {
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
                        stdout_to_stderr: true,
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

/// Bead 14a + 14b: install features into a compose-based devcontainer.
///
/// Workflow when `config.features` is non-empty:
/// 1. Inspect the target service's shape via `docker compose config --format json`.
/// 2. For `image:` services (bead 14a), synthesize a single-container config whose
///    `image` is the resolved compose image and whose `features` is the original
///    config's features; reuse `build_image_with_features` to produce a
///    feature-extended image.
/// 3. For `build:` services (bead 14b), resolve `dockerfile` and `context` paths
///    relative to the compose file's directory (NOT the workspace), read the
///    user's Dockerfile, rewrite its final `FROM` to carry a stable alias via
///    `ensure_dockerfile_has_final_stage_name`, then build with that as the
///    base via `build_image_with_features_from_dockerfile`.
/// 4. In both cases, set `project.service_image_override` so the existing
///    injection override rewrites the target service's `image:` line to point
///    at the extended tag.
#[instrument(skip(config, compose_manager, project, workspace_folder))]
async fn install_features_for_compose(
    config: &DevContainerConfig,
    compose_manager: &ComposeManager,
    project: &mut ComposeProject,
    workspace_folder: &Path,
    config_path: &Path,
    workspace_hash: &str,
) -> Result<Option<FeatureBuildOutput>> {
    let output = match resolve_compose_feature_image(
        config,
        compose_manager,
        project,
        workspace_folder,
        config_path,
        workspace_hash,
    )
    .await?
    {
        Some(o) => o,
        None => return Ok(None),
    };

    // `up` rewrites the target service's `image:` line to the extended tag so
    // the container runs with features installed.
    project.service_image_override = Some(output.image_tag.clone());
    Ok(Some(output))
}

/// Resolve (and build) the feature-extended image for a compose service, without
/// mutating the project. Shared by `up` (which then sets
/// `service_image_override` to run the extended image) and `build` (which tags
/// the produced image for the user and writes the lockfile). Returns `None` when
/// the config declares no features.
#[instrument(skip(config, compose_manager, project, workspace_folder))]
pub(crate) async fn resolve_compose_feature_image(
    config: &DevContainerConfig,
    compose_manager: &ComposeManager,
    project: &ComposeProject,
    workspace_folder: &Path,
    config_path: &Path,
    workspace_hash: &str,
) -> Result<Option<FeatureBuildOutput>> {
    // Nothing to install when features is missing or an empty object.
    let features_obj = match config.features.as_object() {
        Some(o) if !o.is_empty() => o,
        _ => {
            debug!("No features declared on compose config; skipping feature build");
            return Ok(None);
        }
    };
    debug!(
        feature_count = features_obj.len(),
        service = %project.service,
        "Resolving compose service shape for feature install"
    );

    let shape = compose_manager
        .get_command(project)
        .extract_service_shape(&project.service)
        .await
        .with_context(|| {
            format!(
                "Failed to resolve compose service '{}' shape via `docker compose config`",
                project.service
            )
        })?;

    // Compose-flavored identity: produced image tag is namespaced by
    // workspace+service so it does not collide with the single-container path.
    let mut identity = ContainerIdentity::new(workspace_folder, config);
    identity.workspace_hash = format!("{}-compose-{}", workspace_hash, project.service);

    let output = match shape {
        ServiceShape::Image(base_image) => {
            info!(
                service = %project.service,
                base_image = %base_image,
                "Building feature-extended image for compose service (image: shape)"
            );

            // Synthesize a single-container config so `build_image_with_features`
            // can consume it: only `image`, `features`, and
            // `override_feature_install_order` are read.
            let mut synth_config = config.clone();
            synth_config.image = Some(base_image.clone());

            build_image_with_features(
                &synth_config,
                &identity,
                workspace_folder,
                config_path,
                None,
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to build feature-extended image for compose service '{}'",
                    project.service
                )
            })?
        }
        ServiceShape::Build {
            context,
            dockerfile,
            target,
        } => {
            // Compose semantics: `build.context` and `build.dockerfile` are
            // resolved relative to the directory containing the compose file —
            // NOT the workspace folder. When multiple compose files are stacked,
            // we use the first one's directory (`docker compose` itself returns
            // paths as if they were declared in the primary compose file).
            let compose_dir = project
                .compose_files
                .first()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| project.base_path.clone());

            // Default context to `.` per compose schema.
            let context_rel = context.as_deref().unwrap_or(".");
            let context_path = resolve_compose_path(&compose_dir, context_rel);

            // Default dockerfile to `Dockerfile` relative to the *context*, per
            // compose semantics (NOT relative to the compose file directory).
            let dockerfile_rel = dockerfile.as_deref().unwrap_or("Dockerfile");
            let dockerfile_path = resolve_compose_path(&context_path, dockerfile_rel);

            info!(
                service = %project.service,
                context = %context_path.display(),
                dockerfile = %dockerfile_path.display(),
                target = ?target,
                "Building feature-extended image for compose service (build: shape)"
            );

            let dockerfile_content = tokio::fs::read_to_string(&dockerfile_path)
                .await
                .with_context(|| {
                    format!(
                        "Failed to read Dockerfile for compose service '{}' at {}",
                        project.service,
                        dockerfile_path.display()
                    )
                })?;

            let (modified_dockerfile, final_stage) =
                deacon_core::dockerfile_utils::ensure_dockerfile_has_final_stage_name(
                    &dockerfile_content,
                    "dev_containers_user_image",
                )
                .with_context(|| {
                    format!(
                        "Failed to parse Dockerfile for compose service '{}' at {}",
                        project.service,
                        dockerfile_path.display()
                    )
                })?;

            build_image_with_features_from_dockerfile(
                config,
                &identity,
                &modified_dockerfile,
                &final_stage,
                &context_path,
                config_path,
                target.as_deref(),
                None,
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to build feature-extended image for compose service '{}' \
                     using Dockerfile {}",
                    project.service,
                    dockerfile_path.display()
                )
            })?
        }
        ServiceShape::Neither => {
            return Err(DeaconError::Runtime(format!(
                "Compose service '{}' has neither `image:` nor `build:`; cannot \
                 install features against an undefined base",
                project.service
            ))
            .into());
        }
        ServiceShape::NotFound => {
            return Err(DeaconError::Runtime(format!(
                "Compose service '{}' not found in resolved compose config",
                project.service
            ))
            .into());
        }
    };

    info!(
        service = %project.service,
        extended_image = %output.image_tag,
        feature_count = output.resolved_features.len(),
        "Feature-extended image ready"
    );

    Ok(Some(output))
}

/// Resolve a path expressed in a compose file relative to the compose file's
/// directory (or its `context` for Dockerfile resolution). Absolute inputs are
/// returned unchanged. Centralized here so the `build:` arm and any unit tests
/// share the same compose-semantic resolution.
fn resolve_compose_path(base: &Path, candidate: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(candidate);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
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
            compose_manager.stop_project(project).await?;
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

    /// Bead 14b: confirms compose `build.context` and `build.dockerfile` paths
    /// resolve relative to the compose file's directory, not the workspace.
    /// This is the subtle compose semantic the issue called out for a focused
    /// test: a workspace-relative resolution would have produced the wrong
    /// path on stacked compose files in a subdirectory.
    #[test]
    fn resolve_compose_path_relative_joins_base() {
        let base = std::path::Path::new("/repo/compose-dir");
        let p = resolve_compose_path(base, "Dockerfile.dev");
        assert_eq!(
            p,
            std::path::PathBuf::from("/repo/compose-dir/Dockerfile.dev")
        );
    }

    #[test]
    fn resolve_compose_path_dot_returns_base_itself() {
        let base = std::path::Path::new("/repo/compose-dir");
        let p = resolve_compose_path(base, ".");
        // PathBuf::join with "." appends a dot component but compose treats it
        // semantically as the base; this is fine for downstream `-f` and
        // context arguments which accept either form. We assert the literal
        // join result so any change to that contract is observable.
        assert_eq!(p, std::path::PathBuf::from("/repo/compose-dir/."));
    }

    #[test]
    fn resolve_compose_path_subdir_is_joined() {
        let base = std::path::Path::new("/repo/compose-dir");
        let p = resolve_compose_path(base, "build/Dockerfile");
        assert_eq!(
            p,
            std::path::PathBuf::from("/repo/compose-dir/build/Dockerfile")
        );
    }

    #[test]
    fn resolve_compose_path_absolute_unchanged() {
        let base = std::path::Path::new("/repo/compose-dir");
        let p = resolve_compose_path(base, "/absolute/Dockerfile");
        assert_eq!(p, std::path::PathBuf::from("/absolute/Dockerfile"));
    }

    #[test]
    fn resolve_compose_path_parent_traversal_kept_relative() {
        // Compose allows `../sibling` as a context; we preserve it verbatim and
        // let the OS resolve it during `docker buildx build`.
        let base = std::path::Path::new("/repo/compose-dir");
        let p = resolve_compose_path(base, "../sibling");
        assert_eq!(p, std::path::PathBuf::from("/repo/compose-dir/../sibling"));
    }
}
