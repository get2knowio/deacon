//! Container lifecycle command execution
//!
//! This module provides container-specific lifecycle command execution with full
//! variable substitution including containerEnv & containerWorkspaceFolder.

use crate::docker::{CliDocker, Docker, ExecConfig};
use crate::errors::{DeaconError, Result};
use crate::lifecycle::{ExecutionContext, ExecutionMode, LifecycleCommands, LifecyclePhase};
use crate::progress::{ProgressEvent, ProgressTracker};
use crate::redaction::{redact_if_enabled, RedactionConfig};
use crate::state::record_phase_executed;
use crate::variable::{SubstitutionContext, SubstitutionReport, VariableSubstitution};
use serde_json;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tracing::{debug, error, info, instrument, warn};

/// Source attribution for a lifecycle command.
///
/// Tracks whether a lifecycle command originated from a feature or from the
/// devcontainer.json configuration. This enables proper error attribution and
/// ordering when aggregating lifecycle commands during the up command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleCommandSource {
    /// Command from a feature (includes feature ID for attribution)
    Feature { id: String },
    /// Command from devcontainer.json config
    Config,
}

impl std::fmt::Display for LifecycleCommandSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Feature { id } => write!(f, "feature:{}", id),
            Self::Config => write!(f, "config"),
        }
    }
}

/// A lifecycle command ready for execution with source tracking.
///
/// Combines a lifecycle command (which can be a string, array, or object per the
/// devcontainer spec) with its source attribution for error reporting and debugging.
#[derive(Debug, Clone)]
pub struct AggregatedLifecycleCommand {
    /// The command to execute (can be string, array, or object)
    pub command: serde_json::Value,
    /// Where this command came from
    pub source: LifecycleCommandSource,
}

/// Configuration for dotfiles installation during lifecycle execution.
///
/// Dotfiles are installed at the `postCreate -> dotfiles -> postStart` boundary
/// per spec FR-001 (SC-001 in specs/008-up-lifecycle-hooks/).
#[derive(Debug, Clone, Default)]
pub struct DotfilesConfig {
    /// Git repository URL for dotfiles (None means no dotfiles configured)
    pub repository: Option<String>,
    /// Target path where dotfiles should be cloned (defaults based on user)
    pub target_path: Option<String>,
    /// Custom install command (overrides auto-detection of install.sh/setup.sh)
    pub install_command: Option<String>,
}

impl DotfilesConfig {
    /// Check if dotfiles are configured
    pub fn is_configured(&self) -> bool {
        self.repository.is_some()
    }
}

/// Configuration for container lifecycle execution
#[derive(Debug, Clone)]
pub struct ContainerLifecycleConfig {
    /// Container ID to execute commands in
    pub container_id: String,
    /// User to run commands as (defaults to root)
    pub user: Option<String>,
    /// Container workspace folder path
    pub container_workspace_folder: String,
    /// Container environment variables
    pub container_env: HashMap<String, String>,
    /// Skip post-create lifecycle phase
    pub skip_post_create: bool,
    /// Skip non-blocking commands (postStart & postAttach phases)
    pub skip_non_blocking_commands: bool,
    /// Timeout for non-blocking commands (default: 5 minutes)
    pub non_blocking_timeout: Duration,
    /// Whether to use login shell for lifecycle commands (default: true)
    pub use_login_shell: bool,
    /// User environment probe mode for lifecycle commands
    pub user_env_probe: crate::container_env_probe::ContainerProbeMode,
    /// Optional cache folder for env probe results
    pub cache_folder: Option<std::path::PathBuf>,
    /// Whether to force PTY allocation for lifecycle exec commands.
    /// When true, allocates a PTY even in non-interactive environments.
    /// Primarily used with JSON log mode to support interactive lifecycle commands.
    /// Controlled by CLI flag `--force-tty-if-json` or env var `DEACON_FORCE_TTY_IF_JSON`.
    pub force_pty: bool,
    /// Dotfiles configuration for installation after postCreate phase.
    /// Per spec SC-001: dotfiles execute exactly once at `postCreate -> dotfiles -> postStart` boundary.
    pub dotfiles: Option<DotfilesConfig>,
    /// Whether running in prebuild mode (skips dotfiles)
    pub is_prebuild: bool,
}

/// Execute lifecycle commands in a container with full variable substitution
///
/// This function executes lifecycle commands in the specified order:
/// 1. onCreate
/// 2. postCreate (if not skipped)
/// 3. postStart (if not skipped by skip_non_blocking_commands)
/// 4. postAttach (if not skipped by skip_non_blocking_commands)
#[instrument(skip(config, commands, substitution_context))]
pub async fn execute_container_lifecycle(
    config: &ContainerLifecycleConfig,
    commands: &ContainerLifecycleCommands,
    substitution_context: &SubstitutionContext,
) -> Result<ContainerLifecycleResult> {
    execute_container_lifecycle_with_docker(
        config,
        commands,
        substitution_context,
        &CliDocker::new(),
    )
    .await
}

/// Execute lifecycle commands in a container with custom Docker implementation
///
/// This function executes lifecycle commands in the specified order:
/// 1. onCreate
/// 2. postCreate (if not skipped)
/// 3. postStart (if not skipped by skip_non_blocking_commands)
/// 4. postAttach (if not skipped by skip_non_blocking_commands)
#[instrument(skip(config, commands, substitution_context, docker))]
pub async fn execute_container_lifecycle_with_docker<D>(
    config: &ContainerLifecycleConfig,
    commands: &ContainerLifecycleCommands,
    substitution_context: &SubstitutionContext,
    docker: &D,
) -> Result<ContainerLifecycleResult>
where
    D: Docker,
{
    execute_container_lifecycle_with_progress_callback_and_docker(
        config,
        commands,
        substitution_context,
        docker,
        None::<fn(ProgressEvent) -> anyhow::Result<()>>,
    )
    .await
}

/// Type alias for progress event callback
pub type ProgressEventCallback = dyn Fn(ProgressEvent) -> anyhow::Result<()> + Send + Sync;

/// Execute lifecycle commands in a container with optional progress event callback
///
/// This function executes lifecycle commands in the specified order:
/// 1. onCreate
/// 2. postCreate (if not skipped)
/// 3. postStart (if not skipped by skip_non_blocking_commands)
/// 4. postAttach (if not skipped by skip_non_blocking_commands)
///
/// Emits per-command progress events via the callback if provided.
#[instrument(skip(config, commands, substitution_context, progress_callback))]
pub async fn execute_container_lifecycle_with_progress_callback<F>(
    config: &ContainerLifecycleConfig,
    commands: &ContainerLifecycleCommands,
    substitution_context: &SubstitutionContext,
    progress_callback: Option<F>,
) -> Result<ContainerLifecycleResult>
where
    F: Fn(ProgressEvent) -> anyhow::Result<()>,
{
    execute_container_lifecycle_with_progress_callback_and_docker(
        config,
        commands,
        substitution_context,
        &CliDocker::new(),
        progress_callback,
    )
    .await
}

/// Execute lifecycle commands in a container with optional progress event callback and custom Docker implementation
///
/// This function executes lifecycle commands in the specified order:
/// 1. initialize (host-side, if provided)
/// 2. onCreate
/// 3. updateContent (if provided)
/// 4. postCreate (if not skipped)
/// 5. postStart (if not skipped by skip_non_blocking_commands)
/// 6. postAttach (if not skipped by skip_non_blocking_commands)
///
/// Emits per-command progress events via the callback if provided.
#[instrument(skip(config, commands, substitution_context, docker, progress_callback))]
pub async fn execute_container_lifecycle_with_progress_callback_and_docker<D, F>(
    config: &ContainerLifecycleConfig,
    commands: &ContainerLifecycleCommands,
    substitution_context: &SubstitutionContext,
    docker: &D,
    progress_callback: Option<F>,
) -> Result<ContainerLifecycleResult>
where
    D: Docker,
    F: Fn(ProgressEvent) -> anyhow::Result<()>,
{
    debug!(
        "Starting container lifecycle execution in container: {}",
        config.container_id
    );

    let mut result = ContainerLifecycleResult::new();

    // Detect shell early if using login shell mode
    let detected_shell = if config.use_login_shell {
        let prober = crate::container_env_probe::ContainerEnvironmentProber::new();
        let shell = prober
            .detect_container_shell(docker, &config.container_id, config.user.as_deref())
            .await
            .unwrap_or_else(|e| {
                warn!(
                    "Failed to detect container shell, falling back to 'sh': {}",
                    e
                );
                "sh".to_string()
            });
        info!(
            "Detected shell in container for lifecycle execution: {}",
            shell
        );
        Some(shell)
    } else {
        None
    };

    // Probe container environment if enabled
    let probed_env =
        if config.user_env_probe != crate::container_env_probe::ContainerProbeMode::None {
            info!(
                "Probing container environment with mode: {:?}",
                config.user_env_probe
            );
            let prober = crate::container_env_probe::ContainerEnvironmentProber::new();
            match prober
                .probe_container_environment(
                    docker,
                    &config.container_id,
                    config.user_env_probe,
                    config.user.as_deref(),
                    config.cache_folder.as_deref(),
                )
                .await
            {
                Ok(probe_result) => {
                    info!(
                        "Container environment probe completed: {} variables captured using {}",
                        probe_result.var_count, probe_result.shell_used
                    );
                    Some(probe_result.env_vars)
                }
                Err(e) => {
                    warn!(
                        "Container environment probe failed, continuing without probed env: {}",
                        e
                    );
                    None
                }
            }
        } else {
            debug!("Container environment probing disabled");
            None
        };

    // Merge probed environment with container_env (probed env is lowest priority)
    let merged_env = if let Some(probed) = probed_env.as_ref() {
        let prober = crate::container_env_probe::ContainerEnvironmentProber::new();
        prober.merge_environments(probed, Some(&config.container_env), None)
    } else {
        config.container_env.clone()
    };

    // Create substitution context with container information and merged env
    let container_context = substitution_context
        .clone()
        .with_container_workspace_folder(config.container_workspace_folder.clone())
        .with_container_env(merged_env.clone());

    // Create an updated config with merged environment
    let updated_config = ContainerLifecycleConfig {
        container_env: merged_env,
        ..config.clone()
    };

    // Execute initialize phase (host-side) if provided
    if let Some(initialize_commands) = &commands.initialize {
        info!("Executing initialize phase on host");
        result.phases.push(
            execute_host_lifecycle_phase(
                LifecyclePhase::Initialize,
                initialize_commands,
                substitution_context,
                progress_callback.as_ref(),
            )
            .await?,
        );
    }

    // Derive workspace folder from substitution context for marker persistence
    let workspace_folder = std::path::PathBuf::from(&container_context.local_workspace_folder);

    // Execute onCreate phase
    if let Some(on_create_commands) = &commands.on_create {
        let phase_result = execute_lifecycle_phase(
            LifecyclePhase::OnCreate,
            on_create_commands,
            &updated_config,
            docker,
            &container_context,
            detected_shell.as_deref(),
            progress_callback.as_ref(),
        )
        .await?;

        // Per FR-002: Record marker for blocking phase on successful execution
        if phase_result.success {
            if let Err(e) = record_phase_executed(
                &workspace_folder,
                LifecyclePhase::OnCreate,
                updated_config.is_prebuild,
            ) {
                warn!("Failed to record marker for phase onCreate: {}", e);
            } else {
                debug!(
                    "Recorded marker for blocking phase onCreate at {} (prebuild={})",
                    workspace_folder.display(),
                    updated_config.is_prebuild
                );
            }
        }

        result.phases.push(phase_result);
    }

    // Execute updateContent phase if provided
    if let Some(update_content_commands) = &commands.update_content {
        let phase_result = execute_lifecycle_phase(
            LifecyclePhase::UpdateContent,
            update_content_commands,
            &updated_config,
            docker,
            &container_context,
            detected_shell.as_deref(),
            progress_callback.as_ref(),
        )
        .await?;

        // Per FR-002: Record marker for blocking phase on successful execution
        if phase_result.success {
            if let Err(e) = record_phase_executed(
                &workspace_folder,
                LifecyclePhase::UpdateContent,
                updated_config.is_prebuild,
            ) {
                warn!("Failed to record marker for phase updateContent: {}", e);
            } else {
                debug!(
                    "Recorded marker for blocking phase updateContent at {} (prebuild={})",
                    workspace_folder.display(),
                    updated_config.is_prebuild
                );
            }
        }

        result.phases.push(phase_result);
    }

    // Execute postCreate phase (if not skipped)
    if !updated_config.skip_post_create {
        if let Some(post_create_commands) = &commands.post_create {
            let phase_result = execute_lifecycle_phase(
                LifecyclePhase::PostCreate,
                post_create_commands,
                &updated_config,
                docker,
                &container_context,
                detected_shell.as_deref(),
                progress_callback.as_ref(),
            )
            .await?;

            // Per FR-002: Record marker for blocking phase on successful execution
            if phase_result.success {
                if let Err(e) = record_phase_executed(
                    &workspace_folder,
                    LifecyclePhase::PostCreate,
                    updated_config.is_prebuild,
                ) {
                    warn!("Failed to record marker for phase postCreate: {}", e);
                } else {
                    debug!(
                        "Recorded marker for blocking phase postCreate at {} (prebuild={})",
                        workspace_folder.display(),
                        updated_config.is_prebuild
                    );
                }
            }

            result.phases.push(phase_result);
        }
    } else {
        info!("Skipping postCreate phase");
    }

    // Execute dotfiles phase (postCreate -> dotfiles -> postStart boundary per SC-001)
    // Dotfiles are skipped in prebuild mode and when skip_post_create is set
    if !updated_config.is_prebuild && !updated_config.skip_post_create {
        if let Some(ref dotfiles_config) = config.dotfiles {
            if dotfiles_config.is_configured() {
                info!("Executing dotfiles phase (after postCreate, before postStart)");
                let phase_result = execute_dotfiles_in_container(
                    dotfiles_config,
                    &updated_config,
                    docker,
                    detected_shell.as_deref(),
                    progress_callback.as_ref(),
                )
                .await?;

                // Per FR-002: Record marker for blocking phase on successful execution
                if phase_result.success {
                    if let Err(e) = record_phase_executed(
                        &workspace_folder,
                        LifecyclePhase::Dotfiles,
                        updated_config.is_prebuild,
                    ) {
                        warn!("Failed to record marker for phase dotfiles: {}", e);
                    } else {
                        debug!(
                            "Recorded marker for blocking phase dotfiles at {} (prebuild={})",
                            workspace_folder.display(),
                            updated_config.is_prebuild
                        );
                    }
                }

                result.phases.push(phase_result);
            } else {
                debug!("Dotfiles not configured, skipping dotfiles phase");
            }
        } else {
            debug!("No dotfiles configuration provided, skipping dotfiles phase");
        }
    } else if updated_config.is_prebuild {
        info!("Skipping dotfiles phase (prebuild mode)");
    } else {
        info!("Skipping dotfiles phase (skip_post_create flag)");
    }

    // Execute postStart phase (if not skipped by non-blocking commands flag)
    if !updated_config.skip_non_blocking_commands {
        if let Some(post_start_commands) = &commands.post_start {
            // Add postStart phase to non-blocking specs for later execution
            result.non_blocking_phases.push(NonBlockingPhaseSpec {
                phase: LifecyclePhase::PostStart,
                commands: post_start_commands.clone(),
                config: updated_config.clone(),
                context: container_context.clone(),
                timeout: updated_config.non_blocking_timeout,
                detected_shell: detected_shell.clone(),
                workspace_folder: workspace_folder.clone(),
                prebuild: updated_config.is_prebuild,
            });
            info!("Added postStart phase for non-blocking execution");
        }
    } else {
        info!("Skipping postStart phase (non-blocking commands disabled)");
    }

    // Execute postAttach phase (if not skipped by non-blocking commands flag)
    if !updated_config.skip_non_blocking_commands {
        if let Some(post_attach_commands) = &commands.post_attach {
            // Add postAttach phase to non-blocking specs for later execution
            result.non_blocking_phases.push(NonBlockingPhaseSpec {
                phase: LifecyclePhase::PostAttach,
                commands: post_attach_commands.clone(),
                config: updated_config.clone(),
                context: container_context.clone(),
                timeout: updated_config.non_blocking_timeout,
                detected_shell: detected_shell.clone(),
                workspace_folder: workspace_folder.clone(),
                prebuild: updated_config.is_prebuild,
            });
            info!("Added postAttach phase for non-blocking execution");
        }
    } else {
        info!("Skipping postAttach phase (non-blocking commands disabled)");
    }

    debug!("Completed container lifecycle execution");
    Ok(result)
}

/// Execute a lifecycle phase on the host system (not in container)
#[instrument(skip(commands, context, progress_callback))]
async fn execute_host_lifecycle_phase<F>(
    phase: LifecyclePhase,
    commands: &[String],
    context: &SubstitutionContext,
    progress_callback: Option<&F>,
) -> Result<PhaseResult>
where
    F: Fn(ProgressEvent) -> anyhow::Result<()>,
{
    info!("Executing host lifecycle phase: {}", phase.as_str());
    let phase_start = Instant::now();

    // Emit phase begin event
    if let Some(callback) = progress_callback {
        let event = ProgressEvent::LifecyclePhaseBegin {
            id: ProgressTracker::next_event_id(),
            timestamp: ProgressTracker::current_timestamp(),
            phase: phase.as_str().to_string(),
            commands: commands.to_vec(),
        };
        if let Err(e) = callback(event) {
            debug!("Failed to emit phase begin event: {}", e);
        }
    }

    let mut phase_result = PhaseResult {
        phase,
        commands: Vec::new(),
        total_duration: Duration::default(),
        success: true,
    };

    // Convert commands to LifecycleCommands format with variable substitution
    let mut lifecycle_commands_vec = Vec::new();
    let mut substituted_commands = Vec::new();

    for command_template in commands {
        // Apply variable substitution to the command (same as container phases)
        let mut substitution_report = SubstitutionReport::new();
        let substituted_command = VariableSubstitution::substitute_string(
            command_template,
            context,
            &mut substitution_report,
        );

        debug!(
            "Host command after variable substitution: {} -> {}",
            command_template, substituted_command
        );

        if substitution_report.has_substitutions() {
            debug!(
                "Variable substitutions applied: {:?}",
                substitution_report.replacements
            );
            if !substitution_report.unknown_variables.is_empty() {
                debug!(
                    "Unknown variables left unchanged: {:?}",
                    substitution_report.unknown_variables
                );
            }
        }

        // Store the substituted command for later use in CommandResult
        substituted_commands.push(substituted_command.clone());

        lifecycle_commands_vec.push(crate::lifecycle::CommandTemplate {
            command: substituted_command,
            env_vars: context.local_env.clone(),
        });
    }
    let lifecycle_commands = LifecycleCommands {
        commands: lifecycle_commands_vec,
    };

    // Create execution context for host
    let exec_ctx = ExecutionContext {
        environment: context.local_env.clone(),
        working_directory: Some(std::path::PathBuf::from(&context.local_workspace_folder)),
        timeout: None,
        redaction_config: RedactionConfig::default(),
        execution_mode: ExecutionMode::Host,
    };

    // Execute the phase using the lifecycle module's host execution
    let result = tokio::task::spawn_blocking(move || {
        crate::lifecycle::run_phase(phase, &lifecycle_commands, &exec_ctx)
    })
    .await;

    match result {
        Ok(Ok(lifecycle_result)) => {
            // Convert lifecycle result to phase result
            for (i, exit_code) in lifecycle_result.exit_codes.iter().enumerate() {
                let command_result = CommandResult {
                    command: substituted_commands.get(i).cloned().unwrap_or_default(),
                    exit_code: *exit_code,
                    duration: lifecycle_result
                        .durations
                        .get(i)
                        .copied()
                        .unwrap_or_default(),
                    success: *exit_code == 0,
                    stdout: String::new(), // Not captured in detail per-command
                    stderr: String::new(),
                };
                if !command_result.success {
                    phase_result.success = false;
                }
                phase_result.commands.push(command_result);
            }

            if !lifecycle_result.success {
                error!(
                    "Host lifecycle phase {} failed with output:\nstdout: {}\nstderr: {}",
                    phase.as_str(),
                    lifecycle_result.stdout,
                    lifecycle_result.stderr
                );
            }
        }
        Ok(Err(e)) => {
            error!(
                "Failed to execute host lifecycle phase {}: {}",
                phase.as_str(),
                e
            );
            phase_result.success = false;
            return Err(e);
        }
        Err(e) => {
            error!("Failed to spawn host lifecycle phase execution: {}", e);
            phase_result.success = false;
            return Err(DeaconError::Lifecycle(format!(
                "Failed to spawn host lifecycle phase execution: {}",
                e
            )));
        }
    }

    phase_result.total_duration = phase_start.elapsed();

    // Emit phase end event
    if let Some(callback) = progress_callback {
        let event = ProgressEvent::LifecyclePhaseEnd {
            id: ProgressTracker::next_event_id(),
            timestamp: ProgressTracker::current_timestamp(),
            phase: phase.as_str().to_string(),
            duration_ms: phase_result.total_duration.as_millis() as u64,
            success: phase_result.success,
        };
        if let Err(e) = callback(event) {
            debug!("Failed to emit phase end event: {}", e);
        }
    }

    info!(
        "Completed host lifecycle phase: {} in {:?}",
        phase.as_str(),
        phase_result.total_duration
    );
    Ok(phase_result)
}

/// Execute a single lifecycle phase in the container
#[instrument(skip(commands, config, docker, context, progress_callback))]
async fn execute_lifecycle_phase<D, F>(
    phase: LifecyclePhase,
    commands: &[String],
    config: &ContainerLifecycleConfig,
    docker: &D,
    context: &SubstitutionContext,
    detected_shell: Option<&str>,
    progress_callback: Option<&F>,
) -> Result<PhaseResult>
where
    D: Docker,
    F: Fn(ProgressEvent) -> anyhow::Result<()>,
{
    execute_lifecycle_phase_impl(
        phase,
        commands,
        config,
        docker,
        context,
        detected_shell,
        progress_callback,
    )
    .await
}

/// Execute a single lifecycle phase in the container (implementation detail)
/// This is the actual implementation extracted to support both blocking and non-blocking execution
#[instrument(skip(commands, config, docker, context, progress_callback))]
async fn execute_lifecycle_phase_impl<D, F>(
    phase: LifecyclePhase,
    commands: &[String],
    config: &ContainerLifecycleConfig,
    docker: &D,
    context: &SubstitutionContext,
    detected_shell: Option<&str>,
    progress_callback: Option<&F>,
) -> Result<PhaseResult>
where
    D: Docker,
    F: Fn(ProgressEvent) -> anyhow::Result<()>,
{
    debug!("Executing lifecycle phase: {}", phase.as_str());
    let phase_start = Instant::now();

    // Emit phase begin event
    if let Some(callback) = progress_callback {
        let event = ProgressEvent::LifecyclePhaseBegin {
            id: ProgressTracker::next_event_id(),
            timestamp: ProgressTracker::current_timestamp(),
            phase: phase.as_str().to_string(),
            commands: commands.to_vec(),
        };
        if let Err(e) = callback(event) {
            debug!("Failed to emit phase begin event: {}", e);
        }
    }

    let mut phase_result = PhaseResult {
        phase,
        commands: Vec::new(),
        total_duration: std::time::Duration::default(),
        success: true,
    };

    for (i, command_template) in commands.iter().enumerate() {
        debug!(
            "Executing command {} of {} for phase {}: {}",
            i + 1,
            commands.len(),
            phase.as_str(),
            command_template
        );

        // Generate unique command ID
        let command_id = format!("{}-{}", phase.as_str(), i + 1);

        let start_time = Instant::now();

        // Apply variable substitution to the command
        let mut substitution_report = SubstitutionReport::new();
        let substituted_command = VariableSubstitution::substitute_string(
            command_template,
            context,
            &mut substitution_report,
        );

        debug!(
            "Command after variable substitution: {}",
            substituted_command
        );

        if substitution_report.has_substitutions() {
            debug!(
                "Variable substitutions applied: {:?}",
                substitution_report.replacements
            );
            if !substitution_report.unknown_variables.is_empty() {
                debug!(
                    "Unknown variables left unchanged: {:?}",
                    substitution_report.unknown_variables
                );
            }
        }

        // Apply redaction to command string for event emission
        let redaction_config = RedactionConfig::default(); // Use default for now
        let redacted_command = redact_if_enabled(&substituted_command, &redaction_config);

        // Emit command begin event
        if let Some(callback) = progress_callback {
            let event = ProgressEvent::LifecycleCommandBegin {
                id: ProgressTracker::next_event_id(),
                timestamp: ProgressTracker::current_timestamp(),
                phase: phase.as_str().to_string(),
                command_id: command_id.clone(),
                command: redacted_command.clone(),
            };
            if let Err(e) = callback(event) {
                debug!("Failed to emit command begin event: {}", e);
            }
        }

        // Create exec configuration
        let exec_config = ExecConfig {
            user: config.user.clone(),
            working_dir: Some(config.container_workspace_folder.clone()),
            env: config.container_env.clone(),
            tty: config.force_pty,
            interactive: false,
            detach: false,
            silent: false,
            terminal_size: None,
        };

        // Detect shell and create appropriate command args
        let command_args = if config.use_login_shell {
            // Use detected shell or fallback to sh
            let shell = detected_shell.unwrap_or("sh");
            debug!("Using login shell for lifecycle command: {}", shell);
            crate::container_env_probe::get_shell_command_for_lifecycle(
                shell,
                &substituted_command,
                true,
            )
        } else {
            // Legacy mode: plain sh -c
            debug!("Using plain sh -c for lifecycle command (legacy mode)");
            vec![
                "sh".to_string(),
                "-c".to_string(),
                substituted_command.clone(),
            ]
        };

        let exec_result = docker
            .exec(&config.container_id, &command_args, exec_config)
            .await;

        let duration = start_time.elapsed();

        match exec_result {
            Ok(exec_result) => {
                debug!(
                    "Container command completed with exit code: {} in {:?}",
                    exec_result.exit_code, duration
                );

                // Emit command end event (success)
                if let Some(callback) = progress_callback {
                    let event = ProgressEvent::LifecycleCommandEnd {
                        id: ProgressTracker::next_event_id(),
                        timestamp: ProgressTracker::current_timestamp(),
                        phase: phase.as_str().to_string(),
                        command_id: command_id.clone(),
                        duration_ms: duration.as_millis() as u64,
                        success: exec_result.success,
                        exit_code: Some(exec_result.exit_code),
                    };
                    if let Err(e) = callback(event) {
                        debug!("Failed to emit command end event: {}", e);
                    }
                }

                let command_result = CommandResult {
                    command: substituted_command.clone(),
                    exit_code: exec_result.exit_code,
                    duration,
                    success: exec_result.success,
                    stdout: String::new(), // TODO: Capture output when docker exec supports it
                    stderr: String::new(), // TODO: Capture output when docker exec supports it
                };

                phase_result.commands.push(command_result);

                // If command failed, mark phase as failed but continue with next command
                if exec_result.exit_code != 0 {
                    phase_result.success = false;
                    error!(
                        "Container command failed in phase {} with exit code {}: {}",
                        phase.as_str(),
                        exec_result.exit_code,
                        substituted_command
                    );
                }
            }
            Err(e) => {
                // Emit command end event (failure)
                if let Some(callback) = progress_callback {
                    let event = ProgressEvent::LifecycleCommandEnd {
                        id: ProgressTracker::next_event_id(),
                        timestamp: ProgressTracker::current_timestamp(),
                        phase: phase.as_str().to_string(),
                        command_id: command_id.clone(),
                        duration_ms: duration.as_millis() as u64,
                        success: false,
                        exit_code: None, // No exit code when exec itself fails
                    };
                    if let Err(emit_err) = callback(event) {
                        debug!("Failed to emit command end event: {}", emit_err);
                    }
                }

                phase_result.success = false;
                phase_result.total_duration = phase_start.elapsed();

                // Emit phase end event before returning error
                if let Some(callback) = progress_callback {
                    let event = ProgressEvent::LifecyclePhaseEnd {
                        id: ProgressTracker::next_event_id(),
                        timestamp: ProgressTracker::current_timestamp(),
                        phase: phase.as_str().to_string(),
                        duration_ms: phase_result.total_duration.as_millis() as u64,
                        success: false,
                    };
                    if let Err(emit_err) = callback(event) {
                        debug!("Failed to emit phase end event: {}", emit_err);
                    }
                }

                error!(
                    "Failed to execute container command in phase {}: {}",
                    phase.as_str(),
                    e
                );
                return Err(DeaconError::Lifecycle(format!(
                    "Failed to execute container command in phase {}: {}",
                    phase.as_str(),
                    e
                )));
            }
        }
    }

    phase_result.total_duration = phase_start.elapsed();

    // Emit phase end event
    if let Some(callback) = progress_callback {
        let event = ProgressEvent::LifecyclePhaseEnd {
            id: ProgressTracker::next_event_id(),
            timestamp: ProgressTracker::current_timestamp(),
            phase: phase.as_str().to_string(),
            duration_ms: phase_result.total_duration.as_millis() as u64,
            success: phase_result.success,
        };
        if let Err(e) = callback(event) {
            debug!("Failed to emit phase end event: {}", e);
        }
    }

    debug!(
        "Completed lifecycle phase: {} in {:?}",
        phase.as_str(),
        phase_result.total_duration
    );
    Ok(phase_result)
}

/// Execute dotfiles installation in the container
///
/// Per spec SC-001, dotfiles execute at the `postCreate -> dotfiles -> postStart` boundary.
/// This function:
/// 1. Clones the dotfiles repository into the container
/// 2. Executes the install script (custom or auto-detected install.sh/setup.sh)
///
/// # Arguments
///
/// * `dotfiles_config` - Configuration for dotfiles (repository, target path, install command)
/// * `lifecycle_config` - Container lifecycle configuration
/// * `docker` - Docker client for executing commands
/// * `detected_shell` - Shell detected for the container
/// * `progress_callback` - Optional callback for progress events
#[instrument(skip(dotfiles_config, lifecycle_config, docker, progress_callback))]
async fn execute_dotfiles_in_container<D, F>(
    dotfiles_config: &DotfilesConfig,
    lifecycle_config: &ContainerLifecycleConfig,
    docker: &D,
    detected_shell: Option<&str>,
    progress_callback: Option<&F>,
) -> Result<PhaseResult>
where
    D: Docker,
    F: Fn(ProgressEvent) -> anyhow::Result<()>,
{
    let phase = LifecyclePhase::Dotfiles;
    let phase_start = Instant::now();

    info!(
        "Executing dotfiles phase in container: {}",
        lifecycle_config.container_id
    );

    // Emit phase begin event
    if let Some(callback) = progress_callback {
        let event = ProgressEvent::LifecyclePhaseBegin {
            id: ProgressTracker::next_event_id(),
            timestamp: ProgressTracker::current_timestamp(),
            phase: phase.as_str().to_string(),
            commands: vec![format!(
                "dotfiles: {}",
                dotfiles_config.repository.as_deref().unwrap_or("none")
            )],
        };
        if let Err(e) = callback(event) {
            debug!("Failed to emit phase begin event: {}", e);
        }
    }

    let mut phase_result = PhaseResult {
        phase,
        commands: Vec::new(),
        total_duration: Duration::default(),
        success: true,
    };

    // Get repository URL
    let repository = match &dotfiles_config.repository {
        Some(repo) => repo.clone(),
        None => {
            debug!("No dotfiles repository configured");
            phase_result.total_duration = phase_start.elapsed();
            return Ok(phase_result);
        }
    };

    // Determine user and target path
    let user = lifecycle_config
        .user
        .clone()
        .unwrap_or_else(|| "root".to_string());
    let default_target_path = if user == "root" {
        "/root/.dotfiles".to_string()
    } else {
        format!("/home/{}/.dotfiles", user)
    };

    let target_path = dotfiles_config
        .target_path
        .clone()
        .unwrap_or(default_target_path);

    debug!(
        "Installing dotfiles to container path: {} as user: {}",
        target_path, user
    );

    let exec_config = ExecConfig {
        user: Some(user.clone()),
        working_dir: None,
        env: lifecycle_config.container_env.clone(),
        tty: lifecycle_config.force_pty,
        interactive: false,
        detach: false,
        silent: false,
        terminal_size: None,
    };

    // Step 1: Check if dotfiles directory already exists (idempotency)
    let check_exists_command = vec![
        "sh".to_string(),
        "-c".to_string(),
        format!("test -d {}", target_path),
    ];

    let exists_result = docker
        .exec(
            &lifecycle_config.container_id,
            &check_exists_command,
            exec_config.clone(),
        )
        .await
        .map_err(|e| {
            DeaconError::Lifecycle(format!("Failed to check dotfiles directory: {}", e))
        })?;

    if exists_result.success {
        info!(
            "Dotfiles directory already exists at {}, removing to clone fresh",
            target_path
        );
        let remove_command = vec!["rm".to_string(), "-rf".to_string(), target_path.clone()];
        let remove_result = docker
            .exec(
                &lifecycle_config.container_id,
                &remove_command,
                exec_config.clone(),
            )
            .await
            .map_err(|e| {
                DeaconError::Lifecycle(format!("Failed to remove existing dotfiles: {}", e))
            })?;

        if !remove_result.success {
            phase_result.success = false;
            phase_result.total_duration = phase_start.elapsed();
            emit_phase_end_event(progress_callback, &phase_result);
            return Err(DeaconError::Lifecycle(format!(
                "Failed to remove existing dotfiles directory (exit code {}): {}{}",
                remove_result.exit_code, remove_result.stdout, remove_result.stderr
            )));
        }
    }

    // Step 2: Clone dotfiles repository inside container
    info!("Cloning dotfiles repository: {}", repository);
    let clone_command = vec![
        "git".to_string(),
        "clone".to_string(),
        repository.clone(),
        target_path.clone(),
    ];

    let clone_start = Instant::now();
    let clone_result = docker
        .exec(
            &lifecycle_config.container_id,
            &clone_command,
            exec_config.clone(),
        )
        .await
        .map_err(|e| DeaconError::Lifecycle(format!("Failed to execute git clone: {}", e)))?;

    phase_result.commands.push(CommandResult {
        command: format!("git clone {} {}", repository, target_path),
        exit_code: clone_result.exit_code,
        duration: clone_start.elapsed(),
        success: clone_result.success,
        stdout: clone_result.stdout.clone(),
        stderr: clone_result.stderr.clone(),
    });

    if !clone_result.success {
        phase_result.success = false;
        phase_result.total_duration = phase_start.elapsed();
        emit_phase_end_event(progress_callback, &phase_result);
        return Err(DeaconError::Lifecycle(format!(
            "Failed to clone dotfiles repository (exit code {}): {}{}. Ensure git is installed and the repository URL is valid.",
            clone_result.exit_code, clone_result.stdout, clone_result.stderr
        )));
    }

    info!("Dotfiles repository cloned successfully");

    // Step 3: Determine and execute install command
    let install_command_str = if let Some(ref custom_command) = dotfiles_config.install_command {
        debug!("Using custom dotfiles install command: {}", custom_command);
        Some(custom_command.clone())
    } else {
        // Auto-detect install script
        debug!("Auto-detecting install script in dotfiles repository");

        let detect_script_command = vec![
            "sh".to_string(),
            "-c".to_string(),
            format!(
                "if [ -f {}/install.sh ]; then echo 'install.sh'; elif [ -f {}/setup.sh ]; then echo 'setup.sh'; fi",
                target_path, target_path
            ),
        ];

        let detect_result = docker
            .exec(
                &lifecycle_config.container_id,
                &detect_script_command,
                exec_config.clone(),
            )
            .await;

        match detect_result {
            Ok(result) if !result.stdout.trim().is_empty() => {
                let script_name = result.stdout.trim();
                debug!("Auto-detected install script: {}", script_name);

                // Use detected shell or fallback to bash
                let shell = detected_shell.unwrap_or("bash");
                Some(format!("{} {}/{}", shell, target_path, script_name))
            }
            _ => {
                debug!("No install script found in dotfiles repository");
                None
            }
        }
    };

    // Step 4: Execute install command if present
    if let Some(install_cmd) = install_command_str {
        info!("Executing dotfiles install command: {}", install_cmd);

        let install_command = vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("cd {} && {}", target_path, install_cmd),
        ];

        let install_start = Instant::now();
        let install_result = docker
            .exec(
                &lifecycle_config.container_id,
                &install_command,
                exec_config,
            )
            .await
            .map_err(|e| {
                DeaconError::Lifecycle(format!("Failed to execute install command: {}", e))
            })?;

        phase_result.commands.push(CommandResult {
            command: install_cmd.clone(),
            exit_code: install_result.exit_code,
            duration: install_start.elapsed(),
            success: install_result.success,
            stdout: install_result.stdout.clone(),
            stderr: install_result.stderr.clone(),
        });

        if !install_result.success {
            phase_result.success = false;
            phase_result.total_duration = phase_start.elapsed();
            emit_phase_end_event(progress_callback, &phase_result);
            return Err(DeaconError::Lifecycle(format!(
                "Dotfiles install script failed (exit code {}): {}{}",
                install_result.exit_code, install_result.stdout, install_result.stderr
            )));
        }

        info!("Dotfiles install command completed successfully");
    } else {
        info!("No install script to execute, dotfiles cloned only");
    }

    phase_result.total_duration = phase_start.elapsed();
    emit_phase_end_event(progress_callback, &phase_result);

    debug!(
        "Completed dotfiles phase in {:?}",
        phase_result.total_duration
    );
    Ok(phase_result)
}

/// Helper to emit phase end event
fn emit_phase_end_event<F>(progress_callback: Option<&F>, phase_result: &PhaseResult)
where
    F: Fn(ProgressEvent) -> anyhow::Result<()>,
{
    if let Some(callback) = progress_callback {
        let event = ProgressEvent::LifecyclePhaseEnd {
            id: ProgressTracker::next_event_id(),
            timestamp: ProgressTracker::current_timestamp(),
            phase: phase_result.phase.as_str().to_string(),
            duration_ms: phase_result.total_duration.as_millis() as u64,
            success: phase_result.success,
        };
        if let Err(e) = callback(event) {
            debug!("Failed to emit phase end event: {}", e);
        }
    }
}

/// Commands for each lifecycle phase
#[derive(Debug, Clone, Default)]
pub struct ContainerLifecycleCommands {
    /// Commands to run during initialize phase (host-side)
    pub initialize: Option<Vec<String>>,
    /// Commands to run during onCreate phase
    pub on_create: Option<Vec<String>>,
    /// Commands to run during updateContent phase
    pub update_content: Option<Vec<String>>,
    /// Commands to run during postCreate phase
    pub post_create: Option<Vec<String>>,
    /// Commands to run during postStart phase
    pub post_start: Option<Vec<String>>,
    /// Commands to run during postAttach phase
    pub post_attach: Option<Vec<String>>,
}

impl ContainerLifecycleCommands {
    /// Create new empty lifecycle commands
    pub fn new() -> Self {
        Self::default()
    }

    /// Set initialize commands (host-side)
    pub fn with_initialize(mut self, commands: Vec<String>) -> Self {
        self.initialize = Some(commands);
        self
    }

    /// Set onCreate commands
    pub fn with_on_create(mut self, commands: Vec<String>) -> Self {
        self.on_create = Some(commands);
        self
    }

    /// Set updateContent commands
    pub fn with_update_content(mut self, commands: Vec<String>) -> Self {
        self.update_content = Some(commands);
        self
    }

    /// Set postCreate commands
    pub fn with_post_create(mut self, commands: Vec<String>) -> Self {
        self.post_create = Some(commands);
        self
    }

    /// Set postStart commands
    pub fn with_post_start(mut self, commands: Vec<String>) -> Self {
        self.post_start = Some(commands);
        self
    }

    /// Set postAttach commands
    pub fn with_post_attach(mut self, commands: Vec<String>) -> Self {
        self.post_attach = Some(commands);
        self
    }
}

/// Result of executing a single command
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// The command that was executed (after variable substitution)
    pub command: String,
    /// Exit code of the command
    pub exit_code: i32,
    /// Duration of command execution
    pub duration: std::time::Duration,
    /// Whether the command succeeded
    pub success: bool,
    /// Captured stdout (empty for now)
    pub stdout: String,
    /// Captured stderr (empty for now)
    pub stderr: String,
}

/// Result of executing a lifecycle phase
#[derive(Debug, Clone)]
pub struct PhaseResult {
    /// The lifecycle phase that was executed
    pub phase: LifecyclePhase,
    /// Results of individual commands in this phase
    pub commands: Vec<CommandResult>,
    /// Total duration of the phase
    pub total_duration: std::time::Duration,
    /// Whether all commands in the phase succeeded
    pub success: bool,
}

/// Result of executing container lifecycle
#[derive(Debug)]
pub struct ContainerLifecycleResult {
    /// Results of individual phases
    pub phases: Vec<PhaseResult>,
    /// Non-blocking phases that should be executed in background
    pub non_blocking_phases: Vec<NonBlockingPhaseSpec>,
    /// Errors from background phase failures for structured aggregation
    pub background_errors: Vec<String>,
}

/// Specification for a non-blocking phase to be executed later
#[derive(Debug, Clone)]
pub struct NonBlockingPhaseSpec {
    /// Phase to execute
    pub phase: LifecyclePhase,
    /// Commands to execute
    pub commands: Vec<String>,
    /// Configuration for execution
    pub config: ContainerLifecycleConfig,
    /// Substitution context
    pub context: SubstitutionContext,
    /// Timeout for execution
    pub timeout: Duration,
    /// Detected shell for lifecycle execution
    pub detected_shell: Option<String>,
    /// Workspace folder path for marker persistence (derived from context)
    /// Used to write completion markers after runtime hook execution per FR-002.
    pub workspace_folder: std::path::PathBuf,
    /// Whether running in prebuild mode (affects marker directory per FR-008)
    pub prebuild: bool,
}

impl Default for ContainerLifecycleResult {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerLifecycleResult {
    /// Create new empty result
    pub fn new() -> Self {
        Self {
            phases: Vec::new(),
            non_blocking_phases: Vec::new(),
            background_errors: Vec::new(),
        }
    }

    /// Check if all phases succeeded
    pub fn success(&self) -> bool {
        self.phases.iter().all(|phase| phase.success)
    }

    /// Get total duration across all phases
    pub fn total_duration(&self) -> std::time::Duration {
        self.phases.iter().map(|phase| phase.total_duration).sum()
    }

    /// Execute non-blocking phases in the background if supported
    /// For now, just logs that they would be executed non-blockingly
    pub fn log_non_blocking_phases(&self) {
        for phase_spec in &self.non_blocking_phases {
            info!(
                "Non-blocking phase {} would execute {} commands in background with timeout {:?} (container: {})",
                phase_spec.phase.as_str(),
                phase_spec.commands.len(),
                phase_spec.timeout,
                phase_spec.config.container_id
            );
        }
    }

    /// Execute non-blocking phases synchronously (for testing or fallback)
    ///
    /// This method executes non-blocking phases (postStart, postAttach) synchronously
    /// without progress event streaming. For progress event streaming, use
    /// `execute_non_blocking_phases_sync_with_callback`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use deacon_core::container_lifecycle::*;
    /// # use deacon_core::docker::CliDocker;
    /// # async fn example() -> anyhow::Result<()> {
    /// let docker = CliDocker::new();
    /// let result = /* ... get ContainerLifecycleResult ... */
    /// # ContainerLifecycleResult::new();
    ///
    /// // Execute non-blocking phases synchronously
    /// let final_result = result.execute_non_blocking_phases_sync(&docker).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute_non_blocking_phases_sync<D>(self, docker: &D) -> Result<Self>
    where
        D: Docker,
    {
        self.execute_non_blocking_phases_sync_with_callback::<D, fn(ProgressEvent) -> anyhow::Result<()>>(
            docker,
            None,
        )
        .await
    }

    /// Execute non-blocking phases synchronously with optional progress callback
    ///
    /// This method executes non-blocking phases (postStart, postAttach) synchronously
    /// while emitting progress events via the provided callback. This enables log streaming
    /// and real-time progress tracking during non-blocking phase execution.
    ///
    /// # Event Ordering Guarantees
    ///
    /// Progress events are emitted in the following order for each non-blocking phase:
    /// 1. `LifecyclePhaseBegin` - emitted when phase execution starts
    /// 2. For each command in the phase:
    ///    - `LifecycleCommandBegin` - emitted before command execution
    ///    - `LifecycleCommandEnd` - emitted after command completes (success or failure)
    /// 3. `LifecyclePhaseEnd` - emitted when all phase commands complete
    ///
    /// Phases are executed in the order they appear in `non_blocking_phases`:
    /// typically postStart followed by postAttach. Each phase completes fully
    /// (including all its commands) before the next phase begins.
    ///
    /// # Timeout and Error Handling
    ///
    /// - Each phase respects its configured timeout (from `NonBlockingPhaseSpec.timeout`)
    /// - Timeouts are enforced per-phase, not per-command
    /// - Phase failures or timeouts do not stop execution of subsequent phases
    /// - Failed phases are marked unsuccessful but added to the result
    /// - Timeout/execution errors are aggregated in `background_errors`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use deacon_core::container_lifecycle::*;
    /// # use deacon_core::docker::CliDocker;
    /// # use deacon_core::progress::ProgressEvent;
    /// # async fn example() -> anyhow::Result<()> {
    /// let docker = CliDocker::new();
    /// let result = /* ... get ContainerLifecycleResult ... */
    /// # ContainerLifecycleResult::new();
    ///
    /// // Define progress callback for event streaming
    /// let progress_callback = |event: ProgressEvent| {
    ///     println!("Progress: {:?}", event);
    ///     Ok(())
    /// };
    ///
    /// // Execute non-blocking phases with progress streaming
    /// let final_result = result
    ///     .execute_non_blocking_phases_sync_with_callback(&docker, Some(progress_callback))
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute_non_blocking_phases_sync_with_callback<D, F>(
        mut self,
        docker: &D,
        progress_callback: Option<F>,
    ) -> Result<Self>
    where
        D: Docker,
        F: Fn(ProgressEvent) -> anyhow::Result<()>,
    {
        let non_blocking_phases = std::mem::take(&mut self.non_blocking_phases);

        for spec in non_blocking_phases {
            info!(
                "Executing non-blocking phase {} synchronously",
                spec.phase.as_str()
            );

            // Enforce per-phase timeout
            match timeout(
                spec.timeout,
                execute_lifecycle_phase_impl(
                    spec.phase,
                    &spec.commands,
                    &spec.config,
                    docker,
                    &spec.context,
                    spec.detected_shell.as_deref(),
                    progress_callback.as_ref(),
                ),
            )
            .await
            {
                Ok(Ok(phase_result)) => {
                    info!(
                        "Non-blocking phase {} completed successfully",
                        spec.phase.as_str()
                    );

                    // Per FR-002/SC-002: Record marker for runtime hooks on successful execution.
                    // Runtime hooks (postStart, postAttach) always rerun on resume but still
                    // need their markers updated with new timestamps per data-model.md.
                    if phase_result.success {
                        if let Err(e) =
                            record_phase_executed(&spec.workspace_folder, spec.phase, spec.prebuild)
                        {
                            warn!(
                                "Failed to record marker for phase {}: {}",
                                spec.phase.as_str(),
                                e
                            );
                        } else {
                            debug!(
                                "Recorded marker for runtime hook {} at {} (prebuild={})",
                                spec.phase.as_str(),
                                spec.workspace_folder.display(),
                                spec.prebuild
                            );
                        }
                    }

                    self.phases.push(phase_result);
                }
                Ok(Err(e)) => {
                    let error_msg =
                        format!("Non-blocking phase {} failed: {}", spec.phase.as_str(), e);
                    error!("{}", error_msg);
                    self.background_errors.push(error_msg);
                    // Continue with other phases - non-blocking phases should not fail the main flow
                }
                Err(elapsed) => {
                    let error_msg = format!(
                        "Non-blocking phase {} timed out after {:?}: {}",
                        spec.phase.as_str(),
                        spec.timeout,
                        elapsed
                    );
                    error!("{}", error_msg);
                    self.background_errors.push(error_msg);
                    // Continue to next phase
                }
            }
        }

        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_lifecycle_config_creation() {
        let config = ContainerLifecycleConfig {
            container_id: "test-container".to_string(),
            user: Some("root".to_string()),
            container_workspace_folder: "/workspaces/test".to_string(),
            container_env: HashMap::new(),
            skip_post_create: false,
            skip_non_blocking_commands: false,
            non_blocking_timeout: Duration::from_secs(300), // 5 minutes default
            use_login_shell: true,
            user_env_probe: crate::container_env_probe::ContainerProbeMode::LoginShell,
            cache_folder: None,
            force_pty: false,
            dotfiles: None,
            is_prebuild: false,
        };

        assert_eq!(config.container_id, "test-container");
        assert_eq!(config.user, Some("root".to_string()));
        assert_eq!(config.container_workspace_folder, "/workspaces/test");
        assert!(!config.skip_post_create);
        assert!(!config.skip_non_blocking_commands);
        assert_eq!(config.non_blocking_timeout, Duration::from_secs(300));
        assert!(config.use_login_shell);
        assert!(config.dotfiles.is_none());
        assert!(!config.is_prebuild);
    }

    #[test]
    fn test_dotfiles_config() {
        // Test default
        let default_config = DotfilesConfig::default();
        assert!(!default_config.is_configured());
        assert!(default_config.repository.is_none());
        assert!(default_config.target_path.is_none());
        assert!(default_config.install_command.is_none());

        // Test configured
        let config = DotfilesConfig {
            repository: Some("https://github.com/user/dotfiles".to_string()),
            target_path: Some("/home/user/.dotfiles".to_string()),
            install_command: Some("./install.sh".to_string()),
        };
        assert!(config.is_configured());
        assert_eq!(
            config.repository,
            Some("https://github.com/user/dotfiles".to_string())
        );
        assert_eq!(config.target_path, Some("/home/user/.dotfiles".to_string()));
        assert_eq!(config.install_command, Some("./install.sh".to_string()));
    }

    #[test]
    fn test_container_lifecycle_commands_builder() {
        let commands = ContainerLifecycleCommands::new()
            .with_initialize(vec!["echo 'initialize'".to_string()])
            .with_on_create(vec!["echo 'onCreate'".to_string()])
            .with_update_content(vec!["echo 'updateContent'".to_string()])
            .with_post_create(vec!["echo 'postCreate'".to_string()])
            .with_post_start(vec!["echo 'postStart'".to_string()])
            .with_post_attach(vec!["echo 'postAttach'".to_string()]);

        assert!(commands.initialize.is_some());
        assert!(commands.on_create.is_some());
        assert!(commands.update_content.is_some());
        assert!(commands.post_create.is_some());
        assert!(commands.post_start.is_some());
        assert!(commands.post_attach.is_some());
    }

    #[test]
    fn test_lifecycle_commands_all_phases() {
        // Test that all 6 lifecycle phases can be configured
        let commands = ContainerLifecycleCommands::new()
            .with_initialize(vec!["echo 'Phase 1: initialize'".to_string()])
            .with_on_create(vec!["echo 'Phase 2: onCreate'".to_string()])
            .with_update_content(vec!["echo 'Phase 3: updateContent'".to_string()])
            .with_post_create(vec!["echo 'Phase 4: postCreate'".to_string()])
            .with_post_start(vec!["echo 'Phase 5: postStart'".to_string()])
            .with_post_attach(vec!["echo 'Phase 6: postAttach'".to_string()]);

        // Verify all phases are present
        assert_eq!(commands.initialize.as_ref().unwrap().len(), 1);
        assert_eq!(commands.on_create.as_ref().unwrap().len(), 1);
        assert_eq!(commands.update_content.as_ref().unwrap().len(), 1);
        assert_eq!(commands.post_create.as_ref().unwrap().len(), 1);
        assert_eq!(commands.post_start.as_ref().unwrap().len(), 1);
        assert_eq!(commands.post_attach.as_ref().unwrap().len(), 1);

        // Verify phase content
        assert_eq!(
            commands.initialize.as_ref().unwrap()[0],
            "echo 'Phase 1: initialize'"
        );
        assert_eq!(
            commands.on_create.as_ref().unwrap()[0],
            "echo 'Phase 2: onCreate'"
        );
        assert_eq!(
            commands.update_content.as_ref().unwrap()[0],
            "echo 'Phase 3: updateContent'"
        );
        assert_eq!(
            commands.post_create.as_ref().unwrap()[0],
            "echo 'Phase 4: postCreate'"
        );
        assert_eq!(
            commands.post_start.as_ref().unwrap()[0],
            "echo 'Phase 5: postStart'"
        );
        assert_eq!(
            commands.post_attach.as_ref().unwrap()[0],
            "echo 'Phase 6: postAttach'"
        );
    }

    #[test]
    fn test_container_lifecycle_result() {
        let result = ContainerLifecycleResult::new();
        assert!(result.success());
        assert_eq!(result.total_duration(), std::time::Duration::default());
        assert!(result.phases.is_empty());
    }

    #[tokio::test]
    async fn test_execute_non_blocking_phases_sync_without_callback() {
        use crate::docker::mock::{MockDocker, MockDockerConfig, MockExecResponse};
        use crate::lifecycle::LifecyclePhase;
        use crate::variable::SubstitutionContext;

        // Create mock docker
        let config = MockDockerConfig {
            default_exec_response: MockExecResponse {
                exit_code: 0,
                success: true,
                delay: None,
                stdout: None,
                stderr: None,
            },
            ..Default::default()
        };
        let docker = MockDocker::with_config(config);

        // Create a result with non-blocking phases
        let mut result = ContainerLifecycleResult::new();
        let temp_dir = tempfile::TempDir::new().unwrap();
        let substitution_context = SubstitutionContext::new(temp_dir.path()).unwrap();

        let workspace_folder = temp_dir.path().to_path_buf();
        result.non_blocking_phases.push(NonBlockingPhaseSpec {
            phase: LifecyclePhase::PostStart,
            commands: vec!["echo 'test'".to_string()],
            config: ContainerLifecycleConfig {
                container_id: "test".to_string(),
                user: None,
                container_workspace_folder: "/workspace".to_string(),
                container_env: HashMap::new(),
                skip_post_create: false,
                skip_non_blocking_commands: false,
                non_blocking_timeout: Duration::from_secs(30),
                use_login_shell: false, // Use plain sh for tests
                user_env_probe: crate::container_env_probe::ContainerProbeMode::None,
                cache_folder: None,
                force_pty: false,
                dotfiles: None,
                is_prebuild: false,
            },
            context: substitution_context,
            timeout: Duration::from_secs(30),
            detected_shell: None,
            workspace_folder,
            prebuild: false,
        });

        // Execute without callback (None branch)
        let final_result = result
            .execute_non_blocking_phases_sync(&docker)
            .await
            .unwrap();

        // Verify execution completed successfully
        assert_eq!(final_result.phases.len(), 1);
        assert_eq!(final_result.non_blocking_phases.len(), 0);
        assert!(final_result.background_errors.is_empty());
    }

    #[tokio::test]
    async fn test_execute_non_blocking_phases_sync_with_callback() {
        use crate::docker::mock::{MockDocker, MockDockerConfig, MockExecResponse};
        use crate::lifecycle::LifecyclePhase;
        use crate::variable::SubstitutionContext;
        use std::sync::{Arc, Mutex};

        // Create mock docker
        let config = MockDockerConfig {
            default_exec_response: MockExecResponse {
                exit_code: 0,
                success: true,
                delay: None,
                stdout: None,
                stderr: None,
            },
            ..Default::default()
        };
        let docker = MockDocker::with_config(config);

        // Create a result with non-blocking phases
        let mut result = ContainerLifecycleResult::new();
        let temp_dir = tempfile::TempDir::new().unwrap();
        let substitution_context = SubstitutionContext::new(temp_dir.path()).unwrap();

        let workspace_folder = temp_dir.path().to_path_buf();
        result.non_blocking_phases.push(NonBlockingPhaseSpec {
            phase: LifecyclePhase::PostStart,
            commands: vec!["echo 'test'".to_string()],
            config: ContainerLifecycleConfig {
                container_id: "test".to_string(),
                user: None,
                container_workspace_folder: "/workspace".to_string(),
                container_env: HashMap::new(),
                skip_post_create: false,
                skip_non_blocking_commands: false,
                non_blocking_timeout: Duration::from_secs(30),
                use_login_shell: false,
                user_env_probe: crate::container_env_probe::ContainerProbeMode::None,
                cache_folder: None,
                force_pty: false,
                dotfiles: None,
                is_prebuild: false,
            },
            context: substitution_context,
            timeout: Duration::from_secs(30),
            detected_shell: None,
            workspace_folder,
            prebuild: false,
        });

        // Track callback invocations
        let callback_invoked = Arc::new(Mutex::new(false));
        let callback_invoked_clone = callback_invoked.clone();

        let progress_callback = move |_event: ProgressEvent| {
            *callback_invoked_clone.lock().unwrap() = true;
            Ok(())
        };

        // Execute with callback (Some branch)
        let final_result = result
            .execute_non_blocking_phases_sync_with_callback(&docker, Some(progress_callback))
            .await
            .unwrap();

        // Verify execution completed successfully
        assert_eq!(final_result.phases.len(), 1);
        assert_eq!(final_result.non_blocking_phases.len(), 0);
        assert!(final_result.background_errors.is_empty());

        // Verify callback was invoked
        assert!(
            *callback_invoked.lock().unwrap(),
            "Progress callback should have been invoked"
        );
    }

    #[tokio::test]
    async fn test_non_blocking_phases_timeout_unchanged() {
        use crate::docker::mock::{MockDocker, MockDockerConfig, MockExecResponse};
        use crate::lifecycle::LifecyclePhase;
        use crate::variable::SubstitutionContext;

        // Create mock docker with delay that will cause timeout
        let config = MockDockerConfig {
            default_exec_response: MockExecResponse {
                exit_code: 0,
                success: true,
                delay: Some(Duration::from_secs(5)),
                stdout: None,
                stderr: None,
            },
            ..Default::default()
        };
        let docker = MockDocker::with_config(config);

        // Create a result with non-blocking phases
        let mut result = ContainerLifecycleResult::new();
        let temp_dir = tempfile::TempDir::new().unwrap();
        let substitution_context = SubstitutionContext::new(temp_dir.path()).unwrap();

        let workspace_folder = temp_dir.path().to_path_buf();
        result.non_blocking_phases.push(NonBlockingPhaseSpec {
            phase: LifecyclePhase::PostStart,
            commands: vec!["echo 'test'".to_string()],
            config: ContainerLifecycleConfig {
                container_id: "test".to_string(),
                user: None,
                container_workspace_folder: "/workspace".to_string(),
                container_env: HashMap::new(),
                skip_post_create: false,
                skip_non_blocking_commands: false,
                non_blocking_timeout: Duration::from_millis(100), // Very short timeout
                use_login_shell: false,
                user_env_probe: crate::container_env_probe::ContainerProbeMode::None,
                cache_folder: None,
                force_pty: false,
                dotfiles: None,
                is_prebuild: false,
            },
            context: substitution_context,
            timeout: Duration::from_millis(100), // Very short timeout
            detected_shell: None,
            workspace_folder,
            prebuild: false,
        });

        // Execute with callback - should timeout
        let final_result = result
            .execute_non_blocking_phases_sync_with_callback(
                &docker,
                None::<fn(ProgressEvent) -> anyhow::Result<()>>,
            )
            .await
            .unwrap();

        // Verify timeout was handled correctly
        assert_eq!(final_result.background_errors.len(), 1);
        assert!(final_result.background_errors[0].contains("timed out"));
    }

    #[tokio::test]
    async fn test_non_blocking_phases_error_propagation_unchanged() {
        use crate::docker::mock::{MockDocker, MockDockerConfig, MockExecResponse};
        use crate::lifecycle::LifecyclePhase;
        use crate::variable::SubstitutionContext;

        // Create mock docker that returns error
        let config = MockDockerConfig {
            default_exec_response: MockExecResponse {
                exit_code: 1,
                success: false,
                delay: None,
                stdout: None,
                stderr: None,
            },
            ..Default::default()
        };
        let docker = MockDocker::with_config(config);

        // Create a result with non-blocking phases
        let mut result = ContainerLifecycleResult::new();
        let temp_dir = tempfile::TempDir::new().unwrap();
        let substitution_context = SubstitutionContext::new(temp_dir.path()).unwrap();

        let workspace_folder = temp_dir.path().to_path_buf();
        result.non_blocking_phases.push(NonBlockingPhaseSpec {
            phase: LifecyclePhase::PostStart,
            commands: vec!["echo 'test'".to_string()],
            config: ContainerLifecycleConfig {
                container_id: "test".to_string(),
                user: None,
                container_workspace_folder: "/workspace".to_string(),
                container_env: HashMap::new(),
                skip_post_create: false,
                skip_non_blocking_commands: false,
                non_blocking_timeout: Duration::from_secs(30),
                use_login_shell: false,
                user_env_probe: crate::container_env_probe::ContainerProbeMode::None,
                cache_folder: None,
                force_pty: false,
                dotfiles: None,
                is_prebuild: false,
            },
            context: substitution_context,
            timeout: Duration::from_secs(30),
            detected_shell: None,
            workspace_folder,
            prebuild: false,
        });

        // Execute with callback - command should fail but not propagate error
        let final_result = result
            .execute_non_blocking_phases_sync_with_callback(
                &docker,
                None::<fn(ProgressEvent) -> anyhow::Result<()>>,
            )
            .await
            .unwrap();

        // Verify error handling: phase completes but marked as failed
        assert_eq!(final_result.phases.len(), 1);
        assert!(
            !final_result.phases[0].success,
            "Failed phase should be marked as unsuccessful"
        );
        assert_eq!(
            final_result.background_errors.len(),
            0,
            "Non-blocking command failures should not add to background_errors"
        );
    }

    /// Test that runtime hook markers are written after successful non-blocking phase execution.
    /// Per FR-002/SC-002: "The system MUST record completion markers per lifecycle phase"
    #[tokio::test]
    async fn test_runtime_hook_markers_written_on_success() {
        use crate::docker::mock::{MockDocker, MockDockerConfig, MockExecResponse};
        use crate::lifecycle::LifecyclePhase;
        use crate::state::marker_exists;
        use crate::variable::SubstitutionContext;

        // Create mock docker that succeeds
        let config = MockDockerConfig {
            default_exec_response: MockExecResponse {
                exit_code: 0,
                success: true,
                delay: None,
                stdout: None,
                stderr: None,
            },
            ..Default::default()
        };
        let docker = MockDocker::with_config(config);

        // Create a temp workspace for markers
        let temp_dir = tempfile::TempDir::new().unwrap();
        let workspace_folder = temp_dir.path().to_path_buf();
        let substitution_context = SubstitutionContext::new(temp_dir.path()).unwrap();

        // Verify no markers exist before execution
        assert!(
            !marker_exists(&workspace_folder, LifecyclePhase::PostStart, false),
            "postStart marker should not exist before execution"
        );
        assert!(
            !marker_exists(&workspace_folder, LifecyclePhase::PostAttach, false),
            "postAttach marker should not exist before execution"
        );

        // Create a result with both postStart and postAttach phases
        let mut result = ContainerLifecycleResult::new();
        result.non_blocking_phases.push(NonBlockingPhaseSpec {
            phase: LifecyclePhase::PostStart,
            commands: vec!["echo 'postStart'".to_string()],
            config: ContainerLifecycleConfig {
                container_id: "test".to_string(),
                user: None,
                container_workspace_folder: "/workspace".to_string(),
                container_env: HashMap::new(),
                skip_post_create: false,
                skip_non_blocking_commands: false,
                non_blocking_timeout: Duration::from_secs(30),
                use_login_shell: false,
                user_env_probe: crate::container_env_probe::ContainerProbeMode::None,
                cache_folder: None,
                force_pty: false,
                dotfiles: None,
                is_prebuild: false,
            },
            context: substitution_context.clone(),
            timeout: Duration::from_secs(30),
            detected_shell: None,
            workspace_folder: workspace_folder.clone(),
            prebuild: false,
        });
        result.non_blocking_phases.push(NonBlockingPhaseSpec {
            phase: LifecyclePhase::PostAttach,
            commands: vec!["echo 'postAttach'".to_string()],
            config: ContainerLifecycleConfig {
                container_id: "test".to_string(),
                user: None,
                container_workspace_folder: "/workspace".to_string(),
                container_env: HashMap::new(),
                skip_post_create: false,
                skip_non_blocking_commands: false,
                non_blocking_timeout: Duration::from_secs(30),
                use_login_shell: false,
                user_env_probe: crate::container_env_probe::ContainerProbeMode::None,
                cache_folder: None,
                force_pty: false,
                dotfiles: None,
                is_prebuild: false,
            },
            context: substitution_context,
            timeout: Duration::from_secs(30),
            detected_shell: None,
            workspace_folder: workspace_folder.clone(),
            prebuild: false,
        });

        // Execute non-blocking phases
        let final_result = result
            .execute_non_blocking_phases_sync(&docker)
            .await
            .unwrap();

        // Verify both phases executed successfully
        assert_eq!(final_result.phases.len(), 2, "Both phases should execute");
        assert!(final_result.phases[0].success, "postStart should succeed");
        assert!(final_result.phases[1].success, "postAttach should succeed");

        // Verify markers were written for both phases
        assert!(
            marker_exists(&workspace_folder, LifecyclePhase::PostStart, false),
            "postStart marker should exist after successful execution"
        );
        assert!(
            marker_exists(&workspace_folder, LifecyclePhase::PostAttach, false),
            "postAttach marker should exist after successful execution"
        );
    }

    /// Test that runtime hook markers are NOT written when phase fails.
    /// Per FR-002: markers should only be written on successful execution.
    #[tokio::test]
    async fn test_runtime_hook_markers_not_written_on_failure() {
        use crate::docker::mock::{MockDocker, MockDockerConfig, MockExecResponse};
        use crate::lifecycle::LifecyclePhase;
        use crate::state::marker_exists;
        use crate::variable::SubstitutionContext;

        // Create mock docker that fails (exit code 1)
        let config = MockDockerConfig {
            default_exec_response: MockExecResponse {
                exit_code: 1,
                success: false,
                delay: None,
                stdout: None,
                stderr: None,
            },
            ..Default::default()
        };
        let docker = MockDocker::with_config(config);

        // Create a temp workspace for markers
        let temp_dir = tempfile::TempDir::new().unwrap();
        let workspace_folder = temp_dir.path().to_path_buf();
        let substitution_context = SubstitutionContext::new(temp_dir.path()).unwrap();

        // Create a result with a postStart phase that will fail
        let mut result = ContainerLifecycleResult::new();
        result.non_blocking_phases.push(NonBlockingPhaseSpec {
            phase: LifecyclePhase::PostStart,
            commands: vec!["exit 1".to_string()],
            config: ContainerLifecycleConfig {
                container_id: "test".to_string(),
                user: None,
                container_workspace_folder: "/workspace".to_string(),
                container_env: HashMap::new(),
                skip_post_create: false,
                skip_non_blocking_commands: false,
                non_blocking_timeout: Duration::from_secs(30),
                use_login_shell: false,
                user_env_probe: crate::container_env_probe::ContainerProbeMode::None,
                cache_folder: None,
                force_pty: false,
                dotfiles: None,
                is_prebuild: false,
            },
            context: substitution_context,
            timeout: Duration::from_secs(30),
            detected_shell: None,
            workspace_folder: workspace_folder.clone(),
            prebuild: false,
        });

        // Execute non-blocking phases
        let final_result = result
            .execute_non_blocking_phases_sync(&docker)
            .await
            .unwrap();

        // Verify phase executed but failed
        assert_eq!(final_result.phases.len(), 1, "Phase should execute");
        assert!(!final_result.phases[0].success, "postStart should fail");

        // Verify marker was NOT written because phase failed
        assert!(
            !marker_exists(&workspace_folder, LifecyclePhase::PostStart, false),
            "postStart marker should NOT exist after failed execution"
        );
    }

    /// Test that runtime hooks execute in correct order (postStart before postAttach).
    /// Per spec FR-003: "rerun only postStart followed by postAttach"
    #[tokio::test]
    async fn test_runtime_hook_execution_order() {
        use crate::docker::mock::{MockDocker, MockDockerConfig, MockExecResponse};
        use crate::lifecycle::LifecyclePhase;
        use crate::variable::SubstitutionContext;
        use std::sync::{Arc, Mutex};

        // Create mock docker that succeeds
        let config = MockDockerConfig {
            default_exec_response: MockExecResponse {
                exit_code: 0,
                success: true,
                delay: None,
                stdout: None,
                stderr: None,
            },
            ..Default::default()
        };
        let docker = MockDocker::with_config(config);

        // Create a temp workspace
        let temp_dir = tempfile::TempDir::new().unwrap();
        let workspace_folder = temp_dir.path().to_path_buf();
        let substitution_context = SubstitutionContext::new(temp_dir.path()).unwrap();

        // Track execution order via callback
        let execution_order = Arc::new(Mutex::new(Vec::new()));
        let execution_order_clone = execution_order.clone();

        let progress_callback = move |event: ProgressEvent| {
            if let crate::progress::ProgressEvent::LifecyclePhaseBegin { phase, .. } = event {
                execution_order_clone.lock().unwrap().push(phase);
            }
            Ok(())
        };

        // Create a result with postStart and postAttach in that order
        let mut result = ContainerLifecycleResult::new();
        result.non_blocking_phases.push(NonBlockingPhaseSpec {
            phase: LifecyclePhase::PostStart,
            commands: vec!["echo 'postStart'".to_string()],
            config: ContainerLifecycleConfig {
                container_id: "test".to_string(),
                user: None,
                container_workspace_folder: "/workspace".to_string(),
                container_env: HashMap::new(),
                skip_post_create: false,
                skip_non_blocking_commands: false,
                non_blocking_timeout: Duration::from_secs(30),
                use_login_shell: false,
                user_env_probe: crate::container_env_probe::ContainerProbeMode::None,
                cache_folder: None,
                force_pty: false,
                dotfiles: None,
                is_prebuild: false,
            },
            context: substitution_context.clone(),
            timeout: Duration::from_secs(30),
            detected_shell: None,
            workspace_folder: workspace_folder.clone(),
            prebuild: false,
        });
        result.non_blocking_phases.push(NonBlockingPhaseSpec {
            phase: LifecyclePhase::PostAttach,
            commands: vec!["echo 'postAttach'".to_string()],
            config: ContainerLifecycleConfig {
                container_id: "test".to_string(),
                user: None,
                container_workspace_folder: "/workspace".to_string(),
                container_env: HashMap::new(),
                skip_post_create: false,
                skip_non_blocking_commands: false,
                non_blocking_timeout: Duration::from_secs(30),
                use_login_shell: false,
                user_env_probe: crate::container_env_probe::ContainerProbeMode::None,
                cache_folder: None,
                force_pty: false,
                dotfiles: None,
                is_prebuild: false,
            },
            context: substitution_context,
            timeout: Duration::from_secs(30),
            detected_shell: None,
            workspace_folder,
            prebuild: false,
        });

        // Execute non-blocking phases with callback
        let _final_result = result
            .execute_non_blocking_phases_sync_with_callback(&docker, Some(progress_callback))
            .await
            .unwrap();

        // Verify execution order: postStart must come before postAttach
        let order = execution_order.lock().unwrap();
        assert_eq!(order.len(), 2, "Both phases should have been executed");
        assert_eq!(order[0], "postStart", "postStart should execute first");
        assert_eq!(order[1], "postAttach", "postAttach should execute second");
    }

    #[test]
    fn test_lifecycle_command_source_feature() {
        let source = LifecycleCommandSource::Feature {
            id: "ghcr.io/devcontainers/features/node".to_string(),
        };

        // Test Debug trait
        assert_eq!(
            format!("{:?}", source),
            "Feature { id: \"ghcr.io/devcontainers/features/node\" }"
        );

        // Test Display trait
        assert_eq!(
            source.to_string(),
            "feature:ghcr.io/devcontainers/features/node"
        );

        // Test Clone and PartialEq
        let cloned = source.clone();
        assert_eq!(source, cloned);
    }

    #[test]
    fn test_lifecycle_command_source_config() {
        let source = LifecycleCommandSource::Config;

        // Test Debug trait
        assert_eq!(format!("{:?}", source), "Config");

        // Test Display trait
        assert_eq!(source.to_string(), "config");

        // Test Clone and PartialEq
        let cloned = source.clone();
        assert_eq!(source, cloned);
    }

    #[test]
    fn test_lifecycle_command_source_equality() {
        let feature1 = LifecycleCommandSource::Feature {
            id: "node".to_string(),
        };
        let feature2 = LifecycleCommandSource::Feature {
            id: "node".to_string(),
        };
        let feature3 = LifecycleCommandSource::Feature {
            id: "python".to_string(),
        };
        let config1 = LifecycleCommandSource::Config;
        let config2 = LifecycleCommandSource::Config;

        // Same feature IDs should be equal
        assert_eq!(feature1, feature2);

        // Different feature IDs should not be equal
        assert_ne!(feature1, feature3);

        // Config sources should be equal
        assert_eq!(config1, config2);

        // Feature and Config should not be equal
        assert_ne!(feature1, config1);
    }

    #[test]
    fn test_aggregated_lifecycle_command_creation() {
        let source = LifecycleCommandSource::Feature {
            id: "node".to_string(),
        };
        let command = serde_json::json!("npm install");

        let aggregated = AggregatedLifecycleCommand {
            command: command.clone(),
            source: source.clone(),
        };

        // Verify fields
        assert_eq!(aggregated.command, command);
        assert_eq!(aggregated.source, source);
    }

    #[test]
    fn test_aggregated_lifecycle_command_with_different_command_types() {
        // Test with string command
        let string_cmd = AggregatedLifecycleCommand {
            command: serde_json::json!("echo hello"),
            source: LifecycleCommandSource::Config,
        };
        assert_eq!(string_cmd.command, serde_json::json!("echo hello"));

        // Test with array command
        let array_cmd = AggregatedLifecycleCommand {
            command: serde_json::json!(["npm", "install", "--verbose"]),
            source: LifecycleCommandSource::Feature {
                id: "node".to_string(),
            },
        };
        assert_eq!(
            array_cmd.command,
            serde_json::json!(["npm", "install", "--verbose"])
        );

        // Test with object command (parallel commands)
        let object_cmd = AggregatedLifecycleCommand {
            command: serde_json::json!({
                "npm": "npm install",
                "build": "npm run build"
            }),
            source: LifecycleCommandSource::Feature {
                id: "node".to_string(),
            },
        };
        assert_eq!(
            object_cmd.command,
            serde_json::json!({
                "npm": "npm install",
                "build": "npm run build"
            })
        );
    }

    #[test]
    fn test_aggregated_lifecycle_command_clone() {
        let original = AggregatedLifecycleCommand {
            command: serde_json::json!(["test", "command"]),
            source: LifecycleCommandSource::Feature {
                id: "python".to_string(),
            },
        };

        let cloned = original.clone();

        // Verify clone has same values
        assert_eq!(cloned.command, original.command);
        assert_eq!(cloned.source, original.source);
    }

    #[test]
    fn test_aggregated_lifecycle_command_debug() {
        let cmd = AggregatedLifecycleCommand {
            command: serde_json::json!("test"),
            source: LifecycleCommandSource::Config,
        };

        let debug_str = format!("{:?}", cmd);
        assert!(debug_str.contains("AggregatedLifecycleCommand"));
        assert!(debug_str.contains("command"));
        assert!(debug_str.contains("source"));
    }
}
