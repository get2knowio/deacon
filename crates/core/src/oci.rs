//! OCI registry integration for DevContainer features
//!
//! This module implements minimal unauthenticated OCI registry v2 support for fetching
//! and installing DevContainer features. It supports basic caching and install script execution.

use crate::errors::{FeatureError, Result};
use crate::features::{parse_feature_metadata, FeatureMetadata};
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
}

impl<C: HttpClient> FeatureFetcher<C> {
    /// Create a new FeatureFetcher with custom HTTP client
    pub fn new(client: C) -> Self {
        let cache_dir = std::env::temp_dir().join("deacon-features");
        Self { client, cache_dir }
    }

    /// Create a new FeatureFetcher with custom cache directory
    pub fn with_cache_dir(client: C, cache_dir: PathBuf) -> Self {
        Self { client, cache_dir }
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

        let manifest_data = self
            .client
            .get_with_headers(&manifest_url, headers)
            .await
            .map_err(|e| FeatureError::Download {
                message: format!("Failed to download manifest: {}", e),
            })?;

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

        let layer_data = self
            .client
            .get(&blob_url)
            .await
            .map_err(|e| FeatureError::Download {
                message: format!("Failed to download layer: {}", e),
            })?;

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
}
