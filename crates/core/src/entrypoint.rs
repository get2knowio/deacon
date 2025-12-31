//! Entrypoint merge semantics for compose-based devcontainers with features
//!
//! This module implements deterministic entrypoint merge logic that handles:
//! - Explicit compose service entrypoints (highest precedence)
//! - Feature-provided entrypoint augmentations (wrapping)
//! - Base image/Dockerfile entrypoints (lowest precedence)

use crate::features::FeatureMetadata;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, instrument};

/// Strategy for merging entrypoints when conflicts occur
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EntrypointMergeStrategy {
    /// Wrap the original entrypoint with feature hooks (default)
    #[default]
    Wrap,
    /// Ignore feature entrypoints, use only compose/image entrypoint
    Ignore,
    /// Replace with feature entrypoint completely
    Replace,
}

impl std::fmt::Display for EntrypointMergeStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wrap => write!(f, "wrap"),
            Self::Ignore => write!(f, "ignore"),
            Self::Replace => write!(f, "replace"),
        }
    }
}

impl std::str::FromStr for EntrypointMergeStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "wrap" => Ok(Self::Wrap),
            "ignore" => Ok(Self::Ignore),
            "replace" => Ok(Self::Replace),
            _ => Err(format!(
                "Invalid entrypoint merge strategy '{}'. Valid options: wrap, ignore, replace",
                s
            )),
        }
    }
}

/// Sources of entrypoints in order of precedence
#[derive(Debug, Clone, PartialEq)]
pub enum EntrypointSource {
    /// Explicit compose service entrypoint (highest precedence)
    ComposeService(String),
    /// Feature-provided entrypoint
    Feature {
        feature_id: String,
        entrypoint: String,
    },
    /// Base image or Dockerfile entrypoint (lowest precedence)
    BaseImage(Option<String>),
}

/// Result of entrypoint merging
#[derive(Debug, Clone, PartialEq)]
pub struct MergedEntrypoint {
    /// The final entrypoint to use
    pub entrypoint: Option<String>,
    /// Path to wrapper script if generated (for wrap strategy)
    pub wrapper_script_path: Option<PathBuf>,
    /// Description of the merge decision
    pub description: String,
}

/// Entrypoint merger that computes merged entrypoints
pub struct EntrypointMerger;

impl EntrypointMerger {
    /// Merge entrypoints from various sources with the given strategy
    ///
    /// ## Precedence Order
    /// 1. Explicit compose service entrypoint (highest)
    /// 2. Feature-provided entrypoints (middle, can wrap or replace)
    /// 3. Base image/Dockerfile entrypoint (lowest)
    ///
    /// ## Arguments
    /// * `compose_entrypoint` - Entrypoint from compose service definition
    /// * `features` - List of features with their metadata
    /// * `base_entrypoint` - Entrypoint from base image or Dockerfile
    /// * `strategy` - Merge strategy to use
    ///
    /// ## Returns
    /// Returns the merged entrypoint result with optional wrapper script path
    #[instrument(level = "debug", skip(features))]
    pub fn merge_entrypoints(
        compose_entrypoint: Option<&str>,
        features: &[&FeatureMetadata],
        base_entrypoint: Option<&str>,
        strategy: EntrypointMergeStrategy,
    ) -> MergedEntrypoint {
        debug!(
            "Merging entrypoints with strategy: {}, compose={:?}, features_count={}, base={:?}",
            strategy,
            compose_entrypoint,
            features.len(),
            base_entrypoint
        );

        // Explicit compose entrypoint always takes precedence
        if let Some(compose_ep) = compose_entrypoint {
            debug!("Using explicit compose service entrypoint (highest precedence)");
            return MergedEntrypoint {
                entrypoint: Some(compose_ep.to_string()),
                wrapper_script_path: None,
                description: "Compose service entrypoint (explicit)".to_string(),
            };
        }

        // Collect feature entrypoints
        let feature_entrypoints: Vec<_> = features
            .iter()
            .filter_map(|f| {
                f.entrypoint.as_ref().map(|ep| EntrypointSource::Feature {
                    feature_id: f.id.clone(),
                    entrypoint: ep.clone(),
                })
            })
            .collect();

        if feature_entrypoints.is_empty() {
            // No feature entrypoints, use base
            debug!("No feature entrypoints, using base entrypoint");
            return MergedEntrypoint {
                entrypoint: base_entrypoint.map(|s| s.to_string()),
                wrapper_script_path: None,
                description: "Base image/Dockerfile entrypoint".to_string(),
            };
        }

        // Apply strategy when feature entrypoints are present
        match strategy {
            EntrypointMergeStrategy::Ignore => {
                debug!("Ignoring feature entrypoints per strategy");
                MergedEntrypoint {
                    entrypoint: base_entrypoint.map(|s| s.to_string()),
                    wrapper_script_path: None,
                    description: format!(
                        "Base entrypoint (ignored {} feature entrypoint(s))",
                        feature_entrypoints.len()
                    ),
                }
            }
            EntrypointMergeStrategy::Replace => {
                // Use the last feature entrypoint (order matters)
                if let Some(EntrypointSource::Feature {
                    feature_id,
                    entrypoint,
                }) = feature_entrypoints.last()
                {
                    debug!("Replacing with feature '{}' entrypoint", feature_id);
                    MergedEntrypoint {
                        entrypoint: Some(entrypoint.clone()),
                        wrapper_script_path: None,
                        description: format!("Feature '{}' entrypoint (replaced)", feature_id),
                    }
                } else {
                    unreachable!("feature_entrypoints is not empty")
                }
            }
            EntrypointMergeStrategy::Wrap => {
                // Generate wrapper that invokes feature hooks then original
                debug!(
                    "Generating wrapper for {} feature entrypoint(s)",
                    feature_entrypoints.len()
                );
                Self::generate_wrapper(
                    &feature_entrypoints,
                    base_entrypoint,
                    base_entrypoint.is_some(),
                )
            }
        }
    }

    /// Generate a wrapper script that invokes feature entrypoints then the original
    fn generate_wrapper(
        feature_entrypoints: &[EntrypointSource],
        _base_entrypoint: Option<&str>,
        has_original: bool,
    ) -> MergedEntrypoint {
        // For now, we'll generate a conceptual wrapper
        // In a full implementation, this would write a shell script to a temp location
        let wrapper_path = PathBuf::from("/tmp/devcontainer-entrypoint-wrapper.sh");

        let feature_ids: Vec<String> = feature_entrypoints
            .iter()
            .filter_map(|src| {
                if let EntrypointSource::Feature { feature_id, .. } = src {
                    Some(feature_id.clone())
                } else {
                    None
                }
            })
            .collect();

        let description = if has_original {
            format!(
                "Wrapper: {} feature(s) [{}] + base entrypoint",
                feature_ids.len(),
                feature_ids.join(", ")
            )
        } else {
            format!(
                "Wrapper: {} feature(s) [{}]",
                feature_ids.len(),
                feature_ids.join(", ")
            )
        };

        debug!("Wrapper description: {}", description);

        MergedEntrypoint {
            entrypoint: Some(wrapper_path.display().to_string()),
            wrapper_script_path: Some(wrapper_path),
            description,
        }
    }

    /// Generate the actual wrapper script content
    ///
    /// This creates a shell script that:
    /// 1. Runs feature entrypoints in order
    /// 2. Invokes the original entrypoint if present
    /// 3. Passes through all arguments
    pub fn generate_wrapper_script(
        feature_entrypoints: &[&FeatureMetadata],
        base_entrypoint: Option<&str>,
    ) -> String {
        let mut script = String::from("#!/bin/sh\n");
        script.push_str("# DevContainer entrypoint wrapper\n");
        script.push_str("# Generated by deacon entrypoint merger\n\n");
        script.push_str("set -e\n\n");

        // Execute feature entrypoints in order
        for feature in feature_entrypoints {
            if let Some(ref ep) = feature.entrypoint {
                script.push_str(&format!(
                    "# Feature: {}\n",
                    feature.name.as_deref().unwrap_or(&feature.id)
                ));
                script.push_str(&format!(
                    "echo \"Running feature '{}' entrypoint...\"\n",
                    feature.id
                ));
                script.push_str(&format!("{}\n\n", ep));
            }
        }

        // Invoke original entrypoint if present
        if let Some(original) = base_entrypoint {
            script.push_str("# Original entrypoint\n");
            script.push_str("echo \"Invoking original entrypoint...\"\n");
            script.push_str(&format!("exec {} \"$@\"\n", original));
        } else {
            script.push_str("# No original entrypoint to invoke\n");
            script.push_str("exec \"$@\"\n");
        }

        script
    }

    /// Validate entrypoint merge for conflicting directives
    ///
    /// Returns an error message if there are conflicts that cannot be safely merged
    pub fn validate_merge(
        compose_entrypoint: Option<&str>,
        features: &[&FeatureMetadata],
        _base_entrypoint: Option<&str>,
    ) -> Result<(), String> {
        // If compose entrypoint is set, no validation needed (it wins)
        if compose_entrypoint.is_some() {
            return Ok(());
        }

        // Check if multiple features have conflicting entrypoints
        let feature_entrypoints: Vec<_> = features
            .iter()
            .filter_map(|f| f.entrypoint.as_ref().map(|ep| (&f.id, ep)))
            .collect();

        if feature_entrypoints.len() > 1 {
            // Multiple features want to set entrypoints - this is a potential conflict
            // We allow this but user should be aware
            debug!(
                "Multiple features define entrypoints: {:?}",
                feature_entrypoints
                    .iter()
                    .map(|(id, _)| id)
                    .collect::<Vec<_>>()
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_feature(id: &str, entrypoint: Option<String>) -> FeatureMetadata {
        FeatureMetadata {
            id: id.to_string(),
            version: Some("1.0.0".to_string()),
            name: Some(format!("Test Feature {}", id)),
            description: None,
            documentation_url: None,
            license_url: None,
            options: HashMap::new(),
            container_env: HashMap::new(),
            mounts: vec![],
            init: None,
            privileged: None,
            cap_add: vec![],
            security_opt: vec![],
            entrypoint,
            installs_after: vec![],
            depends_on: HashMap::new(),
            on_create_command: None,
            update_content_command: None,
            post_create_command: None,
            post_start_command: None,
            post_attach_command: None,
        }
    }

    #[test]
    fn test_compose_entrypoint_takes_precedence() {
        let feature =
            create_test_feature("test-feature", Some("/feature-entrypoint.sh".to_string()));
        let features = vec![&feature];

        let result = EntrypointMerger::merge_entrypoints(
            Some("/compose-entrypoint.sh"),
            &features,
            Some("/base-entrypoint.sh"),
            EntrypointMergeStrategy::Wrap,
        );

        assert_eq!(
            result.entrypoint,
            Some("/compose-entrypoint.sh".to_string())
        );
        assert_eq!(result.wrapper_script_path, None);
        assert!(result.description.contains("Compose service"));
    }

    #[test]
    fn test_no_feature_entrypoints_uses_base() {
        let feature = create_test_feature("test-feature", None);
        let features = vec![&feature];

        let result = EntrypointMerger::merge_entrypoints(
            None,
            &features,
            Some("/base-entrypoint.sh"),
            EntrypointMergeStrategy::Wrap,
        );

        assert_eq!(result.entrypoint, Some("/base-entrypoint.sh".to_string()));
        assert_eq!(result.wrapper_script_path, None);
        assert!(result.description.contains("Base"));
    }

    #[test]
    fn test_ignore_strategy() {
        let feature =
            create_test_feature("test-feature", Some("/feature-entrypoint.sh".to_string()));
        let features = vec![&feature];

        let result = EntrypointMerger::merge_entrypoints(
            None,
            &features,
            Some("/base-entrypoint.sh"),
            EntrypointMergeStrategy::Ignore,
        );

        assert_eq!(result.entrypoint, Some("/base-entrypoint.sh".to_string()));
        assert_eq!(result.wrapper_script_path, None);
        assert!(result.description.contains("ignored"));
    }

    #[test]
    fn test_replace_strategy() {
        let feature =
            create_test_feature("test-feature", Some("/feature-entrypoint.sh".to_string()));
        let features = vec![&feature];

        let result = EntrypointMerger::merge_entrypoints(
            None,
            &features,
            Some("/base-entrypoint.sh"),
            EntrypointMergeStrategy::Replace,
        );

        assert_eq!(
            result.entrypoint,
            Some("/feature-entrypoint.sh".to_string())
        );
        assert_eq!(result.wrapper_script_path, None);
        assert!(result.description.contains("replaced"));
    }

    #[test]
    fn test_wrap_strategy_single_feature() {
        let feature =
            create_test_feature("test-feature", Some("/feature-entrypoint.sh".to_string()));
        let features = vec![&feature];

        let result = EntrypointMerger::merge_entrypoints(
            None,
            &features,
            Some("/base-entrypoint.sh"),
            EntrypointMergeStrategy::Wrap,
        );

        assert!(result.entrypoint.is_some());
        assert!(result.wrapper_script_path.is_some());
        assert!(result.description.contains("Wrapper"));
        assert!(result.description.contains("test-feature"));
    }

    #[test]
    fn test_wrap_strategy_multiple_features() {
        let feature1 = create_test_feature("feature-1", Some("/entrypoint-1.sh".to_string()));
        let feature2 = create_test_feature("feature-2", Some("/entrypoint-2.sh".to_string()));
        let features = vec![&feature1, &feature2];

        let result = EntrypointMerger::merge_entrypoints(
            None,
            &features,
            Some("/base-entrypoint.sh"),
            EntrypointMergeStrategy::Wrap,
        );

        assert!(result.entrypoint.is_some());
        assert!(result.wrapper_script_path.is_some());
        assert!(result.description.contains("Wrapper"));
        assert!(result.description.contains("feature-1"));
        assert!(result.description.contains("feature-2"));
    }

    #[test]
    fn test_wrap_strategy_no_base_entrypoint() {
        let feature =
            create_test_feature("test-feature", Some("/feature-entrypoint.sh".to_string()));
        let features = vec![&feature];

        let result = EntrypointMerger::merge_entrypoints(
            None,
            &features,
            None,
            EntrypointMergeStrategy::Wrap,
        );

        assert!(result.entrypoint.is_some());
        assert!(result.wrapper_script_path.is_some());
        assert!(result.description.contains("Wrapper"));
        assert!(!result.description.contains("base"));
    }

    #[test]
    fn test_generate_wrapper_script() {
        let feature1 = create_test_feature("feature-1", Some("/entrypoint-1.sh".to_string()));
        let feature2 = create_test_feature("feature-2", Some("/entrypoint-2.sh".to_string()));
        let features = vec![&feature1, &feature2];

        let script = EntrypointMerger::generate_wrapper_script(&features, Some("/bin/bash"));

        assert!(script.contains("#!/bin/sh"));
        assert!(script.contains("feature-1"));
        assert!(script.contains("feature-2"));
        assert!(script.contains("/entrypoint-1.sh"));
        assert!(script.contains("/entrypoint-2.sh"));
        assert!(script.contains("/bin/bash"));
        assert!(script.contains("exec"));
    }

    #[test]
    fn test_generate_wrapper_script_no_base() {
        let feature =
            create_test_feature("test-feature", Some("/feature-entrypoint.sh".to_string()));
        let features = vec![&feature];

        let script = EntrypointMerger::generate_wrapper_script(&features, None);

        assert!(script.contains("#!/bin/sh"));
        assert!(script.contains("test-feature"));
        assert!(script.contains("/feature-entrypoint.sh"));
        assert!(script.contains("exec \"$@\""));
        assert!(!script.contains("Original entrypoint"));
    }

    #[test]
    fn test_strategy_from_str() {
        assert_eq!(
            "wrap".parse::<EntrypointMergeStrategy>().unwrap(),
            EntrypointMergeStrategy::Wrap
        );
        assert_eq!(
            "ignore".parse::<EntrypointMergeStrategy>().unwrap(),
            EntrypointMergeStrategy::Ignore
        );
        assert_eq!(
            "replace".parse::<EntrypointMergeStrategy>().unwrap(),
            EntrypointMergeStrategy::Replace
        );
        assert_eq!(
            "WRAP".parse::<EntrypointMergeStrategy>().unwrap(),
            EntrypointMergeStrategy::Wrap
        );
        assert!("invalid".parse::<EntrypointMergeStrategy>().is_err());
    }

    #[test]
    fn test_validate_merge_no_conflicts() {
        let feature = create_test_feature("test-feature", None);
        let features = vec![&feature];

        let result = EntrypointMerger::validate_merge(None, &features, Some("/base-entrypoint.sh"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_merge_compose_always_ok() {
        let feature =
            create_test_feature("test-feature", Some("/feature-entrypoint.sh".to_string()));
        let features = vec![&feature];

        let result =
            EntrypointMerger::validate_merge(Some("/compose.sh"), &features, Some("/base.sh"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_merge_multiple_features() {
        let feature1 = create_test_feature("feature-1", Some("/entrypoint-1.sh".to_string()));
        let feature2 = create_test_feature("feature-2", Some("/entrypoint-2.sh".to_string()));
        let features = vec![&feature1, &feature2];

        // This is allowed, but noted in logs
        let result = EntrypointMerger::validate_merge(None, &features, Some("/base.sh"));
        assert!(result.is_ok());
    }
}
