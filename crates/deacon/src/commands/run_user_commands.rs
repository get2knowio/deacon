//! Run user-defined lifecycle commands implementation
//!
//! This module provides execution of lifecycle commands in an existing container
//! without going through the full `up` workflow.

use crate::commands::shared::{load_config, ConfigLoadArgs, ConfigLoadResult};
use anyhow::{Context, Result};
use deacon_core::config::DevContainerConfig;
use deacon_core::container_lifecycle::{
    execute_container_lifecycle_with_progress_callback, AggregatedLifecycleCommand,
    ContainerLifecycleCommands, ContainerLifecycleConfig, LifecycleCommandList,
    LifecycleCommandSource, LifecycleCommandValue,
};
use deacon_core::variable::SubstitutionContext;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, info, instrument};

use crate::commands::exec::resolve_target_container;

/// Arguments for the run-user-commands command
#[derive(Debug, Clone)]
pub struct RunUserCommandsArgs {
    pub skip_post_create: bool,
    pub skip_post_attach: bool,
    pub skip_non_blocking_commands: bool,
    #[allow(dead_code)] // Future feature: prebuild mode
    pub prebuild: bool,
    #[allow(dead_code)] // Future feature: stop for personalization
    pub stop_for_personalization: bool,
    /// TODO(#269): Implement container selection for run-user-commands
    /// When container_id is provided, run lifecycle commands in specific container
    #[allow(dead_code)]
    pub container_id: Option<String>,
    /// TODO(#269): Implement container selection for run-user-commands
    /// When id_label is provided, resolve container and run lifecycle commands in it
    #[allow(dead_code)]
    pub id_label: Vec<String>,
    pub workspace_folder: Option<std::path::PathBuf>,
    pub config_path: Option<std::path::PathBuf>,
    pub override_config_path: Option<std::path::PathBuf>,
    pub secrets_files: Vec<std::path::PathBuf>,
    pub progress_tracker: Arc<Mutex<Option<deacon_core::progress::ProgressTracker>>>,
    pub docker_path: String,
    pub container_data_folder: Option<std::path::PathBuf>,
}

/// Execute the run-user-commands command
#[instrument(skip(args))]
pub async fn execute_run_user_commands(args: RunUserCommandsArgs) -> Result<()> {
    info!("Starting run-user-commands execution");

    // Load configuration with override and secrets support via shared helper
    let ConfigLoadResult {
        config,
        workspace_folder,
        ..
    } = load_config(ConfigLoadArgs {
        workspace_folder: args.workspace_folder.as_deref(),
        config_path: args.config_path.as_deref(),
        override_config_path: args.override_config_path.as_deref(),
        secrets_files: &args.secrets_files,
    })?;

    debug!("Loaded configuration with overrides and secrets support");

    let container_id = {
        let docker_client = deacon_core::docker::CliDocker::new();
        match resolve_target_container(
            &docker_client,
            workspace_folder.as_path(),
            &config,
            None,
            &args.docker_path,
            &[],
        )
        .await
        {
            Ok(id) => id,
            Err(e) => {
                debug!(error = ?e, "Failed to resolve target container for workspace");
                return Err(anyhow::anyhow!(
                    "No running container found. Run 'deacon up' first"
                ));
            }
        }
    };

    info!("Found target container: {}", container_id);

    // Execute lifecycle commands
    execute_lifecycle_commands(&container_id, &config, workspace_folder.as_path(), &args).await?;

    info!("Run-user-commands execution completed successfully");
    Ok(())
}

/// Execute lifecycle commands in the container
#[instrument(skip(config, workspace_folder, args))]
async fn execute_lifecycle_commands(
    container_id: &str,
    config: &DevContainerConfig,
    workspace_folder: &Path,
    args: &RunUserCommandsArgs,
) -> Result<()> {
    info!("Executing lifecycle commands in container");

    // Create substitution context
    let substitution_context = SubstitutionContext::new(workspace_folder)?;

    // Determine container workspace folder
    let container_workspace_folder =
        crate::commands::shared::derive_container_workspace_folder(config, workspace_folder);

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
        non_blocking_timeout: Duration::from_secs(300), // 5 minutes default timeout
        use_login_shell: true, // Default: use login shell for lifecycle commands
        user_env_probe: deacon_core::container_env_probe::ContainerProbeMode::LoginShell,
        cache_folder: args.container_data_folder.clone(),
        // Per FR-006: force_pty toggle only applies to 'up' workflow lifecycle exec,
        // not to run-user-commands which is a separate entry point
        force_pty: false,
        // run-user-commands does not install dotfiles - that is handled by `up` command
        dotfiles: None,
        is_prebuild: false,
    };

    // Build lifecycle commands from configuration using typed parser
    let mut commands = ContainerLifecycleCommands::new();

    // Helper to parse a JSON value into a LifecycleCommandList
    let parse_phase_command =
        |json_val: &serde_json::Value, phase_name: &str| -> Result<Option<LifecycleCommandList>> {
            let parsed = LifecycleCommandValue::from_json_value(json_val)
                .with_context(|| format!("Failed to parse {} command", phase_name))?;
            match parsed {
                Some(cmd) if !cmd.is_empty() => Ok(Some(LifecycleCommandList {
                    commands: vec![AggregatedLifecycleCommand {
                        command: cmd,
                        source: LifecycleCommandSource::Config,
                    }],
                })),
                _ => Ok(None),
            }
        };

    // TODO(012): This only collects lifecycle commands from the user's devcontainer.json config.
    // Feature-defined lifecycle commands (onCreateCommand, etc.) are not aggregated here.
    // The `up` command uses `aggregate_lifecycle_commands()` which includes features.
    // To reach parity, we need resolved features from image metadata (container labels).

    // Handle different lifecycle phases based on configuration
    // Note: initializeCommand is intentionally omitted here. It is a host-side command
    // that runs before container creation and belongs only in the `up` workflow.

    // Phase 1: onCreate (container)
    if let Some(ref on_create) = config.on_create_command {
        if let Some(cmd_list) = parse_phase_command(on_create, "onCreateCommand")? {
            commands = commands.with_on_create(cmd_list);
        }
    }

    // Phase 2: updateContent (container)
    if let Some(ref update_content) = config.update_content_command {
        if let Some(cmd_list) = parse_phase_command(update_content, "updateContentCommand")? {
            commands = commands.with_update_content(cmd_list);
        }
    }

    // Phase 3: postCreate (container, can be skipped)
    if !args.skip_post_create {
        if let Some(ref post_create) = config.post_create_command {
            if let Some(cmd_list) = parse_phase_command(post_create, "postCreateCommand")? {
                commands = commands.with_post_create(cmd_list);
            }
        }
    }

    // Phase 4: postStart (container, non-blocking, can be skipped)
    if !args.skip_non_blocking_commands {
        if let Some(ref post_start) = config.post_start_command {
            if let Some(cmd_list) = parse_phase_command(post_start, "postStartCommand")? {
                commands = commands.with_post_start(cmd_list);
            }
        }

        // Phase 5: postAttach (container, non-blocking, can be skipped)
        if !args.skip_post_attach {
            if let Some(ref post_attach) = config.post_attach_command {
                if let Some(cmd_list) = parse_phase_command(post_attach, "postAttachCommand")? {
                    commands = commands.with_post_attach(cmd_list);
                }
            }
        }
    }

    // Execute lifecycle commands with progress callback
    let result = execute_container_lifecycle_with_progress_callback(
        &lifecycle_config,
        &commands,
        &substitution_context,
        Some(crate::commands::shared::progress::make_progress_callback(
            &args.progress_tracker,
        )),
    )
    .await;

    // Return result
    let result = result?;

    debug!(
        "User commands execution completed: {} blocking phases executed, {} non-blocking phases to execute",
        result.phases.len(),
        result.non_blocking_phases.len()
    );

    // Log what non-blocking phases would be executed; do not block CLI
    result.log_non_blocking_phases();

    info!("Lifecycle commands execution completed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_user_commands_args_defaults() {
        // For this simple args test, we don't need a real tracker.
        // Use None to avoid filesystem side effects from progress cache initialization.
        let progress_tracker: Option<deacon_core::progress::ProgressTracker> = None;
        let progress_tracker = std::sync::Arc::new(std::sync::Mutex::new(progress_tracker));

        let args = RunUserCommandsArgs {
            skip_post_create: false,
            skip_post_attach: false,
            skip_non_blocking_commands: false,
            prebuild: false,
            stop_for_personalization: false,
            container_id: None,
            id_label: vec![],
            workspace_folder: None,
            config_path: None,
            override_config_path: None,
            secrets_files: vec![],
            progress_tracker,
            docker_path: "docker".to_string(),
            container_data_folder: None,
        };

        assert!(!args.skip_post_create);
        assert!(!args.skip_non_blocking_commands);
        assert!(!args.prebuild);
    }
}
