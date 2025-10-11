//! Container lifecycle command execution
//!
//! This module provides container-specific lifecycle command execution with full
//! variable substitution including containerEnv & containerWorkspaceFolder.

use crate::docker::{CliDocker, Docker, ExecConfig};
use crate::errors::{DeaconError, Result};
use crate::lifecycle::{ExecutionContext, ExecutionMode, LifecycleCommands, LifecyclePhase};
use crate::progress::{ProgressEvent, ProgressTracker};
use crate::redaction::{redact_if_enabled, RedactionConfig};
use crate::variable::{SubstitutionContext, SubstitutionReport, VariableSubstitution};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tracing::{debug, error, info, instrument};

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

    // Create substitution context with container information
    let container_context = substitution_context
        .clone()
        .with_container_workspace_folder(config.container_workspace_folder.clone())
        .with_container_env(config.container_env.clone());

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

    // Execute onCreate phase
    if let Some(on_create_commands) = &commands.on_create {
        result.phases.push(
            execute_lifecycle_phase(
                LifecyclePhase::OnCreate,
                on_create_commands,
                config,
                &docker,
                &container_context,
                progress_callback.as_ref(),
            )
            .await?,
        );
    }

    // Execute updateContent phase if provided
    if let Some(update_content_commands) = &commands.update_content {
        result.phases.push(
            execute_lifecycle_phase(
                LifecyclePhase::UpdateContent,
                update_content_commands,
                config,
                &docker,
                &container_context,
                progress_callback.as_ref(),
            )
            .await?,
        );
    }

    // Execute postCreate phase (if not skipped)
    if !config.skip_post_create {
        if let Some(post_create_commands) = &commands.post_create {
            result.phases.push(
                execute_lifecycle_phase(
                    LifecyclePhase::PostCreate,
                    post_create_commands,
                    config,
                    &docker,
                    &container_context,
                    progress_callback.as_ref(),
                )
                .await?,
            );
        }
    } else {
        info!("Skipping postCreate phase");
    }

    // Execute postStart phase (if not skipped by non-blocking commands flag)
    if !config.skip_non_blocking_commands {
        if let Some(post_start_commands) = &commands.post_start {
            // Add postStart phase to non-blocking specs for later execution
            result.non_blocking_phases.push(NonBlockingPhaseSpec {
                phase: LifecyclePhase::PostStart,
                commands: post_start_commands.clone(),
                config: config.clone(),
                context: container_context.clone(),
                timeout: config.non_blocking_timeout,
            });
            info!("Added postStart phase for non-blocking execution");
        }
    } else {
        info!("Skipping postStart phase (non-blocking commands disabled)");
    }

    // Execute postAttach phase (if not skipped by non-blocking commands flag)
    if !config.skip_non_blocking_commands {
        if let Some(post_attach_commands) = &commands.post_attach {
            // Add postAttach phase to non-blocking specs for later execution
            result.non_blocking_phases.push(NonBlockingPhaseSpec {
                phase: LifecyclePhase::PostAttach,
                commands: post_attach_commands.clone(),
                config: config.clone(),
                context: container_context.clone(),
                timeout: config.non_blocking_timeout,
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
    progress_callback: Option<&F>,
) -> Result<PhaseResult>
where
    D: Docker,
    F: Fn(ProgressEvent) -> anyhow::Result<()>,
{
    execute_lifecycle_phase_impl(phase, commands, config, docker, context, progress_callback).await
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
            tty: false,
            interactive: false,
            detach: false,
        };

        // Execute command in container using sh -c
        let command_args = vec![
            "sh".to_string(),
            "-c".to_string(),
            substituted_command.clone(),
        ];

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
        };

        assert_eq!(config.container_id, "test-container");
        assert_eq!(config.user, Some("root".to_string()));
        assert_eq!(config.container_workspace_folder, "/workspaces/test");
        assert!(!config.skip_post_create);
        assert!(!config.skip_non_blocking_commands);
        assert_eq!(config.non_blocking_timeout, Duration::from_secs(300));
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
            },
            context: substitution_context,
            timeout: Duration::from_secs(30),
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
            },
            context: substitution_context,
            timeout: Duration::from_secs(30),
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
            },
            context: substitution_context,
            timeout: Duration::from_millis(100), // Very short timeout
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
            },
            context: substitution_context,
            timeout: Duration::from_secs(30),
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
}
