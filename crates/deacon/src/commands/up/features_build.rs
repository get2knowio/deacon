//! Feature image building with BuildKit.
//!
//! This module contains:
//! - `FeatureBuildOutput` - Output from feature image building
//! - `build_image_with_features` - Build extended image with features
//! - `copy_dir_all` - Recursive directory copy helper

use anyhow::Result;
use deacon_core::build::BuildOptions;
use deacon_core::config::DevContainerConfig;
use deacon_core::container::ContainerIdentity;
use deacon_core::errors::DeaconError;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, instrument};

/// Output from building an image with features
#[derive(Debug, Clone)]
pub(crate) struct FeatureBuildOutput {
    pub image_tag: String,
    pub combined_env: HashMap<String, String>,
    pub resolved_features: Vec<deacon_core::features::ResolvedFeature>,
}

/// Build an extended Docker image with features installed using BuildKit
///
/// This function:
/// 1. Parses and resolves feature dependencies from the configuration
/// 2. Downloads features from OCI registries
/// 3. Generates a Dockerfile with BuildKit mount syntax for features
/// 4. Builds the extended image using docker buildx build
/// 5. Returns the tag of the newly built image and combined env from feature metadata
///
/// # Arguments
///
/// * `config` - DevContainer configuration containing features to install
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
    use deacon_core::dockerfile_generator::{DockerfileConfig, DockerfileGenerator};
    use deacon_core::features::{FeatureDependencyResolver, OptionValue, ResolvedFeature};
    use deacon_core::oci::{default_fetcher, DownloadedFeature, FeatureRef};
    use deacon_core::registry_parser::parse_registry_reference;
    use std::io::Write;

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

    // Create feature fetcher
    let fetcher = default_fetcher()?;

    // Parse and fetch features
    let mut feature_refs: Vec<(String, FeatureRef)> = Vec::new();
    let mut feature_options_map: HashMap<String, HashMap<String, OptionValue>> = HashMap::new();

    for (feature_id, feature_options) in features_obj.iter() {
        // Parse feature reference
        let (registry_url, namespace, name, tag) =
            parse_registry_reference(feature_id).map_err(|e| {
                DeaconError::Runtime(format!("Invalid feature ID '{}': {}", feature_id, e))
            })?;

        let feature_ref = FeatureRef::new(registry_url, namespace, name, tag);
        // Canonical ID (no version) so dependency matching aligns with installsAfter/dependsOn entries
        let canonical_id = format!(
            "{}/{}/{}",
            feature_ref.registry, feature_ref.namespace, feature_ref.name
        );

        // Parse options
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

        // Start with user-provided options
        let mut options = feature_options_map
            .get(canonical_id)
            .cloned()
            .unwrap_or_default();

        // Fill in defaults from metadata when the user did not supply a value
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

            // Find the downloaded feature directory
            let downloaded = downloaded_features.get(feature_id).ok_or_else(|| {
                DeaconError::Runtime(format!("Downloaded feature {} not found", feature_id))
            })?;

            // Sanitize feature ID for directory name
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

            // Copy feature directory to BuildKit context
            let feature_dir_name = format!("{}_{}", sanitized_id, level_idx);
            let feature_dest = features_dir.join(&feature_dir_name);
            copy_dir_all(&downloaded.path, &feature_dest)?;
        }
    }

    // Generate Dockerfile
    let dockerfile_config = DockerfileConfig {
        base_image: base_image.clone(),
        target_stage: "dev_containers_target_stage".to_string(),
        features_source_dir: features_dir.display().to_string(),
    };

    let generator = DockerfileGenerator::new(dockerfile_config.clone());
    let dockerfile_content = generator.generate(&installation_plan)?;

    // Write Dockerfile
    let dockerfile_path = temp_dir.join("Dockerfile.extended");
    let mut dockerfile_file = std::fs::File::create(&dockerfile_path)?;
    dockerfile_file.write_all(dockerfile_content.as_bytes())?;

    debug!("Generated Dockerfile at {}", dockerfile_path.display());

    // Generate image tag
    let extended_image_tag = format!("deacon-devcontainer-features:{}", identity.workspace_hash);

    // Check BuildKit availability
    use deacon_core::build::buildkit::is_buildkit_available;
    if !is_buildkit_available()? {
        return Err(DeaconError::Runtime(
            "BuildKit is required for feature installation. Please enable BuildKit.".to_string(),
        )
        .into());
    }

    // Log cache configuration before feature build starts (per research.md Decision 2).
    // Docker/BuildKit handles cache failures gracefully; we inform users of the configuration.
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

    // Build image with BuildKit
    // Pass build_options to include cache-from/cache-to/buildx settings per spec (data-model.md)
    let build_args =
        generator.generate_build_args(&dockerfile_path, &extended_image_tag, build_options);

    // Execute build using CliDocker
    let cli_docker = CliDocker::new();
    debug!("Building image with args: {:?}", build_args);
    let _image_id = cli_docker.build_image(&build_args).await?;

    info!("Successfully built extended image: {}", extended_image_tag);

    Ok(FeatureBuildOutput {
        image_tag: extended_image_tag,
        combined_env,
        resolved_features: installation_plan.features.clone(),
    })
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
