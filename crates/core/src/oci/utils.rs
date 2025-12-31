//! Utility functions for OCI operations

use sha2::{Digest, Sha256};
use std::path::PathBuf;

use super::types::Manifest;
use crate::errors::{FeatureError, Result};
use crate::retry::RetryDecision;

/// Error classifier for network operations
/// Only retries on network-related errors, not on parsing or other logical errors
pub(crate) fn classify_network_error(error: &FeatureError) -> RetryDecision {
    match error {
        FeatureError::Download { .. } => RetryDecision::Retry,
        FeatureError::Oci { .. } => RetryDecision::Retry,
        // Authentication errors should be retried once to allow credential refresh
        FeatureError::Authentication { .. } => RetryDecision::Retry,
        // Don't retry auth failures - they require correct credentials
        FeatureError::Unauthorized { .. } | FeatureError::Forbidden { .. } => RetryDecision::Stop,
        // Don't retry parsing, validation, or other logical errors
        FeatureError::Parsing { .. }
        | FeatureError::Validation { .. }
        | FeatureError::Extraction { .. }
        | FeatureError::Installation { .. }
        | FeatureError::InstallationFailed { .. }
        | FeatureError::NotFound { .. }
        | FeatureError::DependencyCycle { .. }
        | FeatureError::InvalidDependency { .. }
        | FeatureError::DependencyResolution { .. } => RetryDecision::Stop,
        // For IO errors, retry as they might be transient
        FeatureError::Io(_) => RetryDecision::Retry,
        FeatureError::Json(_) => RetryDecision::Stop,
        FeatureError::NotImplemented => RetryDecision::Stop,
    }
}

/// Get the default cache directory for features
///
/// Uses the standard cache directory with a 'features' subdirectory for persistent
/// feature caching across workspace sessions.
///
/// # Examples
///
/// ```
/// use deacon_core::oci::get_features_cache_dir;
/// let cache_dir = get_features_cache_dir().expect("failed to get features cache dir");
/// assert!(cache_dir.ends_with("features"));
/// ```
pub fn get_features_cache_dir() -> Result<PathBuf> {
    let base_cache = crate::progress::get_cache_dir().map_err(|e| FeatureError::Oci {
        message: format!("Failed to get cache directory: {}", e),
    })?;
    let features_cache = base_cache.join("features");

    // Ensure features cache directory exists
    if !features_cache.exists() {
        std::fs::create_dir_all(&features_cache).map_err(|e| FeatureError::Oci {
            message: format!("Failed to create features cache directory: {}", e),
        })?;
    }

    Ok(features_cache)
}

/// Compute the canonical ID (SHA256 digest) of an OCI manifest
///
/// The canonical ID is the SHA256 hash of the manifest's serialized JSON representation.
/// This serves as a unique, content-addressed identifier for the manifest in OCI registries.
///
/// # Arguments
///
/// * `manifest` - The OCI manifest to compute the canonical ID for
///
/// # Returns
///
/// A string in the format `sha256:<64-character-hex-digest>`
///
/// # Examples
///
/// ```
/// use deacon_core::oci::{Manifest, canonical_id};
/// use serde_json::json;
///
/// // Note: This example assumes Manifest can be constructed from JSON
/// let manifest_json = json!({
///     "schemaVersion": 2,
///     "mediaType": "application/vnd.oci.image.manifest.v1+json",
///     "layers": []
/// });
/// // In practice, manifests are parsed from OCI registry responses
/// ```
pub fn canonical_id(manifest: &Manifest) -> Result<String> {
    let manifest_json = serde_json::to_vec(manifest).map_err(FeatureError::Json)?;
    let mut hasher = Sha256::new();
    hasher.update(&manifest_json);
    Ok(format!("sha256:{:x}", hasher.finalize()))
}
