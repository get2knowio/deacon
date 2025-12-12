//! Arguments and validation for the up command.
//!
//! This module contains:
//! - `UpArgs` - CLI argument structure
//! - `NormalizedUpInput` - Validated and normalized inputs
//! - `NormalizedMount` - Parsed mount specifications
//! - `MountType` - Mount type enum
//! - `BuildkitMode` - BuildKit mode enum
//! - Helper to construct `BuildOptions` from CLI args

use crate::commands::shared::{NormalizedRemoteEnv, TerminalDimensions};
use anyhow::Result;
use deacon_core::build::BuildOptions;
use deacon_core::container::ContainerSelector;
use deacon_core::container_env_probe::ContainerProbeMode;
use deacon_core::errors::DeaconError;
use std::path::PathBuf;

/// Parsed and normalized mount specification.
///
/// Validates and stores mount entries in normalized form after parsing CLI input.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedMount {
    pub mount_type: MountType,
    pub source: String,
    pub target: String,
    /// Whether the mount is read-only (adds `:ro` suffix to the volume)
    /// Parsed from CLI `external=true` for backward compatibility
    pub read_only: bool,
    /// Mount consistency option (cached, consistent, delegated)
    /// Only applicable to bind mounts on macOS for performance tuning
    pub consistency: Option<String>,
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
    /// Expected format: `type=(bind|volume),source=<path>,target=<path>[,external=(true|false)][,consistency=(cached|consistent|delegated)]`
    ///
    /// Note: The `external` option in CLI maps to `read_only` field internally.
    /// This maintains backward compatibility while using clearer naming in code.
    /// The optional fields (external, consistency) can appear in any order.
    ///
    /// Returns error if format is invalid or required fields are missing.
    pub fn parse(mount_str: &str) -> Result<Self> {
        // Parse key-value pairs from the mount string
        let mut mount_type_str: Option<&str> = None;
        let mut source: Option<&str> = None;
        let mut target: Option<&str> = None;
        let mut external: Option<&str> = None;
        let mut consistency: Option<&str> = None;

        for part in mount_str.split(',') {
            if let Some((key, value)) = part.split_once('=') {
                match key {
                    "type" => mount_type_str = Some(value),
                    "source" => source = Some(value),
                    "target" => target = Some(value),
                    "external" => external = Some(value),
                    "consistency" => consistency = Some(value),
                    _ => {
                        return Err(DeaconError::Config(
                            deacon_core::errors::ConfigError::Validation {
                                message: format!(
                                    "Invalid mount format: '{}'. Unknown option: '{}'",
                                    mount_str, key
                                ),
                            },
                        )
                        .into())
                    }
                }
            } else {
                return Err(
                    DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                        message: format!(
                            "Invalid mount format: '{}'. Expected key=value format",
                            mount_str
                        ),
                    })
                    .into(),
                );
            }
        }

        // Validate required fields
        let mount_type_str = mount_type_str.ok_or_else(|| {
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: format!(
                    "Invalid mount format: '{}'. Missing required field: type",
                    mount_str
                ),
            })
        })?;

        let source = source.ok_or_else(|| {
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: format!(
                    "Invalid mount format: '{}'. Missing required field: source",
                    mount_str
                ),
            })
        })?;

        let target = target.ok_or_else(|| {
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: format!(
                    "Invalid mount format: '{}'. Missing required field: target",
                    mount_str
                ),
            })
        })?;

        // Validate and convert mount type
        let mount_type = match mount_type_str {
            "bind" => MountType::Bind,
            "volume" => MountType::Volume,
            _ => {
                return Err(
                    DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                        message: format!(
                            "Invalid mount format: '{}'. type must be 'bind' or 'volume'",
                            mount_str
                        ),
                    })
                    .into(),
                )
            }
        };

        // Validate external value if provided
        let read_only = match external {
            Some("true") => true,
            Some("false") | None => false,
            Some(val) => {
                return Err(
                    DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                        message: format!(
                        "Invalid mount format: '{}'. external must be 'true' or 'false', got: '{}'",
                        mount_str, val
                    ),
                    })
                    .into(),
                )
            }
        };

        // Validate consistency value if provided
        let consistency = match consistency {
            Some("cached") => Some("cached".to_string()),
            Some("consistent") => Some("consistent".to_string()),
            Some("delegated") => Some("delegated".to_string()),
            None => None,
            Some(val) => {
                return Err(DeaconError::Config(
                    deacon_core::errors::ConfigError::Validation {
                        message: format!(
                            "Invalid mount format: '{}'. consistency must be 'cached', 'consistent', or 'delegated', got: '{}'",
                            mount_str, val
                        ),
                    },
                )
                .into())
            }
        };

        Ok(Self {
            mount_type,
            source: source.to_string(),
            target: target.to_string(),
            read_only,
            consistency,
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

        if self.read_only {
            parts.push("external=true".to_string());
        }

        if let Some(ref consistency) = self.consistency {
            parts.push(format!("consistency={}", consistency));
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

    // Lockfile control (experimental)
    /// Path to feature lockfile for validation (experimental)
    pub experimental_lockfile: Option<PathBuf>,
    /// Require lockfile to exist and match config features exactly (experimental)
    pub experimental_frozen_lockfile: bool,

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
            experimental_lockfile: None,
            experimental_frozen_lockfile: false,
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

/// Parse remote environment variables from CLI strings.
pub(crate) fn parse_remote_env_vars(envs: &[String]) -> Result<Vec<NormalizedRemoteEnv>> {
    let mut remote_env = Vec::new();
    for env_str in envs {
        remote_env.push(NormalizedRemoteEnv::parse(env_str)?);
    }
    Ok(remote_env)
}

/// Construct `BuildOptions` from up command arguments.
///
/// Extracts the build-related options from `UpArgs` and aggregates them into
/// a `BuildOptions` struct that can be threaded through both Dockerfile and
/// feature builds.
///
/// Per spec (data-model.md):
/// - `cache_from`: ordered list of cache sources, preserved when invoking BuildKit/buildx
/// - `cache_to`: optional cache destination
/// - `builder`: optional buildx builder selection (not yet exposed in up CLI, set to None)
/// - Scope: applies to entire `up` run for both Dockerfile and feature builds
pub(crate) fn build_options_from_args(args: &UpArgs) -> BuildOptions {
    BuildOptions {
        no_cache: args.build_no_cache,
        cache_from: args.cache_from.clone(),
        cache_to: args.cache_to.clone(),
        // builder is not currently exposed in the up CLI; reserved for future addition
        builder: None,
    }
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
pub(crate) fn normalize_and_validate_args(args: &UpArgs) -> Result<NormalizedUpInput> {
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

    // Validate workspace_mount_consistency values
    if let Some(ref consistency) = args.workspace_mount_consistency {
        const VALID_CONSISTENCY: &[&str] = &["cached", "consistent", "delegated"];
        if !VALID_CONSISTENCY.contains(&consistency.as_str()) {
            return Err(anyhow::anyhow!(
                "Invalid workspace mount consistency '{}'. Valid values: cached, consistent, delegated",
                consistency
            ));
        }
    }

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
