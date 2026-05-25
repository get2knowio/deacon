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
use deacon_core::dockerfile_generator::{DockerfileConfig, DockerfileGenerator};
use deacon_core::errors::DeaconError;
use deacon_core::features::{
    FeatureDependencyResolver, InstallationPlan, OptionValue, ResolvedFeature,
};
use deacon_core::oci::{default_fetcher, DownloadedFeature, FeatureRef};
use deacon_core::registry_parser::parse_registry_reference;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::{debug, info, instrument};

/// Output from building an image with features
#[derive(Debug, Clone)]
pub(crate) struct FeatureBuildOutput {
    /// Extended image tag with features installed
    pub image_tag: String,
    /// Combined environment variables from all features
    pub combined_env: HashMap<String, String>,
    /// Resolved features in installation order
    pub resolved_features: Vec<deacon_core::features::ResolvedFeature>,
}

/// Internal: result of resolving + downloading + staging features for a build.
struct StagedFeatures {
    plan: InstallationPlan,
    combined_env: HashMap<String, String>,
    temp_dir: PathBuf,
    features_source_dir: PathBuf,
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
#[instrument(skip(config, identity, build_options))]
pub(crate) async fn build_image_with_features(
    config: &DevContainerConfig,
    identity: &ContainerIdentity,
    _workspace_folder: &Path,
    build_options: Option<&BuildOptions>,
) -> Result<FeatureBuildOutput> {
    use deacon_core::docker::CliDocker;

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
        });
    }

    let staged = resolve_and_stage_features(config, identity).await?;

    // Generate Dockerfile
    let dockerfile_config = DockerfileConfig {
        base_image: base_image.clone(),
        target_stage: "dev_containers_target_stage".to_string(),
        features_source_dir: staged.features_source_dir.display().to_string(),
    };

    let generator = DockerfileGenerator::new(dockerfile_config.clone());
    let dockerfile_content = generator.generate(&staged.plan)?;

    // Write Dockerfile
    let dockerfile_path = staged.temp_dir.join("Dockerfile.extended");
    let mut dockerfile_file = std::fs::File::create(&dockerfile_path)?;
    dockerfile_file.write_all(dockerfile_content.as_bytes())?;

    debug!("Generated Dockerfile at {}", dockerfile_path.display());

    // Generate image tag
    let extended_image_tag = format!("deacon-devcontainer-features:{}", identity.workspace_hash);

    ensure_buildkit_or_error()?;
    log_cache_configuration(build_options);

    // Build image with BuildKit
    let build_args =
        generator.generate_build_args(&dockerfile_path, &extended_image_tag, build_options);

    let cli_docker = CliDocker::new();
    debug!("Building image with args: {:?}", build_args);
    let _image_id = cli_docker.build_image(&build_args).await?;

    info!("Successfully built extended image: {}", extended_image_tag);

    Ok(FeatureBuildOutput {
        image_tag: extended_image_tag,
        combined_env: staged.combined_env,
        resolved_features: staged.plan.features.clone(),
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
    target: Option<&str>,
    build_options: Option<&BuildOptions>,
) -> Result<FeatureBuildOutput> {
    use deacon_core::docker::CliDocker;

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

    let staged = resolve_and_stage_features(config, identity).await?;

    // Generate the feature-install stage targeting the user's final stage by
    // literal name (NOT via an ARG-driven FROM): a Dockerfile that prepends
    // user-authored stages cannot use global-ARG substitution for the FROM of
    // the appended stage — BuildKit only honors global ARGs declared before
    // any FROM, and once we splice content after the user's stages that
    // window is closed. The literal `FROM <stage>` form sidesteps that and
    // resolves directly to the previous stage in the same Dockerfile.
    let target_stage_name = "dev_containers_target_stage";
    let dockerfile_config = DockerfileConfig {
        base_image: base_dockerfile_final_stage.to_string(),
        target_stage: target_stage_name.to_string(),
        features_source_dir: staged.features_source_dir.display().to_string(),
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
    let mut dockerfile_file = std::fs::File::create(&dockerfile_path)?;
    dockerfile_file.write_all(combined.as_bytes())?;
    debug!(
        "Wrote merged Dockerfile ({} bytes) at {}",
        combined.len(),
        dockerfile_path.display()
    );

    let extended_image_tag = format!("deacon-devcontainer-features:{}", identity.workspace_hash);

    ensure_buildkit_or_error()?;
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

    let cli_docker = CliDocker::new();
    debug!("Building image with args: {:?}", build_args);
    let _image_id = cli_docker.build_image(&build_args).await.with_context(|| {
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
    })
}

/// Shared core: parse features from `config`, download them, resolve the
/// installation plan, and stage feature directories into a deterministic temp
/// directory so BuildKit can mount them as the
/// `dev_containers_feature_content_source` build context.
#[instrument(skip(config, identity))]
async fn resolve_and_stage_features(
    config: &DevContainerConfig,
    identity: &ContainerIdentity,
) -> Result<StagedFeatures> {
    let features_obj = config
        .features
        .as_object()
        .ok_or_else(|| DeaconError::Runtime("Features must be an object".to_string()))?;

    // Create feature fetcher
    let fetcher = default_fetcher()?;

    // Parse and fetch features
    let mut feature_refs: Vec<(String, FeatureRef)> = Vec::new();
    let mut feature_options_map: HashMap<String, HashMap<String, OptionValue>> = HashMap::new();

    for (feature_id, feature_options) in features_obj.iter() {
        let (registry_url, namespace, name, tag) =
            parse_registry_reference(feature_id).map_err(|e| {
                DeaconError::Runtime(format!("Invalid feature ID '{}': {}", feature_id, e))
            })?;

        let feature_ref = FeatureRef::new(registry_url, namespace, name, tag);
        let canonical_id = format!(
            "{}/{}/{}",
            feature_ref.registry, feature_ref.namespace, feature_ref.name
        );

        let options = if let Some(opts_obj) = feature_options.as_object() {
            opts_obj
                .iter()
                .filter_map(|(k, v)| {
                    let opt_val = match v {
                        serde_json::Value::Bool(b) => Some(OptionValue::Boolean(*b)),
                        serde_json::Value::String(s) => Some(OptionValue::String(s.clone())),
                        serde_json::Value::Number(n) => Some(OptionValue::Number(n.clone())),
                        serde_json::Value::Array(a) => Some(OptionValue::Array(a.clone())),
                        serde_json::Value::Object(o) => Some(OptionValue::Object(o.clone())),
                        serde_json::Value::Null => Some(OptionValue::Null),
                    };
                    opt_val.map(|v| (k.clone(), v))
                })
                .collect()
        } else {
            HashMap::new()
        };

        feature_options_map.insert(canonical_id.clone(), options);
        feature_refs.push((canonical_id, feature_ref));
    }

    // Download features
    debug!("Downloading {} features", feature_refs.len());
    let mut downloaded_features: HashMap<String, DownloadedFeature> = HashMap::new();
    for (canonical_id, feature_ref) in &feature_refs {
        let downloaded = fetcher.fetch_feature(feature_ref).await?;
        downloaded_features.insert(canonical_id.clone(), downloaded);
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
            source: reference.clone(),
            options,
            metadata: downloaded.metadata.clone(),
        });
    }

    // Resolve dependencies
    let override_order = config.override_feature_install_order.clone();
    let resolver = FeatureDependencyResolver::new(override_order);
    let installation_plan = resolver.resolve(&resolved_features)?;

    debug!(
        "Resolved {} features into {} levels",
        installation_plan.len(),
        installation_plan.levels.len()
    );

    // Collect combined env from feature metadata in plan order so later features win
    let mut combined_env = HashMap::new();
    for level in &installation_plan.levels {
        for feature_id in level {
            if let Some(feature) = installation_plan.get_feature(feature_id) {
                combined_env.extend(feature.metadata.container_env.clone());
            }
        }
    }

    // Create temporary directory for features and Dockerfile
    let temp_dir =
        std::env::temp_dir().join(format!("deacon-features-{}", identity.workspace_hash));
    std::fs::create_dir_all(&temp_dir)?;

    // Create features directory structure for BuildKit context
    let features_dir = temp_dir.join("features");
    std::fs::create_dir_all(&features_dir)?;

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
            copy_dir_all(&downloaded.path, &feature_dest)?;
        }
    }

    Ok(StagedFeatures {
        plan: installation_plan,
        combined_env,
        temp_dir,
        features_source_dir: features_dir,
    })
}

fn ensure_buildkit_or_error() -> Result<()> {
    use deacon_core::build::buildkit::is_buildkit_available;
    if !is_buildkit_available()? {
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
