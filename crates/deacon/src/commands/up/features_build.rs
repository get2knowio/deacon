//! Feature image building with BuildKit.
//!
//! This module contains:
//! - `FeatureBuildOutput` - Output from feature image building
//! - `build_image_with_features` - Build extended image from a base `image:` reference
//! - `build_image_with_features_from_dockerfile` - Build extended image when the
//!   base is a user-authored Dockerfile + context directory (compose `build:` shape)
//! - `prepare_feature_layer` - Resolve/stage Features and emit the install stage
//!   WITHOUT building, so a caller can fold it into a build it drives itself
//!   (`deacon build`'s single-invocation path)
//! - `copy_dir_all` - Recursive directory copy helper

use crate::commands::shared::lockfile::{LockfilePolicy, resolve_lockfile_pins};
use anyhow::{Context, Result};
use deacon_core::build::BuildOptions;
use deacon_core::config::DevContainerConfig;
use deacon_core::container::ContainerIdentity;
use deacon_core::docker::Docker;
use deacon_core::dockerfile_generator::{
    DockerfileConfig, DockerfileGenerator, FeatureInstallEnv, HOST_CA_BUILD_CONTEXT,
    HOST_CA_MOUNT_TARGET,
};
use deacon_core::errors::DeaconError;
use deacon_core::feature_ref::canonicalize_user_feature_id;
use deacon_core::features::{
    FeatureDependencyResolver, InstallationPlan, OptionSetKey, OptionValue, ResolvedFeature,
};
use deacon_core::host_ca::{CorporateCaSet, build_install_script};
use deacon_core::lockfile::{Lockfile, LockfileFeature, LockfilePins};
use deacon_core::oci::{DownloadedFeature, FeatureRef, default_fetcher};
use deacon_core::registry_parser::parse_registry_reference;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, info, instrument, warn};

/// Stage the corporate-CA bundle + install script into a build-context dir for
/// build-time host-CA injection (016, T038). The generated Dockerfile mounts
/// this dir at `/tmp/deacon-ca` and runs `install.sh`, which copies
/// `host-ca.crt` to the canonical path and updates the distro trust store.
/// Deterministic content → byte-stable layer for a given CA set (FR-017).
async fn stage_host_ca_context(temp_dir: &Path, set: &CorporateCaSet) -> Result<PathBuf> {
    let dir = temp_dir.join("deacon-ca");
    tokio::fs::create_dir_all(&dir).await?;
    tokio::fs::write(dir.join("host-ca.crt"), set.pem_bundle.as_bytes()).await?;
    // The script reads the bundle from the build mount target (same constant the
    // generated RUN step mounts it at) and installs it to the canonical path.
    let script = build_install_script(&format!("{HOST_CA_MOUNT_TARGET}/host-ca.crt"));
    tokio::fs::write(dir.join("install.sh"), script.as_bytes()).await?;
    Ok(dir)
}

/// Output from building an image with features
#[derive(Debug, Clone)]
pub(crate) struct FeatureBuildOutput {
    /// Extended image tag with features installed
    pub image_tag: String,
    /// Combined environment variables from all features
    pub combined_env: HashMap<String, String>,
    /// Resolved features in installation order
    pub resolved_features: Vec<deacon_core::features::ResolvedFeature>,
    /// Lockfile entries for the features installed in this build.
    /// Keyed by the user-provided feature ID (as it appears in `devcontainer.json`).
    /// Empty when the config has no features.
    pub lockfile: Lockfile,
    /// The `devcontainer.metadata` value written onto the produced image, when
    /// the caller asked for one (`metadata_raw_config`). `None` when it did not,
    /// or when there was nothing to record.
    pub metadata_label: Option<String>,
}

/// Compute the `devcontainer.metadata` label a Feature build should write, from
/// the base image's own entries plus one per installed Feature plus the config
/// pick — the same construction every other deacon build shape uses (#436).
///
/// Returns `None` when the caller did not ask for a label, or when there is
/// nothing to record: the reference writes no label at all in that case.
///
/// `raw_config` MUST be the configuration as authored — the label travels with
/// the image, so a substituted host path would be wrong for every consumer but
/// the machine that built it (#373).
async fn compute_metadata_label(
    cli: &deacon_core::docker::CliRuntime,
    base_image_ref: Option<&str>,
    raw_config: Option<&DevContainerConfig>,
    features: &[ResolvedFeature],
) -> Option<String> {
    let raw_config = raw_config?;
    let entries = crate::commands::up::merged_config::container_metadata_entries(
        cli,
        base_image_ref,
        raw_config,
        features,
    )
    .await;
    if entries.is_empty() {
        debug!("No devcontainer.metadata entries to record; leaving the label unset");
        return None;
    }
    match serde_json::to_string(&serde_json::Value::Array(entries)) {
        Ok(json) => Some(json),
        Err(e) => {
            warn!(error = %e, "Failed to serialize the devcontainer.metadata label; leaving it unset");
            None
        }
    }
}

/// The stage name every deacon-generated Feature-install layer ends at, and the
/// `--target` every build that carries one must ask for.
pub(crate) const FEATURE_TARGET_STAGE: &str = "dev_containers_target_stage";

/// Recover the `K=V` pairs from a pre-formatted argv slice's `--build-arg` flags,
/// so a `FROM $ARG` or `USER $ARG` can be resolved against the values this build
/// will really pass. Entries with no `=` declare a passthrough from the ambient
/// environment and carry no value to substitute.
fn build_arg_map(argv: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut iter = argv.iter();
    while let Some(arg) = iter.next() {
        if arg != "--build-arg" {
            continue;
        }
        if let Some((k, v)) = iter.next().and_then(|pair| pair.split_once('=')) {
            map.insert(k.to_string(), v.to_string());
        }
    }
    map
}

/// The stage the Feature layers build on, in a Dockerfile the caller is about to
/// append them to.
///
/// The configuration's `build.target` when it names one — that is the image the
/// user asked for, and the Features belong on top of it, which is what the
/// reference does too. Otherwise the document's final stage, aliased if it has no
/// name of its own so the appended `FROM` has something to say.
///
/// Returns `(dockerfile content to build, base stage name)`; the content differs
/// from the input only when an alias had to be added.
pub(crate) fn base_stage_for_features(
    dockerfile_content: &str,
    target: Option<&str>,
) -> Result<(String, String)> {
    match target {
        Some(target) => Ok((dockerfile_content.to_string(), target.to_string())),
        None => Ok(
            deacon_core::dockerfile_utils::ensure_dockerfile_has_final_stage_name(
                dockerfile_content,
                "dev_containers_base_stage",
            )?,
        ),
    }
}

/// Everything a caller needs to fold Feature installation into a build IT drives.
///
/// This is the half of the Feature pipeline that has nothing to do with running
/// `docker build`: resolve the declared Features, download them, order them, and
/// emit the Dockerfile stage that installs them. Handing that back — rather than
/// building a separate image and leaving the caller to `FROM` it — is what lets
/// `deacon build` produce base + Features in ONE BuildKit invocation, so no
/// daemon-local intermediate tag ever has to be resolvable by the builder (#595).
pub(crate) struct PreparedFeatureLayer {
    /// `FROM <base stage> AS dev_containers_target_stage` plus one RUN-mount per
    /// Feature. Appended verbatim after the base Dockerfile's own content.
    pub install_stage: String,
    /// The named build contexts the install stage's mounts resolve against, as
    /// `(name, path)` pairs. Kept unformatted because the two executors spell them
    /// differently: `--build-context name=path` for buildx, an
    /// `additional_contexts:` entry for a Compose-driven build (#629).
    pub build_contexts: Vec<(String, String)>,
    /// Feature-contributed `containerEnv`, in install order.
    pub combined_env: HashMap<String, String>,
    /// The Features that will be installed, in installation order — one
    /// `devcontainer.metadata` entry each.
    pub resolved_features: Vec<ResolvedFeature>,
    /// Lockfile assembled from the resolved + downloaded Features.
    pub lockfile: Lockfile,
}

impl PreparedFeatureLayer {
    /// [`Self::build_contexts`] spelled the way `docker buildx build` takes them.
    pub fn buildx_context_args(&self) -> Vec<String> {
        self.build_contexts
            .iter()
            .flat_map(|(name, path)| ["--build-context".to_string(), format!("{}={}", name, path)])
            .collect()
    }
}

/// Resolve, download and stage the configuration's Features, then emit the
/// Dockerfile stage that installs them on top of `base_stage` — WITHOUT building
/// anything.
///
/// `base_stage` is written into the generated `FROM` literally, so it must name a
/// stage declared earlier in the SAME Dockerfile the caller assembles. That
/// literal form is deliberate: BuildKit only expands a global `ARG` inside a
/// `FROM` when the `ARG` precedes every `FROM` in the file, a window that closes
/// as soon as user-authored stages are prepended.
#[allow(clippy::too_many_arguments)]
#[instrument(skip(config, identity, host_ca_set), fields(base_stage = %base_stage))]
pub(crate) async fn prepare_feature_layer(
    config: &DevContainerConfig,
    identity: &ContainerIdentity,
    config_path: &Path,
    base_stage: &str,
    feature_install_env: FeatureInstallEnv,
    host_ca_set: Option<&CorporateCaSet>,
    lockfile_policy: LockfilePolicy,
) -> Result<PreparedFeatureLayer> {
    let staged = resolve_and_stage_features(config, identity, config_path, lockfile_policy).await?;

    let host_ca_dir = match host_ca_set {
        Some(set) if !set.is_empty() => Some(stage_host_ca_context(&staged.temp_dir, set).await?),
        _ => None,
    };

    let generator = DockerfileGenerator::new(DockerfileConfig {
        base_image: base_stage.to_string(),
        target_stage: FEATURE_TARGET_STAGE.to_string(),
        features_source_dir: staged.features_source_dir.display().to_string(),
        feature_install_env,
        host_ca_build_context: host_ca_dir.as_ref().map(|p| p.display().to_string()),
        config_container_env: config.container_env().clone(),
    });
    let install_stage = generator.generate_install_stage_from(&staged.plan, base_stage)?;

    let mut build_contexts = Vec::new();
    if let Some(ref ca_dir) = host_ca_dir {
        build_contexts.push((
            HOST_CA_BUILD_CONTEXT.to_string(),
            ca_dir.display().to_string(),
        ));
    }
    build_contexts.push((
        "dev_containers_feature_content_source".to_string(),
        staged.features_source_dir.display().to_string(),
    ));

    Ok(PreparedFeatureLayer {
        install_stage,
        build_contexts,
        combined_env: staged.combined_env,
        resolved_features: staged.plan.features.clone(),
        lockfile: staged.lockfile,
    })
}

/// Internal: result of resolving + downloading + staging features for a build.
struct StagedFeatures {
    plan: InstallationPlan,
    combined_env: HashMap<String, String>,
    temp_dir: PathBuf,
    features_source_dir: PathBuf,
    /// Lockfile assembled from the resolved + downloaded features.
    /// Keyed by the user-provided feature ID (as it appears in
    /// `devcontainer.json`), matching upstream `generateLockfile` in
    /// `devcontainers/cli` `src/spec-configuration/lockfile.ts`.
    lockfile: Lockfile,
}

/// Build an extended Docker image with features installed using BuildKit.
///
/// This is the `image:`-shape entry point: `config.image` must be set. The
/// returned image extends the base image with one BuildKit RUN-mount per
/// resolved feature, targeting a synthesized stage named
/// `dev_containers_target_stage`.
///
/// For the compose `build:` shape, see [`build_image_with_features_from_dockerfile`].
///
/// # Arguments
///
/// * `config` - DevContainer configuration containing features to install (and `image`)
/// * `identity` - Container identity for deterministic naming
/// * `_workspace_folder` - Workspace folder path (reserved for future use)
/// * `build_options` - Optional build options for cache-from/cache-to/buildx settings
///
/// When `build_options` is provided and not default, cache arguments are included
/// in the generated build command. This enables cache-from/cache-to/no-cache/builder
/// options to propagate to feature builds per spec (data-model.md).
#[allow(clippy::too_many_arguments)]
#[instrument(skip(config, identity, build_options, cli))]
pub(crate) async fn build_image_with_features(
    config: &DevContainerConfig,
    identity: &ContainerIdentity,
    _workspace_folder: &Path,
    config_path: &Path,
    build_options: Option<&BuildOptions>,
    host_ca_set: Option<&CorporateCaSet>,
    cli: &deacon_core::docker::CliRuntime,
    lockfile_policy: LockfilePolicy,
    metadata_raw_config: Option<&DevContainerConfig>,
) -> Result<FeatureBuildOutput> {
    info!("Building extended image with features");

    // Get base image
    let base_image = config
        .image
        .as_ref()
        .ok_or_else(|| DeaconError::Runtime("No base image specified".to_string()))?;

    // Parse features from config
    let features_obj = config
        .features()
        .as_object()
        .ok_or_else(|| DeaconError::Runtime("Features must be an object".to_string()))?;

    if features_obj.is_empty() {
        return Ok(FeatureBuildOutput {
            image_tag: base_image.clone(),
            combined_env: HashMap::new(),
            resolved_features: Vec::new(),
            lockfile: Lockfile {
                features: HashMap::new(),
            },
            metadata_label: None,
        });
    }

    let staged = resolve_and_stage_features(config, identity, config_path, lockfile_policy).await?;

    // Generate Dockerfile.
    //
    // Spec parity (#89): surface `_REMOTE_USER`, `_REMOTE_USER_HOME`,
    // `_CONTAINER_USER`, `_CONTAINER_USER_HOME` to every feature's
    // `install.sh`. These are resolved from the base image's
    // `devcontainer.metadata` LABEL + baked-in `USER` folded under the user
    // config — NOT from the user config alone, since `remoteUser` is commonly
    // declared by the base image. Empty values are still emitted so
    // `${_REMOTE_USER:-}` resolves to "" rather than `<unset>`.
    let feature_install_env = crate::commands::up::merged_config::resolve_feature_install_env(
        cli, base_image, config, // No Dockerfile on this path: the base IS the image.
        None,
    )
    .await;

    // Build-time host-CA injection (016, T038/T039): stage the bundle + script
    // when a non-empty corporate set was supplied.
    let host_ca_build_context = match host_ca_set {
        Some(set) if !set.is_empty() => {
            let dir = stage_host_ca_context(&staged.temp_dir, set).await?;
            Some(dir.display().to_string())
        }
        _ => None,
    };

    let dockerfile_config = DockerfileConfig {
        base_image: base_image.clone(),
        target_stage: "dev_containers_target_stage".to_string(),
        features_source_dir: staged.features_source_dir.display().to_string(),
        feature_install_env,
        host_ca_build_context,
        config_container_env: config.container_env().clone(),
    };

    let generator = DockerfileGenerator::new(dockerfile_config.clone());
    let dockerfile_content = generator.generate(&staged.plan)?;

    // Write Dockerfile
    let dockerfile_path = staged.temp_dir.join("Dockerfile.extended");
    tokio::fs::write(&dockerfile_path, dockerfile_content.as_bytes()).await?;

    debug!("Generated Dockerfile at {}", dockerfile_path.display());

    // Generate image tag
    let extended_image_tag = format!("deacon-devcontainer-features:{}", identity.workspace_hash);

    ensure_buildkit_or_error().await?;
    log_cache_configuration(build_options);

    // `devcontainer.metadata`, when the caller wants one recorded, computed from
    // the base image the `FROM` names — knowable before the build, which is what
    // lets it ride this invocation instead of a second one (#595). The image is
    // already local: `resolve_feature_install_env` above pulled it.
    let metadata_label = compute_metadata_label(
        cli,
        Some(base_image.as_str()),
        metadata_raw_config,
        &staged.plan.features,
    )
    .await;
    let extra_labels: Vec<String> = metadata_label
        .iter()
        .map(|m| format!("devcontainer.metadata={}", m))
        .collect();

    // `base_image` here is a REGISTRY reference the configuration named, so every
    // buildx driver can resolve this build's `FROM` and no builder is pinned (#595).
    let build_args = generator.generate_build_args(
        &dockerfile_path,
        &extended_image_tag,
        build_options,
        &extra_labels,
    );

    debug!("Building image with args: {:?}", build_args);
    let mode = build_options.map(|o| o.output_mode).unwrap_or_default();
    let renderer = crate::ui::build_render::BuildRenderer::for_mode(
        mode,
        staged.plan.features.iter().map(|f| f.id.as_str()),
    );
    let build_result = cli
        .build_image(&build_args, crate::ui::build_render::io_for(&renderer))
        .await;
    if let Some(r) = &renderer {
        r.finish(build_result.is_ok());
    }
    let _image_id = build_result?;

    info!("Successfully built extended image: {}", extended_image_tag);

    Ok(FeatureBuildOutput {
        image_tag: extended_image_tag,
        combined_env: staged.combined_env,
        resolved_features: staged.plan.features.clone(),
        lockfile: staged.lockfile,
        metadata_label,
    })
}

/// The document, the Feature layer and the label a Dockerfile-based Feature build
/// needs, resolved WITHOUT running anything.
///
/// Two executors consume it: `up`'s single-container path, which runs one
/// `docker buildx build`, and the Compose `build:` path, which hands the same
/// document to `docker compose build` so the service's own `build:` keys keep
/// applying (#629). Sharing the preparation is what keeps those two from drifting
/// on what a Feature layer is.
pub(crate) struct DockerfileFeatureBuild {
    /// The resolved Features and the stage that installs them.
    pub prepared: PreparedFeatureLayer,
    /// Where the merged base + Feature document was written. Outside the build
    /// context on purpose — deacon does not write into the user's workspace.
    pub dockerfile_path: PathBuf,
    /// `devcontainer.metadata` for the produced image, when the caller asked for
    /// one. Computed from the base image BEFORE the build so the build itself can
    /// write it, rather than a second build `FROM` a daemon-local tag (#595).
    pub metadata_label: Option<String>,
}

/// Resolve the Features, emit the merged Dockerfile, and compute the metadata
/// label for a build whose base is a user-authored Dockerfile.
///
/// `base_dockerfile_content` must already have its base stage named
/// (`base_stage_for_features` / `ensure_dockerfile_has_final_stage_name`), and
/// `declared_build_args` are the `ARG` values the build will really pass, so a
/// `FROM $ARG` or `USER $ARG` resolves against them rather than against a guess.
#[allow(clippy::too_many_arguments)]
#[instrument(
    skip(config, identity, base_dockerfile_content, declared_build_args),
    fields(base_stage = %base_dockerfile_final_stage, target = ?target)
)]
pub(crate) async fn prepare_dockerfile_feature_build(
    config: &DevContainerConfig,
    identity: &ContainerIdentity,
    base_dockerfile_content: &str,
    base_dockerfile_final_stage: &str,
    config_path: &Path,
    target: Option<&str>,
    declared_build_args: &HashMap<String, String>,
    host_ca_set: Option<&CorporateCaSet>,
    cli: &deacon_core::docker::CliRuntime,
    lockfile_policy: LockfilePolicy,
    metadata_raw_config: Option<&DevContainerConfig>,
) -> Result<DockerfileFeatureBuild> {
    info!(
        "Preparing feature layer on top of user-authored Dockerfile (stage={})",
        base_dockerfile_final_stage
    );

    let features_obj = config
        .features()
        .as_object()
        .ok_or_else(|| DeaconError::Runtime("Features must be an object".to_string()))?;
    if features_obj.is_empty() {
        return Err(DeaconError::Runtime(
            "prepare_dockerfile_feature_build called with no features".to_string(),
        )
        .into());
    }

    // `_REMOTE_USER` / `_CONTAINER_USER` for this shape. The "base" is a *stage
    // name* inside the user's Dockerfile, so there is no image to inspect until
    // that stage has been built — but the Dockerfile itself may say who it runs
    // as, and the reference reads exactly that (`findUserStatement`) before
    // falling back to the base image (#89).
    let dockerfile_user = deacon_core::dockerfile_utils::find_user_statement(
        base_dockerfile_content,
        declared_build_args,
        target,
    );
    let feature_install_env = FeatureInstallEnv::resolve(
        config.remote_user.as_deref(),
        config.container_user.as_deref(),
        dockerfile_user.as_deref(),
    );

    let prepared = prepare_feature_layer(
        config,
        identity,
        config_path,
        base_dockerfile_final_stage,
        feature_install_env,
        host_ca_set,
        lockfile_policy,
    )
    .await?;

    // Final Dockerfile: user prologue + feature install stage. The user's
    // Dockerfile may carry a `# syntax=` directive at the very top; that's already
    // preserved because we copy the full content first.
    let combined = merge_dockerfile_with_feature_stage(base_dockerfile_content, &prepared);

    // Write the merged Dockerfile to a temp dir (NOT into the user's context dir,
    // so we never pollute the workspace). Both executors read it by path, which
    // BuildKit resolves independently of where the context directory lives.
    let temp_dir =
        crate::commands::shared::feature_resolver::feature_staging_root(&identity.workspace_hash);
    tokio::fs::create_dir_all(&temp_dir).await?;
    let dockerfile_path = temp_dir.join("Dockerfile.extended");
    tokio::fs::write(&dockerfile_path, combined.as_bytes()).await?;
    debug!(
        "Wrote merged Dockerfile ({} bytes) at {}",
        combined.len(),
        dockerfile_path.display()
    );

    // The entries the base contributes come from the EXTERNAL image this
    // Dockerfile derives from, resolved from the document itself — the
    // reference's `findBaseImage`.
    let base_image_ref = deacon_core::dockerfile_utils::resolve_base_image(
        base_dockerfile_content,
        declared_build_args,
        target,
    );
    if let Some(image) = &base_image_ref {
        if let Err(e) = cli.ensure_image_available(image).await {
            debug!(
                image = %image,
                error = %e,
                "Could not make the base image available for metadata inspection; \
                 proceeding with no inherited devcontainer.metadata entries"
            );
        }
    }
    let metadata_label = compute_metadata_label(
        cli,
        base_image_ref.as_deref(),
        metadata_raw_config,
        &prepared.resolved_features,
    )
    .await;

    Ok(DockerfileFeatureBuild {
        prepared,
        dockerfile_path,
        metadata_label,
    })
}

/// Build an extended Docker image with features installed when the base
/// description is a user-authored Dockerfile under `base_context_dir`.
///
/// Used by `up`'s single-container Dockerfile path: the stage-name parser rewrites
/// the user's final `FROM` to carry a deterministic alias, then we concatenate our
/// feature-install stage that targets that alias. The merged Dockerfile is written
/// to a temp directory and built with the user's original context directory so the
/// existing `COPY`/`ADD` directives keep resolving the right files.
///
/// The Compose `build:` shape does NOT come through here: it shares the
/// preparation ([`prepare_dockerfile_feature_build`]) but lets Compose drive the
/// build, so the service's own `build:` keys keep applying (#629).
///
/// # Arguments
///
/// * `config` - DevContainer configuration containing features to install
/// * `identity` - Container identity for deterministic naming
/// * `base_dockerfile_content` - The user's Dockerfile contents, already
///   processed by `base_stage_for_features` so the stage the Features install on
///   carries the name `base_dockerfile_final_stage`
/// * `base_dockerfile_final_stage` - The name of that stage; our feature-install
///   stage will `FROM <stage>`
/// * `base_context_dir` - The configuration's build context, resolved to an
///   absolute path. This is passed as the BuildKit context so the user's
///   relative `COPY`/`ADD` paths keep working
/// * `target` - The configuration's `build.target`, when it declares one. The
///   caller has already resolved it into `base_dockerfile_final_stage`; it is
///   passed again so a `FROM $ARG` / `USER $ARG` is read against the right stage.
/// * `build_options` - Optional build options for cache-from/cache-to/buildx settings
/// * `extra_build_args` - argv the base half of this build needs and the Feature
///   half does not: the configuration's `build.args` as `--build-arg K=V` pairs
///   and its `build.options` verbatim. Since the base is built by THIS invocation
///   rather than by an earlier one, anything the base needs has to arrive here.
#[allow(clippy::too_many_arguments)]
#[instrument(
    skip(config, identity, base_dockerfile_content, build_options, extra_build_args),
    fields(
        base_stage = %base_dockerfile_final_stage,
        base_context = %base_context_dir.display(),
        target = ?target,
    )
)]
pub(crate) async fn build_image_with_features_from_dockerfile(
    config: &DevContainerConfig,
    identity: &ContainerIdentity,
    base_dockerfile_content: &str,
    base_dockerfile_final_stage: &str,
    base_context_dir: &Path,
    config_path: &Path,
    target: Option<&str>,
    build_options: Option<&BuildOptions>,
    host_ca_set: Option<&CorporateCaSet>,
    cli: &deacon_core::docker::CliRuntime,
    lockfile_policy: LockfilePolicy,
    metadata_raw_config: Option<&DevContainerConfig>,
    extra_build_args: &[String],
) -> Result<FeatureBuildOutput> {
    let build = prepare_dockerfile_feature_build(
        config,
        identity,
        base_dockerfile_content,
        base_dockerfile_final_stage,
        config_path,
        target,
        // The `--build-arg` values this build carries, as a map, so a `FROM $ARG`
        // or `USER $ARG` in the Dockerfile resolves to what the build will
        // actually use.
        &build_arg_map(extra_build_args),
        host_ca_set,
        cli,
        lockfile_policy,
        metadata_raw_config,
    )
    .await?;
    let DockerfileFeatureBuild {
        prepared,
        dockerfile_path,
        metadata_label,
    } = build;

    let extended_image_tag = format!("deacon-devcontainer-features:{}", identity.workspace_hash);

    ensure_buildkit_or_error().await?;
    log_cache_configuration(build_options);

    // Build args: hand-rolled here (NOT the generator's defaults) because the
    // generator passes `--build-arg _DEV_CONTAINERS_BASE_IMAGE=...` which is
    // unused (and emits a BuildKit warning) when the FROM is literal. We
    // still pass `--target` so BuildKit stops at our feature stage even if
    // the user has further stages after it, plus `--build-context` so the
    // RUN-mount lines resolve to the staged features directory.
    //
    // No `--builder` is pinned. Every input this invocation resolves is either a
    // stage in the Dockerfile it was handed or an image a registry can serve, so
    // any buildx driver can execute it and the builder the user selected is the
    // one that should (#595). `cli` is unused for driver detection here for the
    // same reason.
    let mut build_args: Vec<String> = vec![
        "buildx".to_string(),
        "build".to_string(),
        "--load".to_string(),
    ];

    if let Some(opts) = build_options {
        if !opts.is_default() {
            build_args.extend(opts.to_docker_args());
        }
    }

    // The base half's own `build.args` / `build.options`. They reach BuildKit here
    // because there is no separate base build any more (#595).
    build_args.extend(extra_build_args.iter().cloned());

    // `devcontainer.metadata`, when the caller wants one recorded — computed
    // BEFORE the build, so this invocation can write it (#595).
    if let Some(metadata) = &metadata_label {
        build_args.push("--label".to_string());
        build_args.push(format!("devcontainer.metadata={}", metadata));
    }

    build_args.extend(prepared.buildx_context_args());
    build_args.extend(vec![
        "--target".to_string(),
        FEATURE_TARGET_STAGE.to_string(),
        "-f".to_string(),
        dockerfile_path.display().to_string(),
        "-t".to_string(),
        extended_image_tag.clone(),
        base_context_dir.display().to_string(),
    ]);

    debug!("Building image with args: {:?}", build_args);
    let mode = build_options.map(|o| o.output_mode).unwrap_or_default();
    let renderer = crate::ui::build_render::BuildRenderer::for_mode(
        mode,
        prepared.resolved_features.iter().map(|f| f.id.as_str()),
    );
    let build_result = cli
        .build_image(&build_args, crate::ui::build_render::io_for(&renderer))
        .await;
    if let Some(r) = &renderer {
        r.finish(build_result.is_ok());
    }
    let _image_id = build_result.with_context(|| {
        format!(
            "Failed to build feature-extended image from Dockerfile {} (context {})",
            dockerfile_path.display(),
            base_context_dir.display(),
        )
    })?;

    info!(
        "Successfully built extended image from Dockerfile: {}",
        extended_image_tag
    );

    Ok(FeatureBuildOutput {
        image_tag: extended_image_tag,
        combined_env: prepared.combined_env,
        resolved_features: prepared.resolved_features,
        lockfile: prepared.lockfile,
        metadata_label,
    })
}

/// Splice a prepared Feature-install stage onto the end of a base Dockerfile,
/// producing the single document one BuildKit invocation builds.
///
/// The base content is copied verbatim — including any `# syntax=` parser
/// directive, which must stay on line 1 — and the install stage is appended after
/// a blank line so the two never share one instruction.
pub(crate) fn merge_dockerfile_with_feature_stage(
    base_dockerfile_content: &str,
    prepared: &PreparedFeatureLayer,
) -> String {
    let mut combined =
        String::with_capacity(base_dockerfile_content.len() + prepared.install_stage.len() + 2);
    combined.push_str(base_dockerfile_content);
    if !base_dockerfile_content.ends_with('\n') {
        combined.push('\n');
    }
    combined.push('\n');
    combined.push_str(&prepared.install_stage);
    combined
}

/// Convert a `devcontainer.json` feature-options JSON value (the value side of
/// a `features` entry, or of a `dependsOn` entry) into the internal option map.
/// A non-object value (e.g. `true`) yields no options.
fn parse_feature_options(value: &serde_json::Value) -> HashMap<String, OptionValue> {
    let Some(obj) = value.as_object() else {
        return HashMap::new();
    };
    obj.iter()
        .map(|(k, v)| {
            let opt = match v {
                serde_json::Value::Bool(b) => OptionValue::Boolean(*b),
                serde_json::Value::String(s) => OptionValue::String(s.clone()),
                serde_json::Value::Number(n) => OptionValue::Number(n.clone()),
                serde_json::Value::Array(a) => OptionValue::Array(a.clone()),
                serde_json::Value::Object(o) => OptionValue::Object(o.clone()),
                serde_json::Value::Null => OptionValue::Null,
            };
            (k.clone(), opt)
        })
        .collect()
}

/// One requested Feature instance: the resource it names plus the options it was
/// requested with. Both halves are the Feature's identity per
/// `feature-dependencies.md` §Definition: Feature Equality, so the set is a `Vec` and not
/// a map keyed by id — the same Feature appears once per distinct option set (#489) and
/// once per declared version (#430).
struct RequestedFeature {
    /// Install key: the tag-bearing OCI reference or `local:<abs path>`.
    canonical_id: String,
    feature_ref: FeatureRef,
    /// Options AS AUTHORED. Declared defaults are applied downstream, at the point they
    /// are consumed, so they cannot blur two differently-authored instances together.
    options: HashMap<String, OptionValue>,
}

/// Is this exact Feature — same resource AND same option set — already requested?
///
/// The resource half is compared by NAME (the id without version or digest) rather than
/// by install key, deliberately. A hard dependency is written without a pin more often
/// than not (`ghcr.io/devcontainers/features/common-utils`), and a user who declared that
/// Feature at a specific version has already satisfied it — installing a second copy at
/// `:latest` would ignore their pin. The user's own two-version declaration is a
/// deliberate double install (#430), an auto-pulled hard dependency is not.
///
/// The options half is what `feature-dependencies.md` §(B1) means by skipping only "the
/// **exact** Feature": a `dependsOn` asking for different options than the one already in
/// the set names a DIFFERENT Feature, and both install (#489).
fn already_installed(
    requested: &[RequestedFeature],
    dep_install_key: &str,
    dep_options: &HashMap<String, OptionValue>,
) -> bool {
    let dep_resource = deacon_core::features::canonical_feature_id(dep_install_key);
    let dep_option_set = OptionSetKey::of(dep_options);
    requested.iter().any(|r| {
        deacon_core::features::canonical_feature_id(&r.canonical_id) == dep_resource
            && OptionSetKey::of(&r.options) == dep_option_set
    })
}

/// Fetch one OCI Feature, pinned to the lockfile's `integrity` for
/// `user_feature_id` when the lockfile recorded one (#571).
///
/// The pin is a LOOKUP KEY, not an after-the-fact comparison: see
/// [`deacon_core::oci::FeatureFetcher::fetch_feature_pinned`].
async fn fetch_feature_honoring_pins<C: deacon_core::oci::HttpClient>(
    fetcher: &deacon_core::oci::FeatureFetcher<C>,
    feature_ref: &FeatureRef,
    user_feature_id: &str,
    pins: &LockfilePins,
) -> Result<DownloadedFeature> {
    let pin = pins.pin_for(user_feature_id);
    if let Some(digest) = pin {
        debug!(
            feature = %user_feature_id,
            integrity = %digest,
            "Resolving Feature at the digest the lockfile pins"
        );
    }
    Ok(fetcher.fetch_feature_pinned(feature_ref, pin).await?)
}

/// Name the lockfile as the thing that shaped a failed fetch, since a registry
/// asked for a digest it does not have answers with an ordinary 404 or 401 and
/// nothing in that answer mentions a workspace file.
///
/// Deliberately does NOT assert that the content changed. A pinned request can
/// also fail for the reasons an unpinned one does — the registry is
/// unreachable, or the caller is not logged in — and the reference's own
/// message for this case hedges the same way ("You may not have permission to
/// access this Feature, or may not be logged in"). What is certain, and what
/// the user cannot see otherwise, is which digest was asked for and why.
fn pinned_fetch_error(
    error: anyhow::Error,
    user_feature_id: &str,
    pins: &LockfilePins,
) -> anyhow::Error {
    let Some(digest) = pins.pin_for(user_feature_id) else {
        return error;
    };
    error.context(format!(
        "Feature '{}' could not be resolved at {}, the digest the lockfile pins it to. \
         If the Feature's published content changed since the lockfile was written, this \
         refusal is the lockfile doing its job: re-resolve it with `deacon upgrade`, or \
         pass --no-lockfile to resolve by tag for this run.",
        user_feature_id, digest
    ))
}

/// Shared core: parse features from `config`, download them, resolve the
/// installation plan, and stage feature directories into a deterministic temp
/// directory so BuildKit can mount them as the
/// `dev_containers_feature_content_source` build context.
///
/// `config_path` is the absolute path to the resolved `devcontainer.json`.
/// It anchors local feature references (`./feature-X`, `../shared/foo`) so
/// they resolve relative to the config file's directory per the spec,
/// regardless of whether the config was auto-discovered or supplied via
/// `--config` (#69).
#[instrument(skip(config, identity))]
async fn resolve_and_stage_features(
    config: &DevContainerConfig,
    identity: &ContainerIdentity,
    config_path: &Path,
    lockfile_policy: LockfilePolicy,
) -> Result<StagedFeatures> {
    let features_obj = config
        .features()
        .as_object()
        .ok_or_else(|| DeaconError::Runtime("Features must be an object".to_string()))?;

    // Anchor for local feature path resolution: the directory containing
    // the resolved devcontainer.json (#69).
    let config_dir = config_path
        .parent()
        .ok_or_else(|| {
            DeaconError::Runtime(format!(
                "Cannot determine parent directory of config file '{}'",
                config_path.display()
            ))
        })?
        .to_path_buf();

    // Anchor for the spec's `.devcontainer/` containment rule (#488): the
    // workspace folder, which `ContainerIdentity` already carries canonicalized
    // as the `devcontainer.local_folder` label. It is `None` only when the
    // workspace path could not be canonicalized — a workspace that does not
    // exist, which every other step here would fail on anyway — so fail fast
    // rather than skip the check.
    let workspace_root = identity.local_folder.clone().ok_or_else(|| {
        DeaconError::Runtime(
            "Cannot determine the workspace folder for local feature path validation".to_string(),
        )
    })?;

    // Create feature fetcher (used for OCI refs only)
    let fetcher = default_fetcher()?;

    // The content pins the on-disk lockfile imposes on this resolution (#571).
    // Keyed by the Feature id AS WRITTEN, because that is the key
    // `generateLockfile` writes and the key the reference looks up
    // (`lockfile?.features[userFeatureId]?.integrity`) — for a declared Feature
    // that is the `features` map key, and for an auto-installed `dependsOn`
    // target it is the dependency key as the Feature's metadata spells it.
    let pins = resolve_lockfile_pins(lockfile_policy, config_path).await?;

    // Parse, classify, and (for OCI refs) fetch features.
    //
    // Local references are resolved relative to `config_dir` and short-
    // circuit the OCI fetcher entirely. We synthesize a `DownloadedFeature`
    // pointing at the on-disk directory so the downstream staging pipeline
    // (copy into BuildKit context + dependency resolution + Dockerfile
    // generation) treats them identically to fetched features.
    //
    // Every map below is keyed by a feature's INSTALL KEY: the tag-bearing OCI
    // reference (`ghcr.io/devcontainers/features/git:1.3.1`) or `local:<abs path>`.
    // The key must carry the version. A `features` map may legally declare one Feature
    // at two versions — `feature-dependencies.md` §Definition: Feature Equality makes
    // OCI equality depend on the manifest digest, and §Feature authorship states that
    // "a single Feature may be installed more than once" — and keying on the tag-less
    // resource name silently collapsed the two entries into one install (#430).
    let mut requested: Vec<RequestedFeature> = Vec::new();
    // Install key → user-provided feature ID (the key as it appears in
    // `devcontainer.json`). The lockfile MUST be keyed by the user-provided form to
    // match upstream `generateLockfile`.
    let mut user_id_by_canonical: HashMap<String, String> = HashMap::new();
    let mut downloaded_features: HashMap<String, DownloadedFeature> = HashMap::new();

    for (feature_id, feature_options) in features_obj.iter() {
        // An absolute path is classified LOCAL even though the spec forbids it
        // (#495 reversed the #126 capability): `resolve_local_feature_dir`
        // rejects it with the accurate diagnostic and the migration, where
        // falling through to OCI parsing would report a registry 404 instead.
        let is_local = feature_id.starts_with("./")
            || feature_id.starts_with("../")
            || feature_id.starts_with('/');

        let (canonical_id, feature_ref) = if is_local {
            // Resolve `./foo` and `../shared/foo` against the config file's
            // directory (spec contract — *not* the workspace folder, *not*
            // the CWD, regardless of how the config was loaded), then enforce
            // the spec's `.devcontainer/` containment rule (#488).
            let canonical_path = deacon_core::features::resolve_local_feature_dir(
                feature_id,
                &config_dir,
                &workspace_root,
            )?;

            let metadata_path = canonical_path.join("devcontainer-feature.json");
            if !metadata_path.exists() {
                return Err(DeaconError::Runtime(format!(
                    "Local feature at '{}' is missing devcontainer-feature.json (resolved from '{}' relative to {})",
                    canonical_path.display(),
                    feature_id,
                    config_dir.display()
                ))
                .into());
            }
            let metadata =
                deacon_core::features::parse_feature_metadata(&metadata_path).map_err(|e| {
                    DeaconError::Runtime(format!(
                        "Failed to parse local feature metadata at '{}': {}",
                        metadata_path.display(),
                        e
                    ))
                })?;

            // Canonical id for local features: the absolute resolved path.
            // Stable across re-runs from the same config, and uniquely
            // distinguishes "./foo" from any OCI ref.
            let canonical_id = format!("local:{}", canonical_path.display());

            // Synthesize a DownloadedFeature pointing at the local dir.
            // The digest field is reserved for OCI layer cache keys; for
            // local features we use a deterministic marker derived from
            // the absolute path so cache invariants don't trip on it.
            let digest = format!("local:{}", canonical_path.display());
            downloaded_features.insert(
                canonical_id.clone(),
                DownloadedFeature {
                    path: canonical_path.clone(),
                    metadata,
                    digest: digest.clone(),
                    manifest_digest: digest,
                },
            );

            // Build a placeholder FeatureRef — never used for fetching,
            // but kept for downstream APIs that key on this struct. The
            // `reference()` field surfaces the user-visible spelling for
            // logs/errors.
            let feature_ref = FeatureRef::new(
                "local".to_string(),
                "fs".to_string(),
                canonical_path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| feature_id.clone()),
                None,
            );
            (canonical_id, feature_ref)
        } else {
            let canonical_ref = canonicalize_user_feature_id(feature_id).map_err(|e| {
                DeaconError::Runtime(format!("Invalid feature ID '{}': {}", feature_id, e))
            })?;
            let (registry_url, namespace, name, tag) = parse_registry_reference(&canonical_ref)
                .map_err(|e| {
                    DeaconError::Runtime(format!("Invalid feature ID '{}': {}", feature_id, e))
                })?;

            let feature_ref = FeatureRef::new(registry_url, namespace, name, tag);
            // Tag-bearing, and normalized (an untagged ref becomes `:latest`), so two
            // spellings of the SAME version still collapse to one install while two
            // versions stay two (#430).
            let canonical_id = feature_ref.reference();
            (canonical_id, feature_ref)
        };

        user_id_by_canonical.insert(canonical_id.clone(), feature_id.clone());

        requested.push(RequestedFeature {
            canonical_id,
            feature_ref,
            options: parse_feature_options(feature_options),
        });
    }

    // Download remaining (OCI) features; local features are already staged
    // in `downloaded_features` above.
    debug!(
        "Downloading {} OCI feature(s); {} local feature(s) already resolved",
        requested.len() - downloaded_features.len(),
        downloaded_features.len()
    );
    for entry in &requested {
        if downloaded_features.contains_key(&entry.canonical_id) {
            continue; // local feature — nothing to fetch
        }
        let user_id = user_id_by_canonical
            .get(&entry.canonical_id)
            .map(String::as_str)
            .unwrap_or(entry.canonical_id.as_str());
        let downloaded = fetch_feature_honoring_pins(&fetcher, &entry.feature_ref, user_id, &pins)
            .await
            .map_err(|e| pinned_fetch_error(e, user_id, &pins))?;
        downloaded_features.insert(entry.canonical_id.clone(), downloaded);
    }

    // Auto-install transitive `dependsOn` (HARD) dependencies.
    //
    // Per spec (https://containers.dev/implementors/features/#dependson) a
    // feature's `dependsOn` targets MUST be installed; the reference CLI fetches
    // and installs them even when the user did not declare them. We compute the
    // transitive closure here and add any missing dependency to the feature set
    // — with the options given on the `dependsOn` entry — before resolving the
    // install order. (`installsAfter` is a soft *ordering* hint and is NOT
    // auto-installed; that stays the resolver's job.)
    //
    // The "same resource AND same options → skip" guard makes a user's own declaration of
    // a dependency win when the options agree, and terminates on dependency cycles: the
    // instance set is bounded by the option sets that appear in the metadata graph.
    //
    // Scanning is per RESOURCE, not per instance: `dependsOn` lives in the Feature's
    // metadata, so every instance of one Feature declares the same dependencies.
    let mut to_scan: Vec<String> = requested.iter().map(|r| r.canonical_id.clone()).collect();
    let mut scanned: HashSet<String> = HashSet::new();
    while let Some(scan_id) = to_scan.pop() {
        if !scanned.insert(scan_id.clone()) {
            continue;
        }
        let Some(downloaded) = downloaded_features.get(&scan_id) else {
            continue;
        };
        // Deterministic order despite the metadata map being unordered.
        let mut deps: Vec<(String, serde_json::Value)> = downloaded
            .metadata
            .depends_on
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        deps.sort_by(|a, b| a.0.cmp(&b.0));

        for (dep_key, dep_options_value) in deps {
            let dep_options = parse_feature_options(&dep_options_value);
            // Absolute stays classified local so `resolve_local_feature_dir`
            // rejects it per #495 — see the declared-features loop above.
            let is_local =
                dep_key.starts_with("./") || dep_key.starts_with("../") || dep_key.starts_with('/');

            let (dep_canonical, dep_ref) = if is_local {
                let canonical_path = deacon_core::features::resolve_local_feature_dir(
                    &dep_key,
                    &config_dir,
                    &workspace_root,
                )
                .map_err(|e| {
                    DeaconError::Runtime(format!(
                        "dependsOn local feature '{}' (of '{}'): {}",
                        dep_key, scan_id, e
                    ))
                })?;
                let dep_canonical = format!("local:{}", canonical_path.display());
                if already_installed(&requested, &dep_canonical, &dep_options) {
                    continue;
                }
                if !downloaded_features.contains_key(&dep_canonical) {
                    let metadata_path = canonical_path.join("devcontainer-feature.json");
                    let metadata = deacon_core::features::parse_feature_metadata(&metadata_path)
                        .map_err(|e| {
                            DeaconError::Runtime(format!(
                                "Failed to parse dependsOn local feature metadata at '{}': {}",
                                metadata_path.display(),
                                e
                            ))
                        })?;
                    let digest = dep_canonical.clone();
                    downloaded_features.insert(
                        dep_canonical.clone(),
                        DownloadedFeature {
                            path: canonical_path.clone(),
                            metadata,
                            digest: digest.clone(),
                            manifest_digest: digest,
                        },
                    );
                }
                let dep_ref = FeatureRef::new(
                    "local".to_string(),
                    "fs".to_string(),
                    canonical_path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| dep_key.clone()),
                    None,
                );
                (dep_canonical, dep_ref)
            } else {
                let canonical_dep = canonicalize_user_feature_id(&dep_key).map_err(|e| {
                    DeaconError::Runtime(format!(
                        "Invalid dependsOn feature ref '{}' (of '{}'): {}",
                        dep_key, scan_id, e
                    ))
                })?;
                let (registry_url, namespace, name, tag) = parse_registry_reference(&canonical_dep)
                    .map_err(|e| {
                        DeaconError::Runtime(format!(
                            "Invalid dependsOn feature ref '{}' (of '{}'): {}",
                            dep_key, scan_id, e
                        ))
                    })?;
                let dep_ref = FeatureRef::new(registry_url, namespace, name, tag);
                let dep_canonical = dep_ref.reference();
                if already_installed(&requested, &dep_canonical, &dep_options) {
                    continue;
                }
                info!(
                    feature = %scan_id,
                    dependency = %dep_key,
                    "Auto-installing transitive dependsOn feature"
                );
                if !downloaded_features.contains_key(&dep_canonical) {
                    let downloaded =
                        fetch_feature_honoring_pins(&fetcher, &dep_ref, &dep_key, &pins)
                            .await
                            .map_err(|e| pinned_fetch_error(e, &dep_key, &pins))?;
                    downloaded_features.insert(dep_canonical.clone(), downloaded);
                }
                (dep_canonical, dep_ref)
            };

            user_id_by_canonical
                .entry(dep_canonical.clone())
                .or_insert_with(|| dep_key.clone());
            to_scan.push(dep_canonical.clone());
            requested.push(RequestedFeature {
                canonical_id: dep_canonical,
                feature_ref: dep_ref,
                options: dep_options,
            });
        }
    }

    // Create resolved features
    let mut resolved_features = Vec::new();
    for entry in &requested {
        let canonical_id = &entry.canonical_id;
        let reference = entry.feature_ref.reference();
        let downloaded = downloaded_features.get(canonical_id).ok_or_else(|| {
            DeaconError::Runtime(format!("Downloaded feature not found for {}", reference))
        })?;

        // Options AS AUTHORED — the Feature's declared defaults are applied where they
        // are consumed (`DockerfileGenerator::build_environment_variables`), because this
        // map is half the Feature's identity (#489) and the spec compares authored
        // options.
        let options = entry.options.clone();

        resolved_features.push(ResolvedFeature {
            id: canonical_id.clone(),
            // Record the features-object reference AS WRITTEN (e.g. `./feature-lib`
            // for locals, or the OCI ref string) so dependency resolution can match
            // `dependsOn`/`installsAfter` keys that use the features-object syntax
            // (issue #155). Local features carry a synthetic empty FeatureRef whose
            // `reference()` is NOT the user path, so prefer the user-facing id and
            // fall back to the normalized reference only when it's unavailable.
            source: user_id_by_canonical
                .get(canonical_id)
                .cloned()
                .unwrap_or_else(|| reference.clone()),
            options,
            metadata: downloaded.metadata.clone(),
        });
    }

    // Resolve dependencies.
    //
    // The user expresses `overrideFeatureInstallOrder` with the feature
    // IDs *as written in devcontainer.json* (e.g. `./feature-charlie`,
    // `ghcr.io/foo/bar:1`). Internally we key every feature by its
    // *canonical* ID (`local:<abs path>` for local features, the
    // registry/namespace/name triple for OCI refs). Translate the
    // override list to canonical form before handing it to the
    // resolver — otherwise `validate_override_order` complains that the
    // user-given path "does not exist in feature set" (#69 follow-up).
    let canonical_by_user: HashMap<String, String> = user_id_by_canonical
        .iter()
        .map(|(canon, user)| (user.clone(), canon.clone()))
        .collect();
    let override_order = config.override_feature_install_order.clone().map(|order| {
        order
            .into_iter()
            .map(|user_id| {
                canonical_by_user.get(&user_id).cloned().unwrap_or(user_id) // unknown ids surface in the validate step with the user form
            })
            .collect::<Vec<_>>()
    });
    // Same ingress rule as the `features` map keys, applied to the dependency
    // references the resolved Features declare (#505).
    deacon_core::features::validate_feature_dependency_references(&resolved_features)?;

    let resolver = FeatureDependencyResolver::new(override_order);
    let installation_plan = resolver.resolve(&resolved_features)?;

    debug!(
        "Resolved {} features into {} levels",
        installation_plan.len(),
        installation_plan.levels.len()
    );

    // Collect combined env from feature metadata in plan order so later
    // features win. Per #124 — feature container_env values may legally
    // reference `${devcontainerId}`, `${localWorkspaceFolder}`, etc. and
    // must be substituted before being baked into the BuildKit image.
    let substitution_context = {
        let config_dir = config_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let mut ctx = deacon_core::variable::SubstitutionContext::new(config_dir)?;
        let id_labels: Vec<(String, String)> = identity.id_hash_labels();
        ctx.devcontainer_id = deacon_core::container::compute_dev_container_id(&id_labels);
        ctx
    };
    let mut substitution_report = deacon_core::variable::SubstitutionReport::new();
    let mut combined_env = HashMap::new();
    for level in &installation_plan.levels {
        for &feature_index in level {
            if let Some(feature) = installation_plan.feature_at(feature_index) {
                for (key, value) in &feature.metadata.container_env {
                    let substituted_value =
                        deacon_core::variable::VariableSubstitution::substitute_string(
                            value,
                            &substitution_context,
                            &mut substitution_report,
                        );
                    combined_env.insert(key.clone(), substituted_value);
                }
            }
        }
    }

    // Create temporary directory for features and Dockerfile. The path is derived
    // by the same helper `read-configuration` reports as `dstFolder`, so the two
    // commands can never name different directories.
    let temp_dir =
        crate::commands::shared::feature_resolver::feature_staging_root(&identity.workspace_hash);
    tokio::fs::create_dir_all(&temp_dir).await?;

    // Create features directory structure for BuildKit context
    let features_dir = crate::commands::shared::feature_resolver::feature_staging_dst_folder(
        &identity.workspace_hash,
    );
    tokio::fs::create_dir_all(&features_dir).await?;

    // Copy features to the BuildKit context directory. The destination name must
    // be the one `DockerfileGenerator` writes as the bind-mount `source=`, so it
    // is derived by that generator's own helper rather than re-spelled here.
    let mut install_index = 0usize;
    for level in installation_plan.levels.iter() {
        for &feature_index in level {
            let feature = installation_plan.feature_at(feature_index).ok_or_else(|| {
                DeaconError::Runtime(format!("Feature #{} not found in plan", feature_index))
            })?;

            // Keyed by the feature's id, not its install key: every instance of one
            // Feature shares the same downloaded content and differs only in the options
            // it is executed with (#489).
            let downloaded = downloaded_features.get(&feature.id).ok_or_else(|| {
                DeaconError::Runtime(format!("Downloaded feature {} not found", feature.id))
            })?;

            let feature_dir_name =
                deacon_core::dockerfile_generator::DockerfileGenerator::feature_staging_dir_name(
                    feature,
                    install_index,
                );
            install_index += 1;
            let feature_dest = features_dir.join(&feature_dir_name);
            let src = downloaded.path.clone();
            // copy_dir_all is sync std::fs; offload to the blocking pool so we
            // don't stall the runtime on a recursive file copy.
            tokio::task::spawn_blocking(move || copy_dir_all(&src, &feature_dest))
                .await
                .map_err(|e| DeaconError::Runtime(format!("copy_dir_all join error: {}", e)))??;
        }
    }

    let lockfile =
        build_lockfile_from_features(&requested, &downloaded_features, &user_id_by_canonical);

    Ok(StagedFeatures {
        plan: installation_plan,
        combined_env,
        temp_dir,
        features_source_dir: features_dir,
        lockfile,
    })
}

/// Assemble the canonical lockfile from resolved + downloaded features.
///
/// Mirrors upstream `generateLockfile` in `devcontainers/cli`
/// `src/spec-configuration/lockfile.ts`:
/// - Keys: the user-provided feature ID (as written in `devcontainer.json`).
/// - `resolved`: `{registry}/{repository}@{digest}` via
///   [`LockfileFeature::from_resolved`].
/// - `integrity`: the manifest digest.
/// - `dependsOn`: alphabetically-sorted vec of dependency keys taken
///   verbatim from `metadata.dependsOn`, or `None` when empty.
///
/// Features whose metadata lacks a version field fall back to the tag from
/// the user reference (e.g. `"1"`) and ultimately to `"0.0.0"` so the
/// schema's semver validation never blocks lockfile assembly. A WARN log is
/// emitted so the gap is visible in CI output.
fn build_lockfile_from_features(
    requested: &[RequestedFeature],
    downloaded_features: &HashMap<String, DownloadedFeature>,
    user_id_by_canonical: &HashMap<String, String>,
) -> Lockfile {
    let mut entries: HashMap<String, LockfileFeature> = HashMap::new();

    // Keyed by user-provided id, so the several instances of one Feature (#489) collapse
    // to the single resolved digest they all share.
    for RequestedFeature {
        canonical_id,
        feature_ref,
        ..
    } in requested
    {
        let Some(downloaded) = downloaded_features.get(canonical_id) else {
            // Should never happen — the caller populated downloaded_features
            // from the same feature_refs vec. If it does, skip rather than
            // silently emit a half-valid entry.
            warn!(
                feature = %canonical_id,
                "Skipping lockfile entry: downloaded feature missing from map"
            );
            continue;
        };

        // Local features (`./foo`, `../shared/bar`) have no fetchable
        // OCI identity — their canonical id is `local:<abs path>` and
        // their FeatureRef is a synthetic placeholder. They MUST NOT be
        // recorded in the lockfile: the lockfile's `resolved` schema
        // demands a `registry/path@sha256:...` form, and a local
        // checkout's content can change underneath us anyway. Upstream
        // `@devcontainers/cli` excludes local features from the lockfile
        // for the same reasons (#69 follow-up).
        if canonical_id.starts_with("local:") {
            debug!(
                feature = %canonical_id,
                "Skipping lockfile entry for local feature (no OCI identity to record)"
            );
            continue;
        }

        let user_id = user_id_by_canonical
            .get(canonical_id)
            .cloned()
            .unwrap_or_else(|| canonical_id.clone());

        let version = match &downloaded.metadata.version {
            Some(v) if !v.is_empty() => v.clone(),
            _ => {
                let fallback = feature_ref.tag();
                warn!(
                    feature = %user_id,
                    fallback = %fallback,
                    "Feature metadata has no version field; using tag as fallback for lockfile entry"
                );
                fallback.to_string()
            }
        };

        let depends_on = if downloaded.metadata.depends_on.is_empty() {
            None
        } else {
            let mut deps: Vec<String> = downloaded.metadata.depends_on.keys().cloned().collect();
            deps.sort();
            Some(deps)
        };

        let entry = LockfileFeature::from_resolved(
            &feature_ref.registry,
            &feature_ref.repository(),
            &downloaded.manifest_digest,
            version,
            depends_on,
        );

        entries.insert(user_id, entry);
    }

    Lockfile { features: entries }
}

pub(crate) async fn ensure_buildkit_or_error() -> Result<()> {
    use deacon_core::build::buildkit::is_buildkit_available;
    if !is_buildkit_available().await? {
        return Err(DeaconError::Runtime(
            "BuildKit is required for feature installation. Please enable BuildKit.".to_string(),
        )
        .into());
    }
    Ok(())
}

fn log_cache_configuration(build_options: Option<&BuildOptions>) {
    if let Some(opts) = build_options {
        if !opts.cache_from.is_empty() {
            info!(
                cache_from = ?opts.cache_from,
                "Using cache source(s) for feature build"
            );
        }
        if let Some(cache_to) = &opts.cache_to {
            info!(
                cache_to = %cache_to,
                "Exporting feature build cache to destination"
            );
        }
    }
}

/// Recursively copy a directory
pub(crate) fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
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

#[cfg(test)]
mod lockfile_assembly_tests {
    use super::*;
    use deacon_core::features::FeatureMetadata;

    #[test]
    fn parse_feature_options_handles_object_and_non_object() {
        // Object → typed options (the `dependsOn` value side and the `features`
        // value side share this shape).
        let opts = parse_feature_options(&serde_json::json!({
            "version": "22",
            "moby": true,
            "count": 3
        }));
        assert_eq!(
            opts.get("version"),
            Some(&OptionValue::String("22".to_string()))
        );
        assert_eq!(opts.get("moby"), Some(&OptionValue::Boolean(true)));
        assert!(matches!(opts.get("count"), Some(OptionValue::Number(_))));

        // Non-object (e.g. `dependsOn: { "ref": true }`) → no options.
        assert!(parse_feature_options(&serde_json::Value::Bool(true)).is_empty());
        assert!(parse_feature_options(&serde_json::json!("str")).is_empty());
    }

    fn make_downloaded(version: Option<&str>, digest: &str) -> DownloadedFeature {
        DownloadedFeature {
            path: PathBuf::from("/tmp/unused"),
            metadata: FeatureMetadata {
                id: "node".to_string(),
                version: version.map(|s| s.to_string()),
                ..FeatureMetadata::default()
            },
            digest: digest.to_string(),
            manifest_digest: digest.to_string(),
        }
    }

    fn make_downloaded_with_deps(version: &str, digest: &str, deps: &[&str]) -> DownloadedFeature {
        let mut depends_on = HashMap::new();
        for d in deps {
            depends_on.insert(d.to_string(), serde_json::Value::Bool(true));
        }
        DownloadedFeature {
            path: PathBuf::from("/tmp/unused"),
            metadata: FeatureMetadata {
                id: "node".to_string(),
                version: Some(version.to_string()),
                depends_on,
                ..FeatureMetadata::default()
            },
            digest: digest.to_string(),
            manifest_digest: digest.to_string(),
        }
    }

    #[test]
    fn build_lockfile_keys_by_user_provided_id() {
        // Mirrors upstream `generateLockfile`: the lockfile key is the
        // user-provided feature ID, not the canonical (no-tag) form.
        let feature_ref = FeatureRef::new(
            "ghcr.io".to_string(),
            "devcontainers".to_string(),
            "node".to_string(),
            Some("1".to_string()),
        );
        let canonical = "ghcr.io/devcontainers/node".to_string();
        let user_id = "ghcr.io/devcontainers/node:1".to_string();

        let mut downloaded_features = HashMap::new();
        downloaded_features.insert(
            canonical.clone(),
            make_downloaded(
                Some("1.6.1"),
                "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            ),
        );

        let mut user_id_by_canonical = HashMap::new();
        user_id_by_canonical.insert(canonical.clone(), user_id.clone());

        let lockfile = build_lockfile_from_features(
            &[RequestedFeature {
                canonical_id: canonical,
                feature_ref,
                options: HashMap::new(),
            }],
            &downloaded_features,
            &user_id_by_canonical,
        );

        assert_eq!(lockfile.features.len(), 1);
        let entry = lockfile
            .features
            .get(&user_id)
            .expect("lockfile must be keyed by the user-provided feature ID");
        assert_eq!(entry.version, "1.6.1");
        assert_eq!(
            entry.resolved,
            "ghcr.io/devcontainers/node@sha256:1111111111111111111111111111111111111111111111111111111111111111"
        );
        assert_eq!(
            entry.integrity,
            "sha256:1111111111111111111111111111111111111111111111111111111111111111"
        );
        assert!(entry.depends_on.is_none());
    }

    /// #264 guard: the lockfile writer must record the OCI *manifest* digest,
    /// not the layer/blob digest used for on-disk caching — even when they
    /// differ, as they always do in practice.
    #[test]
    fn build_lockfile_uses_manifest_digest_not_layer_digest() {
        let feature_ref = FeatureRef::new(
            "ghcr.io".to_string(),
            "devcontainers".to_string(),
            "python".to_string(),
            Some("1".to_string()),
        );
        let canonical = "ghcr.io/devcontainers/python".to_string();
        let user_id = "ghcr.io/devcontainers/python:1".to_string();

        let layer_digest =
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
        let manifest_digest =
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
        assert_ne!(layer_digest, manifest_digest);

        let downloaded = DownloadedFeature {
            path: PathBuf::from("/tmp/unused"),
            metadata: FeatureMetadata {
                id: "python".to_string(),
                version: Some("1.2.3".to_string()),
                ..FeatureMetadata::default()
            },
            digest: layer_digest,
            manifest_digest: manifest_digest.clone(),
        };

        let mut downloaded_features = HashMap::new();
        downloaded_features.insert(canonical.clone(), downloaded);
        let mut user_id_by_canonical = HashMap::new();
        user_id_by_canonical.insert(canonical.clone(), user_id.clone());

        let lockfile = build_lockfile_from_features(
            &[RequestedFeature {
                canonical_id: canonical,
                feature_ref,
                options: HashMap::new(),
            }],
            &downloaded_features,
            &user_id_by_canonical,
        );

        let entry = lockfile
            .features
            .get(&user_id)
            .expect("lockfile must contain the feature entry");
        assert_eq!(
            entry.resolved,
            format!("ghcr.io/devcontainers/python@{}", manifest_digest)
        );
        assert_eq!(entry.integrity, manifest_digest);
    }

    #[test]
    fn build_lockfile_falls_back_to_tag_when_version_missing() {
        // Some features ship without a `version` in their metadata; rather
        // than block lockfile generation we fall back to the tag the user
        // requested (e.g. "1"). This is best-effort — the WARN is the
        // observable signal that something is off.
        let feature_ref = FeatureRef::new(
            "ghcr.io".to_string(),
            "x".to_string(),
            "y".to_string(),
            Some("3".to_string()),
        );
        let canonical = "ghcr.io/x/y".to_string();
        let user_id = "ghcr.io/x/y:3".to_string();

        let mut downloaded_features = HashMap::new();
        downloaded_features.insert(
            canonical.clone(),
            make_downloaded(
                None,
                "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            ),
        );

        let mut user_id_by_canonical = HashMap::new();
        user_id_by_canonical.insert(canonical.clone(), user_id.clone());

        let lockfile = build_lockfile_from_features(
            &[RequestedFeature {
                canonical_id: canonical,
                feature_ref,
                options: HashMap::new(),
            }],
            &downloaded_features,
            &user_id_by_canonical,
        );

        // Tag was "3", so that's the version used for the lockfile entry.
        // Note: "3" is not valid semver, so a subsequent `write_lockfile`
        // call would fail validation — but the assembly itself is best-effort.
        let entry = lockfile.features.get(&user_id).unwrap();
        assert_eq!(entry.version, "3");
    }

    #[test]
    fn build_lockfile_sorts_depends_on_alphabetically() {
        // Upstream `generateLockfile` sorts `dependsOn` so byte-identical
        // output is stable across runs and across implementations.
        let feature_ref = FeatureRef::new(
            "ghcr.io".to_string(),
            "x".to_string(),
            "y".to_string(),
            Some("1".to_string()),
        );
        let canonical = "ghcr.io/x/y".to_string();
        let user_id = "ghcr.io/x/y:1".to_string();

        let mut downloaded_features = HashMap::new();
        downloaded_features.insert(
            canonical.clone(),
            make_downloaded_with_deps(
                "1.0.0",
                "sha256:3333333333333333333333333333333333333333333333333333333333333333",
                &["zeta", "alpha", "mu"],
            ),
        );

        let mut user_id_by_canonical = HashMap::new();
        user_id_by_canonical.insert(canonical.clone(), user_id.clone());

        let lockfile = build_lockfile_from_features(
            &[RequestedFeature {
                canonical_id: canonical,
                feature_ref,
                options: HashMap::new(),
            }],
            &downloaded_features,
            &user_id_by_canonical,
        );

        let entry = lockfile.features.get(&user_id).unwrap();
        let deps = entry.depends_on.as_ref().unwrap();
        assert_eq!(deps, &["alpha", "mu", "zeta"]);
    }

    #[test]
    fn build_lockfile_omits_empty_depends_on() {
        let feature_ref = FeatureRef::new(
            "ghcr.io".to_string(),
            "x".to_string(),
            "y".to_string(),
            Some("1".to_string()),
        );
        let canonical = "ghcr.io/x/y".to_string();
        let user_id = "ghcr.io/x/y:1".to_string();

        let mut downloaded_features = HashMap::new();
        downloaded_features.insert(
            canonical.clone(),
            make_downloaded(
                Some("1.0.0"),
                "sha256:4444444444444444444444444444444444444444444444444444444444444444",
            ),
        );

        let mut user_id_by_canonical = HashMap::new();
        user_id_by_canonical.insert(canonical.clone(), user_id.clone());

        let lockfile = build_lockfile_from_features(
            &[RequestedFeature {
                canonical_id: canonical,
                feature_ref,
                options: HashMap::new(),
            }],
            &downloaded_features,
            &user_id_by_canonical,
        );

        let entry = lockfile.features.get(&user_id).unwrap();
        assert!(entry.depends_on.is_none());
    }
}

#[cfg(test)]
mod local_feature_resolution_tests {
    //! Spec parity (#69): `./feature-X` and `../shared/feature` references in
    //! a `devcontainer.json` resolve relative to the config file's directory,
    //! not the workspace folder and not the CWD. These tests pin that
    //! contract for both `up` and any future path that calls
    //! `resolve_and_stage_features` with a config containing local features.
    //!
    //! Docker is not required for these tests — they exercise the parse
    //! path that the issue's reproduction blew up on (`registry: "."`).

    use super::*;
    use deacon_core::container::ContainerIdentity;
    use tempfile::TempDir;

    /// Build a temp tree like the upstream reproduction:
    ///   <root>/
    ///     .devcontainer/
    ///       example/
    ///         devcontainer.json     ← references "./feature-alpha"
    ///         feature-alpha/
    ///           devcontainer-feature.json
    ///           install.sh
    ///
    /// The config sits one level BELOW `.devcontainer/` deliberately: it keeps
    /// the config-relative claim sharp (a workspace-relative resolver would look
    /// in `<root>/feature-alpha` and find nothing) while satisfying the
    /// `.devcontainer/` containment rule #488 added.
    fn build_local_feature_workspace() -> TempDir {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let example = root.join(".devcontainer").join("example");
        std::fs::create_dir_all(&example).unwrap();

        std::fs::write(
            example.join("devcontainer.json"),
            r#"{
  "image": "alpine:3.18",
  "features": { "./feature-alpha": {} }
}
"#,
        )
        .unwrap();

        let feature_dir = example.join("feature-alpha");
        std::fs::create_dir_all(&feature_dir).unwrap();
        std::fs::write(
            feature_dir.join("devcontainer-feature.json"),
            r#"{
  "id": "feature-alpha",
  "version": "1.0.0",
  "name": "Alpha"
}
"#,
        )
        .unwrap();
        std::fs::write(feature_dir.join("install.sh"), "#!/bin/sh\nexit 0\n").unwrap();

        temp
    }

    #[tokio::test]
    async fn local_feature_resolves_relative_to_config_dir() {
        let temp = build_local_feature_workspace();
        let config_path = temp
            .path()
            .join(".devcontainer")
            .join("example")
            .join("devcontainer.json");
        let raw = std::fs::read_to_string(&config_path).unwrap();
        let config: DevContainerConfig = serde_json::from_str(&raw).unwrap();

        let identity = ContainerIdentity::new(temp.path(), &config);

        let staged =
            resolve_and_stage_features(&config, &identity, &config_path, LockfilePolicy::Write)
                .await
                .expect("local feature should resolve successfully");

        // The installation plan should contain exactly one feature, whose
        // canonical id encodes the resolved absolute path.
        assert_eq!(staged.plan.features.len(), 1);
        let resolved = &staged.plan.features[0];
        assert!(
            resolved.id.starts_with("local:"),
            "local feature canonical id should be 'local:<abs>', got {}",
            resolved.id
        );
        assert!(
            resolved.id.contains("feature-alpha"),
            "canonical id should embed the local feature name, got {}",
            resolved.id
        );

        // The staged tree must contain the feature's contents (install.sh).
        // Walk just one level deep — each feature gets its own subdirectory
        // under `features_source_dir`.
        let mut staged_install_seen = false;
        for sub in std::fs::read_dir(&staged.features_source_dir).unwrap() {
            let sub = sub.unwrap();
            if sub.path().join("install.sh").exists() {
                staged_install_seen = true;
                break;
            }
        }
        assert!(
            staged_install_seen,
            "local feature contents (install.sh) should be copied into the BuildKit context"
        );
    }

    #[tokio::test]
    async fn missing_local_feature_path_surfaces_clear_error() {
        // Spec parity (#69): a bad local path must produce a clear error
        // naming both the user-provided reference and the resolution base,
        // rather than the cryptic `registry: "."` OCI failure.
        let temp = TempDir::new().unwrap();
        let example = temp.path().join(".devcontainer").join("example");
        std::fs::create_dir_all(&example).unwrap();
        let config_path = example.join("devcontainer.json");
        std::fs::write(
            &config_path,
            r#"{
  "image": "alpine:3.18",
  "features": { "./missing-feature": {} }
}
"#,
        )
        .unwrap();
        let raw = std::fs::read_to_string(&config_path).unwrap();
        let config: DevContainerConfig = serde_json::from_str(&raw).unwrap();
        let identity = ContainerIdentity::new(temp.path(), &config);

        let err =
            resolve_and_stage_features(&config, &identity, &config_path, LockfilePolicy::Write)
                .await
                .err()
                .expect("missing local feature path must error");
        let msg = err.to_string();
        assert!(
            msg.contains("./missing-feature"),
            "error must include the user-provided reference, got: {msg}"
        );
        assert!(
            msg.contains("not accessible"),
            "error must explain the failure mode, got: {msg}"
        );
        assert!(
            !msg.contains("registry"),
            "error must NOT misclassify the local path as an OCI ref, got: {msg}"
        );
    }
}
