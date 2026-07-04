//! Lifecycle command execution for the up command.
//!
//! This module contains:
//! - `resolve_force_pty` - Resolve PTY preference based on flags and environment
//! - `build_invocation_context` - Build InvocationContext from CLI args and prior state
//! - `execute_lifecycle_commands` - Execute lifecycle phases in container
//! - `execute_initialize_command` - Execute initializeCommand on host (with workspace-trust gate)
//! - `HostTrustArgs` / `enforce_host_trust` - Workspace-trust gate primitives

use super::args::UpArgs;
use super::{ENV_FORCE_TTY_IF_JSON, ENV_LOG_FORMAT};
use anyhow::{Context, Result};
use deacon_core::config::DevContainerConfig;
use deacon_core::container_lifecycle::{
    AggregatedLifecycleCommand, DotfilesConfig, LifecycleCommandList, LifecycleCommandSource,
    LifecycleCommandValue, aggregate_lifecycle_commands,
};
use deacon_core::features::ResolvedFeature;
use deacon_core::lifecycle::{
    InvocationContext, InvocationFlags, LifecyclePhase, LifecyclePhaseState,
    should_queue_phase_for_wait_for, should_run_dotfiles_for_wait_for, wait_for_phase,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{Level, debug, info, instrument, span, warn};

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
        ctx.mode,
        ctx.flags.skip_post_create,
        ctx.flags.prebuild,
        ctx.prior_markers.len()
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
    config_hash: Option<&str>,
    runtime: &deacon_core::runtime::ContainerRuntimeImpl,
) -> Result<()> {
    use deacon_core::container_lifecycle::{
        ContainerLifecycleCommands, ContainerLifecycleConfig,
        execute_container_lifecycle_with_progress_callback_and_docker,
    };
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

    let wait_for = wait_for_phase(config.wait_for.as_deref())?;
    if args.skip_non_blocking_commands {
        debug!(
            "Lifecycle will stop after waitFor phase {} because --skip-non-blocking-commands is set",
            wait_for.as_str()
        );
    }

    // Build dotfiles configuration from CLI args (T009: per SC-001 lifecycle ordering)
    let dotfiles_config = if args.dotfiles_repository.is_some()
        && should_run_dotfiles_for_wait_for(args.skip_non_blocking_commands, wait_for)
    {
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
        // Part 3: buffer lifecycle output behind the per-phase spinner in compact
        // mode; verbose/non-TTY/JSON keeps streaming it live.
        capture_output: args.build_output_mode == deacon_core::build::BuildOutputMode::Compact,
        container_id: container_id.to_string(),
        user: effective_user,
        container_workspace_folder,
        container_env: effective_env,
        skip_post_create: args.skip_post_create,
        skip_non_blocking_commands: args.skip_non_blocking_commands,
        non_blocking_timeout: Duration::from_secs(300), // 5 minutes default timeout
        use_login_shell: true, // Default: use login shell for lifecycle commands
        user_env_probe: config.user_env_probe.unwrap_or(args.default_user_env_probe),
        cache_folder: cache_folder.clone(),
        force_pty,
        dotfiles: dotfiles_config,
        is_prebuild: args.prebuild,
        config_hash: config_hash.map(String::from),
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
        if !should_queue_phase_for_wait_for(args.skip_non_blocking_commands, wait_for, phase) {
            debug!(
                "Skipping {}: occurs after configured waitFor phase {} with --skip-non-blocking-commands",
                phase.as_str(),
                wait_for.as_str()
            );
            continue;
        }

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

    // Execute lifecycle commands with progress callback, using the SELECTED
    // runtime (docker or podman). The plain wrapper hardcodes a docker client,
    // which under podman cannot see the podman-created container ("No such
    // container") — so every lifecycle exec must go through `runtime`.
    let result = execute_container_lifecycle_with_progress_callback_and_docker(
        &lifecycle_config,
        &commands,
        &substitution_context,
        &runtime.cli_docker(),
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
        debug!(
            "Executing {} non-blocking phases synchronously",
            result.non_blocking_phases.len()
        );

        // Use the selected runtime (see blocking-phase note above).
        let docker = runtime.cli_docker();

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

/// Inputs needed to resolve the workspace-trust policy for a host-side
/// lifecycle hook.
///
/// Borrowed view to avoid forcing callers to clone every flag — the
/// `up`-tier already owns the underlying buffers.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HostTrustArgs<'a> {
    /// `--trust-workspace` flag (one-shot trust, no persistence).
    pub trust_workspace: bool,
    /// `--trust-workspace-persist` flag (one-shot + writes to the trust store).
    pub trust_workspace_persist: bool,
    /// Host-side user data folder (where the trust store lives). When
    /// `None`, the default `~/.deacon/` is used.
    pub user_data_folder: Option<&'a Path>,
}

/// Read `DEACON_NO_PROMPT` from the environment. `1`, `true`, `yes`
/// (case-insensitive) are truthy; anything else (including unset) is falsey.
fn deacon_no_prompt_env() -> bool {
    std::env::var("DEACON_NO_PROMPT")
        .ok()
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes"
        })
        .unwrap_or(false)
}

/// Enforce the workspace-trust gate before a host-side lifecycle hook runs.
///
/// On success this also handles the `--trust-workspace-persist` side-effect
/// (writing the workspace into the trust store) so callers don't have to
/// thread the persistence step separately.
pub(crate) async fn enforce_host_trust(
    workspace_folder: &Path,
    args: &HostTrustArgs<'_>,
) -> Result<()> {
    use deacon_core::trust::{
        check_workspace_trust, decision_to_result, record_trusted_workspace, resolve_policy,
    };

    let policy = resolve_policy(
        args.trust_workspace,
        args.trust_workspace_persist,
        deacon_no_prompt_env(),
        args.user_data_folder,
    )
    .context("Failed to resolve workspace trust policy")?;

    let decision = check_workspace_trust(workspace_folder, policy)
        .await
        .context("Workspace trust check failed")?;

    decision_to_result(decision).map_err(anyhow::Error::from)?;

    if args.trust_workspace_persist {
        record_trusted_workspace(workspace_folder, args.user_data_folder)
            .await
            .context("Failed to persist workspace trust entry")?;
    }

    Ok(())
}

/// Execute initializeCommand on the host before container creation
///
/// `initializeCommand` runs arbitrary shell on the **developer's host** before
/// any container sandboxing. The trust check below is the only thing standing
/// between `git clone <hostile-repo> && deacon up` and arbitrary code
/// execution on the host. Callers MUST pass the resolved trust args
/// (`--trust-workspace`, `--trust-workspace-persist`, `DEACON_NO_PROMPT`)
/// through `trust_args`; see [`HostTrustArgs`] for the source-of-truth
/// resolution rules.
#[instrument(skip(initialize_command, progress_tracker, trust_args))]
pub(crate) async fn execute_initialize_command(
    initialize_command: &serde_json::Value,
    workspace_folder: &Path,
    progress_tracker: &std::sync::Arc<
        std::sync::Mutex<Option<deacon_core::progress::ProgressTracker>>,
    >,
    trust_args: HostTrustArgs<'_>,
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

    // Trust gate: refuse to run host-side shell from an untrusted workspace.
    enforce_host_trust(workspace_folder, &trust_args).await?;

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
        capture_output: false,
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
        config_hash: None,
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
    use deacon_core::lifecycle::{InvocationMode, PhaseStatus};

    fn default_args() -> UpArgs {
        UpArgs::default()
    }

    #[test]
    fn test_wait_for_phase_defaults_to_update_content() {
        assert_eq!(wait_for_phase(None).unwrap(), LifecyclePhase::UpdateContent);
    }

    #[test]
    fn test_wait_for_phase_accepts_spec_values() {
        assert_eq!(
            wait_for_phase(Some("initializeCommand")).unwrap(),
            LifecyclePhase::Initialize
        );
        assert_eq!(
            wait_for_phase(Some("onCreateCommand")).unwrap(),
            LifecyclePhase::OnCreate
        );
        assert_eq!(
            wait_for_phase(Some("postAttachCommand")).unwrap(),
            LifecyclePhase::PostAttach
        );
    }

    #[test]
    fn test_wait_for_phase_rejects_invalid_value() {
        let err = wait_for_phase(Some("postCreate")).unwrap_err();
        assert!(err.to_string().contains("Invalid waitFor value"));
    }

    #[test]
    fn test_skip_non_blocking_queues_only_through_wait_for() {
        let wait_for = LifecyclePhase::UpdateContent;

        assert!(should_queue_phase_for_wait_for(
            true,
            wait_for,
            LifecyclePhase::OnCreate
        ));
        assert!(should_queue_phase_for_wait_for(
            true,
            wait_for,
            LifecyclePhase::UpdateContent
        ));
        assert!(!should_queue_phase_for_wait_for(
            true,
            wait_for,
            LifecyclePhase::PostCreate
        ));
    }

    #[test]
    fn test_wait_for_post_start_includes_dotfiles_boundary() {
        assert!(!should_run_dotfiles_for_wait_for(
            true,
            LifecyclePhase::PostCreate
        ));
        assert!(should_run_dotfiles_for_wait_for(
            true,
            LifecyclePhase::PostStart
        ));
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
