//! Integration tests for OCI feature fetch and install functionality
//!
//! Note: These tests use Unix-specific APIs and are only compiled on Unix systems.
#![cfg(unix)]

use bytes::Bytes;
use deacon_core::oci::{FeatureFetcher, FeatureRef, MockHttpClient, ReqwestClient};
use tempfile::TempDir;

/// Create a minimal valid tar archive containing a devcontainer feature
fn create_test_feature_tar() -> Vec<u8> {
    let mut tar_data = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_data);

        // Add devcontainer-feature.json
        let feature_json = r#"
        {
            "id": "test-feature",
            "name": "Test Feature",
            "description": "A test feature for OCI integration",
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
echo "Installing test feature"
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

#[tokio::test]
async fn test_oci_feature_fetch_with_mock_client() {
    // Create test data
    let tar_data = create_test_feature_tar();
    // The layer digest MUST be the real sha256 of the blob bytes — the fetcher
    // now verifies downloaded content against the manifest's declared digest,
    // so a placeholder digest would (correctly) be rejected.
    let layer_digest = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&tar_data);
        format!("sha256:{:x}", hasher.finalize())
    };

    // Create the manifest JSON
    let manifest_json = format!(
        r#"{{
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "layers": [
                {{
                    "mediaType": "application/vnd.oci.image.layer.v1.tar",
                    "size": {},
                    "digest": "{}"
                }}
            ]
        }}"#,
        tar_data.len(),
        layer_digest
    );

    // Create a feature reference
    let feature_ref = FeatureRef::new(
        "ghcr.io".to_string(),
        "test".to_string(),
        "feature".to_string(),
        Some("1.0.0".to_string()),
    );

    // Create a temporary directory for cache
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().to_path_buf();

    // Create feature fetcher with mock HTTP client
    let mock_client = MockHttpClient::new();
    let fetcher = FeatureFetcher::with_cache_dir(mock_client, cache_dir.clone());

    // Add mock responses
    let manifest_url = "https://ghcr.io/v2/test/feature/manifests/1.0.0";
    let blob_url = format!("https://ghcr.io/v2/test/feature/blobs/{}", layer_digest);

    fetcher
        .client()
        .add_response(manifest_url.to_string(), Bytes::from(manifest_json))
        .await;
    fetcher
        .client()
        .add_response(blob_url, Bytes::from(tar_data))
        .await;

    // Test the fetch operation
    let downloaded_feature = fetcher.fetch_feature(&feature_ref).await.unwrap();

    // Verify the downloaded feature
    assert_eq!(downloaded_feature.metadata.id, "test-feature");
    assert_eq!(
        downloaded_feature.metadata.name,
        Some("Test Feature".to_string())
    );
    assert_eq!(
        downloaded_feature.metadata.version,
        Some("1.0.0".to_string())
    );
    assert!(downloaded_feature.path.exists());
    assert!(downloaded_feature
        .path
        .join("devcontainer-feature.json")
        .exists());
    assert!(downloaded_feature.path.join("install.sh").exists());

    // Test caching - fetch the same feature again
    let cached_feature = fetcher.fetch_feature(&feature_ref).await.unwrap();
    assert_eq!(cached_feature.metadata.id, "test-feature");
    assert_eq!(cached_feature.path, downloaded_feature.path);
}

/// Security regression: a registry that serves blob bytes which do not match
/// the manifest's declared layer digest must be rejected, not extracted and
/// executed. Guards the content-integrity verification in `download_layer`.
#[tokio::test]
async fn test_oci_feature_fetch_rejects_tampered_blob() {
    let tar_data = create_test_feature_tar();

    // Manifest claims a digest the served bytes will NOT hash to.
    let claimed_digest = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    let manifest_json = format!(
        r#"{{
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "layers": [
                {{
                    "mediaType": "application/vnd.oci.image.layer.v1.tar",
                    "size": {},
                    "digest": "{}"
                }}
            ]
        }}"#,
        tar_data.len(),
        claimed_digest
    );

    let feature_ref = FeatureRef::new(
        "ghcr.io".to_string(),
        "test".to_string(),
        "feature".to_string(),
        Some("1.0.0".to_string()),
    );

    let temp_dir = TempDir::new().unwrap();
    let mock_client = MockHttpClient::new();
    let fetcher = FeatureFetcher::with_cache_dir(mock_client, temp_dir.path().to_path_buf());

    let manifest_url = "https://ghcr.io/v2/test/feature/manifests/1.0.0";
    let blob_url = format!("https://ghcr.io/v2/test/feature/blobs/{}", claimed_digest);

    fetcher
        .client()
        .add_response(manifest_url.to_string(), Bytes::from(manifest_json))
        .await;
    // Serve the (mismatched) tar bytes at the claimed-digest URL.
    fetcher
        .client()
        .add_response(blob_url, Bytes::from(tar_data))
        .await;

    let result = fetcher.fetch_feature(&feature_ref).await;
    let err = result.expect_err("tampered blob must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("Integrity verification failed"),
        "expected integrity error, got: {msg}"
    );
}

#[tokio::test]
async fn test_oci_feature_install() {
    // Create a test feature manually
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature");
    std::fs::create_dir_all(&feature_dir).unwrap();

    // Create devcontainer-feature.json
    let feature_json = r#"
    {
        "id": "test-install-feature",
        "name": "Test Install Feature",
        "version": "1.0.0"
    }
    "#;
    std::fs::write(feature_dir.join("devcontainer-feature.json"), feature_json).unwrap();

    // Create install.sh script
    let install_script = r#"#!/bin/bash
echo "Feature ID: $FEATURE_ID"
echo "Feature Version: $FEATURE_VERSION"
echo "Installation completed"
"#;
    std::fs::write(feature_dir.join("install.sh"), install_script).unwrap();

    // Make install.sh executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(feature_dir.join("install.sh"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(feature_dir.join("install.sh"), perms).unwrap();
    }

    // Parse the metadata
    let metadata = deacon_core::features::parse_feature_metadata(
        &feature_dir.join("devcontainer-feature.json"),
    )
    .unwrap();

    // Create a DownloadedFeature
    let downloaded_feature = deacon_core::oci::DownloadedFeature {
        path: feature_dir,
        metadata,
        digest: "test-digest".to_string(),
    };

    // Create fetcher and test installation
    let client = ReqwestClient::new().unwrap();
    let fetcher = FeatureFetcher::new(client);

    // Test the install operation
    let result = fetcher.install_feature(&downloaded_feature).await;
    assert!(result.is_ok(), "Installation should succeed: {:?}", result);
}

#[tokio::test]
async fn test_oci_feature_install_no_script() {
    // Create a test feature without install.sh
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join("test-feature-no-script");
    std::fs::create_dir_all(&feature_dir).unwrap();

    // Create devcontainer-feature.json
    let feature_json = r#"
    {
        "id": "test-no-script-feature",
        "name": "Test No Script Feature",
        "version": "1.0.0"
    }
    "#;
    std::fs::write(feature_dir.join("devcontainer-feature.json"), feature_json).unwrap();

    // Parse the metadata
    let metadata = deacon_core::features::parse_feature_metadata(
        &feature_dir.join("devcontainer-feature.json"),
    )
    .unwrap();

    // Create a DownloadedFeature
    let downloaded_feature = deacon_core::oci::DownloadedFeature {
        path: feature_dir,
        metadata,
        digest: "test-digest".to_string(),
    };

    // Create fetcher and test installation
    let client = ReqwestClient::new().unwrap();
    let fetcher = FeatureFetcher::new(client);

    // Test the install operation - should succeed even without install.sh
    let result = fetcher.install_feature(&downloaded_feature).await;
    assert!(
        result.is_ok(),
        "Installation should succeed without script: {:?}",
        result
    );
}

#[test]
fn test_feature_ref_functionality() {
    let feature_ref = FeatureRef::new(
        "ghcr.io".to_string(),
        "devcontainers".to_string(),
        "node".to_string(),
        Some("18".to_string()),
    );

    assert_eq!(feature_ref.tag(), "18");
    assert_eq!(feature_ref.repository(), "devcontainers/node");
    assert_eq!(feature_ref.reference(), "ghcr.io/devcontainers/node:18");

    // Test with default version
    let feature_ref_default = FeatureRef::new(
        "ghcr.io".to_string(),
        "devcontainers".to_string(),
        "python".to_string(),
        None,
    );

    assert_eq!(feature_ref_default.tag(), "latest");
    assert_eq!(
        feature_ref_default.reference(),
        "ghcr.io/devcontainers/python:latest"
    );
}
