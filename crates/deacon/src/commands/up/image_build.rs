//! Docker image building from Dockerfile configuration.
//!
//! This module contains:
//! - `BuildConfig` - Build configuration extracted from DevContainerConfig
//! - `extract_build_config_from_devcontainer` - Extract build config from devcontainer.json
//! - `build_image_from_config` - Build Docker image from build configuration

use anyhow::{Context, Result};
use deacon_core::build::BuildOptions;
use deacon_core::config::DevContainerConfig;
use deacon_core::errors::{DeaconError, DockerError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, instrument};

/// Build configuration extracted from DevContainerConfig
#[derive(Debug, Clone)]
pub(crate) struct BuildConfig {
    pub dockerfile: String,
    pub context: String,
    pub context_folder: PathBuf,
    pub target: Option<String>,
    pub options: HashMap<String, String>,
}

/// Extract build configuration from DevContainerConfig.build object
pub(crate) fn extract_build_config_from_devcontainer(
    config: &DevContainerConfig,
    workspace_folder: &Path,
) -> Result<Option<BuildConfig>> {
    // If image is specified, no build needed
    if config.image.is_some() {
        return Ok(None);
    }

    // Determine config folder (where devcontainer.json is located)
    // Typically .devcontainer/ or workspace root
    let config_folder = workspace_folder.join(".devcontainer");
    let config_folder = if config_folder.exists() {
        config_folder
    } else {
        workspace_folder.to_path_buf()
    };

    // Check for build object with dockerfile field
    let build_value = match &config.build {
        Some(v) => v,
        None => {
            // Check for top-level dockerFile field
            if let Some(dockerfile) = &config.dockerfile {
                let dockerfile_path = config_folder.join(dockerfile);
                if !dockerfile_path.exists() {
                    return Err(
                        DeaconError::Config(deacon_core::errors::ConfigError::NotFound {
                            path: dockerfile_path.to_string_lossy().to_string(),
                        })
                        .into(),
                    );
                }

                return Ok(Some(BuildConfig {
                    dockerfile: dockerfile.clone(),
                    context: ".".to_string(),
                    context_folder: config_folder.clone(),
                    target: None,
                    options: HashMap::new(),
                }));
            }
            return Ok(None);
        }
    };

    let build_obj = build_value.as_object().ok_or_else(|| {
        DeaconError::Config(deacon_core::errors::ConfigError::Validation {
            message: "build field must be an object".to_string(),
        })
    })?;

    // Extract dockerfile from build object
    let dockerfile = build_obj
        .get("dockerfile")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: "build.dockerfile is required when using build object".to_string(),
            })
        })?;

    // Verify dockerfile exists - it's resolved relative to the config folder (where devcontainer.json is)
    // The context is used at build time as the Docker build context, but the Dockerfile path
    // itself is relative to the devcontainer.json location
    let dockerfile_path = config_folder.join(dockerfile);
    if !dockerfile_path.exists() {
        return Err(
            DeaconError::Config(deacon_core::errors::ConfigError::NotFound {
                path: dockerfile_path.to_string_lossy().to_string(),
            })
            .into(),
        );
    }

    let mut build_config = BuildConfig {
        dockerfile: dockerfile_path
            .to_str()
            .ok_or_else(|| {
                DeaconError::Docker(DockerError::CLIError("Invalid dockerfile path".to_string()))
            })?
            .to_string(),
        context: ".".to_string(),
        context_folder: config_folder.clone(),
        target: None,
        options: HashMap::new(),
    };

    // Extract context
    if let Some(context) = build_obj.get("context").and_then(|v| v.as_str()) {
        build_config.context = context.to_string();
    }

    // Extract target
    if let Some(target) = build_obj.get("target").and_then(|v| v.as_str()) {
        build_config.target = Some(target.to_string());
    }

    // Extract build options/args
    if let Some(options) = build_obj.get("options").and_then(|v| v.as_object()) {
        for (key, value) in options {
            let val_str = value
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| value.to_string());
            build_config.options.insert(key.clone(), val_str);
        }
    }

    // Extract build args (upstream-compatible: build.args)
    if let Some(args_obj) = build_obj.get("args").and_then(|v| v.as_object()) {
        for (key, value) in args_obj {
            let val_str = value
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| value.to_string());
            build_config.options.insert(key.clone(), val_str);
        }
    }

    Ok(Some(build_config))
}

/// Build Docker image from build configuration
///
/// # Arguments
///
/// * `build_config` - Build configuration extracted from devcontainer.json
/// * `build_options` - Build options containing cache and buildx settings
///
/// When `build_options.is_default()` is true, no extra arguments are added,
/// preserving existing behavior. When cache-from/cache-to/builder options
/// are set, they are passed to the docker build command via `to_docker_args()`.
#[instrument(skip(build_config, build_options))]
pub(crate) async fn build_image_from_config(
    build_config: &BuildConfig,
    build_options: &BuildOptions,
) -> Result<String> {
    debug!(
        "Building image from Dockerfile: {}",
        build_config.dockerfile
    );

    // Log cache configuration before build starts (per research.md Decision 2).
    // Docker/BuildKit handles cache failures gracefully; we inform users of the configuration.
    if !build_options.cache_from.is_empty() {
        info!(
            cache_from = ?build_options.cache_from,
            "Using cache source(s) for build"
        );
    }
    if let Some(cache_to) = &build_options.cache_to {
        info!(
            cache_to = %cache_to,
            "Exporting build cache to destination"
        );
    }

    // Resolve context path relative to the directory containing devcontainer.json
    // This handles ".." and other relative paths correctly
    let context_path = build_config
        .context_folder
        .join(&build_config.context)
        .canonicalize()
        .context("Failed to resolve build context path")?;

    // Prepare docker build arguments
    let mut build_args = vec!["build".to_string()];

    // Add dockerfile (already a full path from extract_build_config_from_devcontainer)
    build_args.push("-f".to_string());
    build_args.push(build_config.dockerfile.clone());

    // Add build options (no-cache, cache-from, cache-to, builder) if any are set
    // When is_default() is true, to_docker_args() returns an empty vec, preserving existing behavior
    let cache_args = build_options.to_docker_args();
    if !cache_args.is_empty() {
        debug!("Adding build options: {:?}", cache_args);
        build_args.extend(cache_args);
    }

    // Add target
    if let Some(target) = &build_config.target {
        build_args.push("--target".to_string());
        build_args.push(target.clone());
    }

    // Add build args from config
    for (key, value) in &build_config.options {
        build_args.push("--build-arg".to_string());
        build_args.push(format!("{}={}", key, value));
    }

    // Add quiet flag to reduce output noise and get just the image ID
    build_args.push("-q".to_string());

    // Finally add build context (must be last)
    build_args.push(
        context_path
            .to_str()
            .ok_or_else(|| {
                DeaconError::Docker(DockerError::CLIError("Invalid context path".to_string()))
            })?
            .to_string(),
    );

    debug!("Docker build command: docker {}", build_args.join(" "));

    // Execute docker build
    let output = tokio::process::Command::new("docker")
        .args(&build_args)
        .output()
        .await
        .context("Failed to execute docker build")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DeaconError::Docker(DockerError::CLIError(format!(
            "Docker build failed: {}",
            stderr
        )))
        .into());
    }

    let image_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    debug!("Built image with ID: {}", image_id);

    Ok(image_id)
}
