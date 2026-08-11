//! Docker Compose flow for the up command.
//!
//! This module contains:
//! - `execute_compose_up` - Main compose up execution
//! - `execute_compose_lifecycle` - Full lifecycle phase set for compose, through
//!   the same engine the single-container path uses
//! - `handle_compose_shutdown` - Shutdown handling for compose

use super::args::{MountType, NormalizedMount, UpArgs};
use super::features_build::{
    FeatureBuildOutput, build_image_with_features, build_image_with_features_from_dockerfile,
};
use super::helpers::{apply_user_mapping, handle_lockfile_post_build};
use super::lifecycle::{HostTrustArgs, execute_initialize_command};
use super::merged_config::{
    build_merged_configuration_with_options, inspect_for_merged_configuration,
};
use super::ports::handle_port_events;
use super::result::{EffectiveMount, UpContainerInfo};
use anyhow::{Context, Result};
use deacon_core::IndexMap;
use deacon_core::compose::{ComposeCommand, ComposeManager, ComposeProject, ServiceShape};
use deacon_core::config::DevContainerConfig;
use deacon_core::container::ContainerIdentity;
use deacon_core::docker::Docker;
use deacon_core::errors::{DeaconError, DockerError};
use deacon_core::host_ca::{CA_ENV_VARS, CorporateCaSet, HOST_CA_BUNDLE_PATH, inject_runtime};
use deacon_core::runtime::ContainerRuntimeImpl;
use deacon_core::state::{ComposeState, StateManager};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

/// Resolve the primary service container ID, retrying with exponential backoff
/// to absorb the brief window between `docker compose up` returning and the
/// container appearing in `docker compose ps`. Shared by the host-CA injection
/// step (before post-create) and the final result assembly.
async fn resolve_primary_container_id_with_retry(
    compose_manager: &ComposeManager,
    project: &ComposeProject,
) -> Result<String> {
    let mut attempts = 0;
    const MAX_ATTEMPTS: u32 = 10;
    const INITIAL_DELAY_MS: u64 = 100;

    loop {
        match compose_manager.get_primary_container_id(project).await? {
            Some(id) => break Ok(id),
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
                break Err(anyhow::anyhow!(
                    "Failed to get primary container ID after starting compose project (tried {} times)",
                    MAX_ATTEMPTS
                ));
            }
        }
    }
}

/// Execute up for Docker Compose configurations
#[allow(clippy::needless_borrows_for_generic_args)] // config borrowed twice for serialization
#[allow(clippy::too_many_arguments)]
#[instrument(skip(config, workspace_folder, args, state_manager, runtime))]
pub(crate) async fn execute_compose_up(
    config: &DevContainerConfig,
    raw_config: &DevContainerConfig,
    identity: &ContainerIdentity,
    workspace_folder: &Path,
    args: &UpArgs,
    state_manager: &mut StateManager,
    workspace_hash: &str,
    effective_env: &IndexMap<String, String>,
    config_path: &Path,
    runtime: &ContainerRuntimeImpl,
    cache_folder: &Option<PathBuf>,
    host_ca_set: Option<&CorporateCaSet>,
) -> Result<UpContainerInfo> {
    debug!("Starting Docker Compose project");

    // Build options (cache flags + the resolved build-output mode) for the
    // feature-extended image builds below. Previously this path passed `None`,
    // which (a) ignored cache-from/cache-to/no-cache and (b) defaulted the build
    // output to Plain — so on a TTY the compose feature build dumped the raw
    // BuildKit firehose instead of the compact per-step view. Mirror the
    // single-container path (container.rs), which threads these through.
    let build_options = super::args::build_options_from_args(args);

    let compose_manager = ComposeManager::with_docker_path(args.docker_path.clone());
    // Compose files resolve relative to the directory containing devcontainer.json
    // (the `.devcontainer` dir for the standard layout), not the workspace folder.
    let config_dir = config_path.parent().unwrap_or(workspace_folder);
    let mut project = compose_manager.create_project(config, workspace_folder, config_dir)?;

    // #564. Compose prefixes every named volume with the project name, so any change to
    // the project name leaves the previous project's volumes intact but INVISIBLE to the
    // new project. Two transitions produce that silently: an older deacon's
    // `deacon_<wsHash>_<cfgHash>` (the format before the workspace stem was prepended),
    // and a `<folder>_devcontainer` project someone arrived with from the reference CLI.
    // The container side is already covered by `stop_superseded_containers`; volumes are
    // deliberately never swept because they hold data, which is precisely why the
    // situation has to be said out loud rather than left for an unexplained empty
    // database to explain later.
    //
    // Emitted here — before `docker compose up` and before the reconnect branch below —
    // so it is reported on every shape of this call exactly once, and reported even if
    // the provisioning that follows fails. Detection is a daemon query with no
    // suppression state: the condition clears itself the moment the old volumes are gone,
    // and persisting a "already told you" marker would risk the single emission landing
    // in a run whose output the user never read.
    let superseded_projects = compose_manager
        .detect_superseded_volume_projects(
            &project.name,
            &identity.workspace_hash,
            workspace_folder,
        )
        .await;
    if let Some(advice) =
        deacon_core::compose::superseded_project_advice(&superseded_projects, &project.name)
    {
        warn!("{advice}");
    }

    // Add env files from CLI args
    project.env_files = args.env_file.clone();

    // Spec parity (#100): stamp the same deacon identity labels the
    // single-container path uses onto every compose service. Without these, VS
    // Code Dev Containers reconnect / `docker ps --filter
    // label=devcontainer.local_folder=<abs>` / `deacon exec --id-label` all
    // miss compose-managed containers. The identity is the canonical
    // (as-loaded) one computed by the caller (#187) — it must NOT be rebuilt
    // from the post-substitution `config` here, or the stamped `configHash`
    // could drift from what `exec`/`down` compute.
    for (key, value) in identity.labels() {
        project.deacon_labels.insert(key, value);
    }

    // Apply default workspace mount for Compose when consistency is provided
    // Per FR-001: workspace_mount_consistency MUST apply to both Docker and Compose outputs
    // This mirrors the Docker behavior in execute_docker_up().
    //
    // Spec parity (#67): when `--mount-workspace-git-root` is true, the
    // mount *source* walks up to the enclosing git root so git operations
    // inside the container work; otherwise the user's workspace folder.
    // Discovery has already used the user's path by this point.
    let mut additional_mounts = Vec::new();
    if args.workspace_mount_consistency.is_some() {
        let mount_source = if args.mount_workspace_git_root {
            deacon_core::workspace::resolve_workspace_root(workspace_folder)?
        } else {
            workspace_folder.to_path_buf()
        };
        let target_path = config.workspace_folder.clone().unwrap_or_else(|| {
            format!(
                "/workspaces/{}",
                mount_source
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("workspace")
            )
        });

        additional_mounts.push(deacon_core::compose::ComposeMount {
            mount_type: "bind".to_string(),
            source: mount_source.display().to_string(),
            target: target_path,
            read_only: false,
            consistency: args.workspace_mount_consistency.clone(),
        });
        debug!(
            "Added default workspace mount for Compose with consistency: {:?}",
            args.workspace_mount_consistency
        );
    }

    // `devcontainer.json` `mounts` (#266) and feature-contributed `mounts`
    // (#272) are folded into `additional_mounts` further below, AFTER
    // features are resolved (`feature_build`) — `merge_mounts` needs the
    // resolved features to fold their `mounts` in, mirroring the
    // single-container path (up/container.rs).

    // Apply `containerEnv` (config) + remote env (CLI, higher precedence) to
    // compose services. `config.container_env` was never read here at all —
    // only CLI `--remote-env` reached the compose service, unlike the
    // single-container path where `create_container` (docker.rs) reads
    // `config.container_env` directly. Found via the #267 parity suite.
    let mut merged_container_env: IndexMap<String, String> = IndexMap::new();
    // `config.container_env` is an `IndexMap` (#394), so iterating it yields the
    // order the author wrote. This used to insert in sorted-key order to work
    // around the `HashMap` it was — deterministic run-to-run, but still not the
    // authored order the reference renders. The sort is redundant now, and
    // keeping it would silently re-order what the author wrote.
    for (key, value) in config.container_env() {
        merged_container_env.insert(key.clone(), value.clone());
    }
    for (key, value) in effective_env {
        merged_container_env.insert(key.clone(), value.clone());
    }
    if !merged_container_env.is_empty() {
        project.additional_env = merged_container_env;
    }

    // Host-CA env (016, T028): synthesize the six CA env vars into the primary
    // service environment, insert-if-absent so user remoteEnv/containerEnv win
    // (FR-024). Applied after the user env above so user values take precedence.
    if host_ca_set.is_some() {
        for name in CA_ENV_VARS {
            project
                .additional_env
                .entry(name.to_string())
                .or_insert_with(|| HOST_CA_BUNDLE_PATH.to_string());
        }
    }

    // Per T006: Mount/env injection is now handled via ComposeManager::start_project()
    // which uses ComposeProject::generate_injection_override() to pipe YAML via stdin.
    // No temporary override files are created.

    // Populate profiles from compose config.
    // Per spec §7: detect profiles for services in runServices and pass them
    // via --profile flags to all compose commands.
    // Uses `docker compose config --format json` (same pattern as external volumes).
    if let Err(e) = compose_manager.populate_profiles(&mut project).await {
        debug!(
            "Could not detect compose profiles (Docker may be unavailable): {}",
            e
        );
    }

    // Populate external volumes from compose config.
    // This enables tracking which volumes are external for validation and preservation.
    // Per spec: external volumes must not be replaced or mutated by injection.
    // Note: This operation requires Docker - if unavailable, we continue without
    // external volume information as this is non-blocking for the core up workflow.
    if let Err(e) = compose_manager
        .populate_external_volumes(&mut project)
        .await
    {
        debug!(
            "Could not populate external volumes (Docker may be unavailable): {}",
            e
        );
    }

    debug!("Created compose project: {:?}", project.name);

    // If we expect an existing project, fail fast when it's not running.
    if args.expect_existing_container {
        match compose_manager.is_project_running(&project).await {
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
        match compose_manager.is_project_running(&project).await {
            Ok(true) => {
                debug!("Compose project {} is already running", project.name);
                // Get the primary container ID for potential exec operations
                let container_id = compose_manager
                    .get_primary_container_id(&project)
                    .await?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Failed to get primary container ID for running compose project"
                        )
                    })?;
                debug!("Primary service container ID: {}", container_id);

                // #371: this project is the workspace's live generation, so any
                // OTHER project this workspace still has running is superseded.
                // Reached by editing the configuration and editing it back — the
                // original project name is restored and reconnected to, leaving
                // the intermediate generation running. Same sweep, same ruling as
                // the created-project path below.
                deacon_core::container::stop_superseded_containers(
                    runtime,
                    identity,
                    deacon_core::container::CurrentContainer {
                        container_id: Some(&container_id),
                        compose_project: Some(&project.name),
                    },
                )
                .await;

                // Return container info for already-running project
                let remote_user = config
                    .remote_user
                    .clone()
                    .or_else(|| config.container_user.clone())
                    .unwrap_or_else(|| "root".to_string());

                // Spec default is `/workspaces/${localWorkspaceFolderBasename}`,
                // not a bare `/workspaces`.
                let remote_workspace_folder = super::helpers::default_remote_workspace_folder(
                    workspace_folder,
                    config.workspace_folder.as_deref(),
                    args.mount_workspace_git_root,
                );

                // Serialize configuration if requested
                let configuration = if args.include_configuration {
                    Some(serde_json::to_value(config)?)
                } else {
                    None
                };

                // Existing container reconnect - no resolved features available
                let merged_configuration = if args.include_merged_configuration {
                    // Use shared helper with injected runtime (respects --docker-path)
                    let options = inspect_for_merged_configuration(
                        runtime,
                        &container_id,
                        config.image.as_deref(),
                        Some(project.service.clone()),
                        None, // No resolved features for reconnect
                    )
                    .await;
                    Some(build_merged_configuration_with_options(
                        config,
                        config_path,
                        options,
                    )?)
                } else {
                    None
                };

                return Ok(UpContainerInfo {
                    container_id,
                    remote_user,
                    remote_workspace_folder,
                    compose_project_name: Some(project.name.clone()),
                    // For existing container reconnect, we don't have injection data
                    effective_mounts: None,
                    effective_env: None,
                    profiles_applied: None,
                    external_volumes_preserved: None,
                    configuration,
                    merged_configuration,
                    injected_ca_subjects: host_ca_set
                        .map(|s| s.subjects.clone())
                        .unwrap_or_default(),
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
        let trust_args = HostTrustArgs {
            trust_workspace: args.trust_workspace,
            trust_workspace_persist: args.trust_workspace_persist,
            user_data_folder: args.user_data_folder.as_deref(),
        };
        // FR-020a: bypass the workspace-trust gate only when the effective
        // command is owner-authored (from a user-data profile fragment).
        let author_trusted = super::lifecycle::initialize_command_author_trusted(
            config.initialize_command.is_some(),
            &args.settings_merge_paths,
            &args.cli_merge_paths,
            args.user_data_folder.as_deref(),
        )
        .await;
        execute_initialize_command(
            initialize,
            workspace_folder,
            &args.progress_tracker,
            trust_args,
            author_trusted,
        )
        .await?;
    }

    // Stop existing containers if requested
    if args.remove_existing_container {
        debug!("Stopping existing compose project");
        if let Err(e) = compose_manager.stop_project(&project).await {
            warn!("Failed to stop existing project: {}", e);
        }
        // Spec parity (#117): recreating containers means a fresh project that has
        // never had any lifecycle phase run, so wipe the workspace's prior markers —
        // matching the single-container path (container.rs). Best-effort; markers are
        // deacon-internal state, so a failure here never blocks the up flow.
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

    // Bead 14a + 14b: when features are declared, install them by building a
    // feature-extended image and rewriting the target service's `image:` via
    // the existing injection override. Both the `image:` shape (14a) and the
    // `build:` shape (14b — user-authored Dockerfile + context) are supported.
    // Future work (per spec): thread resolved_features into merged_configuration.
    // Pause the interactive spinner around the feature build so the build's
    // streaming renderer owns stderr (otherwise the steady-tick spinner clobbers
    // the build progress).
    let feature_build = {
        let _pause = crate::commands::shared::progress::SpinnerPause::new(&args.progress_tracker);
        install_features_for_compose(
            config,
            &compose_manager,
            &mut project,
            workspace_folder,
            config_path,
            workspace_hash,
            Some(&build_options),
            host_ca_set,
            &runtime.cli_docker(),
        )
        .await?
    };

    // Fold feature-contributed `containerEnv` into the compose service environment,
    // BELOW the configuration's own and the CLI's (already inserted above).
    //
    // The compose path never read `feature_build.combined_env` at all, so a Feature
    // declaring `containerEnv` had it baked into the feature-extended image but never
    // into the service's `environment:` block — while the single-container path applies
    // it in `up/container.rs`. `bhv-up-container-env-merge-precedence` is stated over
    // "the created container", not over one container shape, so the two paths disagreeing
    // is a nonconformance on this one. Must run here (after `feature_build`), the same
    // constraint the feature-`mounts` merge below is placed for.
    //
    // `or_insert` (never `insert`) is the precedence: configuration and CLI values
    // already present win the conflict, matching the single-container fix from 024 US5.
    // Keys are applied in sorted order so the rendered override block stays
    // deterministic run-to-run. Unlike `config.container_env` above — an `IndexMap`
    // carrying the order the AUTHOR wrote since #394 — this map is Feature-contributed
    // and is still a `HashMap`, so sorting is the only order available to it.
    if let Some(ref fb) = feature_build {
        let mut feature_env_keys: Vec<&String> = fb.combined_env.keys().collect();
        feature_env_keys.sort();
        for key in feature_env_keys {
            project
                .additional_env
                .entry(key.clone())
                .or_insert_with(|| fb.combined_env[key].clone());
        }
    }

    // Lockfile graduation (PR-4b): mirror the single-container flow — write
    // the lockfile to disk, or byte-compare it in `--frozen-lockfile` mode.
    // Only runs when features were actually built (the compose path returns
    // `None` when no features are declared).
    if let Some(ref fb) = feature_build {
        handle_lockfile_post_build(args, config_path, &fb.lockfile).await?;
    }

    // The image the primary service container will actually RUN: the
    // feature-extended tag when Features were built, else the service's own
    // `image:`. Two things below need it — the `devcontainer.metadata` label
    // deacon stamps (#322) and the image-metadata merge (#448) — so it is
    // resolved once here rather than probed twice.
    let effective_image = match &project.service_image_override {
        Some(img) => Some(img.clone()),
        None => match compose_manager
            .get_command(&project)
            .extract_service_shape(&project.service)
            .await
        {
            Ok(ServiceShape::Image(img)) => Some(img),
            _ => None,
        },
    };

    // #322: stamp the merged `devcontainer.metadata` on the compose service
    // container too (via `deacon_labels`, applied by the injection override), so
    // config living only in devcontainer.json — esp. `remoteEnv` — survives and
    // is recoverable by exec/read-configuration/set-up `--container-id`, exactly
    // like the single-container path.
    if let Some(img) = effective_image.as_deref() {
        if let Some(json) = super::merged_config::build_container_metadata_label(
            &runtime.cli_docker(),
            img,
            raw_config,
            feature_build
                .as_ref()
                .map(|fb| fb.resolved_features.as_slice())
                .unwrap_or(&[]),
        )
        .await
        {
            project.deacon_labels.insert(
                deacon_core::container::LABEL_DEVCONTAINER_METADATA.to_string(),
                json,
            );
        }
    }

    // Spec parity (#448): fold the service image's `devcontainer.metadata` LABEL
    // into the configuration, at lower precedence than the user's
    // devcontainer.json — the SAME merge, at the same point in the flow (image
    // ready, before lifecycle and env resolution), that the single-container
    // path performs at `up/container.rs`. `merge_image_metadata_after_image_ready`
    // stays the one implementation; there is no compose-specific re-reader.
    //
    // Without this a `remoteUser`, `remoteEnv` or lifecycle hook contributed
    // only by the service image was honored by `up`'s single-container path and
    // by every later `exec` / `run-user-commands` attach (#405), and silently
    // dropped by a compose `up`'s own lifecycle run. Measured against the pinned
    // reference CLI 0.87.0, which applies all three on the compose path.
    //
    // `raw_config` — the pre-substitution config the stamped LABEL is built from
    // (#437) — is deliberately NOT merged into: the label records what the author
    // wrote, not what the image contributed.
    let merged_config = match effective_image.as_deref() {
        Some(img) => {
            super::merged_config::merge_image_metadata_after_image_ready(
                &runtime.cli_docker(),
                img,
                config.clone(),
            )
            .await
        }
        None => config.clone(),
    };
    let config = &merged_config;

    // Apply `devcontainer.json` `mounts` (#266) and feature-contributed
    // `mounts` (#272) to the compose project. Run through `merge_mounts` in a
    // single call — same as the single-container path (up/container.rs) —
    // so config-vs-feature precedence and target-based dedup happen exactly
    // once. Must run here (after `feature_build`) so `resolved_features` is
    // available. Precedence overall: workspace < feature < config < CLI.
    let resolved_features_for_mounts: &[deacon_core::features::ResolvedFeature] = feature_build
        .as_ref()
        .map(|fb| fb.resolved_features.as_slice())
        .unwrap_or(&[]);
    if !config.mounts().is_empty() || !resolved_features_for_mounts.is_empty() {
        let mount_substitution_context = {
            let mut ctx = deacon_core::variable::SubstitutionContext::new(workspace_folder)?;
            let id_labels: Vec<(String, String)> = identity.id_hash_labels();
            ctx.devcontainer_id = deacon_core::container::compute_dev_container_id(&id_labels);
            ctx
        };
        let merged_config_mounts = deacon_core::mount::merge_mounts(
            config.mounts(),
            resolved_features_for_mounts,
            Some(&mount_substitution_context),
        )
        .with_context(|| "Failed to merge devcontainer.json + feature mounts for compose")?;
        for mount_str in &merged_config_mounts.mounts {
            let mount = NormalizedMount::parse(mount_str).with_context(|| {
                format!(
                    "Invalid mount specification in devcontainer.json/feature: {}",
                    mount_str
                )
            })?;
            additional_mounts.push(normalized_mount_to_compose_mount(&mount));
        }
    }

    // Apply CLI mounts to compose project
    // Per CLAUDE.md: No silent fallbacks - fail fast on invalid mounts
    for mount_str in &args.mount {
        let mount = NormalizedMount::parse(mount_str)
            .with_context(|| format!("Invalid mount specification: {}", mount_str))?;
        additional_mounts.push(normalized_mount_to_compose_mount(&mount));
    }

    // Dedupe by target, last-wins, so later sources (feature, config, then
    // CLI) can override the workspace-consistency mount or each other for the
    // same target path — mirrors `merge_mounts`' union-by-target semantics
    // (`mount.rs`), which `generate_injection_override` does NOT do on its
    // own; it renders every `additional_mounts` entry verbatim.
    if !additional_mounts.is_empty() {
        project.additional_mounts = dedupe_compose_mounts_by_target(additional_mounts);
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

    compose_manager
        .start_project(&project, args.gpu_mode)
        .await?;

    info!("Compose project {} started successfully", project.name);

    // Two things must happen against the primary service container after the
    // project is up and BEFORE the compose post-create lifecycle hook runs. Both
    // need its id, so it is resolved once here rather than probed twice.
    //
    // 1. Host-CA runtime injection (016, T027): install the corporate CA.
    //
    // 2. Spec parity (#462): apply the `updateRemoteUserUID` user mapping —
    //    create the remote user if absent, remap its uid/gid to the host user's,
    //    and adjust workspace ownership. `up`'s single-container path
    //    (`up/container.rs`) has always done this; the compose path never did, so
    //    a non-root `remoteUser` kept the IMAGE's uid and any hook writing into
    //    the bind-mounted workspace died with `Permission denied` unless the
    //    host's uid happened to match. Measured against the pinned reference CLI
    //    0.87.0 on a compose service whose image pins its user to uid 1234 and a
    //    host at uid 1000: the reference's hook reported `uid=1000` and wrote;
    //    deacon's wrote nothing.
    //
    //    The reference reaches the same observable a different way — it builds a
    //    derived `<image>-uid` image whose final `RUN` layer rewrites
    //    `/etc/passwd`/`/etc/group`, then points the service at it through its
    //    generated `docker-compose.devcontainer.containerFeatures-*.yml`
    //    override. deacon instead remaps in the running container, which is the
    //    mechanism `apply_user_mapping` already implements and the
    //    single-container path already ships; reusing it keeps ONE uid-remap
    //    implementation, and the observable — the remote user's uid inside the
    //    container, and its ability to write the bind mount — is identical.
    //
    //    The condition, the helper and therefore every spec rule it encodes
    //    (`updateRemoteUserUID` defaults true on Linux, is a no-op for a root
    //    `remoteUser` (#90) or a root host user, and honors an explicit
    //    `false`) are the single-container path's, not a compose-specific copy.
    //
    //    Order matches `up/container.rs`: CA first, then the user mapping.
    //
    //    Resolved ONCE here and reused by everything downstream that needs the
    //    primary service container — the CA injection, the user mapping, the
    //    lifecycle run below, and the result assembly. The retry absorbs the
    //    window between `docker compose up` returning and the container appearing
    //    in `docker compose ps`, so paying for it once rather than per consumer
    //    also removes a `docker compose ps` round-trip from every compose `up`.
    let primary_container_id =
        resolve_primary_container_id_with_retry(&compose_manager, &project).await?;

    // #371, compose half. The project name is
    // `deacon_<stem>_<workspace_hash>_<config_hash>`
    // (`compose::derive_project_name`), so an edited configuration names a WHOLE NEW
    // project and the previous one's containers keep running exactly as on the
    // single-container path. Reaching here means we started a project rather than
    // reconnecting to one already running (that branch returned above), so anything
    // else this workspace still has live belongs to a superseded generation.
    // Sweeping by project — not by deacon's own labels — is what reaches compose's
    // dependency services, which carry no `devcontainer.*` labels of ours; see
    // `stop_superseded_containers`. Stopped, not removed, per the same ruling.
    deacon_core::container::stop_superseded_containers(
        runtime,
        identity,
        deacon_core::container::CurrentContainer {
            container_id: Some(&primary_container_id),
            compose_project: Some(&project.name),
        },
    )
    .await;

    if let Some(set) = host_ca_set {
        let _ = inject_runtime(runtime, &primary_container_id, set).await?;
    }

    if config.remote_user.is_some() || config.container_user.is_some() {
        apply_user_mapping(runtime, &primary_container_id, config, workspace_folder).await?;
    }

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

    // Run the FULL lifecycle phase set through the same machinery the
    // single-container path uses (#460). This replaced a compose-local exec that
    // ran `postCreateCommand` only in its STRING form and queued no other phase,
    // so an array/object hook, an `onCreate`/`updateContent`/`postStart`/
    // `postAttach` hook, a Feature-contributed hook, and the phase markers were
    // all silently dropped on compose while `up`'s single-container path ran
    // them. There is one lifecycle engine (`up::lifecycle::execute_lifecycle_commands`
    // → `deacon_core::container_lifecycle`); the compose path is now a caller of
    // it, not a second implementation.
    //
    // The `--skip-post-create` guard that used to wrap this call is deliberately
    // gone even though that flag now defers EVERY phase (#476): the decision belongs
    // to `InvocationContext::should_skip_phase`, so compose and the single-container
    // path answer it the same way, and a second copy here could only drift. It also
    // keeps the flag from suppressing the compose-side work that is not a hook
    // (state save, port forwarding) the way a call-site gate would.
    execute_compose_lifecycle(
        &primary_container_id,
        config,
        identity,
        workspace_folder,
        args,
        effective_env,
        cache_folder,
        feature_build
            .as_ref()
            .map(|fb| fb.resolved_features.as_slice())
            .unwrap_or(&[]),
    )
    .await?;

    // Handle port forwarding and events
    if args.ports_events {
        handle_port_events(
            config,
            &project,
            &args.redaction_config,
            &args.secret_registry,
            &args.docker_path,
            args.auto_forward,
            args.browser_setting.as_deref(),
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

    // Collect container information for JSON output. Already resolved (with
    // backoff) for the lifecycle run above.
    let container_id = primary_container_id;

    // Start the detached port forwarder for the primary service container if
    // requested. Declared `"service:port"` specs relay over the compose network
    // to the named service; auto-detection stays scoped to the primary service
    // (FR-023). Best-effort (FR-002, FR-025).
    if args.auto_forward {
        let declared = super::forward::declared_port_specs(config, &args.forward_ports);
        super::forward::spawn_or_adopt(
            args,
            &container_id,
            workspace_folder,
            config_path,
            &declared,
        )
        .await;
    }

    let remote_user = config
        .remote_user
        .clone()
        .or_else(|| config.container_user.clone())
        .unwrap_or_else(|| "root".to_string());

    // Spec default is `/workspaces/${localWorkspaceFolderBasename}`, not a bare
    // `/workspaces`.
    let remote_workspace_folder = super::helpers::default_remote_workspace_folder(
        workspace_folder,
        config.workspace_folder.as_deref(),
        args.mount_workspace_git_root,
    );

    // Serialize configuration if requested
    let configuration = if args.include_configuration {
        Some(serde_json::to_value(&config)?)
    } else {
        None
    };

    let merged_configuration = if args.include_merged_configuration {
        // Use shared helper with injected runtime (respects --docker-path)
        let options = inspect_for_merged_configuration(
            runtime,
            &container_id,
            config.image.as_deref(),
            Some(project.service.clone()),
            None, // Features not yet supported for compose flow
        )
        .await;
        Some(build_merged_configuration_with_options(
            config,
            config_path,
            options,
        )?)
    } else {
        None
    };

    // Capture effective mounts from compose project
    let effective_mounts = if project.additional_mounts.is_empty() {
        None
    } else {
        Some(
            project
                .additional_mounts
                .iter()
                .map(|m| {
                    let mut options = Vec::new();
                    if m.read_only {
                        options.push("ro".to_string());
                    }
                    if let Some(ref consistency) = m.consistency {
                        options.push(format!("consistency={}", consistency));
                    }
                    EffectiveMount {
                        source: m.source.clone(),
                        target: m.target.clone(),
                        options,
                    }
                })
                .collect(),
        )
    };

    // Capture effective env from compose project
    let effective_env = if project.additional_env.is_empty() {
        None
    } else {
        Some(
            project
                .additional_env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<HashMap<_, _>>(),
        )
    };

    // Capture profiles applied
    let profiles_applied = if project.profiles.is_empty() {
        None
    } else {
        Some(project.profiles.clone())
    };

    // Capture external volumes preserved
    let external_volumes_preserved = if project.external_volumes.is_empty() {
        None
    } else {
        Some(project.external_volumes.clone())
    };

    Ok(UpContainerInfo {
        container_id,
        remote_user,
        remote_workspace_folder,
        compose_project_name: Some(project.name.clone()),
        effective_mounts,
        effective_env,
        profiles_applied,
        external_volumes_preserved,
        configuration,
        merged_configuration,
        injected_ca_subjects: host_ca_set.map(|s| s.subjects.clone()).unwrap_or_default(),
    })
}

/// Run the full lifecycle phase set against a compose project's primary service
/// container, through the ONE lifecycle implementation
/// (`up::lifecycle::execute_lifecycle_commands`).
///
/// This is a caller of the shared engine, not a second engine: phase set and
/// ordering, every command form (string / array / object), Feature-contributed
/// commands, `--skip-post-create` / `--skip-non-blocking-commands` /
/// `--prebuild` / `waitFor` semantics, dotfiles and the `.devcontainer-state`
/// phase markers all come from there and are identical to the single-container
/// path. Everything this function owns is the two inputs that path resolves
/// differently: the lifecycle **user/env**, and the lifecycle **cwd**.
///
/// `config` is the configuration AFTER the service image's `devcontainer.metadata`
/// has been folded in (#448), so a `remoteUser`, `remoteEnv` or lifecycle hook
/// contributed only by the image reaches these phases exactly as it does on the
/// single-container path.
#[allow(clippy::too_many_arguments)]
#[instrument(skip(config, identity, args, cli_remote_env, resolved_features))]
async fn execute_compose_lifecycle(
    container_id: &str,
    config: &DevContainerConfig,
    identity: &ContainerIdentity,
    workspace_folder: &Path,
    args: &UpArgs,
    cli_remote_env: &IndexMap<String, String>,
    cache_folder: &Option<PathBuf>,
    resolved_features: &[deacon_core::features::ResolvedFeature],
) -> Result<()> {
    debug!(
        "Executing lifecycle phases in compose container: {}",
        container_id
    );

    // Exec against the SAME CLI that created this compose project, which is
    // `--docker-path` (what `ComposeManager` is built with above) and NOT the
    // `--runtime`-selected `ContainerRuntimeImpl`. The two can differ: the
    // Podman CI lane sets `DEACON_CONTAINER_RUNTIME=podman` while
    // `--docker-path` keeps its `docker` default, so `docker compose` creates
    // the project in the DOCKER daemon and a `podman exec` against that
    // container fails with `no container with name or ID … found` — measured on
    // that lane. `CliRuntime::with_runtime_path` derives the flavor from the
    // binary name, so this client is Podman-shaped when `--docker-path` names a
    // podman binary and Docker-shaped otherwise; it follows the containers
    // rather than a flag that did not create them. Whether the compose path
    // should honor `--runtime` at all is a separate question, and pre-existing.
    let compose_runtime = ContainerRuntimeImpl::Docker(
        deacon_core::runtime::DockerRuntime::with_path(args.docker_path.clone()),
    );
    let runtime = &compose_runtime;

    // Resolve the hook's user and environment through the SAME shared helper the
    // single-container `up`, `exec` and `run-user-commands` use (CLAUDE.md
    // principle 6): probe → config `remoteEnv` → CLI `--remote-env`, with
    // `remoteUser` (else `containerUser`) as the exec user (#448).
    let env_user = crate::commands::shared::resolve_env_and_user(
        runtime,
        container_id,
        None,
        config
            .remote_user
            .clone()
            .or_else(|| config.container_user.clone()),
        config.user_env_probe.unwrap_or(args.default_user_env_probe),
        Some(config.remote_env()),
        cli_remote_env,
        cache_folder.as_deref(),
    )
    .await;

    // The lifecycle cwd, resolved against the RUNNING container rather than
    // derived host-side. deacon injects no workspace bind mount for compose (the
    // user's compose file provides it, or does not), so the single-container
    // derivation `/workspaces/<basename>` can name a directory the service never
    // mounts — and the core executor's `docker exec -w` hard-fails on a missing
    // cwd. `resolve_container_cwd` reads the container's actual mounts and falls
    // back to `/` for a compose config with no explicit `workspaceFolder`, which
    // is the reference's effective compose workspace (#294/#295).
    let mounts = match runtime.inspect_container(container_id).await {
        Ok(Some(info)) => info.mounts,
        Ok(None) => Vec::new(),
        Err(e) => {
            debug!("Could not inspect compose container for its mounts: {}", e);
            Vec::new()
        }
    };
    let container_workspace_folder = crate::commands::shared::resolve_container_cwd(
        config,
        workspace_folder,
        &mounts,
        args.mount_workspace_git_root,
    );

    // Prior phase markers for the resume decision, filtered by the current
    // config hash — the same read the single-container path performs (#93/#117).
    // `--remove-existing-container` already cleared them further up, symmetrically
    // with `up/container.rs`.
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

    super::lifecycle::execute_lifecycle_commands(
        container_id,
        config,
        workspace_folder,
        args,
        env_user.effective_env,
        env_user.effective_user,
        cache_folder,
        resolved_features,
        prior_markers,
        Some(&identity.config_hash),
        runtime,
        Some(container_workspace_folder),
    )
    .await
}

/// Bead 14a + 14b: install features into a compose-based devcontainer.
///
/// Workflow when `config.features` is non-empty:
/// 1. Inspect the target service's shape via `docker compose config --format json`.
/// 2. For `image:` services (bead 14a), synthesize a single-container config whose
///    `image` is the resolved compose image and whose `features` is the original
///    config's features; reuse `build_image_with_features` to produce a
///    feature-extended image.
/// 3. For `build:` services (bead 14b), resolve `dockerfile` and `context` paths
///    relative to the compose file's directory (NOT the workspace), read the
///    user's Dockerfile, rewrite its final `FROM` to carry a stable alias via
///    `ensure_dockerfile_has_final_stage_name`, then build with that as the
///    base via `build_image_with_features_from_dockerfile`.
/// 4. In both cases, set `project.service_image_override` so the existing
///    injection override rewrites the target service's `image:` line to point
///    at the extended tag.
#[allow(clippy::too_many_arguments)]
#[instrument(skip(config, compose_manager, project, workspace_folder))]
async fn install_features_for_compose(
    config: &DevContainerConfig,
    compose_manager: &ComposeManager,
    project: &mut ComposeProject,
    workspace_folder: &Path,
    config_path: &Path,
    workspace_hash: &str,
    build_options: Option<&deacon_core::build::BuildOptions>,
    host_ca_set: Option<&CorporateCaSet>,
    cli: &deacon_core::docker::CliRuntime,
) -> Result<Option<FeatureBuildOutput>> {
    let output = match resolve_compose_feature_image(
        config,
        compose_manager,
        project,
        workspace_folder,
        config_path,
        workspace_hash,
        build_options,
        host_ca_set,
        cli,
    )
    .await?
    {
        Some(o) => o,
        None => return Ok(None),
    };

    // `up` rewrites the target service's `image:` line to the extended tag so
    // the container runs with features installed.
    project.service_image_override = Some(output.image_tag.clone());
    Ok(Some(output))
}

/// Resolve (and build) the feature-extended image for a compose service, without
/// mutating the project. Shared by `up` (which then sets
/// `service_image_override` to run the extended image) and `build` (which tags
/// the produced image for the user and writes the lockfile). Returns `None` when
/// the config declares no features.
#[allow(clippy::too_many_arguments)]
#[instrument(skip(config, compose_manager, project, workspace_folder))]
pub(crate) async fn resolve_compose_feature_image(
    config: &DevContainerConfig,
    compose_manager: &ComposeManager,
    project: &ComposeProject,
    workspace_folder: &Path,
    config_path: &Path,
    workspace_hash: &str,
    build_options: Option<&deacon_core::build::BuildOptions>,
    host_ca_set: Option<&CorporateCaSet>,
    cli: &deacon_core::docker::CliRuntime,
) -> Result<Option<FeatureBuildOutput>> {
    // Nothing to install when features is missing or an empty object.
    let features_obj = match config.features().as_object() {
        Some(o) if !o.is_empty() => o,
        _ => {
            debug!("No features declared on compose config; skipping feature build");
            return Ok(None);
        }
    };
    debug!(
        feature_count = features_obj.len(),
        service = %project.service,
        "Resolving compose service shape for feature install"
    );

    let shape = compose_manager
        .get_command(project)
        .extract_service_shape(&project.service)
        .await
        .with_context(|| {
            format!(
                "Failed to resolve compose service '{}' shape via `docker compose config`",
                project.service
            )
        })?;

    // Compose-flavored identity: produced image tag is namespaced by
    // workspace+service so it does not collide with the single-container path.
    let mut identity = ContainerIdentity::new(workspace_folder, config);
    identity.workspace_hash = format!("{}-compose-{}", workspace_hash, project.service);

    let output = match shape {
        ServiceShape::Image(base_image) => {
            info!(
                service = %project.service,
                base_image = %base_image,
                "Building feature-extended image for compose service (image: shape)"
            );

            // Synthesize a single-container config so `build_image_with_features`
            // can consume it: only `image`, `features`, and
            // `override_feature_install_order` are read.
            let mut synth_config = config.clone();
            synth_config.image = Some(base_image.clone());

            build_image_with_features(
                &synth_config,
                &identity,
                workspace_folder,
                config_path,
                build_options,
                host_ca_set,
                cli,
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to build feature-extended image for compose service '{}'",
                    project.service
                )
            })?
        }
        ServiceShape::Build {
            context,
            dockerfile,
            target,
        } => {
            // Compose semantics: `build.context` and `build.dockerfile` are
            // resolved relative to the directory containing the compose file —
            // NOT the workspace folder. When multiple compose files are stacked,
            // we use the first one's directory (`docker compose` itself returns
            // paths as if they were declared in the primary compose file).
            let compose_dir = project
                .compose_files
                .first()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| project.base_path.clone());

            // Default context to `.` per compose schema.
            let context_rel = context.as_deref().unwrap_or(".");
            let context_path = resolve_compose_path(&compose_dir, context_rel);

            // Default dockerfile to `Dockerfile` relative to the *context*, per
            // compose semantics (NOT relative to the compose file directory).
            let dockerfile_rel = dockerfile.as_deref().unwrap_or("Dockerfile");
            let dockerfile_path = resolve_compose_path(&context_path, dockerfile_rel);

            info!(
                service = %project.service,
                context = %context_path.display(),
                dockerfile = %dockerfile_path.display(),
                target = ?target,
                "Building feature-extended image for compose service (build: shape)"
            );

            let dockerfile_content = tokio::fs::read_to_string(&dockerfile_path)
                .await
                .with_context(|| {
                    format!(
                        "Failed to read Dockerfile for compose service '{}' at {}",
                        project.service,
                        dockerfile_path.display()
                    )
                })?;

            let (modified_dockerfile, final_stage) =
                deacon_core::dockerfile_utils::ensure_dockerfile_has_final_stage_name(
                    &dockerfile_content,
                    "dev_containers_user_image",
                )
                .with_context(|| {
                    format!(
                        "Failed to parse Dockerfile for compose service '{}' at {}",
                        project.service,
                        dockerfile_path.display()
                    )
                })?;

            build_image_with_features_from_dockerfile(
                config,
                &identity,
                &modified_dockerfile,
                &final_stage,
                &context_path,
                config_path,
                target.as_deref(),
                build_options,
                host_ca_set,
                cli,
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to build feature-extended image for compose service '{}' \
                     using Dockerfile {}",
                    project.service,
                    dockerfile_path.display()
                )
            })?
        }
        ServiceShape::Neither => {
            return Err(DeaconError::Runtime(format!(
                "Compose service '{}' has neither `image:` nor `build:`; cannot \
                 install features against an undefined base",
                project.service
            ))
            .into());
        }
        ServiceShape::NotFound => {
            return Err(DeaconError::Runtime(format!(
                "Compose service '{}' not found in resolved compose config",
                project.service
            ))
            .into());
        }
    };

    info!(
        service = %project.service,
        extended_image = %output.image_tag,
        feature_count = output.resolved_features.len(),
        "Feature-extended image ready"
    );

    Ok(Some(output))
}

/// Resolve a path expressed in a compose file relative to the compose file's
/// directory (or its `context` for Dockerfile resolution). Absolute inputs are
/// returned unchanged. Centralized here so the `build:` arm and any unit tests
/// share the same compose-semantic resolution.
fn resolve_compose_path(base: &Path, candidate: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(candidate);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}

/// Map a validated CLI/config mount into the compose project's mount shape.
/// Shared by the `devcontainer.json` `mounts` (#266) and CLI `--mount` loops
/// in [`execute_compose_up`] so both normalize identically.
fn normalized_mount_to_compose_mount(
    mount: &NormalizedMount,
) -> deacon_core::compose::ComposeMount {
    deacon_core::compose::ComposeMount {
        mount_type: match mount.mount_type {
            MountType::Bind => "bind".to_string(),
            MountType::Volume => "volume".to_string(),
            MountType::Tmpfs => "tmpfs".to_string(),
        },
        source: mount.source.clone(),
        target: mount.target.clone(),
        read_only: mount.read_only,
        consistency: mount.consistency.clone(),
    }
}

/// Dedupe compose mounts by target path, last-wins, preserving each target's
/// original position. Mirrors `merge_mounts`' union-by-target semantics
/// (`deacon_core::mount`), which `generate_injection_override` does NOT apply
/// on its own — it renders every `additional_mounts` entry verbatim. Without
/// this, a later mount source (config, then CLI) for the same target would be
/// appended alongside the earlier one instead of overriding it.
fn dedupe_compose_mounts_by_target(
    mounts: Vec<deacon_core::compose::ComposeMount>,
) -> Vec<deacon_core::compose::ComposeMount> {
    let mut by_target: IndexMap<String, deacon_core::compose::ComposeMount> = IndexMap::new();
    for mount in mounts {
        by_target.insert(mount.target.clone(), mount);
    }
    by_target.into_values().collect()
}

/// Handle shutdown for compose configurations
#[instrument(skip(config, state_manager, docker_path))]
pub(crate) async fn handle_compose_shutdown(
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
            compose_manager.stop_project(project).await?;
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

    /// Bead 14b: confirms compose `build.context` and `build.dockerfile` paths
    /// resolve relative to the compose file's directory, not the workspace.
    /// This is the subtle compose semantic the issue called out for a focused
    /// test: a workspace-relative resolution would have produced the wrong
    /// path on stacked compose files in a subdirectory.
    #[test]
    fn resolve_compose_path_relative_joins_base() {
        let base = std::path::Path::new("/repo/compose-dir");
        let p = resolve_compose_path(base, "Dockerfile.dev");
        assert_eq!(
            p,
            std::path::PathBuf::from("/repo/compose-dir/Dockerfile.dev")
        );
    }

    #[test]
    fn resolve_compose_path_dot_returns_base_itself() {
        let base = std::path::Path::new("/repo/compose-dir");
        let p = resolve_compose_path(base, ".");
        // PathBuf::join with "." appends a dot component but compose treats it
        // semantically as the base; this is fine for downstream `-f` and
        // context arguments which accept either form. We assert the literal
        // join result so any change to that contract is observable.
        assert_eq!(p, std::path::PathBuf::from("/repo/compose-dir/."));
    }

    #[test]
    fn resolve_compose_path_subdir_is_joined() {
        let base = std::path::Path::new("/repo/compose-dir");
        let p = resolve_compose_path(base, "build/Dockerfile");
        assert_eq!(
            p,
            std::path::PathBuf::from("/repo/compose-dir/build/Dockerfile")
        );
    }

    #[test]
    fn resolve_compose_path_absolute_unchanged() {
        let base = std::path::Path::new("/repo/compose-dir");
        let p = resolve_compose_path(base, "/absolute/Dockerfile");
        assert_eq!(p, std::path::PathBuf::from("/absolute/Dockerfile"));
    }

    #[test]
    fn resolve_compose_path_parent_traversal_kept_relative() {
        // Compose allows `../sibling` as a context; we preserve it verbatim and
        // let the OS resolve it during `docker buildx build`.
        let base = std::path::Path::new("/repo/compose-dir");
        let p = resolve_compose_path(base, "../sibling");
        assert_eq!(p, std::path::PathBuf::from("/repo/compose-dir/../sibling"));
    }

    /// #266: `merge_mounts` normalizes both string and object `config.mounts`
    /// forms to Docker CLI mount-string format; `normalized_mount_to_compose_mount`
    /// must map either form's parsed result into `ComposeMount` identically.
    #[test]
    fn config_mounts_string_and_object_forms_normalize_identically() {
        let string_mount =
            serde_json::json!("source=/host/a,target=/container/a,type=bind,readonly");
        let object_mount = serde_json::json!({
            "source": "/host/b",
            "target": "/container/b",
            "type": "volume"
        });

        let merged = deacon_core::mount::merge_mounts(&[string_mount, object_mount], &[], None)
            .expect("merge_mounts should accept both string and object config mount forms");

        let compose_mounts: Vec<_> = merged
            .mounts
            .iter()
            .map(|s| {
                let parsed = NormalizedMount::parse(s).expect("normalized mount string reparses");
                normalized_mount_to_compose_mount(&parsed)
            })
            .collect();

        let a = compose_mounts
            .iter()
            .find(|m| m.target == "/container/a")
            .expect("string-form mount present");
        assert_eq!(a.mount_type, "bind");
        assert_eq!(a.source, "/host/a");
        assert!(a.read_only);

        let b = compose_mounts
            .iter()
            .find(|m| m.target == "/container/b")
            .expect("object-form mount present");
        assert_eq!(b.mount_type, "volume");
        assert_eq!(b.source, "/host/b");
        assert!(!b.read_only);
    }

    #[test]
    fn config_mounts_tmpfs_string_form_normalizes_without_source() {
        let merged = deacon_core::mount::merge_mounts(
            &[serde_json::json!("type=tmpfs,target=/mnt/config-tmp")],
            &[],
            None,
        )
        .expect("merge_mounts should accept tmpfs mount string");

        assert_eq!(merged.mounts.len(), 1);
        let parsed =
            NormalizedMount::parse(&merged.mounts[0]).expect("normalized mount string reparses");
        let compose_mount = normalized_mount_to_compose_mount(&parsed);

        assert_eq!(compose_mount.mount_type, "tmpfs");
        assert!(compose_mount.source.is_empty());
        assert_eq!(compose_mount.target, "/mnt/config-tmp");
    }

    #[test]
    fn config_mounts_tmpfs_destination_key_normalizes_without_source() {
        // #293: the reference CLI accepts `type=tmpfs,destination=…` (Docker's
        // `destination`/`dst` alias for `target`) with no `source`. The config
        // string is normalized by `merge_mounts` before it reaches the compose
        // `NormalizedMount::parse`, so the alias must survive the round trip.
        let merged = deacon_core::mount::merge_mounts(
            &[serde_json::json!("type=tmpfs,destination=/mnt/config-tmp")],
            &[],
            None,
        )
        .expect("merge_mounts should accept tmpfs mount with destination key");

        assert_eq!(merged.mounts.len(), 1);
        let parsed =
            NormalizedMount::parse(&merged.mounts[0]).expect("normalized mount string reparses");
        let compose_mount = normalized_mount_to_compose_mount(&parsed);

        assert_eq!(compose_mount.mount_type, "tmpfs");
        assert!(compose_mount.source.is_empty());
        assert_eq!(compose_mount.target, "/mnt/config-tmp");
    }

    /// #266: when config and CLI mounts target the same path, the later
    /// source (CLI, appended after config) must win — matching
    /// `merge_mounts`' "config overrides features" precedence extended one
    /// level further ("CLI overrides config").
    #[test]
    fn dedupe_compose_mounts_by_target_last_wins() {
        let workspace_mount = deacon_core::compose::ComposeMount {
            mount_type: "bind".to_string(),
            source: "/host/workspace".to_string(),
            target: "/workspaces/app".to_string(),
            read_only: false,
            consistency: None,
        };
        let config_mount = deacon_core::compose::ComposeMount {
            mount_type: "bind".to_string(),
            source: "/host/config-src".to_string(),
            target: "/data".to_string(),
            read_only: false,
            consistency: None,
        };
        let cli_mount = deacon_core::compose::ComposeMount {
            mount_type: "volume".to_string(),
            source: "cli-vol".to_string(),
            target: "/data".to_string(),
            read_only: true,
            consistency: None,
        };

        let deduped = dedupe_compose_mounts_by_target(vec![
            workspace_mount.clone(),
            config_mount,
            cli_mount.clone(),
        ]);

        assert_eq!(deduped.len(), 2, "targets /workspaces/app and /data only");
        let workspace_result = deduped
            .iter()
            .find(|m| m.target == "/workspaces/app")
            .expect("workspace mount preserved");
        assert_eq!(workspace_result.source, workspace_mount.source);

        let data_result = deduped
            .iter()
            .find(|m| m.target == "/data")
            .expect("deduped /data mount present");
        assert_eq!(
            data_result.source, cli_mount.source,
            "CLI mount must win over config mount for the same target"
        );
        assert_eq!(data_result.mount_type, "volume");
        assert!(data_result.read_only);
    }
}
