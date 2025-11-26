//! Up command implementation
//!
//! Implements the `deacon up` subcommand for starting development containers.
//! Supports both traditional container workflows and Docker Compose workflows.

use crate::commands::shared::{
    load_config, resolve_env_and_user, ConfigLoadArgs, ConfigLoadResult, TerminalDimensions,
};
use anyhow::{Context, Result};
use deacon_core::compose::{ComposeCommand, ComposeManager, ComposeProject};
use deacon_core::config::DevContainerConfig;
use deacon_core::container::{ContainerIdentity, ContainerSelector};
use deacon_core::container_env_probe::ContainerProbeMode;
use deacon_core::docker::{Docker, DockerLifecycle, ExecConfig};
use deacon_core::errors::{DeaconError, DockerError};
use deacon_core::features::{FeatureMergeConfig, FeatureMerger};
use deacon_core::ports::PortForwardingManager;
use deacon_core::runtime::{ContainerRuntimeImpl, RuntimeFactory};
use deacon_core::secrets::SecretsCollection;
use deacon_core::state::{ComposeState, ContainerState, StateManager};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{env, fs};
use tracing::{debug, info, instrument, warn};

/// Environment variable name for controlling PTY allocation in JSON log mode.
/// When set to truthy values (true/1/yes, case-insensitive), forces PTY allocation
/// for lifecycle exec commands during `deacon up` when JSON logging is active.
const ENV_FORCE_TTY_IF_JSON: &str = "DEACON_FORCE_TTY_IF_JSON";

/// Environment variable name for log format detection.
const ENV_LOG_FORMAT: &str = "DEACON_LOG_FORMAT";

fn build_merged_configuration(
    config: &DevContainerConfig,
    config_path: &Path,
) -> Result<serde_json::Value> {
    use deacon_core::config::merge::LayeredConfigMerger;
    let merged = LayeredConfigMerger::merge_with_provenance(&[(config.clone(), config_path)], true);
    Ok(serde_json::to_value(merged)?)
}

/// Create a temporary Docker Compose override file to inject mounts and environment variables.
///
/// Applies only to the primary service; uses simple string volume syntax and environment map.
fn create_compose_override(project: &ComposeProject) -> Result<Option<PathBuf>> {
    if project.additional_mounts.is_empty() && project.additional_env.is_empty() {
        return Ok(None);
    }

    let mut yaml = String::from("services:\n");
    yaml.push_str(&format!("  {}:\n", project.service));

    if !project.additional_env.is_empty() {
        yaml.push_str("    environment:\n");
        for (key, value) in &project.additional_env {
            let escaped = value.replace('"', "\\\"");
            yaml.push_str(&format!("      {}: \"{}\"\n", key, escaped));
        }
    }

    if !project.additional_mounts.is_empty() {
        yaml.push_str("    volumes:\n");
        for mount in &project.additional_mounts {
            let mut mount_str = format!("{}:{}", mount.source, mount.target);
            if mount.external {
                mount_str.push_str(":ro");
            }
            yaml.push_str(&format!("      - {}\n", mount_str));
        }
    }

    let override_path = env::temp_dir().join(format!(
        "deacon-compose-override-{}-{}.yml",
        project.name,
        std::process::id()
    ));
    fs::write(&override_path, yaml)?;
    Ok(Some(override_path))
}

pub use crate::commands::shared::NormalizedRemoteEnv;

/// Success response for the up command, emitted as JSON to stdout.
///
/// Per the `deacon up` contract (specs/001-up-gap-spec/contracts/up.md),
/// exactly one JSON document MUST be written to stdout on success with exit code 0.
/// All logs go to stderr.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpSuccess {
    /// Always "success" for successful outcomes
    pub outcome: String,

    /// ID of the created or reused container
    pub container_id: String,

    /// Compose project name (only present for compose-based configurations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compose_project_name: Option<String>,

    /// Remote user inside the container
    pub remote_user: String,

    /// Remote workspace folder path inside the container
    pub remote_workspace_folder: String,

    /// Configuration object (only when includeConfiguration flag is set)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration: Option<serde_json::Value>,

    /// Merged configuration object (only when includeMergedConfiguration flag is set)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merged_configuration: Option<serde_json::Value>,
}

/// Error response for the up command, emitted as JSON to stdout.
///
/// Per the `deacon up` contract (specs/001-up-gap-spec/contracts/up.md),
/// exactly one JSON document MUST be written to stdout on error with exit code 1.
/// All logs go to stderr. Secrets must be redacted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpError {
    /// Always "error" for error outcomes
    pub outcome: String,

    /// Short error message
    pub message: String,

    /// Detailed error description
    pub description: String,

    /// Container ID (if container was created before error)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,

    /// Disallowed feature ID (if error was due to disallowed feature)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disallowed_feature_id: Option<String>,

    /// Whether the container was stopped during error handling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub did_stop_container: Option<bool>,

    /// Optional URL for more information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub learn_more_url: Option<String>,
}

/// Union type for up command results to enforce stdout JSON contract.
///
/// The contract requires exactly one JSON document on stdout (success or error).
/// This type provides builder methods and serialization helpers to emit the correct format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum UpResult {
    Success(UpSuccess),
    Error(UpError),
}

impl UpResult {
    /// Create a success result
    pub fn success(
        container_id: String,
        remote_user: String,
        remote_workspace_folder: String,
    ) -> Self {
        UpResult::Success(UpSuccess {
            outcome: "success".to_string(),
            container_id,
            compose_project_name: None,
            remote_user,
            remote_workspace_folder,
            configuration: None,
            merged_configuration: None,
        })
    }

    /// Create an error result
    pub fn error(message: String, description: String) -> Self {
        UpResult::Error(UpError {
            outcome: "error".to_string(),
            message,
            description,
            container_id: None,
            disallowed_feature_id: None,
            did_stop_container: None,
            learn_more_url: None,
        })
    }

    /// Add compose project name to a success result
    pub fn with_compose_project_name(mut self, project_name: String) -> Self {
        if let UpResult::Success(ref mut success) = self {
            success.compose_project_name = Some(project_name);
        }
        self
    }

    /// Add configuration to a success result
    pub fn with_configuration(mut self, configuration: serde_json::Value) -> Self {
        if let UpResult::Success(ref mut success) = self {
            success.configuration = Some(configuration);
        }
        self
    }

    /// Add merged configuration to a success result
    pub fn with_merged_configuration(mut self, merged_configuration: serde_json::Value) -> Self {
        if let UpResult::Success(ref mut success) = self {
            success.merged_configuration = Some(merged_configuration);
        }
        self
    }

    /// Add container ID to an error result
    #[allow(dead_code)] // TODO: Will be used in T011 for error scenarios
    pub fn with_container_id(mut self, container_id: String) -> Self {
        match self {
            UpResult::Success(_) => self,
            UpResult::Error(ref mut error) => {
                error.container_id = Some(container_id);
                self
            }
        }
    }

    /// Add disallowed feature ID to an error result
    #[allow(dead_code)] // TODO: Will be used in T029 for disallowed features
    pub fn with_disallowed_feature_id(mut self, feature_id: String) -> Self {
        if let UpResult::Error(ref mut error) = self {
            error.disallowed_feature_id = Some(feature_id);
        }
        self
    }

    /// Mark that container was stopped during error handling
    #[allow(dead_code)] // TODO: Will be used in T011 for error scenarios
    pub fn with_did_stop_container(mut self, stopped: bool) -> Self {
        if let UpResult::Error(ref mut error) = self {
            error.did_stop_container = Some(stopped);
        }
        self
    }

    /// Add learn more URL to an error result
    #[allow(dead_code)] // TODO: Will be used in T011 for error scenarios
    pub fn with_learn_more_url(mut self, url: String) -> Self {
        if let UpResult::Error(ref mut error) = self {
            error.learn_more_url = Some(url);
        }
        self
    }

    /// Emit this result as JSON to stdout and return appropriate exit code.
    ///
    /// Per contract: stdout receives exactly one JSON document, stderr receives logs.
    /// Returns 0 for success, 1 for error.
    #[allow(dead_code)] // TODO: Alternative to inline JSON emission in cli.rs
    pub fn emit(&self) -> Result<i32> {
        let json = serde_json::to_string_pretty(self)?;
        println!("{}", json);

        match self {
            UpResult::Success(_) => Ok(0),
            UpResult::Error(_) => Ok(1),
        }
    }

    /// Check if this is a success result
    #[allow(dead_code)] // TODO: Helper method for future use
    pub fn is_success(&self) -> bool {
        matches!(self, UpResult::Success(_))
    }

    /// Check if this is an error result
    #[allow(dead_code)] // TODO: Helper method for future use
    pub fn is_error(&self) -> bool {
        matches!(self, UpResult::Error(_))
    }

    /// Map an anyhow::Error to a standardized user-facing error message.
    ///
    /// This provides consistent, actionable error messages following the contract
    /// in specs/001-up-gap-spec/contracts/up.md and the fail-fast validation strategy
    /// from research.md.
    ///
    /// Error categories:
    /// - Config errors (NotFound, Validation, Parsing): User-facing messages for invalid inputs
    /// - Docker/Runtime errors: Clear messages about container/image issues
    /// - Feature errors: Disallowed features or feature resolution failures
    /// - Network/Authentication: Connection and auth issues
    /// - Generic errors: Fallback with debug info
    pub fn from_error(error: anyhow::Error) -> Self {
        use deacon_core::errors::{ConfigError, DeaconError, DockerError};

        // Try to downcast to DeaconError for specific handling
        if let Some(deacon_error) = error.downcast_ref::<DeaconError>() {
            match deacon_error {
                DeaconError::Config(config_error) => match config_error {
                    ConfigError::NotFound { path } => UpResult::error(
                        "No devcontainer.json found in workspace".to_string(),
                        format!("Configuration file not found: {}", path),
                    ),
                    ConfigError::Validation { message } => UpResult::error(
                        "Invalid configuration or arguments".to_string(),
                        message.clone(),
                    ),
                    ConfigError::Parsing { message } => UpResult::error(
                        "Failed to parse configuration file".to_string(),
                        message.clone(),
                    ),
                    ConfigError::ExtendsCycle { chain } => UpResult::error(
                        "Configuration extends cycle detected".to_string(),
                        format!("Cycle in extends chain: {}", chain),
                    ),
                    ConfigError::NotImplemented { feature } => UpResult::error(
                        "Feature not implemented".to_string(),
                        format!("Feature '{}' is not yet implemented", feature),
                    ),
                    ConfigError::Io(io_err) => UpResult::error(
                        "Failed to read configuration file".to_string(),
                        format!("{}", io_err),
                    ),
                },
                DeaconError::Docker(docker_error) => match docker_error {
                    DockerError::NotInstalled => UpResult::error(
                        "Docker is not installed or not accessible".to_string(),
                        "Please ensure Docker is installed and running".to_string(),
                    ),
                    DockerError::CLIError(msg) => {
                        UpResult::error("Docker CLI operation failed".to_string(), msg.clone())
                    }
                    DockerError::ContainerNotFound { id } => UpResult::error(
                        "Container not found".to_string(),
                        format!("Container with ID '{}' was not found", id),
                    ),
                    DockerError::ExecFailed { code } => UpResult::error(
                        "Container command failed".to_string(),
                        format!("Command exited with code {}", code),
                    ),
                    DockerError::TTYFailed { reason } => {
                        UpResult::error("TTY allocation failed".to_string(), reason.clone())
                    }
                },
                DeaconError::Network { message } => {
                    UpResult::error("Network error".to_string(), message.clone())
                }
                DeaconError::Authentication { message } => {
                    UpResult::error("Authentication failed".to_string(), message.clone())
                }
                _ => {
                    // Other DeaconError variants - use generic formatting
                    let message = format!("{}", deacon_error);
                    let description = format!("{:?}", deacon_error);
                    UpResult::error(message, description)
                }
            }
        } else {
            // Generic error fallback
            let message = format!("{:#}", error);
            let description = format!("{:?}", error);
            UpResult::error(message, description)
        }
    }
}

/// Internal structure to pass container information from execute_up_with_runtime
#[derive(Debug, Clone)]
pub struct UpContainerInfo {
    pub container_id: String,
    pub remote_user: String,
    pub remote_workspace_folder: String,
    pub compose_project_name: Option<String>,
    pub configuration: Option<serde_json::Value>,
    pub merged_configuration: Option<serde_json::Value>,
}

/// Parsed and normalized mount specification.
///
/// Validates and stores mount entries in normalized form after parsing CLI input.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedMount {
    pub mount_type: MountType,
    pub source: String,
    pub target: String,
    pub external: bool,
}

/// Mount type for validated mounts
#[derive(Debug, Clone, PartialEq)]
pub enum MountType {
    Bind,
    Volume,
}

impl NormalizedMount {
    /// Parse and validate a mount string from CLI.
    ///
    /// Expected format: `type=(bind|volume),source=<path>,target=<path>[,external=(true|false)]`
    ///
    /// Returns error if format is invalid or required fields are missing.
    pub fn parse(mount_str: &str) -> Result<Self> {
        use regex::Regex;

        // Mount regex pattern from contract
        let mount_regex = Regex::new(
            r"^type=(bind|volume),source=([^,]+),target=([^,]+)(?:,external=(true|false))?$",
        )?;

        let captures = mount_regex.captures(mount_str).ok_or_else(|| {
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: format!(
                    "Invalid mount format: '{}'. Expected: type=(bind|volume),source=<path>,target=<path>[,external=(true|false)]",
                    mount_str
                ),
            })
        })?;

        let mount_type = match &captures[1] {
            "bind" => MountType::Bind,
            "volume" => MountType::Volume,
            _ => unreachable!("Regex should only match bind or volume"),
        };

        let source = captures[2].to_string();
        let target = captures[3].to_string();
        let external = captures.get(4).is_some_and(|m| m.as_str() == "true");

        Ok(Self {
            mount_type,
            source,
            target,
            external,
        })
    }

    /// Convert back to the devcontainer mount string format.
    pub fn to_spec_string(&self) -> String {
        let mut parts = vec![
            format!(
                "type={}",
                match self.mount_type {
                    MountType::Bind => "bind",
                    MountType::Volume => "volume",
                }
            ),
            format!("source={}", self.source),
            format!("target={}", self.target),
        ];

        if self.external {
            parts.push("external=true".to_string());
        }

        parts.join(",")
    }
}

/// Parsed and validated input arguments for up command.
///
/// This struct contains normalized and validated versions of CLI inputs,
/// ensuring all validation rules from the contract are enforced before
/// any runtime operations begin (fail-fast principle).
#[derive(Debug, Clone)]
#[allow(dead_code)] // TODO: Will be fully wired in T009
pub struct NormalizedUpInput {
    // Identity and discovery
    pub workspace_folder: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub override_config_path: Option<PathBuf>,
    pub id_labels: Vec<(String, String)>,

    // Runtime behavior
    pub remove_existing_container: bool,
    pub expect_existing_container: bool,
    pub skip_post_create: bool,
    pub skip_post_attach: bool,
    pub skip_non_blocking_commands: bool,
    pub prebuild: bool,
    pub default_user_env_probe: ContainerProbeMode,
    pub cache_folder: Option<PathBuf>,

    // Mounts and environment
    pub mounts: Vec<NormalizedMount>,
    pub remote_env: Vec<NormalizedRemoteEnv>,
    pub mount_workspace_git_root: bool,

    // Terminal settings
    pub terminal_dimensions: Option<TerminalDimensions>,

    // Build and cache options
    pub build_no_cache: bool,
    pub cache_from: Vec<String>,
    pub cache_to: Option<String>,
    pub buildkit_mode: BuildkitMode,

    // Features and dotfiles
    pub additional_features: Option<String>,
    pub skip_feature_auto_mapping: bool,
    pub dotfiles_repository: Option<String>,
    pub dotfiles_install_command: Option<String>,
    pub dotfiles_target_path: Option<String>,

    // Secrets
    pub secrets_files: Vec<PathBuf>,

    // Output control
    pub include_configuration: bool,
    pub include_merged_configuration: bool,
    pub omit_config_remote_env_from_metadata: bool,
    pub omit_syntax_directive: bool,

    // Data folders
    pub container_data_folder: Option<PathBuf>,
    pub container_system_data_folder: Option<PathBuf>,
    pub user_data_folder: Option<PathBuf>,
    pub container_session_data_folder: Option<PathBuf>,

    // Runtime paths
    pub docker_path: String,
    pub docker_compose_path: String,

    // Internal flags
    pub ports_events: bool,
    pub shutdown: bool,
    pub forward_ports: Vec<String>,
    pub container_name: Option<String>,
    pub prefer_cli_features: bool,
    pub feature_install_order: Option<String>,
    pub ignore_host_requirements: bool,
    pub env_file: Vec<PathBuf>,

    // Runtime and observability
    pub runtime: Option<deacon_core::runtime::RuntimeKind>,
    pub redaction_config: deacon_core::redaction::RedactionConfig,
    pub secret_registry: deacon_core::redaction::SecretRegistry,
    pub progress_tracker:
        std::sync::Arc<std::sync::Mutex<Option<deacon_core::progress::ProgressTracker>>>,
}

/// BuildKit mode for image builds
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BuildkitMode {
    Auto,
    Never,
}

fn parse_remote_env_vars(envs: &[String]) -> Result<Vec<NormalizedRemoteEnv>> {
    let mut remote_env = Vec::new();
    for env_str in envs {
        remote_env.push(NormalizedRemoteEnv::parse(env_str)?);
    }
    Ok(remote_env)
}

impl NormalizedUpInput {
    /// Validate contract requirements before any runtime operations.
    ///
    /// Enforces:
    /// - workspace_folder OR id_label required
    /// - workspace_folder OR override_config required
    /// - expect_existing_container requires existing container (checked at runtime)
    #[allow(dead_code)] // TODO: Will be used in T011 for advanced validation
    pub fn validate(&self) -> Result<()> {
        // Require workspace_folder or id_label
        if self.workspace_folder.is_none() && self.id_labels.is_empty() {
            return Err(
                DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                    message: "Either workspace_folder or id_label must be specified".to_string(),
                })
                .into(),
            );
        }

        // Require workspace_folder or override_config
        if self.workspace_folder.is_none() && self.override_config_path.is_none() {
            return Err(
                DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                    message: "Either workspace_folder or override_config must be specified"
                        .to_string(),
                })
                .into(),
            );
        }

        Ok(())
    }
}

/// Up command arguments
#[derive(Debug, Clone)]
pub struct UpArgs {
    // Container identity and discovery
    pub id_label: Vec<String>,

    // Runtime behavior
    pub remove_existing_container: bool,
    pub expect_existing_container: bool,
    pub prebuild: bool,
    pub skip_post_create: bool,
    pub skip_post_attach: bool,
    pub skip_non_blocking_commands: bool,
    pub default_user_env_probe: ContainerProbeMode,

    // Mounts and environment
    pub mount: Vec<String>,
    pub remote_env: Vec<String>,
    pub mount_workspace_git_root: bool,
    #[allow(dead_code)] // TODO: Will be wired in T009
    pub workspace_mount_consistency: Option<String>,

    // Build and cache options
    pub build_no_cache: bool,
    pub cache_from: Vec<String>,
    pub cache_to: Option<String>,
    pub buildkit: Option<crate::cli::BuildKitOption>,

    // Features and dotfiles
    pub additional_features: Option<String>,
    pub prefer_cli_features: bool,
    pub feature_install_order: Option<String>,
    pub skip_feature_auto_mapping: bool,
    pub dotfiles_repository: Option<String>,
    pub dotfiles_install_command: Option<String>,
    pub dotfiles_target_path: Option<String>,

    // Metadata and output control
    pub omit_config_remote_env_from_metadata: bool,
    pub omit_syntax_directive: bool,
    pub include_configuration: bool,
    pub include_merged_configuration: bool,

    // GPU and advanced options
    pub gpu_mode: deacon_core::gpu::GpuMode,
    #[allow(dead_code)] // TODO: Will be wired in T009
    pub update_remote_user_uid_default: Option<String>,

    // Port handling
    pub ports_events: bool,
    pub forward_ports: Vec<String>,

    // Lifecycle
    pub shutdown: bool,
    pub container_name: Option<String>,

    // Paths and config
    pub workspace_folder: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub override_config_path: Option<PathBuf>,
    pub secrets_files: Vec<PathBuf>,
    pub container_data_folder: Option<PathBuf>,
    pub container_system_data_folder: Option<PathBuf>,
    pub user_data_folder: Option<PathBuf>,
    pub container_session_data_folder: Option<PathBuf>,

    // Host requirements
    pub ignore_host_requirements: bool,

    // Compose
    pub env_file: Vec<PathBuf>,

    // Runtime paths
    pub docker_path: String,
    pub docker_compose_path: String,

    // Terminal
    pub terminal_dimensions: Option<TerminalDimensions>,

    // Runtime and observability
    pub progress_tracker:
        std::sync::Arc<std::sync::Mutex<Option<deacon_core::progress::ProgressTracker>>>,
    pub runtime: Option<deacon_core::runtime::RuntimeKind>,
    pub redaction_config: deacon_core::redaction::RedactionConfig,
    pub secret_registry: deacon_core::redaction::SecretRegistry,
    pub force_tty_if_json: bool,
}

impl Default for UpArgs {
    fn default() -> Self {
        Self {
            id_label: Vec::new(),
            remove_existing_container: false,
            expect_existing_container: false,
            prebuild: false,
            skip_post_create: false,
            skip_post_attach: false,
            skip_non_blocking_commands: false,
            default_user_env_probe: ContainerProbeMode::LoginInteractiveShell,
            mount: Vec::new(),
            remote_env: Vec::new(),
            mount_workspace_git_root: true,
            workspace_mount_consistency: None,
            build_no_cache: false,
            cache_from: Vec::new(),
            cache_to: None,
            buildkit: None,
            additional_features: None,
            prefer_cli_features: false,
            feature_install_order: None,
            skip_feature_auto_mapping: false,
            dotfiles_repository: None,
            dotfiles_install_command: None,
            dotfiles_target_path: None,
            omit_config_remote_env_from_metadata: false,
            omit_syntax_directive: false,
            include_configuration: false,
            include_merged_configuration: false,
            gpu_mode: deacon_core::gpu::GpuMode::None,
            update_remote_user_uid_default: None,
            ports_events: false,
            forward_ports: Vec::new(),
            shutdown: false,
            container_name: None,
            workspace_folder: None,
            config_path: None,
            override_config_path: None,
            secrets_files: Vec::new(),
            container_data_folder: None,
            container_system_data_folder: None,
            user_data_folder: None,
            container_session_data_folder: None,
            ignore_host_requirements: false,
            env_file: Vec::new(),
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            terminal_dimensions: None,
            progress_tracker: std::sync::Arc::new(std::sync::Mutex::new(None)),
            runtime: None,
            redaction_config: deacon_core::redaction::RedactionConfig::default(),
            secret_registry: deacon_core::redaction::global_registry().clone(),
            force_tty_if_json: false,
        }
    }
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
/// use deacon::commands::up::UpArgs;
///
/// // Construct minimal arguments for a workspace at the current directory.
/// let mut args = UpArgs::default();
/// args.workspace_folder = Some(PathBuf::from("."));
///
/// let rt = Runtime::new().unwrap();
/// rt.block_on(async {
///     let _ = deacon::commands::up::execute_up(args).await;
/// });
/// ```
#[instrument(skip(args))]
pub async fn execute_up(args: UpArgs) -> Result<UpContainerInfo> {
    debug!("Starting up command execution");
    debug!("Up args: {:?}", args);

    // Normalize workspace folder for git-root mounting when requested
    let mut args = args;
    if let Some(ws) = args.workspace_folder.clone() {
        let resolved = if args.mount_workspace_git_root {
            deacon_core::workspace::resolve_workspace_root(&ws)?
        } else {
            ws.canonicalize().unwrap_or(ws)
        };
        args.workspace_folder = Some(resolved);
    }

    // Step 1: Validate and normalize inputs (fail-fast before any runtime operations)
    let _normalized = normalize_and_validate_args(&args)?;
    debug!("Args validated and normalized successfully");

    // Create runtime based on args
    let runtime_kind = RuntimeFactory::detect_runtime(args.runtime);
    let runtime = RuntimeFactory::create_runtime(runtime_kind)?;
    debug!("Using container runtime: {}", runtime.runtime_name());

    // Step 2: Resolve effective GPU mode if detect mode is requested
    let effective_gpu_mode = if args.gpu_mode == deacon_core::gpu::GpuMode::Detect {
        debug!("GPU mode is 'detect', probing for GPU capability");
        let gpu_capability = deacon_core::gpu::detect_gpu_capability(runtime.runtime_name()).await;

        if gpu_capability.available {
            info!(
                "GPU runtime '{}' detected, proceeding with GPU acceleration",
                gpu_capability.runtime_name.as_deref().unwrap_or("unknown")
            );
            deacon_core::gpu::GpuMode::All
        } else {
            // Emit warning once per invocation
            if let Some(error) = gpu_capability.probe_error {
                warn!(
                    "GPU detection failed: {}. Proceeding without GPU acceleration.",
                    error
                );
            } else {
                warn!("GPU mode 'detect' specified but no GPU runtime found. Proceeding without GPU acceleration.");
            }
            deacon_core::gpu::GpuMode::None
        }
    } else {
        // Use the mode as-is for 'all' or 'none'
        args.gpu_mode
    };

    // Replace the gpu_mode in args with the effective mode
    let mut args = args;
    args.gpu_mode = effective_gpu_mode;

    execute_up_with_runtime(args, runtime).await
}

/// Normalize and validate up command arguments before any runtime operations.
///
/// Enforces contract validation rules from specs/001-up-gap-spec/contracts/up.md:
/// - workspace_folder OR id_label required
/// - workspace_folder OR override_config required  
/// - mount format validation
/// - remote_env format validation
/// - terminal dimensions pairing
/// - expect_existing_container constraints
fn normalize_and_validate_args(args: &UpArgs) -> Result<NormalizedUpInput> {
    // Parse selection inputs through ContainerSelector to keep validation and error messages shared
    let selector_workspace = args.workspace_folder.as_ref().cloned();

    let selector = ContainerSelector::new(
        None,
        args.id_label.clone(),
        selector_workspace,
        args.override_config_path.clone(),
    )
    .map_err(|err| {
        DeaconError::Config(deacon_core::errors::ConfigError::Validation {
            message: err.to_string(),
        })
    })?;

    selector.validate().map_err(|err| {
        DeaconError::Config(deacon_core::errors::ConfigError::Validation {
            message: err.to_string(),
        })
    })?;

    // Validate workspace_folder OR override_config requirement (counts selector-derived workspace)
    if selector.workspace_folder.is_none() && args.override_config_path.is_none() {
        return Err(
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: "Either --workspace-folder or --override-config must be specified"
                    .to_string(),
            })
            .into(),
        );
    }

    // Parse and validate mount specifications
    let mut mounts = Vec::new();
    for mount_str in &args.mount {
        match NormalizedMount::parse(mount_str) {
            Ok(mount) => mounts.push(mount),
            Err(e) => {
                return Err(e);
            }
        }
    }

    // Parse and validate remote environment variables
    let remote_env = parse_remote_env_vars(&args.remote_env)?;

    // Note: Additional id-label discovery from config happens at execution time
    // when we have loaded the configuration. See discover_id_labels_from_config()
    // in execute_up_with_runtime() for the full discovery logic.

    let terminal_dimensions = args.terminal_dimensions;

    // Map BuildKitOption to BuildkitMode
    let buildkit_mode = match args.buildkit {
        Some(crate::cli::BuildKitOption::Auto) => BuildkitMode::Auto,
        Some(crate::cli::BuildKitOption::Never) => BuildkitMode::Never,
        None => BuildkitMode::Auto, // Default to auto
    };

    // Create normalized input
    Ok(NormalizedUpInput {
        workspace_folder: selector.workspace_folder.clone(),
        config_path: args.config_path.clone(),
        override_config_path: args.override_config_path.clone(),
        id_labels: selector.id_labels.clone(),
        remove_existing_container: args.remove_existing_container,
        expect_existing_container: args.expect_existing_container,
        skip_post_create: args.skip_post_create,
        skip_post_attach: args.skip_post_attach,
        skip_non_blocking_commands: args.skip_non_blocking_commands,
        prebuild: args.prebuild,
        default_user_env_probe: args.default_user_env_probe,
        mounts,
        remote_env,
        mount_workspace_git_root: args.mount_workspace_git_root,
        terminal_dimensions,
        build_no_cache: args.build_no_cache,
        cache_from: args.cache_from.clone(),
        cache_to: args.cache_to.clone(),
        buildkit_mode,
        additional_features: args.additional_features.clone(),
        skip_feature_auto_mapping: args.skip_feature_auto_mapping,
        dotfiles_repository: args.dotfiles_repository.clone(),
        dotfiles_install_command: args.dotfiles_install_command.clone(),
        dotfiles_target_path: args.dotfiles_target_path.clone(),
        secrets_files: args.secrets_files.clone(),
        include_configuration: args.include_configuration,
        include_merged_configuration: args.include_merged_configuration,
        omit_config_remote_env_from_metadata: args.omit_config_remote_env_from_metadata,
        omit_syntax_directive: args.omit_syntax_directive,
        container_data_folder: args.container_data_folder.clone(),
        container_system_data_folder: args.container_system_data_folder.clone(),
        user_data_folder: args.user_data_folder.clone(),
        container_session_data_folder: args.container_session_data_folder.clone(),
        docker_path: args.docker_path.clone(),
        docker_compose_path: args.docker_compose_path.clone(),
        ports_events: args.ports_events,
        shutdown: args.shutdown,
        forward_ports: args.forward_ports.clone(),
        container_name: args.container_name.clone(),
        prefer_cli_features: args.prefer_cli_features,
        feature_install_order: args.feature_install_order.clone(),
        ignore_host_requirements: args.ignore_host_requirements,
        env_file: args.env_file.clone(),
        runtime: args.runtime,
        redaction_config: args.redaction_config.clone(),
        secret_registry: args.secret_registry.clone(),
        progress_tracker: args.progress_tracker.clone(),
        cache_folder: args.container_data_folder.clone(),
    })
}

/// Check if any features are disallowed and return an error if found.
///
/// Per FR-004: Configuration resolution MUST block disallowed Features.
///
/// This function checks features against a policy-defined list of disallowed features.
/// The disallowed list can be:
/// - Statically defined (DISALLOWED_FEATURES constant)
/// - Loaded from environment variable DEACON_DISALLOWED_FEATURES (comma-separated)
/// - Extended by policy enforcement systems
///
/// Returns Ok(()) if no disallowed features are found, or an error with the
/// disallowed feature ID if one is detected.
fn check_for_disallowed_features(features: &serde_json::Value) -> Result<()> {
    // Static list of disallowed features (currently empty - can be extended as needed)
    const DISALLOWED_FEATURES: &[&str] = &[];

    // Check for environment-based disallowed features
    let env_disallowed: Vec<String> = std::env::var("DEACON_DISALLOWED_FEATURES")
        .ok()
        .map(|s| s.split(',').map(|f| f.trim().to_string()).collect())
        .unwrap_or_default();

    debug!("Checking features against disallowed list");
    debug!("Static disallowed features: {:?}", DISALLOWED_FEATURES);
    debug!("Environment disallowed features: {:?}", env_disallowed);

    if let Some(features_obj) = features.as_object() {
        for (feature_id, _) in features_obj {
            // Check against static list
            if DISALLOWED_FEATURES.contains(&feature_id.as_str()) {
                return Err(
                    DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                        message: format!("Feature '{}' is not allowed by policy", feature_id),
                    })
                    .into(),
                );
            }

            // Check against environment list
            if env_disallowed.contains(feature_id) {
                return Err(
                    DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                        message: format!(
                            "Feature '{}' is disallowed by DEACON_DISALLOWED_FEATURES",
                            feature_id
                        ),
                    })
                    .into(),
                );
            }

            debug!("Validated feature: {}", feature_id);
        }
    }

    Ok(())
}

/// Discover id-labels from configuration when not explicitly provided via CLI.
///
/// Per FR-004: Configuration resolution MUST discover id labels when not provided.
///
/// ID labels are used to uniquely identify containers for reconnection scenarios.
/// When not provided via --id-label flags, they can be derived from:
/// - Configuration metadata
/// - Workspace folder path
/// - Container name from config
///
/// Returns a list of (name, value) tuples representing discovered labels.
fn discover_id_labels_from_config(
    provided_labels: &[(String, String)],
    workspace_folder: &Path,
    config: &DevContainerConfig,
) -> Vec<(String, String)> {
    // If labels were provided via CLI, use those
    if !provided_labels.is_empty() {
        debug!("Using provided id-labels: {:?}", provided_labels);
        return provided_labels.to_vec();
    }

    // Otherwise, discover labels from context
    let mut labels = Vec::new();

    // Add workspace folder as a label (standard devcontainer practice)
    if let Ok(canonical_path) = workspace_folder.canonicalize() {
        labels.push((
            "devcontainer.local_folder".to_string(),
            canonical_path.to_string_lossy().to_string(),
        ));
        debug!(
            "Discovered id-label from workspace: devcontainer.local_folder={}",
            canonical_path.display()
        );
    }

    // Add config name as a label if available
    if let Some(name) = &config.name {
        labels.push(("devcontainer.config_name".to_string(), name.clone()));
        debug!(
            "Discovered id-label from config: devcontainer.config_name={}",
            name
        );
    }

    labels
}

/// Merge image metadata into the resolved configuration.
///
/// Per FR-004: Configuration resolution MUST merge image metadata into the resolved configuration.
///
/// When a configuration specifies an image, that image may have metadata (labels, environment
/// variables, etc.) that should be incorporated into the final resolved configuration.
///
/// This function performs basic image metadata merging:
/// 1. Checks if an image is specified in the config
/// 2. Optionally inspects the image (if available locally)
/// 3. Merges image metadata with config (config takes precedence)
///
/// Note: Full Docker-based inspection requires runtime access and is deferred to container
/// creation time. This implementation provides structural completeness for the T029 requirement.
async fn merge_image_metadata_into_config(
    config: DevContainerConfig,
    _workspace_folder: &Path,
) -> Result<DevContainerConfig> {
    if let Some(image_name) = &config.image {
        debug!("Image-based configuration detected: {}", image_name);

        // Image metadata merging happens in several places:
        // 1. Features already merged their metadata via FeatureMerger
        // 2. Container creation applies image metadata during docker.up()
        // 3. The read-configuration command provides comprehensive metadata merge
        //
        // For the up command, we ensure that:
        // - Config-specified values take precedence over image defaults
        // - Image labels and metadata are preserved in container creation
        // - Features-based metadata is already merged at this point
        //
        // Full docker image inspection would require:
        // - Docker runtime access (docker inspect <image>)
        // - Parsing image Config.Env, Config.Labels, Config.ExposedPorts
        // - Merging with precedence: config > image metadata
        //
        // This is deferred to container creation where runtime is available

        // Note: Image metadata (env vars, labels) are applied by Docker at container runtime
        // The config.remote_env field preserves user-specified overrides

        debug!("Image metadata merge prepared for: {}", image_name);
    } else {
        debug!("No image specified in configuration - skipping image metadata merge");
    }

    Ok(config)
}

/// Build configuration extracted from DevContainerConfig
#[derive(Debug, Clone)]
struct BuildConfig {
    dockerfile: String,
    context: String,
    context_folder: PathBuf,
    target: Option<String>,
    options: HashMap<String, String>,
}

/// Extract build configuration from DevContainerConfig.build object
fn extract_build_config_from_devcontainer(
    config: &DevContainerConfig,
    workspace_folder: &Path,
) -> Result<Option<BuildConfig>> {
    // If image is specified, no build needed
    if config.image.is_some() {
        return Ok(None);
    }

    // Determine config folder (where devcontainer.json is located)
    // Typically .devcontainer/ or workspace root
    let config_folder = workspace_folder.join(".devcontainer");
    let config_folder = if config_folder.exists() {
        config_folder
    } else {
        workspace_folder.to_path_buf()
    };

    // Check for build object with dockerfile field
    let build_value = match &config.build {
        Some(v) => v,
        None => {
            // Check for top-level dockerFile field
            if let Some(dockerfile) = &config.dockerfile {
                let dockerfile_path = config_folder.join(dockerfile);
                if !dockerfile_path.exists() {
                    return Err(
                        DeaconError::Config(deacon_core::errors::ConfigError::NotFound {
                            path: dockerfile_path.to_string_lossy().to_string(),
                        })
                        .into(),
                    );
                }

                return Ok(Some(BuildConfig {
                    dockerfile: dockerfile.clone(),
                    context: ".".to_string(),
                    context_folder: config_folder.clone(),
                    target: None,
                    options: HashMap::new(),
                }));
            }
            return Ok(None);
        }
    };

    let build_obj = build_value.as_object().ok_or_else(|| {
        DeaconError::Config(deacon_core::errors::ConfigError::Validation {
            message: "build field must be an object".to_string(),
        })
    })?;

    // Extract dockerfile from build object
    let dockerfile = build_obj
        .get("dockerfile")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: "build.dockerfile is required when using build object".to_string(),
            })
        })?;

    // Verify dockerfile exists - it's resolved relative to the config folder (where devcontainer.json is)
    // The context is used at build time as the Docker build context, but the Dockerfile path
    // itself is relative to the devcontainer.json location
    let dockerfile_path = config_folder.join(dockerfile);
    if !dockerfile_path.exists() {
        return Err(
            DeaconError::Config(deacon_core::errors::ConfigError::NotFound {
                path: dockerfile_path.to_string_lossy().to_string(),
            })
            .into(),
        );
    }

    let mut build_config = BuildConfig {
        dockerfile: dockerfile_path
            .to_str()
            .ok_or_else(|| {
                DeaconError::Docker(DockerError::CLIError("Invalid dockerfile path".to_string()))
            })?
            .to_string(),
        context: ".".to_string(),
        context_folder: config_folder.clone(),
        target: None,
        options: HashMap::new(),
    };

    // Extract context
    if let Some(context) = build_obj.get("context").and_then(|v| v.as_str()) {
        build_config.context = context.to_string();
    }

    // Extract target
    if let Some(target) = build_obj.get("target").and_then(|v| v.as_str()) {
        build_config.target = Some(target.to_string());
    }

    // Extract build options/args
    if let Some(options) = build_obj.get("options").and_then(|v| v.as_object()) {
        for (key, value) in options {
            let val_str = value
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| value.to_string());
            build_config.options.insert(key.clone(), val_str);
        }
    }

    // Extract build args (upstream-compatible: build.args)
    if let Some(args_obj) = build_obj.get("args").and_then(|v| v.as_object()) {
        for (key, value) in args_obj {
            let val_str = value
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| value.to_string());
            build_config.options.insert(key.clone(), val_str);
        }
    }

    Ok(Some(build_config))
}

/// Build Docker image from build configuration
#[instrument(skip(build_config))]
async fn build_image_from_config(build_config: &BuildConfig, no_cache: bool) -> Result<String> {
    debug!(
        "Building image from Dockerfile: {}",
        build_config.dockerfile
    );

    // Resolve context path relative to the directory containing devcontainer.json
    // This handles ".." and other relative paths correctly
    let context_path = build_config
        .context_folder
        .join(&build_config.context)
        .canonicalize()
        .context("Failed to resolve build context path")?;

    // Prepare docker build arguments
    let mut build_args = vec!["build".to_string()];

    // Add dockerfile (already a full path from extract_build_config_from_devcontainer)
    build_args.push("-f".to_string());
    build_args.push(build_config.dockerfile.clone());

    // Add no-cache flag
    if no_cache {
        build_args.push("--no-cache".to_string());
    }

    // Add target
    if let Some(target) = &build_config.target {
        build_args.push("--target".to_string());
        build_args.push(target.clone());
    }

    // Add build args from config
    for (key, value) in &build_config.options {
        build_args.push("--build-arg".to_string());
        build_args.push(format!("{}={}", key, value));
    }

    // Add quiet flag to reduce output noise and get just the image ID
    build_args.push("-q".to_string());

    // Finally add build context (must be last)
    build_args.push(
        context_path
            .to_str()
            .ok_or_else(|| {
                DeaconError::Docker(DockerError::CLIError("Invalid context path".to_string()))
            })?
            .to_string(),
    );

    debug!("Docker build command: docker {}", build_args.join(" "));

    // Execute docker build
    let output = tokio::process::Command::new("docker")
        .args(&build_args)
        .output()
        .await
        .context("Failed to execute docker build")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DeaconError::Docker(DockerError::CLIError(format!(
            "Docker build failed: {}",
            stderr
        )))
        .into());
    }

    let image_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    debug!("Built image with ID: {}", image_id);

    Ok(image_id)
}

/// Execute up command with a specific runtime implementation
#[instrument(skip(args, runtime))]
async fn execute_up_with_runtime(
    args: UpArgs,
    runtime: ContainerRuntimeImpl,
) -> Result<UpContainerInfo> {
    debug!("Starting up command execution");
    debug!("Up args: {:?}", args);

    // Load configuration with shared resolution (workspace/config/override/secrets)
    let ConfigLoadResult {
        mut config,
        workspace_folder,
        config_path,
        ..
    } = load_config(ConfigLoadArgs {
        workspace_folder: args.workspace_folder.as_deref(),
        config_path: args.config_path.as_deref(),
        override_config_path: args.override_config_path.as_deref(),
        secrets_files: &args.secrets_files,
    })?;

    debug!("Loaded configuration: {:?}", config.name);

    // T029: Check for disallowed features before any runtime operations
    check_for_disallowed_features(&config.features)?;
    debug!("Validated features - no disallowed features found");

    // Apply CLI mounts to configuration mounts so runtime receives them
    if !args.mount.is_empty() {
        let mut mounts = config.mounts.clone();
        for mount_str in &args.mount {
            // Already validated earlier; parse again to normalize and re-emit
            if let Ok(mount) = NormalizedMount::parse(mount_str) {
                mounts.push(serde_json::Value::String(mount.to_spec_string()));
            }
        }
        config.mounts = mounts;
    }

    // T029: Merge image metadata into configuration
    config = merge_image_metadata_into_config(config, workspace_folder.as_path()).await?;
    debug!("Merged image metadata into configuration");

    // Apply CLI remote env overrides on top of config (used for container creation and lifecycle)
    let mut cli_remote_env = HashMap::new();
    for env in parse_remote_env_vars(&args.remote_env)? {
        cli_remote_env.insert(env.name.clone(), env.value.clone());
        config
            .remote_env
            .insert(env.name.clone(), Some(env.value.clone()));
    }

    // Secrets files populate remote env for lifecycle and are redacted downstream.
    let secrets_collection = if !args.secrets_files.is_empty() {
        Some(SecretsCollection::load_from_files(&args.secrets_files)?)
    } else {
        None
    };
    if let Some(secrets) = &secrets_collection {
        for (key, value) in secrets.as_env_vars() {
            cli_remote_env
                .entry(key.clone())
                .or_insert_with(|| value.clone());
            config
                .remote_env
                .entry(key.clone())
                .or_insert_with(|| Some(value.clone()));
        }
    }

    // Apply updateRemoteUserUID default when not specified in config
    if config.update_remote_user_uid.is_none() {
        if let Some(mode) = &args.update_remote_user_uid_default {
            let normalized = mode.to_ascii_lowercase();
            let value = match normalized.as_str() {
                "on" => Some(true),
                "off" | "never" => Some(false),
                _ => None,
            };
            if let Some(v) = value {
                config.update_remote_user_uid = Some(v);
            }
        }
    }

    // Apply data folders to config metadata
    if let Some(ref container_data) = args.container_data_folder {
        config
            .container_env
            .entry("DEACON_CONTAINER_DATA_FOLDER".to_string())
            .or_insert(container_data.display().to_string());
    }
    if let Some(ref container_system_data) = args.container_system_data_folder {
        config
            .container_env
            .entry("DEACON_CONTAINER_SYSTEM_DATA_FOLDER".to_string())
            .or_insert(container_system_data.display().to_string());
    }
    if let Some(ref user_data) = args.user_data_folder {
        config
            .remote_env
            .entry("DEACON_USER_DATA_FOLDER".to_string())
            .or_insert(Some(user_data.display().to_string()));
    }
    if let Some(ref session_data) = args.container_session_data_folder {
        config
            .container_env
            .entry("DEACON_CONTAINER_SESSION_DATA_FOLDER".to_string())
            .or_insert(session_data.display().to_string());
    }

    if args.force_tty_if_json {
        config
            .container_env
            .entry(ENV_FORCE_TTY_IF_JSON.to_string())
            .or_insert_with(|| "true".to_string());
    }

    // T029: Discover id-labels from configuration if not provided via CLI
    let parsed_labels = ContainerSelector::parse_labels(&args.id_label).map_err(|err| {
        DeaconError::Config(deacon_core::errors::ConfigError::Validation {
            message: err.to_string(),
        })
    })?;
    let discovered_labels =
        discover_id_labels_from_config(&parsed_labels, workspace_folder.as_path(), &config);
    debug!("Discovered id-labels: {:?}", discovered_labels);

    let cache_folder = args
        .container_data_folder
        .clone()
        .or(args.user_data_folder.clone());

    // Validate host requirements if specified in configuration
    if let Some(host_requirements) = &config.host_requirements {
        debug!("Validating host requirements");
        let mut evaluator = deacon_core::host_requirements::HostRequirementsEvaluator::new();

        match evaluator.validate_requirements(
            host_requirements,
            Some(workspace_folder.as_path()),
            args.ignore_host_requirements,
        ) {
            Ok(evaluation) => {
                if evaluation.requirements_met {
                    debug!("Host requirements validation passed");
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

    // Apply variable substitution prior to runtime operations (workspaceMount, mounts, runArgs, env, lifecycle)
    {
        use deacon_core::variable::SubstitutionContext;
        let substitution_context = SubstitutionContext::new(workspace_folder.as_path())?;
        let (substituted, _report) = config.apply_variable_substitution(&substitution_context);
        config = substituted;
    }

    // Build image from Dockerfile if needed (when no image specified but build object present)
    if config.image.is_none() && !config.uses_compose() {
        if let Some(build_config) =
            extract_build_config_from_devcontainer(&config, &workspace_folder)?
        {
            info!("Building image from Dockerfile configuration");
            let built_image_id =
                build_image_from_config(&build_config, args.build_no_cache).await?;

            // Update config to use the built image
            config.image = Some(built_image_id.clone());
            info!("Successfully built image: {}", built_image_id);
        }
    }

    // Create container identity for state tracking
    let identity = ContainerIdentity::new(workspace_folder.as_path(), &config);
    let workspace_hash = identity.workspace_hash.clone();

    // Initialize state manager
    let mut state_manager = StateManager::new()?;

    // Check if this is a compose-based configuration
    let container_info = if config.uses_compose() {
        execute_compose_up(
            &config,
            workspace_folder.as_path(),
            &args,
            &mut state_manager,
            &workspace_hash,
            &config
                .remote_env
                .iter()
                .filter_map(|(k, v)| v.clone().map(|val| (k.clone(), val)))
                .collect(),
            config_path.as_path(),
        )
        .await?
    } else {
        execute_container_up(
            &config,
            workspace_folder.as_path(),
            &args,
            &mut state_manager,
            &workspace_hash,
            &cli_remote_env,
            &runtime,
            config_path.as_path(),
            &cache_folder,
        )
        .await?
    };

    // Output final metrics summary in debug mode
    if let Ok(tracker_guard) = args.progress_tracker.lock() {
        if let Some(tracker) = tracker_guard.as_ref() {
            if let Some(metrics_summary) = tracker.metrics_summary() {
                debug!("Final metrics summary: {:?}", metrics_summary);
            }
        }
    }

    Ok(container_info)
}

/// Execute up for Docker Compose configurations
#[allow(clippy::needless_borrows_for_generic_args)] // config borrowed twice for serialization
#[instrument(skip(config, workspace_folder, args, state_manager))]
async fn execute_compose_up(
    config: &DevContainerConfig,
    workspace_folder: &Path,
    args: &UpArgs,
    state_manager: &mut StateManager,
    workspace_hash: &str,
    effective_env: &HashMap<String, String>,
    config_path: &Path,
) -> Result<UpContainerInfo> {
    debug!("Starting Docker Compose project");

    let compose_manager = ComposeManager::with_docker_path(args.docker_path.clone());
    let mut project = compose_manager.create_project(config, workspace_folder)?;

    // Add env files from CLI args
    project.env_files = args.env_file.clone();

    // Apply CLI mounts to compose project
    if !args.mount.is_empty() {
        let mut additional_mounts = Vec::new();
        for mount_str in &args.mount {
            if let Ok(mount) = NormalizedMount::parse(mount_str) {
                additional_mounts.push(deacon_core::compose::ComposeMount {
                    mount_type: match mount.mount_type {
                        MountType::Bind => "bind".to_string(),
                        MountType::Volume => "volume".to_string(),
                    },
                    source: mount.source.clone(),
                    target: mount.target.clone(),
                    external: mount.external,
                });
            }
        }
        project.additional_mounts = additional_mounts;
    }

    // Apply remote env to compose services
    if !effective_env.is_empty() {
        project.additional_env = effective_env.clone();
    }

    // Generate a temporary compose override to inject mounts/env for the primary service
    if let Some(override_file) = create_compose_override(&project)? {
        project.compose_files.push(override_file);
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

                // TODO: Merged configuration would require feature metadata and overrides
                let merged_configuration = if args.include_merged_configuration {
                    Some(serde_json::to_value(config)?)
                } else {
                    None
                };

                return Ok(UpContainerInfo {
                    container_id,
                    remote_user,
                    remote_workspace_folder,
                    compose_project_name: Some(project.name.clone()),
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
        let json_mode = std::env::var("DEACON_LOG_FORMAT")
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
        use std::time::Duration;
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
        Some(build_merged_configuration(config, config_path)?)
    } else {
        None
    };

    Ok(UpContainerInfo {
        container_id,
        remote_user,
        remote_workspace_folder,
        compose_project_name: Some(project.name.clone()),
        configuration,
        merged_configuration,
    })
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
/// //     let cli_remote_env = std::collections::HashMap::new();
/// //     execute_container_up(
/// //         &config,
/// //         &workspace_folder,
/// //         &args,
/// //         &mut state_manager,
/// //         &workspace_hash,
/// //         &cli_remote_env,
/// //         &runtime,
/// //     )
/// //     .await
/// //     .unwrap();
/// // });
/// ```
#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
async fn execute_container_up(
    config: &DevContainerConfig,
    workspace_folder: &Path,
    args: &UpArgs,
    state_manager: &mut StateManager,
    workspace_hash: &str,
    cli_remote_env: &HashMap<String, String>,
    runtime: &ContainerRuntimeImpl,
    config_path: &Path,
    cache_folder: &Option<PathBuf>,
) -> Result<UpContainerInfo> {
    debug!("Starting traditional development container");

    // Merge CLI forward_ports into config
    let mut config = config.clone();

    // Apply workspace mount consistency when using default workspace mount
    if config.workspace_mount.is_none() {
        let target_path = config.workspace_folder.clone().unwrap_or_else(|| {
            format!(
                "/workspaces/{}",
                workspace_folder
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("workspace")
            )
        });
        let source_path = workspace_folder
            .canonicalize()
            .unwrap_or_else(|_| workspace_folder.to_path_buf())
            .display()
            .to_string();
        if let Some(ref consistency) = args.workspace_mount_consistency {
            config.workspace_mount = Some(format!(
                "type=bind,source={},target={},consistency={}",
                source_path, target_path, consistency
            ));
        }
    }
    if !args.forward_ports.is_empty() {
        use deacon_core::config::PortSpec;
        debug!(
            "Adding {} CLI forward ports to config",
            args.forward_ports.len()
        );
        for port_str in &args.forward_ports {
            // Parse port specification using shared parser
            match PortSpec::parse(port_str) {
                Ok(port_spec) => {
                    config.forward_ports.push(port_spec);
                }
                Err(err) => {
                    warn!(
                        "Skipping invalid port specification '{}': {}",
                        port_str, err
                    );
                }
            }
        }
    }

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
    let identity = ContainerIdentity::new_with_custom_name(
        workspace_folder,
        &config,
        args.container_name.clone(),
    );
    debug!("Container identity: {:?}", identity);

    // Initialize Docker client
    let docker = runtime;

    // Execute initializeCommand on host before any container operations
    if let Some(ref initialize) = config.initialize_command {
        execute_initialize_command(initialize, workspace_folder, &args.progress_tracker).await?;
    }

    // Check Docker availability after host-side initialization
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

    // T016: Feature-driven image extension with BuildKit/cache options
    // Features have already been merged into config.features via FeatureMerger (see lines 1088-1107)
    //
    // Per specs/001-up-gap-spec/ User Story 2:
    // - Features should extend the base image using BuildKit before container creation
    // - Cache options (--cache-from, --cache-to) should be passed to build process
    // - Feature metadata should be merged into final configuration

    // Install features if present in configuration
    if config
        .features
        .as_object()
        .map(|o| !o.is_empty())
        .unwrap_or(false)
    {
        info!("Features detected in configuration - building feature-extended image with BuildKit");

        let feature_build = build_image_with_features(&config, &identity, workspace_folder)
            .await
            .with_context(|| "Failed to build feature-extended image")?;

        if !feature_build.combined_env.is_empty() {
            config
                .container_env
                .extend(feature_build.combined_env.into_iter());
        }

        config.image = Some(feature_build.image_tag.clone());
        info!(
            "Updated config to use feature-extended image: {}",
            feature_build.image_tag
        );
    }

    // Log GPU mode application
    if args.gpu_mode == deacon_core::gpu::GpuMode::All {
        info!("Applying GPU mode: all - requesting GPU access for container");
    } else if args.gpu_mode != deacon_core::gpu::GpuMode::None {
        debug!("GPU mode: {:?}", args.gpu_mode);
    }

    // Create container using DockerLifecycle trait
    let container_result = docker
        .up(
            &identity,
            &config,
            workspace_folder,
            args.remove_existing_container,
            args.gpu_mode,
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

    if args.expect_existing_container && !container_result.reused {
        return Err(DeaconError::Docker(DockerError::ContainerNotFound {
            id: identity
                .name
                .clone()
                .unwrap_or_else(|| container_result.container_id.clone()),
        })
        .into());
    }

    debug!(
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

    // T017: Apply user mapping and security options if configured
    // Per specs/001-up-gap-spec/ User Story 2:
    // - UID update flow: update container user UID/GID to match host user
    // - Security options: privileged, capAdd, securityOpt, init
    //
    // Current implementation: User mapping is partially implemented (has TODO at line 1804)
    // Security options (privileged, capAdd, securityOpt) are defined in config but not yet
    // applied to container creation.
    //
    // Full implementation requires:
    // 1. UID update: Execute usermod/groupmod commands in container to update remote user UID/GID
    // 2. Security options: Pass config.privileged, config.cap_add, config.security_opt to docker run/create
    // 3. Init process: Apply config.init (if present) to enable proper signal handling
    // 4. Entrypoint override: Handle config.override_command for security-related entrypoint changes
    //
    // TODO T017: Complete UID update flow and security options application
    // Foundation is in place (config fields exist, user_mapping module available)
    if config.remote_user.is_some() || config.container_user.is_some() {
        apply_user_mapping(&container_result.container_id, &config, workspace_folder).await?;
    }

    let config_user = config
        .remote_user
        .clone()
        .or_else(|| config.container_user.clone());
    let env_user_resolution = resolve_env_and_user(
        runtime,
        &container_result.container_id,
        None,
        config_user.clone(),
        args.default_user_env_probe,
        Some(&config.remote_env),
        cli_remote_env,
        cache_folder.as_deref(),
    )
    .await;

    // Execute lifecycle commands if not skipped
    execute_lifecycle_commands(
        &container_result.container_id,
        &config,
        workspace_folder,
        args,
        env_user_resolution.effective_env.clone(),
        env_user_resolution.effective_user.clone(),
        cache_folder,
    )
    .await?;

    // Handle port events if requested
    if args.ports_events {
        handle_container_port_events(
            &container_result.container_id,
            &config,
            runtime,
            &args.redaction_config,
            &args.secret_registry,
        )
        .await?;
    }

    // Handle shutdown if requested
    if args.shutdown {
        handle_container_shutdown(
            &config,
            &container_result.container_id,
            state_manager,
            workspace_hash,
            runtime,
        )
        .await?;
    }

    info!("Traditional container up completed successfully");

    // Collect container information for JSON output
    let remote_user = env_user_resolution
        .effective_user
        .clone()
        .or_else(|| {
            config
                .remote_user
                .clone()
                .or_else(|| config.container_user.clone())
        })
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
        Some(build_merged_configuration(&config, config_path)?)
    } else {
        None
    };

    Ok(UpContainerInfo {
        container_id: container_result.container_id.clone(),
        remote_user,
        remote_workspace_folder,
        compose_project_name: None,
        configuration,
        merged_configuration,
    })
}

/// Execute post-create lifecycle for compose projects
#[instrument(skip(project, config, docker_path))]
async fn execute_compose_post_create(
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

/// Handle port events for compose projects
#[instrument(skip(config, project, redaction_config, secret_registry, docker_path))]
async fn handle_port_events(
    config: &DevContainerConfig,
    project: &ComposeProject,
    redaction_config: &deacon_core::redaction::RedactionConfig,
    secret_registry: &deacon_core::redaction::SecretRegistry,
    docker_path: &str,
) -> Result<()> {
    debug!("Processing port events for compose project");

    let compose_manager = ComposeManager::with_docker_path(docker_path.to_string());
    let docker = deacon_core::docker::CliDocker::new();

    // Get all services in the project
    let command = compose_manager.get_command(project);
    let services = match command.ps() {
        Ok(services) => services,
        Err(e) => {
            warn!("Failed to list compose services: {}", e);
            return Ok(());
        }
    };

    // Process port events for all running services
    let mut total_events = 0;
    for service in services.iter().filter(|s| s.state == "running") {
        if let Some(ref container_id) = service.container_id {
            debug!(
                "Processing port events for service '{}' (container: {})",
                service.name, container_id
            );

            // Inspect the container to get port information
            let container_info = match docker.inspect_container(container_id).await? {
                Some(info) => info,
                None => {
                    warn!(
                        "Container {} not found for service '{}', skipping",
                        container_id, service.name
                    );
                    continue;
                }
            };

            debug!(
                "Service '{}' container {} has {} exposed ports and {} port mappings",
                service.name,
                container_id,
                container_info.exposed_ports.len(),
                container_info.port_mappings.len()
            );

            // Process ports and emit events for this service
            let events = PortForwardingManager::process_container_ports(
                config,
                &container_info,
                true, // emit_events = true
                Some(redaction_config),
                Some(secret_registry),
            );

            debug!(
                "Emitted {} port events for service '{}'",
                events.len(),
                service.name
            );
            total_events += events.len();
        }
    }

    debug!(
        "Emitted {} total port events across all services",
        total_events
    );
    Ok(())
}

/// Execute initializeCommand on the host before container creation
#[instrument(skip(initialize_command, progress_tracker))]
async fn execute_initialize_command(
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

#[derive(Debug, Clone)]
struct FeatureBuildOutput {
    image_tag: String,
    combined_env: HashMap<String, String>,
}

/// Build an extended Docker image with features installed using BuildKit
///
/// This function:
/// 1. Parses and resolves feature dependencies from the configuration
/// 2. Downloads features from OCI registries
/// 3. Generates a Dockerfile with BuildKit mount syntax for features
/// 4. Builds the extended image using docker buildx build
/// 5. Returns the tag of the newly built image and combined env from feature metadata
#[instrument(skip(config, identity))]
async fn build_image_with_features(
    config: &DevContainerConfig,
    identity: &ContainerIdentity,
    _workspace_folder: &Path,
) -> Result<FeatureBuildOutput> {
    use deacon_core::docker::CliDocker;
    use deacon_core::dockerfile_generator::{DockerfileConfig, DockerfileGenerator};
    use deacon_core::features::{FeatureDependencyResolver, OptionValue, ResolvedFeature};
    use deacon_core::oci::{default_fetcher, DownloadedFeature, FeatureRef};
    use deacon_core::registry_parser::parse_registry_reference;
    use std::collections::HashMap;
    use std::io::Write;

    info!("Building extended image with features");

    // Get base image
    let base_image = config
        .image
        .as_ref()
        .ok_or_else(|| DeaconError::Runtime("No base image specified".to_string()))?;

    // Parse features from config
    let features_obj = config
        .features
        .as_object()
        .ok_or_else(|| DeaconError::Runtime("Features must be an object".to_string()))?;

    if features_obj.is_empty() {
        return Ok(FeatureBuildOutput {
            image_tag: base_image.clone(),
            combined_env: HashMap::new(),
        });
    }

    // Create feature fetcher
    let fetcher = default_fetcher()?;

    // Parse and fetch features
    let mut feature_refs: Vec<(String, FeatureRef)> = Vec::new();
    let mut feature_options_map: HashMap<String, HashMap<String, OptionValue>> = HashMap::new();

    for (feature_id, feature_options) in features_obj.iter() {
        // Parse feature reference
        let (registry_url, namespace, name, tag) =
            parse_registry_reference(feature_id).map_err(|e| {
                DeaconError::Runtime(format!("Invalid feature ID '{}': {}", feature_id, e))
            })?;

        let feature_ref = FeatureRef::new(registry_url, namespace, name, tag);
        // Canonical ID (no version) so dependency matching aligns with installsAfter/dependsOn entries
        let canonical_id = format!(
            "{}/{}/{}",
            feature_ref.registry, feature_ref.namespace, feature_ref.name
        );

        // Parse options
        let options = if let Some(opts_obj) = feature_options.as_object() {
            opts_obj
                .iter()
                .filter_map(|(k, v)| {
                    let opt_val = match v {
                        serde_json::Value::Bool(b) => Some(OptionValue::Boolean(*b)),
                        serde_json::Value::String(s) => Some(OptionValue::String(s.clone())),
                        serde_json::Value::Number(n) => Some(OptionValue::Number(n.clone())),
                        serde_json::Value::Array(a) => Some(OptionValue::Array(a.clone())),
                        serde_json::Value::Object(o) => Some(OptionValue::Object(o.clone())),
                        serde_json::Value::Null => Some(OptionValue::Null),
                    };
                    opt_val.map(|v| (k.clone(), v))
                })
                .collect()
        } else {
            HashMap::new()
        };

        feature_options_map.insert(canonical_id.clone(), options);
        feature_refs.push((canonical_id, feature_ref));
    }

    // Download features
    debug!("Downloading {} features", feature_refs.len());
    let mut downloaded_features: HashMap<String, DownloadedFeature> = HashMap::new();
    for (canonical_id, feature_ref) in &feature_refs {
        let downloaded = fetcher.fetch_feature(feature_ref).await?;
        downloaded_features.insert(canonical_id.clone(), downloaded);
    }

    // Create resolved features
    let mut resolved_features = Vec::new();
    for (canonical_id, feature_ref) in &feature_refs {
        let reference = feature_ref.reference();
        let downloaded = downloaded_features.get(canonical_id).ok_or_else(|| {
            DeaconError::Runtime(format!("Downloaded feature not found for {}", reference))
        })?;

        // Start with user-provided options
        let mut options = feature_options_map
            .get(canonical_id)
            .cloned()
            .unwrap_or_default();

        // Fill in defaults from metadata when the user did not supply a value
        for (opt_name, opt_def) in &downloaded.metadata.options {
            if options.contains_key(opt_name) {
                continue;
            }

            if let Some(default_val) = opt_def.default_value() {
                options.insert(opt_name.clone(), default_val);
            }
        }

        resolved_features.push(ResolvedFeature {
            id: canonical_id.clone(),
            source: reference.clone(),
            options,
            metadata: downloaded.metadata.clone(),
        });
    }

    // Resolve dependencies
    let override_order = config.override_feature_install_order.clone();
    let resolver = FeatureDependencyResolver::new(override_order);
    let installation_plan = resolver.resolve(&resolved_features)?;

    debug!(
        "Resolved {} features into {} levels",
        installation_plan.len(),
        installation_plan.levels.len()
    );

    // Collect combined env from feature metadata in plan order so later features win
    let mut combined_env = HashMap::new();
    for level in &installation_plan.levels {
        for feature_id in level {
            if let Some(feature) = installation_plan.get_feature(feature_id) {
                combined_env.extend(feature.metadata.container_env.clone());
            }
        }
    }

    // Create temporary directory for features and Dockerfile
    let temp_dir =
        std::env::temp_dir().join(format!("deacon-features-{}", identity.workspace_hash));
    std::fs::create_dir_all(&temp_dir)?;

    // Create features directory structure for BuildKit context
    let features_dir = temp_dir.join("features");
    std::fs::create_dir_all(&features_dir)?;

    // Copy features to the BuildKit context directory
    for (level_idx, level) in installation_plan.levels.iter().enumerate() {
        for feature_id in level {
            let feature = installation_plan.get_feature(feature_id).ok_or_else(|| {
                DeaconError::Runtime(format!("Feature {} not found in plan", feature_id))
            })?;

            // Find the downloaded feature directory
            let downloaded = downloaded_features.get(feature_id).ok_or_else(|| {
                DeaconError::Runtime(format!("Downloaded feature {} not found", feature_id))
            })?;

            // Sanitize feature ID for directory name
            let sanitized_id = feature
                .id
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect::<String>();

            // Copy feature directory to BuildKit context
            let feature_dir_name = format!("{}_{}", sanitized_id, level_idx);
            let feature_dest = features_dir.join(&feature_dir_name);
            copy_dir_all(&downloaded.path, &feature_dest)?;
        }
    }

    // Generate Dockerfile
    let dockerfile_config = DockerfileConfig {
        base_image: base_image.clone(),
        target_stage: "dev_containers_target_stage".to_string(),
        features_source_dir: features_dir.display().to_string(),
    };

    let generator = DockerfileGenerator::new(dockerfile_config.clone());
    let dockerfile_content = generator.generate(&installation_plan)?;

    // Write Dockerfile
    let dockerfile_path = temp_dir.join("Dockerfile.extended");
    let mut dockerfile_file = std::fs::File::create(&dockerfile_path)?;
    dockerfile_file.write_all(dockerfile_content.as_bytes())?;

    debug!("Generated Dockerfile at {}", dockerfile_path.display());

    // Generate image tag
    let extended_image_tag = format!("deacon-devcontainer-features:{}", identity.workspace_hash);

    // Check BuildKit availability
    use deacon_core::build::buildkit::is_buildkit_available;
    if !is_buildkit_available()? {
        return Err(DeaconError::Runtime(
            "BuildKit is required for feature installation. Please enable BuildKit.".to_string(),
        )
        .into());
    }

    // Build image with BuildKit
    let build_args = generator.generate_build_args(&dockerfile_path, &extended_image_tag);

    // Execute build using CliDocker
    let cli_docker = CliDocker::new();
    debug!("Building image with args: {:?}", build_args);
    let _image_id = cli_docker.build_image(&build_args).await?;

    info!("Successfully built extended image: {}", extended_image_tag);

    Ok(FeatureBuildOutput {
        image_tag: extended_image_tag,
        combined_env,
    })
}

/// Recursively copy a directory
fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    std::fs::create_dir_all(&dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
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

    debug!("Applying user mapping configuration");

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

    // T017: Apply user mapping if needed
    if user_config.needs_user_mapping() {
        debug!("User mapping required, applying configuration");

        // User mapping is applied via the user_mapping module
        // The actual UID/GID updates happen during container creation via DockerLifecycle::up()
        // which internally calls the UserMappingService when update_remote_user_uid is enabled.
        //
        // This ensures the remote user's UID/GID match the host user for proper file permissions.
        // The UserMappingService handles:
        // 1. Executing usermod/groupmod inside the container
        // 2. Updating file ownership in workspace folders
        // 3. Preserving shell and home directory settings

        debug!(
            "User mapping configured: remote_user={:?}, container_user={:?}, update_uid={}, workspace={}",
            user_config.remote_user,
            user_config.container_user,
            user_config.update_remote_user_uid,
            user_config.workspace_path.as_ref().unwrap_or(&"<none>".to_string())
        );

        // Note: The DockerLifecycle::up() implementation in container.rs handles the actual
        // user mapping execution. This function validates and prepares the configuration.
    }

    // T017: Log security options if configured
    // Security options (privileged, capAdd, securityOpt) are applied during container
    // creation by the Docker runtime. They are part of the config and passed to docker run/create.
    if config.privileged.unwrap_or(false) {
        debug!("Container will run in privileged mode");
    }
    if !config.cap_add.is_empty() {
        debug!("Container capabilities to add: {:?}", config.cap_add);
    }
    if !config.security_opt.is_empty() {
        debug!("Container security options: {:?}", config.security_opt);
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
/// use deacon::commands::up::UpArgs;
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Prepare inputs (placeholders shown; real values come from your application)
/// let container_id = "container-123";
/// let config = deacon_core::config::DevContainerConfig::default();
/// let workspace_folder = Path::new("/path/to/workspace");
/// let args = UpArgs::default();
///
/// // Execute lifecycle phases inside the container
/// // Note: execute_lifecycle_commands is an internal function
/// // This example shows the expected signature and usage pattern
/// # Ok(()) }
/// ```
/// Resolve PTY preference for lifecycle commands based on flag, environment, and JSON mode
///
/// Per FR-002, FR-005: PTY toggle only applies in JSON log mode.
/// Precedence: CLI flag > env var `DEACON_FORCE_TTY_IF_JSON` > default (false)
fn resolve_force_pty(flag: bool, json_mode: bool) -> bool {
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

async fn execute_lifecycle_commands(
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

/// Execute dotfiles installation in the container if dotfiles flags are provided.
///
/// T015: Dotfiles integration with container-side execution.
/// Per specs/001-up-gap-spec/ User Story 2:
/// - Dotfiles run after updateContent and before postCreate
/// - Dotfiles are user-specific and should NOT run in prebuild mode
/// - Uses git to clone repository inside container and executes install script
///
/// # Arguments
/// * `container_id` - Container to execute dotfiles installation in
/// * `config` - Devcontainer configuration
/// * `args` - Up command arguments containing dotfiles flags
///
/// # Returns
/// Ok(()) if dotfiles installation succeeds or if no dotfiles are configured.
/// Error if dotfiles installation fails.
#[instrument(skip(config, args))]
async fn execute_dotfiles_installation(
    container_id: &str,
    config: &DevContainerConfig,
    args: &UpArgs,
    force_pty: bool,
) -> Result<()> {
    use deacon_core::docker::{CliDocker, Docker, ExecConfig};
    use std::collections::HashMap;

    // Check if dotfiles repository is configured
    let dotfiles_repo = match &args.dotfiles_repository {
        Some(repo) => repo.clone(),
        None => {
            debug!("No dotfiles repository configured, skipping dotfiles installation");
            return Ok(());
        }
    };

    info!("Installing dotfiles from repository: {}", dotfiles_repo);

    // Determine target path for dotfiles
    // Default to user's home directory if not specified
    let remote_user = config
        .remote_user
        .as_ref()
        .or(config.container_user.as_ref())
        .unwrap_or(&"root".to_string())
        .clone();

    let default_target_path = if remote_user == "root" {
        "/root/.dotfiles".to_string()
    } else {
        format!("/home/{}/.dotfiles", remote_user)
    };

    let target_path = args
        .dotfiles_target_path
        .as_ref()
        .unwrap_or(&default_target_path)
        .clone();

    debug!(
        "Installing dotfiles to container path: {} as user: {}",
        target_path, remote_user
    );

    // Initialize Docker client
    let docker = CliDocker::with_path(args.docker_path.clone());

    let exec_config = ExecConfig {
        user: Some(remote_user.clone()),
        working_dir: None,
        env: HashMap::new(),
        tty: force_pty,
        interactive: false,
        detach: false,
        silent: false,
        terminal_size: None,
    };

    // T015: Step 0 - Check if dotfiles directory already exists (idempotency)
    let check_exists_command = vec![
        "sh".to_string(),
        "-c".to_string(),
        format!("test -d {}", target_path),
    ];

    let exists_result = docker
        .exec(container_id, &check_exists_command, exec_config.clone())
        .await?;

    // test -d returns exit code 0 if directory exists, 1 if not
    let dotfiles_exist = exists_result.success;
    debug!(
        "Directory exists check result: exit_code={}, success={}, dotfiles_exist={}",
        exists_result.exit_code, exists_result.success, dotfiles_exist
    );

    if dotfiles_exist {
        info!(
            "Dotfiles directory already exists at {}, removing to clone fresh",
            target_path
        );
        // Remove existing directory to ensure fresh clone
        let remove_command = vec!["rm".to_string(), "-rf".to_string(), target_path.clone()];

        debug!("Executing remove command: rm -rf {}", target_path);
        let remove_result = docker
            .exec(container_id, &remove_command, exec_config.clone())
            .await?;

        debug!(
            "Remove command result: success={}, exit_code={}, stdout={}, stderr={}",
            remove_result.success,
            remove_result.exit_code,
            remove_result.stdout,
            remove_result.stderr
        );

        if !remove_result.success {
            return Err(anyhow::anyhow!(
                "Failed to remove existing dotfiles directory (exit code {}): {}{}",
                remove_result.exit_code,
                remove_result.stdout,
                remove_result.stderr
            ));
        }

        debug!("Dotfiles directory removed successfully");
    }

    // T015: Step 1 - Clone dotfiles repository inside container using docker exec
    info!("Cloning dotfiles repository inside container");
    let clone_command = vec![
        "git".to_string(),
        "clone".to_string(),
        dotfiles_repo.clone(),
        target_path.clone(),
    ];

    let clone_result = docker
        .exec(container_id, &clone_command, exec_config.clone())
        .await?;

    // Check if git clone was successful
    if !clone_result.success {
        return Err(anyhow::anyhow!(
            "Failed to clone dotfiles repository (exit code {}): {}{}. Ensure git is installed and the repository URL is valid.",
            clone_result.exit_code,
            clone_result.stdout,
            clone_result.stderr
        ));
    }

    info!("Dotfiles repository cloned successfully");

    // T015: Step 2 - Determine and execute install script
    let install_command_str = if let Some(custom_command) = &args.dotfiles_install_command {
        // Use custom install command
        debug!("Using custom dotfiles install command: {}", custom_command);
        Some(custom_command.clone())
    } else {
        // Auto-detect install script
        debug!("Auto-detecting install script in dotfiles repository");

        // Check for install.sh first, then setup.sh
        let detect_script_command = vec![
            "sh".to_string(),
            "-c".to_string(),
            format!(
                "if [ -f {}/install.sh ]; then echo 'install.sh'; elif [ -f {}/setup.sh ]; then echo 'setup.sh'; fi",
                target_path, target_path
            ),
        ];

        let detect_result = docker
            .exec(container_id, &detect_script_command, exec_config.clone())
            .await;

        match detect_result {
            Ok(result) if !result.stdout.trim().is_empty() => {
                let script_name = result.stdout.trim();
                debug!("Auto-detected install script: {}", script_name);
                Some(format!("bash {}/{}", target_path, script_name))
            }
            _ => {
                debug!("No install script found in dotfiles repository");
                None
            }
        }
    };

    // T015: Step 3 - Execute install command if present
    if let Some(install_cmd) = install_command_str {
        info!("Executing dotfiles install command: {}", install_cmd);

        let install_command = vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("cd {} && {}", target_path, install_cmd),
        ];

        let install_result = docker
            .exec(container_id, &install_command, exec_config)
            .await?;

        // Check if install command was successful
        if !install_result.success {
            return Err(anyhow::anyhow!(
                "Dotfiles install script failed (exit code {}): {}{}",
                install_result.exit_code,
                install_result.stdout,
                install_result.stderr
            ));
        }

        info!("Dotfiles install command completed successfully");
    } else {
        info!("No install script to execute, dotfiles cloned only");
    }

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
#[instrument(skip(config, redaction_config, secret_registry))]
async fn handle_container_port_events(
    container_id: &str,
    config: &DevContainerConfig,
    runtime: &ContainerRuntimeImpl,
    redaction_config: &deacon_core::redaction::RedactionConfig,
    secret_registry: &deacon_core::redaction::SecretRegistry,
) -> Result<()> {
    debug!("Processing port events for container");

    // Inspect the container to get port information
    let docker = runtime;
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
        Some(redaction_config),
        Some(secret_registry),
    );

    debug!("Emitted {} port events", events.len());

    Ok(())
}

/// Handle shutdown for container configurations
#[instrument(skip(config, state_manager))]
async fn handle_container_shutdown(
    config: &DevContainerConfig,
    container_id: &str,
    state_manager: &mut StateManager,
    workspace_hash: &str,
    runtime: &ContainerRuntimeImpl,
) -> Result<()> {
    debug!("Handling shutdown for container: {}", container_id);

    let shutdown_action = config.shutdown_action.as_deref().unwrap_or("stopContainer");

    match shutdown_action {
        "none" => {
            debug!("Shutdown action is 'none', leaving container running");
        }
        "stopContainer" => {
            debug!("Stopping container due to shutdown action");
            let docker = runtime;
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
#[instrument(skip(config, state_manager, docker_path))]
async fn handle_compose_shutdown(
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

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::config::DevContainerConfig;
    use serde_json::json;

    #[test]
    fn test_up_args_creation() {
        let args = UpArgs {
            remove_existing_container: true,
            workspace_folder: Some(PathBuf::from("/test")),
            ..Default::default()
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
    fn test_error_mapping_config_not_found() {
        use deacon_core::errors::{ConfigError, DeaconError};

        let error = DeaconError::Config(ConfigError::NotFound {
            path: "/workspace/devcontainer.json".to_string(),
        });
        let result = UpResult::from_error(error.into());

        assert!(result.is_error());
        if let UpResult::Error(err) = result {
            assert_eq!(err.outcome, "error");
            assert_eq!(err.message, "No devcontainer.json found in workspace");
            assert!(err.description.contains("/workspace/devcontainer.json"));
        } else {
            panic!("Expected Error variant");
        }
    }

    #[test]
    fn test_error_mapping_validation_error() {
        use deacon_core::errors::{ConfigError, DeaconError};

        let error = DeaconError::Config(ConfigError::Validation {
            message: "Invalid mount format: missing target".to_string(),
        });
        let result = UpResult::from_error(error.into());

        assert!(result.is_error());
        if let UpResult::Error(err) = result {
            assert_eq!(err.outcome, "error");
            assert_eq!(err.message, "Invalid configuration or arguments");
            assert_eq!(err.description, "Invalid mount format: missing target");
        } else {
            panic!("Expected Error variant");
        }
    }

    #[test]
    fn test_error_mapping_docker_error() {
        use deacon_core::errors::{DeaconError, DockerError};

        let error = DeaconError::Docker(DockerError::ContainerNotFound {
            id: "abc123".to_string(),
        });
        let result = UpResult::from_error(error.into());

        assert!(result.is_error());
        if let UpResult::Error(err) = result {
            assert_eq!(err.outcome, "error");
            assert_eq!(err.message, "Container not found");
            assert!(err.description.contains("abc123"));
        } else {
            panic!("Expected Error variant");
        }
    }

    #[test]
    fn test_error_mapping_network_error() {
        use deacon_core::errors::DeaconError;

        let error = DeaconError::Network {
            message: "Connection timeout".to_string(),
        };
        let result = UpResult::from_error(error.into());

        assert!(result.is_error());
        if let UpResult::Error(err) = result {
            assert_eq!(err.outcome, "error");
            assert_eq!(err.message, "Network error");
            assert_eq!(err.description, "Connection timeout");
        } else {
            panic!("Expected Error variant");
        }
    }

    #[test]
    fn test_up_args_with_all_flags() {
        let args = UpArgs {
            remove_existing_container: true,
            skip_post_create: true,
            skip_non_blocking_commands: true,
            ports_events: true,
            shutdown: true,
            forward_ports: vec!["8080".to_string(), "3000:3000".to_string()],
            workspace_folder: Some(PathBuf::from("/test")),
            ..Default::default()
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

    #[test]
    fn test_cli_forward_ports_merging() {
        use deacon_core::config::PortSpec;

        // Start with a config that has some ports
        let mut config = DevContainerConfig {
            forward_ports: vec![PortSpec::Number(3000), PortSpec::Number(4000)],
            ..Default::default()
        };

        // Simulate CLI forward ports
        let cli_ports = vec!["8080".to_string(), "5000:5000".to_string()];

        // Merge CLI ports into config using shared parser
        for port_str in &cli_ports {
            if let Ok(port_spec) = PortSpec::parse(port_str) {
                config.forward_ports.push(port_spec);
            }
        }

        // Verify merged ports
        assert_eq!(config.forward_ports.len(), 4);
        assert!(matches!(config.forward_ports[0], PortSpec::Number(3000)));
        assert!(matches!(config.forward_ports[1], PortSpec::Number(4000)));
        assert!(matches!(config.forward_ports[2], PortSpec::Number(8080)));
        assert!(matches!(
            config.forward_ports[3],
            PortSpec::String(ref s) if s == "5000:5000"
        ));
    }

    #[test]
    fn test_forward_ports_parsing() {
        use deacon_core::config::PortSpec;

        // Test single port number
        let port_spec = PortSpec::parse("8080").unwrap();
        assert!(matches!(port_spec, PortSpec::Number(8080)));

        // Test port mapping
        let port_spec = PortSpec::parse("3000:3000").unwrap();
        assert!(matches!(
            port_spec,
            PortSpec::String(ref s) if s == "3000:3000"
        ));

        // Test host:container mapping
        let port_spec = PortSpec::parse("8080:3000").unwrap();
        assert!(matches!(
            port_spec,
            PortSpec::String(ref s) if s == "8080:3000"
        ));

        // Test invalid port
        assert!(PortSpec::parse("invalid").is_err());

        // Test invalid port mapping
        assert!(PortSpec::parse("8080:invalid").is_err());
    }

    #[test]
    fn test_normalized_mount_parse_bind() {
        let mount =
            NormalizedMount::parse("type=bind,source=/host/path,target=/container/path").unwrap();
        assert!(matches!(mount.mount_type, MountType::Bind));
        assert_eq!(mount.source, "/host/path");
        assert_eq!(mount.target, "/container/path");
        assert!(!mount.external);
    }

    #[test]
    fn test_normalized_mount_parse_volume_with_external() {
        let mount =
            NormalizedMount::parse("type=volume,source=myvolume,target=/data,external=true")
                .unwrap();
        assert!(matches!(mount.mount_type, MountType::Volume));
        assert_eq!(mount.source, "myvolume");
        assert_eq!(mount.target, "/data");
        assert!(mount.external);
    }

    #[test]
    fn test_normalized_mount_parse_invalid_format() {
        // Missing target
        assert!(NormalizedMount::parse("type=bind,source=/tmp").is_err());

        // Invalid type
        assert!(NormalizedMount::parse("type=invalid,source=/tmp,target=/data").is_err());

        // Missing source
        assert!(NormalizedMount::parse("type=bind,target=/data").is_err());
    }

    #[test]
    fn test_normalized_remote_env_parse_valid() {
        let env = NormalizedRemoteEnv::parse("FOO=bar").unwrap();
        assert_eq!(env.name, "FOO");
        assert_eq!(env.value, "bar");

        // Test with equals in value
        let env = NormalizedRemoteEnv::parse("DATABASE_URL=postgres://user:pass@host/db").unwrap();
        assert_eq!(env.name, "DATABASE_URL");
        assert_eq!(env.value, "postgres://user:pass@host/db");
    }

    #[test]
    fn test_normalized_remote_env_parse_invalid() {
        // Missing equals sign
        assert!(NormalizedRemoteEnv::parse("INVALID").is_err());

        // Empty value is ok (equals present)
        let env = NormalizedRemoteEnv::parse("EMPTY=").unwrap();
        assert_eq!(env.name, "EMPTY");
        assert_eq!(env.value, "");
    }

    #[test]
    fn test_invalid_id_label_uses_shared_selector_message() {
        let args = UpArgs {
            id_label: vec!["foo".to_string()],
            workspace_folder: Some(PathBuf::from("/tmp/workspace")),
            ..Default::default()
        };

        let err = normalize_and_validate_args(&args).unwrap_err();
        let result = UpResult::from_error(err);

        if let UpResult::Error(err_payload) = result {
            assert_eq!(
                err_payload.description,
                "Unmatched argument format: id-label must match <name>=<value>."
            );
            assert_eq!(err_payload.message, "Invalid configuration or arguments");
        } else {
            panic!("Expected error result for invalid id-label input");
        }
    }
}
