//! Feature image building with BuildKit.
//!
//! This module contains:
//! - `FeatureBuildOutput` - Output from feature image building
//! - `build_image_with_features` - Build extended image from a base `image:` reference
//! - `build_image_with_features_from_dockerfile` - Build extended image when the
//!   base is a user-authored Dockerfile + context directory (compose `build:` shape)
//! - `copy_dir_all` - Recursive directory copy helper

use anyhow::{Context, Result};
use deacon_core::build::BuildOptions;
use deacon_core::config::DevContainerConfig;
use deacon_core::container::ContainerIdentity;
use deacon_core::dockerfile_generator::{
    DockerfileConfig, DockerfileGenerator, FeatureInstallEnv, HOST_CA_BUILD_CONTEXT,
    HOST_CA_MOUNT_TARGET,
};
use deacon_core::errors::DeaconError;
use deacon_core::features::{
    FeatureDependencyResolver, InstallationPlan, OptionValue, ResolvedFeature,
};
use deacon_core::host_ca::{CorporateCaSet, build_install_script};
use deacon_core::lockfile::{Lockfile, LockfileFeature};
use deacon_core::oci::{DownloadedFeature, FeatureRef, default_fetcher};
use deacon_core::registry_parser::parse_registry_reference;
use std::collections::HashMap;
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
#[instrument(skip(config, identity, build_options, cli))]
pub(crate) async fn build_image_with_features(
    config: &DevContainerConfig,
    identity: &ContainerIdentity,
    _workspace_folder: &Path,
    config_path: &Path,
    build_options: Option<&BuildOptions>,
    host_ca_set: Option<&CorporateCaSet>,
    cli: &deacon_core::docker::CliRuntime,
) -> Result<FeatureBuildOutput> {
    info!("Building extended image with features");

    // Get base image
    let base_image = config
        .image
        .as_ref()
        .ok_or_else(|| DeaconError::Runtime("No base image specified".to_string()))?;

    // Parse features from config
    let features_obj = config
        .features
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
        });
    }

    let staged = resolve_and_stage_features(config, identity, config_path).await?;

    // Generate Dockerfile.
    //
    // Spec parity (#89): surface `_REMOTE_USER`, `_REMOTE_USER_HOME`,
    // `_CONTAINER_USER`, `_CONTAINER_USER_HOME` to every feature's
    // `install.sh`. These are resolved from the base image's
    // `devcontainer.metadata` LABEL + baked-in `USER` folded under the user
    // config — NOT from the user config alone, since `remoteUser` is commonly
    // declared by the base image. Empty values are still emitted so
    // `${_REMOTE_USER:-}` resolves to "" rather than `<unset>`.
    let feature_install_env =
        crate::commands::up::merged_config::resolve_feature_install_env(cli, base_image, config)
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

    // Build image with BuildKit
    let build_args =
        generator.generate_build_args(&dockerfile_path, &extended_image_tag, build_options);

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
    })
}

/// Build an extended Docker image with features installed when the base
/// description is a user-authored Dockerfile under `base_context_dir`.
///
/// Used by the compose `build:` shape: bead 14b's Dockerfile-stage-name parser
/// rewrites the user's final `FROM` to carry a deterministic alias, then we
/// concatenate our feature-install stage that targets that alias. The merged
/// Dockerfile is written to a temp directory and built with the user's
/// original context directory so the existing `COPY`/`ADD` directives keep
/// resolving the right files.
///
/// # Arguments
///
/// * `config` - DevContainer configuration containing features to install
/// * `identity` - Container identity for deterministic naming
/// * `base_dockerfile_content` - The user's Dockerfile contents, already
///   processed by `ensure_dockerfile_has_final_stage_name` so the final stage
///   has the alias `base_dockerfile_final_stage`
/// * `base_dockerfile_final_stage` - The name of that final stage; our
///   feature-install stage will `FROM <stage>`
/// * `base_context_dir` - The compose `build.context` directory, resolved to
///   an absolute path. This is passed as the BuildKit context so the user's
///   relative `COPY`/`ADD` paths keep working
/// * `target` - Optional `build.target` from compose; ignored today because
///   our feature stage always builds on top of the user's *final* stage
///   (which `ensure_dockerfile_has_final_stage_name` selected). Recorded in
///   the tracing span for diagnostics.
/// * `build_options` - Optional build options for cache-from/cache-to/buildx settings
#[allow(clippy::too_many_arguments)]
#[instrument(
    skip(config, identity, base_dockerfile_content, build_options),
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
) -> Result<FeatureBuildOutput> {
    // Optional `build.target` is honored as the upstream stage we extend. The
    // reference CLI rewrites the FROM matching `target`; we accomplish the
    // same outcome by trusting the caller to pre-process via
    // `ensure_dockerfile_has_final_stage_name`, which already picks the final
    // stage. We log `target` so any compose configs that rely on intermediate
    // stages can be diagnosed without silently picking the wrong layer.
    if let Some(t) = target {
        if t != base_dockerfile_final_stage {
            debug!(
                requested_target = %t,
                used_stage = %base_dockerfile_final_stage,
                "compose build.target differs from Dockerfile final stage; \
                 features will be installed on top of the final stage"
            );
        }
    }

    info!(
        "Building extended image with features on top of user-authored Dockerfile (stage={})",
        base_dockerfile_final_stage
    );

    let features_obj = config
        .features
        .as_object()
        .ok_or_else(|| DeaconError::Runtime("Features must be an object".to_string()))?;
    if features_obj.is_empty() {
        return Err(DeaconError::Runtime(
            "build_image_with_features_from_dockerfile called with no features".to_string(),
        )
        .into());
    }

    let staged = resolve_and_stage_features(config, identity, config_path).await?;

    // Generate the feature-install stage targeting the user's final stage by
    // literal name (NOT via an ARG-driven FROM): a Dockerfile that prepends
    // user-authored stages cannot use global-ARG substitution for the FROM of
    // the appended stage — BuildKit only honors global ARGs declared before
    // any FROM, and once we splice content after the user's stages that
    // window is closed. The literal `FROM <stage>` form sidesteps that and
    // resolves directly to the previous stage in the same Dockerfile.
    let target_stage_name = "dev_containers_target_stage";
    // Unlike the `image:` path, the "base" here is a *stage name* inside the
    // user's Dockerfile, not a registry ref — there is nothing to inspect for a
    // `devcontainer.metadata` LABEL or baked-in `USER` until that stage has been
    // built. So `_REMOTE_USER` / `_CONTAINER_USER` come from the user config
    // alone; set `remoteUser` / `containerUser` explicitly if a feature needs
    // them on this path (#89).
    let feature_install_env = FeatureInstallEnv::resolve(
        config.remote_user.as_deref(),
        config.container_user.as_deref(),
        None,
    );
    // Build-time host-CA injection (016, T038): stage the bundle + script when a
    // non-empty corporate set was supplied (compose `build:` shape).
    let host_ca_dir = match host_ca_set {
        Some(set) if !set.is_empty() => Some(stage_host_ca_context(&staged.temp_dir, set).await?),
        _ => None,
    };
    let dockerfile_config = DockerfileConfig {
        base_image: base_dockerfile_final_stage.to_string(),
        target_stage: target_stage_name.to_string(),
        features_source_dir: staged.features_source_dir.display().to_string(),
        feature_install_env,
        host_ca_build_context: host_ca_dir.as_ref().map(|p| p.display().to_string()),
    };
    let generator = DockerfileGenerator::new(dockerfile_config.clone());
    let feature_stage =
        generator.generate_install_stage_from(&staged.plan, base_dockerfile_final_stage)?;

    // Compose final Dockerfile: user prologue + feature install stage.
    // The user's Dockerfile may carry a `# syntax=` directive at the very top;
    // that's already preserved because we copy the full content first.
    let mut combined =
        String::with_capacity(base_dockerfile_content.len() + feature_stage.len() + 2);
    combined.push_str(base_dockerfile_content);
    if !base_dockerfile_content.ends_with('\n') {
        combined.push('\n');
    }
    combined.push('\n');
    combined.push_str(&feature_stage);

    // Write merged Dockerfile to the temp dir (NOT into the user's context
    // dir, so we never pollute the workspace). buildx will read it via `-f`
    // regardless of the context directory's location.
    let dockerfile_path = staged.temp_dir.join("Dockerfile.extended");
    tokio::fs::write(&dockerfile_path, combined.as_bytes()).await?;
    debug!(
        "Wrote merged Dockerfile ({} bytes) at {}",
        combined.len(),
        dockerfile_path.display()
    );

    let extended_image_tag = format!("deacon-devcontainer-features:{}", identity.workspace_hash);

    ensure_buildkit_or_error().await?;
    log_cache_configuration(build_options);

    // Build args: hand-rolled here (NOT the generator's defaults) because the
    // generator passes `--build-arg _DEV_CONTAINERS_BASE_IMAGE=...` which is
    // unused (and emits a BuildKit warning) when the FROM is literal. We
    // still pass `--target` so BuildKit stops at our feature stage even if
    // the user has further stages after it, plus `--build-context` so the
    // RUN-mount lines resolve to the staged features directory.
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

    // Build-time host-CA build context (016): mounted by the generated RUN step.
    if let Some(ref ca_dir) = host_ca_dir {
        build_args.push("--build-context".to_string());
        build_args.push(format!("{}={}", HOST_CA_BUILD_CONTEXT, ca_dir.display()));
    }

    build_args.extend(vec![
        "--build-context".to_string(),
        format!(
            "dev_containers_feature_content_source={}",
            staged.features_source_dir.display()
        ),
        "--target".to_string(),
        target_stage_name.to_string(),
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
        staged.plan.features.iter().map(|f| f.id.as_str()),
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
        combined_env: staged.combined_env,
        resolved_features: staged.plan.features.clone(),
        lockfile: staged.lockfile,
    })
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
) -> Result<StagedFeatures> {
    let features_obj = config
        .features
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

    // Create feature fetcher (used for OCI refs only)
    let fetcher = default_fetcher()?;

    // Parse, classify, and (for OCI refs) fetch features.
    //
    // Local references are resolved relative to `config_dir` and short-
    // circuit the OCI fetcher entirely. We synthesize a `DownloadedFeature`
    // pointing at the on-disk directory so the downstream staging pipeline
    // (copy into BuildKit context + dependency resolution + Dockerfile
    // generation) treats them identically to fetched features.
    let mut feature_refs: Vec<(String, FeatureRef)> = Vec::new();
    let mut feature_options_map: HashMap<String, HashMap<String, OptionValue>> = HashMap::new();
    // Canonical id (registry/namespace/name, no tag) → user-provided feature ID
    // (the key as it appears in `devcontainer.json`). The lockfile MUST be
    // keyed by the user-provided form to match upstream `generateLockfile`.
    let mut user_id_by_canonical: HashMap<String, String> = HashMap::new();
    let mut downloaded_features: HashMap<String, DownloadedFeature> = HashMap::new();

    for (feature_id, feature_options) in features_obj.iter() {
        // Per #126: absolute paths are also valid local-feature locations
        // (parity with read_configuration's local dispatch added in #109).
        let is_local = feature_id.starts_with("./")
            || feature_id.starts_with("../")
            || feature_id.starts_with('/');

        let (canonical_id, feature_ref) = if is_local {
            // Resolve `./foo` and `../shared/foo` against the config file's
            // directory (spec contract — *not* the workspace folder, *not*
            // the CWD, regardless of how the config was loaded).
            let resolved = config_dir.join(feature_id);
            let canonical_path = resolved.canonicalize().map_err(|e| {
                DeaconError::Runtime(format!(
                    "Local feature path '{}' (resolved to '{}' relative to {}) is not accessible: {}",
                    feature_id,
                    resolved.display(),
                    config_dir.display(),
                    e
                ))
            })?;

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
            let (registry_url, namespace, name, tag) = parse_registry_reference(feature_id)
                .map_err(|e| {
                    DeaconError::Runtime(format!("Invalid feature ID '{}': {}", feature_id, e))
                })?;

            let feature_ref = FeatureRef::new(registry_url, namespace, name, tag);
            let canonical_id = format!(
                "{}/{}/{}",
                feature_ref.registry, feature_ref.namespace, feature_ref.name
            );
            (canonical_id, feature_ref)
        };

        user_id_by_canonical.insert(canonical_id.clone(), feature_id.clone());

        let options = parse_feature_options(feature_options);

        feature_options_map.insert(canonical_id.clone(), options);
        feature_refs.push((canonical_id, feature_ref));
    }

    // Download remaining (OCI) features; local features are already staged
    // in `downloaded_features` above.
    debug!(
        "Downloading {} OCI feature(s); {} local feature(s) already resolved",
        feature_refs.len() - downloaded_features.len(),
        downloaded_features.len()
    );
    for (canonical_id, feature_ref) in &feature_refs {
        if downloaded_features.contains_key(canonical_id) {
            continue; // local feature — nothing to fetch
        }
        let downloaded = fetcher.fetch_feature(feature_ref).await?;
        downloaded_features.insert(canonical_id.clone(), downloaded);
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
    // The "already downloaded → skip" guard makes a user's own declaration of a
    // dependency win (its options are kept) and terminates on dependency cycles.
    let mut to_scan: Vec<String> = feature_refs.iter().map(|(c, _)| c.clone()).collect();
    while let Some(scan_id) = to_scan.pop() {
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
            let is_local =
                dep_key.starts_with("./") || dep_key.starts_with("../") || dep_key.starts_with('/');

            let (dep_canonical, dep_ref) = if is_local {
                let resolved = config_dir.join(&dep_key);
                let canonical_path = resolved.canonicalize().map_err(|e| {
                    DeaconError::Runtime(format!(
                        "dependsOn local feature '{}' (of '{}', resolved to '{}') is not accessible: {}",
                        dep_key, scan_id, resolved.display(), e
                    ))
                })?;
                let dep_canonical = format!("local:{}", canonical_path.display());
                if downloaded_features.contains_key(&dep_canonical) {
                    continue;
                }
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
                let (registry_url, namespace, name, tag) = parse_registry_reference(&dep_key)
                    .map_err(|e| {
                        DeaconError::Runtime(format!(
                            "Invalid dependsOn feature ref '{}' (of '{}'): {}",
                            dep_key, scan_id, e
                        ))
                    })?;
                let dep_ref = FeatureRef::new(registry_url, namespace, name, tag);
                let dep_canonical = format!(
                    "{}/{}/{}",
                    dep_ref.registry, dep_ref.namespace, dep_ref.name
                );
                if downloaded_features.contains_key(&dep_canonical) {
                    continue;
                }
                info!(
                    feature = %scan_id,
                    dependency = %dep_key,
                    "Auto-installing transitive dependsOn feature"
                );
                let downloaded = fetcher.fetch_feature(&dep_ref).await?;
                downloaded_features.insert(dep_canonical.clone(), downloaded);
                (dep_canonical, dep_ref)
            };

            feature_options_map.insert(
                dep_canonical.clone(),
                parse_feature_options(&dep_options_value),
            );
            user_id_by_canonical
                .entry(dep_canonical.clone())
                .or_insert_with(|| dep_key.clone());
            feature_refs.push((dep_canonical.clone(), dep_ref));
            to_scan.push(dep_canonical);
        }
    }

    // Create resolved features
    let mut resolved_features = Vec::new();
    for (canonical_id, feature_ref) in &feature_refs {
        let reference = feature_ref.reference();
        let downloaded = downloaded_features.get(canonical_id).ok_or_else(|| {
            DeaconError::Runtime(format!("Downloaded feature not found for {}", reference))
        })?;

        let mut options = feature_options_map
            .get(canonical_id)
            .cloned()
            .unwrap_or_default();

        for (opt_name, opt_def) in &downloaded.metadata.options {
            if options.contains_key(opt_name) {
                continue;
            }

            if let Some(default_val) = opt_def.default_value() {
                options.insert(opt_name.clone(), default_val);
            }
        }

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
        let id_labels: Vec<(String, String)> = identity.labels().into_iter().collect();
        ctx.devcontainer_id = deacon_core::container::compute_dev_container_id(&id_labels);
        ctx
    };
    let mut substitution_report = deacon_core::variable::SubstitutionReport::new();
    let mut combined_env = HashMap::new();
    for level in &installation_plan.levels {
        for feature_id in level {
            if let Some(feature) = installation_plan.get_feature(feature_id) {
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

    // Create temporary directory for features and Dockerfile
    let temp_dir =
        std::env::temp_dir().join(format!("deacon-features-{}", identity.workspace_hash));
    tokio::fs::create_dir_all(&temp_dir).await?;

    // Create features directory structure for BuildKit context
    let features_dir = temp_dir.join("features");
    tokio::fs::create_dir_all(&features_dir).await?;

    // Copy features to the BuildKit context directory
    for (level_idx, level) in installation_plan.levels.iter().enumerate() {
        for feature_id in level {
            let feature = installation_plan.get_feature(feature_id).ok_or_else(|| {
                DeaconError::Runtime(format!("Feature {} not found in plan", feature_id))
            })?;

            let downloaded = downloaded_features.get(feature_id).ok_or_else(|| {
                DeaconError::Runtime(format!("Downloaded feature {} not found", feature_id))
            })?;

            let sanitized_id = feature
                .id
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect::<String>();

            let feature_dir_name = format!("{}_{}", sanitized_id, level_idx);
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
        build_lockfile_from_features(&feature_refs, &downloaded_features, &user_id_by_canonical);

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
    feature_refs: &[(String, FeatureRef)],
    downloaded_features: &HashMap<String, DownloadedFeature>,
    user_id_by_canonical: &HashMap<String, String>,
) -> Lockfile {
    let mut entries: HashMap<String, LockfileFeature> = HashMap::new();

    for (canonical_id, feature_ref) in feature_refs {
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

async fn ensure_buildkit_or_error() -> Result<()> {
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
            &[(canonical, feature_ref)],
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
            &[(canonical, feature_ref)],
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
            &[(canonical, feature_ref)],
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
            &[(canonical, feature_ref)],
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
            &[(canonical, feature_ref)],
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
    ///     example/
    ///       devcontainer.json     ← references "./feature-alpha"
    ///       feature-alpha/
    ///         devcontainer-feature.json
    ///         install.sh
    fn build_local_feature_workspace() -> TempDir {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let example = root.join("example");
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
        let config_path = temp.path().join("example").join("devcontainer.json");
        let raw = std::fs::read_to_string(&config_path).unwrap();
        let config: DevContainerConfig = serde_json::from_str(&raw).unwrap();

        let identity = ContainerIdentity::new(temp.path(), &config);

        let staged = resolve_and_stage_features(&config, &identity, &config_path)
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
        let example = temp.path().join("example");
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

        let err = resolve_and_stage_features(&config, &identity, &config_path)
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
