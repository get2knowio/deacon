//! Docker image building from Dockerfile configuration.
//!
//! This module contains:
//! - `BuildConfig` - Build configuration extracted from DevContainerConfig
//! - `extract_build_config_from_devcontainer` - Extract build config from devcontainer.json
//! - `build_image_from_config` - Build Docker image from build configuration

use crate::commands::shared::build_resolution::resolve_devcontainer_build_config;
use anyhow::{Context, Result};
use deacon_core::build::BuildOptions;
use deacon_core::config::DevContainerConfig;
use deacon_core::errors::{DeaconError, DockerError};
use std::collections::HashMap;
use std::path::PathBuf;
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
    config_path: &std::path::Path,
) -> Result<Option<BuildConfig>> {
    let resolved = match resolve_devcontainer_build_config(config, config_path)? {
        Some(resolved) => resolved,
        None => return Ok(None),
    };

    let dockerfile = resolved
        .dockerfile_path
        .to_str()
        .ok_or_else(|| {
            DeaconError::Docker(DockerError::CLIError("Invalid dockerfile path".to_string()))
        })?
        .to_string();

    Ok(Some(BuildConfig {
        dockerfile,
        context: resolved.context,
        context_folder: resolved.context_folder,
        target: resolved.target,
        options: resolved.options,
    }))
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

    // Execute docker build with retry-on-transient (network blips, 429,
    // 5xx from registry). Terminal failures (Dockerfile syntax, RUN failure,
    // 401/403) fail on the first attempt — see classifier in
    // deacon_core::docker_retry.
    let output = deacon_core::docker_retry::run_build_with_retry(
        std::path::Path::new("docker"),
        &build_args,
    )
    .await
    .context("docker build failed")?;

    let image_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    debug!("Built image with ID: {}", image_id);

    Ok(image_id)
}
