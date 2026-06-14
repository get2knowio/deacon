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
#[instrument(skip(build_config, build_options, cli))]
pub(crate) async fn build_image_from_config(
    build_config: &BuildConfig,
    build_options: &BuildOptions,
    cli: &deacon_core::docker::CliRuntime,
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

    let runtime_path = cli.runtime_path();
    debug!("Build command: {} {}", runtime_path, build_args.join(" "));

    // Execute the build with retry-on-transient (network blips, 429,
    // 5xx from registry). Terminal failures (Dockerfile syntax, RUN failure,
    // 401/403) fail on the first attempt — see classifier in
    // deacon_core::docker_retry.
    let output = deacon_core::docker_retry::run_build_with_retry(
        std::path::Path::new(runtime_path),
        &build_args,
    )
    .await
    .context("image build failed")?;

    let image_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    debug!("Built image with ID: {}", image_id);

    // `docker build -q` returns a bare `sha256:<digest>` image ID. Returning
    // that as the resolved `config.image` works for `docker create`, but when
    // features are layered on top the generated `FROM ${base}` makes BuildKit
    // treat a bare digest as a `docker.io/library/sha256:...` repository
    // (pull access denied / 404). Tag the freshly-built image with a real,
    // deterministic `repo:tag` so any downstream `FROM` resolves to the local
    // image — mirroring `deacon build`'s `deacon-build:<hash>` base tag. The
    // digest is content-addressed, so the derived tag is stable across rebuilds.
    let tag = derive_local_build_tag(&image_id);
    tag_built_image(runtime_path, &image_id, &tag).await?;
    debug!("Tagged built image {} as {}", image_id, tag);

    Ok(tag)
}

/// Derive a deterministic, BuildKit-`FROM`-safe `repo:tag` from a bare image
/// digest. `docker build -q` yields `sha256:<64hex>`; a bare digest used as a
/// `FROM` is resolved as a remote `docker.io/library/...` repo (404), so reuse
/// the digest's leading hex (content-addressed → stable) as the tag suffix.
fn derive_local_build_tag(image_id: &str) -> String {
    let stripped = image_id.strip_prefix("sha256:").unwrap_or(image_id);
    let short = &stripped[..stripped.len().min(12)];
    format!("deacon-build:{}", short)
}

/// Apply `tag` to the locally-built image `image_id` (`<runtime> tag`).
async fn tag_built_image(runtime_path: &str, image_id: &str, tag: &str) -> Result<()> {
    let output = tokio::process::Command::new(runtime_path)
        .args(["tag", image_id, tag])
        .output()
        .await
        .with_context(|| format!("Failed to run '{} tag {} {}'", runtime_path, image_id, tag))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DeaconError::Docker(DockerError::CLIError(format!(
            "Failed to tag built image '{}' as '{}': {}",
            image_id,
            tag,
            stderr.trim()
        )))
        .into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_local_build_tag_from_digest_is_real_repo_tag() {
        // A bare `sha256:` digest used as a `FROM` 404s in BuildKit; the derived
        // tag must be a real `repo:tag` with no `sha256:` prefix.
        let tag = derive_local_build_tag(
            "sha256:f621c4937bcb40b86157263eb8c14c45ca0b4c273747da4a57bc301f895d398b",
        );
        assert_eq!(tag, "deacon-build:f621c4937bcb");
        assert!(!tag.contains("sha256:"));
        // Deterministic: same digest → same tag (content-addressed).
        assert_eq!(
            tag,
            derive_local_build_tag(
                "sha256:f621c4937bcb40b86157263eb8c14c45ca0b4c273747da4a57bc301f895d398b"
            )
        );
    }

    #[test]
    fn derive_local_build_tag_without_sha256_prefix() {
        // Defensive: a non-`sha256:` id still yields a valid-looking tag.
        assert_eq!(
            derive_local_build_tag("abcdef123456"),
            "deacon-build:abcdef123456"
        );
        assert_eq!(derive_local_build_tag("short"), "deacon-build:short");
    }
}
