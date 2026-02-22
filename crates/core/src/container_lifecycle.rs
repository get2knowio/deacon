//! Container lifecycle command execution
//!
//! This module provides container-specific lifecycle command execution with full
//! variable substitution including containerEnv & containerWorkspaceFolder.

use crate::docker::{CliDocker, Docker, ExecConfig};
use crate::errors::{ConfigError, DeaconError, Result};
use crate::lifecycle::LifecyclePhase;
use crate::progress::{ProgressEvent, ProgressTracker};
use crate::redaction::{redact_if_enabled, RedactionConfig};
use crate::state::record_phase_executed;
use crate::variable::{SubstitutionContext, SubstitutionReport, VariableSubstitution};
use indexmap::IndexMap;
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

/// Check if a lifecycle command value is empty or null.
///
/// This helper function determines whether a lifecycle command should be filtered out
/// during command aggregation. Commands are considered empty if they are:
/// - `null` - No command specified
/// - Empty string `""` - Blank command
/// - Empty array `[]` - Array with no elements
/// - Empty object `{}` - Object with no properties
///
/// # Arguments
///
/// * `cmd` - The command value to check (from devcontainer.json or feature metadata)
///
/// # Returns
///
/// `true` if the command is empty and should be skipped, `false` otherwise
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use deacon_core::container_lifecycle::is_empty_command;
///
/// // Null command is empty
/// assert!(is_empty_command(&json!(null)));
///
/// // Empty string is empty
/// assert!(is_empty_command(&json!("")));
///
/// // Empty array is empty
/// assert!(is_empty_command(&json!([])));
///
/// // Empty object is empty
/// assert!(is_empty_command(&json!({})));
///
/// // Non-empty string is not empty
/// assert!(!is_empty_command(&json!("npm install")));
///
/// // Non-empty array is not empty
/// assert!(!is_empty_command(&json!(["npm", "install"])));
///
/// // Non-empty object is not empty
/// assert!(!is_empty_command(&json!({"build": "npm run build"})));
/// ```
pub fn is_empty_command(cmd: &serde_json::Value) -> bool {
    match cmd {
        // Null is empty
        serde_json::Value::Null => true,
        // Empty string is empty
        serde_json::Value::String(s) => s.is_empty(),
        // Empty array is empty
        serde_json::Value::Array(arr) => arr.is_empty(),
        // Empty object is empty
        serde_json::Value::Object(obj) => obj.is_empty(),
        // All other values (non-empty strings, arrays, objects, numbers, booleans) are not empty
        _ => false,
    }
}

/// A parsed lifecycle command value that preserves format semantics.
///
/// The DevContainer spec defines three formats for lifecycle commands:
/// - String: executed through a shell (`/bin/sh -c` in container, platform shell on host)
/// - Array: exec-style, passed directly to OS without shell interpretation
/// - Object: named parallel commands, each value is itself a Shell or Exec command
#[derive(Debug, Clone, PartialEq)]
pub enum LifecycleCommandValue {
    /// Shell-interpreted command string (e.g., "npm install && npm build")
    Shell(String),
    /// Exec-style command as program + arguments (e.g., ["npm", "install"])
    Exec(Vec<String>),
    /// Named parallel commands (e.g., {"install": "npm install", "build": ["npm", "run", "build"]})
    Parallel(IndexMap<String, LifecycleCommandValue>),
}

impl LifecycleCommandValue {
    /// Parse a JSON value into a typed lifecycle command.
    ///
    /// # Returns
    ///
    /// - `Ok(None)` for `null` values
    /// - `Ok(Some(Shell(s)))` for string values (even empty strings - caller filters)
    /// - `Ok(Some(Exec(args)))` for arrays where all elements are strings
    /// - `Ok(Some(Parallel(map)))` for objects (preserves insertion order)
    /// - `Err` for invalid types (number, boolean) or arrays with non-string elements
    ///
    /// NOTE: Object key ordering relies on `serde_json`'s `preserve_order` feature
    /// being enabled in Cargo.toml. Without it, parallel command execution order
    /// would be non-deterministic.
    pub fn from_json_value(value: &serde_json::Value) -> Result<Option<Self>> {
        match value {
            serde_json::Value::Null => Ok(None),
            serde_json::Value::String(s) => Ok(Some(LifecycleCommandValue::Shell(s.clone()))),
            serde_json::Value::Array(arr) => {
                let mut strings = Vec::with_capacity(arr.len());
                for (i, elem) in arr.iter().enumerate() {
                    match elem {
                        serde_json::Value::String(s) => strings.push(s.clone()),
                        other => {
                            return Err(DeaconError::Config(ConfigError::Validation {
                                message: format!(
                                    "lifecycle command array element at index {} must be a string, got {}",
                                    i,
                                    json_type_name(other)
                                ),
                            }));
                        }
                    }
                }
                Ok(Some(LifecycleCommandValue::Exec(strings)))
            }
            serde_json::Value::Object(map) => {
                let mut parsed_map = IndexMap::with_capacity(map.len());
                for (key, val) in map {
                    match val {
                        serde_json::Value::Null => {
                            warn!(
                                key = %key,
                                "Skipping null value in lifecycle command object"
                            );
                        }
                        serde_json::Value::String(s) => {
                            if s.is_empty() {
                                continue;
                            }
                            parsed_map.insert(key.clone(), LifecycleCommandValue::Shell(s.clone()));
                        }
                        serde_json::Value::Array(arr) => {
                            let mut strings = Vec::with_capacity(arr.len());
                            for (idx, elem) in arr.iter().enumerate() {
                                match elem {
                                    serde_json::Value::String(s) => strings.push(s.clone()),
                                    _ => {
                                        return Err(DeaconError::Config(
                                            ConfigError::Validation {
                                                message: format!(
                                                    "Lifecycle command object entry '{}' contains array with non-string element at index {}",
                                                    key, idx
                                                ),
                                            },
                                        ));
                                    }
                                }
                            }
                            if !strings.is_empty() {
                                parsed_map
                                    .insert(key.clone(), LifecycleCommandValue::Exec(strings));
                            }
                        }
                        other => {
                            warn!(
                                key = %key,
                                value_type = %json_type_name(other),
                                "Skipping lifecycle command object entry with invalid value type"
                            );
                        }
                    }
                }
                Ok(Some(LifecycleCommandValue::Parallel(parsed_map)))
            }
            other => Err(DeaconError::Config(ConfigError::Validation {
                message: format!(
                    "lifecycle command must be a string, array, or object, got {}",
                    json_type_name(other)
                ),
            })),
        }
    }

    /// Check if this command value is empty.
    ///
    /// - `Shell(s)` is empty if the string is empty
    /// - `Exec(args)` is empty if the argument list is empty
    /// - `Parallel(map)` is empty if the map has no entries
    pub fn is_empty(&self) -> bool {
        match self {
            LifecycleCommandValue::Shell(s) => s.is_empty(),
            LifecycleCommandValue::Exec(args) => args.is_empty(),
            LifecycleCommandValue::Parallel(map) => map.is_empty(),
        }
    }

    /// Apply variable substitution to this command value, returning a new value.
    ///
    /// - `Shell(s)`: substitutes the whole string
    /// - `Exec(args)`: substitutes each element independently
    /// - `Parallel(map)`: substitutes each value recursively
    pub fn substitute_variables(&self, context: &SubstitutionContext) -> Self {
        match self {
            LifecycleCommandValue::Shell(s) => {
                let mut report = SubstitutionReport::new();
                let substituted = VariableSubstitution::substitute_string(s, context, &mut report);
                LifecycleCommandValue::Shell(substituted)
            }
            LifecycleCommandValue::Exec(args) => {
                // Reuse a single report across all args to avoid per-element allocation
                let mut report = SubstitutionReport::new();
                let substituted_args = args
                    .iter()
                    .map(|arg| VariableSubstitution::substitute_string(arg, context, &mut report))
                    .collect();
                LifecycleCommandValue::Exec(substituted_args)
            }
            LifecycleCommandValue::Parallel(map) => {
                let substituted_map = map
                    .iter()
                    .map(|(key, val)| (key.clone(), val.substitute_variables(context)))
                    .collect();
                LifecycleCommandValue::Parallel(substituted_map)
            }
        }
    }
}

/// Allow comparing `LifecycleCommandValue` with `serde_json::Value` in tests.
///
/// This implementation converts the JSON value to a `LifecycleCommandValue` and
/// compares structurally. Useful for test assertions where the expected value is
/// expressed as `json!(...)`.
impl PartialEq<serde_json::Value> for LifecycleCommandValue {
    fn eq(&self, other: &serde_json::Value) -> bool {
        match (self, other) {
            (LifecycleCommandValue::Shell(s), serde_json::Value::String(os)) => s == os,
            (LifecycleCommandValue::Exec(args), serde_json::Value::Array(arr)) => {
                args.len() == arr.len()
                    && args.iter().zip(arr.iter()).all(|(a, b)| match b {
                        serde_json::Value::String(bs) => a == bs,
                        _ => false,
                    })
            }
            (LifecycleCommandValue::Parallel(map), serde_json::Value::Object(obj)) => {
                map.len() == obj.len()
                    && map
                        .iter()
                        .all(|(k, v)| obj.get(k).is_some_and(|ov| v == ov))
            }
            _ => false,
        }
    }
}

/// Return a human-readable name for a JSON value type.
fn json_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Result of executing one named command within a parallel set.
#[derive(Debug, Clone)]
pub struct ParallelCommandResult {
    /// The named key from the object (e.g., "install", "build")
    pub key: String,
    /// Exit code from the command
    pub exit_code: i32,
    /// Duration of execution
    pub duration: std::time::Duration,
    /// Whether the command succeeded
    pub success: bool,
    /// Captured stdout from the command
    pub stdout: String,
    /// Captured stderr from the command
    pub stderr: String,
}

/// Aggregate lifecycle commands for a specific phase from features and config.
///
/// This function collects lifecycle commands from all resolved features (in installation order)
/// and the devcontainer configuration, filtering out empty/null commands and preserving
/// source attribution for error reporting.
///
/// # Ordering
///
/// Per the feature lifecycle contract:
/// 1. Feature commands execute first, in installation order
/// 2. Config command executes last
///
/// This ensures features set up prerequisites (environment, tools) before config commands run.
///
/// # Arguments
///
/// * `phase` - Which lifecycle phase to aggregate (onCreate, postCreate, etc.)
/// * `features` - Resolved features in installation order
/// * `config` - DevContainerConfig with user commands
///
/// # Returns
///
/// `Result<LifecycleCommandList>` with feature commands first, then config.
/// Empty/null commands are filtered out. Returns an error if any command
/// value has an invalid type (e.g., number or boolean).
///
/// # Examples
///
/// ```
/// use deacon_core::container_lifecycle::aggregate_lifecycle_commands;
/// use deacon_core::lifecycle::LifecyclePhase;
/// use deacon_core::features::ResolvedFeature;
/// use deacon_core::config::DevContainerConfig;
///
/// // Given features with lifecycle commands and a config
/// let features = vec![/* ... */];
/// let config = DevContainerConfig::default();
///
/// // Aggregate onCreate commands
/// let command_list = aggregate_lifecycle_commands(
///     LifecyclePhase::OnCreate,
///     &features,
///     &config,
/// ).unwrap();
///
/// // Commands are ordered: feature1, feature2, ..., config
/// ```
pub fn aggregate_lifecycle_commands(
    phase: LifecyclePhase,
    features: &[crate::features::ResolvedFeature],
    config: &crate::config::DevContainerConfig,
) -> Result<LifecycleCommandList> {
    let mut commands = Vec::new();

    // Feature commands first, in installation order
    for feature in features {
        let cmd_opt = match phase {
            LifecyclePhase::Initialize => None, // Features don't have initialize commands
            LifecyclePhase::OnCreate => feature.metadata.on_create_command.as_ref(),
            LifecyclePhase::UpdateContent => feature.metadata.update_content_command.as_ref(),
            LifecyclePhase::PostCreate => feature.metadata.post_create_command.as_ref(),
            LifecyclePhase::Dotfiles => None, // Dotfiles phase has no corresponding command field
            LifecyclePhase::PostStart => feature.metadata.post_start_command.as_ref(),
            LifecyclePhase::PostAttach => feature.metadata.post_attach_command.as_ref(),
        };

        if let Some(cmd) = cmd_opt {
            if let Some(parsed) = LifecycleCommandValue::from_json_value(cmd)? {
                if !parsed.is_empty() {
                    commands.push(AggregatedLifecycleCommand {
                        command: parsed,
                        source: LifecycleCommandSource::Feature {
                            id: feature.id.clone(),
                        },
                    });
                }
            }
        }
    }

    // Config command last
    let config_cmd_opt = match phase {
        LifecyclePhase::Initialize => config.initialize_command.as_ref(),
        LifecyclePhase::OnCreate => config.on_create_command.as_ref(),
        LifecyclePhase::UpdateContent => config.update_content_command.as_ref(),
        LifecyclePhase::PostCreate => config.post_create_command.as_ref(),
        LifecyclePhase::Dotfiles => None, // Dotfiles phase has no corresponding command field
        LifecyclePhase::PostStart => config.post_start_command.as_ref(),
        LifecyclePhase::PostAttach => config.post_attach_command.as_ref(),
    };

    if let Some(cmd) = config_cmd_opt {
        if let Some(parsed) = LifecycleCommandValue::from_json_value(cmd)? {
            if !parsed.is_empty() {
                commands.push(AggregatedLifecycleCommand {
                    command: parsed,
                    source: LifecycleCommandSource::Config,
                });
            }
        }
    }

    Ok(LifecycleCommandList { commands })
}

/// A lifecycle command ready for execution with source tracking.
///
/// Combines a lifecycle command (which can be a string, array, or object per the
/// devcontainer spec) with its source attribution for error reporting and debugging.
#[derive(Debug, Clone)]
pub struct AggregatedLifecycleCommand {
    /// The typed command to execute (Shell, Exec, or Parallel)
    pub command: LifecycleCommandValue,
    /// Where this command came from
    pub source: LifecycleCommandSource,
}

/// Ordered list of lifecycle commands for a specific phase.
///
/// Feature commands come first (in installation order), then config command.
/// Empty/null commands are filtered out during aggregation.
#[derive(Debug, Clone, Default)]
pub struct LifecycleCommandList {
    /// Commands in execution order
    pub commands: Vec<AggregatedLifecycleCommand>,
}

impl LifecycleCommandList {
    /// Create a new empty command list
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the command list is empty
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Get the number of commands in the list
    pub fn len(&self) -> usize {
        self.commands.len()
    }
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
    if let Some(ref initialize_commands) = commands.initialize {
        info!("Executing initialize phase on host");
        result.phases.push(
            execute_host_lifecycle_phase(
                LifecyclePhase::Initialize,
                &initialize_commands.commands,
                substitution_context,
                progress_callback.as_ref(),
            )
            .await?,
        );
    }

    // Derive workspace folder from substitution context for marker persistence
    let workspace_folder = std::path::PathBuf::from(&container_context.local_workspace_folder);

    // Execute onCreate phase
    if let Some(ref on_create_commands) = commands.on_create {
        let phase_result = execute_lifecycle_phase_impl(
            LifecyclePhase::OnCreate,
            &on_create_commands.commands,
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
    if let Some(ref update_content_commands) = commands.update_content {
        let phase_result = execute_lifecycle_phase_impl(
            LifecyclePhase::UpdateContent,
            &update_content_commands.commands,
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
        if let Some(ref post_create_commands) = commands.post_create {
            let phase_result = execute_lifecycle_phase_impl(
                LifecyclePhase::PostCreate,
                &post_create_commands.commands,
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
        if let Some(ref post_start_commands) = commands.post_start {
            result.non_blocking_phases.push(NonBlockingPhaseSpec {
                phase: LifecyclePhase::PostStart,
                commands: post_start_commands.commands.clone(),
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
        if let Some(ref post_attach_commands) = commands.post_attach {
            result.non_blocking_phases.push(NonBlockingPhaseSpec {
                phase: LifecyclePhase::PostAttach,
                commands: post_attach_commands.commands.clone(),
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
    commands: &[AggregatedLifecycleCommand],
    context: &SubstitutionContext,
    progress_callback: Option<&F>,
) -> Result<PhaseResult>
where
    F: Fn(ProgressEvent) -> anyhow::Result<()>,
{
    info!("Executing host lifecycle phase: {}", phase.as_str());
    let phase_start = Instant::now();

    // Convert aggregated commands to string vec for progress event
    let command_strings: Vec<String> = commands
        .iter()
        .map(|agg| match &agg.command {
            LifecycleCommandValue::Shell(s) => s.clone(),
            LifecycleCommandValue::Exec(args) => args.join(" "),
            LifecycleCommandValue::Parallel(map) => {
                let keys: Vec<&str> = map.keys().map(|k| k.as_str()).collect();
                format!("parallel: {}", keys.join(", "))
            }
        })
        .collect();

    // Emit phase begin event
    if let Some(callback) = progress_callback {
        let event = ProgressEvent::LifecyclePhaseBegin {
            id: ProgressTracker::next_event_id(),
            timestamp: ProgressTracker::current_timestamp(),
            phase: phase.as_str().to_string(),
            commands: command_strings,
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

    // Execute each command individually, handling all format variants inline
    for agg_cmd in commands {
        match &agg_cmd.command {
            LifecycleCommandValue::Shell(command_template) => {
                // Apply variable substitution to the command
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

                let display_cmd = substituted_command.clone();
                let working_dir = context.local_workspace_folder.clone();
                let env_vars = context.local_env.clone();

                let start_time = Instant::now();

                let output = tokio::task::spawn_blocking(move || {
                    std::process::Command::new("sh")
                        .args(["-c", &substituted_command])
                        .current_dir(&working_dir)
                        .envs(&env_vars)
                        .output()
                })
                .await;

                let duration = start_time.elapsed();

                match output {
                    Ok(Ok(output)) => {
                        let exit_code = output.status.code().unwrap_or(-1);
                        let command_result = CommandResult {
                            command: display_cmd.clone(),
                            exit_code,
                            duration,
                            success: output.status.success(),
                            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                        };
                        if !command_result.success {
                            phase_result.success = false;
                            error!(
                                "Host lifecycle command failed in phase {} with exit code {}: {}",
                                phase.as_str(),
                                exit_code,
                                display_cmd
                            );
                        }
                        phase_result.commands.push(command_result);
                    }
                    Ok(Err(e)) => {
                        error!("Failed to execute host shell command: {}", e);
                        phase_result.success = false;
                        phase_result.commands.push(CommandResult {
                            command: display_cmd,
                            exit_code: -1,
                            duration,
                            success: false,
                            stdout: String::new(),
                            stderr: e.to_string(),
                        });
                    }
                    Err(e) => {
                        error!("Failed to spawn host shell command: {}", e);
                        phase_result.success = false;
                        return Err(DeaconError::Lifecycle(format!(
                            "Failed to spawn host shell command: {}",
                            e
                        )));
                    }
                }
            }
            LifecycleCommandValue::Exec(_) => {
                // Apply variable substitution element-wise
                let substituted = agg_cmd.command.substitute_variables(context);
                let substituted_args = match &substituted {
                    LifecycleCommandValue::Exec(a) => a.clone(),
                    _ => {
                        return Err(DeaconError::Lifecycle(format!(
                            "Internal error: variable substitution changed command variant for phase '{}'",
                            phase.as_str()
                        )));
                    }
                };

                if substituted_args.is_empty() {
                    debug!("Skipping empty exec-style host command");
                    continue;
                }

                let display_cmd = substituted_args.join(" ");
                debug!("Host exec-style command: {:?}", substituted_args);

                let start_time = Instant::now();
                let program = substituted_args[0].clone();
                let cmd_args: Vec<String> = substituted_args[1..].to_vec();
                let working_dir = context.local_workspace_folder.clone();
                let env_vars = context.local_env.clone();

                let output = tokio::task::spawn_blocking(move || {
                    std::process::Command::new(&program)
                        .args(&cmd_args)
                        .current_dir(&working_dir)
                        .envs(&env_vars)
                        .output()
                })
                .await;

                let duration = start_time.elapsed();

                match output {
                    Ok(Ok(output)) => {
                        let exit_code = output.status.code().unwrap_or(-1);
                        let command_result = CommandResult {
                            command: display_cmd.clone(),
                            exit_code,
                            duration,
                            success: output.status.success(),
                            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                        };
                        if !command_result.success {
                            phase_result.success = false;
                            error!(
                                "Host exec-style command failed in phase {} with exit code {}: {}",
                                phase.as_str(),
                                exit_code,
                                display_cmd
                            );
                        }
                        phase_result.commands.push(command_result);
                    }
                    Ok(Err(e)) => {
                        error!("Failed to execute host exec-style command: {}", e);
                        phase_result.success = false;
                        phase_result.commands.push(CommandResult {
                            command: display_cmd,
                            exit_code: -1,
                            duration,
                            success: false,
                            stdout: String::new(),
                            stderr: e.to_string(),
                        });
                    }
                    Err(e) => {
                        error!("Failed to spawn host exec-style command: {}", e);
                        phase_result.success = false;
                        return Err(DeaconError::Lifecycle(format!(
                            "Failed to spawn host exec-style command: {}",
                            e
                        )));
                    }
                }
            }
            LifecycleCommandValue::Parallel(_) => {
                // Apply variable substitution to all entries
                let substituted = agg_cmd.command.substitute_variables(context);
                let substituted_entries = match &substituted {
                    LifecycleCommandValue::Parallel(m) => m.clone(),
                    _ => {
                        return Err(DeaconError::Lifecycle(format!(
                            "Internal error: variable substitution changed command variant for phase '{}'",
                            phase.as_str()
                        )));
                    }
                };

                if substituted_entries.is_empty() {
                    debug!("Skipping empty parallel host command set");
                    continue;
                }

                debug!(
                    "Executing {} parallel host commands for phase {}",
                    substituted_entries.len(),
                    phase.as_str()
                );

                // Spawn all entries concurrently using JoinSet with spawn_blocking
                let mut join_set = tokio::task::JoinSet::new();
                for (key, value) in substituted_entries.clone() {
                    let working_dir = context.local_workspace_folder.clone();
                    let env_vars = context.local_env.clone();
                    join_set.spawn_blocking(move || {
                        let start = Instant::now();
                        let output = match &value {
                            LifecycleCommandValue::Shell(cmd) => std::process::Command::new("sh")
                                .args(["-c", cmd])
                                .current_dir(&working_dir)
                                .envs(&env_vars)
                                .output(),
                            LifecycleCommandValue::Exec(args) if !args.is_empty() => {
                                std::process::Command::new(&args[0])
                                    .args(&args[1..])
                                    .current_dir(&working_dir)
                                    .envs(&env_vars)
                                    .output()
                            }
                            _ => {
                                debug!("Skipping empty parallel exec-style host command '{}'", key);
                                return ParallelCommandResult {
                                    key,
                                    exit_code: 0,
                                    duration: start.elapsed(),
                                    success: true,
                                    stdout: String::new(),
                                    stderr: String::new(),
                                };
                            }
                        };
                        match output {
                            Ok(output) => ParallelCommandResult {
                                key,
                                exit_code: output.status.code().unwrap_or(-1),
                                duration: start.elapsed(),
                                success: output.status.success(),
                                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                            },
                            Err(e) => ParallelCommandResult {
                                key,
                                exit_code: -1,
                                duration: start.elapsed(),
                                success: false,
                                stdout: String::new(),
                                stderr: e.to_string(),
                            },
                        }
                    });
                }

                // Collect all results (wait for ALL - no early cancellation)
                let mut parallel_results = Vec::new();
                while let Some(join_result) = join_set.join_next().await {
                    match join_result {
                        Ok(result) => {
                            let display_cmd = match substituted_entries.get(&result.key) {
                                Some(LifecycleCommandValue::Shell(s)) => s.clone(),
                                Some(LifecycleCommandValue::Exec(a)) => a.join(" "),
                                _ => result.key.clone(),
                            };
                            let cmd_result = CommandResult {
                                command: format!("[{}] {}", result.key, display_cmd),
                                exit_code: result.exit_code,
                                duration: result.duration,
                                success: result.success,
                                stdout: result.stdout.clone(),
                                stderr: result.stderr.clone(),
                            };
                            if !cmd_result.success {
                                phase_result.success = false;
                            }
                            phase_result.commands.push(cmd_result);
                            parallel_results.push(result);
                        }
                        Err(e) => {
                            error!("Parallel host task panicked: {}", e);
                            phase_result.success = false;
                            phase_result.commands.push(CommandResult {
                                command: "parallel task panicked".to_string(),
                                exit_code: -1,
                                duration: Duration::default(),
                                success: false,
                                stdout: String::new(),
                                stderr: e.to_string(),
                            });
                        }
                    }
                }

                // Report all failures
                let failed: Vec<&ParallelCommandResult> =
                    parallel_results.iter().filter(|r| !r.success).collect();
                if !failed.is_empty() {
                    let failed_info: Vec<String> = failed
                        .iter()
                        .map(|r| format!("{} (exit {})", r.key, r.exit_code))
                        .collect();
                    error!(
                        "Parallel host commands failed in phase {}: {}",
                        phase.as_str(),
                        failed_info.join(", ")
                    );
                }
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
        "Completed host lifecycle phase: {} in {:?}",
        phase.as_str(),
        phase_result.total_duration
    );
    Ok(phase_result)
}

/// Execute a single lifecycle phase with source-attributed commands.
///
/// T027: This function supports fail-fast error handling with source attribution.
/// When a command fails, execution stops immediately and the error message includes
/// which feature or config provided the failing command.
///
/// # Arguments
///
/// * `phase` - The lifecycle phase to execute
/// * `commands` - Commands with source attribution (from features and config)
/// * `config` - Container lifecycle configuration
/// * `docker` - Docker client
/// * `context` - Variable substitution context
/// * `detected_shell` - Shell detected in the container
/// * `progress_callback` - Optional callback for progress events
pub async fn execute_lifecycle_phase_with_sources<D, F>(
    phase: LifecyclePhase,
    commands: &[AggregatedLifecycleCommand],
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
///
/// T027: Now accepts AggregatedLifecycleCommand to support source attribution in error messages
#[instrument(skip(commands, config, docker, context, progress_callback))]
async fn execute_lifecycle_phase_impl<D, F>(
    phase: LifecyclePhase,
    commands: &[AggregatedLifecycleCommand],
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

    // Convert aggregated commands to string vec for progress event
    let command_strings: Vec<String> = commands
        .iter()
        .map(|agg| match &agg.command {
            LifecycleCommandValue::Shell(s) => s.clone(),
            LifecycleCommandValue::Exec(args) => args.join(" "),
            LifecycleCommandValue::Parallel(map) => {
                let keys: Vec<&str> = map.keys().map(|k| k.as_str()).collect();
                format!("parallel: {}", keys.join(", "))
            }
        })
        .collect();

    // Emit phase begin event
    if let Some(callback) = progress_callback {
        let event = ProgressEvent::LifecyclePhaseBegin {
            id: ProgressTracker::next_event_id(),
            timestamp: ProgressTracker::current_timestamp(),
            phase: phase.as_str().to_string(),
            commands: command_strings.clone(),
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

    for (i, agg_cmd) in commands.iter().enumerate() {
        // Dispatch based on command format
        match &agg_cmd.command {
            LifecycleCommandValue::Shell(command_template) => {
                debug!(
                    "Executing command {} of {} for phase {} (source: {}): {}",
                    i + 1,
                    commands.len(),
                    phase.as_str(),
                    agg_cmd.source,
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
                let redaction_config = RedactionConfig::default();
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
                            stdout: String::new(),
                            stderr: String::new(),
                        };

                        phase_result.commands.push(command_result);

                        // T027: Fail-fast behavior - stop immediately on command failure with source attribution
                        if exec_result.exit_code != 0 {
                            phase_result.success = false;
                            let error_msg = format!(
                                "Lifecycle command failed (source: {}) in phase {} with exit code {}: {}",
                                agg_cmd.source,
                                phase.as_str(),
                                exec_result.exit_code,
                                substituted_command
                            );
                            error!("{}", error_msg);

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

                            return Err(DeaconError::Lifecycle(error_msg));
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
                                exit_code: None,
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
                            "Failed to execute container command (source: {}) in phase {}: {}",
                            agg_cmd.source,
                            phase.as_str(),
                            e
                        );
                        return Err(DeaconError::Lifecycle(format!(
                            "Failed to execute container command (source: {}) in phase {}: {}",
                            agg_cmd.source,
                            phase.as_str(),
                            e
                        )));
                    }
                }
            }
            LifecycleCommandValue::Exec(args) => {
                debug!(
                    "Executing exec-style command {} of {} for phase {} (source: {}): {:?}",
                    i + 1,
                    commands.len(),
                    phase.as_str(),
                    agg_cmd.source,
                    args
                );

                let command_id = format!("{}-{}", phase.as_str(), i + 1);
                let start_time = Instant::now();

                // Apply variable substitution element-wise
                let substituted = agg_cmd.command.substitute_variables(context);
                let substituted_args = match &substituted {
                    LifecycleCommandValue::Exec(a) => a.clone(),
                    _ => {
                        return Err(DeaconError::Lifecycle(format!(
                            "Internal error: variable substitution changed command variant for phase '{}'",
                            phase.as_str()
                        )));
                    }
                };

                if substituted_args.is_empty() {
                    debug!("Skipping empty exec-style command");
                    continue;
                }

                let display_cmd = substituted_args.join(" ");

                // Apply redaction
                let redaction_config = RedactionConfig::default();
                let redacted_command = redact_if_enabled(&display_cmd, &redaction_config);

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

                // Pass args directly to docker exec - NO shell wrapping
                let exec_result = docker
                    .exec(&config.container_id, &substituted_args, exec_config)
                    .await;

                let duration = start_time.elapsed();

                match exec_result {
                    Ok(exec_result) => {
                        debug!(
                            "Exec-style command completed with exit code: {} in {:?}",
                            exec_result.exit_code, duration
                        );

                        // Emit command end event
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
                            command: display_cmd,
                            exit_code: exec_result.exit_code,
                            duration,
                            success: exec_result.success,
                            stdout: String::new(),
                            stderr: String::new(),
                        };

                        phase_result.commands.push(command_result);

                        // Fail-fast: stop on non-zero exit code
                        if exec_result.exit_code != 0 {
                            phase_result.success = false;
                            let error_msg = format!(
                                "Exec-style lifecycle command failed (source: {}) in phase {} with exit code {}: {:?}",
                                agg_cmd.source,
                                phase.as_str(),
                                exec_result.exit_code,
                                substituted_args
                            );
                            error!("{}", error_msg);

                            phase_result.total_duration = phase_start.elapsed();
                            emit_phase_end_event(progress_callback, &phase_result);
                            return Err(DeaconError::Lifecycle(error_msg));
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
                                exit_code: None,
                            };
                            if let Err(emit_err) = callback(event) {
                                debug!("Failed to emit command end event: {}", emit_err);
                            }
                        }

                        phase_result.success = false;
                        phase_result.total_duration = phase_start.elapsed();
                        emit_phase_end_event(progress_callback, &phase_result);

                        error!(
                            "Failed to execute exec-style command (source: {}) in phase {}: {}",
                            agg_cmd.source,
                            phase.as_str(),
                            e
                        );
                        return Err(DeaconError::Lifecycle(format!(
                            "Failed to execute exec-style command (source: {}) in phase {}: {}",
                            agg_cmd.source,
                            phase.as_str(),
                            e
                        )));
                    }
                }
            }
            LifecycleCommandValue::Parallel(entries) => {
                debug!(
                    "Executing parallel command set for phase {} (source: {}) with {} entries",
                    phase.as_str(),
                    agg_cmd.source,
                    entries.len()
                );

                if entries.is_empty() {
                    debug!("Skipping empty parallel command set");
                    continue;
                }

                // Apply variable substitution to all entries
                let substituted = agg_cmd.command.substitute_variables(context);
                let substituted_entries = match &substituted {
                    LifecycleCommandValue::Parallel(m) => m.clone(),
                    _ => {
                        return Err(DeaconError::Lifecycle(format!(
                            "Internal error: variable substitution changed command variant for phase '{}'",
                            phase.as_str()
                        )));
                    }
                };

                // Emit per-entry begin events
                for key in substituted_entries.keys() {
                    let command_id = format!("{}-{}", phase.as_str(), key);
                    if let Some(callback) = progress_callback {
                        let display = match substituted_entries.get(key) {
                            Some(LifecycleCommandValue::Shell(s)) => s.clone(),
                            Some(LifecycleCommandValue::Exec(a)) => a.join(" "),
                            _ => String::new(),
                        };
                        let redaction_config = RedactionConfig::default();
                        let redacted = redact_if_enabled(&display, &redaction_config);
                        let event = ProgressEvent::LifecycleCommandBegin {
                            id: ProgressTracker::next_event_id(),
                            timestamp: ProgressTracker::current_timestamp(),
                            phase: phase.as_str().to_string(),
                            command_id,
                            command: redacted,
                        };
                        if let Err(e) = callback(event) {
                            debug!("Failed to emit command begin event: {}", e);
                        }
                    }
                }

                // Build futures for all entries - they all borrow docker concurrently
                let futures: Vec<_> = substituted_entries
                    .iter()
                    .map(|(key, value)| {
                        let key = key.clone();
                        let command_args = match value {
                            LifecycleCommandValue::Shell(cmd) => {
                                if config.use_login_shell {
                                    let shell = detected_shell.unwrap_or("sh");
                                    crate::container_env_probe::get_shell_command_for_lifecycle(
                                        shell, cmd, true,
                                    )
                                } else {
                                    vec!["sh".to_string(), "-c".to_string(), cmd.clone()]
                                }
                            }
                            LifecycleCommandValue::Exec(args) => args.clone(),
                            LifecycleCommandValue::Parallel(_) => {
                                // Nested parallel not supported
                                vec![]
                            }
                        };
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
                        let container_id = &config.container_id;
                        async move {
                            let start_time = Instant::now();
                            if command_args.is_empty() {
                                debug!(
                                    "Skipping empty parallel exec-style container command '{}'",
                                    key
                                );
                                return ParallelCommandResult {
                                    key,
                                    exit_code: 0,
                                    duration: start_time.elapsed(),
                                    success: true,
                                    stdout: String::new(),
                                    stderr: String::new(),
                                };
                            }
                            match docker.exec(container_id, &command_args, exec_config).await {
                                Ok(result) => ParallelCommandResult {
                                    key,
                                    exit_code: result.exit_code,
                                    duration: start_time.elapsed(),
                                    success: result.success,
                                    stdout: result.stdout,
                                    stderr: result.stderr,
                                },
                                Err(e) => {
                                    error!(
                                        "[{}] Parallel command failed in phase {}: {}",
                                        key,
                                        phase.as_str(),
                                        e
                                    );
                                    ParallelCommandResult {
                                        key,
                                        exit_code: -1,
                                        duration: start_time.elapsed(),
                                        success: false,
                                        stdout: String::new(),
                                        stderr: e.to_string(),
                                    }
                                }
                            }
                        }
                    })
                    .collect();

                // Execute all concurrently and wait for ALL (no cancellation per Decision 8)
                let results = futures::future::join_all(futures).await;

                // Emit per-entry end events and collect results
                for result in &results {
                    let command_id = format!("{}-{}", phase.as_str(), result.key);
                    if let Some(callback) = progress_callback {
                        let event = ProgressEvent::LifecycleCommandEnd {
                            id: ProgressTracker::next_event_id(),
                            timestamp: ProgressTracker::current_timestamp(),
                            phase: phase.as_str().to_string(),
                            command_id,
                            duration_ms: result.duration.as_millis() as u64,
                            success: result.success,
                            exit_code: Some(result.exit_code),
                        };
                        if let Err(e) = callback(event) {
                            debug!("Failed to emit command end event: {}", e);
                        }
                    }

                    let display_cmd = match substituted_entries.get(&result.key) {
                        Some(LifecycleCommandValue::Shell(s)) => s.clone(),
                        Some(LifecycleCommandValue::Exec(a)) => a.join(" "),
                        _ => result.key.clone(),
                    };
                    phase_result.commands.push(CommandResult {
                        command: format!("[{}] {}", result.key, display_cmd),
                        exit_code: result.exit_code,
                        duration: result.duration,
                        success: result.success,
                        stdout: result.stdout.clone(),
                        stderr: result.stderr.clone(),
                    });
                }

                // Check for failures - aggregate all failed keys
                let failed: Vec<&ParallelCommandResult> =
                    results.iter().filter(|r| !r.success).collect();
                if !failed.is_empty() {
                    phase_result.success = false;
                    let failed_info: Vec<String> = failed
                        .iter()
                        .map(|r| format!("{} (exit {})", r.key, r.exit_code))
                        .collect();
                    let error_msg = format!(
                        "Parallel lifecycle commands failed (source: {}) in phase {}: {}",
                        agg_cmd.source,
                        phase.as_str(),
                        failed_info.join(", ")
                    );
                    error!("{}", error_msg);
                    phase_result.total_duration = phase_start.elapsed();
                    emit_phase_end_event(progress_callback, &phase_result);
                    return Err(DeaconError::Lifecycle(error_msg));
                }
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
    pub initialize: Option<LifecycleCommandList>,
    /// Commands to run during onCreate phase
    pub on_create: Option<LifecycleCommandList>,
    /// Commands to run during updateContent phase
    pub update_content: Option<LifecycleCommandList>,
    /// Commands to run during postCreate phase
    pub post_create: Option<LifecycleCommandList>,
    /// Commands to run during postStart phase
    pub post_start: Option<LifecycleCommandList>,
    /// Commands to run during postAttach phase
    pub post_attach: Option<LifecycleCommandList>,
}

impl ContainerLifecycleCommands {
    /// Create new empty lifecycle commands
    pub fn new() -> Self {
        Self::default()
    }

    /// Set initialize commands (host-side)
    pub fn with_initialize(mut self, commands: LifecycleCommandList) -> Self {
        self.initialize = Some(commands);
        self
    }

    /// Set onCreate commands
    pub fn with_on_create(mut self, commands: LifecycleCommandList) -> Self {
        self.on_create = Some(commands);
        self
    }

    /// Set updateContent commands
    pub fn with_update_content(mut self, commands: LifecycleCommandList) -> Self {
        self.update_content = Some(commands);
        self
    }

    /// Set postCreate commands
    pub fn with_post_create(mut self, commands: LifecycleCommandList) -> Self {
        self.post_create = Some(commands);
        self
    }

    /// Set postStart commands
    pub fn with_post_start(mut self, commands: LifecycleCommandList) -> Self {
        self.post_start = Some(commands);
        self
    }

    /// Set postAttach commands
    pub fn with_post_attach(mut self, commands: LifecycleCommandList) -> Self {
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
    /// Commands to execute (aggregated with source attribution)
    pub commands: Vec<AggregatedLifecycleCommand>,
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
                    // Non-blocking phase failed - record as a failed phase result instead of
                    // adding to background_errors. This allows callers to see phase outcomes.
                    let error_msg =
                        format!("Non-blocking phase {} failed: {}", spec.phase.as_str(), e);
                    error!("{}", error_msg);

                    // Create a failed phase result to record the failure
                    let failed_result = PhaseResult {
                        phase: spec.phase,
                        commands: Vec::new(),
                        total_duration: std::time::Duration::default(),
                        success: false,
                    };
                    self.phases.push(failed_result);
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

    /// Helper to convert string commands to AggregatedLifecycleCommand for tests
    fn make_config_commands(cmds: &[&str]) -> Vec<AggregatedLifecycleCommand> {
        cmds.iter()
            .map(|cmd| AggregatedLifecycleCommand {
                command: LifecycleCommandValue::Shell(cmd.to_string()),
                source: LifecycleCommandSource::Config,
            })
            .collect()
    }

    /// Helper to create a LifecycleCommandList from string commands for tests
    fn make_shell_command_list(cmds: &[&str]) -> LifecycleCommandList {
        LifecycleCommandList {
            commands: make_config_commands(cmds),
        }
    }

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
            .with_initialize(make_shell_command_list(&["echo 'initialize'"]))
            .with_on_create(make_shell_command_list(&["echo 'onCreate'"]))
            .with_update_content(make_shell_command_list(&["echo 'updateContent'"]))
            .with_post_create(make_shell_command_list(&["echo 'postCreate'"]))
            .with_post_start(make_shell_command_list(&["echo 'postStart'"]))
            .with_post_attach(make_shell_command_list(&["echo 'postAttach'"]));

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
            .with_initialize(make_shell_command_list(&["echo 'Phase 1: initialize'"]))
            .with_on_create(make_shell_command_list(&["echo 'Phase 2: onCreate'"]))
            .with_update_content(make_shell_command_list(&["echo 'Phase 3: updateContent'"]))
            .with_post_create(make_shell_command_list(&["echo 'Phase 4: postCreate'"]))
            .with_post_start(make_shell_command_list(&["echo 'Phase 5: postStart'"]))
            .with_post_attach(make_shell_command_list(&["echo 'Phase 6: postAttach'"]));

        // Verify all phases are present
        assert_eq!(commands.initialize.as_ref().unwrap().len(), 1);
        assert_eq!(commands.on_create.as_ref().unwrap().len(), 1);
        assert_eq!(commands.update_content.as_ref().unwrap().len(), 1);
        assert_eq!(commands.post_create.as_ref().unwrap().len(), 1);
        assert_eq!(commands.post_start.as_ref().unwrap().len(), 1);
        assert_eq!(commands.post_attach.as_ref().unwrap().len(), 1);
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
            commands: make_config_commands(&["echo 'test'"]),
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
            commands: make_config_commands(&["echo 'test'"]),
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
            commands: make_config_commands(&["echo 'test'"]),
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
            commands: make_config_commands(&["echo 'test'"]),
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
            commands: make_config_commands(&["echo 'postStart'"]),
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
            commands: make_config_commands(&["echo 'postAttach'"]),
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
            commands: make_config_commands(&["exit 1"]),
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
            commands: make_config_commands(&["echo 'postStart'"]),
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
            commands: make_config_commands(&["echo 'postAttach'"]),
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
        let command = LifecycleCommandValue::Shell("npm install".to_string());

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
            command: LifecycleCommandValue::Shell("echo hello".to_string()),
            source: LifecycleCommandSource::Config,
        };
        assert_eq!(
            string_cmd.command,
            LifecycleCommandValue::Shell("echo hello".to_string())
        );

        // Test with array command
        let array_cmd = AggregatedLifecycleCommand {
            command: LifecycleCommandValue::Exec(vec![
                "npm".to_string(),
                "install".to_string(),
                "--verbose".to_string(),
            ]),
            source: LifecycleCommandSource::Feature {
                id: "node".to_string(),
            },
        };
        assert_eq!(
            array_cmd.command,
            LifecycleCommandValue::Exec(vec![
                "npm".to_string(),
                "install".to_string(),
                "--verbose".to_string(),
            ])
        );

        // Test with object command (parallel commands)
        let object_cmd = AggregatedLifecycleCommand {
            command: LifecycleCommandValue::Parallel(IndexMap::from([
                (
                    "npm".to_string(),
                    LifecycleCommandValue::Shell("npm install".to_string()),
                ),
                (
                    "build".to_string(),
                    LifecycleCommandValue::Shell("npm run build".to_string()),
                ),
            ])),
            source: LifecycleCommandSource::Feature {
                id: "node".to_string(),
            },
        };
        assert_eq!(
            object_cmd.command,
            LifecycleCommandValue::Parallel(IndexMap::from([
                (
                    "npm".to_string(),
                    LifecycleCommandValue::Shell("npm install".to_string()),
                ),
                (
                    "build".to_string(),
                    LifecycleCommandValue::Shell("npm run build".to_string()),
                ),
            ]))
        );
    }

    #[test]
    fn test_aggregated_lifecycle_command_clone() {
        let original = AggregatedLifecycleCommand {
            command: LifecycleCommandValue::Exec(vec!["test".to_string(), "command".to_string()]),
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
            command: LifecycleCommandValue::Shell("test".to_string()),
            source: LifecycleCommandSource::Config,
        };

        let debug_str = format!("{:?}", cmd);
        assert!(debug_str.contains("AggregatedLifecycleCommand"));
        assert!(debug_str.contains("command"));
        assert!(debug_str.contains("source"));
    }

    #[test]
    fn test_is_empty_command_null() {
        use serde_json::json;
        // Null value is empty
        assert!(super::is_empty_command(&json!(null)));
    }

    #[test]
    fn test_is_empty_command_empty_string() {
        use serde_json::json;
        // Empty string is empty
        assert!(super::is_empty_command(&json!("")));
    }

    #[test]
    fn test_is_empty_command_empty_array() {
        use serde_json::json;
        // Empty array is empty
        assert!(super::is_empty_command(&json!([])));
    }

    #[test]
    fn test_is_empty_command_empty_object() {
        use serde_json::json;
        // Empty object is empty
        assert!(super::is_empty_command(&json!({})));
    }

    #[test]
    fn test_is_empty_command_non_empty_string() {
        use serde_json::json;
        // Non-empty strings are not empty
        assert!(!super::is_empty_command(&json!("npm install")));
        assert!(!super::is_empty_command(&json!("echo hello")));
        assert!(!super::is_empty_command(&json!(" "))); // Single space is not empty
    }

    #[test]
    fn test_is_empty_command_non_empty_array() {
        use serde_json::json;
        // Non-empty arrays are not empty
        assert!(!super::is_empty_command(&json!(["npm", "install"])));
        assert!(!super::is_empty_command(&json!(["single_element"])));
        assert!(!super::is_empty_command(&json!([
            "cmd", "arg1", "arg2", "arg3"
        ])));
    }

    #[test]
    fn test_is_empty_command_non_empty_object() {
        use serde_json::json;
        // Non-empty objects are not empty
        assert!(!super::is_empty_command(&json!({"build": "npm run build"})));
        assert!(!super::is_empty_command(&json!({
            "npm": "npm install",
            "build": "npm run build"
        })));
    }

    #[test]
    fn test_is_empty_command_other_json_types() {
        use serde_json::json;
        // Numbers, booleans are not considered empty (though unusual for commands)
        assert!(!super::is_empty_command(&json!(0)));
        assert!(!super::is_empty_command(&json!(42)));
        assert!(!super::is_empty_command(&json!(true)));
        assert!(!super::is_empty_command(&json!(false)));
    }

    #[test]
    fn test_is_empty_command_whitespace_only_strings() {
        use serde_json::json;
        // Whitespace-only strings are NOT considered empty per the contract
        // The contract only specifies empty string "", not trimmed whitespace
        assert!(!super::is_empty_command(&json!(" ")));
        assert!(!super::is_empty_command(&json!("  ")));
        assert!(!super::is_empty_command(&json!("\n")));
        assert!(!super::is_empty_command(&json!("\t")));
    }

    #[test]
    fn test_is_empty_command_nested_empty_structures() {
        use serde_json::json;
        // Nested structures with content are not empty
        assert!(!super::is_empty_command(&json!([[]])));
        assert!(!super::is_empty_command(&json!([{}])));
        assert!(!super::is_empty_command(&json!({"nested": {}})));
        assert!(!super::is_empty_command(&json!({"nested": []})));
    }

    // ============================================================================
    // Tests for LifecycleCommandValue::from_json_value()

    #[test]
    fn test_lifecycle_command_value_from_string() {
        let val = serde_json::json!("npm install");
        let result = LifecycleCommandValue::from_json_value(&val).unwrap();
        assert_eq!(
            result,
            Some(LifecycleCommandValue::Shell("npm install".to_string()))
        );
    }

    #[test]
    fn test_lifecycle_command_value_from_empty_string() {
        let val = serde_json::json!("");
        let result = LifecycleCommandValue::from_json_value(&val).unwrap();
        assert_eq!(result, Some(LifecycleCommandValue::Shell(String::new())));
    }

    #[test]
    fn test_lifecycle_command_value_from_array() {
        let val = serde_json::json!(["npm", "install"]);
        let result = LifecycleCommandValue::from_json_value(&val).unwrap();
        assert_eq!(
            result,
            Some(LifecycleCommandValue::Exec(vec![
                "npm".to_string(),
                "install".to_string()
            ]))
        );
    }

    #[test]
    fn test_lifecycle_command_value_from_empty_array() {
        let val = serde_json::json!([]);
        let result = LifecycleCommandValue::from_json_value(&val).unwrap();
        assert_eq!(result, Some(LifecycleCommandValue::Exec(vec![])));
    }

    #[test]
    fn test_lifecycle_command_value_from_array_non_string_error() {
        let val = serde_json::json!(["npm", 42]);
        let result = LifecycleCommandValue::from_json_value(&val);
        assert!(result.is_err());
    }

    #[test]
    fn test_lifecycle_command_value_from_object() {
        let val = serde_json::json!({"install": "npm install", "build": ["npm", "run", "build"]});
        let result = LifecycleCommandValue::from_json_value(&val)
            .unwrap()
            .unwrap();
        match result {
            LifecycleCommandValue::Parallel(map) => {
                assert_eq!(map.len(), 2);
                assert_eq!(
                    map.get("install"),
                    Some(&LifecycleCommandValue::Shell("npm install".to_string()))
                );
                assert_eq!(
                    map.get("build"),
                    Some(&LifecycleCommandValue::Exec(vec![
                        "npm".to_string(),
                        "run".to_string(),
                        "build".to_string()
                    ]))
                );
            }
            _ => panic!("Expected Parallel variant"),
        }
    }

    #[test]
    fn test_lifecycle_command_value_from_empty_object() {
        let val = serde_json::json!({});
        let result = LifecycleCommandValue::from_json_value(&val).unwrap();
        assert_eq!(
            result,
            Some(LifecycleCommandValue::Parallel(IndexMap::new()))
        );
    }

    #[test]
    fn test_lifecycle_command_value_from_null() {
        let val = serde_json::json!(null);
        let result = LifecycleCommandValue::from_json_value(&val).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_lifecycle_command_value_from_number_error() {
        let val = serde_json::json!(42);
        assert!(LifecycleCommandValue::from_json_value(&val).is_err());
    }

    #[test]
    fn test_lifecycle_command_value_from_boolean_error() {
        let val = serde_json::json!(true);
        assert!(LifecycleCommandValue::from_json_value(&val).is_err());
    }

    #[test]
    fn test_lifecycle_command_value_is_empty() {
        assert!(LifecycleCommandValue::Shell(String::new()).is_empty());
        assert!(!LifecycleCommandValue::Shell("cmd".to_string()).is_empty());
        assert!(LifecycleCommandValue::Exec(vec![]).is_empty());
        assert!(!LifecycleCommandValue::Exec(vec!["cmd".to_string()]).is_empty());
        assert!(LifecycleCommandValue::Parallel(IndexMap::new()).is_empty());
    }

    #[test]
    fn test_lifecycle_command_value_object_skips_invalid_values() {
        // Object with a number value should skip that entry with a log, not error
        let val = serde_json::json!({"install": "npm install", "bad": 42});
        let result = LifecycleCommandValue::from_json_value(&val)
            .unwrap()
            .unwrap();
        match result {
            LifecycleCommandValue::Parallel(map) => {
                assert_eq!(map.len(), 1);
                assert!(map.contains_key("install"));
            }
            _ => panic!("Expected Parallel variant"),
        }
    }

    // Tests for aggregate_lifecycle_commands()
    // ============================================================================

    #[test]
    fn test_aggregate_lifecycle_commands_basic_ordering() {
        use crate::config::DevContainerConfig;
        use crate::features::{FeatureMetadata, ResolvedFeature};
        use crate::lifecycle::LifecyclePhase;
        use serde_json::json;
        use std::collections::HashMap;

        // Create two features in installation order
        let feature1 = ResolvedFeature {
            id: "node".to_string(),
            source: "ghcr.io/devcontainers/features/node".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "node".to_string(),
                on_create_command: Some(json!("npm install")),
                ..Default::default()
            },
        };

        let feature2 = ResolvedFeature {
            id: "python".to_string(),
            source: "ghcr.io/devcontainers/features/python".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "python".to_string(),
                on_create_command: Some(json!("pip install -r requirements.txt")),
                ..Default::default()
            },
        };

        // Create config with onCreate command
        let config = DevContainerConfig {
            on_create_command: Some(json!("echo ready")),
            ..Default::default()
        };

        let features = vec![feature1, feature2];

        // Aggregate onCreate commands
        let commands =
            super::aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config)
                .unwrap()
                .commands;

        // Verify ordering: feature1, feature2, config
        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0].command, json!("npm install"));
        assert_eq!(
            commands[0].source,
            LifecycleCommandSource::Feature {
                id: "node".to_string()
            }
        );
        assert_eq!(
            commands[1].command,
            json!("pip install -r requirements.txt")
        );
        assert_eq!(
            commands[1].source,
            LifecycleCommandSource::Feature {
                id: "python".to_string()
            }
        );
        assert_eq!(commands[2].command, json!("echo ready"));
        assert_eq!(commands[2].source, LifecycleCommandSource::Config);
    }

    #[test]
    fn test_aggregate_lifecycle_commands_empty_filtering() {
        use crate::config::DevContainerConfig;
        use crate::features::{FeatureMetadata, ResolvedFeature};
        use crate::lifecycle::LifecyclePhase;
        use serde_json::json;
        use std::collections::HashMap;

        // Feature with null onCreate command
        let feature1 = ResolvedFeature {
            id: "node".to_string(),
            source: "ghcr.io/devcontainers/features/node".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "node".to_string(),
                on_create_command: None,
                ..Default::default()
            },
        };

        // Feature with valid onCreate command
        let feature2 = ResolvedFeature {
            id: "python".to_string(),
            source: "ghcr.io/devcontainers/features/python".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "python".to_string(),
                on_create_command: Some(json!("pip install")),
                ..Default::default()
            },
        };

        // Config with empty string onCreate command
        let config = DevContainerConfig {
            on_create_command: Some(json!("")),
            ..Default::default()
        };

        let features = vec![feature1, feature2];

        // Aggregate onCreate commands
        let commands =
            super::aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config)
                .unwrap()
                .commands;

        // Only python feature command should be included
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command, json!("pip install"));
        assert_eq!(
            commands[0].source,
            LifecycleCommandSource::Feature {
                id: "python".to_string()
            }
        );
    }

    #[test]
    fn test_aggregate_lifecycle_commands_all_empty() {
        use crate::config::DevContainerConfig;
        use crate::features::{FeatureMetadata, ResolvedFeature};
        use crate::lifecycle::LifecyclePhase;
        use serde_json::json;
        use std::collections::HashMap;

        // Feature with null onCreate command
        let feature1 = ResolvedFeature {
            id: "node".to_string(),
            source: "ghcr.io/devcontainers/features/node".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "node".to_string(),
                on_create_command: None,
                ..Default::default()
            },
        };

        // Feature with empty array onCreate command
        let feature2 = ResolvedFeature {
            id: "python".to_string(),
            source: "ghcr.io/devcontainers/features/python".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "python".to_string(),
                on_create_command: Some(json!([])),
                ..Default::default()
            },
        };

        // Config with empty object onCreate command
        let config = DevContainerConfig {
            on_create_command: Some(json!({})),
            ..Default::default()
        };

        let features = vec![feature1, feature2];

        // Aggregate onCreate commands
        let commands =
            super::aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config)
                .unwrap()
                .commands;

        // No commands should be included
        assert_eq!(commands.len(), 0);
    }

    #[test]
    fn test_aggregate_lifecycle_commands_no_features() {
        use crate::config::DevContainerConfig;
        use crate::lifecycle::LifecyclePhase;
        use serde_json::json;

        // Config with onCreate command
        let config = DevContainerConfig {
            on_create_command: Some(json!("echo hello")),
            ..Default::default()
        };

        let features = vec![];

        // Aggregate onCreate commands
        let commands =
            super::aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config)
                .unwrap()
                .commands;

        // Only config command should be included
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command, json!("echo hello"));
        assert_eq!(commands[0].source, LifecycleCommandSource::Config);
    }

    #[test]
    fn test_aggregate_lifecycle_commands_single_feature() {
        use crate::config::DevContainerConfig;
        use crate::features::{FeatureMetadata, ResolvedFeature};
        use crate::lifecycle::LifecyclePhase;
        use serde_json::json;
        use std::collections::HashMap;

        // Single feature with onCreate command
        let feature = ResolvedFeature {
            id: "node".to_string(),
            source: "ghcr.io/devcontainers/features/node".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "node".to_string(),
                on_create_command: Some(json!("npm install")),
                ..Default::default()
            },
        };

        // Config with no onCreate command
        let config = DevContainerConfig::default();

        let features = vec![feature];

        // Aggregate onCreate commands
        let commands =
            super::aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config)
                .unwrap()
                .commands;

        // Only feature command should be included
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command, json!("npm install"));
        assert_eq!(
            commands[0].source,
            LifecycleCommandSource::Feature {
                id: "node".to_string()
            }
        );
    }

    #[test]
    fn test_aggregate_lifecycle_commands_complex_formats() {
        use crate::config::DevContainerConfig;
        use crate::features::{FeatureMetadata, ResolvedFeature};
        use crate::lifecycle::LifecyclePhase;
        use serde_json::json;
        use std::collections::HashMap;

        // Feature with object command (parallel commands)
        let feature = ResolvedFeature {
            id: "node".to_string(),
            source: "ghcr.io/devcontainers/features/node".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "node".to_string(),
                on_create_command: Some(json!({
                    "npm": "npm install",
                    "build": "npm run build"
                })),
                ..Default::default()
            },
        };

        // Config with array command
        let config = DevContainerConfig {
            on_create_command: Some(json!(["./setup.sh", "--verbose"])),
            ..Default::default()
        };

        let features = vec![feature];

        // Aggregate onCreate commands
        let commands =
            super::aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config)
                .unwrap()
                .commands;

        // Both commands should be included with their complex formats
        assert_eq!(commands.len(), 2);
        assert_eq!(
            commands[0].command,
            json!({
                "npm": "npm install",
                "build": "npm run build"
            })
        );
        assert_eq!(
            commands[0].source,
            LifecycleCommandSource::Feature {
                id: "node".to_string()
            }
        );
        assert_eq!(commands[1].command, json!(["./setup.sh", "--verbose"]));
        assert_eq!(commands[1].source, LifecycleCommandSource::Config);
    }

    #[test]
    fn test_aggregate_lifecycle_commands_all_phases() {
        use crate::config::DevContainerConfig;
        use crate::features::{FeatureMetadata, ResolvedFeature};
        use crate::lifecycle::LifecyclePhase;
        use serde_json::json;
        use std::collections::HashMap;

        // Feature with commands for all phases
        let feature = ResolvedFeature {
            id: "node".to_string(),
            source: "ghcr.io/devcontainers/features/node".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "node".to_string(),
                on_create_command: Some(json!("onCreate-feature")),
                update_content_command: Some(json!("updateContent-feature")),
                post_create_command: Some(json!("postCreate-feature")),
                post_start_command: Some(json!("postStart-feature")),
                post_attach_command: Some(json!("postAttach-feature")),
                ..Default::default()
            },
        };

        // Config with commands for all phases
        let config = DevContainerConfig {
            on_create_command: Some(json!("onCreate-config")),
            update_content_command: Some(json!("updateContent-config")),
            post_create_command: Some(json!("postCreate-config")),
            post_start_command: Some(json!("postStart-config")),
            post_attach_command: Some(json!("postAttach-config")),
            ..Default::default()
        };

        let features = vec![feature];

        // Test OnCreate phase
        let on_create_commands =
            super::aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config)
                .unwrap()
                .commands;
        assert_eq!(on_create_commands.len(), 2);
        assert_eq!(on_create_commands[0].command, json!("onCreate-feature"));
        assert_eq!(on_create_commands[1].command, json!("onCreate-config"));

        // Test UpdateContent phase
        let update_content_commands =
            super::aggregate_lifecycle_commands(LifecyclePhase::UpdateContent, &features, &config)
                .unwrap()
                .commands;
        assert_eq!(update_content_commands.len(), 2);
        assert_eq!(
            update_content_commands[0].command,
            json!("updateContent-feature")
        );
        assert_eq!(
            update_content_commands[1].command,
            json!("updateContent-config")
        );

        // Test PostCreate phase
        let post_create_commands =
            super::aggregate_lifecycle_commands(LifecyclePhase::PostCreate, &features, &config)
                .unwrap()
                .commands;
        assert_eq!(post_create_commands.len(), 2);
        assert_eq!(post_create_commands[0].command, json!("postCreate-feature"));
        assert_eq!(post_create_commands[1].command, json!("postCreate-config"));

        // Test PostStart phase
        let post_start_commands =
            super::aggregate_lifecycle_commands(LifecyclePhase::PostStart, &features, &config)
                .unwrap()
                .commands;
        assert_eq!(post_start_commands.len(), 2);
        assert_eq!(post_start_commands[0].command, json!("postStart-feature"));
        assert_eq!(post_start_commands[1].command, json!("postStart-config"));

        // Test PostAttach phase
        let post_attach_commands =
            super::aggregate_lifecycle_commands(LifecyclePhase::PostAttach, &features, &config)
                .unwrap()
                .commands;
        assert_eq!(post_attach_commands.len(), 2);
        assert_eq!(post_attach_commands[0].command, json!("postAttach-feature"));
        assert_eq!(post_attach_commands[1].command, json!("postAttach-config"));
    }

    #[test]
    fn test_aggregate_lifecycle_commands_initialize_phase() {
        use crate::config::DevContainerConfig;
        use crate::features::{FeatureMetadata, ResolvedFeature};
        use crate::lifecycle::LifecyclePhase;
        use serde_json::json;
        use std::collections::HashMap;

        // Feature (features don't have initialize commands)
        let feature = ResolvedFeature {
            id: "node".to_string(),
            source: "ghcr.io/devcontainers/features/node".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "node".to_string(),
                on_create_command: Some(json!("npm install")),
                ..Default::default()
            },
        };

        // Config with initialize command
        let config = DevContainerConfig {
            initialize_command: Some(json!("initialize-config")),
            ..Default::default()
        };

        let features = vec![feature];

        // Test Initialize phase
        let initialize_commands =
            super::aggregate_lifecycle_commands(LifecyclePhase::Initialize, &features, &config)
                .unwrap()
                .commands;

        // Only config command should be included (features don't support initialize)
        assert_eq!(initialize_commands.len(), 1);
        assert_eq!(initialize_commands[0].command, json!("initialize-config"));
        assert_eq!(
            initialize_commands[0].source,
            LifecycleCommandSource::Config
        );
    }

    #[test]
    fn test_aggregate_lifecycle_commands_dotfiles_phase() {
        use crate::config::DevContainerConfig;
        use crate::features::{FeatureMetadata, ResolvedFeature};
        use crate::lifecycle::LifecyclePhase;
        use serde_json::json;
        use std::collections::HashMap;

        // Feature with onCreate command
        let feature = ResolvedFeature {
            id: "node".to_string(),
            source: "ghcr.io/devcontainers/features/node".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "node".to_string(),
                on_create_command: Some(json!("npm install")),
                ..Default::default()
            },
        };

        // Config with onCreate command
        let config = DevContainerConfig {
            on_create_command: Some(json!("echo ready")),
            ..Default::default()
        };

        let features = vec![feature];

        // Test Dotfiles phase (no corresponding command field)
        let dotfiles_commands =
            super::aggregate_lifecycle_commands(LifecyclePhase::Dotfiles, &features, &config)
                .unwrap()
                .commands;

        // No commands should be returned for Dotfiles phase
        assert_eq!(dotfiles_commands.len(), 0);
    }

    #[test]
    fn test_aggregate_lifecycle_commands_preserves_installation_order() {
        use crate::config::DevContainerConfig;
        use crate::features::{FeatureMetadata, ResolvedFeature};
        use crate::lifecycle::LifecyclePhase;
        use serde_json::json;
        use std::collections::HashMap;

        // Create multiple features to test ordering
        let feature1 = ResolvedFeature {
            id: "first".to_string(),
            source: "first-source".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "first".to_string(),
                on_create_command: Some(json!("first-command")),
                ..Default::default()
            },
        };

        let feature2 = ResolvedFeature {
            id: "second".to_string(),
            source: "second-source".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "second".to_string(),
                on_create_command: Some(json!("second-command")),
                ..Default::default()
            },
        };

        let feature3 = ResolvedFeature {
            id: "third".to_string(),
            source: "third-source".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "third".to_string(),
                on_create_command: Some(json!("third-command")),
                ..Default::default()
            },
        };

        let config = DevContainerConfig {
            on_create_command: Some(json!("config-command")),
            ..Default::default()
        };

        let features = vec![feature1, feature2, feature3];

        // Aggregate onCreate commands
        let commands =
            super::aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config)
                .unwrap()
                .commands;

        // Verify strict ordering: first, second, third, config
        assert_eq!(commands.len(), 4);
        assert_eq!(commands[0].command, json!("first-command"));
        assert_eq!(
            commands[0].source,
            LifecycleCommandSource::Feature {
                id: "first".to_string()
            }
        );
        assert_eq!(commands[1].command, json!("second-command"));
        assert_eq!(
            commands[1].source,
            LifecycleCommandSource::Feature {
                id: "second".to_string()
            }
        );
        assert_eq!(commands[2].command, json!("third-command"));
        assert_eq!(
            commands[2].source,
            LifecycleCommandSource::Feature {
                id: "third".to_string()
            }
        );
        assert_eq!(commands[3].command, json!("config-command"));
        assert_eq!(commands[3].source, LifecycleCommandSource::Config);
    }

    #[test]
    fn test_aggregate_lifecycle_commands_mixed_empty_commands() {
        use crate::config::DevContainerConfig;
        use crate::features::{FeatureMetadata, ResolvedFeature};
        use crate::lifecycle::LifecyclePhase;
        use serde_json::json;
        use std::collections::HashMap;

        // Feature 1 with valid command
        let feature1 = ResolvedFeature {
            id: "feature1".to_string(),
            source: "source1".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "feature1".to_string(),
                on_create_command: Some(json!("command1")),
                ..Default::default()
            },
        };

        // Feature 2 with null command
        let feature2 = ResolvedFeature {
            id: "feature2".to_string(),
            source: "source2".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "feature2".to_string(),
                on_create_command: None,
                ..Default::default()
            },
        };

        // Feature 3 with empty string
        let feature3 = ResolvedFeature {
            id: "feature3".to_string(),
            source: "source3".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "feature3".to_string(),
                on_create_command: Some(json!("")),
                ..Default::default()
            },
        };

        // Feature 4 with valid command
        let feature4 = ResolvedFeature {
            id: "feature4".to_string(),
            source: "source4".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "feature4".to_string(),
                on_create_command: Some(json!("command4")),
                ..Default::default()
            },
        };

        // Feature 5 with empty array
        let feature5 = ResolvedFeature {
            id: "feature5".to_string(),
            source: "source5".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "feature5".to_string(),
                on_create_command: Some(json!([])),
                ..Default::default()
            },
        };

        let config = DevContainerConfig {
            on_create_command: Some(json!("config-command")),
            ..Default::default()
        };

        let features = vec![feature1, feature2, feature3, feature4, feature5];

        // Aggregate onCreate commands
        let commands =
            super::aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config)
                .unwrap()
                .commands;

        // Only feature1, feature4, and config should be included
        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0].command, json!("command1"));
        assert_eq!(
            commands[0].source,
            LifecycleCommandSource::Feature {
                id: "feature1".to_string()
            }
        );
        assert_eq!(commands[1].command, json!("command4"));
        assert_eq!(
            commands[1].source,
            LifecycleCommandSource::Feature {
                id: "feature4".to_string()
            }
        );
        assert_eq!(commands[2].command, json!("config-command"));
        assert_eq!(commands[2].source, LifecycleCommandSource::Config);
    }

    #[test]
    fn test_aggregate_lifecycle_commands_no_config_command() {
        use crate::config::DevContainerConfig;
        use crate::features::{FeatureMetadata, ResolvedFeature};
        use crate::lifecycle::LifecyclePhase;
        use serde_json::json;
        use std::collections::HashMap;

        // Features with onCreate commands
        let feature1 = ResolvedFeature {
            id: "feature1".to_string(),
            source: "source1".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "feature1".to_string(),
                on_create_command: Some(json!("command1")),
                ..Default::default()
            },
        };

        let feature2 = ResolvedFeature {
            id: "feature2".to_string(),
            source: "source2".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "feature2".to_string(),
                on_create_command: Some(json!("command2")),
                ..Default::default()
            },
        };

        // Config with no onCreate command
        let config = DevContainerConfig::default();

        let features = vec![feature1, feature2];

        // Aggregate onCreate commands
        let commands =
            super::aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config)
                .unwrap()
                .commands;

        // Only feature commands should be included
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].command, json!("command1"));
        assert_eq!(
            commands[0].source,
            LifecycleCommandSource::Feature {
                id: "feature1".to_string()
            }
        );
        assert_eq!(commands[1].command, json!("command2"));
        assert_eq!(
            commands[1].source,
            LifecycleCommandSource::Feature {
                id: "feature2".to_string()
            }
        );
    }

    #[test]
    fn test_aggregate_lifecycle_commands_whitespace_not_empty() {
        use crate::config::DevContainerConfig;
        use crate::features::{FeatureMetadata, ResolvedFeature};
        use crate::lifecycle::LifecyclePhase;
        use serde_json::json;
        use std::collections::HashMap;

        // Feature with whitespace command (should NOT be filtered)
        let feature = ResolvedFeature {
            id: "node".to_string(),
            source: "ghcr.io/devcontainers/features/node".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "node".to_string(),
                on_create_command: Some(json!(" ")),
                ..Default::default()
            },
        };

        let config = DevContainerConfig::default();

        let features = vec![feature];

        // Aggregate onCreate commands
        let commands =
            super::aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config)
                .unwrap()
                .commands;

        // Whitespace is not considered empty per the contract
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command, json!(" "));
    }

    // ===================================================================
    // T018: Unit tests for exec-style execution
    // ===================================================================

    #[test]
    fn test_exec_style_variable_substitution() {
        // Verify that substitute_variables preserves the Exec variant
        // and applies substitution element-wise
        let cmd = LifecycleCommandValue::Exec(vec![
            "echo".to_string(),
            "${localWorkspaceFolder}".to_string(),
        ]);
        assert!(matches!(cmd, LifecycleCommandValue::Exec(_)));
        assert!(!cmd.is_empty());

        // Apply substitution with a context
        let temp_dir = tempfile::TempDir::new().unwrap();
        let context = SubstitutionContext::new(temp_dir.path()).unwrap();
        let substituted = cmd.substitute_variables(&context);

        // Verify the variant is preserved
        match &substituted {
            LifecycleCommandValue::Exec(args) => {
                assert_eq!(args.len(), 2);
                assert_eq!(args[0], "echo");
                // The second element should have been substituted
                // (localWorkspaceFolder maps to the temp dir path)
                assert_ne!(args[1], "${localWorkspaceFolder}");
            }
            _ => panic!("Expected Exec variant after substitution"),
        }
    }

    #[test]
    fn test_exec_empty_args_is_noop() {
        let cmd = LifecycleCommandValue::Exec(vec![]);
        assert!(cmd.is_empty());
    }

    #[test]
    fn test_exec_single_element() {
        let cmd = LifecycleCommandValue::Exec(vec!["ls".to_string()]);
        assert!(!cmd.is_empty());
    }

    #[test]
    fn test_exec_args_with_spaces_preserved() {
        // Verify that args with spaces and metacharacters are preserved literally
        let cmd = LifecycleCommandValue::Exec(vec![
            "echo".to_string(),
            "hello world".to_string(),
            "foo && bar".to_string(),
            "$(whoami)".to_string(),
        ]);
        match &cmd {
            LifecycleCommandValue::Exec(args) => {
                assert_eq!(args.len(), 4);
                assert_eq!(args[0], "echo");
                assert_eq!(args[1], "hello world");
                assert_eq!(args[2], "foo && bar");
                assert_eq!(args[3], "$(whoami)");
            }
            _ => panic!("Expected Exec variant"),
        }
    }

    #[test]
    fn test_exec_no_shell_wrapping_in_args() {
        // Verify the Exec variant does not contain shell wrapper elements
        let cmd = LifecycleCommandValue::Exec(vec![
            "npm".to_string(),
            "install".to_string(),
            "--save-dev".to_string(),
        ]);
        match &cmd {
            LifecycleCommandValue::Exec(args) => {
                // Should NOT have "sh" or "-c" anywhere
                assert!(!args.contains(&"sh".to_string()));
                assert!(!args.contains(&"-c".to_string()));
                // First element is the executable
                assert_eq!(args[0], "npm");
            }
            _ => panic!("Expected Exec variant"),
        }
    }

    // ===================================================================
    // T023: Unit tests for parallel execution
    // ===================================================================

    #[test]
    fn test_parallel_command_result() {
        let result = ParallelCommandResult {
            key: "install".to_string(),
            exit_code: 0,
            duration: Duration::from_millis(100),
            success: true,
            stdout: String::new(),
            stderr: String::new(),
        };
        assert!(result.success);
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.key, "install");
        assert_eq!(result.duration, Duration::from_millis(100));
    }

    #[test]
    fn test_parallel_command_result_failure() {
        let result = ParallelCommandResult {
            key: "build".to_string(),
            exit_code: 1,
            duration: Duration::from_millis(500),
            success: false,
            stdout: String::new(),
            stderr: "build failed".to_string(),
        };
        assert!(!result.success);
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.key, "build");
        assert_eq!(result.stderr, "build failed");
    }

    #[test]
    fn test_parallel_empty_object_is_noop() {
        let cmd = LifecycleCommandValue::Parallel(IndexMap::new());
        assert!(cmd.is_empty());
    }

    #[test]
    fn test_parallel_mixed_format_values() {
        let mut map = IndexMap::new();
        map.insert(
            "shell".to_string(),
            LifecycleCommandValue::Shell("echo hello".to_string()),
        );
        map.insert(
            "exec".to_string(),
            LifecycleCommandValue::Exec(vec!["echo".to_string(), "world".to_string()]),
        );
        let cmd = LifecycleCommandValue::Parallel(map);
        assert!(!cmd.is_empty());
        match &cmd {
            LifecycleCommandValue::Parallel(m) => {
                assert_eq!(m.len(), 2);
                assert!(matches!(
                    m.get("shell"),
                    Some(LifecycleCommandValue::Shell(_))
                ));
                assert!(matches!(
                    m.get("exec"),
                    Some(LifecycleCommandValue::Exec(_))
                ));
            }
            _ => panic!("Expected Parallel"),
        }
    }

    #[test]
    fn test_parallel_variable_substitution() {
        // Verify that substitute_variables applies to all entries recursively
        let mut map = IndexMap::new();
        map.insert(
            "shell".to_string(),
            LifecycleCommandValue::Shell("echo ${localWorkspaceFolder}".to_string()),
        );
        map.insert(
            "exec".to_string(),
            LifecycleCommandValue::Exec(vec![
                "echo".to_string(),
                "${localWorkspaceFolder}".to_string(),
            ]),
        );
        let cmd = LifecycleCommandValue::Parallel(map);

        let temp_dir = tempfile::TempDir::new().unwrap();
        let context = SubstitutionContext::new(temp_dir.path()).unwrap();
        let substituted = cmd.substitute_variables(&context);

        match &substituted {
            LifecycleCommandValue::Parallel(m) => {
                assert_eq!(m.len(), 2);
                // Shell entry should have substitution applied
                if let Some(LifecycleCommandValue::Shell(s)) = m.get("shell") {
                    assert!(!s.contains("${localWorkspaceFolder}"));
                } else {
                    panic!("Expected Shell variant for 'shell' key");
                }
                // Exec entry should have substitution applied element-wise
                if let Some(LifecycleCommandValue::Exec(args)) = m.get("exec") {
                    assert_eq!(args.len(), 2);
                    assert_eq!(args[0], "echo");
                    assert!(!args[1].contains("${localWorkspaceFolder}"));
                } else {
                    panic!("Expected Exec variant for 'exec' key");
                }
            }
            _ => panic!("Expected Parallel variant after substitution"),
        }
    }

    #[test]
    fn test_parallel_preserves_key_order() {
        // Verify that IndexMap preserves insertion order
        let mut map = IndexMap::new();
        map.insert(
            "install".to_string(),
            LifecycleCommandValue::Shell("npm install".to_string()),
        );
        map.insert(
            "build".to_string(),
            LifecycleCommandValue::Shell("npm run build".to_string()),
        );
        map.insert(
            "test".to_string(),
            LifecycleCommandValue::Shell("npm test".to_string()),
        );
        let cmd = LifecycleCommandValue::Parallel(map);

        match &cmd {
            LifecycleCommandValue::Parallel(m) => {
                let keys: Vec<&String> = m.keys().collect();
                assert_eq!(keys, vec!["install", "build", "test"]);
            }
            _ => panic!("Expected Parallel"),
        }
    }

    #[test]
    fn test_parallel_from_json_mixed_formats() {
        use serde_json::json;
        // Parse a JSON object with mixed Shell and Exec values
        let json_val = json!({
            "install": "npm install",
            "build": ["npm", "run", "build"]
        });
        let parsed = LifecycleCommandValue::from_json_value(&json_val)
            .unwrap()
            .unwrap();
        match &parsed {
            LifecycleCommandValue::Parallel(m) => {
                assert_eq!(m.len(), 2);
                assert!(matches!(
                    m.get("install"),
                    Some(LifecycleCommandValue::Shell(_))
                ));
                assert!(matches!(
                    m.get("build"),
                    Some(LifecycleCommandValue::Exec(_))
                ));
            }
            _ => panic!("Expected Parallel variant from JSON object"),
        }
    }
}
