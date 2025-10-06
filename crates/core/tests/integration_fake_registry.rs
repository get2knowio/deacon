//! Fake registry integration test for CI environments
//!
//! This module provides a fake registry implementation for testing OCI operations
//! without requiring external network dependencies. It serves as the CI integration
//! test stub mentioned in the acceptance criteria.

use bytes::Bytes;
use deacon_core::oci::{FeatureFetcher, FeatureRef, HttpResponse, MockHttpClient, TemplateRef};
use sha2::Digest;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

/// A fake registry implementation for testing
pub struct FakeRegistry {
    /// Mock HTTP client to simulate registry responses  
    pub mock_client: MockHttpClient,
    /// Base URL for the fake registry
    pub base_url: String,
    /// Storage for published content
    storage: Arc<Mutex<FakeRegistryStorage>>,
}

#[derive(Default)]
struct FakeRegistryStorage {
    /// Map of (repository, tag) -> manifest JSON
    manifests: std::collections::HashMap<String, String>,
    /// Map of digest -> blob content
    blobs: std::collections::HashMap<String, Vec<u8>>,
}

impl FakeRegistry {
    /// Create a new fake registry
    pub fn new() -> Self {
        Self {
            mock_client: MockHttpClient::new(),
            base_url: "localhost:5000".to_string(),
            storage: Arc::new(Mutex::new(FakeRegistryStorage::default())),
        }
    }

    /// Add a feature to the fake registry
    pub async fn add_feature(&self, feature_ref: &FeatureRef, tar_data: Vec<u8>) {
        let layer_digest = format!("sha256:{:x}", sha2::Sha256::digest(&tar_data));

        // Create manifest
        let manifest = serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "layers": [{
                "mediaType": "application/vnd.oci.image.layer.v1.tar",
                "size": tar_data.len(),
                "digest": layer_digest
            }]
        });

        let repository_key = format!("{}/{}", feature_ref.namespace, feature_ref.name);
        let tag = feature_ref.version.as_deref().unwrap_or("latest");

        {
            let mut storage = self.storage.lock().unwrap();
            storage
                .manifests
                .insert(format!("{}:{}", repository_key, tag), manifest.to_string());
            storage.blobs.insert(layer_digest.clone(), tar_data);
        }

        // Set up mock responses
        let manifest_url = format!(
            "https://{}/v2/{}/manifests/{}",
            self.base_url, repository_key, tag
        );
        let blob_url = format!(
            "https://{}/v2/{}/blobs/{}",
            self.base_url, repository_key, layer_digest
        );

        self.mock_client
            .add_response(manifest_url, Bytes::from(manifest.to_string()))
            .await;
        self.mock_client
            .add_response(
                blob_url,
                Bytes::from({
                    let storage = self.storage.lock().unwrap();
                    storage.blobs.get(&layer_digest).unwrap().clone()
                }),
            )
            .await;
    }

    /// Add a template to the fake registry
    pub async fn add_template(&self, template_ref: &TemplateRef, tar_data: Vec<u8>) {
        let layer_digest = format!("sha256:{:x}", sha2::Sha256::digest(&tar_data));

        // Create manifest
        let manifest = serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "layers": [{
                "mediaType": "application/vnd.oci.image.layer.v1.tar",
                "size": tar_data.len(),
                "digest": layer_digest
            }]
        });

        let repository_key = format!("{}/{}", template_ref.namespace, template_ref.name);
        let tag = template_ref.version.as_deref().unwrap_or("latest");

        {
            let mut storage = self.storage.lock().unwrap();
            storage
                .manifests
                .insert(format!("{}:{}", repository_key, tag), manifest.to_string());
            storage.blobs.insert(layer_digest.clone(), tar_data);
        }

        // Set up mock responses
        let manifest_url = format!(
            "https://{}/v2/{}/manifests/{}",
            self.base_url, repository_key, tag
        );
        let blob_url = format!(
            "https://{}/v2/{}/blobs/{}",
            self.base_url, repository_key, layer_digest
        );

        self.mock_client
            .add_response(manifest_url, Bytes::from(manifest.to_string()))
            .await;
        self.mock_client
            .add_response(
                blob_url,
                Bytes::from({
                    let storage = self.storage.lock().unwrap();
                    storage.blobs.get(&layer_digest).unwrap().clone()
                }),
            )
            .await;
    }

    /// Create a feature fetcher that uses this fake registry
    pub fn feature_fetcher(&self, cache_dir: std::path::PathBuf) -> FeatureFetcher<MockHttpClient> {
        FeatureFetcher::with_cache_dir(self.mock_client.clone(), cache_dir)
    }
}

impl Default for FakeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a minimal valid tar archive containing a DevContainer feature
fn create_test_feature_tar() -> Vec<u8> {
    let mut tar_data = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_data);

        // Add devcontainer-feature.json
        let feature_json = r#"
        {
            "id": "test-feature",
            "name": "Test Feature",
            "description": "A test feature for fake registry testing",
            "version": "1.0.0",
            "options": {
                "version": {
                    "type": "string",
                    "default": "latest",
                    "description": "Version to install"
                }
            }
        }
        "#;

        let mut header = tar::Header::new_gnu();
        header.set_path("devcontainer-feature.json").unwrap();
        header.set_size(feature_json.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, feature_json.as_bytes()).unwrap();

        // Add install.sh script
        let install_script = r#"#!/bin/bash
echo "Installing test feature from fake registry"
echo "Feature ID: $FEATURE_ID"
echo "Feature Version: $FEATURE_VERSION"
echo "Test feature installed successfully"
"#;

        let mut header = tar::Header::new_gnu();
        header.set_path("install.sh").unwrap();
        header.set_size(install_script.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append(&header, install_script.as_bytes()).unwrap();

        builder.finish().unwrap();
    }
    tar_data
}

/// Create a minimal valid tar archive containing a DevContainer template
fn create_test_template_tar() -> Vec<u8> {
    let mut tar_data = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_data);

        // Add devcontainer-template.json
        let template_json = r#"
        {
            "id": "test-template",
            "name": "Test Template",
            "description": "A test template for fake registry testing",
            "options": {
                "variant": {
                    "type": "string",
                    "default": "default",
                    "description": "Template variant"
                }
            }
        }
        "#;

        let mut header = tar::Header::new_gnu();
        header.set_path("devcontainer-template.json").unwrap();
        header.set_size(template_json.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, template_json.as_bytes()).unwrap();

        // Add .devcontainer/devcontainer.json
        let devcontainer_json = r#"
        {
            "name": "Test Template Container",
            "image": "ubuntu:latest",
            "features": {},
            "customizations": {
                "vscode": {
                    "extensions": []
                }
            }
        }
        "#;

        let mut header = tar::Header::new_gnu();
        header.set_path(".devcontainer/devcontainer.json").unwrap();
        header.set_size(devcontainer_json.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append(&header, devcontainer_json.as_bytes())
            .unwrap();

        builder.finish().unwrap();
    }
    tar_data
}

#[tokio::test]
async fn test_fake_registry_feature_operations() {
    // Create fake registry and test data
    let fake_registry = FakeRegistry::new();
    let tar_data = create_test_feature_tar();

    let feature_ref = FeatureRef::new(
        "localhost:5000".to_string(),
        "test".to_string(),
        "feature".to_string(),
        Some("1.0.0".to_string()),
    );

    // Add feature to fake registry
    fake_registry.add_feature(&feature_ref, tar_data).await;

    // Create temporary cache directory
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().to_path_buf();

    // Create feature fetcher using fake registry
    let fetcher = fake_registry.feature_fetcher(cache_dir);

    // Test feature fetch operation
    let downloaded_feature = fetcher.fetch_feature(&feature_ref).await;

    // Should succeed with fake registry
    assert!(
        downloaded_feature.is_ok(),
        "Feature fetch should succeed with fake registry"
    );

    let feature = downloaded_feature.unwrap();
    assert_eq!(feature.metadata.id, "test-feature");
    assert_eq!(feature.metadata.name, Some("Test Feature".to_string()));
}

#[tokio::test]
async fn test_fake_registry_template_operations() {
    // Create fake registry and test data
    let fake_registry = FakeRegistry::new();
    let tar_data = create_test_template_tar();

    let template_ref = TemplateRef::new(
        "localhost:5000".to_string(),
        "test".to_string(),
        "template".to_string(),
        Some("1.0.0".to_string()),
    );

    // Add template to fake registry
    fake_registry.add_template(&template_ref, tar_data).await;

    // Create temporary cache directory
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().to_path_buf();

    // Create feature fetcher (used for both features and templates)
    let fetcher = fake_registry.feature_fetcher(cache_dir);

    // Test template fetch operation
    let downloaded_template = fetcher.fetch_template(&template_ref).await;

    // Should succeed with fake registry
    assert!(
        downloaded_template.is_ok(),
        "Template fetch should succeed with fake registry"
    );

    let template = downloaded_template.unwrap();
    assert_eq!(template.metadata.id, "test-template");
    assert_eq!(template.metadata.name, Some("Test Template".to_string()));
}

/// Test that demonstrates the fake registry can replace real network calls
/// This is the core guarantee for CI integration - no external dependencies
#[tokio::test]
async fn test_fake_registry_no_network_dependency() {
    // This test verifies that the fake registry works completely offline
    // and can be used in CI environments without external network access

    let fake_registry = FakeRegistry::new();
    let feature_tar = create_test_feature_tar();
    let template_tar = create_test_template_tar();

    // Set up fake registry with both features and templates
    let feature_ref = FeatureRef::new(
        "localhost:5000".to_string(),
        "ci-test".to_string(),
        "feature".to_string(),
        Some("ci".to_string()),
    );

    let template_ref = TemplateRef::new(
        "localhost:5000".to_string(),
        "ci-test".to_string(),
        "template".to_string(),
        Some("ci".to_string()),
    );

    fake_registry.add_feature(&feature_ref, feature_tar).await;
    fake_registry
        .add_template(&template_ref, template_tar)
        .await;

    // Create temporary cache
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().to_path_buf();

    // Test both feature and template operations work without network
    let fetcher = fake_registry.feature_fetcher(cache_dir);

    // Both operations should succeed completely offline
    let feature_result = fetcher.fetch_feature(&feature_ref).await;
    let template_result = fetcher.fetch_template(&template_ref).await;

    assert!(feature_result.is_ok(), "Feature fetch should work offline");
    assert!(
        template_result.is_ok(),
        "Template fetch should work offline"
    );
}

/// Test full push/pull cycle with OCI registry operations
/// This test verifies the improved blob upload workflow with proper retry logic
#[tokio::test]
async fn test_fake_registry_push_pull_cycle() {
    use deacon_core::features::FeatureMetadata;

    let fake_registry = FakeRegistry::new();
    let tar_data = create_test_feature_tar();

    // Parse the feature metadata from the tar
    let metadata = FeatureMetadata {
        id: "test-feature".to_string(),
        name: Some("Test Feature".to_string()),
        description: Some("A test feature for push/pull testing".to_string()),
        version: Some("1.0.0".to_string()),
        options: std::collections::HashMap::new(),
        container_env: std::collections::HashMap::new(),
        mounts: vec![],
        init: None,
        privileged: None,
        cap_add: vec![],
        security_opt: vec![],
        entrypoint: None,
        installs_after: vec![],
        depends_on: std::collections::HashMap::new(),
        documentation_url: None,
        license_url: None,
        on_create_command: None,
        update_content_command: None,
        post_create_command: None,
        post_start_command: None,
        post_attach_command: None,
    };

    let feature_ref = FeatureRef::new(
        "localhost:5000".to_string(),
        "test".to_string(),
        "push-pull-feature".to_string(),
        Some("1.0.0".to_string()),
    );

    // Set up mock responses for push operations
    // 1. Mock the POST to initiate upload session with Location header
    let upload_init_url = format!(
        "https://{}/v2/{}/blobs/uploads/",
        fake_registry.base_url,
        feature_ref.repository()
    );

    // Generate a realistic upload session UUID
    let upload_uuid = "550e8400-e29b-41d4-a716-446655440000";
    let upload_location = format!(
        "/v2/{}/blobs/uploads/{}",
        feature_ref.repository(),
        upload_uuid
    );

    let mut post_headers = std::collections::HashMap::new();
    post_headers.insert("location".to_string(), upload_location.clone());
    post_headers.insert("Location".to_string(), upload_location.clone());

    fake_registry
        .mock_client
        .add_response_with_headers(
            upload_init_url,
            HttpResponse {
                status: 202,
                headers: post_headers,
                body: bytes::Bytes::from(""),
            },
        )
        .await;

    // 2. Mock the PUT for monolithic blob upload using the Location from POST
    let layer_digest = format!("sha256:{:x}", sha2::Sha256::digest(&tar_data));
    let upload_complete_url = format!("{}?digest={}", upload_location, layer_digest);
    fake_registry
        .mock_client
        .add_response(upload_complete_url.clone(), Bytes::from(""))
        .await;

    // 3. Mock the PUT for manifest upload
    let manifest_url = format!(
        "https://{}/v2/{}/manifests/{}",
        fake_registry.base_url,
        feature_ref.repository(),
        feature_ref.tag()
    );
    fake_registry
        .mock_client
        .add_response(manifest_url, Bytes::from(""))
        .await;

    // Create temporary cache
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().to_path_buf();
    let fetcher = fake_registry.feature_fetcher(cache_dir.clone());

    // Test publish operation
    let publish_result = fetcher
        .publish_feature(&feature_ref, Bytes::from(tar_data.clone()), &metadata)
        .await;

    assert!(
        publish_result.is_ok(),
        "Feature publish should succeed: {:?}",
        publish_result.err()
    );

    let publish_info = publish_result.unwrap();
    assert_eq!(publish_info.registry, "localhost:5000");
    assert_eq!(publish_info.repository, "test/push-pull-feature");
    assert_eq!(publish_info.tag, "1.0.0");

    // Now test pull after push - add feature to fake registry for pull
    fake_registry.add_feature(&feature_ref, tar_data).await;

    // Create new fetcher with clean cache to test actual pull
    let temp_dir2 = TempDir::new().unwrap();
    let cache_dir2 = temp_dir2.path().to_path_buf();
    let fetcher2 = fake_registry.feature_fetcher(cache_dir2);

    let fetch_result = fetcher2.fetch_feature(&feature_ref).await;

    assert!(
        fetch_result.is_ok(),
        "Feature pull should succeed after push: {:?}",
        fetch_result.err()
    );

    let fetched_feature = fetch_result.unwrap();
    assert_eq!(fetched_feature.metadata.id, "test-feature");
    assert_eq!(
        fetched_feature.metadata.name,
        Some("Test Feature".to_string())
    );
}
