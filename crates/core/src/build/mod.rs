//! Build domain types and logic
//!
//! This module contains the domain model for dev container builds, including
//! request aggregation, artifact representation, feature manifests, and validation events.
//!
//! ## Design Principles
//!
//! - All domain types are immutable after construction to ensure deterministic builds
//! - Validation rules are enforced at construction time, not during execution
//! - BuildKit-only features are explicitly flagged to enable fail-fast behavior
//! - Metadata labels are structured and serializable for downstream tooling

use crate::errors::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub mod buildkit;
pub mod metadata;

/// Aggregates all inputs required to execute `deacon build`.
///
/// This struct represents a validated build request with all CLI arguments,
/// configuration overrides, and feature specifications. Once constructed,
/// all fields are immutable to ensure deterministic build behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildRequest {
    /// Normalized absolute workspace path
    pub workspace_folder: PathBuf,

    /// Explicit devcontainer configuration file (must be devcontainer.json or .devcontainer.json)
    pub config_file: Option<PathBuf>,

    /// Ordered list of tags derived from `--image-name`
    pub image_names: Vec<String>,

    /// Indicates registry push requested
    pub push: bool,

    /// BuildKit export specification (mutually exclusive with push)
    pub output: Option<String>,

    /// Ordered user-provided metadata labels
    pub labels: Vec<(String, String)>,

    /// Merged feature overrides from CLI `--additional-features`
    pub additional_features: serde_json::Value,

    /// BuildKit mode selection (auto/enable/disable)
    pub buildkit_mode: BuildKitMode,

    /// Multi-arch target platform
    pub platform: Option<String>,

    /// Cache sources for BuildKit
    pub cache_from: Vec<String>,

    /// Cache destination for BuildKit
    pub cache_to: Option<String>,

    /// Skip automatic feature-to-service mapping
    pub skip_feature_auto_mapping: bool,

    /// Skip persisting customizations in metadata
    pub skip_persist_customizations: bool,

    /// Enable experimental lockfile generation
    pub experimental_lockfile: bool,

    /// Require lockfile match (fail if lockfile doesn't match)
    pub experimental_frozen_lockfile: bool,

    /// Omit syntax directive from generated Dockerfile
    pub omit_syntax_directive: bool,
}

impl BuildRequest {
    /// Validates the build request according to specification rules.
    ///
    /// # Validation Rules
    ///
    /// - Config filename must be `devcontainer.json` or `.devcontainer.json` when provided
    /// - `push` and `output` are mutually exclusive
    /// - BuildKit-only flags require BuildKit availability (checked separately)
    /// - All image names must be valid image references
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails with a descriptive message.
    pub fn validate(&self) -> Result<()> {
        // Validate config file name if provided
        if let Some(config_file) = &self.config_file {
            let filename = config_file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            if filename != "devcontainer.json" && filename != ".devcontainer.json" {
                return Err(crate::errors::DeaconError::Runtime(format!(
                    "Configuration file must be named 'devcontainer.json' or '.devcontainer.json', got '{}'",
                    filename
                )));
            }
        }

        // Validate push/output mutual exclusivity
        if self.push && self.output.is_some() {
            return Err(crate::errors::DeaconError::Runtime(
                "Cannot use both --push and --output; they are mutually exclusive".to_string(),
            ));
        }

        Ok(())
    }
}

/// BuildKit mode selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BuildKitMode {
    /// Auto-detect BuildKit availability
    #[default]
    Auto,
    /// Force BuildKit usage
    Enable,
    /// Disable BuildKit
    Disable,
}

/// Represents the outputs produced by a build.
///
/// This struct captures all artifacts generated during the build process,
/// including tags, metadata labels, and export paths.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageArtifact {
    /// Deterministic fallback plus user-specified tags
    pub tags: Vec<String>,

    /// Serialized devcontainer metadata JSON (stored in image label)
    pub metadata_label: String,

    /// Labels supplied by `--label`
    pub user_labels: Vec<(String, String)>,

    /// Path to exported artifact when `--output` is used
    pub export_path: Option<PathBuf>,

    /// Indicates whether image was pushed to registry
    pub pushed: bool,
}

impl ImageArtifact {
    /// Creates a new image artifact with validation.
    ///
    /// # Arguments
    ///
    /// * `tags` - List of image tags (must be valid image references)
    /// * `metadata_label` - Serialized JSON metadata
    /// * `user_labels` - User-provided labels
    /// * `export_path` - Optional export path
    /// * `pushed` - Whether image was pushed
    ///
    /// # Errors
    ///
    /// Returns an error if metadata label is not valid UTF-8 JSON.
    pub fn new(
        tags: Vec<String>,
        metadata_label: String,
        user_labels: Vec<(String, String)>,
        export_path: Option<PathBuf>,
        pushed: bool,
    ) -> Result<Self> {
        // Validate metadata label is valid JSON
        serde_json::from_str::<serde_json::Value>(&metadata_label).map_err(|e| {
            crate::errors::DeaconError::Runtime(format!("Metadata label is not valid JSON: {}", e))
        })?;

        Ok(Self {
            tags,
            metadata_label,
            user_labels,
            export_path,
            pushed,
        })
    }
}

/// Canonical record of features applied during build.
///
/// This struct captures the resolved feature set, customizations, and
/// BuildKit-specific requirements needed for feature installation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureManifest {
    /// Resolved features in execution order
    pub install_order: Vec<FeatureRef>,

    /// Persisted customizations (optional when skip flag set)
    pub customizations: Option<HashMap<String, serde_json::Value>>,

    /// BuildKit contexts required for features
    pub build_contexts: Vec<FeatureBuildContext>,

    /// Container security options appended during build
    pub security_opts: Vec<String>,

    /// Generated or validated lockfile payloads
    pub lockfile_state: Option<FeatureLockfile>,
}

/// Reference to a feature with its configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureRef {
    /// Feature identifier (e.g., "ghcr.io/devcontainers/features/node")
    pub id: String,

    /// Feature version (e.g., "1.2.3")
    pub version: Option<String>,

    /// Feature options
    pub options: HashMap<String, serde_json::Value>,
}

/// BuildKit build context for a feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureBuildContext {
    /// Context name
    pub name: String,

    /// Context source path or URL
    pub source: String,
}

/// Feature lockfile state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureLockfile {
    /// Locked features with resolved versions
    pub features: Vec<FeatureRef>,

    /// Lockfile content hash
    pub hash: String,
}

/// Captures CLI and configuration validation outcomes.
///
/// Validation events are accumulated during preflight checks. Any error
/// event transitions execution into the failure path with spec-compliant
/// JSON error output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationEvent {
    /// Validation code identifying the rule breached
    pub code: ValidationCode,

    /// Human-readable error message matching spec
    pub message: String,

    /// Supplemental context for JSON error payloads
    pub description: Option<String>,

    /// Validation category
    pub category: ValidationCategory,
}

/// Validation error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationCode {
    /// Invalid input argument
    InvalidInput,

    /// BuildKit requirement not met
    BuildKitRequired,

    /// Compose configuration error
    ComposeError,

    /// Feature resolution error
    FeatureError,

    /// Runtime execution error
    RuntimeError,

    /// Mutual exclusivity violation
    MutualExclusivity,
}

/// Validation category for grouping related errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationCategory {
    /// CLI input validation
    Input,

    /// BuildKit capability validation
    BuildKit,

    /// Compose configuration validation
    Compose,

    /// Feature resolution validation
    Feature,

    /// Runtime execution validation
    Runtime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_request_validate_config_filename() {
        let mut request = BuildRequest {
            workspace_folder: PathBuf::from("/workspace"),
            config_file: Some(PathBuf::from("/workspace/.devcontainer/devcontainer.json")),
            image_names: vec![],
            push: false,
            output: None,
            labels: vec![],
            additional_features: serde_json::Value::Object(Default::default()),
            buildkit_mode: BuildKitMode::Auto,
            platform: None,
            cache_from: vec![],
            cache_to: None,
            skip_feature_auto_mapping: false,
            skip_persist_customizations: false,
            experimental_lockfile: false,
            experimental_frozen_lockfile: false,
            omit_syntax_directive: false,
        };

        // Valid config file name
        assert!(request.validate().is_ok());

        // Invalid config file name
        request.config_file = Some(PathBuf::from("/workspace/.devcontainer/config.json"));
        assert!(request.validate().is_err());
    }

    #[test]
    fn test_build_request_validate_push_output_exclusive() {
        let request = BuildRequest {
            workspace_folder: PathBuf::from("/workspace"),
            config_file: None,
            image_names: vec![],
            push: true,
            output: Some("type=docker,dest=output.tar".to_string()),
            labels: vec![],
            additional_features: serde_json::Value::Object(Default::default()),
            buildkit_mode: BuildKitMode::Auto,
            platform: None,
            cache_from: vec![],
            cache_to: None,
            skip_feature_auto_mapping: false,
            skip_persist_customizations: false,
            experimental_lockfile: false,
            experimental_frozen_lockfile: false,
            omit_syntax_directive: false,
        };

        // Both push and output set - should fail
        assert!(request.validate().is_err());
    }

    #[test]
    fn test_image_artifact_new_validates_metadata() {
        // Valid metadata
        let result = ImageArtifact::new(
            vec!["test:latest".to_string()],
            r#"{"config": "test"}"#.to_string(),
            vec![],
            None,
            false,
        );
        assert!(result.is_ok());

        // Invalid metadata (not JSON)
        let result = ImageArtifact::new(
            vec!["test:latest".to_string()],
            "not-json".to_string(),
            vec![],
            None,
            false,
        );
        assert!(result.is_err());
    }
}
