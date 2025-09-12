//! OCI registry integration for DevContainer features
//!
//! This module implements minimal unauthenticated OCI registry v2 support for fetching
//! and installing DevContainer features. It supports basic caching and install script execution.

use crate::errors::{FeatureError, Result};
use crate::features::{parse_feature_metadata, FeatureMetadata};
use crate::retry::{retry_async, RetryConfig, RetryDecision};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tar::Archive;
use tokio::sync::Mutex;
use tracing::{debug, info, instrument, warn};

/// Reference to a feature in an OCI registry
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureRef {
    /// Registry hostname (e.g., "ghcr.io")
    pub registry: String,
    /// Namespace (e.g., "devcontainers")
    pub namespace: String,
    /// Feature name (e.g., "node")
    pub name: String,
    /// Version (optional, defaults to "latest")
    pub version: Option<String>,
}

impl FeatureRef {
    /// Create a new FeatureRef
    pub fn new(registry: String, namespace: String, name: String, version: Option<String>) -> Self {
        Self {
            registry,
            namespace,
            name,
            version,
        }
    }

    /// Get the tag for this feature reference
    pub fn tag(&self) -> &str {
        self.version.as_deref().unwrap_or("latest")
    }

    /// Get the repository name for this feature
    pub fn repository(&self) -> String {
        format!("{}/{}", self.namespace, self.name)
    }

    /// Get the full reference string
    pub fn reference(&self) -> String {
        format!("{}/{}:{}", self.registry, self.repository(), self.tag())
    }
}

/// Downloaded and extracted feature data
#[derive(Debug)]
pub struct DownloadedFeature {
    /// Extracted feature directory
    pub path: PathBuf,
    /// Feature metadata
    pub metadata: FeatureMetadata,
    /// Feature digest for caching
    pub digest: String,
}

/// OCI manifest structure (minimal)
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Manifest {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    #[serde(rename = "mediaType")]
    media_type: String,
    layers: Vec<Layer>,
}

/// OCI layer structure (minimal)
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Layer {
    #[serde(rename = "mediaType")]
    media_type: String,
    size: u64,
    digest: String,
}

/// HTTP client trait to enable mocking and testing
#[async_trait::async_trait]
pub trait HttpClient: Send + Sync {
    /// Perform a GET request and return the response body
    async fn get(
        &self,
        url: &str,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>>;

    /// Get with custom headers
    async fn get_with_headers(
        &self,
        url: &str,
        headers: HashMap<String, String>,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>>;
}

/// Default HTTP client implementation using reqwest
#[derive(Debug, Clone)]
pub struct ReqwestClient {
    client: reqwest::Client,
}

impl ReqwestClient {
    /// Create a new ReqwestClient
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for ReqwestClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl HttpClient for ReqwestClient {
    async fn get(
        &self,
        url: &str,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        let response = self.client.get(url).send().await?;
        let bytes = response.bytes().await?;
        Ok(bytes)
    }

    async fn get_with_headers(
        &self,
        url: &str,
        headers: HashMap<String, String>,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        let mut request = self.client.get(url);
        for (key, value) in headers {
            request = request.header(&key, &value);
        }
        let response = request.send().await?;
        let bytes = response.bytes().await?;
        Ok(bytes)
    }
}

/// Feature fetcher for OCI registries
pub struct FeatureFetcher<C: HttpClient> {
    client: C,
    cache_dir: PathBuf,
    retry_config: RetryConfig,
}

/// Error classifier for network operations
/// Only retries on network-related errors, not on parsing or other logical errors
fn classify_network_error(error: &FeatureError) -> RetryDecision {
    match error {
        FeatureError::Download { .. } => RetryDecision::Retry,
        FeatureError::Oci { .. } => RetryDecision::Retry,
        // Don't retry parsing, validation, or other logical errors
        FeatureError::Parsing { .. }
        | FeatureError::Validation { .. }
        | FeatureError::Extraction { .. }
        | FeatureError::Installation { .. }
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

impl<C: HttpClient> FeatureFetcher<C> {
    /// Create a new FeatureFetcher with custom HTTP client
    pub fn new(client: C) -> Self {
        let cache_dir = std::env::temp_dir().join("deacon-features");
        Self {
            client,
            cache_dir,
            retry_config: RetryConfig::default(),
        }
    }

    /// Create a new FeatureFetcher with custom cache directory
    pub fn with_cache_dir(client: C, cache_dir: PathBuf) -> Self {
        Self {
            client,
            cache_dir,
            retry_config: RetryConfig::default(),
        }
    }

    /// Create a new FeatureFetcher with custom retry configuration
    pub fn with_retry_config(client: C, cache_dir: PathBuf, retry_config: RetryConfig) -> Self {
        Self {
            client,
            cache_dir,
            retry_config,
        }
    }

    /// Get a reference to the HTTP client (for testing)
    pub fn client(&self) -> &C {
        &self.client
    }

    /// Fetch a feature from an OCI registry
    #[instrument(level = "info", skip(self))]
    pub async fn fetch_feature(&self, feature_ref: &FeatureRef) -> Result<DownloadedFeature> {
        info!("Fetching feature: {}", feature_ref.reference());

        // Get the manifest
        let manifest = self.get_manifest(feature_ref).await?;
        debug!("Got manifest with {} layers", manifest.layers.len());

        // For now, assume single tar layer (as per requirements)
        if manifest.layers.is_empty() {
            return Err(FeatureError::Oci {
                message: "No layers found in manifest".to_string(),
            }
            .into());
        }

        let layer = &manifest.layers[0];
        debug!("Using layer: digest={}, size={}", layer.digest, layer.size);

        // Check cache first
        let cache_key = self.get_cache_key(&layer.digest);
        let cached_dir = self.cache_dir.join(&cache_key);

        if cached_dir.exists() {
            info!("Found cached feature at: {}", cached_dir.display());
            return self
                .load_cached_feature(cached_dir, layer.digest.clone())
                .await;
        }

        // Download and extract the layer
        let layer_data = self.download_layer(feature_ref, &layer.digest).await?;
        let extracted_dir = self.extract_layer(layer_data, &cache_key).await?;

        // Parse metadata
        let metadata_path = extracted_dir.join("devcontainer-feature.json");
        let metadata = parse_feature_metadata(&metadata_path)?;

        info!("Successfully fetched feature: {}", metadata.id);
        Ok(DownloadedFeature {
            path: extracted_dir,
            metadata,
            digest: layer.digest.clone(),
        })
    }

    /// Install a downloaded feature by executing its install script
    #[instrument(level = "info", skip(self))]
    pub async fn install_feature(&self, feature: &DownloadedFeature) -> Result<()> {
        info!("Installing feature: {}", feature.metadata.id);

        let install_script = feature.path.join("install.sh");
        if !install_script.exists() {
            info!("No install.sh script found, skipping installation");
            return Ok(());
        }

        // Set up environment variables
        let mut env_vars = HashMap::new();
        env_vars.insert("FEATURE_ID".to_string(), feature.metadata.id.clone());
        if let Some(version) = &feature.metadata.version {
            env_vars.insert("FEATURE_VERSION".to_string(), version.clone());
        }

        // Execute the install script
        self.execute_install_script(&install_script, &env_vars)
            .await?;

        info!("Feature installation completed: {}", feature.metadata.id);
        Ok(())
    }

    /// Get the OCI manifest for a feature
    async fn get_manifest(&self, feature_ref: &FeatureRef) -> Result<Manifest> {
        let manifest_url = format!(
            "https://{}/v2/{}/manifests/{}",
            feature_ref.registry,
            feature_ref.repository(),
            feature_ref.tag()
        );

        debug!("Fetching manifest from: {}", manifest_url);

        let mut headers = HashMap::new();
        headers.insert(
            "Accept".to_string(),
            "application/vnd.oci.image.manifest.v1+json".to_string(),
        );

        // Retry the manifest download with exponential backoff
        let manifest_data = retry_async(
            &self.retry_config,
            || {
                let client = &self.client;
                let url = &manifest_url;
                let headers = headers.clone();
                async move {
                    client.get_with_headers(url, headers).await.map_err(|e| {
                        FeatureError::Download {
                            message: format!("Failed to download manifest: {}", e),
                        }
                    })
                }
            },
            classify_network_error,
        )
        .await?;

        let manifest: Manifest =
            serde_json::from_slice(&manifest_data).map_err(|e| FeatureError::Parsing {
                message: format!("Failed to parse manifest: {}", e),
            })?;

        Ok(manifest)
    }

    /// Download a layer blob
    async fn download_layer(&self, feature_ref: &FeatureRef, digest: &str) -> Result<Bytes> {
        let blob_url = format!(
            "https://{}/v2/{}/blobs/{}",
            feature_ref.registry,
            feature_ref.repository(),
            digest
        );

        debug!("Downloading layer from: {}", blob_url);

        // Retry the layer download with exponential backoff
        let layer_data = retry_async(
            &self.retry_config,
            || {
                let client = &self.client;
                let url = &blob_url;
                async move {
                    client.get(url).await.map_err(|e| FeatureError::Download {
                        message: format!("Failed to download layer: {}", e),
                    })
                }
            },
            classify_network_error,
        )
        .await?;

        Ok(layer_data)
    }

    /// Extract a tar layer to the cache directory
    async fn extract_layer(&self, layer_data: Bytes, cache_key: &str) -> Result<PathBuf> {
        let extraction_dir = self.cache_dir.join(cache_key);

        // Create cache directory if it doesn't exist
        std::fs::create_dir_all(&extraction_dir).map_err(|e| FeatureError::Extraction {
            message: format!("Failed to create extraction directory: {}", e),
        })?;

        debug!("Extracting layer to: {}", extraction_dir.display());

        // Extract tar archive
        let cursor = std::io::Cursor::new(layer_data);
        let mut archive = Archive::new(cursor);

        archive
            .unpack(&extraction_dir)
            .map_err(|e| FeatureError::Extraction {
                message: format!("Failed to extract tar archive: {}", e),
            })?;

        Ok(extraction_dir)
    }

    /// Load a cached feature
    async fn load_cached_feature(
        &self,
        cached_dir: PathBuf,
        digest: String,
    ) -> Result<DownloadedFeature> {
        let metadata_path = cached_dir.join("devcontainer-feature.json");
        let metadata = parse_feature_metadata(&metadata_path)?;

        Ok(DownloadedFeature {
            path: cached_dir,
            metadata,
            digest,
        })
    }

    /// Generate a cache key from a digest
    fn get_cache_key(&self, digest: &str) -> String {
        // Use a shortened version of the digest as cache key
        let mut hasher = Sha256::new();
        hasher.update(digest.as_bytes());
        let hash = hasher.finalize();
        format!("{:x}", hash)[..16].to_string()
    }

    /// Execute the install script with environment variables
    async fn execute_install_script(
        &self,
        script_path: &Path,
        env_vars: &HashMap<String, String>,
    ) -> Result<()> {
        debug!("Executing install script: {}", script_path.display());

        let mut command = Command::new("bash");
        command.arg(script_path);

        // Set environment variables
        for (key, value) in env_vars {
            command.env(key, value);
            debug!("Set environment variable: {}={}", key, value);
        }

        // Execute the command
        let output = command.output().map_err(|e| FeatureError::Installation {
            message: format!("Failed to execute install script: {}", e),
        })?;

        // Log stdout and stderr
        if !output.stdout.is_empty() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                info!("[install] stdout: {}", line);
            }
        }

        if !output.stderr.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            for line in stderr.lines() {
                warn!("[install] stderr: {}", line);
            }
        }

        // Check exit status
        if !output.status.success() {
            return Err(FeatureError::Installation {
                message: format!(
                    "Install script failed with exit code: {}",
                    output.status.code().unwrap_or(-1)
                ),
            }
            .into());
        }

        debug!("Install script completed successfully");
        Ok(())
    }
}

/// Convenience function to create a default feature fetcher
pub fn default_fetcher() -> FeatureFetcher<ReqwestClient> {
    FeatureFetcher::new(ReqwestClient::new())
}

/// Mock HTTP client for testing
#[derive(Debug, Clone)]
pub struct MockHttpClient {
    responses: Arc<Mutex<HashMap<String, Bytes>>>,
}

impl MockHttpClient {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn add_response(&self, url: String, response: Bytes) {
        let mut responses = self.responses.lock().await;
        responses.insert(url, response);
    }
}

impl Default for MockHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl HttpClient for MockHttpClient {
    async fn get(
        &self,
        url: &str,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        let responses = self.responses.lock().await;
        responses
            .get(url)
            .cloned()
            .ok_or_else(|| format!("No mock response for URL: {}", url).into())
    }

    async fn get_with_headers(
        &self,
        url: &str,
        _headers: HashMap<String, String>,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        self.get(url).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

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
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

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
}
