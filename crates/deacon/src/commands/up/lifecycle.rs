//! Lifecycle command execution for the up command.
//!
//! This module contains:
//! - `resolve_force_pty` - Resolve PTY preference based on flags and environment
//! - `build_invocation_context` - Build InvocationContext from CLI args and prior state
//! - `execute_lifecycle_commands` - Execute lifecycle phases in container
//! - `execute_initialize_command` - Execute initializeCommand on host

use super::args::UpArgs;
use super::{ENV_FORCE_TTY_IF_JSON, ENV_LOG_FORMAT};
use anyhow::{Context, Result};
use deacon_core::config::DevContainerConfig;
use deacon_core::container_env_probe::ContainerProbeMode;
use deacon_core::container_lifecycle::{
    aggregate_lifecycle_commands, AggregatedLifecycleCommand, DotfilesConfig, LifecycleCommandList,
    LifecycleCommandSource, LifecycleCommandValue,
};
use deacon_core::features::ResolvedFeature;
use deacon_core::lifecycle::{InvocationContext, InvocationFlags, LifecyclePhaseState};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, info, instrument, span, warn, Level};

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

/// Build an `InvocationContext` from CLI arguments and prior state markers.
///
/// This function determines the appropriate invocation mode based on:
/// 1. CLI flags (`--skip-post-create`, `--prebuild`)
/// 2. Prior phase markers (for resume detection per SC-002 and FR-004)
///
/// Per spec (data-model.md):
/// - **mode**: enum {fresh, resume, prebuild, skip_post_create}
/// - **flags**: inputs affecting lifecycle (skip_post_create, prebuild booleans)
/// - **workspace_root**: path to the devcontainer workspace
/// - **prior_markers**: collection of LifecyclePhaseState loaded from disk
///
/// Mode determination precedence (per spec):
/// 1. If `--prebuild` is set -> `Prebuild` mode
/// 2. If `--skip-post-create` is set -> `SkipPostCreate` mode
/// 3. If all non-runtime phases complete in markers -> `Resume` mode (SC-002)
/// 4. If some markers exist but not all non-runtime complete -> `Fresh` mode with markers (FR-004)
/// 5. No markers -> `Fresh` mode
///
/// # Arguments
///
/// * `args` - The parsed CLI arguments for the up command
/// * `workspace_folder` - Path to the workspace root directory
/// * `prior_markers` - Previously completed phase states loaded from disk (if any)
///
/// # Returns
///
/// An `InvocationContext` configured with the appropriate mode, flags, and prior state.
pub(crate) fn build_invocation_context(
    args: &UpArgs,
    workspace_folder: &Path,
    prior_markers: Vec<LifecyclePhaseState>,
) -> InvocationContext {
    // Build flags from CLI args
    let flags = InvocationFlags {
        skip_post_create: args.skip_post_create,
        prebuild: args.prebuild,
    };

    // Use the new marker-aware mode determination from core library
    // This properly handles:
    // - SC-002: Resume mode when all non-runtime phases are complete
    // - FR-004: Fresh mode with markers for partial resume (skip completed phases)
    let ctx = InvocationContext::from_markers_with_flags(
        workspace_folder.to_path_buf(),
        prior_markers,
        flags,
    );

    debug!(
        "Built invocation context: mode={:?}, flags={{skip_post_create={}, prebuild={}}}, prior_markers={}",
        ctx.mode, ctx.flags.skip_post_create, ctx.flags.prebuild, ctx.prior_markers.len()
    );

    ctx
}

/// Execute configured lifecycle phases inside a running container.
///
/// This runs the lifecycle command phases defined in `config` and `resolved_features` (onCreate,
/// updateContent, postCreate, postStart, postAttach) in that order, emitting per-phase progress
/// events to `args.progress_tracker` when present and recording an overall lifecycle duration metric.
///
/// Per User Story 2 (US2) and lifecycle command aggregation contract:
/// - Feature lifecycle commands execute BEFORE config lifecycle commands
/// - Commands are aggregated in feature installation order, then config
/// - Each command's source (feature ID or "config") is logged for tracing and debugging
///
/// Per SC-002 and FR-004:
/// - On resume with all non-runtime phases complete: skip onCreate, updateContent, postCreate, dotfiles; run postStart, postAttach
/// - On partial resume: skip completed phases, run remaining phases from earliest incomplete
///
/// Parameters:
/// - `container_id`: container identifier where commands will be executed.
/// - `config`: devcontainer configuration containing lifecycle command definitions and environment.
/// - `workspace_folder`: host path used to build the substitution context and to derive the container workspace path when not explicitly set in `config`.
/// - `args`: runtime flags that influence execution (e.g., skipping post-create, non-blocking behavior) and an optional progress tracker.
/// - `resolved_features`: Features resolved during image build, containing lifecycle commands to execute before config commands.
/// - `prior_markers`: Previously executed phase markers for resume detection.
///
/// Behavior notes:
/// - Commands may be provided as a single string, array, object, or null in the config.
/// - Emits LifecyclePhaseBegin for each phase before execution and LifecyclePhaseEnd for each phase after execution (end events contain an approximate per-phase duration).
/// - Records the total lifecycle duration under the metric name "lifecycle" if a progress tracker is available.
/// - Returns any error produced by the underlying lifecycle executor, with source attribution context.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_lifecycle_commands(
    container_id: &str,
    config: &DevContainerConfig,
    workspace_folder: &Path,
    args: &UpArgs,
    effective_env: HashMap<String, String>,
    effective_user: Option<String>,
    cache_folder: &Option<PathBuf>,
    resolved_features: &[ResolvedFeature],
    prior_markers: Vec<LifecyclePhaseState>,
) -> Result<()> {
    use deacon_core::container_lifecycle::{
        execute_container_lifecycle_with_progress_callback, ContainerLifecycleCommands,
        ContainerLifecycleConfig,
    };
    use deacon_core::lifecycle::LifecyclePhase;
    use deacon_core::variable::SubstitutionContext;

    debug!("Executing lifecycle commands in container");

    // Log feature integration for lifecycle command aggregation
    if !resolved_features.is_empty() {
        info!(
            feature_count = resolved_features.len(),
            "Lifecycle command aggregation: {} features will have their lifecycle commands executed before config commands",
            resolved_features.len()
        );
        for (idx, feature) in resolved_features.iter().enumerate() {
            debug!(
                feature_index = idx,
                feature_id = %feature.id,
                "Feature in lifecycle aggregation order"
            );
        }
    } else {
        debug!("No features with lifecycle commands; using config commands only");
    }

    // T020: --skip-post-create flag handling
    // Per FR-005: When --skip-post-create is provided, up MUST perform required base setup
    // (container creation and content update via onCreate/updateContent) and MUST skip
    // postCreate, postStart, postAttach, and dotfiles.
    //
    // The skipping of specific phases is handled by the InvocationContext::should_skip_phase()
    // method which returns "--skip-post-create flag" as the reason for skipped phases.
    // This allows onCreate and updateContent to still execute.

    // T014: Build invocation context with prior markers for resume decision logic
    // Per SC-002: On resume, skip onCreate, updateContent, postCreate, dotfiles; run postStart, postAttach
    // Per FR-004: On partial resume, skip completed phases, run remaining from earliest incomplete
    let invocation_context = build_invocation_context(args, workspace_folder, prior_markers);

    debug!(
        "Lifecycle invocation mode: {:?}, prior_markers: {}",
        invocation_context.mode,
        invocation_context.prior_markers.len()
    );

    // Create substitution context
    let substitution_context = SubstitutionContext::new(workspace_folder)?;

    // Determine container workspace folder
    let container_workspace_folder =
        crate::commands::shared::derive_container_workspace_folder(config, workspace_folder);

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

    // Build dotfiles configuration from CLI args (T009: per SC-001 lifecycle ordering)
    let dotfiles_config = if args.dotfiles_repository.is_some() {
        Some(DotfilesConfig {
            repository: args.dotfiles_repository.clone(),
            target_path: args.dotfiles_target_path.clone(),
            install_command: args.dotfiles_install_command.clone(),
        })
    } else {
        None
    };

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
        dotfiles: dotfiles_config,
        is_prebuild: args.prebuild,
    };

    // Build lifecycle commands from configuration, respecting resume decisions
    // T014/T020: Use invocation context to determine which phases should be skipped
    // The should_skip_phase method returns the reason for skipping (e.g., "--skip-post-create flag",
    // "prior completion marker", "prebuild mode") which we use in debug logs.
    //
    // T020: --skip-post-create and prebuild mode both skip postCreate/dotfiles/postStart/postAttach.
    // The InvocationContext handles these via should_skip_phase().
    // PostAttach also respects the separate --skip-post-attach flag.
    let mut commands = ContainerLifecycleCommands::new();
    let phases = [
        LifecyclePhase::OnCreate,
        LifecyclePhase::UpdateContent,
        LifecyclePhase::PostCreate,
        LifecyclePhase::PostStart,
        LifecyclePhase::PostAttach,
    ];

    for &phase in &phases {
        if let Some(skip_reason) = invocation_context.should_skip_phase(phase) {
            debug!("Skipping {}: {}", phase.as_str(), skip_reason);
            continue;
        }

        // PostAttach has an additional skip flag
        if phase == LifecyclePhase::PostAttach && args.skip_post_attach {
            debug!("Skipping postAttach: --skip-post-attach flag");
            continue;
        }

        let aggregated_commands = aggregate_lifecycle_commands(phase, resolved_features, config)
            .with_context(|| format!("Failed to parse {} lifecycle commands", phase.as_str()))?;

        if !aggregated_commands.is_empty() {
            let _span =
                span!(Level::INFO, "lifecycle_aggregation", phase = phase.as_str()).entered();
            for (idx, agg_cmd) in aggregated_commands.commands.iter().enumerate() {
                info!(
                    command_index = idx,
                    source = %agg_cmd.source,
                    "{} command queued for execution",
                    phase.as_str()
                );
            }

            debug!(
                "{} phase queued for execution with {} aggregated commands",
                phase.as_str(),
                aggregated_commands.len(),
            );
            commands = commands.set_phase(phase, aggregated_commands);
        }
    }

    let lifecycle_start_time = std::time::Instant::now();

    // Execute lifecycle commands with progress callback
    let result = execute_container_lifecycle_with_progress_callback(
        &lifecycle_config,
        &commands,
        &substitution_context,
        Some(crate::commands::shared::progress::make_progress_callback(
            &args.progress_tracker,
        )),
    )
    .await;

    let lifecycle_duration = lifecycle_start_time.elapsed();

    // Record metrics
    match args.progress_tracker.lock() {
        Ok(tracker_guard) => {
            if let Some(tracker) = tracker_guard.as_ref() {
                tracker.record_duration("lifecycle", lifecycle_duration);
            }
        }
        Err(e) => {
            warn!("Progress tracker mutex poisoned: {}", e);
        }
    }

    let result = result.context("Lifecycle command execution failed in container")?;

    debug!(
        "Lifecycle execution completed: {} blocking phases executed, {} non-blocking phases to execute",
        result.phases.len(),
        result.non_blocking_phases.len()
    );

    // T009: Dotfiles execution is now integrated into container_lifecycle.rs
    // per SC-001 lifecycle ordering: postCreate -> dotfiles -> postStart
    // Dotfiles are automatically skipped in prebuild mode and when skip_post_create is set

    // Execute non-blocking phases (postStart, postAttach) synchronously
    // This ensures they run before the up command returns
    if !result.non_blocking_phases.is_empty() {
        use deacon_core::docker::CliDocker;

        debug!(
            "Executing {} non-blocking phases synchronously",
            result.non_blocking_phases.len()
        );

        let docker = CliDocker::new();

        let _final_result = result
            .execute_non_blocking_phases_sync_with_callback(
                &docker,
                Some(crate::commands::shared::progress::make_progress_callback(
                    &args.progress_tracker,
                )),
            )
            .await
            .context("Non-blocking lifecycle phase execution failed")?;

        debug!("Non-blocking phases execution completed");
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

    // Parse the initialize command using the typed parser from core
    let parsed = LifecycleCommandValue::from_json_value(initialize_command)
        .context("Failed to parse initializeCommand")?;

    // If null or empty, nothing to do
    let parsed = match parsed {
        Some(cmd) if !cmd.is_empty() => cmd,
        _ => {
            debug!("initializeCommand is null or empty, skipping");
            return Ok(());
        }
    };

    // Build a LifecycleCommandList from the parsed value
    let command_list = LifecycleCommandList {
        commands: vec![AggregatedLifecycleCommand {
            command: parsed,
            source: LifecycleCommandSource::Config,
        }],
    };

    // Create substitution context for host-side execution
    let substitution_context = SubstitutionContext::new(workspace_folder)?;

    // Build lifecycle commands with just initialize phase
    let commands = ContainerLifecycleCommands::new().with_initialize(command_list);

    // Create a dummy lifecycle config (only needed for container phases, not host phases)
    let lifecycle_config = deacon_core::container_lifecycle::ContainerLifecycleConfig {
        container_id: "<host-only-no-container>".to_string(),
        user: None,
        container_workspace_folder: "<host-only-no-container>".to_string(),
        container_env: std::collections::HashMap::new(),
        skip_post_create: false,
        skip_non_blocking_commands: false,
        non_blocking_timeout: Duration::from_secs(300),
        use_login_shell: true,
        user_env_probe: deacon_core::container_env_probe::ContainerProbeMode::None,
        cache_folder: None,
        force_pty: false,
        dotfiles: None,
        is_prebuild: false,
    };

    // Execute only the initialize phase (host-side)
    use deacon_core::container_lifecycle::execute_container_lifecycle_with_progress_callback;
    let result = execute_container_lifecycle_with_progress_callback(
        &lifecycle_config,
        &commands,
        &substitution_context,
        Some(crate::commands::shared::progress::make_progress_callback(
            progress_tracker,
        )),
    )
    .await?;

    debug!(
        "initializeCommand execution completed: {} phases executed",
        result.phases.len()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::lifecycle::{InvocationMode, LifecyclePhase, PhaseStatus};

    fn default_args() -> UpArgs {
        UpArgs::default()
    }

    // ========================================================================
    // build_invocation_context tests
    // ========================================================================

    #[test]
    fn test_build_invocation_context_fresh_mode() {
        let args = default_args();
        let workspace = PathBuf::from("/workspace");
        let prior_markers = Vec::new();

        let ctx = build_invocation_context(&args, &workspace, prior_markers);

        assert_eq!(ctx.mode, InvocationMode::Fresh);
        assert!(!ctx.flags.skip_post_create);
        assert!(!ctx.flags.prebuild);
        assert!(ctx.prior_markers.is_empty());
        assert_eq!(ctx.workspace_root, workspace);
    }

    #[test]
    fn test_build_invocation_context_prebuild_mode() {
        let mut args = default_args();
        args.prebuild = true;
        let workspace = PathBuf::from("/workspace");

        let ctx = build_invocation_context(&args, &workspace, Vec::new());

        assert_eq!(ctx.mode, InvocationMode::Prebuild);
        assert!(ctx.flags.prebuild);
    }

    #[test]
    fn test_build_invocation_context_skip_post_create_mode() {
        let mut args = default_args();
        args.skip_post_create = true;
        let workspace = PathBuf::from("/workspace");

        let ctx = build_invocation_context(&args, &workspace, Vec::new());

        assert_eq!(ctx.mode, InvocationMode::SkipPostCreate);
        assert!(ctx.flags.skip_post_create);
    }

    #[test]
    fn test_build_invocation_context_prebuild_takes_precedence() {
        // When both prebuild and skip_post_create are set, prebuild takes precedence
        let mut args = default_args();
        args.prebuild = true;
        args.skip_post_create = true;
        let workspace = PathBuf::from("/workspace");

        let ctx = build_invocation_context(&args, &workspace, Vec::new());

        assert_eq!(ctx.mode, InvocationMode::Prebuild);
        // Both flags should still be set in the flags struct
        assert!(ctx.flags.prebuild);
        assert!(ctx.flags.skip_post_create);
    }

    #[test]
    fn test_build_invocation_context_resume_mode() {
        // SC-002: Resume mode requires ALL non-runtime phases to be complete
        let args = default_args();
        let workspace = PathBuf::from("/workspace");

        // Create prior markers with all non-runtime phases complete
        let prior_markers = vec![
            LifecyclePhaseState::new_executed(
                LifecyclePhase::OnCreate,
                PathBuf::from("/tmp/markers/onCreate"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                PathBuf::from("/tmp/markers/updateContent"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostCreate,
                PathBuf::from("/tmp/markers/postCreate"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::Dotfiles,
                PathBuf::from("/tmp/markers/dotfiles"),
            ),
        ];

        let ctx = build_invocation_context(&args, &workspace, prior_markers);

        assert_eq!(ctx.mode, InvocationMode::Resume);
        assert_eq!(ctx.prior_markers.len(), 4);
    }

    #[test]
    fn test_build_invocation_context_partial_resume_is_fresh_mode() {
        // FR-004: Partial markers result in Fresh mode (with markers preserved for skipping)
        let args = default_args();
        let workspace = PathBuf::from("/workspace");

        // Only onCreate complete - not all non-runtime phases
        let prior_markers = vec![LifecyclePhaseState::new_executed(
            LifecyclePhase::OnCreate,
            PathBuf::from("/tmp/markers/onCreate"),
        )];

        let ctx = build_invocation_context(&args, &workspace, prior_markers);

        // Should be Fresh mode (partial resume) not Resume mode
        assert_eq!(ctx.mode, InvocationMode::Fresh);
        // But markers are preserved for FR-004 skip logic
        assert_eq!(ctx.prior_markers.len(), 1);
        assert_eq!(ctx.prior_markers[0].phase, LifecyclePhase::OnCreate);
        assert_eq!(ctx.prior_markers[0].status, PhaseStatus::Executed);
    }

    #[test]
    fn test_build_invocation_context_flags_override_resume() {
        // Flags should take precedence over resume detection
        let mut args = default_args();
        args.prebuild = true;
        let workspace = PathBuf::from("/workspace");

        // Even with prior markers, prebuild flag should result in Prebuild mode
        let marker_path = PathBuf::from("/tmp/markers/onCreate");
        let prior_markers = vec![LifecyclePhaseState::new_executed(
            LifecyclePhase::OnCreate,
            marker_path,
        )];

        let ctx = build_invocation_context(&args, &workspace, prior_markers);

        // Prebuild mode takes precedence over Resume
        assert_eq!(ctx.mode, InvocationMode::Prebuild);
        // But prior_markers are still stored for potential use
        assert_eq!(ctx.prior_markers.len(), 1);
    }

    #[test]
    fn test_build_invocation_context_workspace_path() {
        let args = default_args();
        let workspace = PathBuf::from("/my/workspace/path");

        let ctx = build_invocation_context(&args, &workspace, Vec::new());

        assert_eq!(ctx.workspace_root, workspace);
    }
}
