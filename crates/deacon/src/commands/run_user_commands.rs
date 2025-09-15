//! Run user-defined lifecycle commands implementation
//!
//! This module provides execution of lifecycle commands in an existing container
//! without going through the full `up` workflow.

use anyhow::Result;
use deacon_core::config::{ConfigLoader, DevContainerConfig};
use deacon_core::container_lifecycle::{
    execute_container_lifecycle_with_progress_callback, ContainerLifecycleCommands,
    ContainerLifecycleConfig,
};
use deacon_core::secrets::SecretsCollection;
use deacon_core::variable::SubstitutionContext;
use std::path::Path;
use std::sync::{Arc, Mutex};
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
    pub workspace_folder: Option<std::path::PathBuf>,
    pub config_path: Option<std::path::PathBuf>,
    pub override_config_path: Option<std::path::PathBuf>,
    pub secrets_files: Vec<std::path::PathBuf>,
    pub progress_tracker: Arc<Mutex<Option<deacon_core::progress::ProgressTracker>>>,
}

/// Execute the run-user-commands command
#[instrument(skip(args))]
pub async fn execute_run_user_commands(args: RunUserCommandsArgs) -> Result<()> {
    info!("Starting run-user-commands execution");

    // Resolve workspace folder
    let workspace_folder = if let Some(ref folder) = args.workspace_folder {
        folder.clone()
    } else {
        std::env::current_dir()?
    };

    // Load configuration with override and secrets support
    let (config, _substitution_report) = if let Some(ref config_path) = args.config_path {
        // Load secrets if provided
        let secrets = if !args.secrets_files.is_empty() {
            Some(SecretsCollection::load_from_files(&args.secrets_files)?)
        } else {
            None
        };

        ConfigLoader::load_with_overrides_and_substitution(
            config_path,
            args.override_config_path.as_deref(),
            secrets.as_ref(),
            &workspace_folder,
        )?
    } else {
        // Discover configuration
        let config_location = ConfigLoader::discover_config(&workspace_folder)?;

        // Load secrets if provided
        let secrets = if !args.secrets_files.is_empty() {
            Some(SecretsCollection::load_from_files(&args.secrets_files)?)
        } else {
            None
        };

        ConfigLoader::load_with_overrides_and_substitution(
            config_location.path(),
            args.override_config_path.as_deref(),
            secrets.as_ref(),
            &workspace_folder,
        )?
    };

    debug!("Loaded configuration with overrides and secrets support");

    // Resolve target container
    let docker_client = deacon_core::docker::CliDocker::new();
    let container_id =
        match resolve_target_container(&docker_client, &workspace_folder, &config).await {
            Ok(id) => id,
            Err(e) => {
                // Keep diagnostics for developers while returning a friendly, test‑expected message.
                debug!(error = ?e, "Failed to resolve target container for workspace");
                return Err(anyhow::anyhow!(
                    "No running container found. Run 'deacon up' first"
                ));
            }
        };

    info!("Found target container: {}", container_id);

    // Execute lifecycle commands
    execute_lifecycle_commands(&container_id, &config, &workspace_folder, &args).await?;

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

    // Handle different lifecycle phases based on configuration
    if let Some(ref on_create) = config.on_create_command {
        let phase_commands = commands_from_json_value(on_create)?;
        commands = commands.with_on_create(phase_commands.clone());
        phases_to_execute.push(("onCreate".to_string(), phase_commands));
    }

    if !args.skip_post_create {
        if let Some(ref post_create) = config.post_create_command {
            let phase_commands = commands_from_json_value(post_create)?;
            commands = commands.with_post_create(phase_commands.clone());
            phases_to_execute.push(("postCreate".to_string(), phase_commands));
        }
    }

    if !args.skip_non_blocking_commands {
        if let Some(ref post_start) = config.post_start_command {
            let phase_commands = commands_from_json_value(post_start)?;
            commands = commands.with_post_start(phase_commands.clone());
            phases_to_execute.push(("postStart".to_string(), phase_commands));
        }

        if !args.skip_post_attach {
            if let Some(ref post_attach) = config.post_attach_command {
                let phase_commands = commands_from_json_value(post_attach)?;
                commands = commands.with_post_attach(phase_commands.clone());
                phases_to_execute.push(("postAttach".to_string(), phase_commands));
            }
        }
    }

    // Create a progress event callback
    let emit_progress_event = |event: deacon_core::progress::ProgressEvent| -> Result<()> {
        if let Ok(mut tracker_guard) = args.progress_tracker.lock() {
            if let Some(ref mut tracker) = tracker_guard.as_mut() {
                tracker.emit_event(event)?;
            }
        }
        Ok(())
    };

    // Execute lifecycle commands with progress callback
    let result = execute_container_lifecycle_with_progress_callback(
        &lifecycle_config,
        &commands,
        &substitution_context,
        Some(emit_progress_event),
    )
    .await;

    // Return result
    result?;

    info!("Lifecycle commands execution completed");
    Ok(())
}

/// Convert JSON value to vector of command strings
fn commands_from_json_value(value: &serde_json::Value) -> Result<Vec<String>> {
    use deacon_core::errors::{ConfigError, DeaconError};

    match value {
        serde_json::Value::String(cmd) => Ok(vec![cmd.clone()]),
        serde_json::Value::Array(cmds) => {
            let mut commands = Vec::new();
            for cmd_value in cmds {
                if let serde_json::Value::String(cmd) = cmd_value {
                    commands.push(cmd.clone());
                } else {
                    return Err(DeaconError::Config(ConfigError::Validation {
                        message: format!(
                            "Invalid command in array: expected string, got {:?}",
                            cmd_value
                        ),
                    })
                    .into());
                }
            }
            Ok(commands)
        }
        _ => Err(DeaconError::Config(ConfigError::Validation {
            message: format!(
                "Invalid command format: expected string or array of strings, got {:?}",
                value
            ),
        })
        .into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::progress::{create_progress_tracker, ProgressFormat};

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
    fn test_run_user_commands_args_defaults() {
        let progress_tracker = create_progress_tracker(&ProgressFormat::None, None, None).unwrap();
        let progress_tracker = std::sync::Arc::new(std::sync::Mutex::new(progress_tracker));

        let args = RunUserCommandsArgs {
            skip_post_create: false,
            skip_post_attach: false,
            skip_non_blocking_commands: false,
            prebuild: false,
            stop_for_personalization: false,
            workspace_folder: None,
            config_path: None,
            override_config_path: None,
            secrets_files: vec![],
            progress_tracker,
        };

        assert!(!args.skip_post_create);
        assert!(!args.skip_non_blocking_commands);
        assert!(!args.prebuild);
    }
}
