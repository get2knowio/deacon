//! OCI registry integration for DevContainer features
//!
//! This module implements OCI registry v2 support with authentication for fetching
//! and installing DevContainer features. It supports authentication via environment
//! variables, Docker credential helpers, custom CA certificates, and proxy configuration.
//!
//! ## Core Capabilities
//!
//! - **Feature and Template Operations**: Fetch and install features and templates
//! - **Tag Listing**: Query available tags from registries (`list_tags`)
//! - **Manifest Operations**: Fetch manifests by tag or digest (`get_manifest`, `get_manifest_by_digest`)
//! - **Collection Metadata**: Support for devcontainer-collection.json structure
//! - **Semantic Versioning**: Parse, filter, sort, and compute semantic version tags (`semver_utils` module)
//!
//! ## Authentication
//!
//! Supports multiple authentication methods with the following precedence order:
//!
//! 1. **Environment Variables** (highest priority):
//!    - `DEACON_REGISTRY_TOKEN`: Bearer token authentication
//!    - `DEACON_REGISTRY_USER` + `DEACON_REGISTRY_PASS`: Basic authentication
//! 2. **Docker config.json**: Credentials from `~/.docker/config.json`
//! 3. **No authentication**: Fallback for public registries
//!
//! Custom CA certificates can be configured via:
//! - `DEACON_CUSTOM_CA_BUNDLE`: Path to a PEM-encoded CA certificate bundle
//!
//! ## Semantic Version Utilities
//!
//! The `semver_utils` module provides utilities for working with semantic versions:
//! - Parse versions from tags (handles "v1.2.3", "1.2.3", "1.2", "1" formats)
//! - Filter tags to only semantic versions
//! - Sort tags in semantic version order
//! - Compute semantic tags (e.g., "1.2.3" → ["1", "1.2", "1.2.3", "latest"])
//! - Compare versions

mod auth;
mod client;
mod fetcher;
mod types;
mod utils;

// Re-export public types
pub use auth::{RegistryAuth, RegistryCredentials};
pub use client::{HttpClient, MockHttpClient, ReqwestClient};
pub use fetcher::{FeatureFetcher, default_fetcher, default_fetcher_with_config};
pub use types::{
    CollectionFeature, CollectionMetadata, CollectionSourceInfo, CollectionTemplate,
    DownloadedFeature, DownloadedTemplate, FeatureRef, HttpResponse, Layer, Manifest, TagList,
    TemplateRef,
};
pub use utils::{canonical_id, get_features_cache_dir};

// Re-export semver_utils for backwards compatibility with oci::semver_utils path
pub use crate::semver_utils;
#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use bytes::Bytes;
    use std::collections::HashMap;

    use utils::classify_network_error;

    #[test]
    fn test_feature_ref_creation() {
        let feature_ref = FeatureRef::new(
            "ghcr.io".to_string(),
            "devcontainers".to_string(),
            "node".to_string(),
            Some("18".to_string()),
        );

        assert_eq!(feature_ref.registry, "ghcr.io");
        assert_eq!(feature_ref.namespace, "devcontainers");
        assert_eq!(feature_ref.name, "node");
        assert_eq!(feature_ref.version, Some("18".to_string()));
        assert_eq!(feature_ref.tag(), "18");
        assert_eq!(feature_ref.repository(), "devcontainers/node");
        assert_eq!(feature_ref.reference(), "ghcr.io/devcontainers/node:18");
    }

    #[test]
    fn test_feature_ref_default_version() {
        let feature_ref = FeatureRef::new(
            "ghcr.io".to_string(),
            "devcontainers".to_string(),
            "node".to_string(),
            None,
        );

        assert_eq!(feature_ref.tag(), "latest");
        assert_eq!(feature_ref.reference(), "ghcr.io/devcontainers/node:latest");
    }

    #[test]
    fn test_template_ref_creation() {
        let template_ref = TemplateRef::new(
            "ghcr.io".to_string(),
            "devcontainers".to_string(),
            "python".to_string(),
            Some("3.11".to_string()),
        );

        assert_eq!(template_ref.registry, "ghcr.io");
        assert_eq!(template_ref.namespace, "devcontainers");
        assert_eq!(template_ref.name, "python");
        assert_eq!(template_ref.version, Some("3.11".to_string()));
        assert_eq!(template_ref.tag(), "3.11");
        assert_eq!(template_ref.repository(), "devcontainers/python");
        assert_eq!(
            template_ref.reference(),
            "ghcr.io/devcontainers/python:3.11"
        );
    }

    #[test]
    fn test_template_ref_default_version() {
        let template_ref = TemplateRef::new(
            "ghcr.io".to_string(),
            "devcontainers".to_string(),
            "python".to_string(),
            None,
        );

        assert_eq!(template_ref.tag(), "latest");
        assert_eq!(
            template_ref.reference(),
            "ghcr.io/devcontainers/python:latest"
        );
    }

    #[test]
    fn test_registry_credentials_auth_header() {
        // Test Basic authentication
        let basic_creds = RegistryCredentials::Basic {
            username: "user".to_string(),
            password: "pass".to_string(),
        };
        let auth_header = basic_creds.to_auth_header().unwrap();
        assert!(auth_header.starts_with("Basic "));

        // Decode and verify
        let encoded = auth_header.strip_prefix("Basic ").unwrap();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        let decoded_str = String::from_utf8(decoded).unwrap();
        assert_eq!(decoded_str, "user:pass");

        // Test Bearer authentication
        let bearer_creds = RegistryCredentials::Bearer {
            token: "abc123".to_string(),
        };
        let auth_header = bearer_creds.to_auth_header().unwrap();
        assert_eq!(auth_header, "Bearer abc123");

        // Test no authentication
        let none_creds = RegistryCredentials::None;
        assert!(none_creds.to_auth_header().is_none());
    }

    #[test]
    fn test_registry_auth_configuration() {
        let mut auth = RegistryAuth::new();

        // Test default credentials
        auth.set_default_credentials(RegistryCredentials::Basic {
            username: "default_user".to_string(),
            password: "default_pass".to_string(),
        });

        // Test registry-specific credentials
        auth.set_credentials(
            "ghcr.io".to_string(),
            RegistryCredentials::Bearer {
                token: "ghcr_token".to_string(),
            },
        );

        // Test getting default credentials
        let creds = auth.get_credentials("unknown.registry");
        assert_eq!(
            creds,
            &RegistryCredentials::Basic {
                username: "default_user".to_string(),
                password: "default_pass".to_string(),
            }
        );

        // Test getting registry-specific credentials
        let creds = auth.get_credentials("ghcr.io");
        assert_eq!(
            creds,
            &RegistryCredentials::Bearer {
                token: "ghcr_token".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn test_mock_http_client() {
        let client = MockHttpClient::new();
        let test_data = Bytes::from("test response");

        client
            .add_response("https://example.com/test".to_string(), test_data.clone())
            .await;

        let result = client.get("https://example.com/test").await.unwrap();
        assert_eq!(result, test_data);

        // Test non-existent URL
        let result = client.get("https://example.com/nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_retry_integration_with_manifest_fetch() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU32, Ordering};

        // Mock client that fails first N attempts
        #[derive(Debug, Clone)]
        struct FailingMockClient {
            failure_count: Arc<AtomicU32>,
            fail_attempts: u32,
        }

        impl FailingMockClient {
            fn new(fail_attempts: u32) -> Self {
                Self {
                    failure_count: Arc::new(AtomicU32::new(0)),
                    fail_attempts,
                }
            }
        }

        #[async_trait::async_trait]
        impl HttpClient for FailingMockClient {
            async fn get(
                &self,
                _url: &str,
            ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
                let current = self.failure_count.fetch_add(1, Ordering::SeqCst);
                if current < self.fail_attempts {
                    Err("network error".into())
                } else {
                    // Return a valid manifest JSON after failures
                    let manifest = serde_json::json!({
                        "schemaVersion": 2,
                        "mediaType": "application/vnd.oci.image.manifest.v1+json",
                        "layers": [{
                            "mediaType": "application/vnd.oci.image.layer.v1.tar",
                            "size": 1024,
                            "digest": "sha256:abc123"
                        }]
                    });
                    Ok(Bytes::from(manifest.to_string()))
                }
            }

            async fn get_with_headers(
                &self,
                url: &str,
                _headers: HashMap<String, String>,
            ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
                self.get(url).await
            }

            async fn get_with_headers_and_response(
                &self,
                _url: &str,
                _headers: HashMap<String, String>,
            ) -> std::result::Result<HttpResponse, Box<dyn std::error::Error + Send + Sync>>
            {
                let current = self.failure_count.fetch_add(1, Ordering::SeqCst);
                if current < self.fail_attempts {
                    Err("network error".into())
                } else {
                    Ok(HttpResponse {
                        status: 200,
                        headers: HashMap::new(),
                        body: Bytes::new(),
                    })
                }
            }

            async fn head(
                &self,
                _url: &str,
                _headers: HashMap<String, String>,
            ) -> std::result::Result<u16, Box<dyn std::error::Error + Send + Sync>> {
                let current = self.failure_count.fetch_add(1, Ordering::SeqCst);
                if current < self.fail_attempts {
                    Err("network error".into())
                } else {
                    Ok(404)
                }
            }
        }

        // Test that retry works - should succeed after 2 failures
        let client = FailingMockClient::new(2);
        let retry_config = crate::retry::RetryConfig {
            max_attempts: 3,
            base_delay: std::time::Duration::from_millis(1),
            max_delay: std::time::Duration::from_millis(10),
            jitter: crate::retry::JitterStrategy::FullJitter,
        };

        let fetcher = FeatureFetcher::with_retry_config(
            client.clone(),
            std::env::temp_dir().join("test-cache"),
            retry_config,
        );

        let feature_ref = FeatureRef::new(
            "test.registry".to_string(),
            "test".to_string(),
            "feature".to_string(),
            Some("v1.0".to_string()),
        );

        let result = fetcher.get_manifest(&feature_ref).await;
        assert!(result.is_ok());

        let manifest = result.unwrap();
        assert_eq!(manifest.layers.len(), 1);
        assert_eq!(manifest.layers[0].digest, "sha256:abc123");

        // Verify that it tried 3 times (initial + 2 retries)
        assert_eq!(client.failure_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_gives_up_after_max_attempts() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU32, Ordering};

        // Mock client that always fails
        #[derive(Debug, Clone)]
        struct AlwaysFailingClient {
            call_count: Arc<AtomicU32>,
        }

        impl AlwaysFailingClient {
            fn new() -> Self {
                Self {
                    call_count: Arc::new(AtomicU32::new(0)),
                }
            }
        }

        #[async_trait::async_trait]
        impl HttpClient for AlwaysFailingClient {
            async fn get(
                &self,
                _url: &str,
            ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
                self.call_count.fetch_add(1, Ordering::SeqCst);
                Err("permanent network error".into())
            }

            async fn get_with_headers(
                &self,
                url: &str,
                _headers: HashMap<String, String>,
            ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
                self.get(url).await
            }

            async fn get_with_headers_and_response(
                &self,
                _url: &str,
                _headers: HashMap<String, String>,
            ) -> std::result::Result<HttpResponse, Box<dyn std::error::Error + Send + Sync>>
            {
                self.call_count.fetch_add(1, Ordering::SeqCst);
                Err("permanent network error".into())
            }

            async fn head(
                &self,
                _url: &str,
                _headers: HashMap<String, String>,
            ) -> std::result::Result<u16, Box<dyn std::error::Error + Send + Sync>> {
                self.call_count.fetch_add(1, Ordering::SeqCst);
                Err("permanent network error".into())
            }
        }

        let client = AlwaysFailingClient::new();
        let retry_config = crate::retry::RetryConfig {
            max_attempts: 2,
            base_delay: std::time::Duration::from_millis(1),
            max_delay: std::time::Duration::from_millis(10),
            jitter: crate::retry::JitterStrategy::FullJitter,
        };

        let fetcher = FeatureFetcher::with_retry_config(
            client.clone(),
            std::env::temp_dir().join("test-cache"),
            retry_config,
        );

        let feature_ref = FeatureRef::new(
            "test.registry".to_string(),
            "test".to_string(),
            "feature".to_string(),
            Some("v1.0".to_string()),
        );

        let result = fetcher.get_manifest(&feature_ref).await;
        assert!(result.is_err());

        // Should have tried 3 times total (initial + 2 retries)
        assert_eq!(client.call_count.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_error_classifier() {
        use crate::errors::FeatureError;

        // Test that network errors are retried
        let download_error = FeatureError::Download {
            message: "network timeout".to_string(),
        };
        assert_eq!(
            classify_network_error(&download_error),
            crate::retry::RetryDecision::Retry
        );

        let oci_error = FeatureError::Oci {
            message: "registry unavailable".to_string(),
        };
        assert_eq!(
            classify_network_error(&oci_error),
            crate::retry::RetryDecision::Retry
        );

        // Test that authentication errors are retried
        let auth_error = FeatureError::Authentication {
            message: "invalid credentials".to_string(),
        };
        assert_eq!(
            classify_network_error(&auth_error),
            crate::retry::RetryDecision::Retry
        );

        // Test that logical errors are not retried
        let parsing_error = FeatureError::Parsing {
            message: "invalid json".to_string(),
        };
        assert_eq!(
            classify_network_error(&parsing_error),
            crate::retry::RetryDecision::Stop
        );

        let validation_error = FeatureError::Validation {
            message: "missing field".to_string(),
        };
        assert_eq!(
            classify_network_error(&validation_error),
            crate::retry::RetryDecision::Stop
        );
    }

    #[test]
    fn test_get_features_cache_dir() {
        let cache_dir = get_features_cache_dir().expect("should get features cache dir");

        // Verify the cache directory is created and has the right structure
        assert!(cache_dir.exists());
        assert!(cache_dir.is_dir());
        assert!(cache_dir.ends_with("features"));

        // Verify it's using the persistent cache base directory
        let expected_base = crate::progress::get_cache_dir().expect("should get cache dir");
        let expected_features_cache = expected_base.join("features");
        assert_eq!(cache_dir, expected_features_cache);
    }

    #[test]
    fn test_feature_fetcher_uses_persistent_cache() {
        let mock_client = MockHttpClient::new();
        let fetcher = FeatureFetcher::new(mock_client);

        // The fetcher should use the persistent cache directory
        let expected_cache = get_features_cache_dir()
            .unwrap_or_else(|_| std::env::temp_dir().join("deacon-features"));

        assert_eq!(fetcher.cache_dir, expected_cache);
    }
}
