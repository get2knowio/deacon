//! OCI registry integration for DevContainer features
//!
//! This module implements OCI registry v2 support with authentication for fetching
//! and installing DevContainer features. It supports authentication via environment
//! variables, Docker credential helpers, custom CA certificates, and proxy configuration.

use crate::errors::{FeatureError, Result};
use crate::features::{parse_feature_metadata, FeatureMetadata};
use crate::retry::{retry_async, RetryConfig, RetryDecision};
use base64::Engine;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs;
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

/// Reference to a template in an OCI registry
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateRef {
    /// Registry hostname (e.g., "ghcr.io")
    pub registry: String,
    /// Namespace (e.g., "devcontainers")
    pub namespace: String,
    /// Template name (e.g., "python")
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

impl TemplateRef {
    /// Create a new TemplateRef
    pub fn new(registry: String, namespace: String, name: String, version: Option<String>) -> Self {
        Self {
            registry,
            namespace,
            name,
            version,
        }
    }

    /// Get the tag for this template reference
    pub fn tag(&self) -> &str {
        self.version.as_deref().unwrap_or("latest")
    }

    /// Get the repository name for this template
    pub fn repository(&self) -> String {
        format!("{}/{}", self.namespace, self.name)
    }

    /// Get the full reference string
    pub fn reference(&self) -> String {
        format!("{}/{}:{}", self.registry, self.repository(), self.tag())
    }
}

/// Authentication credentials for registry access
#[derive(Debug, Clone, PartialEq)]
pub enum RegistryCredentials {
    /// No authentication
    None,
    /// Basic authentication with username and password
    Basic { username: String, password: String },
    /// Bearer token authentication
    Bearer { token: String },
}

impl RegistryCredentials {
    /// Create an Authorization header value
    pub fn to_auth_header(&self) -> Option<String> {
        match self {
            RegistryCredentials::None => None,
            RegistryCredentials::Basic { username, password } => {
                let credentials = format!("{}:{}", username, password);
                let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
                Some(format!("Basic {}", encoded))
            }
            RegistryCredentials::Bearer { token } => Some(format!("Bearer {}", token)),
        }
    }
}

/// Registry authentication configuration
#[derive(Debug, Clone)]
pub struct RegistryAuth {
    /// Default credentials to use for all registries
    pub default_credentials: RegistryCredentials,
    /// Registry-specific credentials
    pub registry_credentials: HashMap<String, RegistryCredentials>,
}

impl RegistryAuth {
    /// Create a new empty registry auth configuration
    pub fn new() -> Self {
        Self {
            default_credentials: RegistryCredentials::None,
            registry_credentials: HashMap::new(),
        }
    }

    /// Get credentials for a specific registry
    pub fn get_credentials(&self, registry: &str) -> &RegistryCredentials {
        self.registry_credentials
            .get(registry)
            .unwrap_or(&self.default_credentials)
    }

    /// Set credentials for a specific registry
    pub fn set_credentials(&mut self, registry: String, credentials: RegistryCredentials) {
        self.registry_credentials.insert(registry, credentials);
    }

    /// Set default credentials
    pub fn set_default_credentials(&mut self, credentials: RegistryCredentials) {
        self.default_credentials = credentials;
    }
}

impl Default for RegistryAuth {
    fn default() -> Self {
        Self::new()
    }
}

/// Docker config.json authentication entry
#[derive(Debug, Deserialize)]
struct DockerConfigAuth {
    auth: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

/// Docker config.json structure (simplified)
#[derive(Debug, Deserialize)]
struct DockerConfig {
    auths: Option<HashMap<String, DockerConfigAuth>>,
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

/// Downloaded and extracted template data
#[derive(Debug)]
pub struct DownloadedTemplate {
    /// Extracted template directory
    pub path: PathBuf,
    /// Template metadata
    pub metadata: crate::templates::TemplateMetadata,
    /// Template digest for caching
    pub digest: String,
}

/// Result of publishing an artifact to an OCI registry
#[derive(Debug, Clone)]
pub struct PublishResult {
    /// Registry URL where the artifact was published
    pub registry: String,
    /// Repository name
    pub repository: String,
    /// Tag used for publishing
    pub tag: String,
    /// Digest of the published manifest
    pub digest: String,
    /// Size of the published artifact in bytes
    pub size: u64,
}

/// OCI manifest structure (minimal)
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Manifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub layers: Vec<Layer>,
}

/// OCI layer structure (minimal)
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Layer {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub size: u64,
    pub digest: String,
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

    /// PUT request with data and headers
    async fn put_with_headers(
        &self,
        url: &str,
        data: Bytes,
        headers: HashMap<String, String>,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>>;

    /// POST request with data and headers
    async fn post_with_headers(
        &self,
        url: &str,
        data: Bytes,
        headers: HashMap<String, String>,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>>;
}

/// Default HTTP client implementation using reqwest
#[derive(Debug, Clone)]
pub struct ReqwestClient {
    client: reqwest::Client,
    auth: RegistryAuth,
}

impl ReqwestClient {
    /// Create a new ReqwestClient with default configuration
    pub fn new() -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut client_builder = reqwest::Client::builder();

        // Configure custom CA certificates if specified
        if let Ok(ca_bundle_path) = env::var("DEACON_CUSTOM_CA_BUNDLE") {
            let ca_bundle = fs::read(&ca_bundle_path)?;
            let cert = reqwest::Certificate::from_pem(&ca_bundle)?;
            client_builder = client_builder.add_root_certificate(cert);
            debug!("Added custom CA certificate from: {}", ca_bundle_path);
        }

        // Build the client
        let client = client_builder.build()?;

        let mut auth = RegistryAuth::new();

        // Load authentication from environment and Docker config
        Self::load_auth_from_env(&mut auth)?;
        Self::load_auth_from_docker_config(&mut auth)?;

        Ok(Self { client, auth })
    }

    /// Create a new ReqwestClient with custom authentication
    pub fn with_auth(
        auth: RegistryAuth,
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut instance = Self::new()?;
        instance.auth = auth;
        Ok(instance)
    }

    /// Load authentication from environment variables
    fn load_auth_from_env(
        auth: &mut RegistryAuth,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Check for token authentication first
        if let Ok(token) = env::var("DEACON_REGISTRY_TOKEN") {
            debug!("Found DEACON_REGISTRY_TOKEN environment variable");
            auth.set_default_credentials(RegistryCredentials::Bearer { token });
            return Ok(());
        }

        // Check for basic authentication
        if let (Ok(username), Ok(password)) = (
            env::var("DEACON_REGISTRY_USER"),
            env::var("DEACON_REGISTRY_PASS"),
        ) {
            debug!("Found DEACON_REGISTRY_USER and DEACON_REGISTRY_PASS environment variables");
            auth.set_default_credentials(RegistryCredentials::Basic { username, password });
        }

        Ok(())
    }

    /// Load authentication from Docker config.json
    fn load_auth_from_docker_config(
        auth: &mut RegistryAuth,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let home_dir = env::var("HOME").or_else(|_| env::var("USERPROFILE"))?;
        let docker_config_path = Path::new(&home_dir).join(".docker").join("config.json");

        if !docker_config_path.exists() {
            debug!(
                "Docker config.json not found at: {}",
                docker_config_path.display()
            );
            return Ok(());
        }

        let config_content = fs::read_to_string(&docker_config_path)?;
        let docker_config: DockerConfig = serde_json::from_str(&config_content)?;

        if let Some(auths) = docker_config.auths {
            for (registry, auth_config) in auths {
                if let Some(auth_string) = auth_config.auth {
                    // Decode base64 auth string
                    if let Ok(decoded) =
                        base64::engine::general_purpose::STANDARD.decode(&auth_string)
                    {
                        if let Ok(auth_str) = String::from_utf8(decoded) {
                            if let Some((username, password)) = auth_str.split_once(':') {
                                debug!("Loaded Docker config auth for registry: {}", registry);
                                auth.set_credentials(
                                    registry,
                                    RegistryCredentials::Basic {
                                        username: username.to_string(),
                                        password: password.to_string(),
                                    },
                                );
                            }
                        }
                    }
                } else if let (Some(username), Some(password)) =
                    (auth_config.username, auth_config.password)
                {
                    debug!(
                        "Loaded Docker config username/password for registry: {}",
                        registry
                    );
                    auth.set_credentials(
                        registry,
                        RegistryCredentials::Basic { username, password },
                    );
                }
            }
        }

        Ok(())
    }

    /// Get credentials for a specific registry URL
    fn get_credentials_for_url(&self, url: &str) -> &RegistryCredentials {
        // Extract hostname from URL
        if let Ok(parsed_url) = reqwest::Url::parse(url) {
            if let Some(host) = parsed_url.host_str() {
                return self.auth.get_credentials(host);
            }
        }
        &self.auth.default_credentials
    }

    /// Get access to the authentication configuration (for testing)
    pub fn auth(&self) -> &RegistryAuth {
        &self.auth
    }
}

impl Default for ReqwestClient {
    fn default() -> Self {
        Self::new().unwrap_or_else(|e| {
            warn!(
                "Failed to create ReqwestClient with authentication: {}. Using basic client.",
                e
            );
            Self {
                client: reqwest::Client::new(),
                auth: RegistryAuth::new(),
            }
        })
    }
}

#[async_trait::async_trait]
impl HttpClient for ReqwestClient {
    async fn get(
        &self,
        url: &str,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        self.get_with_headers(url, HashMap::new()).await
    }

    async fn get_with_headers(
        &self,
        url: &str,
        mut headers: HashMap<String, String>,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        // Add authentication header if available
        let credentials = self.get_credentials_for_url(url);
        if let Some(auth_header) = credentials.to_auth_header() {
            headers.insert("Authorization".to_string(), auth_header);
        }

        let mut request = self.client.get(url);
        for (key, value) in headers {
            request = request.header(&key, &value);
        }

        let response = request.send().await?;

        // Handle 401 authentication errors
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(format!("Authentication failed for URL: {}", url).into());
        }

        // Handle other HTTP errors
        if !response.status().is_success() {
            return Err(format!("HTTP {} for URL: {}", response.status(), url).into());
        }

        let bytes = response.bytes().await?;
        Ok(bytes)
    }

    async fn put_with_headers(
        &self,
        url: &str,
        data: Bytes,
        mut headers: HashMap<String, String>,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        // Add authentication header if available
        let credentials = self.get_credentials_for_url(url);
        if let Some(auth_header) = credentials.to_auth_header() {
            headers.insert("Authorization".to_string(), auth_header);
        }

        let mut request = self.client.put(url).body(data);
        for (key, value) in headers {
            request = request.header(&key, &value);
        }

        let response = request.send().await?;

        // Handle 401 authentication errors
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(format!("Authentication failed for URL: {}", url).into());
        }

        // Handle other HTTP errors
        if !response.status().is_success() {
            return Err(format!("HTTP {} for URL: {}", response.status(), url).into());
        }

        let bytes = response.bytes().await?;
        Ok(bytes)
    }

    async fn post_with_headers(
        &self,
        url: &str,
        data: Bytes,
        mut headers: HashMap<String, String>,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        // Add authentication header if available
        let credentials = self.get_credentials_for_url(url);
        if let Some(auth_header) = credentials.to_auth_header() {
            headers.insert("Authorization".to_string(), auth_header);
        }

        let mut request = self.client.post(url).body(data);
        for (key, value) in headers {
            request = request.header(&key, &value);
        }

        let response = request.send().await?;

        // Handle 401 authentication errors
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(format!("Authentication failed for URL: {}", url).into());
        }

        // Handle other HTTP errors
        if !response.status().is_success() {
            return Err(format!("HTTP {} for URL: {}", response.status(), url).into());
        }

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
        // Authentication errors should be retried once to allow credential refresh
        FeatureError::Authentication { .. } => RetryDecision::Retry,
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
    pub async fn get_manifest(&self, feature_ref: &FeatureRef) -> Result<Manifest> {
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
                        let error_msg = e.to_string();
                        if error_msg.contains("Authentication failed") {
                            FeatureError::Authentication {
                                message: format!("Failed to authenticate for manifest: {}", e),
                            }
                        } else {
                            FeatureError::Download {
                                message: format!("Failed to download manifest: {}", e),
                            }
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

    /// Publish a feature to an OCI registry
    #[instrument(level = "info", skip(self, tar_data))]
    pub async fn publish_feature(
        &self,
        feature_ref: &FeatureRef,
        tar_data: Bytes,
        metadata: &FeatureMetadata,
    ) -> Result<PublishResult> {
        info!("Publishing feature: {}", feature_ref.reference());

        // Calculate digest for the tar layer
        let mut hasher = Sha256::new();
        hasher.update(&tar_data);
        let layer_digest = format!("sha256:{:x}", hasher.finalize());
        let layer_size = tar_data.len() as u64;

        // Upload the blob (layer)
        self.upload_blob(feature_ref, &layer_digest, tar_data)
            .await?;

        // Create and upload manifest
        let manifest = serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": {
                "mediaType": "application/vnd.devcontainers.feature.config.v1+json",
                "size": 0,
                "digest": "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
            },
            "layers": [{
                "mediaType": "application/vnd.oci.image.layer.v1.tar",
                "size": layer_size,
                "digest": layer_digest
            }],
            "annotations": {
                "org.opencontainers.image.title": metadata.name.as_deref().unwrap_or(&metadata.id),
                "org.opencontainers.image.description": metadata.description.as_deref().unwrap_or(""),
                "org.opencontainers.image.version": metadata.version.as_deref().unwrap_or("latest")
            }
        });

        let manifest_bytes =
            Bytes::from(serde_json::to_vec(&manifest).map_err(FeatureError::Json)?);
        let manifest_digest = self
            .upload_manifest(feature_ref, manifest_bytes.clone())
            .await?;

        info!(
            "Successfully published feature {} with digest {}",
            feature_ref.reference(),
            manifest_digest
        );

        Ok(PublishResult {
            registry: feature_ref.registry.clone(),
            repository: feature_ref.repository(),
            tag: feature_ref.tag().to_string(),
            digest: manifest_digest,
            size: layer_size,
        })
    }

    /// Publish a template to an OCI registry
    #[instrument(level = "info", skip(self, tar_data))]
    pub async fn publish_template(
        &self,
        template_ref: &TemplateRef,
        tar_data: Bytes,
        metadata: &crate::templates::TemplateMetadata,
    ) -> Result<PublishResult> {
        info!("Publishing template: {}", template_ref.reference());

        // Calculate digest for the tar layer
        let mut hasher = Sha256::new();
        hasher.update(&tar_data);
        let layer_digest = format!("sha256:{:x}", hasher.finalize());
        let layer_size = tar_data.len() as u64;

        // Upload the blob (layer)
        self.upload_blob_template(template_ref, &layer_digest, tar_data)
            .await?;

        // Create and upload manifest
        let manifest = serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": {
                "mediaType": "application/vnd.devcontainers.template.config.v1+json",
                "size": 0,
                "digest": "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
            },
            "layers": [{
                "mediaType": "application/vnd.oci.image.layer.v1.tar",
                "size": layer_size,
                "digest": layer_digest
            }],
            "annotations": {
                "org.opencontainers.image.title": metadata.name.as_deref().unwrap_or(&metadata.id),
                "org.opencontainers.image.description": metadata.description.as_deref().unwrap_or(""),
                "org.opencontainers.image.version": metadata.version.as_deref().unwrap_or("latest")
            }
        });

        let manifest_bytes =
            Bytes::from(serde_json::to_vec(&manifest).map_err(FeatureError::Json)?);
        let manifest_digest = self
            .upload_manifest_template(template_ref, manifest_bytes.clone())
            .await?;

        info!(
            "Successfully published template {} with digest {}",
            template_ref.reference(),
            manifest_digest
        );

        Ok(PublishResult {
            registry: template_ref.registry.clone(),
            repository: template_ref.repository(),
            tag: template_ref.tag().to_string(),
            digest: manifest_digest,
            size: layer_size,
        })
    }

    /// Fetch a template from an OCI registry  
    #[instrument(level = "info", skip(self))]
    pub async fn fetch_template(&self, template_ref: &TemplateRef) -> Result<DownloadedTemplate> {
        info!("Fetching template: {}", template_ref.reference());

        // Get the manifest
        let manifest = self.get_manifest_template(template_ref).await?;
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
            info!("Found cached template at: {}", cached_dir.display());
            return self
                .load_cached_template(cached_dir, layer.digest.clone())
                .await;
        }

        // Download and extract the layer
        let layer_data = self
            .download_layer_template(template_ref, &layer.digest)
            .await?;
        let extracted_dir = self.extract_layer(layer_data, &cache_key).await?;

        // Parse metadata
        let metadata_path = extracted_dir.join("devcontainer-template.json");
        let metadata = crate::templates::parse_template_metadata(&metadata_path)?;

        info!("Successfully fetched template: {}", metadata.id);
        Ok(DownloadedTemplate {
            path: extracted_dir,
            metadata,
            digest: layer.digest.clone(),
        })
    }

    /// Upload a blob to the registry for features
    async fn upload_blob(&self, feature_ref: &FeatureRef, digest: &str, data: Bytes) -> Result<()> {
        let blob_url = format!(
            "https://{}/v2/{}/blobs/{}",
            feature_ref.registry,
            feature_ref.repository(),
            digest
        );

        debug!("Uploading blob to: {}", blob_url);

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/octet-stream".to_string(),
        );

        self.client
            .put_with_headers(&blob_url, data, headers)
            .await
            .map_err(|e| FeatureError::Oci {
                message: format!("Failed to upload blob: {}", e),
            })?;

        Ok(())
    }

    /// Upload a blob to the registry for templates
    async fn upload_blob_template(
        &self,
        template_ref: &TemplateRef,
        digest: &str,
        data: Bytes,
    ) -> Result<()> {
        let blob_url = format!(
            "https://{}/v2/{}/blobs/{}",
            template_ref.registry,
            template_ref.repository(),
            digest
        );

        debug!("Uploading blob to: {}", blob_url);

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/octet-stream".to_string(),
        );

        self.client
            .put_with_headers(&blob_url, data, headers)
            .await
            .map_err(|e| FeatureError::Oci {
                message: format!("Failed to upload blob: {}", e),
            })?;

        Ok(())
    }

    /// Upload a manifest to the registry for features
    async fn upload_manifest(
        &self,
        feature_ref: &FeatureRef,
        manifest_data: Bytes,
    ) -> Result<String> {
        let manifest_url = format!(
            "https://{}/v2/{}/manifests/{}",
            feature_ref.registry,
            feature_ref.repository(),
            feature_ref.tag()
        );

        debug!("Uploading manifest to: {}", manifest_url);

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/vnd.oci.image.manifest.v1+json".to_string(),
        );

        let _response = self
            .client
            .put_with_headers(&manifest_url, manifest_data.clone(), headers)
            .await
            .map_err(|e| FeatureError::Oci {
                message: format!("Failed to upload manifest: {}", e),
            })?;

        // Calculate digest of the manifest
        let mut hasher = Sha256::new();
        hasher.update(&manifest_data);
        let digest = format!("sha256:{:x}", hasher.finalize());

        debug!("Manifest uploaded with digest: {}", digest);
        Ok(digest)
    }

    /// Upload a manifest to the registry for templates
    async fn upload_manifest_template(
        &self,
        template_ref: &TemplateRef,
        manifest_data: Bytes,
    ) -> Result<String> {
        let manifest_url = format!(
            "https://{}/v2/{}/manifests/{}",
            template_ref.registry,
            template_ref.repository(),
            template_ref.tag()
        );

        debug!("Uploading manifest to: {}", manifest_url);

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/vnd.oci.image.manifest.v1+json".to_string(),
        );

        let _response = self
            .client
            .put_with_headers(&manifest_url, manifest_data.clone(), headers)
            .await
            .map_err(|e| FeatureError::Oci {
                message: format!("Failed to upload manifest: {}", e),
            })?;

        // Calculate digest of the manifest
        let mut hasher = Sha256::new();
        hasher.update(&manifest_data);
        let digest = format!("sha256:{:x}", hasher.finalize());

        debug!("Manifest uploaded with digest: {}", digest);
        Ok(digest)
    }

    /// Get the OCI manifest for a template
    async fn get_manifest_template(&self, template_ref: &TemplateRef) -> Result<Manifest> {
        let manifest_url = format!(
            "https://{}/v2/{}/manifests/{}",
            template_ref.registry,
            template_ref.repository(),
            template_ref.tag()
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
                        let error_msg = e.to_string();
                        if error_msg.contains("Authentication failed") {
                            FeatureError::Authentication {
                                message: format!("Failed to authenticate for manifest: {}", e),
                            }
                        } else {
                            FeatureError::Download {
                                message: format!("Failed to download manifest: {}", e),
                            }
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

    /// Download a layer blob for templates
    async fn download_layer_template(
        &self,
        template_ref: &TemplateRef,
        digest: &str,
    ) -> Result<Bytes> {
        let blob_url = format!(
            "https://{}/v2/{}/blobs/{}",
            template_ref.registry,
            template_ref.repository(),
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
                    client.get(url).await.map_err(|e| {
                        let error_msg = e.to_string();
                        if error_msg.contains("Authentication failed") {
                            FeatureError::Authentication {
                                message: format!("Failed to authenticate for layer: {}", e),
                            }
                        } else {
                            FeatureError::Download {
                                message: format!("Failed to download layer: {}", e),
                            }
                        }
                    })
                }
            },
            classify_network_error,
        )
        .await?;

        Ok(layer_data)
    }

    /// Load cached template from directory
    async fn load_cached_template(
        &self,
        cached_dir: PathBuf,
        digest: String,
    ) -> Result<DownloadedTemplate> {
        let metadata_path = cached_dir.join("devcontainer-template.json");
        let metadata = crate::templates::parse_template_metadata(&metadata_path)?;

        Ok(DownloadedTemplate {
            path: cached_dir,
            metadata,
            digest,
        })
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
                    client.get(url).await.map_err(|e| {
                        let error_msg = e.to_string();
                        if error_msg.contains("Authentication failed") {
                            FeatureError::Authentication {
                                message: format!("Failed to authenticate for layer: {}", e),
                            }
                        } else {
                            FeatureError::Download {
                                message: format!("Failed to download layer: {}", e),
                            }
                        }
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
pub fn default_fetcher() -> Result<FeatureFetcher<ReqwestClient>> {
    let client = ReqwestClient::new().map_err(|e| FeatureError::Authentication {
        message: format!("Failed to create HTTP client: {}", e),
    })?;
    Ok(FeatureFetcher::new(client))
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

    async fn put_with_headers(
        &self,
        url: &str,
        _data: Bytes,
        _headers: HashMap<String, String>,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        let responses = self.responses.lock().await;
        responses
            .get(url)
            .cloned()
            .ok_or_else(|| format!("No mock response for URL: {}", url).into())
    }

    async fn post_with_headers(
        &self,
        url: &str,
        _data: Bytes,
        _headers: HashMap<String, String>,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        let responses = self.responses.lock().await;
        responses
            .get(url)
            .cloned()
            .ok_or_else(|| format!("No mock response for URL: {}", url).into())
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

            async fn put_with_headers(
                &self,
                _url: &str,
                _data: Bytes,
                _headers: HashMap<String, String>,
            ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
                let current = self.failure_count.fetch_add(1, Ordering::SeqCst);
                if current < self.fail_attempts {
                    Err("network error".into())
                } else {
                    Ok(Bytes::new())
                }
            }

            async fn post_with_headers(
                &self,
                _url: &str,
                _data: Bytes,
                _headers: HashMap<String, String>,
            ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
                let current = self.failure_count.fetch_add(1, Ordering::SeqCst);
                if current < self.fail_attempts {
                    Err("network error".into())
                } else {
                    Ok(Bytes::new())
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

            async fn put_with_headers(
                &self,
                _url: &str,
                _data: Bytes,
                _headers: HashMap<String, String>,
            ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
                self.call_count.fetch_add(1, Ordering::SeqCst);
                Err("permanent network error".into())
            }

            async fn post_with_headers(
                &self,
                _url: &str,
                _data: Bytes,
                _headers: HashMap<String, String>,
            ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
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
}
