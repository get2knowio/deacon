//! Run user-defined lifecycle commands implementation
//!
//! This module provides execution of lifecycle commands in an existing container
//! without going through the full `up` workflow.

use crate::commands::shared::{ConfigLoadArgs, ConfigLoadResult, load_config, resolve_runtime};
use anyhow::{Context, Result};
use deacon_core::config::DevContainerConfig;
use deacon_core::container_lifecycle::{
    ContainerLifecycleCommands, ContainerLifecycleConfig, LifecycleCommandList,
    aggregate_lifecycle_commands, execute_container_lifecycle_with_progress_callback_and_docker,
};
use deacon_core::docker::CliRuntime;
use deacon_core::lifecycle::{LifecyclePhase, should_queue_phase_for_wait_for, wait_for_phase};
use deacon_core::runtime::RuntimeKind;
use deacon_core::variable::SubstitutionContext;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, info, instrument};

use crate::commands::exec::resolve_target_container;

/// Arguments for the run-user-commands command
#[derive(Debug, Clone)]
pub struct RunUserCommandsArgs {
    pub skip_post_create: bool,
    pub skip_post_attach: bool,
    pub skip_non_blocking_commands: bool,
    pub prebuild: bool,
    #[allow(dead_code)] // Future feature: stop for personalization
    pub stop_for_personalization: bool,
    /// When set, target this container directly; skips workspace-based discovery.
    pub container_id: Option<String>,
    /// When non-empty, resolve target container by matching these `key=value` labels;
    /// takes precedence over workspace-based discovery but yields to `container_id`.
    pub id_label: Vec<String>,
    pub workspace_folder: Option<std::path::PathBuf>,
    pub config_path: Option<std::path::PathBuf>,
    pub override_config_path: Option<std::path::PathBuf>,
    /// CLI `--merge-config` fragments deep-overlaid on the base (highest layer)
    pub cli_merge_paths: Vec<std::path::PathBuf>,
    pub secrets_files: Vec<std::path::PathBuf>,
    pub progress_tracker: Arc<Mutex<Option<deacon_core::progress::ProgressTracker>>>,
    pub docker_path: String,
    pub container_data_folder: Option<std::path::PathBuf>,
    /// Host user-data folder (`--user-data-folder`); `None` → `~/.deacon`.
    /// Roots lifecycle markers outside the project (#280).
    pub user_data_folder: Option<std::path::PathBuf>,
}

/// Execute the run-user-commands command
#[instrument(skip(args, runtime))]
pub async fn execute_run_user_commands(
    args: RunUserCommandsArgs,
    runtime: Option<RuntimeKind>,
) -> Result<()> {
    info!("Starting run-user-commands execution");

    // Select the runtime (docker/podman) honoring --runtime/DEACON_CONTAINER_RUNTIME.
    // Hardcoding CliDocker::new() here would talk to docker while the container
    // lives in podman → "Dev container not found" (mirrors the up/exec/down fix).
    let cli = resolve_runtime(runtime, &args.docker_path).cli_docker();

    // Load configuration with override and secrets support via shared helper
    let ConfigLoadResult {
        mut config,
        workspace_folder,
        ..
    } = load_config(ConfigLoadArgs {
        workspace_folder: args.workspace_folder.as_deref(),
        config_path: args.config_path.as_deref(),
        settings_merge_paths: &[],
        cli_merge_paths: &args.cli_merge_paths,
        override_config_path: args.override_config_path.as_deref(),
        secrets_files: &args.secrets_files,
        resolve_devcontainer_id: true,
    })
    .await?;

    debug!("Loaded configuration with overrides and secrets support");

    let container_id = {
        let docker_client = cli.clone();

        // Container selection precedence (matches `exec`):
        // 1. --container-id (direct lookup)
        // 2. --id-label (label-based lookup)
        // 3. workspace-based discovery
        if args.container_id.is_some() || !args.id_label.is_empty() {
            use deacon_core::container::{ContainerSelector, resolve_container};

            let selector = ContainerSelector::new(
                args.container_id.clone(),
                args.id_label.clone(),
                args.workspace_folder.clone(),
                args.override_config_path.clone(),
            )?;
            selector.validate()?;

            match resolve_container(&docker_client, &selector).await? {
                Some(info) => {
                    if info.state != "running" {
                        return Err(anyhow::anyhow!("Dev container is not running."));
                    }
                    info.id
                }
                None => {
                    return Err(anyhow::anyhow!("Dev container not found."));
                }
            }
        } else {
            // Compose files resolve relative to the directory containing
            // devcontainer.json, not the workspace folder (spec parity).
            let target_config_dir = match args.config_path.as_deref() {
                Some(cfg) if cfg.is_dir() => cfg.to_path_buf(),
                Some(cfg) => cfg
                    .parent()
                    .unwrap_or(workspace_folder.as_path())
                    .to_path_buf(),
                None => {
                    let dc = workspace_folder.join(".devcontainer");
                    if dc.is_dir() {
                        dc
                    } else {
                        workspace_folder.to_path_buf()
                    }
                }
            };
            match resolve_target_container(
                &docker_client,
                workspace_folder.as_path(),
                &config,
                &target_config_dir,
                None,
                &args.docker_path,
                &[],
            )
            .await
            {
                Ok(id) => id,
                Err(e) => {
                    debug!(error = ?e, "Failed to resolve target container for workspace");
                    return Err(anyhow::anyhow!(
                        "No running container found. Run 'deacon up' first"
                    ));
                }
            }
        }
    };

    info!("Found target container: {}", container_id);

    // Host-CA reconnect (016, T033): re-apply the six CA env vars from the
    // container's `devcontainer.deacon.hostCaBundlePath` label (no re-discovery,
    // no activation re-resolve) into containerEnv, insert-if-absent so user
    // values win. Mirrors `exec`.
    {
        if let Some(bundle_path) =
            crate::commands::shared::host_ca::read_host_ca_bundle_path(&cli, &container_id).await
        {
            for name in deacon_core::host_ca::CA_ENV_VARS {
                config
                    .container_env
                    .entry(name.to_string())
                    .or_insert_with(|| bundle_path.clone());
            }
            debug!("Re-applied host-CA env vars from container labels (no re-discovery)");
        }
    }

    // Execute lifecycle commands
    execute_lifecycle_commands(
        &container_id,
        &config,
        workspace_folder.as_path(),
        &args,
        &cli,
    )
    .await?;

    info!("Run-user-commands execution completed successfully");
    Ok(())
}

/// Execute lifecycle commands in the container
#[instrument(skip(config, workspace_folder, args))]
async fn execute_lifecycle_commands(
    container_id: &str,
    config: &DevContainerConfig,
    workspace_folder: &Path,
    args: &RunUserCommandsArgs,
    cli: &CliRuntime,
) -> Result<()> {
    info!("Executing lifecycle commands in container");

    // Create substitution context
    let substitution_context = SubstitutionContext::new(workspace_folder)?;

    // Determine the container workspace folder = the lifecycle cwd. Prefer the
    // RUNNING container's ACTUAL workspace bind-mount over host-side re-derivation:
    // `run-user-commands` doesn't expose `--mount-workspace-git-root`, so a
    // host-side guess (previously hardcoded to the git-root default) disagrees with
    // an `up --mount-workspace-git-root false` and the lifecycle `chdir` fails.
    // Reading the mount reflects exactly where `up` mounted, regardless of flags.
    // Fall back to host-side derivation when the mount can't be read.
    let container_workspace_folder = {
        use deacon_core::docker::Docker;
        let from_mount = match cli.inspect_container(container_id).await {
            Ok(Some(info)) => crate::commands::shared::container_workspace_folder_from_mounts(
                config,
                workspace_folder,
                &info.mounts,
            ),
            _ => None,
        };
        from_mount.unwrap_or_else(|| {
            crate::commands::shared::derive_container_workspace_folder(
                config,
                workspace_folder,
                true,
            )
        })
    };

    // Create container lifecycle configuration
    let lifecycle_config = ContainerLifecycleConfig {
        capture_output: false,
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
        cache_folder: args.container_data_folder.clone(),
        user_data_folder: args.user_data_folder.clone(),
        // Per FR-006: force_pty toggle only applies to 'up' workflow lifecycle exec,
        // not to run-user-commands which is a separate entry point
        force_pty: false,
        // run-user-commands does not install dotfiles - that is handled by `up` command
        dotfiles: None,
        is_prebuild: args.prebuild,
        config_hash: None,
    };

    // Resolve declared features (fail-fast) so feature-contributed lifecycle
    // commands are aggregated alongside the config's, matching `up`. Local
    // feature paths (`./`, `../`, `/abs`) resolve relative to the config's
    // directory. If a declared feature cannot be resolved (missing local path,
    // OCI fetch error, dependency cycle), we propagate the error rather than
    // silently running a partial set of hooks.
    let config_dir = if let Some(cfg) = args.config_path.as_deref() {
        if cfg.is_dir() {
            cfg.to_path_buf()
        } else {
            cfg.parent().unwrap_or(workspace_folder).to_path_buf()
        }
    } else {
        let dc = workspace_folder.join(".devcontainer");
        if dc.is_dir() {
            dc
        } else {
            workspace_folder.to_path_buf()
        }
    };
    let fetcher =
        deacon_core::oci::default_fetcher().context("Failed to initialize OCI feature fetcher")?;
    let resolved_features = crate::commands::shared::feature_resolver::resolve_features_ordered(
        config,
        &config_dir,
        &fetcher,
    )
    .await
    .context("Failed to resolve features for lifecycle command aggregation")?;
    if !resolved_features.is_empty() {
        debug!(
            feature_count = resolved_features.len(),
            "Aggregating feature-contributed lifecycle commands"
        );
    }

    // Build lifecycle commands: feature-contributed commands (in install order)
    // first, then the config's command, per `aggregate_lifecycle_commands` —
    // identical to the `up` flow.
    let mut commands = ContainerLifecycleCommands::new();

    // initializeCommand is intentionally omitted: it is a host-side command that
    // runs before container creation and belongs only to the `up` workflow.
    let wait_for = wait_for_phase(config.wait_for.as_deref())?;

    // Aggregate a phase's commands (features + config); `None` when empty.
    let aggregate = |phase: LifecyclePhase| -> Result<Option<LifecycleCommandList>> {
        let list = aggregate_lifecycle_commands(phase, &resolved_features, config)?;
        Ok((!list.commands.is_empty()).then_some(list))
    };

    // Phase 1: onCreate (container)
    if should_queue_phase_for_wait_for(
        args.skip_non_blocking_commands,
        wait_for,
        LifecyclePhase::OnCreate,
    ) {
        if let Some(list) = aggregate(LifecyclePhase::OnCreate)? {
            commands = commands.with_on_create(list);
        }
    }

    // Phase 2: updateContent (container)
    if should_queue_phase_for_wait_for(
        args.skip_non_blocking_commands,
        wait_for,
        LifecyclePhase::UpdateContent,
    ) {
        if let Some(list) = aggregate(LifecyclePhase::UpdateContent)? {
            commands = commands.with_update_content(list);
        }
    }

    // Phase 3: postCreate (container, can be skipped)
    // In prebuild mode the run stops after updateContent (postCreate, dotfiles,
    // postStart, postAttach are all skipped — see InvocationMode::Prebuild in
    // core::lifecycle and the `up` parity path), so gate postCreate onward on
    // `!args.prebuild` in addition to `--skip-post-create`.
    if !args.skip_post_create
        && !args.prebuild
        && should_queue_phase_for_wait_for(
            args.skip_non_blocking_commands,
            wait_for,
            LifecyclePhase::PostCreate,
        )
    {
        if let Some(list) = aggregate(LifecyclePhase::PostCreate)? {
            commands = commands.with_post_create(list);
        }
    }

    // Phase 4: postStart (container, non-blocking, can be skipped).
    // Also skipped in prebuild mode (stops after updateContent).
    if !args.prebuild
        && should_queue_phase_for_wait_for(
            args.skip_non_blocking_commands,
            wait_for,
            LifecyclePhase::PostStart,
        )
    {
        if let Some(list) = aggregate(LifecyclePhase::PostStart)? {
            commands = commands.with_post_start(list);
        }

        // Phase 5: postAttach (container, non-blocking, can be skipped)
        if !args.skip_post_attach
            && should_queue_phase_for_wait_for(
                args.skip_non_blocking_commands,
                wait_for,
                LifecyclePhase::PostAttach,
            )
        {
            if let Some(list) = aggregate(LifecyclePhase::PostAttach)? {
                commands = commands.with_post_attach(list);
            }
        }
    }

    // Execute lifecycle commands with progress callback, against the SELECTED
    // runtime (docker/podman) rather than a hardcoded docker client.
    let result = execute_container_lifecycle_with_progress_callback_and_docker(
        &lifecycle_config,
        &commands,
        &substitution_context,
        cli,
        Some(crate::commands::shared::progress::make_progress_callback(
            &args.progress_tracker,
        )),
    )
    .await;

    // Return result
    let result = result?;

    debug!(
        "User commands execution completed: {} blocking phases executed, {} non-blocking phases queued",
        result.phases.len(),
        result.non_blocking_phases.len()
    );

    // #73: actually execute the non-blocking phases (postStart, postAttach)
    // inside the container — not just log that we "would". The upstream
    // reference CLI fires both phases before returning, so any flag/file
    // side effects must be observable to the next `docker exec`. Previously
    // deacon stopped at the log line and the side effects never landed.
    //
    // Phases are filtered to queue-or-skip *before* execution based on
    // --skip-non-blocking-commands (see should_queue_phase_for_wait_for
    // above), so an empty `non_blocking_phases` here means we have nothing
    // to do.
    if !result.non_blocking_phases.is_empty() {
        debug!(
            "Executing {} non-blocking phase(s) synchronously",
            result.non_blocking_phases.len()
        );
        result
            .execute_non_blocking_phases_sync_with_callback(
                cli,
                Some(crate::commands::shared::progress::make_progress_callback(
                    &args.progress_tracker,
                )),
            )
            .await
            .context("Non-blocking lifecycle phase execution failed")?;
    }

    info!("Lifecycle commands execution completed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_user_commands_args_defaults() {
        // For this simple args test, we don't need a real tracker.
        // Use None to avoid filesystem side effects from progress cache initialization.
        let progress_tracker: Option<deacon_core::progress::ProgressTracker> = None;
        let progress_tracker = std::sync::Arc::new(std::sync::Mutex::new(progress_tracker));

        let args = RunUserCommandsArgs {
            skip_post_create: false,
            skip_post_attach: false,
            skip_non_blocking_commands: false,
            prebuild: false,
            stop_for_personalization: false,
            container_id: None,
            id_label: vec![],
            workspace_folder: None,
            config_path: None,
            override_config_path: None,
            cli_merge_paths: vec![],
            secrets_files: vec![],
            progress_tracker,
            docker_path: "docker".to_string(),
            container_data_folder: None,
            user_data_folder: None,
        };

        assert!(!args.skip_post_create);
        assert!(!args.skip_non_blocking_commands);
        assert!(!args.prebuild);
    }

    /// Confirms the new container-selection fields round-trip through the args
    /// struct. The functional precedence (container_id > id_label > workspace)
    /// is exercised end-to-end by the smoke_run_user_commands suite.
    #[test]
    fn test_run_user_commands_args_container_selection_fields() {
        let progress_tracker: Option<deacon_core::progress::ProgressTracker> = None;
        let progress_tracker = std::sync::Arc::new(std::sync::Mutex::new(progress_tracker));

        let args = RunUserCommandsArgs {
            skip_post_create: false,
            skip_post_attach: false,
            skip_non_blocking_commands: false,
            prebuild: false,
            stop_for_personalization: false,
            container_id: Some("deadbeef".to_string()),
            id_label: vec!["devcontainer.local_folder=/x".to_string()],
            workspace_folder: None,
            config_path: None,
            override_config_path: None,
            cli_merge_paths: vec![],
            secrets_files: vec![],
            progress_tracker,
            docker_path: "docker".to_string(),
            container_data_folder: None,
            user_data_folder: None,
        };

        assert_eq!(args.container_id.as_deref(), Some("deadbeef"));
        assert_eq!(args.id_label.len(), 1);
    }
}
