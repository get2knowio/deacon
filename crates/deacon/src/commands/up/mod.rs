//! Up command implementation
//!
//! Implements the `deacon up` subcommand for starting development containers.
//! Supports both traditional container workflows and Docker Compose workflows.
//!
//! ## Module Organization
//!
//! This module is organized into focused submodules:
//! - `args` - Arguments and validation (UpArgs, NormalizedUpInput, NormalizedMount, etc.)
//! - `result` - Response types (UpSuccess, UpError, UpResult, UpContainerInfo)
//! - `merged_config` - Configuration merging logic
//! - `image_build` - Docker image building from Dockerfile
//! - `features_build` - Feature image building with BuildKit
//! - `compose` - Docker Compose flow
//! - `container` - Single container flow
//! - `lifecycle` - Lifecycle command execution
//! - `dotfiles` - Dotfiles installation
//! - `ports` - Port event handling
//! - `helpers` - Utility functions

mod args;
mod compose;
mod container;
mod dotfiles;
mod features_build;
mod helpers;
mod image_build;
mod lifecycle;
mod merged_config;
mod ports;
mod result;
#[cfg(test)]
mod tests;

// Re-export public types from submodules
#[allow(unused_imports)]
pub use args::{BuildkitMode, MountType, NormalizedMount, NormalizedUpInput, UpArgs};
#[allow(unused_imports)]
pub use result::{EffectiveMount, UpContainerInfo, UpError, UpResult, UpSuccess};

// Re-export NormalizedRemoteEnv from shared module
#[allow(unused_imports)]
pub use crate::commands::shared::NormalizedRemoteEnv;

use crate::commands::shared::{load_config, ConfigLoadArgs, ConfigLoadResult};
use anyhow::{Context, Result};
use deacon_core::container::{ContainerIdentity, ContainerSelector};
use deacon_core::errors::DeaconError;
use deacon_core::features::{FeatureMergeConfig, FeatureMerger};
use deacon_core::lockfile::{
    get_lockfile_path, read_lockfile, validate_lockfile_against_config, LockfileValidationResult,
};
use deacon_core::runtime::{ContainerRuntimeImpl, RuntimeFactory};
use deacon_core::secrets::SecretsCollection;
use deacon_core::state::StateManager;
use deacon_core::IndexMap;
use tracing::{debug, info, instrument, warn};

// Internal imports from submodules
use args::{build_options_from_args, normalize_and_validate_args, parse_remote_env_vars};
use compose::execute_compose_up;
use container::execute_container_up;
use helpers::{check_for_disallowed_features, discover_id_labels_from_config};
use image_build::{build_image_from_config, extract_build_config_from_devcontainer};
use merged_config::merge_image_metadata_into_config;

/// Environment variable name for controlling PTY allocation in JSON log mode.
/// When set to truthy values (true/1/yes, case-insensitive), forces PTY allocation
/// for lifecycle exec commands during `deacon up` when JSON logging is active.
pub(crate) const ENV_FORCE_TTY_IF_JSON: &str = "DEACON_FORCE_TTY_IF_JSON";

/// Environment variable name for log format detection.
pub(crate) const ENV_LOG_FORMAT: &str = "DEACON_LOG_FORMAT";

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
            // Check if git root exists before resolving
            let git_root_result = deacon_core::workspace::find_git_repository_root(&ws)?;
            if git_root_result.is_none() {
                // T016: Log fallback when git root requested but not found
                info!(
                    "Git root requested (--mount-workspace-git-root) but no git repository found at '{}'. Using workspace root instead.",
                    ws.display()
                );
            }
            deacon_core::workspace::resolve_workspace_root(&ws)?
        } else {
            ws.canonicalize().with_context(|| {
                format!(
                    "Failed to resolve workspace path '{}': path does not exist or cannot be accessed",
                    ws.display()
                )
            })?
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

/// Execute up command with a specific runtime implementation
#[instrument(skip(args, runtime))]
pub(crate) async fn execute_up_with_runtime(
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

    // T012: Enforce lockfile/frozen validation pre-build
    // T013: User-facing error handling for lockfile/frozen enforcement
    // If frozen mode is enabled, or lockfile path is explicitly provided, validate before build
    if args.experimental_frozen_lockfile || args.experimental_lockfile.is_some() {
        // Determine lockfile path: use explicit path if provided, otherwise derive from config
        let lockfile_path = args
            .experimental_lockfile
            .clone()
            .unwrap_or_else(|| get_lockfile_path(&config_path));

        // Use info-level log so users can see lockfile validation is active
        if args.experimental_frozen_lockfile {
            info!(
                "Frozen lockfile mode enabled: validating features against '{}'",
                lockfile_path.display()
            );
        } else {
            debug!(
                "Lockfile validation enabled: path={}",
                lockfile_path.display()
            );
        }

        // Read the lockfile (may be None if missing)
        let lockfile = read_lockfile(&lockfile_path).with_context(|| {
            format!(
                "Failed to read lockfile at '{}'. \
                 The file may be corrupted or contain invalid JSON. \
                 To regenerate, remove the file and run without --experimental-frozen-lockfile.",
                lockfile_path.display()
            )
        })?;

        // Validate lockfile against config features
        let validation_result =
            validate_lockfile_against_config(lockfile.as_ref(), &config.features, &lockfile_path);

        match &validation_result {
            LockfileValidationResult::Matched => {
                if args.experimental_frozen_lockfile {
                    info!(
                        "Lockfile validation passed: all features match '{}'",
                        lockfile_path.display()
                    );
                } else {
                    debug!("Lockfile validation passed");
                }
            }
            _ => {
                let error_message = validation_result.format_error();

                if args.experimental_frozen_lockfile {
                    // Frozen mode: fail immediately on any mismatch (exit code 1)
                    return Err(DeaconError::Config(
                        deacon_core::errors::ConfigError::Validation {
                            message: error_message,
                        },
                    )
                    .into());
                } else {
                    // Non-frozen lockfile mode: warn but continue
                    warn!("{}", error_message);
                }
            }
        }
    }

    // T006: Prepare build options from CLI args for threading through Dockerfile and feature builds
    let build_options = build_options_from_args(&args);
    if build_options.requires_buildkit() {
        debug!(
            "Build options require BuildKit: cache_from={:?}, cache_to={:?}, builder={:?}",
            build_options.cache_from, build_options.cache_to, build_options.builder
        );
    } else if !build_options.is_default() {
        debug!(
            "Build options set (no BuildKit required): no_cache={}",
            build_options.no_cache
        );
    }
    // T007: build_options is passed to Dockerfile builds below
    // T008: build_options is passed to feature builds via execute_container_up

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
    // Using IndexMap to preserve CLI argument order per spec
    let mut cli_remote_env: IndexMap<String, String> = IndexMap::new();
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
        // T013: Log info when skip-feature-auto-mapping is enabled so users know CLI features are ignored
        if args.skip_feature_auto_mapping && args.additional_features.is_some() {
            info!(
                "Skip-feature-auto-mapping enabled: CLI-provided features (--additional-features) \
                 will be ignored. Only features declared in devcontainer.json will be used."
            );
        }

        let merge_config = FeatureMergeConfig::new(
            args.additional_features.clone(),
            args.prefer_cli_features,
            args.feature_install_order.clone(),
            args.skip_feature_auto_mapping,
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
    // T007: Pass build_options to apply cache-from/cache-to/buildx settings
    if config.image.is_none() && !config.uses_compose() {
        if let Some(build_config) =
            extract_build_config_from_devcontainer(&config, &workspace_folder)?
        {
            info!("Building image from Dockerfile configuration");
            let built_image_id = build_image_from_config(&build_config, &build_options).await?;

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
            &cli_remote_env,
            config_path.as_path(),
            &runtime,
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
            &build_options,
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
