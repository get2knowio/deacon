//! Container lifecycle command execution
//!
//! This module provides container-specific lifecycle command execution with full
//! variable substitution including containerEnv & containerWorkspaceFolder.

use crate::docker::{CliDocker, Docker, ExecConfig};
use crate::errors::{DeaconError, Result};
use crate::lifecycle::LifecyclePhase;
use crate::progress::{ProgressEvent, ProgressTracker};
use crate::redaction::{redact_if_enabled, RedactionConfig};
use crate::variable::{SubstitutionContext, SubstitutionReport, VariableSubstitution};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{debug, error, info, instrument, warn};

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
/// 1. onCreate
/// 2. postCreate (if not skipped)
/// 3. postStart (if not skipped by skip_non_blocking_commands)
/// 4. postAttach (if not skipped by skip_non_blocking_commands)
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
    info!(
        "Starting container lifecycle execution in container: {}",
        config.container_id
    );

    let mut result = ContainerLifecycleResult::new();

    // Create substitution context with container information
    let container_context = substitution_context
        .clone()
        .with_container_workspace_folder(config.container_workspace_folder.clone())
        .with_container_env(config.container_env.clone());

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
            // Execute postStart phase as non-blocking background task
            let background_task = spawn_non_blocking_phase(
                LifecyclePhase::PostStart,
                post_start_commands.clone(),
                config.clone(),
                Arc::new(CliDocker::new()), // Clone docker implementation
                container_context.clone(),
                progress_callback.as_ref().map(|_| ()), // TODO: Handle progress callback in background task
                config.non_blocking_timeout,
            ).await?;
            
            result.background_tasks.push(background_task);
            info!("Started non-blocking postStart phase in background");
        }
    } else {
        info!("Skipping postStart phase (non-blocking commands disabled)");
    }

    // Execute postAttach phase (if not skipped by non-blocking commands flag)
    if !config.skip_non_blocking_commands {
        if let Some(post_attach_commands) = &commands.post_attach {
            // Execute postAttach phase as non-blocking background task
            let background_task = spawn_non_blocking_phase(
                LifecyclePhase::PostAttach,
                post_attach_commands.clone(),
                config.clone(),
                Arc::new(CliDocker::new()), // Clone docker implementation
                container_context.clone(),
                progress_callback.as_ref().map(|_| ()), // TODO: Handle progress callback in background task
                config.non_blocking_timeout,
            ).await?;
            
            result.background_tasks.push(background_task);
            info!("Started non-blocking postAttach phase in background");
        }
    } else {
        info!("Skipping postAttach phase (non-blocking commands disabled)");
    }

    info!("Completed container lifecycle execution");
    Ok(result)
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

/// Spawn a non-blocking lifecycle phase as a background task
#[instrument(skip(commands, config, docker, context, _progress_callback))]
async fn spawn_non_blocking_phase<D>(
    phase: LifecyclePhase,
    commands: Vec<String>,
    config: ContainerLifecycleConfig,
    docker: Arc<D>,
    context: SubstitutionContext,
    _progress_callback: Option<()>, // TODO: Implement progress callback forwarding
    timeout_duration: Duration,
) -> Result<BackgroundTaskHandle>
where
    D: Docker + Send + Sync + 'static,
{
    info!("Spawning non-blocking lifecycle phase: {}", phase.as_str());

    let (cancellation_tx, mut cancellation_rx) = mpsc::unbounded_channel::<()>();

    let handle = tokio::spawn(async move {
        // Execute the phase with timeout and cancellation support
        let phase_future = execute_lifecycle_phase_impl::<D, fn(ProgressEvent) -> anyhow::Result<()>>(
            phase,
            &commands,
            &config,
            docker.as_ref(),
            &context,
            None, // TODO: Pass progress callback
        );

        tokio::select! {
            result = timeout(timeout_duration, phase_future) => {
                match result {
                    Ok(phase_result) => {
                        info!("Non-blocking phase {} completed within timeout", phase.as_str());
                        phase_result
                    }
                    Err(_) => {
                        error!("Non-blocking phase {} timed out after {:?}", phase.as_str(), timeout_duration);
                        Err(DeaconError::Lifecycle(format!(
                            "Non-blocking phase {} timed out after {:?}",
                            phase.as_str(),
                            timeout_duration
                        )))
                    }
                }
            }
            _ = cancellation_rx.recv() => {
                warn!("Non-blocking phase {} was cancelled", phase.as_str());
                Err(DeaconError::Lifecycle(format!(
                    "Non-blocking phase {} was cancelled",
                    phase.as_str()
                )))
            }
        }
    });

    Ok(BackgroundTaskHandle {
        phase,
        handle,
        cancellation_tx: Some(cancellation_tx),
    })
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
    info!("Executing lifecycle phase: {}", phase.as_str());
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

    info!(
        "Completed lifecycle phase: {} in {:?}",
        phase.as_str(),
        phase_result.total_duration
    );
    Ok(phase_result)
}

/// Commands for each lifecycle phase
#[derive(Debug, Clone, Default)]
pub struct ContainerLifecycleCommands {
    /// Commands to run during onCreate phase
    pub on_create: Option<Vec<String>>,
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

    /// Set onCreate commands
    pub fn with_on_create(mut self, commands: Vec<String>) -> Self {
        self.on_create = Some(commands);
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
    /// Background tasks for non-blocking phases
    pub background_tasks: Vec<BackgroundTaskHandle>,
}

/// Handle for a background non-blocking task
#[derive(Debug)]
pub struct BackgroundTaskHandle {
    /// Phase being executed in the background
    pub phase: LifecyclePhase,
    /// Task handle for the background execution
    pub handle: JoinHandle<Result<PhaseResult>>,
    /// Channel sender for cancellation (if needed)
    pub cancellation_tx: Option<mpsc::UnboundedSender<()>>,
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
            background_tasks: Vec::new(),
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

    /// Wait for all background tasks to complete or timeout
    pub async fn await_background_tasks(mut self, timeout_duration: Option<Duration>) -> Self {
        let tasks = std::mem::take(&mut self.background_tasks);
        
        for task in tasks {
            let result = if let Some(timeout_duration) = timeout_duration {
                match timeout(timeout_duration, task.handle).await {
                    Ok(Ok(Ok(phase_result))) => {
                        info!("Background phase {} completed successfully", task.phase.as_str());
                        Some(phase_result)
                    }
                    Ok(Ok(Err(e))) => {
                        error!("Background phase {} failed: {}", task.phase.as_str(), e);
                        None
                    }
                    Ok(Err(e)) => {
                        error!("Background phase {} panicked: {}", task.phase.as_str(), e);
                        None
                    }
                    Err(_) => {
                        warn!("Background phase {} timed out after {:?}", task.phase.as_str(), timeout_duration);
                        None
                    }
                }
            } else {
                match task.handle.await {
                    Ok(Ok(phase_result)) => {
                        info!("Background phase {} completed successfully", task.phase.as_str());
                        Some(phase_result)
                    }
                    Ok(Err(e)) => {
                        error!("Background phase {} failed: {}", task.phase.as_str(), e);
                        None
                    }
                    Err(e) => {
                        error!("Background phase {} panicked: {}", task.phase.as_str(), e);
                        None
                    }
                }
            };

            if let Some(phase_result) = result {
                self.phases.push(phase_result);
            }
        }

        self
    }

    /// Cancel all background tasks
    pub async fn cancel_background_tasks(mut self) {
        let tasks = std::mem::take(&mut self.background_tasks);
        
        for task in tasks {
            if let Some(cancellation_tx) = task.cancellation_tx {
                let _ = cancellation_tx.send(());
            }
            task.handle.abort();
            let _ = task.handle.await;
        }
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
            .with_on_create(vec!["echo 'onCreate'".to_string()])
            .with_post_create(vec!["echo 'postCreate'".to_string()])
            .with_post_start(vec!["echo 'postStart'".to_string()])
            .with_post_attach(vec!["echo 'postAttach'".to_string()]);

        assert!(commands.on_create.is_some());
        assert!(commands.post_create.is_some());
        assert!(commands.post_start.is_some());
        assert!(commands.post_attach.is_some());
    }

    #[test]
    fn test_container_lifecycle_result() {
        let result = ContainerLifecycleResult::new();
        assert!(result.success());
        assert_eq!(result.total_duration(), std::time::Duration::default());
        assert!(result.phases.is_empty());
    }
}
