//! Run user-defined lifecycle commands implementation
//!
//! This module provides execution of lifecycle commands in an existing container
//! without going through the full `up` workflow.

use crate::commands::shared::{
    ConfigLoadArgs, ConfigLoadResult, canonical_reconnect_identity, load_config, resolve_runtime,
};
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
    /// Stop after `postCreateCommand`, before `postStartCommand` (#637).
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

/// How far the run got, reported as the `result` field of the success document.
///
/// The four values are the reference CLI's own (#635), and each corresponds to a
/// flag deacon already parses. They are not decoration: "stopped early because you
/// asked me to" and "ran everything" are both exit 0, so `result` is the ONLY thing
/// that tells a caller which one happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunOutcome {
    /// `--skip-non-blocking-commands` cut the run off at the `waitFor` phase.
    SkipNonBlocking,
    /// `--prebuild`: stops after `updateContentCommand`.
    Prebuild,
    /// `--stop-for-personalization`: stops after `postCreateCommand`.
    StopForPersonalization,
    /// Ran to the end.
    Done,
}

impl RunOutcome {
    fn as_str(self) -> &'static str {
        match self {
            RunOutcome::SkipNonBlocking => "skipNonBlocking",
            RunOutcome::Prebuild => "prebuild",
            RunOutcome::StopForPersonalization => "stopForPersonalization",
            RunOutcome::Done => "done",
        }
    }
}

/// Decide how far the run will get, from the flags and the configured `waitFor`.
///
/// This is a transcription of the reference CLI's own chain at 0.87.0, and the ORDER
/// is the whole content — each early stop sits at a specific point between two
/// phases, so a rearrangement changes the answer for flag combinations that reach
/// more than one checkpoint:
///
/// ```text
///   onCreate, updateContent          → --skip-non-blocking-commands cuts off here first
///   --prebuild stops here
///   postCreate                       → --skip-non-blocking-commands cuts off here
///   --stop-for-personalization stops here
///   postStart                        → --skip-non-blocking-commands cuts off here
///   postAttach                       → "done"
/// ```
///
/// It is a pure function of the four inputs BECAUSE the phases themselves cannot
/// change any of them, which is what makes the reported value and the phases that
/// actually run derivable from ONE decision — see the `postStart` gate at the call
/// site. Reporting `stopForPersonalization` while `postStart` had in fact run is
/// precisely the defect [#637](https://github.com/get2knowio/deacon/issues/637) is
/// about, and it can only reappear if these two are computed separately again.
fn run_outcome(
    skip_non_blocking_commands: bool,
    wait_for: LifecyclePhase,
    prebuild: bool,
    stop_for_personalization: bool,
) -> RunOutcome {
    let cuts_off_at = |phase| {
        skip_non_blocking_commands
            && !should_queue_phase_for_wait_for(skip_non_blocking_commands, wait_for, phase)
    };

    if cuts_off_at(LifecyclePhase::PostCreate) {
        // `waitFor` is initializeCommand, onCreateCommand or updateContentCommand:
        // the run stops before postCreate, ahead of --prebuild's own stop.
        RunOutcome::SkipNonBlocking
    } else if prebuild {
        RunOutcome::Prebuild
    } else if cuts_off_at(LifecyclePhase::PostStart) {
        RunOutcome::SkipNonBlocking
    } else if stop_for_personalization {
        RunOutcome::StopForPersonalization
    } else if cuts_off_at(LifecyclePhase::PostAttach) {
        RunOutcome::SkipNonBlocking
    } else {
        RunOutcome::Done
    }
}

/// The result document written to stdout on success (#635).
///
/// The reference CLI prints this on every terminating path and its own e2e suite
/// parses it; deacon printed nothing at all, on success and on failure alike.
#[derive(Debug, serde::Serialize)]
struct RunUserCommandsSuccess {
    outcome: &'static str,
    result: &'static str,
}

/// The result document written to stdout on failure (#635).
#[derive(Debug, serde::Serialize)]
struct RunUserCommandsErrorDocument {
    outcome: &'static str,
    message: String,
    description: String,
}

/// Render a failure as the result document.
///
/// Same split `build` settled on for [#594](https://github.com/get2knowio/deacon/issues/594):
/// `message` is the outermost context — what deacon was doing — and `description` is
/// the chain beneath it, where the actionable detail lives. The reference fills both
/// with the same sentence; deacon's chain carries more, so it goes in the field meant
/// for it rather than being thrown away.
fn error_document(err: &anyhow::Error) -> RunUserCommandsErrorDocument {
    let causes: Vec<String> = err.chain().skip(1).map(|c| c.to_string()).collect();
    let message = err.to_string();
    let description = if causes.is_empty() {
        message.clone()
    } else {
        causes.join(": ")
    };
    RunUserCommandsErrorDocument {
        outcome: "error",
        message,
        description,
    }
}

/// Execute the run-user-commands command, writing the result document to stdout.
///
/// Exactly one JSON document reaches stdout on every terminating path — success and
/// failure alike ([#635](https://github.com/get2knowio/deacon/issues/635)) — which is
/// what `up`, `set-up` and (since #594) `build` already do, and what the reference
/// CLI does here. Logs and diagnostics stay on stderr; the error is still propagated
/// so the binary boundary renders it there and exits 1.
///
/// Unconditional rather than gated on an output-format flag, because
/// `run-user-commands` has none — on either CLI.
#[instrument(skip(args, runtime))]
pub async fn execute_run_user_commands(
    args: RunUserCommandsArgs,
    runtime: Option<RuntimeKind>,
) -> Result<()> {
    match execute_run_user_commands_inner(args, runtime).await {
        Ok(outcome) => {
            let document = RunUserCommandsSuccess {
                outcome: "success",
                result: outcome.as_str(),
            };
            println!(
                "{}",
                serde_json::to_string(&document)
                    .context("Failed to serialize run-user-commands result")?
            );
            Ok(())
        }
        Err(err) => {
            if let Ok(json) = serde_json::to_string(&error_document(&err)) {
                println!("{}", json);
            }
            Err(err)
        }
    }
}

#[instrument(skip(args, runtime))]
async fn execute_run_user_commands_inner(
    args: RunUserCommandsArgs,
    runtime: Option<RuntimeKind>,
) -> Result<RunOutcome> {
    info!("Starting run-user-commands execution");

    // Select the runtime (docker/podman) honoring --runtime/DEACON_CONTAINER_RUNTIME.
    // Hardcoding CliDocker::new() here would talk to docker while the container
    // lives in podman → "Dev container not found" (mirrors the up/exec/down fix).
    let cli = resolve_runtime(runtime, &args.docker_path).cli_docker();

    // Container-only mode: a selector named the target and nothing named a
    // configuration, so there is no document to discover — the container's own
    // `devcontainer.metadata` label is the configuration, folded in further down by
    // `resolve_config_against_container` (#656).
    //
    // Discovering one from the current directory instead is what deacon used to do,
    // and it is wrong twice over. It FAILED outright when the cwd held no document —
    // `run-user-commands --container-id <id>` against a metadata-labelled container
    // exited 1 with `Configuration file not found`, where every sibling subcommand
    // (`set-up`, `read-configuration`, `exec`) already succeeded. And when the cwd
    // DID hold one it silently used it: measured at oracle 0.87.0 from a directory
    // with its own `postCreateCommand`, the reference ran the LABEL's hook and
    // ignored the cwd entirely.
    //
    // The gate is deliberately narrow. Without a container selector, a missing
    // document is still an error — `case-runusercommands-upstream-no-config-in-cwd-rejected`
    // pins that — and naming any of `--config` / `--override-config` /
    // `--workspace-folder` still means the caller chose a configuration.
    let container_only_mode = (args.container_id.is_some() || !args.id_label.is_empty())
        && args.config_path.is_none()
        && args.override_config_path.is_none()
        && args.workspace_folder.is_none();

    // Load configuration with override and secrets support via shared helper
    let (mut config, workspace_folder) = if container_only_mode {
        debug!("Container-only mode: taking the configuration from the container's metadata label");
        let cwd = std::env::current_dir().context(
            "Failed to resolve the current directory for container-only run-user-commands",
        )?;
        (DevContainerConfig::default(), cwd)
    } else {
        let ConfigLoadResult {
            config,
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
        (config, workspace_folder)
    };

    debug!("Loaded configuration with overrides and secrets support");

    // Lifecycle phase markers are keyed on the WORKSPACE hash alone, so `up` and
    // `run-user-commands` write into the same directory. Writing `None` here
    // stamped the markers as "legacy, no recorded hash" — which
    // `read_all_markers_for_config` treats as compatible with any config — and
    // silently erased `up`'s config-drift detection: a later
    // `up --override-config <changed>` created a fresh container and then SKIPPED
    // `postCreate` because the clobbered marker still claimed it had run (#372).
    //
    // The hash must be computed from the configuration **as loaded**, via the same
    // `canonical_reconnect_identity` contract that makes this command's container
    // lookup agree with `up`'s labels (#187) — i.e. BEFORE the host-CA
    // `containerEnv` injection below and BEFORE the image-metadata fold inside
    // `execute_lifecycle_commands`. Hashing a mutated config would produce a value
    // `up` can never reproduce, turning the erased-drift bug into a permanent
    // false-positive drift instead.
    let config_hash =
        canonical_reconnect_identity(workspace_folder.as_path(), &config, None, None).config_hash;
    debug!(
        config_hash = %config_hash,
        "Resolved config hash for lifecycle markers"
    );

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
                    .get_or_insert_default()
                    .entry(name.to_string())
                    .or_insert_with(|| bundle_path.clone());
            }
            debug!("Re-applied host-CA env vars from container labels (no re-discovery)");
        }
    }

    // Execute lifecycle commands
    let outcome = execute_lifecycle_commands(
        &container_id,
        &config,
        workspace_folder.as_path(),
        &args,
        &cli,
        &config_hash,
    )
    .await?;

    info!("Run-user-commands execution completed successfully");
    Ok(outcome)
}

/// Execute lifecycle commands in the container
///
/// `config_hash` is the hash of the configuration **as loaded** (see the call
/// site): it is stamped on every lifecycle marker this run writes so `up`'s
/// config-drift detection survives a `run-user-commands` invocation (#372).
#[instrument(skip(config, workspace_folder, args))]
async fn execute_lifecycle_commands(
    container_id: &str,
    config: &DevContainerConfig,
    workspace_folder: &Path,
    args: &RunUserCommandsArgs,
    cli: &CliRuntime,
    config_hash: &str,
) -> Result<RunOutcome> {
    info!("Executing lifecycle commands in container");

    // Create substitution context
    let substitution_context = SubstitutionContext::new(workspace_folder)?;

    // Determine the container workspace folder = the lifecycle cwd. Prefer the
    // RUNNING container's ACTUAL workspace bind-mount over host-side re-derivation
    // (`run-user-commands` doesn't expose `--mount-workspace-git-root`, so a
    // host-side guess disagrees with an `up --mount-workspace-git-root false`); a
    // Compose config without an explicit workspaceFolder resolves to `/` (the
    // reference default), not the single-container `/workspaces/<basename>` the
    // service never mounts (#294/#295).
    let container_info = {
        use deacon_core::docker::Docker;
        match cli.inspect_container(container_id).await {
            Ok(Some(info)) => Some(info),
            _ => None,
        }
    };
    let mounts = container_info
        .as_ref()
        .map(|info| info.mounts.clone())
        .unwrap_or_default();

    // #405: fold the running container's `devcontainer.metadata` label into the
    // workspace config, exactly as `exec` does — otherwise a `remoteUser`,
    // `remoteEnv` or lifecycle hook contributed only by that metadata is
    // silently ignored here while `up` and `exec` honor it. The reference CLI
    // 0.87.0 runs its user commands with all three applied (measured).
    //
    // The label is read off the CONTAINER inspect, not the image (#527), and
    // the composition it reports decides who owns the lifecycle record: on a
    // container carrying this workspace's identity labels the label IS the
    // record (image + Feature + config entries), so this pass must not also
    // resolve the Features itself.
    let (merged_config, metadata_composition) = match container_info.as_ref() {
        Some(info) => {
            let resolved =
                crate::commands::shared::container_metadata::resolve_config_against_container(
                    info,
                    config.clone(),
                    workspace_folder,
                );
            (resolved.config, resolved.composition)
        }
        None => (
            config.clone(),
            crate::commands::shared::container_metadata::MetadataComposition::Layered,
        ),
    };
    let config = &merged_config;

    // `None` means the target has no workspace at all — a container this deacon did
    // not create, named with `--container-id`. The reference's rule there is
    // `remoteCwd = remoteWorkspaceFolder || homeFolder`; deriving `/workspaces/<x>`
    // anyway made every hook fail with rc 127, and a non-blocking phase reported
    // success over it (#655).
    let resolved_workspace_folder =
        crate::commands::shared::resolve_container_cwd(config, workspace_folder, &mounts, true);
    let container_workspace_folder = match resolved_workspace_folder.clone() {
        Some(folder) => folder,
        None => {
            debug!(
                "Container has no workspace folder; using the container user's home as the lifecycle cwd"
            );
            deacon_core::container_env_probe::resolve_home_folder(
                cli,
                container_id,
                config
                    .remote_user
                    .as_deref()
                    .or(config.container_user.as_deref()),
                &config
                    .container_env()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            )
            .await
        }
    };

    // `remoteEnv` is layered over `containerEnv` for user commands, matching the
    // `up` flow (`resolve_env_and_user` → `build_effective_env`) and the
    // reference CLI. A `None` value means "set to the empty string" per the
    // spec's null handling.
    let mut lifecycle_env = config.container_env().clone();
    for (name, value) in config.remote_env() {
        lifecycle_env.insert(name.clone(), value.clone().unwrap_or_default());
    }

    // Create container lifecycle configuration
    let lifecycle_config = ContainerLifecycleConfig {
        capture_output: false,
        container_id: container_id.to_string(),
        user: config
            .remote_user
            .clone()
            .or_else(|| config.container_user.clone()),
        // `${containerWorkspaceFolder}` is the WORKSPACE, not the cwd — so it stays
        // absent when the target has none, leaving the token literal rather than
        // resolving it to a home directory that is not a workspace (#513 set the
        // contract; #655 is what first produced a cwd with no workspace behind it).
        substitution_workspace_folder: resolved_workspace_folder,
        container_workspace_folder,
        // Where the authored-order map (#394) stops, deliberately: the lifecycle
        // environment becomes `docker exec -e K=V` flags — order carries no meaning
        // there — and it is merged with the unordered userEnvProbe result anyway.
        // Nothing downstream of this point serializes the map, so the ordering the
        // label and the compose override depend on is not lost here. The map itself
        // is the #405 layering (remoteEnv over containerEnv) built above.
        container_env: lifecycle_env.into_iter().collect(),
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
        // #372: stamp the SAME hash `up` records. Markers are shared (keyed on the
        // workspace hash only), so a `None` here would erase `up`'s drift detection.
        config_hash: Some(config_hash.to_string()),
    };

    // Resolve declared features (fail-fast) so feature-contributed lifecycle
    // commands are aggregated alongside the config's, matching `up`. Local
    // feature paths (`./`, `../`, `/abs`) resolve relative to the config's
    // directory. If a declared feature cannot be resolved (missing local path,
    // OCI fetch error, dependency cycle), we propagate the error rather than
    // silently running a partial set of hooks.
    //
    // SKIPPED entirely on a container whose metadata label is the complete
    // record (#527): that label already carries one entry per INSTALLED
    // Feature, so resolving the declared set here would queue every Feature
    // hook a second time. The reference skips it for the same reason — its
    // complete-record branch never touches `featuresConfig`. Skipping the
    // resolution also skips its fail-fast, which is correct on this path: the
    // Features are already installed, so what the config declares NOW cannot
    // change what runs.
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
    let resolved_features = if metadata_composition.suppresses_caller_features() {
        debug!(
            "Container metadata label is the complete lifecycle record; \
             skipping Feature resolution so its hooks are not queued twice (#527)"
        );
        Vec::new()
    } else {
        let fetcher = deacon_core::oci::default_fetcher()
            .context("Failed to initialize OCI feature fetcher")?;
        let resolved = crate::commands::shared::feature_resolver::resolve_features_ordered(
            config,
            &config_dir,
            workspace_folder,
            &fetcher,
        )
        .await
        .context("Failed to resolve features for lifecycle command aggregation")?;
        if !resolved.is_empty() {
            debug!(
                feature_count = resolved.len(),
                "Aggregating feature-contributed lifecycle commands"
            );
        }
        resolved
    };

    // Build lifecycle commands: feature-contributed commands (in install order)
    // first, then the config's command, per `aggregate_lifecycle_commands` —
    // identical to the `up` flow.
    let mut commands = ContainerLifecycleCommands::new();

    // initializeCommand is intentionally omitted: it is a host-side command that
    // runs before container creation and belongs only to the `up` workflow.
    let wait_for = wait_for_phase(config.wait_for.as_deref())?;

    // How far this run gets. Computed ONCE, here, and used for two things: the
    // `result` field of the success document (#635) and the `postStart` gate below
    // (#637). Deriving both from one decision is what keeps them from disagreeing —
    // before this, `--stop-for-personalization` was parsed and dropped, so `postStart`
    // and `postAttach` ran anyway and nothing a caller could read said so.
    let outcome = run_outcome(
        args.skip_non_blocking_commands,
        wait_for,
        args.prebuild,
        args.stop_for_personalization,
    );

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
    // Also skipped in prebuild mode (stops after updateContent), and under
    // `--stop-for-personalization`, which stops the run right here so dotfiles /
    // personalization can be applied before the attach-time hooks fire (#637).
    if outcome != RunOutcome::StopForPersonalization
        && !args.prebuild
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
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every cell of the reference's chain, and the ORDER between the early stops is
    /// what the table is really pinning: three of these rows have two flags asking to
    /// stop at different points, and only one of the two answers is the reference's.
    #[test]
    fn run_outcome_transcribes_the_references_chain() {
        use LifecyclePhase::*;
        use RunOutcome::*;

        // (skip_non_blocking, waitFor, prebuild, stop_for_personalization) -> result
        let table: &[(bool, LifecyclePhase, bool, bool, RunOutcome)] = &[
            // Nothing asked for an early stop.
            (false, UpdateContent, false, false, Done),
            (false, PostAttach, false, false, Done),
            // One flag at a time.
            (true, Initialize, false, false, SkipNonBlocking),
            (true, OnCreate, false, false, SkipNonBlocking),
            (true, UpdateContent, false, false, SkipNonBlocking),
            (true, PostCreate, false, false, SkipNonBlocking),
            (true, PostStart, false, false, SkipNonBlocking),
            (false, UpdateContent, true, false, Prebuild),
            (false, UpdateContent, false, true, StopForPersonalization),
            // `waitFor: postAttachCommand` cuts nothing off — there is no checkpoint
            // after postStart, so the run reaches the end and reports `done`.
            (true, PostAttach, false, false, Done),
            // Two flags, different stopping points. `--skip-non-blocking-commands`
            // wins over `--prebuild` only when it cuts off EARLIER than prebuild does.
            (true, UpdateContent, true, false, SkipNonBlocking),
            (true, PostCreate, true, false, Prebuild),
            (true, PostStart, true, false, Prebuild),
            // `--prebuild` stops before personalization is ever reached.
            (false, UpdateContent, true, true, Prebuild),
            // `--stop-for-personalization` sits after postCreate, so a skip that cuts
            // off at or before postCreate wins; one that cuts off at postStart loses.
            (true, UpdateContent, false, true, SkipNonBlocking),
            (true, PostCreate, false, true, SkipNonBlocking),
            (true, PostStart, false, true, StopForPersonalization),
        ];

        for &(skip, wait_for, prebuild, personalize, expected) in table {
            assert_eq!(
                run_outcome(skip, wait_for, prebuild, personalize),
                expected,
                "skip_non_blocking={skip}, waitFor={wait_for:?}, prebuild={prebuild}, \
                 stop_for_personalization={personalize}"
            );
        }
    }

    /// The success document is the reference's, field for field (#635).
    #[test]
    fn success_document_matches_the_reference_shape() {
        let document = RunUserCommandsSuccess {
            outcome: "success",
            result: RunOutcome::Done.as_str(),
        };
        assert_eq!(
            serde_json::to_string(&document).unwrap(),
            r#"{"outcome":"success","result":"done"}"#
        );
    }

    /// A failure document carries the outermost context as `message` and the cause
    /// chain as `description`; with no chain to draw on, both say the same thing
    /// rather than one of them being empty (which is what the reference emits).
    #[test]
    fn error_document_splits_the_cause_chain() {
        let bare = error_document(&anyhow::anyhow!("Dev container not found."));
        assert_eq!(bare.outcome, "error");
        assert_eq!(bare.message, "Dev container not found.");
        assert_eq!(bare.description, "Dev container not found.");

        let chained = error_document(
            &anyhow::anyhow!("Configuration file not found: /ws/.devcontainer/devcontainer.json")
                .context("Configuration error"),
        );
        assert_eq!(chained.message, "Configuration error");
        assert_eq!(
            chained.description,
            "Configuration file not found: /ws/.devcontainer/devcontainer.json"
        );
    }

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
