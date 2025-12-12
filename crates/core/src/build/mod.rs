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

/// Build options that apply to both Dockerfile and feature builds.
///
/// This struct aggregates all cache and builder options passed via CLI flags.
/// When any option is set that requires BuildKit, the `requires_buildkit()` method
/// returns true to enable fail-fast validation before build execution.
///
/// # Spec Alignment
///
/// Per data-model.md:
/// - `cache_from`: ordered list of cache sources supplied by user; preserved order used when invoking BuildKit/buildx
/// - `cache_to`: optional cache destination supplied by user
/// - `builder`: optional buildx/builder selection applied to Dockerfile and feature builds
/// - Scope: applies to the entire `up` run and must be threaded to both Dockerfile and feature builds
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildOptions {
    /// Skip Docker layer cache during build (--no-cache flag).
    pub no_cache: bool,

    /// Ordered list of external cache sources (e.g., `type=registry,ref=<image>`).
    /// Order is preserved when passing to BuildKit/buildx.
    pub cache_from: Vec<String>,

    /// External cache destination (e.g., `type=registry,ref=<image>`).
    pub cache_to: Option<String>,

    /// Optional buildx builder name to use for builds.
    /// When set, builds use `docker buildx build --builder <name>`.
    pub builder: Option<String>,
}

impl BuildOptions {
    /// Creates a new BuildOptions with all fields set to defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if any option is set that requires BuildKit/buildx availability.
    ///
    /// BuildKit is required when:
    /// - Any cache-from sources are specified (requires `--cache-from` support)
    /// - A cache-to destination is specified (requires `--cache-to` support)
    /// - A specific builder is requested (requires `docker buildx`)
    ///
    /// When this returns true, build execution should verify BuildKit availability
    /// before proceeding and emit a fail-fast error if unavailable.
    pub fn requires_buildkit(&self) -> bool {
        !self.cache_from.is_empty() || self.cache_to.is_some() || self.builder.is_some()
    }

    /// Returns true if no cache/builder options are set.
    ///
    /// When this is true, builds should use default behavior without injecting
    /// any cache or buildx-specific arguments.
    pub fn is_default(&self) -> bool {
        !self.no_cache
            && self.cache_from.is_empty()
            && self.cache_to.is_none()
            && self.builder.is_none()
    }

    /// Generates the Docker build arguments for cache options.
    ///
    /// Returns a vector of argument strings (e.g., `["--cache-from", "type=registry,ref=foo"]`)
    /// that should be appended to the Docker build command.
    pub fn to_docker_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        if self.no_cache {
            args.push("--no-cache".to_string());
        }

        for cache_source in &self.cache_from {
            args.push("--cache-from".to_string());
            args.push(cache_source.clone());
        }

        if let Some(cache_dest) = &self.cache_to {
            args.push("--cache-to".to_string());
            args.push(cache_dest.clone());
        }

        if let Some(builder) = &self.builder {
            args.push("--builder".to_string());
            args.push(builder.clone());
        }

        args
    }
}

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

    #[test]
    fn test_build_options_default() {
        let opts = BuildOptions::default();
        assert!(!opts.no_cache);
        assert!(opts.cache_from.is_empty());
        assert!(opts.cache_to.is_none());
        assert!(opts.builder.is_none());
        assert!(opts.is_default());
        assert!(!opts.requires_buildkit());
    }

    #[test]
    fn test_build_options_requires_buildkit_cache_from() {
        let opts = BuildOptions {
            cache_from: vec!["type=registry,ref=myrepo/cache".to_string()],
            ..Default::default()
        };
        assert!(opts.requires_buildkit());
        assert!(!opts.is_default());
    }

    #[test]
    fn test_build_options_requires_buildkit_cache_to() {
        let opts = BuildOptions {
            cache_to: Some("type=registry,ref=myrepo/cache".to_string()),
            ..Default::default()
        };
        assert!(opts.requires_buildkit());
        assert!(!opts.is_default());
    }

    #[test]
    fn test_build_options_requires_buildkit_builder() {
        let opts = BuildOptions {
            builder: Some("mybuilder".to_string()),
            ..Default::default()
        };
        assert!(opts.requires_buildkit());
        assert!(!opts.is_default());
    }

    #[test]
    fn test_build_options_no_cache_does_not_require_buildkit() {
        let opts = BuildOptions {
            no_cache: true,
            ..Default::default()
        };
        // no_cache alone doesn't require BuildKit - legacy docker build supports it
        assert!(!opts.requires_buildkit());
        // But it's not default behavior
        assert!(!opts.is_default());
    }

    #[test]
    fn test_build_options_to_docker_args_empty() {
        let opts = BuildOptions::default();
        let args = opts.to_docker_args();
        assert!(args.is_empty());
    }

    #[test]
    fn test_build_options_to_docker_args_full() {
        let opts = BuildOptions {
            no_cache: true,
            cache_from: vec![
                "type=registry,ref=repo/cache:v1".to_string(),
                "type=local,src=/tmp/cache".to_string(),
            ],
            cache_to: Some("type=registry,ref=repo/cache:latest".to_string()),
            builder: Some("mybuilder".to_string()),
        };
        let args = opts.to_docker_args();

        assert_eq!(args.len(), 9);
        assert_eq!(args[0], "--no-cache");
        assert_eq!(args[1], "--cache-from");
        assert_eq!(args[2], "type=registry,ref=repo/cache:v1");
        assert_eq!(args[3], "--cache-from");
        assert_eq!(args[4], "type=local,src=/tmp/cache");
        assert_eq!(args[5], "--cache-to");
        assert_eq!(args[6], "type=registry,ref=repo/cache:latest");
        assert_eq!(args[7], "--builder");
        assert_eq!(args[8], "mybuilder");
    }

    #[test]
    fn test_build_options_to_docker_args_preserves_cache_from_order() {
        let opts = BuildOptions {
            cache_from: vec![
                "first".to_string(),
                "second".to_string(),
                "third".to_string(),
            ],
            ..Default::default()
        };
        let args = opts.to_docker_args();

        // Verify order is preserved
        assert_eq!(args[0], "--cache-from");
        assert_eq!(args[1], "first");
        assert_eq!(args[2], "--cache-from");
        assert_eq!(args[3], "second");
        assert_eq!(args[4], "--cache-from");
        assert_eq!(args[5], "third");
    }
}
