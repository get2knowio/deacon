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
        // Integrity failures are deterministic for given bytes; retrying the
        // same registry response cannot help, so fail fast.
        | FeatureError::IntegrityMismatch { .. }
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

/// Verify that the SHA256 digest of `data` matches the `expected` OCI digest.
///
/// `expected` is an OCI content descriptor digest in `algorithm:hex` form
/// (e.g. `sha256:<64 hex chars>`). Only `sha256` is supported; any other
/// algorithm is rejected rather than silently accepted (no silent fallback per
/// the project's fail-fast principle). The hex comparison is case-insensitive.
///
/// On any mismatch — malformed digest, unsupported algorithm, or content that
/// does not hash to the expected value — returns [`FeatureError::IntegrityMismatch`]
/// so callers fail closed before trusting downloaded bytes. `context` is a short
/// human-readable description of what is being verified (e.g. the blob URL or
/// reference) for diagnostics.
pub(crate) fn verify_content_digest(data: &[u8], expected: &str, context: &str) -> Result<()> {
    let (algorithm, expected_hex) =
        expected
            .split_once(':')
            .ok_or_else(|| FeatureError::IntegrityMismatch {
                context: context.to_string(),
                expected: expected.to_string(),
                actual: "<malformed digest: missing 'algorithm:' prefix>".to_string(),
            })?;

    if !algorithm.eq_ignore_ascii_case("sha256") {
        return Err(FeatureError::IntegrityMismatch {
            context: context.to_string(),
            expected: expected.to_string(),
            actual: format!("<unsupported digest algorithm: {}>", algorithm),
        }
        .into());
    }

    let mut hasher = Sha256::new();
    hasher.update(data);
    let actual_hex = format!("{:x}", hasher.finalize());

    if !actual_hex.eq_ignore_ascii_case(expected_hex) {
        return Err(FeatureError::IntegrityMismatch {
            context: context.to_string(),
            expected: expected.to_string(),
            actual: format!("sha256:{}", actual_hex),
        }
        .into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retry::{JitterStrategy, RetryConfig, RetryDecision, retry_async};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
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

    #[test]
    fn verify_content_digest_accepts_matching_sha256() {
        // sha256("hello world") = b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
        let data = b"hello world";
        let expected = "sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        assert!(verify_content_digest(data, expected, "test blob").is_ok());
    }

    #[test]
    fn verify_content_digest_is_case_insensitive_on_hex() {
        let data = b"hello world";
        let expected = "sha256:B94D27B9934D3E08A52E52D7DA7DABFAC484EFE37A5380EE9088F7ACE2EFCDE9";
        assert!(verify_content_digest(data, expected, "test blob").is_ok());
    }

    #[test]
    fn verify_content_digest_rejects_tampered_content() {
        let data = b"tampered payload";
        let expected = "sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        let err = verify_content_digest(data, expected, "test blob").unwrap_err();
        assert!(
            matches!(
                err,
                crate::errors::DeaconError::Feature(FeatureError::IntegrityMismatch { .. })
            ),
            "expected IntegrityMismatch, got {:?}",
            err
        );
    }

    #[test]
    fn verify_content_digest_rejects_unsupported_algorithm() {
        let data = b"hello world";
        let err = verify_content_digest(data, "sha512:deadbeef", "test blob").unwrap_err();
        assert!(matches!(
            err,
            crate::errors::DeaconError::Feature(FeatureError::IntegrityMismatch { .. })
        ));
    }

    #[test]
    fn verify_content_digest_rejects_malformed_digest() {
        let data = b"hello world";
        let err = verify_content_digest(data, "no-prefix-here", "test blob").unwrap_err();
        assert!(matches!(
            err,
            crate::errors::DeaconError::Feature(FeatureError::IntegrityMismatch { .. })
        ));
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
