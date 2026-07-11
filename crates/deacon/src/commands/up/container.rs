//! Single container flow for the up command.
//!
//! This module contains:
//! - `execute_container_up` - Main single container up execution
//! - `handle_container_shutdown` - Shutdown handling for single container

use super::args::UpArgs;
use super::features_build::build_image_with_features;
use super::helpers::{apply_user_mapping, handle_lockfile_post_build};
use super::lifecycle::{HostTrustArgs, execute_initialize_command, execute_lifecycle_commands};
use super::merged_config::{
    build_merged_configuration_with_options, inspect_for_merged_configuration,
    merge_image_metadata_after_image_ready,
};
use super::ports::handle_container_port_events;
use super::result::UpContainerInfo;
use crate::commands::shared::resolve_env_and_user;
use anyhow::{Context, Result};
use deacon_core::IndexMap;
use deacon_core::build::BuildOptions;
use deacon_core::config::DevContainerConfig;
use deacon_core::container::ContainerIdentity;
use deacon_core::docker::{Docker, DockerLifecycle};
use deacon_core::errors::{DeaconError, DockerError};
use deacon_core::features::{
    build_entrypoint_chain, generate_wrapper_script, merge_security_options,
};
use deacon_core::host_ca::{
    CorporateCaSet, HOST_CA_BUNDLE_PATH, apply_ca_env_vars, inject_runtime,
};
use deacon_core::mount::merge_mounts;
use deacon_core::runtime::ContainerRuntimeImpl;
use deacon_core::state::{ContainerState, StateManager};
use std::path::{Path, PathBuf};
use tracing::{debug, info, instrument, warn};

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
pub(crate) async fn execute_container_up(
    config: &DevContainerConfig,
    identity: &ContainerIdentity,
    workspace_folder: &Path,
    args: &UpArgs,
    state_manager: &mut StateManager,
    workspace_hash: &str,
    cli_remote_env: &IndexMap<String, String>,
    runtime: &ContainerRuntimeImpl,
    config_path: &Path,
    cache_folder: &Option<PathBuf>,
    build_options: &BuildOptions,
    host_ca_set: Option<&CorporateCaSet>,
) -> Result<UpContainerInfo> {
    debug!("Starting traditional development container");

    // Merge CLI forward_ports into config
    let mut config = config.clone();

    // Host-CA injection (016, T028): synthesize the six CA env vars into the
    // container environment at create time, insert-if-absent so user
    // containerEnv values win (FR-024). The canonical bundle is written by the
    // runtime install step below; these vars point at it.
    if host_ca_set.is_some() {
        apply_ca_env_vars(&mut config.container_env, HOST_CA_BUNDLE_PATH);
    }

    // Warn if workspace_mount_consistency is specified but workspace_mount is already defined
    if config.workspace_mount.is_some() && args.workspace_mount_consistency.is_some() {
        tracing::warn!(
            "workspace_mount_consistency specified but workspace_mount is already defined in config; CLI option will be ignored"
        );
    }

    // Apply workspace mount consistency when using default workspace mount.
    //
    // We keep the behavior tight: only inject `config.workspace_mount` here
    // when the user explicitly requested a consistency mode. Container
    // identity (computed later) hashes the entire config — so injecting a
    // mount string here that depends on host paths would change the
    // workspace+config hash and break `deacon up` ↔ `deacon exec`
    // reconnection. The default mount (without consistency) is built later
    // by `Docker::create_container` from the `workspace_path` argument; #67
    // re-anchors that argument to the git root via `workspace_mount_source`
    // below.
    if config.workspace_mount.is_none() {
        if let Some(ref consistency) = args.workspace_mount_consistency {
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
                .with_context(|| {
                    format!(
                        "Failed to canonicalize workspace folder '{}' for mounting: path does not exist or cannot be accessed",
                        workspace_folder.display()
                    )
                })?
                .display()
                .to_string();
            config.workspace_mount = Some(format!(
                "type=bind,source={},target={},consistency={}",
                source_path, target_path, consistency
            ));
        }
    }

    // Spec parity (#67): the workspace mount *source* is the enclosing git
    // root when `--mount-workspace-git-root=true` (default), so git
    // operations inside the container work even when the user passes a
    // sub-project directory as `--workspace-folder`. Discovery already used
    // the user's path (above, in `load_config`) — this only affects the
    // bind mount source. When git-root walking is disabled, fall back to
    // the canonicalized workspace folder.
    let workspace_mount_source: PathBuf = if args.mount_workspace_git_root {
        if deacon_core::workspace::find_git_repository_root(workspace_folder)?.is_none() {
            info!(
                "Git root requested (--mount-workspace-git-root) but no git repository found at '{}'. Using workspace folder for the workspace mount source.",
                workspace_folder.display()
            );
        }
        deacon_core::workspace::resolve_workspace_root(workspace_folder)?
    } else {
        workspace_folder.canonicalize().with_context(|| {
            format!(
                "Failed to canonicalize workspace folder '{}' for mounting: path does not exist or cannot be accessed",
                workspace_folder.display()
            )
        })?
    };

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
    let emit_progress_event =
        crate::commands::shared::progress::make_progress_callback(&args.progress_tracker);

    // Container identity (deterministic naming + labels, incl. the
    // spec-mandated `devcontainer.config_file` label, #68) is computed by the
    // caller from the as-loaded config and passed in, so the stamped
    // `devcontainer.configHash` is reproducible by `exec`/`run-user-commands`
    // (#187). It must NOT be recomputed from the post-mutation `config` here
    // (CLI `--mount`, forward-ports, and the Dockerfile-built image would all
    // perturb the hash and break `up` ↔ `exec` reconnection).
    debug!("Container identity: {:?}", identity);

    // Initialize Docker client
    let docker = runtime;

    // Execute initializeCommand on host before any container operations
    if let Some(ref initialize) = config.initialize_command {
        let trust_args = HostTrustArgs {
            trust_workspace: args.trust_workspace,
            trust_workspace_persist: args.trust_workspace_persist,
            user_data_folder: args.user_data_folder.as_deref(),
        };
        execute_initialize_command(
            initialize,
            workspace_folder,
            &args.progress_tracker,
            trust_args,
        )
        .await?;
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
    let resolved_features = if config
        .features
        .as_object()
        .map(|o| !o.is_empty())
        .unwrap_or(false)
    {
        info!("Features detected in configuration - building feature-extended image with BuildKit");

        // Pass build_options to propagate cache-from/cache-to/buildx settings per spec (data-model.md).
        // `config_path` anchors local feature references (#69). Pause the
        // interactive spinner for the build so its streaming renderer owns stderr
        // (otherwise the steady-tick spinner clobbers the build progress).
        let feature_build = {
            let _pause =
                crate::commands::shared::progress::SpinnerPause::new(&args.progress_tracker);
            build_image_with_features(
                &config,
                identity,
                workspace_folder,
                config_path,
                Some(build_options),
                host_ca_set,
                &runtime.cli_docker(),
            )
            .await
            .with_context(|| "Failed to build feature-extended image")?
        };

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

        // Lockfile graduation (PR-4b): write the freshly-built lockfile to disk
        // (or byte-compare it in `--frozen-lockfile` mode). Runs only when
        // features were actually resolved; with no features there is nothing
        // to lock.
        handle_lockfile_post_build(args, config_path, &feature_build.lockfile).await?;

        Some(feature_build.resolved_features)
    } else {
        None
    };

    // Spec parity (#70): once the base/feature image is locally available,
    // merge its `devcontainer.metadata` LABEL into the user config as the
    // lower-precedence layer so containerEnv / remoteUser / lifecycle
    // entries baked into the image actually contribute to the final
    // configuration. For plain `image:` configs without features, the image
    // may not be local yet — the helper falls through cleanly in that case
    // and Docker will still apply the image's own ENV/USER instructions at
    // container run time.
    if let Some(image_ref) = config.image.clone() {
        config = merge_image_metadata_after_image_ready(docker, &image_ref, config).await;
    }

    // Merge security options from config and features
    debug!("Merging security options from config and features");
    let merged_security =
        merge_security_options(&config, resolved_features.as_deref().unwrap_or(&[]));
    debug!(
        privileged = merged_security.privileged,
        init = merged_security.init,
        cap_add_count = merged_security.cap_add.len(),
        security_opt_count = merged_security.security_opt.len(),
        "Merged security options from config and features"
    );
    if !merged_security.cap_add.is_empty() {
        debug!(
            cap_add = ?merged_security.cap_add,
            "Merged capabilities"
        );
    }
    if !merged_security.security_opt.is_empty() {
        debug!(
            security_opt = ?merged_security.security_opt,
            "Merged security options"
        );
    }

    // Merge mounts from config and features
    let feature_mount_count: usize = resolved_features
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|f| f.metadata.mounts.len())
        .sum();
    let config_mount_count = config.mounts.len();

    debug!(
        feature_mounts = feature_mount_count,
        config_mounts = config_mount_count,
        "Merging mounts from config and features"
    );

    // Per #122, feature mounts must run through the same substitution
    // context the config uses so tokens like `${devcontainerId}` resolve
    // before docker sees them. Build a context anchored to the user's
    // workspace folder; `devcontainerId` is the spec-defined hash of the
    // container's id-labels (matches read-configuration's path at
    // `read_configuration.rs:1185`).
    let mount_substitution_context = {
        let mut ctx = deacon_core::variable::SubstitutionContext::new(workspace_folder)?;
        let id_labels: Vec<(String, String)> = identity.labels().into_iter().collect();
        ctx.devcontainer_id = deacon_core::container::compute_dev_container_id(&id_labels);
        ctx
    };
    let merged_mounts = merge_mounts(
        &config.mounts,
        resolved_features.as_deref().unwrap_or(&[]),
        Some(&mount_substitution_context),
    )
    .with_context(|| "Failed to merge mounts from config and features")?;

    info!(
        feature_mounts = feature_mount_count,
        config_mounts = config_mount_count,
        merged_mount_count = merged_mounts.mounts.len(),
        "Merged mounts from config and features"
    );

    if !merged_mounts.mounts.is_empty() {
        debug!(
            mounts = ?merged_mounts.mounts,
            "Merged mount specifications"
        );
    }

    // Build entrypoint chain from features and config
    // DevContainerConfig does not currently have an entrypoint field; pass None for config entrypoint.
    //
    // Per #124, feature entrypoints may legally reference `${devcontainerId}`
    // etc.; substitute before chaining so the Docker `--entrypoint` arg
    // doesn't see literal `${...}`.
    let features_owned: Vec<deacon_core::features::ResolvedFeature>;
    let features_slice: &[deacon_core::features::ResolvedFeature] =
        if let Some(features) = resolved_features.as_deref() {
            let mut report = deacon_core::variable::SubstitutionReport::new();
            features_owned = features
                .iter()
                .map(|f| {
                    let mut new_f = f.clone();
                    if let Some(ref ep) = f.metadata.entrypoint {
                        new_f.metadata.entrypoint = Some(
                            deacon_core::variable::VariableSubstitution::substitute_string(
                                ep,
                                &mount_substitution_context,
                                &mut report,
                            ),
                        );
                    }
                    new_f
                })
                .collect();
            &features_owned
        } else {
            &[]
        };
    let entrypoint_chain = build_entrypoint_chain(features_slice, None);

    // T044: Log entrypoint chain decision
    match &entrypoint_chain {
        deacon_core::features::EntrypointChain::None => {
            debug!(
                feature_count = features_slice.len(),
                "No entrypoints found in features or config"
            );
        }
        deacon_core::features::EntrypointChain::Single(path) => {
            info!(
                entrypoint = %path,
                feature_count = features_slice.len(),
                "Single entrypoint from features, no wrapper needed"
            );
        }
        deacon_core::features::EntrypointChain::Chained {
            wrapper_path,
            entrypoints,
        } => {
            info!(
                wrapper_path = %wrapper_path,
                entrypoint_count = entrypoints.len(),
                feature_count = features_slice.len(),
                "Multiple entrypoints detected, wrapper script required"
            );
            for (i, ep) in entrypoints.iter().enumerate() {
                debug!(index = i, entrypoint = %ep, "Chained entrypoint");
            }
        }
    }

    // For the Chained variant, generate the wrapper script and write it to a persistent
    // location on the host, then add a bind mount so it is available inside the container.
    // The script lives in the host user-data folder
    // (`~/.deacon/entrypoints/<workspace_hash>/`) — keyed by the stable workspace hash so
    // the bind-mounted path survives container restarts — rather than inside the project
    // (`<workspace>/.devcontainer/.deacon/`), so `up` leaves no stray files in the repo (#280).
    let mut merged_mounts = merged_mounts;
    if let deacon_core::features::EntrypointChain::Chained {
        ref wrapper_path,
        ref entrypoints,
    } = entrypoint_chain
    {
        let script_content = generate_wrapper_script(entrypoints);
        debug!(
            wrapper_path = %wrapper_path,
            script_length = script_content.len(),
            "Generated entrypoint wrapper script"
        );

        // Write wrapper script to a persistent location in the user-data folder so the
        // bind-mounted path remains valid across container restarts.
        let wrapper_dir = deacon_core::trust::user_data_root(args.user_data_folder.as_deref())
            .context("Failed to resolve user-data folder for entrypoint wrapper")?
            .join("entrypoints")
            .join(&identity.workspace_hash);
        tokio::fs::create_dir_all(&wrapper_dir)
            .await
            .context("Failed to create entrypoints directory for entrypoint wrapper")?;

        let wrapper_host_path = wrapper_dir.join("entrypoint-wrapper.sh");
        tokio::fs::write(&wrapper_host_path, script_content.as_bytes())
            .await
            .with_context(|| {
                format!(
                    "Failed to write entrypoint wrapper script to '{}'",
                    wrapper_host_path.display()
                )
            })?;

        // Make the script executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o755);
            tokio::fs::set_permissions(&wrapper_host_path, perms)
                .await
                .with_context(|| {
                    format!(
                        "Failed to set executable permissions on wrapper script '{}'",
                        wrapper_host_path.display()
                    )
                })?;
        }

        // Add a bind mount from the host file to the wrapper path inside the container
        let host_path = wrapper_host_path.display().to_string();
        let mount_spec = format!(
            "type=bind,source={},target={},readonly",
            host_path, wrapper_path
        );
        debug!(
            host_path = %host_path,
            container_path = %wrapper_path,
            "Adding bind mount for entrypoint wrapper script"
        );
        merged_mounts.mounts.push(mount_spec);
    }

    // Log GPU mode application
    if args.gpu_mode == deacon_core::gpu::GpuMode::All {
        info!("Applying GPU mode: all - requesting GPU access for container");
    } else if args.gpu_mode != deacon_core::gpu::GpuMode::None {
        debug!("GPU mode: {:?}", args.gpu_mode);
    }

    // When `--auto-forward` is set, declared ports are routed to the detached
    // forwarder and must NOT also be statically `-p` published, so Docker and
    // the daemon never contend for the same host port (FR-006). Capture the
    // declared specs first (for the forwarder), then publish via a config clone
    // with the ports stripped — the unmodified `config` is still used for the
    // result/report so the stdout contract is unchanged (FR-010).
    let auto_forward_declared: Vec<String> = if args.auto_forward {
        crate::commands::up::forward::declared_port_specs(&config, &[])
    } else {
        Vec::new()
    };
    // When replacing an existing container, reap its forwarder first so its
    // host ports are released before the replacement's forwarder allocates
    // (FR-014). No-op when no forwarder marker exists.
    if args.remove_existing_container {
        crate::commands::up::forward::reap_existing_forwarders(
            docker,
            identity,
            args.user_data_folder.as_deref(),
        )
        .await;
    }

    let stripped_config;
    let config_for_create: &DevContainerConfig = if args.auto_forward {
        let mut c = config.clone();
        c.forward_ports.clear();
        c.app_port = None;
        stripped_config = c;
        &stripped_config
    } else {
        &config
    };

    // Create container using DockerLifecycle trait.
    //
    // We pass `workspace_mount_source` (#67), not the raw workspace folder,
    // so the default workspace bind mount inside
    // `Docker::create_container` sources from the git root when
    // `--mount-workspace-git-root=true`. Container identity has already
    // been computed from the user's workspace folder above, so this only
    // affects the bind mount and not workspace+config hashing.
    let container_result = docker
        .up(
            identity,
            config_for_create,
            &workspace_mount_source,
            args.remove_existing_container,
            args.gpu_mode,
            &merged_security,
            &merged_mounts,
            &entrypoint_chain,
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

    // Host-CA runtime injection (016, T026): stream the corporate bundle into
    // the running container and install it into the distro trust store BEFORE
    // any lifecycle hook runs, so onCreate/postCreate network calls trust the
    // proxy CA. Runs only when discovery produced a non-empty set; degrades to
    // env-var-only (already-set vars) on unsupported distro / non-root.
    if let Some(set) = host_ca_set {
        let _ = inject_runtime(runtime, &container_result.container_id, set).await?;
    }

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

    // Apply user mapping: create remote user, update UID/GID to match host, set ownership
    // Security options (privileged, capAdd, securityOpt, init) are already applied during
    // container creation via merged_security (see above).
    if config.remote_user.is_some() || config.container_user.is_some() {
        apply_user_mapping(
            runtime,
            &container_result.container_id,
            &config,
            workspace_folder,
        )
        .await?;
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
        config.user_env_probe.unwrap_or(args.default_user_env_probe),
        Some(&config.remote_env),
        cli_remote_env,
        cache_folder.as_deref(),
    )
    .await;

    // T014: Read prior lifecycle markers for resume decision logic
    // Per SC-002: On resume, skip onCreate, updateContent, postCreate, dotfiles; run postStart, postAttach
    // Per FR-004: On partial resume, skip completed phases, run remaining from earliest incomplete
    //
    // Spec parity (#117): `--remove-existing-container` destroys and
    // recreates the container. A new container has never had any lifecycle
    // phase run on it, so wipe the workspace's prior markers before we
    // build the invocation context — otherwise onCreate / updateContent /
    // postCreate are silently skipped on the fresh container.
    if args.remove_existing_container {
        if let Err(e) = deacon_core::state::clear_markers(
            workspace_folder,
            args.prebuild,
            args.user_data_folder.as_deref(),
        )
        .await
        {
            debug!(
                "Failed to clear lifecycle markers for --remove-existing-container: {}",
                e
            );
        }
    }

    // Spec parity (#93): filter markers by the *current* config_hash so
    // a re-invocation with a different `--override-config` (or any input
    // that produces a different resolved config) reruns lifecycle from
    // scratch. Markers without a recorded config_hash predate this
    // change and remain compatible with any current hash.
    let prior_markers = deacon_core::state::read_all_markers_for_config(
        workspace_folder,
        args.prebuild,
        Some(&identity.config_hash),
        args.user_data_folder.as_deref(),
    )
    .await
    .unwrap_or_else(|e| {
        debug!("Failed to read prior lifecycle markers: {}", e);
        Vec::new()
    });

    debug!(
        "Prior lifecycle markers: {} markers loaded (prebuild={})",
        prior_markers.len(),
        args.prebuild
    );

    // Execute lifecycle commands if not skipped
    // Pass resolved features for lifecycle command aggregation (feature commands execute before config)
    execute_lifecycle_commands(
        &container_result.container_id,
        &config,
        workspace_folder,
        args,
        env_user_resolution.effective_env.clone(),
        env_user_resolution.effective_user.clone(),
        cache_folder,
        resolved_features.as_deref().unwrap_or(&[]),
        prior_markers,
        Some(&identity.config_hash),
        runtime,
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
            args.auto_forward,
            args.user_data_folder.as_deref(),
        )
        .await?;
    }

    // Start the detached port forwarder if requested. Best-effort: a failure
    // warns but never fails `up` (FR-002, FR-025).
    if args.auto_forward {
        crate::commands::up::forward::spawn_or_adopt(
            args,
            &container_result.container_id,
            workspace_folder,
            config_path,
            &auto_forward_declared,
        )
        .await;
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

    // Spec default for the container workspace folder is
    // `/workspaces/${localWorkspaceFolderBasename}` (not a bare `/workspaces`).
    // Mirror the actual bind-mount target built in `Docker::create_container`,
    // which uses the basename of `workspace_mount_source` (the git-root mount
    // source under `--mount-workspace-git-root`), so the reported value always
    // matches what is really mounted inside the container.
    let remote_workspace_folder = super::helpers::default_remote_workspace_folder(
        config.workspace_folder.as_deref(),
        &workspace_mount_source,
    );

    // Serialize configuration if requested
    let configuration = if args.include_configuration {
        Some(serde_json::to_value(&config)?)
    } else {
        None
    };

    let merged_configuration = if args.include_merged_configuration {
        // Use shared helper with injected runtime
        let options = inspect_for_merged_configuration(
            docker,
            &container_result.container_id,
            config.image.as_deref(),
            None, // Single container, no service context
            resolved_features,
        )
        .await;
        Some(build_merged_configuration_with_options(
            &config,
            config_path,
            options,
        )?)
    } else {
        None
    };

    Ok(UpContainerInfo {
        container_id: container_result.container_id.clone(),
        remote_user,
        remote_workspace_folder,
        compose_project_name: None,
        // Single container flow doesn't use compose profiles or external volumes
        effective_mounts: None,
        effective_env: None,
        profiles_applied: None,
        external_volumes_preserved: None,
        configuration,
        merged_configuration,
        injected_ca_subjects: host_ca_set.map(|s| s.subjects.clone()).unwrap_or_default(),
    })
}

/// Handle shutdown for container configurations
#[instrument(skip(config, state_manager))]
pub(crate) async fn handle_container_shutdown(
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
