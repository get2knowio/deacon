//! Lifecycle command execution for the up command.
//!
//! This module contains:
//! - `resolve_force_pty` - Resolve PTY preference based on flags and environment
//! - `execute_lifecycle_commands` - Execute lifecycle phases in container
//! - `execute_initialize_command` - Execute initializeCommand on host
//! - `commands_from_json_value` - Parse command JSON to string vector

use super::args::UpArgs;
use super::dotfiles::execute_dotfiles_installation;
use super::{ENV_FORCE_TTY_IF_JSON, ENV_LOG_FORMAT};
use anyhow::Result;
use deacon_core::config::DevContainerConfig;
use deacon_core::container_env_probe::ContainerProbeMode;
use deacon_core::errors::DeaconError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, instrument};

/// Resolve PTY preference for lifecycle commands based on flag, environment, and JSON mode
///
/// Per FR-002, FR-005: PTY toggle only applies in JSON log mode.
/// Precedence: CLI flag > env var `DEACON_FORCE_TTY_IF_JSON` > default (false)
pub(crate) fn resolve_force_pty(flag: bool, json_mode: bool) -> bool {
    // PTY toggle only applies when in JSON log mode
    if !json_mode {
        return false;
    }

    // CLI flag takes precedence
    if flag {
        return true;
    }

    // Check environment variable (truthy: true/1/yes; falsey: false/0/no or unset)
    if let Ok(val) = std::env::var(ENV_FORCE_TTY_IF_JSON) {
        matches!(val.to_lowercase().as_str(), "true" | "1" | "yes")
    } else {
        false // default: no PTY
    }
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
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_lifecycle_commands(
    container_id: &str,
    config: &DevContainerConfig,
    workspace_folder: &Path,
    args: &UpArgs,
    effective_env: HashMap<String, String>,
    effective_user: Option<String>,
    cache_folder: &Option<PathBuf>,
) -> Result<()> {
    use deacon_core::container_lifecycle::{
        execute_container_lifecycle_with_progress_callback, ContainerLifecycleCommands,
        ContainerLifecycleConfig,
    };
    use deacon_core::variable::SubstitutionContext;

    debug!("Executing lifecycle commands in container");

    // Skip all lifecycle work when --skip-post-create is set (per reference behavior: skip onCreate/updateContent/post* and dotfiles).
    if args.skip_post_create {
        debug!("Skipping lifecycle execution due to --skip-post-create");
        return Ok(());
    }

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

    // Determine if JSON log mode is active by checking DEACON_LOG_FORMAT env var
    // Per FR-001, FR-002: PTY toggle only applies in JSON log mode
    let json_mode = std::env::var(ENV_LOG_FORMAT)
        .map(|v| v == "json")
        .unwrap_or(false);

    // Resolve PTY preference based on flag, env, and JSON mode
    let force_pty = resolve_force_pty(args.force_tty_if_json, json_mode);

    debug!(
        "PTY preference resolved: force_pty={}, json_mode={}, flag={}, env={}",
        force_pty,
        json_mode,
        args.force_tty_if_json,
        std::env::var(ENV_FORCE_TTY_IF_JSON).unwrap_or_else(|_| "unset".to_string())
    );

    // Create container lifecycle configuration
    let lifecycle_config = ContainerLifecycleConfig {
        container_id: container_id.to_string(),
        user: effective_user,
        container_workspace_folder,
        container_env: effective_env,
        skip_post_create: args.skip_post_create,
        skip_non_blocking_commands: args.skip_non_blocking_commands,
        non_blocking_timeout: Duration::from_secs(300), // 5 minutes default timeout
        use_login_shell: true, // Default: use login shell for lifecycle commands
        user_env_probe: ContainerProbeMode::None,
        cache_folder: cache_folder.clone(),
        force_pty,
    };

    // Build lifecycle commands from configuration
    let mut commands = ContainerLifecycleCommands::new();
    let mut phases_to_execute = Vec::new();

    if let Some(ref on_create) = config.on_create_command {
        let phase_commands = commands_from_json_value(on_create)?;
        commands = commands.with_on_create(phase_commands.clone());
        phases_to_execute.push(("onCreate".to_string(), phase_commands));
    }

    // T014: Add updateContentCommand support
    // updateContent runs after onCreate and before postCreate
    if let Some(ref update_content) = config.update_content_command {
        let phase_commands = commands_from_json_value(update_content)?;
        commands = commands.with_update_content(phase_commands.clone());
        phases_to_execute.push(("updateContent".to_string(), phase_commands));
    }

    // T014: Prebuild mode stops after updateContent; skip postCreate/postStart/postAttach
    // Per specs/001-up-gap-spec/ User Story 2: prebuild is for CI image creation
    let is_prebuild_mode = args.prebuild;

    if !is_prebuild_mode {
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

        // T014: Prebuild implies skip-post-attach behavior
        // Also respect explicit --skip-post-attach flag
        if !args.skip_post_attach {
            if let Some(ref post_attach) = config.post_attach_command {
                let phase_commands = commands_from_json_value(post_attach)?;
                commands = commands.with_post_attach(phase_commands.clone());
                phases_to_execute.push(("postAttach".to_string(), phase_commands));
            }
        }
    } else {
        debug!("Prebuild mode: skipping postCreate, postStart, and postAttach phases");
    }

    let lifecycle_start_time = std::time::Instant::now();

    // Create a progress event callback
    let emit_progress_event_fn = |event: deacon_core::progress::ProgressEvent| -> Result<()> {
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
        Some(emit_progress_event_fn),
    )
    .await;

    let lifecycle_duration = lifecycle_start_time.elapsed();

    // Record metrics
    if let Ok(tracker_guard) = args.progress_tracker.lock() {
        if let Some(tracker) = tracker_guard.as_ref() {
            tracker.record_duration("lifecycle", lifecycle_duration);
        }
    }

    let result = result?;

    debug!(
        "Lifecycle execution completed: {} blocking phases executed, {} non-blocking phases to execute",
        result.phases.len(),
        result.non_blocking_phases.len()
    );

    // Log what non-blocking phases would be executed; do not block CLI
    result.log_non_blocking_phases();

    // T015: Execute dotfiles installation after updateContent and before postCreate
    // Per specs/001-up-gap-spec/ User Story 2: dotfiles are user-specific customizations
    // Dotfiles should NOT run in prebuild mode (CI images should not include user-specific dotfiles)
    if !is_prebuild_mode {
        execute_dotfiles_installation(container_id, config, args, force_pty).await?;
    } else {
        debug!("Prebuild mode: skipping dotfiles installation");
    }

    Ok(())
}

/// Execute initializeCommand on the host before container creation
#[instrument(skip(initialize_command, progress_tracker))]
pub(crate) async fn execute_initialize_command(
    initialize_command: &serde_json::Value,
    workspace_folder: &Path,
    progress_tracker: &std::sync::Arc<
        std::sync::Mutex<Option<deacon_core::progress::ProgressTracker>>,
    >,
) -> Result<()> {
    use deacon_core::container_lifecycle::ContainerLifecycleCommands;
    use deacon_core::variable::SubstitutionContext;

    debug!("Executing initializeCommand on host");

    // Parse the initialize command
    let phase_commands = commands_from_json_value(initialize_command)?;

    // Create substitution context for host-side execution
    let substitution_context = SubstitutionContext::new(workspace_folder)?;

    // Build lifecycle commands with just initialize phase
    let commands = ContainerLifecycleCommands::new().with_initialize(phase_commands.clone());

    // Create a dummy lifecycle config (only needed for container phases, not host phases)
    let lifecycle_config = deacon_core::container_lifecycle::ContainerLifecycleConfig {
        container_id: String::new(),
        user: None,
        container_workspace_folder: String::new(),
        container_env: std::collections::HashMap::new(),
        skip_post_create: false,
        skip_non_blocking_commands: false,
        non_blocking_timeout: Duration::from_secs(300),
        use_login_shell: true,
        user_env_probe: deacon_core::container_env_probe::ContainerProbeMode::None,
        cache_folder: None,
        force_pty: false,
    };

    // Create a progress event callback
    let emit_progress_event = |event: deacon_core::progress::ProgressEvent| -> Result<()> {
        if let Ok(mut tracker_guard) = progress_tracker.lock() {
            if let Some(ref mut tracker) = tracker_guard.as_mut() {
                tracker.emit_event(event)?;
            }
        }
        Ok(())
    };

    // Execute only the initialize phase (host-side)
    use deacon_core::container_lifecycle::execute_container_lifecycle_with_progress_callback;
    let result = execute_container_lifecycle_with_progress_callback(
        &lifecycle_config,
        &commands,
        &substitution_context,
        Some(emit_progress_event),
    )
    .await?;

    debug!(
        "initializeCommand execution completed: {} phases executed",
        result.phases.len()
    );

    Ok(())
}

/// Convert JSON value to vector of command strings
pub(crate) fn commands_from_json_value(value: &serde_json::Value) -> Result<Vec<String>> {
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
