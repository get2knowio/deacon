//! Up command implementation
//!
//! Implements the `deacon up` subcommand for starting development containers.
//! Supports both traditional container workflows and Docker Compose workflows.

use anyhow::Result;
use deacon_core::compose::{ComposeCommand, ComposeManager, ComposeProject};
use deacon_core::config::{ConfigLoader, DevContainerConfig};
use deacon_core::container::ContainerIdentity;
use deacon_core::docker::{Docker, DockerLifecycle, ExecConfig};
use deacon_core::errors::DeaconError;
use deacon_core::features::{FeatureMergeConfig, FeatureMerger};
use deacon_core::ports::PortForwardingManager;
use deacon_core::runtime::{ContainerRuntimeImpl, RuntimeFactory};
use deacon_core::state::{ComposeState, ContainerState, StateManager};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

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
    #[allow(dead_code)] // TODO: Will be used when includeConfiguration flag is wired
    pub fn with_configuration(mut self, configuration: serde_json::Value) -> Self {
        if let UpResult::Success(ref mut success) = self {
            success.configuration = Some(configuration);
        }
        self
    }

    /// Add merged configuration to a success result
    #[allow(dead_code)] // TODO: Will be used when includeMergedConfiguration flag is wired
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
}

/// Parsed and normalized remote environment variable.
///
/// Validates env var entries from CLI input.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedRemoteEnv {
    pub name: String,
    pub value: String,
}

impl NormalizedRemoteEnv {
    /// Parse and validate a remote env string from CLI.
    ///
    /// Expected format: `NAME=value`
    ///
    /// Returns error if format is invalid (missing =).
    pub fn parse(env_str: &str) -> Result<Self> {
        let parts: Vec<&str> = env_str.splitn(2, '=').collect();

        if parts.len() != 2 {
            return Err(
                DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                    message: format!(
                        "Invalid remote-env format: '{}'. Expected: NAME=value",
                        env_str
                    ),
                })
                .into(),
            );
        }

        Ok(Self {
            name: parts[0].to_string(),
            value: parts[1].to_string(),
        })
    }
}

/// Terminal dimensions for output formatting.
///
/// Both columns and rows must be specified together.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerminalDimensions {
    pub columns: u32,
    pub rows: u32,
}

impl TerminalDimensions {
    /// Create terminal dimensions, ensuring both are specified.
    pub fn new(columns: Option<u32>, rows: Option<u32>) -> Result<Option<Self>> {
        match (columns, rows) {
            (Some(cols), Some(rows)) => Ok(Some(Self {
                columns: cols,
                rows,
            })),
            (None, None) => Ok(None),
            (Some(_), None) => Err(DeaconError::Config(
                deacon_core::errors::ConfigError::Validation {
                    message: "terminalColumns requires terminalRows to be specified".to_string(),
                },
            )
            .into()),
            (None, Some(_)) => Err(DeaconError::Config(
                deacon_core::errors::ConfigError::Validation {
                    message: "terminalRows requires terminalColumns to be specified".to_string(),
                },
            )
            .into()),
        }
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
    #[allow(dead_code)] // TODO: Will be wired in T009
    pub gpu_availability: Option<String>,
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
    pub terminal_columns: Option<u32>,
    pub terminal_rows: Option<u32>,

    // Runtime and observability
    pub progress_tracker:
        std::sync::Arc<std::sync::Mutex<Option<deacon_core::progress::ProgressTracker>>>,
    pub runtime: Option<deacon_core::runtime::RuntimeKind>,
    pub redaction_config: deacon_core::redaction::RedactionConfig,
    pub secret_registry: deacon_core::redaction::SecretRegistry,
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
            gpu_availability: None,
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
            terminal_columns: None,
            terminal_rows: None,
            progress_tracker: std::sync::Arc::new(std::sync::Mutex::new(None)),
            runtime: None,
            redaction_config: deacon_core::redaction::RedactionConfig::default(),
            secret_registry: deacon_core::redaction::global_registry().clone(),
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

    // Step 1: Validate and normalize inputs (fail-fast before any runtime operations)
    let _normalized = normalize_and_validate_args(&args)?;
    debug!("Args validated and normalized successfully");

    // Create runtime based on args
    let runtime_kind = RuntimeFactory::detect_runtime(args.runtime);
    let runtime = RuntimeFactory::create_runtime(runtime_kind)?;
    debug!("Using container runtime: {}", runtime.runtime_name());

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
    // Validate workspace_folder OR id_label requirement
    if args.workspace_folder.is_none() && args.id_label.is_empty() {
        return Err(
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: "Either --workspace-folder or --id-label must be specified".to_string(),
            })
            .into(),
        );
    }

    // Validate workspace_folder OR override_config requirement
    if args.workspace_folder.is_none() && args.override_config_path.is_none() {
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
    let mut remote_env = Vec::new();
    for env_str in &args.remote_env {
        match NormalizedRemoteEnv::parse(env_str) {
            Ok(env) => remote_env.push(env),
            Err(e) => {
                return Err(e);
            }
        }
    }

    // Parse and validate id labels
    let mut id_labels = Vec::new();
    for label_str in &args.id_label {
        let parts: Vec<&str> = label_str.splitn(2, '=').collect();
        if parts.len() != 2 {
            return Err(
                DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                    message: format!(
                        "Invalid id-label format: '{}'. Expected: name=value",
                        label_str
                    ),
                })
                .into(),
            );
        }
        id_labels.push((parts[0].to_string(), parts[1].to_string()));
    }

    // Note: Additional id-label discovery from config happens at execution time
    // when we have loaded the configuration. See discover_id_labels_from_config()
    // in execute_up_with_runtime() for the full discovery logic.

    // Validate terminal dimensions pairing
    let terminal_dimensions = TerminalDimensions::new(args.terminal_columns, args.terminal_rows)?;

    // Map BuildKitOption to BuildkitMode
    let buildkit_mode = match args.buildkit {
        Some(crate::cli::BuildKitOption::Auto) => BuildkitMode::Auto,
        Some(crate::cli::BuildKitOption::Never) => BuildkitMode::Never,
        None => BuildkitMode::Auto, // Default to auto
    };

    // Create normalized input
    Ok(NormalizedUpInput {
        workspace_folder: args.workspace_folder.clone(),
        config_path: args.config_path.clone(),
        override_config_path: args.override_config_path.clone(),
        id_labels,
        remove_existing_container: args.remove_existing_container,
        expect_existing_container: args.expect_existing_container,
        skip_post_create: args.skip_post_create,
        skip_post_attach: args.skip_post_attach,
        skip_non_blocking_commands: args.skip_non_blocking_commands,
        prebuild: args.prebuild,
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
    })
}

/// Check if any features are disallowed and return an error if found.
///
/// Per FR-004: Configuration resolution MUST block disallowed Features.
///
/// Currently, no features are explicitly disallowed by default. This function
/// serves as a placeholder for future policy enforcement (e.g., security
/// restrictions, compatibility checks).
///
/// Returns Ok(()) if no disallowed features are found, or an error with the
/// disallowed feature ID if one is detected.
fn check_for_disallowed_features(features: &serde_json::Value) -> Result<()> {
    // TODO: Implement actual disallowed features list
    // This could come from:
    // - A configuration file
    // - Environment variables
    // - Policy enforcement system
    // - Feature compatibility matrix

    // Placeholder: no features are currently disallowed
    // Future implementation might check against a list like:
    // const DISALLOWED_FEATURES: &[&str] = &["unsafe-feature", "deprecated-feature"];

    if let Some(features_obj) = features.as_object() {
        for (feature_id, _) in features_obj {
            // Example future check:
            // if DISALLOWED_FEATURES.contains(&feature_id.as_str()) {
            //     return Err(DeaconError::Config(ConfigError::Validation {
            //         message: format!("Feature '{}' is not allowed", feature_id),
            //     }).into());
            // }
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
#[allow(dead_code)] // TODO: Wire into execute_up_with_runtime for automatic label discovery
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
/// This is a placeholder for full image inspection and metadata merging. The actual
/// implementation would need to:
/// 1. Inspect the image using Docker/container runtime
/// 2. Extract relevant metadata (env vars, labels, exposed ports, etc.)
/// 3. Merge that metadata with the config, respecting precedence rules
///
/// For now, this returns the config unchanged, as full image inspection is complex
/// and would require runtime access. Image metadata merge is better handled during
/// container creation where we have full runtime access.
async fn merge_image_metadata_into_config(
    config: DevContainerConfig,
    _workspace_folder: &Path,
) -> Result<DevContainerConfig> {
    // TODO: Implement full image metadata merge
    // This requires:
    // 1. Docker image inspection (docker inspect <image>)
    // 2. Extracting labels, env vars, exposed ports from image metadata
    // 3. Merging with config (config takes precedence over image metadata)
    // 4. Handling image pull if not present locally

    // For now, return config as-is
    // The read-configuration command already implements features-based metadata merge
    // which is more comprehensive for most use cases

    debug!("Image metadata merge placeholder - returning config unchanged");
    Ok(config)
}

/// Execute up command with a specific runtime implementation
#[instrument(skip(args, runtime))]
async fn execute_up_with_runtime(
    args: UpArgs,
    runtime: ContainerRuntimeImpl,
) -> Result<UpContainerInfo> {
    debug!("Starting up command execution");
    debug!("Up args: {:?}", args);

    // Load configuration
    let workspace_folder = args.workspace_folder.as_deref().unwrap_or(Path::new("."));

    let mut config = if let Some(config_path) = args.config_path.as_ref() {
        ConfigLoader::load_from_path(config_path)?
    } else {
        let config_location = ConfigLoader::discover_config(workspace_folder)?;
        if !config_location.exists() {
            return Err(
                DeaconError::Config(deacon_core::errors::ConfigError::NotFound {
                    path: config_location.path().to_string_lossy().to_string(),
                })
                .into(),
            );
        }
        ConfigLoader::load_from_path(config_location.path())?
    };

    debug!("Loaded configuration: {:?}", config.name);

    // T029: Check for disallowed features before any runtime operations
    check_for_disallowed_features(&config.features)?;
    debug!("Validated features - no disallowed features found");

    // T029: Merge image metadata into configuration
    config = merge_image_metadata_into_config(config, workspace_folder).await?;
    debug!("Merged image metadata into configuration");

    // Validate host requirements if specified in configuration
    if let Some(host_requirements) = &config.host_requirements {
        debug!("Validating host requirements");
        let mut evaluator = deacon_core::host_requirements::HostRequirementsEvaluator::new();

        match evaluator.validate_requirements(
            host_requirements,
            Some(workspace_folder),
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
        let substitution_context = SubstitutionContext::new(workspace_folder)?;
        let (substituted, _report) = config.apply_variable_substitution(&substitution_context);
        config = substituted;
    }

    // Create container identity for state tracking
    let identity = ContainerIdentity::new(workspace_folder, &config);
    let workspace_hash = identity.workspace_hash.clone();

    // Initialize state manager
    let mut state_manager = StateManager::new()?;

    // Check if this is a compose-based configuration
    let container_info = if config.uses_compose() {
        execute_compose_up(
            &config,
            workspace_folder,
            &args,
            &mut state_manager,
            &workspace_hash,
        )
        .await?
    } else {
        execute_container_up(
            &config,
            workspace_folder,
            &args,
            &mut state_manager,
            &workspace_hash,
            &runtime,
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
#[instrument(skip(config, workspace_folder, args, state_manager))]
async fn execute_compose_up(
    config: &DevContainerConfig,
    workspace_folder: &Path,
    args: &UpArgs,
    state_manager: &mut StateManager,
    workspace_hash: &str,
) -> Result<UpContainerInfo> {
    debug!("Starting Docker Compose project");

    let compose_manager = ComposeManager::with_docker_path(args.docker_path.clone());
    let mut project = compose_manager.create_project(config, workspace_folder)?;

    // Add env files from CLI args
    project.env_files = args.env_file.clone();

    debug!("Created compose project: {:?}", project.name);

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

                return Ok(UpContainerInfo {
                    container_id,
                    remote_user,
                    remote_workspace_folder,
                    compose_project_name: Some(project.name.clone()),
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

    compose_manager.start_project(&project)?;

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
        execute_compose_post_create(&project, config, &args.docker_path).await?;
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
    let container_id = compose_manager
        .get_primary_container_id(&project)?
        .ok_or_else(|| {
            anyhow::anyhow!("Failed to get primary container ID after starting compose project")
        })?;

    let remote_user = config
        .remote_user
        .clone()
        .or_else(|| config.container_user.clone())
        .unwrap_or_else(|| "root".to_string());

    let remote_workspace_folder = config
        .workspace_folder
        .clone()
        .unwrap_or_else(|| "/workspaces".to_string());

    Ok(UpContainerInfo {
        container_id,
        remote_user,
        remote_workspace_folder,
        compose_project_name: Some(project.name.clone()),
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
/// //     execute_container_up(&config, &workspace_folder, &args, &mut state_manager, &workspace_hash).await.unwrap();
/// // });
/// ```
#[instrument(skip_all)]
async fn execute_container_up(
    config: &DevContainerConfig,
    workspace_folder: &Path,
    args: &UpArgs,
    state_manager: &mut StateManager,
    workspace_hash: &str,
    runtime: &ContainerRuntimeImpl,
) -> Result<UpContainerInfo> {
    debug!("Starting traditional development container");

    // Merge CLI forward_ports into config
    let mut config = config.clone();
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

    // Create container using DockerLifecycle trait
    let container_result = docker
        .up(
            &identity,
            &config,
            workspace_folder,
            args.remove_existing_container,
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

    // Apply user mapping if configured
    if config.remote_user.is_some() || config.container_user.is_some() {
        apply_user_mapping(&container_result.container_id, &config, workspace_folder).await?;
    }

    // Execute lifecycle commands if not skipped
    execute_lifecycle_commands(
        &container_result.container_id,
        &config,
        workspace_folder,
        args,
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
    let remote_user = config
        .remote_user
        .clone()
        .or_else(|| config.container_user.clone())
        .unwrap_or_else(|| "root".to_string());

    let remote_workspace_folder = config
        .workspace_folder
        .clone()
        .unwrap_or_else(|| "/workspaces".to_string());

    Ok(UpContainerInfo {
        container_id: container_result.container_id.clone(),
        remote_user,
        remote_workspace_folder,
        compose_project_name: None,
    })
}

/// Execute post-create lifecycle for compose projects
#[instrument(skip(project, config, docker_path))]
async fn execute_compose_post_create(
    project: &ComposeProject,
    config: &DevContainerConfig,
    docker_path: &str,
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
                        tty: false,
                        interactive: false,
                        detach: false,
                        silent: false,
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

    // Apply user mapping if needed
    if user_config.needs_user_mapping() {
        debug!("User mapping required, applying configuration");

        // TODO: Implement user mapping application using UserMappingService
        // For now, log that user mapping would be applied
        debug!(
            "User mapping configured: remote_user={:?}, container_user={:?}, update_uid={}",
            user_config.remote_user, user_config.container_user, user_config.update_remote_user_uid
        );
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
async fn execute_lifecycle_commands(
    container_id: &str,
    config: &DevContainerConfig,
    workspace_folder: &Path,
    args: &UpArgs,
) -> Result<()> {
    use deacon_core::container_lifecycle::{
        execute_container_lifecycle_with_progress_callback, ContainerLifecycleCommands,
        ContainerLifecycleConfig,
    };
    use deacon_core::variable::SubstitutionContext;

    debug!("Executing lifecycle commands in container");

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
        non_blocking_timeout: Duration::from_secs(300), // 5 minutes default timeout
        use_login_shell: true, // Default: use login shell for lifecycle commands
        user_env_probe: deacon_core::container_env_probe::ContainerProbeMode::LoginShell,
    };

    // Build lifecycle commands from configuration
    let mut commands = ContainerLifecycleCommands::new();
    let mut phases_to_execute = Vec::new();

    if let Some(ref on_create) = config.on_create_command {
        let phase_commands = commands_from_json_value(on_create)?;
        commands = commands.with_on_create(phase_commands.clone());
        phases_to_execute.push(("onCreate".to_string(), phase_commands));
    }

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

    if let Some(ref post_attach) = config.post_attach_command {
        let phase_commands = commands_from_json_value(post_attach)?;
        commands = commands.with_post_attach(phase_commands.clone());
        phases_to_execute.push(("postAttach".to_string(), phase_commands));
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
    fn test_terminal_dimensions_both_specified() {
        let dims = TerminalDimensions::new(Some(80), Some(24)).unwrap();
        assert!(dims.is_some());
        let dims = dims.unwrap();
        assert_eq!(dims.columns, 80);
        assert_eq!(dims.rows, 24);
    }

    #[test]
    fn test_terminal_dimensions_neither_specified() {
        let dims = TerminalDimensions::new(None, None).unwrap();
        assert!(dims.is_none());
    }

    #[test]
    fn test_terminal_dimensions_only_columns_specified() {
        let result = TerminalDimensions::new(Some(80), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_terminal_dimensions_only_rows_specified() {
        let result = TerminalDimensions::new(None, Some(24));
        assert!(result.is_err());
    }
}
