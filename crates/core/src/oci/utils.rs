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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retry::{retry_async, JitterStrategy, RetryConfig, RetryDecision};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// Tight test profile: same classifier behavior, but with millisecond delays
    /// so tests don't actually wait seconds.
    fn fast_network_config() -> RetryConfig {
        RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            jitter: JitterStrategy::FullJitter,
        }
    }

    /// BEAD-16-T01: transient network errors are retried up to max_attempts times.
    #[tokio::test]
    async fn classify_transient_download_error_retries() {
        let config = fast_network_config();
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let op = move || {
            let counter = Arc::clone(&attempts_clone);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<(), FeatureError>(FeatureError::Download {
                    message: "connection refused".to_string(),
                })
            }
        };

        let result = retry_async(&config, op, classify_network_error).await;
        assert!(result.is_err());
        // Initial attempt + max_attempts retries = 4 total
        assert_eq!(attempts.load(Ordering::SeqCst), 4);
    }

    /// BEAD-16-T02: 401 auth failure stops immediately (no retries).
    #[tokio::test]
    async fn classify_unauthorized_stops_immediately() {
        let config = fast_network_config();
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let op = move || {
            let counter = Arc::clone(&attempts_clone);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<(), FeatureError>(FeatureError::Unauthorized {
                    message: "401 from registry".to_string(),
                })
            }
        };

        let result = retry_async(&config, op, classify_network_error).await;
        assert!(result.is_err());
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            1,
            "401 must not be retried"
        );
    }

    /// BEAD-16-T03: NotFound (404) stops immediately (no retries).
    #[tokio::test]
    async fn classify_not_found_stops_immediately() {
        let config = fast_network_config();
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let op = move || {
            let counter = Arc::clone(&attempts_clone);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<(), FeatureError>(FeatureError::NotFound {
                    path: "missing-feature".to_string(),
                })
            }
        };

        let result = retry_async(&config, op, classify_network_error).await;
        assert!(result.is_err());
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            1,
            "404 must not be retried"
        );
    }

    /// BEAD-16-T04: transient failures followed by success returns Ok on the
    /// successful attempt without further retries.
    #[tokio::test]
    async fn classify_transient_then_success() {
        let config = fast_network_config();
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let op = move || {
            let counter = Arc::clone(&attempts_clone);
            async move {
                let n = counter.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err(FeatureError::Download {
                        message: "transient".to_string(),
                    })
                } else {
                    Ok(42)
                }
            }
        };

        let result = retry_async(&config, op, classify_network_error).await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    /// Sanity: classifier returns the expected decisions for boundary cases.
    /// Documents the contract that 401/403/404/parse all stop without retry.
    #[test]
    fn classify_decisions_at_boundaries() {
        use FeatureError::*;
        assert_eq!(
            classify_network_error(&Forbidden {
                message: "403".into()
            }),
            RetryDecision::Stop
        );
        assert_eq!(
            classify_network_error(&Parsing {
                message: "bad json".into()
            }),
            RetryDecision::Stop
        );
        // Network errors retry
        assert_eq!(
            classify_network_error(&Oci {
                message: "registry timeout".into()
            }),
            RetryDecision::Retry
        );
    }
}
