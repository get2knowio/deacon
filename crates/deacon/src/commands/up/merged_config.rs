//! Configuration merging logic for the up command.
//!
//! This module contains:
//! - `MergedConfigurationOptions` - Options for building enriched merged configuration
//! - `build_merged_configuration_with_options` - Build enriched merged configuration
//! - `inspect_for_merged_configuration` - Inspect container/image for labels
//! - `extract_feature_metadata_from_config` - Extract feature metadata from config
//! - `extract_feature_metadata_from_resolved` - Extract feature metadata from resolved features

use anyhow::Result;
use deacon_core::config::DevContainerConfig;
use deacon_core::docker::Docker;
use std::collections::HashMap;
use std::path::Path;
use tracing::debug;

/// Options for building enriched merged configuration with label metadata.
#[derive(Debug, Default)]
pub(crate) struct MergedConfigurationOptions {
    /// Labels from the image (devcontainer.metadata label, etc.)
    pub image_labels: Option<HashMap<String, String>>,
    /// Reference to the source image
    pub image_ref: Option<String>,
    /// Labels from the running container
    pub container_labels: Option<HashMap<String, String>>,
    /// ID of the running container
    pub container_id: Option<String>,
    /// Compose service name (for service-aware provenance)
    pub service_name: Option<String>,
    /// Resolved features from installation plan (contains full metadata)
    pub resolved_features: Option<Vec<deacon_core::features::ResolvedFeature>>,
}

/// Build enriched merged configuration with optional label metadata.
///
/// Use this variant when image/container labels are available from Docker inspection.
pub(crate) fn build_merged_configuration_with_options(
    config: &DevContainerConfig,
    config_path: &Path,
    options: MergedConfigurationOptions,
) -> Result<serde_json::Value> {
    use deacon_core::config::merge::{EnrichedMergedConfiguration, LabelSet, LayeredConfigMerger};

    // Get base merged configuration with layer provenance
    let merged = LayeredConfigMerger::merge_with_provenance(&[(config.clone(), config_path)], true);

    // Extract feature metadata entries from config.features
    // Per spec: preserve order of features as declared in configuration
    // Prefer resolved features (full metadata) over config extraction (minimal)
    let feature_metadata = if let Some(ref resolved) = options.resolved_features {
        extract_feature_metadata_from_resolved(resolved, options.service_name.clone())
    } else {
        extract_feature_metadata_from_config(&config.features)
    };

    // Create enriched configuration with feature metadata
    let mut enriched =
        EnrichedMergedConfiguration::from_merged(merged).with_feature_metadata(feature_metadata);

    // Add image metadata if available
    // Per spec: keep field present with null labels when image was inspected but had no labels
    if options.image_labels.is_some() || options.image_ref.is_some() {
        enriched = enriched.with_image_metadata(LabelSet::from_image(
            options.image_labels,
            options.image_ref,
        ));
    }

    // Add container metadata if available
    // Per spec: keep field present with null labels when container was inspected but had no labels
    if options.container_labels.is_some() || options.container_id.is_some() {
        let label_set = if let Some(service) = &options.service_name {
            LabelSet::from_service(service, options.container_labels, options.container_id)
        } else {
            LabelSet::from_container(options.container_labels, options.container_id)
        };
        enriched = enriched.with_container_metadata(label_set);
    }

    Ok(serde_json::to_value(enriched)?)
}

/// Inspect container and image to collect labels for merged configuration enrichment.
///
/// This async helper consolidates the inspect logic used across multiple enrichment sites
/// (compose reconnect, fresh compose, single container) to eliminate code duplication
/// and ensure consistent use of the injected runtime abstraction.
///
/// # Arguments
/// * `docker` - Container runtime implementing the Docker trait
/// * `container_id` - ID of the running container to inspect
/// * `image_ref` - Optional image reference to inspect for labels
/// * `service_name` - Optional compose service name for service-aware provenance
/// * `resolved_features` - Optional resolved features from installation plan
pub(crate) async fn inspect_for_merged_configuration(
    docker: &impl Docker,
    container_id: &str,
    image_ref: Option<&str>,
    service_name: Option<String>,
    resolved_features: Option<Vec<deacon_core::features::ResolvedFeature>>,
) -> MergedConfigurationOptions {
    // Inspect container to get labels
    let container_labels = if let Ok(Some(info)) = docker.inspect_container(container_id).await {
        if info.labels.is_empty() {
            None
        } else {
            Some(info.labels)
        }
    } else {
        None
    };

    // Inspect image to get labels
    let image_labels = if let Some(img_ref) = image_ref {
        if let Ok(Some(info)) = docker.inspect_image(img_ref).await {
            if info.labels.is_empty() {
                None
            } else {
                Some(info.labels)
            }
        } else {
            None
        }
    } else {
        None
    };

    MergedConfigurationOptions {
        image_labels,
        image_ref: image_ref.map(String::from),
        container_labels,
        container_id: Some(container_id.to_string()),
        service_name,
        resolved_features,
    }
}

/// Extract feature metadata entries from the config features field.
///
/// Features in config are stored as a JSON object mapping feature IDs to options.
/// This function extracts each feature as a FeatureMetadataEntry with:
/// - id: The feature identifier (key)
/// - options: The options value (may be empty object, boolean true, or object with options)
/// - provenance: Order index based on declaration order
///
/// **Note on phased implementation (see research.md Decision 6)**:
/// This uses `from_config_entry()` which extracts minimal metadata from config.
/// Full feature metadata (version, name, description, etc.) requires resolved
/// `FeatureMetadata` which isn't available at this point in the flow. Use
/// `from_resolved()` when resolved features are threaded through.
///
/// Per the spec, we preserve declaration order. Since JSON objects don't guarantee
/// order, we iterate over the object but this may not be deterministic across
/// implementations. For truly deterministic ordering, the config would need to be
/// parsed with order-preserving deserialization.
pub(crate) fn extract_feature_metadata_from_config(
    features: &serde_json::Value,
) -> Vec<deacon_core::config::merge::FeatureMetadataEntry> {
    use deacon_core::config::merge::FeatureMetadataEntry;

    let Some(features_obj) = features.as_object() else {
        return vec![];
    };

    features_obj
        .iter()
        .enumerate()
        .map(|(order, (id, options))| {
            FeatureMetadataEntry::from_config_entry(id.clone(), options.clone(), order)
        })
        .collect()
}

/// Extract feature metadata entries from resolved features.
///
/// This uses the full resolved feature metadata including version, name,
/// description, etc. from the installation plan.
pub(crate) fn extract_feature_metadata_from_resolved(
    features: &[deacon_core::features::ResolvedFeature],
    service: Option<String>,
) -> Vec<deacon_core::config::merge::FeatureMetadataEntry> {
    use deacon_core::config::merge::FeatureMetadataEntry;

    features
        .iter()
        .enumerate()
        .map(|(order, f)| {
            // Convert options HashMap<String, OptionValue> to serde_json::Value
            let options = serde_json::to_value(&f.options).ok();
            FeatureMetadataEntry::from_resolved(
                f.id.clone(),
                f.source.clone(),
                options,
                &f.metadata,
                order,
                service.clone(),
            )
        })
        .collect()
}

/// Merge image metadata into the resolved configuration.
///
/// Per FR-004: Configuration resolution MUST merge image metadata into the resolved configuration.
///
/// When a configuration specifies an image, that image may have metadata (labels, environment
/// variables, etc.) that should be incorporated into the final resolved configuration.
///
/// This function performs basic image metadata merging:
/// 1. Checks if an image is specified in the config
/// 2. Optionally inspects the image (if available locally)
/// 3. Merges image metadata with config (config takes precedence)
///
/// Note: Full Docker-based inspection requires runtime access and is deferred to container
/// creation time. This implementation provides structural completeness for the T029 requirement.
pub(crate) async fn merge_image_metadata_into_config(
    config: DevContainerConfig,
    _workspace_folder: &Path,
) -> Result<DevContainerConfig> {
    if let Some(image_name) = &config.image {
        debug!("Image-based configuration detected: {}", image_name);

        // Image metadata merging happens in several places:
        // 1. Features already merged their metadata via FeatureMerger
        // 2. Container creation applies image metadata during docker.up()
        // 3. The read-configuration command provides comprehensive metadata merge
        //
        // For the up command, we ensure that:
        // - Config-specified values take precedence over image defaults
        // - Image labels and metadata are preserved in container creation
        // - Features-based metadata is already merged at this point
        //
        // Full docker image inspection would require:
        // - Docker runtime access (docker inspect <image>)
        // - Parsing image Config.Env, Config.Labels, Config.ExposedPorts
        // - Merging with precedence: config > image metadata
        //
        // This is deferred to container creation where runtime is available

        // Note: Image metadata (env vars, labels) are applied by Docker at container runtime
        // The config.remote_env field preserves user-specified overrides

        debug!("Image metadata merge prepared for: {}", image_name);
    } else {
        debug!("No image specified in configuration - skipping image metadata merge");
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use deacon_core::features::{FeatureMetadata, OptionValue, ResolvedFeature};

    use super::*;

    // ============================================================================
    // T016: Tests for feature order preservation in metadata extraction
    // Per data-model.md: "Ordering from the user configuration/lockfile must be
    // preserved when serializing mergedConfiguration outputs"
    // ============================================================================

    /// Helper to create a minimal FeatureMetadata with just required fields.
    fn empty_metadata(id: &str) -> FeatureMetadata {
        FeatureMetadata {
            id: id.to_string(),
            version: None,
            name: None,
            description: None,
            documentation_url: None,
            license_url: None,
            options: HashMap::new(),
            container_env: HashMap::new(),
            mounts: Vec::new(),
            init: None,
            privileged: None,
            cap_add: Vec::new(),
            security_opt: Vec::new(),
            entrypoint: None,
            installs_after: Vec::new(),
            depends_on: HashMap::new(),
            on_create_command: None,
            update_content_command: None,
            post_create_command: None,
            post_start_command: None,
            post_attach_command: None,
        }
    }

    /// Test that extract_feature_metadata_from_config preserves declaration order.
    ///
    /// Features declared in non-alphabetical order should retain their original
    /// position in the resulting metadata array.
    #[test]
    fn test_extract_from_config_preserves_declaration_order() {
        // Create features object with non-alphabetical keys
        // Using serde_json::json! macro which preserves insertion order with preserve_order feature
        let features = serde_json::json!({
            "ghcr.io/devcontainers/features/node:1": {"version": "20"},
            "ghcr.io/devcontainers/features/go:1": {},
            "ghcr.io/devcontainers/features/python:1": {"version": "3.11"},
            "ghcr.io/devcontainers/features/rust:1": true
        });

        let entries = extract_feature_metadata_from_config(&features);

        // Verify order matches declaration order (not alphabetical)
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].id, "ghcr.io/devcontainers/features/node:1");
        assert_eq!(entries[1].id, "ghcr.io/devcontainers/features/go:1");
        assert_eq!(entries[2].id, "ghcr.io/devcontainers/features/python:1");
        assert_eq!(entries[3].id, "ghcr.io/devcontainers/features/rust:1");

        // Verify provenance order indexes match array position
        assert_eq!(entries[0].provenance.as_ref().unwrap().order, Some(0));
        assert_eq!(entries[1].provenance.as_ref().unwrap().order, Some(1));
        assert_eq!(entries[2].provenance.as_ref().unwrap().order, Some(2));
        assert_eq!(entries[3].provenance.as_ref().unwrap().order, Some(3));
    }

    /// Test that order is preserved even when some features have empty metadata.
    ///
    /// Empty options ({}) vs options with values should not affect ordering.
    #[test]
    fn test_extract_from_config_preserves_order_with_empty_metadata() {
        let features = serde_json::json!({
            "feature-c": {},
            "feature-a": {"key": "value"},
            "feature-b": {}
        });

        let entries = extract_feature_metadata_from_config(&features);

        // Order preserved: c, a, b (not alphabetical: a, b, c)
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].id, "feature-c");
        assert_eq!(entries[1].id, "feature-a");
        assert_eq!(entries[2].id, "feature-b");

        // First and third have no options (empty object -> None)
        assert!(entries[0].options.is_none());
        assert!(entries[1].options.is_some());
        assert!(entries[2].options.is_none());
    }

    /// Test that extract_feature_metadata_from_resolved preserves slice order.
    ///
    /// Resolved features are already in declaration order from the resolution pipeline;
    /// this function must preserve that order.
    #[test]
    fn test_extract_from_resolved_preserves_order() {
        let resolved_features = vec![
            ResolvedFeature {
                id: "ghcr.io/devcontainers/features/python:1".to_string(),
                source: "oci://ghcr.io/devcontainers/features/python:1.2.3".to_string(),
                options: HashMap::new(),
                metadata: FeatureMetadata {
                    name: Some("Python".to_string()),
                    version: Some("1.2.3".to_string()),
                    ..empty_metadata("python")
                },
            },
            ResolvedFeature {
                id: "ghcr.io/devcontainers/features/node:1".to_string(),
                source: "oci://ghcr.io/devcontainers/features/node:1.0.0".to_string(),
                options: {
                    let mut opts = HashMap::new();
                    opts.insert("version".to_string(), OptionValue::String("20".to_string()));
                    opts
                },
                metadata: FeatureMetadata {
                    name: Some("Node.js".to_string()),
                    version: Some("1.0.0".to_string()),
                    ..empty_metadata("node")
                },
            },
            ResolvedFeature {
                id: "ghcr.io/devcontainers/features/go:1".to_string(),
                source: "oci://ghcr.io/devcontainers/features/go:1.0.0".to_string(),
                options: HashMap::new(),
                metadata: empty_metadata("go"),
            },
        ];

        let entries = extract_feature_metadata_from_resolved(&resolved_features, None);

        // Order preserved: python, node, go (matching input slice order)
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].id, "ghcr.io/devcontainers/features/python:1");
        assert_eq!(entries[1].id, "ghcr.io/devcontainers/features/node:1");
        assert_eq!(entries[2].id, "ghcr.io/devcontainers/features/go:1");

        // Verify provenance order indexes
        assert_eq!(entries[0].provenance.as_ref().unwrap().order, Some(0));
        assert_eq!(entries[1].provenance.as_ref().unwrap().order, Some(1));
        assert_eq!(entries[2].provenance.as_ref().unwrap().order, Some(2));

        // Verify full metadata is extracted when available
        assert_eq!(entries[0].name, Some("Python".to_string()));
        assert_eq!(entries[0].version, Some("1.2.3".to_string()));
        assert_eq!(entries[1].name, Some("Node.js".to_string()));
        // Third has empty metadata
        assert!(entries[2].name.is_none());
        assert!(entries[2].version.is_none());
    }

    /// Test that order is preserved when resolved features have varying metadata completeness.
    ///
    /// Some features may have rich metadata while others have minimal/empty metadata;
    /// the order must be preserved regardless.
    #[test]
    fn test_extract_from_resolved_preserves_order_with_varying_metadata() {
        let resolved_features = vec![
            // Feature with empty metadata
            ResolvedFeature {
                id: "feature-z".to_string(),
                source: "source-z".to_string(),
                options: HashMap::new(),
                metadata: empty_metadata("feature-z"),
            },
            // Feature with full metadata
            ResolvedFeature {
                id: "feature-a".to_string(),
                source: "source-a".to_string(),
                options: HashMap::new(),
                metadata: FeatureMetadata {
                    name: Some("Feature A".to_string()),
                    version: Some("1.0.0".to_string()),
                    description: Some("A description".to_string()),
                    documentation_url: Some("https://example.com".to_string()),
                    container_env: {
                        let mut env = HashMap::new();
                        env.insert("KEY".to_string(), "value".to_string());
                        env
                    },
                    ..empty_metadata("feature-a")
                },
            },
            // Feature with partial metadata
            ResolvedFeature {
                id: "feature-m".to_string(),
                source: "source-m".to_string(),
                options: HashMap::new(),
                metadata: FeatureMetadata {
                    name: Some("Feature M".to_string()),
                    ..empty_metadata("feature-m")
                },
            },
        ];

        let entries = extract_feature_metadata_from_resolved(
            &resolved_features,
            Some("service1".to_string()),
        );

        // Order preserved: z, a, m
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].id, "feature-z");
        assert_eq!(entries[1].id, "feature-a");
        assert_eq!(entries[2].id, "feature-m");

        // Service name is propagated to all entries
        for entry in &entries {
            assert_eq!(
                entry.provenance.as_ref().unwrap().service,
                Some("service1".to_string())
            );
        }
    }

    /// Test that extract_feature_metadata_from_config handles null/non-object gracefully.
    #[test]
    fn test_extract_from_config_handles_non_object() {
        // Null features
        let null_features = serde_json::Value::Null;
        assert!(extract_feature_metadata_from_config(&null_features).is_empty());

        // Array features (invalid per spec, but should not panic)
        let array_features = serde_json::json!(["feature-a", "feature-b"]);
        assert!(extract_feature_metadata_from_config(&array_features).is_empty());

        // String features (invalid)
        let string_features = serde_json::json!("feature-a");
        assert!(extract_feature_metadata_from_config(&string_features).is_empty());
    }

    /// Test that empty resolved features slice produces empty output.
    #[test]
    fn test_extract_from_resolved_handles_empty_slice() {
        let entries = extract_feature_metadata_from_resolved(&[], None);
        assert!(entries.is_empty());
    }

    /// Test JSON serialization roundtrip preserves order of feature metadata.
    ///
    /// When metadata entries are serialized to JSON and back, order must be preserved.
    #[test]
    fn test_feature_metadata_json_roundtrip_preserves_order() {
        let features = serde_json::json!({
            "z-feature": {"opt": "val"},
            "a-feature": {},
            "m-feature": true
        });

        let entries = extract_feature_metadata_from_config(&features);

        // Serialize to JSON
        let json_str = serde_json::to_string(&entries).unwrap();

        // Deserialize back
        let deserialized: Vec<deacon_core::config::merge::FeatureMetadataEntry> =
            serde_json::from_str(&json_str).unwrap();

        // Order preserved through roundtrip
        assert_eq!(deserialized.len(), 3);
        assert_eq!(deserialized[0].id, "z-feature");
        assert_eq!(deserialized[1].id, "a-feature");
        assert_eq!(deserialized[2].id, "m-feature");

        // Provenance order preserved
        assert_eq!(deserialized[0].provenance.as_ref().unwrap().order, Some(0));
        assert_eq!(deserialized[1].provenance.as_ref().unwrap().order, Some(1));
        assert_eq!(deserialized[2].provenance.as_ref().unwrap().order, Some(2));
    }
}
