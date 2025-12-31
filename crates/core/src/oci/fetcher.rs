//! Feature fetcher for OCI registries

use bytes::Bytes;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tar::Archive;
use tokio::sync::Mutex;
use tracing::{debug, info, instrument, warn};

use crate::errors::{FeatureError, Result};
use crate::features::{parse_feature_metadata, FeatureMetadata};
use crate::progress::{ProgressEvent, ProgressTracker};
use crate::retry::{retry_async, RetryConfig};

use super::client::{HttpClient, ReqwestClient};
use super::types::{
    DownloadedFeature, DownloadedTemplate, FeatureRef, Manifest, PublishResult, TagList,
    TemplateRef,
};
use super::utils::{classify_network_error, get_features_cache_dir};

/// Feature fetcher for OCI registries
pub struct FeatureFetcher<C: HttpClient> {
    client: C,
    pub cache_dir: PathBuf,
    retry_config: RetryConfig,
    progress_tracker: Arc<Mutex<Option<ProgressTracker>>>,
}
impl<C: HttpClient> FeatureFetcher<C> {
    /// Create a new FeatureFetcher with custom HTTP client
    pub fn new(client: C) -> Self {
        let cache_dir = get_features_cache_dir().unwrap_or_else(|_| {
            // Fallback to temp directory if persistent cache fails
            std::env::temp_dir().join("deacon-features")
        });
        Self {
            client,
            cache_dir,
            retry_config: RetryConfig::default(),
            progress_tracker: Arc::new(Mutex::new(None)),
        }
    }

    /// Create a new FeatureFetcher with custom cache directory
    pub fn with_cache_dir(client: C, cache_dir: PathBuf) -> Self {
        Self {
            client,
            cache_dir,
            retry_config: RetryConfig::default(),
            progress_tracker: Arc::new(Mutex::new(None)),
        }
    }

    /// Create a new FeatureFetcher with custom retry configuration
    pub fn with_retry_config(client: C, cache_dir: PathBuf, retry_config: RetryConfig) -> Self {
        Self {
            client,
            cache_dir,
            retry_config,
            progress_tracker: Arc::new(Mutex::new(None)),
        }
    }

    /// Set progress tracker for emitting events
    pub async fn set_progress_tracker(&self, tracker: ProgressTracker) {
        let mut progress = self.progress_tracker.lock().await;
        *progress = Some(tracker);
    }

    /// Emit a progress event if a tracker is configured
    async fn emit_progress_event(&self, event: ProgressEvent) {
        if let Some(ref mut tracker) = self.progress_tracker.lock().await.as_mut() {
            if let Err(e) = tracker.emit_event(event) {
                warn!("Failed to emit progress event: {}", e);
            }
        }
    }

    /// Get a reference to the HTTP client (for testing)
    pub fn client(&self) -> &C {
        &self.client
    }

    /// Fetch a feature from an OCI registry
    #[instrument(level = "info", skip(self))]
    pub async fn fetch_feature(&self, feature_ref: &FeatureRef) -> Result<DownloadedFeature> {
        let start_time = Instant::now();
        let event_id =
            crate::progress::EVENT_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Emit fetch begin event
        self.emit_progress_event(ProgressEvent::OciFetchBegin {
            id: event_id,
            timestamp,
            registry: feature_ref.registry.clone(),
            repository: feature_ref.repository(),
            tag: feature_ref.tag().to_string(),
        })
        .await;

        info!("Fetching feature: {}", feature_ref.reference());

        let result = async {
            // Get the manifest
            let manifest = self.get_manifest(feature_ref).await.map_err(|e| match e {
                crate::errors::DeaconError::Feature(f) => f,
                _ => FeatureError::Oci {
                    message: format!("Get manifest error: {}", e),
                },
            })?;
            debug!("Got manifest with {} layers", manifest.layers.len());

            // For now, assume single tar layer (as per requirements)
            if manifest.layers.is_empty() {
                return Err(FeatureError::Oci {
                    message: "No layers found in manifest".to_string(),
                });
            }

            let layer = &manifest.layers[0];
            debug!("Using layer: digest={}, size={}", layer.digest, layer.size);

            // Check cache first
            let cache_key = self.get_cache_key(&layer.digest);
            let cached_dir = self.cache_dir.join(&cache_key);

            let is_cached = cached_dir.exists();
            if is_cached {
                info!("Found cached feature at: {}", cached_dir.display());
                let feature = self
                    .load_cached_feature(cached_dir, layer.digest.clone())
                    .await
                    .map_err(|e| match e {
                        crate::errors::DeaconError::Feature(f) => f,
                        _ => FeatureError::Oci {
                            message: format!("Cache error: {}", e),
                        },
                    })?;
                return Ok((feature, is_cached));
            }

            // Download and extract the layer
            let layer_data = self
                .download_layer(feature_ref, &layer.digest)
                .await
                .map_err(|e| match e {
                    crate::errors::DeaconError::Feature(f) => f,
                    _ => FeatureError::Oci {
                        message: format!("Download error: {}", e),
                    },
                })?;
            let extracted_dir = self
                .extract_layer(layer_data, &cache_key)
                .await
                .map_err(|e| match e {
                    crate::errors::DeaconError::Feature(f) => f,
                    _ => FeatureError::Oci {
                        message: format!("Extract error: {}", e),
                    },
                })?;

            // Parse metadata
            let metadata_path = extracted_dir.join("devcontainer-feature.json");
            let metadata = parse_feature_metadata(&metadata_path).map_err(|e| match e {
                crate::errors::DeaconError::Feature(f) => f,
                _ => FeatureError::Oci {
                    message: format!("Metadata parse error: {}", e),
                },
            })?;

            // Validate metadata before use
            metadata.validate()?;

            info!("Successfully fetched feature: {}", metadata.id);
            Ok((
                DownloadedFeature {
                    path: extracted_dir,
                    metadata,
                    digest: layer.digest.clone(),
                },
                is_cached,
            ))
        }
        .await;

        // Emit fetch end event
        let end_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let duration_ms = start_time.elapsed().as_millis() as u64;

        self.emit_progress_event(ProgressEvent::OciFetchEnd {
            id: event_id,
            timestamp: end_timestamp,
            registry: feature_ref.registry.clone(),
            repository: feature_ref.repository(),
            tag: feature_ref.tag().to_string(),
            duration_ms,
            success: result.is_ok(),
            cached: result.as_ref().map(|(_, cached)| *cached).unwrap_or(false),
        })
        .await;

        result.map(|(feature, _)| feature).map_err(Into::into)
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

    /// Get the OCI manifest for a feature with SHA256 digest of the raw body
    ///
    /// Returns both the parsed manifest JSON and the SHA256 hex digest of the raw manifest body.
    /// This is useful for computing canonical IDs and verifying manifest integrity.
    pub async fn get_manifest_with_digest(
        &self,
        feature_ref: &FeatureRef,
    ) -> Result<(serde_json::Value, String)> {
        let manifest_url = format!(
            "https://{}/v2/{}/manifests/{}",
            feature_ref.registry,
            feature_ref.repository(),
            feature_ref.tag()
        );

        debug!("Fetching manifest with digest from: {}", manifest_url);

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

        // Compute SHA256 digest of the raw manifest body
        let mut hasher = Sha256::new();
        hasher.update(&manifest_data);
        let digest = format!("{:x}", hasher.finalize());

        // Parse the manifest JSON
        let manifest: serde_json::Value =
            serde_json::from_slice(&manifest_data).map_err(|e| FeatureError::Parsing {
                message: format!("Failed to parse manifest: {}", e),
            })?;

        debug!("Manifest fetched with digest: {}", digest);
        Ok((manifest, digest))
    }

    /// Publish a feature to an OCI registry
    #[instrument(level = "info", skip(self, tar_data))]
    pub async fn publish_feature(
        &self,
        feature_ref: &FeatureRef,
        tar_data: Bytes,
        metadata: &FeatureMetadata,
    ) -> Result<PublishResult> {
        let start_time = Instant::now();
        let event_id =
            crate::progress::EVENT_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Emit publish begin event
        self.emit_progress_event(ProgressEvent::OciPublishBegin {
            id: event_id,
            timestamp,
            registry: feature_ref.registry.clone(),
            repository: feature_ref.repository(),
            tag: feature_ref.tag().to_string(),
        })
        .await;

        info!("Publishing feature: {}", feature_ref.reference());

        let result = async {
            // Calculate digest for the tar layer
            let mut hasher = Sha256::new();
            hasher.update(&tar_data);
            let layer_digest = format!("sha256:{:x}", hasher.finalize());
            let layer_size = tar_data.len() as u64;

            // Upload the blob (layer)
            self.upload_blob(feature_ref, &layer_digest, tar_data)
                .await
                .map_err(|e| match e {
                    crate::errors::DeaconError::Feature(f) => f,
                    _ => FeatureError::Oci { message: format!("Upload blob error: {}", e) },
                })?;

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
                .await
                .map_err(|e| match e {
                    crate::errors::DeaconError::Feature(f) => f,
                    _ => FeatureError::Oci { message: format!("Upload manifest error: {}", e) },
                })?;

            info!(
                "Successfully published feature {} with digest {}",
                feature_ref.reference(),
                manifest_digest
            );

            Ok::<_, crate::errors::FeatureError>(PublishResult {
                registry: feature_ref.registry.clone(),
                repository: feature_ref.repository(),
                tag: feature_ref.tag().to_string(),
                digest: manifest_digest.clone(),
                size: layer_size,
            })
        }.await;

        // Emit publish end event
        let end_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let duration_ms = start_time.elapsed().as_millis() as u64;

        self.emit_progress_event(ProgressEvent::OciPublishEnd {
            id: event_id,
            timestamp: end_timestamp,
            registry: feature_ref.registry.clone(),
            repository: feature_ref.repository(),
            tag: feature_ref.tag().to_string(),
            duration_ms,
            success: result.is_ok(),
            digest: result.as_ref().ok().map(|r| r.digest.clone()),
        })
        .await;

        result.map_err(Into::into)
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

    /// Upload a blob to any registry with a generic reference
    /// Follows OCI Distribution Spec v2: POST to initiate, PUT to complete
    async fn upload_blob_generic(
        &self,
        registry: &str,
        repository: &str,
        digest: &str,
        data: Bytes,
    ) -> Result<()> {
        debug!(
            "Uploading blob to registry: {}, repository: {}, digest: {}, size: {} bytes",
            registry,
            repository,
            digest,
            data.len()
        );

        // Step 1: Check if blob already exists using HEAD request
        let blob_check_url = format!("https://{}/v2/{}/blobs/{}", registry, repository, digest);

        match self.client.head(&blob_check_url, HashMap::new()).await {
            Ok(status) => {
                if status == 200 {
                    debug!(
                        "Blob already exists in registry (status {}), skipping upload",
                        status
                    );
                    return Ok(());
                } else if status == 404 {
                    debug!("Blob not found in registry (status 404), proceeding with upload");
                } else if status == 401 || status == 403 {
                    return Err(FeatureError::Authentication {
                        message: format!(
                            "Authentication failed when checking blob existence (status {})",
                            status
                        ),
                    }
                    .into());
                } else if status >= 500 {
                    return Err(FeatureError::Oci {
                        message: format!(
                            "Registry server error when checking blob existence (status {})",
                            status
                        ),
                    }
                    .into());
                } else {
                    debug!(
                        "Unexpected status {} when checking blob existence, proceeding with upload",
                        status
                    );
                }
            }
            Err(e) => {
                // If HEAD request fails, log and proceed with upload anyway
                debug!("HEAD request failed: {}, proceeding with upload", e);
            }
        }

        // Step 2: Initiate upload session (POST to /v2/{repo}/blobs/uploads/)
        let upload_url = format!("https://{}/v2/{}/blobs/uploads/", registry, repository);
        debug!("Initiating upload session at: {}", upload_url);

        let empty_body = Bytes::new();
        let response = self
            .client
            .post_with_headers(&upload_url, empty_body, HashMap::new())
            .await
            .map_err(|e| FeatureError::Oci {
                message: format!("Failed to initiate blob upload: {}", e),
            })?;

        // Extract Location header from POST response per OCI spec
        let location = response
            .headers
            .get("location")
            .or_else(|| response.headers.get("Location"))
            .ok_or_else(|| FeatureError::Oci {
                message: "Missing Location header in upload initiation response".to_string(),
            })?
            .clone();

        debug!("Upload session initiated, location: {}", location);

        // Build final upload URL by appending digest query parameter to Location
        let upload_location = if location.contains('?') {
            format!("{}&digest={}", location, digest)
        } else {
            format!("{}?digest={}", location, digest)
        };

        debug!("Uploading blob to: {}", upload_location);

        // Step 3: Upload blob with monolithic PUT (entire blob in one request)
        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/octet-stream".to_string(),
        );
        headers.insert("Content-Length".to_string(), data.len().to_string());

        // Retry blob upload with exponential backoff
        retry_async(
            &self.retry_config,
            || {
                let client = &self.client;
                let url = &upload_location;
                let data_clone = data.clone();
                let headers_clone = headers.clone();
                async move {
                    client
                        .put_with_headers(url, data_clone, headers_clone)
                        .await
                        .map_err(|e| {
                            let error_msg = e.to_string();
                            if error_msg.contains("Authentication failed") {
                                FeatureError::Authentication {
                                    message: format!(
                                        "Failed to authenticate for blob upload: {}",
                                        e
                                    ),
                                }
                            } else {
                                FeatureError::Oci {
                                    message: format!("Failed to upload blob: {}", e),
                                }
                            }
                        })
                }
            },
            classify_network_error,
        )
        .await?;

        debug!("Blob uploaded successfully: {}", digest);
        Ok(())
    }

    /// Upload a blob to the registry for features
    async fn upload_blob(&self, feature_ref: &FeatureRef, digest: &str, data: Bytes) -> Result<()> {
        self.upload_blob_generic(
            &feature_ref.registry,
            &feature_ref.repository(),
            digest,
            data,
        )
        .await
    }

    /// Upload a blob to the registry for templates
    async fn upload_blob_template(
        &self,
        template_ref: &TemplateRef,
        digest: &str,
        data: Bytes,
    ) -> Result<()> {
        self.upload_blob_generic(
            &template_ref.registry,
            &template_ref.repository(),
            digest,
            data,
        )
        .await
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

        // Retry manifest upload with exponential backoff
        retry_async(
            &self.retry_config,
            || {
                let client = &self.client;
                let url = &manifest_url;
                let data = manifest_data.clone();
                let headers = headers.clone();
                async move {
                    client
                        .put_with_headers(url, data, headers)
                        .await
                        .map_err(|e| {
                            let error_msg = e.to_string();
                            if error_msg.contains("Authentication failed") {
                                FeatureError::Authentication {
                                    message: format!(
                                        "Failed to authenticate for manifest upload: {}",
                                        e
                                    ),
                                }
                            } else {
                                FeatureError::Oci {
                                    message: format!("Failed to upload manifest: {}", e),
                                }
                            }
                        })
                }
            },
            classify_network_error,
        )
        .await?;

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

        // Retry manifest upload with exponential backoff
        retry_async(
            &self.retry_config,
            || {
                let client = &self.client;
                let url = &manifest_url;
                let data = manifest_data.clone();
                let headers = headers.clone();
                async move {
                    client
                        .put_with_headers(url, data, headers)
                        .await
                        .map_err(|e| {
                            let error_msg = e.to_string();
                            if error_msg.contains("Authentication failed") {
                                FeatureError::Authentication {
                                    message: format!(
                                        "Failed to authenticate for manifest upload: {}",
                                        e
                                    ),
                                }
                            } else {
                                FeatureError::Oci {
                                    message: format!("Failed to upload manifest: {}", e),
                                }
                            }
                        })
                }
            },
            classify_network_error,
        )
        .await?;

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

        // Validate metadata before use
        metadata.validate()?;

        Ok(DownloadedFeature {
            path: cached_dir,
            metadata,
            digest,
        })
    }

    /// Parse the Link header to extract the next URL for pagination
    /// Link headers typically look like: `<url>; rel="next"`
    fn parse_next_link_from_headers(headers: &HashMap<String, String>) -> Option<String> {
        let link_header = headers.get("Link").or_else(|| headers.get("link"))?;

        // Parse Link header format: <url>; rel="next", <url>; rel="last"
        for link_part in link_header.split(',') {
            let link_part = link_part.trim();

            // Check if this part contains rel="next"
            if link_part.contains("rel=\"next\"") {
                // Extract the URL from <url>
                if let Some(start) = link_part.find('<') {
                    if let Some(end) = link_part.find('>') {
                        if start < end {
                            let url = &link_part[start + 1..end];
                            return Some(url.to_string());
                        }
                    }
                }
            }
        }

        None
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
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::process::Command;

        debug!("Executing install script: {}", script_path.display());

        let mut command = Command::new("bash");
        command.arg(script_path);

        // Set environment variables
        for (key, value) in env_vars {
            command.env(key, value);
            debug!("Set environment variable: {}={}", key, value);
        }

        // Capture stdout and stderr for streaming
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());

        // Spawn the child process
        let mut child = command.spawn().map_err(|e| FeatureError::Installation {
            message: format!("Failed to execute install script: {}", e),
        })?;

        // Get stdout and stderr handles
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| FeatureError::Installation {
                message: "Failed to capture stdout".to_string(),
            })?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| FeatureError::Installation {
                message: "Failed to capture stderr".to_string(),
            })?;

        // Stream stdout asynchronously
        let stdout_handle = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                info!("[install] stdout: {}", line);
            }
        });

        // Stream stderr asynchronously
        let stderr_handle = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                warn!("[install] stderr: {}", line);
            }
        });

        // Wait for the process to complete
        let status = child.wait().await.map_err(|e| FeatureError::Installation {
            message: format!("Failed to wait for install script: {}", e),
        })?;

        // Wait for output streaming tasks to complete
        let _ = tokio::try_join!(stdout_handle, stderr_handle);

        // Check exit status
        if !status.success() {
            return Err(FeatureError::Installation {
                message: format!(
                    "Install script failed with exit code: {}",
                    status.code().unwrap_or(-1)
                ),
            }
            .into());
        }

        debug!("Install script completed successfully");
        Ok(())
    }

    /// List tags for a repository with Link header pagination
    /// Implements OCI Distribution Spec `/v2/<name>/tags/list` endpoint
    /// Enforces: max 10 pages, max 1000 tags total
    #[instrument(level = "info", skip(self))]
    pub async fn list_tags(&self, feature_ref: &FeatureRef) -> Result<Vec<String>> {
        const MAX_PAGES: usize = 10;
        const MAX_TAGS: usize = 1000;

        let initial_url = format!(
            "https://{}/v2/{}/tags/list",
            feature_ref.registry,
            feature_ref.repository()
        );

        debug!("Fetching tags from: {}", initial_url);

        let mut headers = HashMap::new();
        headers.insert("Accept".to_string(), "application/json".to_string());

        let mut all_tags: Vec<String> = Vec::new();
        let mut page_count = 0;
        let mut current_url = initial_url.clone();

        loop {
            // Check if we've reached page limit
            if page_count >= MAX_PAGES {
                debug!("Reached maximum page limit ({} pages)", MAX_PAGES);
                break;
            }

            // Fetch current page
            let response = retry_async(
                &self.retry_config,
                || {
                    let client = &self.client;
                    let url = &current_url;
                    let headers = headers.clone();
                    async move {
                        client
                            .get_with_headers_and_response(url, headers)
                            .await
                            .map_err(|e| {
                                let error_msg = e.to_string();
                                if error_msg.contains("Authentication failed") {
                                    FeatureError::Authentication {
                                        message: format!(
                                            "Failed to authenticate for tags list: {}",
                                            e
                                        ),
                                    }
                                } else {
                                    FeatureError::Download {
                                        message: format!("Failed to download tags list: {}", e),
                                    }
                                }
                            })
                    }
                },
                classify_network_error,
            )
            .await?;

            // Parse tags from response
            let tag_list: TagList =
                serde_json::from_slice(&response.body).map_err(|e| FeatureError::Parsing {
                    message: format!("Failed to parse tags list: {}", e),
                })?;

            // Add tags to collection, but check for limit
            for tag in tag_list.tags {
                if all_tags.len() >= MAX_TAGS {
                    debug!("Reached maximum tag limit ({})", MAX_TAGS);
                    break;
                }
                all_tags.push(tag);
            }

            page_count += 1;

            // Check for Link header to get next page URL
            match Self::parse_next_link_from_headers(&response.headers) {
                Some(next_url) => {
                    debug!(
                        "Found next page link (page {}), fetching: {}",
                        page_count, next_url
                    );
                    current_url = next_url;
                }
                None => {
                    debug!(
                        "No more pages available (pagination ended at page {})",
                        page_count
                    );
                    break;
                }
            }

            // Stop if we've already hit the tag limit
            if all_tags.len() >= MAX_TAGS {
                break;
            }
        }

        // Remove duplicates while preserving insertion order
        let mut seen = std::collections::HashSet::new();
        all_tags.retain(|tag| seen.insert(tag.clone()));

        debug!(
            "Successfully fetched {} tags across {} pages",
            all_tags.len(),
            page_count
        );

        Ok(all_tags)
    }

    /// Get the OCI manifest for a feature by digest
    /// Allows fetching specific manifest versions using their digest
    #[instrument(level = "info", skip(self))]
    pub async fn get_manifest_by_digest(
        &self,
        feature_ref: &FeatureRef,
        digest: &str,
    ) -> Result<Manifest> {
        let manifest_url = format!(
            "https://{}/v2/{}/manifests/{}",
            feature_ref.registry,
            feature_ref.repository(),
            digest
        );

        debug!("Fetching manifest by digest from: {}", manifest_url);

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

    /// Publish a feature with multiple tags
    /// This is more efficient than calling publish_feature multiple times
    ///
    /// # Error Handling
    ///
    /// - If a tag already exists (manifest found), it is skipped with a log message
    /// - If manifest check returns 404 (not found), the tag is published
    /// - If manifest check returns other errors (network, auth, 5xx), the error is propagated
    #[instrument(level = "info", skip(self, tar_data))]
    pub async fn publish_feature_multi_tag(
        &self,
        registry: String,
        namespace: String,
        name: String,
        tags: Vec<String>,
        tar_data: Bytes,
        metadata: &FeatureMetadata,
    ) -> Result<Vec<PublishResult>> {
        info!(
            "Publishing feature {}/{} with {} tags",
            namespace,
            name,
            tags.len()
        );

        let mut results = Vec::new();

        for tag in tags {
            let feature_ref = FeatureRef::new(
                registry.clone(),
                namespace.clone(),
                name.clone(),
                Some(tag.clone()),
            );

            // Check if tag already exists by trying to fetch manifest
            match self.get_manifest(&feature_ref).await {
                Ok(_) => {
                    info!("Tag {} already exists, skipping", tag);
                    continue;
                }
                Err(e) => {
                    // Inspect the error to determine if it's a 404 (tag doesn't exist)
                    // or a different error (network, auth, etc.)
                    let error_msg = e.to_string().to_lowercase();

                    // Check if this is a "not found" error (404)
                    // Common patterns: "404", "not found", "no such"
                    let is_not_found = error_msg.contains("404")
                        || error_msg.contains("not found")
                        || error_msg.contains("no such");

                    if is_not_found {
                        // Tag doesn't exist, proceed with publishing
                        debug!("Tag {} doesn't exist, publishing", tag);
                    } else {
                        // This is a different error (network, auth, 5xx, etc.)
                        // Propagate it instead of continuing
                        warn!("Failed to check if tag {} exists: {}", tag, e);
                        return Err(e);
                    }
                }
            }

            let result = self
                .publish_feature(&feature_ref, tar_data.clone(), metadata)
                .await?;
            results.push(result);
        }

        info!(
            "Successfully published {} tags for {}/{}",
            results.len(),
            namespace,
            name
        );
        Ok(results)
    }

    /// Publish collection metadata as an OCI artifact
    ///
    /// This publishes the devcontainer-collection.json as an OCI artifact to the
    /// collection repository `<registry>/<namespace>` with tag `collection`.
    /// The collection JSON is stored as the config blob with media type
    /// `application/vnd.devcontainer.collection+json`.
    ///
    /// # Arguments
    /// * `registry` - The registry hostname (e.g., "ghcr.io")
    /// * `namespace` - The namespace/repository path (e.g., "owner/repo")
    /// * `collection_json` - The collection metadata as JSON bytes
    ///
    /// # Returns
    /// The digest of the published manifest
    #[instrument(level = "info", skip(self, collection_json))]
    pub async fn publish_collection_metadata(
        &self,
        registry: &str,
        namespace: &str,
        collection_json: Bytes,
    ) -> Result<String> {
        info!(
            "Publishing collection metadata to {}/{}",
            registry, namespace
        );

        // Calculate digest for the collection JSON
        let mut hasher = Sha256::new();
        hasher.update(&collection_json);
        let config_digest = format!("sha256:{:x}", hasher.finalize());
        let config_size = collection_json.len() as u64;

        // Upload the collection JSON as a blob
        self.upload_blob_generic(registry, namespace, &config_digest, collection_json)
            .await
            .map_err(|e| match e {
                crate::errors::DeaconError::Feature(f) => f,
                _ => FeatureError::Oci {
                    message: format!("Upload collection blob error: {}", e),
                },
            })?;

        // Create manifest for the collection artifact
        let manifest = serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": {
                "mediaType": "application/vnd.devcontainer.collection+json",
                "size": config_size,
                "digest": config_digest
            },
            "layers": [],
            "annotations": {
                "org.opencontainers.image.title": "DevContainer Collection",
                "org.opencontainers.image.description": "DevContainer feature and template collection metadata"
            }
        });

        let manifest_bytes =
            Bytes::from(serde_json::to_vec(&manifest).map_err(FeatureError::Json)?);

        // Upload manifest to the collection tag
        let manifest_url = format!("https://{}/v2/{}/manifests/collection", registry, namespace);

        debug!("Uploading collection manifest to: {}", manifest_url);

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/vnd.oci.image.manifest.v1+json".to_string(),
        );

        // Retry manifest upload with exponential backoff
        retry_async(
            &self.retry_config,
            || {
                let client = &self.client;
                let url = &manifest_url;
                let data = manifest_bytes.clone();
                let headers = headers.clone();
                async move {
                    client
                        .put_with_headers(url, data, headers)
                        .await
                        .map_err(|e| {
                            let error_msg = e.to_string();
                            if error_msg.contains("Authentication failed") {
                                FeatureError::Authentication {
                                    message: format!(
                                        "Failed to authenticate for collection manifest upload: {}",
                                        e
                                    ),
                                }
                            } else {
                                FeatureError::Oci {
                                    message: format!("Failed to upload collection manifest: {}", e),
                                }
                            }
                        })
                }
            },
            classify_network_error,
        )
        .await?;

        // Calculate digest of the manifest
        let mut hasher = Sha256::new();
        hasher.update(&manifest_bytes);
        let manifest_digest = format!("sha256:{:x}", hasher.finalize());

        info!(
            "Successfully published collection metadata with digest {}",
            manifest_digest
        );
        Ok(manifest_digest)
    }
}

/// Convenience function to create a default feature fetcher
pub fn default_fetcher() -> Result<FeatureFetcher<ReqwestClient>> {
    let client = ReqwestClient::new().map_err(|e| FeatureError::Authentication {
        message: format!("Failed to create HTTP client: {}", e),
    })?;
    Ok(FeatureFetcher::new(client))
}

/// Create a feature fetcher with custom timeout and retry configuration
///
/// This function is useful for operations that need predictable performance guarantees,
/// such as the read-configuration command which should minimize latency.
///
/// # Arguments
/// * `timeout` - Timeout for each HTTP request (e.g., 2 seconds)
/// * `retry_config` - Retry configuration (max attempts, backoff delays)
///
/// # Examples
/// ```
/// use deacon_core::oci::default_fetcher_with_config;
/// use deacon_core::retry::RetryConfig;
/// use std::time::Duration;
///
/// // Create fetcher with 2s timeout and 1 retry
/// let retry_config = RetryConfig::new(
///     1, // max_attempts (1 retry after initial attempt)
///     Duration::from_millis(100), // base_delay
///     Duration::from_secs(1), // max_delay
///     deacon_core::retry::JitterStrategy::FullJitter,
/// );
/// let fetcher = default_fetcher_with_config(
///     Some(Duration::from_secs(2)),
///     retry_config,
/// );
/// ```
pub fn default_fetcher_with_config(
    timeout: Option<std::time::Duration>,
    retry_config: RetryConfig,
) -> Result<FeatureFetcher<ReqwestClient>> {
    let client =
        ReqwestClient::with_timeout(timeout).map_err(|e| FeatureError::Authentication {
            message: format!("Failed to create HTTP client with timeout: {}", e),
        })?;

    let cache_dir = get_features_cache_dir()?;
    Ok(FeatureFetcher::with_retry_config(
        client,
        cache_dir,
        retry_config,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    // Note: These tests execute bash scripts directly on the host.
    // DevContainer features are Linux-only (install.sh runs inside Linux containers).
    // See: https://containers.dev/implementors/features/
    // Integration tests with Docker test the full container flow.

    #[tokio::test]
    #[cfg(unix)] // Bash script execution - Linux container behavior
    async fn test_install_script_non_zero_exit() {
        // Create a temporary directory for the test
        let temp_dir = TempDir::new().unwrap();
        let script_path = temp_dir.path().join("install.sh");

        // Create a failing install script
        let mut script_file = std::fs::File::create(&script_path).unwrap();
        writeln!(script_file, "#!/bin/bash").unwrap();
        writeln!(script_file, "echo 'This script will fail'").unwrap();
        writeln!(script_file, "exit 1").unwrap();

        // Make the script executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).unwrap();
        }

        // Create a feature fetcher with a mock client
        let client = crate::oci::client::ReqwestClient::new().unwrap();
        let fetcher = FeatureFetcher::new(client);

        // Execute the install script and verify it fails
        let env_vars = HashMap::new();
        let result = fetcher
            .execute_install_script(&script_path, &env_vars)
            .await;

        // Verify the error is properly returned
        assert!(result.is_err(), "Expected install script to fail");

        // Check the error details - use debug format to see the underlying error
        if let Err(ref e) = result {
            let debug_msg = format!("{:?}", e);

            // The error should be a Feature error containing Installation variant with exit code
            assert!(
                debug_msg.contains("Installation") && debug_msg.contains("exit code"),
                "Error should be Installation variant with exit code, got: {}",
                debug_msg
            );
        }
    }

    #[tokio::test]
    #[cfg(unix)] // Bash script execution - Linux container behavior
    async fn test_install_script_success() {
        // Create a temporary directory for the test
        let temp_dir = TempDir::new().unwrap();
        let script_path = temp_dir.path().join("install.sh");

        // Create a successful install script
        let mut script_file = std::fs::File::create(&script_path).unwrap();
        writeln!(script_file, "#!/bin/bash").unwrap();
        writeln!(script_file, "echo 'Installation successful'").unwrap();
        writeln!(script_file, "exit 0").unwrap();

        // Make the script executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).unwrap();
        }

        // Create a feature fetcher with a mock client
        let client = crate::oci::client::ReqwestClient::new().unwrap();
        let fetcher = FeatureFetcher::new(client);

        // Execute the install script and verify it succeeds
        let env_vars = HashMap::new();
        let result = fetcher
            .execute_install_script(&script_path, &env_vars)
            .await;

        // Verify the script executed successfully
        assert!(
            result.is_ok(),
            "Expected install script to succeed, got error: {:?}",
            result.unwrap_err()
        );
    }

    #[tokio::test]
    #[cfg(unix)] // Bash script execution - Linux container behavior
    async fn test_install_script_with_env_vars() {
        // Create a temporary directory for the test
        let temp_dir = TempDir::new().unwrap();
        let script_path = temp_dir.path().join("install.sh");

        // Create a script that uses environment variables
        let mut script_file = std::fs::File::create(&script_path).unwrap();
        writeln!(script_file, "#!/bin/bash").unwrap();
        writeln!(script_file, "echo \"Feature ID: $FEATURE_ID\"").unwrap();
        writeln!(script_file, "echo \"Feature Version: $FEATURE_VERSION\"").unwrap();
        writeln!(script_file, "exit 0").unwrap();

        // Make the script executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).unwrap();
        }

        // Create a feature fetcher with a mock client
        let client = crate::oci::client::ReqwestClient::new().unwrap();
        let fetcher = FeatureFetcher::new(client);

        // Execute the install script with environment variables
        let mut env_vars = HashMap::new();
        env_vars.insert("FEATURE_ID".to_string(), "test-feature".to_string());
        env_vars.insert("FEATURE_VERSION".to_string(), "1.0.0".to_string());

        let result = fetcher
            .execute_install_script(&script_path, &env_vars)
            .await;

        // Verify the script executed successfully
        assert!(
            result.is_ok(),
            "Expected install script to succeed with env vars, got error: {:?}",
            result.unwrap_err()
        );
    }
}
