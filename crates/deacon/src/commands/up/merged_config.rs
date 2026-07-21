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
use deacon_core::dockerfile_generator::FeatureInstallEnv;
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
/// This is the call-site invoked during configuration resolution, before the
/// image is built or pulled. The image isn't locally available yet, so the
/// real merge happens later in [`merge_image_metadata_after_image_ready`]
/// once the image has materialized. We keep this thin stub at the resolution
/// stage so future enhancements (e.g. pre-pull) have a hook.
pub(crate) async fn merge_image_metadata_into_config(
    config: DevContainerConfig,
    _workspace_folder: &Path,
) -> Result<DevContainerConfig> {
    if let Some(image_name) = &config.image {
        debug!(
            "Image-based configuration detected: {} (metadata merge deferred to post-build)",
            image_name
        );
    } else {
        debug!("No image specified in configuration - skipping image metadata merge");
    }

    Ok(config)
}

/// Merge the image's `devcontainer.metadata` LABEL into the user config.
///
/// Per the upstream spec (`docs/specs/devcontainer-reference.md` § Image
/// Metadata): when an image is used, the CLI MUST read the
/// `devcontainer.metadata` LABEL — either a single partial `devcontainer.json`
/// object or an array of partial entries — and merge each entry into the resolved configuration with
/// **lower precedence than the user's devcontainer.json**.
///
/// This call site runs *after* the image is locally available (i.e. after
/// `build_image_with_features` for feature builds, or after a pull/load for
/// plain `image:` configs).
///
/// Behaviour (#70):
/// - If the image cannot be inspected (not local, daemon unavailable, etc.),
///   log a warn and return the config unchanged. The user's invocation still
///   succeeds — the merge is best-effort lifting of metadata baked into the
///   image. The container itself still runs with the image's own ENV/USER
///   instructions, which Docker applies at run time.
/// - If the LABEL is absent, return the config unchanged.
/// - If the LABEL is present but malformed (not valid JSON, or entries
///   that don't deserialize as `DevContainerConfig`), surface a warn with
///   the parse error and return the config unchanged. We do not fail the
///   `up` flow on a bad image label — the spec says image metadata is the
///   *lower-precedence* layer, so a broken label simply contributes nothing.
/// - On success, each entry is folded into the resolved config via the
///   existing `ConfigMerger`, with the user's config winning on conflict.
pub(crate) async fn merge_image_metadata_after_image_ready(
    docker: &impl Docker,
    image_ref: &str,
    user_config: DevContainerConfig,
) -> DevContainerConfig {
    let info = match docker.inspect_image(image_ref).await {
        Ok(Some(info)) => info,
        Ok(None) => {
            debug!(
                "Image '{}' not locally available; image-metadata merge skipped (#70)",
                image_ref
            );
            return user_config;
        }
        Err(e) => {
            tracing::warn!(
                "Failed to inspect image '{}' for devcontainer.metadata label; \
                 proceeding without image-metadata merge: {}",
                image_ref,
                e
            );
            return user_config;
        }
    };

    apply_image_metadata_label(
        image_ref,
        info.labels.get("devcontainer.metadata"),
        user_config,
    )
}

/// Resolve the four spec-mandated feature-install env vars (`_REMOTE_USER`,
/// `_REMOTE_USER_HOME`, `_CONTAINER_USER`, `_CONTAINER_USER_HOME`) for a
/// feature build on top of `base_image`.
///
/// Why this exists (#89): the feature build bakes these values into the
/// generated Dockerfile, so they must be known *before* the build runs. But
/// `remoteUser` frequently comes from the base image's `devcontainer.metadata`
/// LABEL rather than the user's devcontainer.json — e.g.
/// `mcr.microsoft.com/devcontainers/base` declares `{"remoteUser": "vscode"}` —
/// and [`merge_image_metadata_after_image_ready`] only folds that in *after*
/// the build. Resolving from the user config alone therefore emitted
/// `_REMOTE_USER=""`, which silently breaks any feature that does
/// `su - "$_REMOTE_USER" -c ...`.
///
/// Precedence (mirrors upstream `@devcontainers/cli`):
/// `remoteUser` → user config, else image metadata, else `containerUser`;
/// `containerUser` → user config, else image metadata, else the image's
/// baked-in `USER`, else `root`.
///
/// The metadata is applied to a **clone** of the config purely to derive the
/// effective users; the clone is discarded. The real merge stays where it is,
/// post-build. This is deliberate — the feature-extended image inherits the
/// base's `devcontainer.metadata` LABEL verbatim (deacon emits no LABEL of its
/// own), so merging here *and* post-build would fold the same entries twice and
/// duplicate concatenated fields like `runArgs`. Sharing
/// [`apply_image_metadata_label`] keeps this resolution byte-identical to the
/// post-build merge.
///
/// Best-effort: if the image can't be inspected or pulled we warn and fall back
/// to the user config alone. We don't fail the build — configs whose features
/// never read `_REMOTE_USER` are unaffected — but the warning makes the gap
/// visible instead of silent.
pub(crate) async fn resolve_feature_install_env(
    docker: &impl Docker,
    base_image: &str,
    config: &DevContainerConfig,
) -> FeatureInstallEnv {
    let info = match docker.ensure_image_available(base_image).await {
        Ok(Some(info)) => Some(info),
        Ok(None) => {
            tracing::warn!(
                "Image '{}' is unavailable locally and could not be pulled; resolving \
                 feature install env (_REMOTE_USER, _CONTAINER_USER) from devcontainer.json \
                 alone. Features that depend on these may misbehave — set \"remoteUser\" / \
                 \"containerUser\" explicitly to be sure (#89).",
                base_image
            );
            None
        }
        Err(e) => {
            tracing::warn!(
                "Failed to inspect image '{}' while resolving feature install env \
                 (_REMOTE_USER, _CONTAINER_USER); falling back to devcontainer.json alone: {}",
                base_image,
                e
            );
            None
        }
    };

    let Some(info) = info else {
        return resolve_feature_install_env_from_image(base_image, None, None, config);
    };

    // The image was inspected successfully, so an absent `Config.User` really
    // does mean `root` (upstream: `imageDetails.Config.User || 'root'`).
    let image_user = info.user.as_deref().unwrap_or("root");

    let env = resolve_feature_install_env_from_image(
        base_image,
        info.labels.get("devcontainer.metadata"),
        Some(image_user),
        config,
    );

    debug!(
        remote_user = ?env.remote_user,
        container_user = ?env.container_user,
        image_user = %image_user,
        "Resolved feature install env from image metadata + config (#89)"
    );

    env
}

/// Pure helper behind [`resolve_feature_install_env`]. Extracted so the
/// precedence rules can be unit-tested without a Docker mock (mirrors
/// [`apply_image_metadata_label`]).
///
/// `image_user` is `None` when the image could not be inspected at all — in
/// which case there is no metadata label either and resolution degrades to the
/// user config alone.
fn resolve_feature_install_env_from_image(
    base_image: &str,
    label: Option<&String>,
    image_user: Option<&str>,
    config: &DevContainerConfig,
) -> FeatureInstallEnv {
    // Fold the image's metadata in at lower precedence than the user config,
    // then read the effective users back off the result.
    let effective = apply_image_metadata_label(base_image, label, config.clone());

    FeatureInstallEnv::resolve(
        effective.remote_user.as_deref(),
        effective.container_user.as_deref(),
        image_user,
    )
}

/// Pure helper: merge the given image-metadata label (raw JSON string) into
/// `user_config`. Extracted from [`merge_image_metadata_after_image_ready`]
/// so the parse + merge logic can be unit-tested without a Docker mock (#70).
fn apply_image_metadata_label(
    image_ref: &str,
    label_value: Option<&String>,
    user_config: DevContainerConfig,
) -> DevContainerConfig {
    use deacon_core::config::ConfigMerger;

    let Some(label_json) = label_value else {
        debug!(
            "Image '{}' has no devcontainer.metadata label; nothing to merge (#70)",
            image_ref
        );
        return user_config;
    };

    // The label may be a single object or an array of partial config entries;
    // both forms are accepted per the image-metadata spec (#70, #300).
    let entries: Vec<DevContainerConfig> =
        match deacon_core::config::parse_image_metadata_label(label_json) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    "Image '{}' has a devcontainer.metadata label that is not a valid \
                     devcontainer metadata object/array; proceeding without merge: {}",
                    image_ref,
                    e
                );
                return user_config;
            }
        };

    if entries.is_empty() {
        return user_config;
    }

    debug!(
        "Merging {} entry(ies) from image '{}' devcontainer.metadata label as the \
         lower-precedence layer (#70)",
        entries.len(),
        image_ref
    );

    // Spec ordering: image metadata is lower precedence, user config is
    // higher. ConfigMerger::merge_configs folds left-to-right with later
    // entries winning, so push image entries first then the user config.
    let mut chain: Vec<DevContainerConfig> = entries;
    chain.push(user_config);
    ConfigMerger::merge_configs(&chain)
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
            customizations: None,
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

#[cfg(test)]
mod image_metadata_merge_tests {
    //! Spec parity (#70): the image's `devcontainer.metadata` LABEL is a
    //! JSON array of partial devcontainer entries that MUST be merged with
    //! the user's devcontainer.json as the *lower-precedence* layer (user
    //! config wins on conflict).

    use super::apply_image_metadata_label;
    use deacon_core::config::DevContainerConfig;
    use std::collections::HashMap;

    fn user_config_with_env(pairs: &[(&str, &str)]) -> DevContainerConfig {
        let mut container_env = HashMap::new();
        for (k, v) in pairs {
            container_env.insert((*k).to_string(), (*v).to_string());
        }
        DevContainerConfig {
            container_env,
            ..DevContainerConfig::default()
        }
    }

    #[test]
    fn image_metadata_container_env_merges_in_at_lower_precedence() {
        // Image label declares two containerEnv keys, one of which the user
        // also declares. After the merge:
        //   - IMAGE_LAYER (image only) must be present
        //   - CONFIG_LAYER (user only) must be present
        //   - MERGED_LAYER (both) must take the user's value (user wins)
        let label = r#"[{
            "remoteUser": "root",
            "containerEnv": {
                "IMAGE_LAYER":  "from-image",
                "MERGED_LAYER": "image-loses"
            }
        }]"#
        .to_string();

        let user =
            user_config_with_env(&[("CONFIG_LAYER", "from-user"), ("MERGED_LAYER", "user-wins")]);

        let merged = apply_image_metadata_label("alpine:3.18", Some(&label), user);

        assert_eq!(
            merged.container_env.get("IMAGE_LAYER"),
            Some(&"from-image".to_string()),
            "image-only containerEnv key must survive the merge (#70)"
        );
        assert_eq!(
            merged.container_env.get("CONFIG_LAYER"),
            Some(&"from-user".to_string()),
            "user containerEnv key must survive the merge"
        );
        assert_eq!(
            merged.container_env.get("MERGED_LAYER"),
            Some(&"user-wins".to_string()),
            "on conflict, user devcontainer.json wins over image metadata (#70)"
        );
    }

    #[test]
    fn image_metadata_remote_user_only_applied_when_user_did_not_set_it() {
        // Image declares remoteUser=root; user devcontainer.json leaves it unset.
        // After merge, remoteUser should be "root" from the image layer.
        let label = r#"[{ "remoteUser": "root" }]"#.to_string();
        let user = DevContainerConfig::default();
        let merged = apply_image_metadata_label("alpine:3.18", Some(&label), user);
        assert_eq!(merged.remote_user.as_deref(), Some("root"));

        // When the user explicitly sets remoteUser, the user wins.
        let user = DevContainerConfig {
            remote_user: Some("devuser".to_string()),
            ..DevContainerConfig::default()
        };
        let merged = apply_image_metadata_label("alpine:3.18", Some(&label), user);
        assert_eq!(merged.remote_user.as_deref(), Some("devuser"));
    }

    #[test]
    fn object_form_image_metadata_label_is_applied() {
        let label = r#"{
            "containerEnv": {
                "R4_PREBUILT": "object"
            },
            "remoteEnv": {
                "R4_PREBUILT_REMOTE": "remote"
            },
            "init": true
        }"#
        .to_string();
        let user = DevContainerConfig::default();
        let merged = apply_image_metadata_label("alpine:3.18", Some(&label), user);

        assert_eq!(
            merged.container_env.get("R4_PREBUILT"),
            Some(&"object".to_string())
        );
        assert_eq!(
            merged
                .remote_env
                .get("R4_PREBUILT_REMOTE")
                .and_then(|v| v.as_deref()),
            Some("remote")
        );
        assert_eq!(merged.init, Some(true));
    }

    #[test]
    fn missing_label_is_a_noop() {
        let user = user_config_with_env(&[("X", "1")]);
        let user_clone = user.clone();
        let merged = apply_image_metadata_label("alpine:3.18", None, user);
        assert_eq!(merged.container_env, user_clone.container_env);
    }

    #[test]
    fn malformed_label_does_not_panic_and_returns_user_config_unchanged() {
        // Spec parity (#70): a malformed image label must not break `up`.
        // The user's config is the higher-precedence layer; we keep it
        // as-is and log a warning (covered by tracing, not asserted here).
        let user = user_config_with_env(&[("X", "1")]);
        let user_clone = user.clone();
        let label = r#"this is not JSON"#.to_string();
        let merged = apply_image_metadata_label("alpine:3.18", Some(&label), user);
        assert_eq!(merged.container_env, user_clone.container_env);

        // Also handle "valid JSON but not a devcontainer config object/array".
        let user = user_config_with_env(&[("X", "1")]);
        let user_clone = user.clone();
        let label = r#"true"#.to_string();
        let merged = apply_image_metadata_label("alpine:3.18", Some(&label), user);
        assert_eq!(merged.container_env, user_clone.container_env);
    }

    #[test]
    fn empty_array_label_is_a_noop() {
        let user = user_config_with_env(&[("X", "1")]);
        let user_clone = user.clone();
        let label = r#"[]"#.to_string();
        let merged = apply_image_metadata_label("alpine:3.18", Some(&label), user);
        assert_eq!(merged.container_env, user_clone.container_env);
    }
}

#[cfg(test)]
mod feature_install_env_tests {
    //! Spec parity (#89): the four `_*_USER` env vars handed to every feature's
    //! `install.sh` must be resolved from the base image's
    //! `devcontainer.metadata` LABEL and baked-in `USER` folded *under* the
    //! user's devcontainer.json — not from the user config alone.

    use super::resolve_feature_install_env_from_image;
    use deacon_core::config::DevContainerConfig;

    /// The `mcr.microsoft.com/devcontainers/base` family declares its
    /// `remoteUser` via the image label rather than the user's config.
    fn base_image_label() -> String {
        r#"[
            { "id": "ghcr.io/devcontainers/features/common-utils:2" },
            { "remoteUser": "vscode" }
        ]"#
        .to_string()
    }

    #[test]
    fn remote_user_comes_from_image_metadata_when_config_is_silent() {
        // The regression that motivated this (#89): devcontainer.json sets
        // neither remoteUser nor containerUser, the base image declares
        // remoteUser=vscode, and features doing `su - "$_REMOTE_USER"` were
        // handed an empty string and silently failed to install.
        let env = resolve_feature_install_env_from_image(
            "mcr.microsoft.com/devcontainers/base:ubuntu-24.04",
            Some(&base_image_label()),
            Some("root"),
            &DevContainerConfig::default(),
        );

        assert_eq!(
            env.remote_user.as_deref(),
            Some("vscode"),
            "_REMOTE_USER must come from the image's devcontainer.metadata label"
        );
        assert_eq!(
            env.remote_user_home.as_deref(),
            Some("/home/vscode"),
            "_REMOTE_USER_HOME must follow the resolved remote user"
        );
        // containerUser is unset in both config and label, so it falls back to
        // the image's baked-in USER.
        assert_eq!(env.container_user.as_deref(), Some("root"));
        assert_eq!(env.container_user_home.as_deref(), Some("/root"));
    }

    #[test]
    fn user_config_wins_over_image_metadata() {
        let user = DevContainerConfig {
            remote_user: Some("devuser".to_string()),
            ..DevContainerConfig::default()
        };
        let env = resolve_feature_install_env_from_image(
            "mcr.microsoft.com/devcontainers/base:ubuntu-24.04",
            Some(&base_image_label()),
            Some("root"),
            &user,
        );
        assert_eq!(env.remote_user.as_deref(), Some("devuser"));
        assert_eq!(env.remote_user_home.as_deref(), Some("/home/devuser"));
    }

    #[test]
    fn remote_user_falls_back_to_container_user_then_image_user() {
        // No remoteUser anywhere, but containerUser is set: _REMOTE_USER
        // defaults to _CONTAINER_USER per the features spec.
        let user = DevContainerConfig {
            container_user: Some("builder".to_string()),
            ..DevContainerConfig::default()
        };
        let env = resolve_feature_install_env_from_image("img", None, Some("root"), &user);
        assert_eq!(env.remote_user.as_deref(), Some("builder"));
        assert_eq!(env.container_user.as_deref(), Some("builder"));

        // Nothing set anywhere: both fall through to the image's USER.
        let env = resolve_feature_install_env_from_image(
            "img",
            None,
            Some("node"),
            &DevContainerConfig::default(),
        );
        assert_eq!(env.remote_user.as_deref(), Some("node"));
        assert_eq!(env.container_user.as_deref(), Some("node"));
        assert_eq!(env.remote_user_home.as_deref(), Some("/home/node"));
    }

    #[test]
    fn uninspectable_image_degrades_to_user_config() {
        // Image could not be pulled/inspected (image_user = None). We still
        // honor whatever the config states rather than inventing a user.
        let user = DevContainerConfig {
            remote_user: Some("devuser".to_string()),
            ..DevContainerConfig::default()
        };
        let env = resolve_feature_install_env_from_image("img", None, None, &user);
        assert_eq!(env.remote_user.as_deref(), Some("devuser"));

        // With nothing to go on, everything stays None — the generator emits
        // empty strings and the caller has already warned.
        let env = resolve_feature_install_env_from_image(
            "img",
            None,
            None,
            &DevContainerConfig::default(),
        );
        assert_eq!(env.remote_user, None);
        assert_eq!(env.container_user, None);
    }

    #[test]
    fn malformed_label_does_not_break_resolution() {
        let bad = "{ not a json array }".to_string();
        let env = resolve_feature_install_env_from_image(
            "img",
            Some(&bad),
            Some("root"),
            &DevContainerConfig::default(),
        );
        // Falls back to the image USER rather than failing the build.
        assert_eq!(env.remote_user.as_deref(), Some("root"));
        assert_eq!(env.container_user.as_deref(), Some("root"));
    }
}
