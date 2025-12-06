//! Dockerfile generation for feature installation using BuildKit
//!
//! This module generates Dockerfiles that install DevContainer features during
//! the image build phase using Docker BuildKit's mount capabilities. This approach
//! provides proper layer caching and follows the DevContainer specification.

use crate::build::BuildOptions;
use crate::errors::{FeatureError, Result};
use crate::features::{InstallationPlan, OptionValue, ResolvedFeature};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, instrument};

/// Build context name for feature content source
/// This name is used in both the Dockerfile generation and build arguments
const FEATURE_CONTENT_SOURCE: &str = "dev_containers_feature_content_source";

/// Configuration for Dockerfile generation
#[derive(Debug, Clone)]
pub struct DockerfileConfig {
    /// Base image to extend
    pub base_image: String,
    /// Target stage name
    pub target_stage: String,
    /// Directory where features are downloaded on the host
    pub features_source_dir: String,
}

impl Default for DockerfileConfig {
    fn default() -> Self {
        Self {
            base_image: String::new(),
            target_stage: "dev_containers_target_stage".to_string(),
            features_source_dir: String::new(),
        }
    }
}

/// Generates a Dockerfile for installing features using BuildKit
#[derive(Debug)]
pub struct DockerfileGenerator {
    config: DockerfileConfig,
}

impl DockerfileGenerator {
    /// Create a new Dockerfile generator
    pub fn new(config: DockerfileConfig) -> Self {
        Self { config }
    }

    /// Generate a complete Dockerfile for feature installation
    #[instrument(skip(self, plan))]
    pub fn generate(&self, plan: &InstallationPlan) -> Result<String> {
        debug!(
            "Generating Dockerfile for {} features across {} levels",
            plan.len(),
            plan.levels.len()
        );

        let mut dockerfile = String::new();

        // Build argument for base image
        dockerfile.push_str(&format!(
            "ARG _DEV_CONTAINERS_BASE_IMAGE={}\n\n",
            self.config.base_image
        ));

        // FROM stage
        dockerfile.push_str(&format!(
            "FROM ${{_DEV_CONTAINERS_BASE_IMAGE}} AS {}\n\n",
            self.config.target_stage
        ));

        // Create temporary directory for features
        dockerfile.push_str("RUN mkdir -p /tmp/dev-container-features\n\n");

        // Install features level by level
        for (level_idx, level) in plan.levels.iter().enumerate() {
            dockerfile.push_str(&format!("# Level {}: Installing features\n", level_idx));

            for feature_id in level {
                let feature =
                    plan.get_feature(feature_id)
                        .ok_or_else(|| FeatureError::NotFound {
                            path: format!("Feature {} in installation plan", feature_id),
                        })?;

                dockerfile.push_str(&self.generate_feature_install_command(feature, level_idx)?);
            }

            dockerfile.push('\n');
        }

        Ok(dockerfile)
    }

    /// Generate the RUN command for installing a single feature
    fn generate_feature_install_command(
        &self,
        feature: &ResolvedFeature,
        level_idx: usize,
    ) -> Result<String> {
        let sanitized_id = Self::sanitize_feature_id(&feature.id);
        let feature_dir_name = format!("{}_{}", sanitized_id, level_idx);
        let mount_target = format!("/tmp/build-features-{}/{}", level_idx, feature_dir_name);

        let mut command = String::new();

        // Start RUN command with BuildKit mount
        command.push_str(&format!(
            "RUN --mount=type=bind,from={},source={},target={},rw \\\n",
            FEATURE_CONTENT_SOURCE, feature_dir_name, mount_target
        ));

        // Add environment variables for feature options
        let env_vars = Self::build_environment_variables(feature);
        for (key, value) in env_vars {
            command.push_str(&format!("    {} \\\n", Self::format_env_var(&key, &value)));
        }

        // Execute the install script
        command.push_str(&format!(
            "    cd {} && chmod +x install.sh && ./install.sh\n\n",
            mount_target
        ));

        Ok(command)
    }

    /// Sanitize feature ID for use in file paths
    fn sanitize_feature_id(id: &str) -> String {
        // Replace special characters with underscores
        id.chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    /// Build environment variables from feature options
    fn build_environment_variables(feature: &ResolvedFeature) -> HashMap<String, String> {
        let mut env_vars = HashMap::new();

        for (key, value) in &feature.options {
            // Convert option key to uppercase as per DevContainer spec
            let env_key = key.to_uppercase();
            let env_value = Self::option_value_to_string(value);
            env_vars.insert(env_key, env_value);
        }

        env_vars
    }

    /// Convert OptionValue to string for environment variable
    fn option_value_to_string(value: &OptionValue) -> String {
        match value {
            OptionValue::Boolean(b) => b.to_string(),
            OptionValue::String(s) => s.clone(),
            OptionValue::Number(n) => n.to_string(),
            OptionValue::Array(a) => serde_json::to_string(a).unwrap_or_default(),
            OptionValue::Object(o) => serde_json::to_string(o).unwrap_or_default(),
            OptionValue::Null => String::new(),
        }
    }

    /// Format environment variable for Dockerfile
    fn format_env_var(key: &str, value: &str) -> String {
        // Escape special characters in value
        let escaped_value = value.replace('\\', "\\\\").replace('"', "\\\"");
        format!("{}=\"{}\"", key, escaped_value)
    }

    /// Generate build context arguments for docker buildx build command
    ///
    /// When `build_options` is provided and not default, cache arguments are included
    /// in the generated command. This enables cache-from/cache-to/no-cache/builder
    /// options to propagate to feature builds.
    ///
    /// Per spec (data-model.md):
    /// - `cache_from`: ordered list of cache sources, preserved when invoking BuildKit/buildx
    /// - `cache_to`: optional cache destination
    /// - `builder`: optional buildx builder selection
    /// - When `build_options.is_default()` returns true, no extra arguments are added
    pub fn generate_build_args(
        &self,
        dockerfile_path: &Path,
        image_tag: &str,
        build_options: Option<&BuildOptions>,
    ) -> Vec<String> {
        let mut args = vec![
            "buildx".to_string(),
            "build".to_string(),
            "--load".to_string(),
        ];

        // Add cache/builder arguments from BuildOptions if provided and not default
        if let Some(opts) = build_options {
            if !opts.is_default() {
                args.extend(opts.to_docker_args());
            }
        }

        // Add build context and other standard arguments
        args.extend(vec![
            "--build-context".to_string(),
            format!(
                "{}={}",
                FEATURE_CONTENT_SOURCE, self.config.features_source_dir
            ),
            "--build-arg".to_string(),
            format!("_DEV_CONTAINERS_BASE_IMAGE={}", self.config.base_image),
            "--target".to_string(),
            self.config.target_stage.clone(),
            "-f".to_string(),
            dockerfile_path.display().to_string(),
            "-t".to_string(),
            image_tag.to_string(),
            ".".to_string(),
        ]);

        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::{FeatureMetadata, ResolvedFeature};

    fn create_test_feature(id: &str, options: HashMap<String, OptionValue>) -> ResolvedFeature {
        ResolvedFeature {
            id: id.to_string(),
            source: "ghcr.io/devcontainers/features".to_string(),
            options,
            metadata: FeatureMetadata {
                id: id.to_string(),
                name: Some(format!("Test {}", id)),
                version: Some("1.0.0".to_string()),
                description: None,
                documentation_url: None,
                license_url: None,
                options: HashMap::new(),
                container_env: HashMap::new(),
                mounts: Vec::new(),
                entrypoint: None,
                privileged: None,
                init: None,
                cap_add: Vec::new(),
                security_opt: Vec::new(),
                depends_on: HashMap::new(),
                installs_after: Vec::new(),
                on_create_command: None,
                update_content_command: None,
                post_create_command: None,
                post_start_command: None,
                post_attach_command: None,
            },
        }
    }

    #[test]
    fn test_sanitize_feature_id() {
        assert_eq!(
            DockerfileGenerator::sanitize_feature_id("ghcr.io/devcontainers/features/node:1"),
            "ghcr_io_devcontainers_features_node_1"
        );
        assert_eq!(
            DockerfileGenerator::sanitize_feature_id("common-utils"),
            "common-utils"
        );
    }

    #[test]
    fn test_option_value_to_string() {
        assert_eq!(
            DockerfileGenerator::option_value_to_string(&OptionValue::Boolean(true)),
            "true"
        );
        assert_eq!(
            DockerfileGenerator::option_value_to_string(&OptionValue::String("test".to_string())),
            "test"
        );
        assert_eq!(
            DockerfileGenerator::option_value_to_string(&OptionValue::Number(
                serde_json::Number::from(42)
            )),
            "42"
        );
    }

    #[test]
    fn test_format_env_var() {
        assert_eq!(
            DockerfileGenerator::format_env_var("VERSION", "1.0"),
            "VERSION=\"1.0\""
        );
        assert_eq!(
            DockerfileGenerator::format_env_var("PATH", "/usr/bin:/bin"),
            "PATH=\"/usr/bin:/bin\""
        );
        // Test escaping
        assert_eq!(
            DockerfileGenerator::format_env_var("VAR", "value with \"quotes\""),
            "VAR=\"value with \\\"quotes\\\"\""
        );
    }

    #[test]
    fn test_generate_simple_dockerfile() {
        let mut options = HashMap::new();
        options.insert("version".to_string(), OptionValue::String("20".to_string()));

        let feature = create_test_feature("node", options);
        let plan = InstallationPlan::new(vec![feature]);

        let config = DockerfileConfig {
            base_image: "ubuntu:22.04".to_string(),
            target_stage: "dev_containers_target_stage".to_string(),
            features_source_dir: "/tmp/features".to_string(),
        };

        let generator = DockerfileGenerator::new(config);
        let dockerfile = generator.generate(&plan).unwrap();

        assert!(dockerfile.contains("ARG _DEV_CONTAINERS_BASE_IMAGE=ubuntu:22.04"));
        assert!(dockerfile
            .contains("FROM ${_DEV_CONTAINERS_BASE_IMAGE} AS dev_containers_target_stage"));
        assert!(dockerfile.contains("RUN mkdir -p /tmp/dev-container-features"));
        assert!(dockerfile.contains("RUN --mount=type=bind"));
        assert!(dockerfile.contains("VERSION=\"20\""));
        assert!(dockerfile.contains("./install.sh"));
    }

    #[test]
    fn test_generate_build_args() {
        let config = DockerfileConfig {
            base_image: "ubuntu:22.04".to_string(),
            target_stage: "dev_containers_target_stage".to_string(),
            features_source_dir: "/tmp/features".to_string(),
        };

        let generator = DockerfileGenerator::new(config);
        let args = generator.generate_build_args(
            Path::new("/tmp/Dockerfile.extended"),
            "test:latest",
            None,
        );

        assert!(args.contains(&"buildx".to_string()));
        assert!(args.contains(&"build".to_string()));
        assert!(args.contains(&"--load".to_string()));
        assert!(args.contains(&"--build-context".to_string()));
        assert!(args.contains(&"dev_containers_feature_content_source=/tmp/features".to_string()));
        assert!(args.contains(&"-t".to_string()));
        assert!(args.contains(&"test:latest".to_string()));
        // No cache arguments when build_options is None
        assert!(!args.contains(&"--cache-from".to_string()));
        assert!(!args.contains(&"--cache-to".to_string()));
    }

    #[test]
    fn test_generate_build_args_with_cache_options() {
        let config = DockerfileConfig {
            base_image: "ubuntu:22.04".to_string(),
            target_stage: "dev_containers_target_stage".to_string(),
            features_source_dir: "/tmp/features".to_string(),
        };

        let build_options = BuildOptions {
            no_cache: false,
            cache_from: vec![
                "type=registry,ref=myrepo/cache:v1".to_string(),
                "type=local,src=/tmp/cache".to_string(),
            ],
            cache_to: Some("type=registry,ref=myrepo/cache:latest".to_string()),
            builder: Some("mybuilder".to_string()),
        };

        let generator = DockerfileGenerator::new(config);
        let args = generator.generate_build_args(
            Path::new("/tmp/Dockerfile.extended"),
            "test:latest",
            Some(&build_options),
        );

        // Standard args still present
        assert!(args.contains(&"buildx".to_string()));
        assert!(args.contains(&"build".to_string()));
        assert!(args.contains(&"--load".to_string()));

        // Cache args from BuildOptions
        assert!(args.contains(&"--cache-from".to_string()));
        assert!(args.contains(&"type=registry,ref=myrepo/cache:v1".to_string()));
        assert!(args.contains(&"type=local,src=/tmp/cache".to_string()));
        assert!(args.contains(&"--cache-to".to_string()));
        assert!(args.contains(&"type=registry,ref=myrepo/cache:latest".to_string()));
        assert!(args.contains(&"--builder".to_string()));
        assert!(args.contains(&"mybuilder".to_string()));
    }

    #[test]
    fn test_generate_build_args_with_default_options() {
        let config = DockerfileConfig {
            base_image: "ubuntu:22.04".to_string(),
            target_stage: "dev_containers_target_stage".to_string(),
            features_source_dir: "/tmp/features".to_string(),
        };

        // Default options should not add any cache arguments
        let build_options = BuildOptions::default();
        assert!(build_options.is_default());

        let generator = DockerfileGenerator::new(config);
        let args = generator.generate_build_args(
            Path::new("/tmp/Dockerfile.extended"),
            "test:latest",
            Some(&build_options),
        );

        // Standard args present
        assert!(args.contains(&"buildx".to_string()));
        assert!(args.contains(&"build".to_string()));

        // No cache arguments when build_options is default
        assert!(!args.contains(&"--cache-from".to_string()));
        assert!(!args.contains(&"--cache-to".to_string()));
        assert!(!args.contains(&"--no-cache".to_string()));
        assert!(!args.contains(&"--builder".to_string()));
    }
}
