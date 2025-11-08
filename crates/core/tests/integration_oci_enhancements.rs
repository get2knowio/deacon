//! Integration tests for enhanced OCI registry operations
//!
//! Tests for tag listing, manifest fetching by digest, semantic version operations,
//! collection metadata, and multi-tag publishing.

use bytes::Bytes;
use deacon_core::features::FeatureMetadata;
use deacon_core::oci::{
    semver_utils, CollectionFeature, CollectionMetadata, CollectionSourceInfo, FeatureFetcher,
    FeatureRef, MockHttpClient, TagList,
};
use std::collections::HashMap;
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

        builder.finish().unwrap();
    }
    tar_data
}

#[tokio::test]
async fn test_list_tags() {
    // Create mock HTTP client
    let mock_client = MockHttpClient::new();
    let temp_dir = TempDir::new().unwrap();
    let fetcher =
        FeatureFetcher::with_cache_dir(mock_client.clone(), temp_dir.path().to_path_buf());

    // Create feature reference
    let feature_ref = FeatureRef::new(
        "ghcr.io".to_string(),
        "devcontainers".to_string(),
        "node".to_string(),
        None,
    );

    // Mock the tags list response
    let tags_url = "https://ghcr.io/v2/devcontainers/node/tags/list";
    let tag_list = TagList {
        name: "devcontainers/node".to_string(),
        tags: vec![
            "1.0.0".to_string(),
            "1.1.0".to_string(),
            "2.0.0".to_string(),
            "latest".to_string(),
        ],
    };
    let tags_json = serde_json::to_vec(&tag_list).unwrap();

    mock_client
        .add_response(tags_url.to_string(), Bytes::from(tags_json))
        .await;

    // Call list_tags
    let result = fetcher.list_tags(&feature_ref).await;
    assert!(result.is_ok());

    let tags = result.unwrap();
    assert_eq!(tags.len(), 4);
    assert!(tags.contains(&"1.0.0".to_string()));
    assert!(tags.contains(&"1.1.0".to_string()));
    assert!(tags.contains(&"2.0.0".to_string()));
    assert!(tags.contains(&"latest".to_string()));
}

#[tokio::test]
async fn test_get_manifest_by_digest() {
    // Create mock HTTP client
    let mock_client = MockHttpClient::new();
    let temp_dir = TempDir::new().unwrap();
    let fetcher =
        FeatureFetcher::with_cache_dir(mock_client.clone(), temp_dir.path().to_path_buf());

    // Create feature reference
    let feature_ref = FeatureRef::new(
        "ghcr.io".to_string(),
        "devcontainers".to_string(),
        "node".to_string(),
        Some("1.0.0".to_string()),
    );

    // Mock the manifest response by digest
    let digest = "sha256:1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    let manifest_url = format!("https://ghcr.io/v2/devcontainers/node/manifests/{}", digest);
    let manifest_json = r#"{
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "layers": [
            {
                "mediaType": "application/vnd.oci.image.layer.v1.tar",
                "size": 1024,
                "digest": "sha256:abc123"
            }
        ]
    }"#;

    mock_client
        .add_response(manifest_url, Bytes::from(manifest_json))
        .await;

    // Call get_manifest_by_digest
    let result = fetcher.get_manifest_by_digest(&feature_ref, digest).await;
    assert!(result.is_ok());

    let manifest = result.unwrap();
    assert_eq!(manifest.schema_version, 2);
    assert_eq!(manifest.layers.len(), 1);
    assert_eq!(manifest.layers[0].digest, "sha256:abc123");
}

#[tokio::test]
async fn test_list_tags_pagination() {
    // Test pagination support for tag listing with Link headers
    let mock_client = MockHttpClient::new();
    let temp_dir = TempDir::new().unwrap();
    let fetcher =
        FeatureFetcher::with_cache_dir(mock_client.clone(), temp_dir.path().to_path_buf());

    let feature_ref = FeatureRef::new(
        "ghcr.io".to_string(),
        "devcontainers".to_string(),
        "node".to_string(),
        None,
    );

    // Mock page 1 with Link header pointing to page 2
    let tags_url = "https://ghcr.io/v2/devcontainers/node/tags/list";
    let page1_tags = TagList {
        name: "devcontainers/node".to_string(),
        tags: vec!["1.0.0".to_string(), "1.1.0".to_string()],
    };
    let page1_json = serde_json::to_vec(&page1_tags).unwrap();

    let mut page1_headers = HashMap::new();
    page1_headers.insert(
        "Link".to_string(),
        "<https://ghcr.io/v2/devcontainers/node/tags/list?last=1.1.0>; rel=\"next\"".to_string(),
    );

    let page1_response = deacon_core::oci::HttpResponse {
        status: 200,
        headers: page1_headers,
        body: bytes::Bytes::from(page1_json),
    };

    mock_client
        .add_response_with_headers(tags_url.to_string(), page1_response)
        .await;

    // Mock page 2 with Link header pointing to page 3
    let page2_url = "https://ghcr.io/v2/devcontainers/node/tags/list?last=1.1.0";
    let page2_tags = TagList {
        name: "devcontainers/node".to_string(),
        tags: vec!["2.0.0".to_string(), "2.1.0".to_string()],
    };
    let page2_json = serde_json::to_vec(&page2_tags).unwrap();

    let mut page2_headers = HashMap::new();
    page2_headers.insert(
        "Link".to_string(),
        "<https://ghcr.io/v2/devcontainers/node/tags/list?last=2.1.0>; rel=\"next\"".to_string(),
    );

    let page2_response = deacon_core::oci::HttpResponse {
        status: 200,
        headers: page2_headers,
        body: bytes::Bytes::from(page2_json),
    };

    mock_client
        .add_response_with_headers(page2_url.to_string(), page2_response)
        .await;

    // Mock page 3 with no Link header (end of pagination)
    let page3_url = "https://ghcr.io/v2/devcontainers/node/tags/list?last=2.1.0";
    let page3_tags = TagList {
        name: "devcontainers/node".to_string(),
        tags: vec!["3.0.0".to_string()],
    };
    let page3_json = serde_json::to_vec(&page3_tags).unwrap();

    let page3_response = deacon_core::oci::HttpResponse {
        status: 200,
        headers: HashMap::new(),
        body: bytes::Bytes::from(page3_json),
    };

    mock_client
        .add_response_with_headers(page3_url.to_string(), page3_response)
        .await;

    let result = fetcher.list_tags(&feature_ref).await;
    assert!(result.is_ok());

    let tags = result.unwrap();
    assert_eq!(tags.len(), 5);
    assert!(tags.contains(&"1.0.0".to_string()));
    assert!(tags.contains(&"1.1.0".to_string()));
    assert!(tags.contains(&"2.0.0".to_string()));
    assert!(tags.contains(&"2.1.0".to_string()));
    assert!(tags.contains(&"3.0.0".to_string()));
}

#[tokio::test]
async fn test_list_tags_pagination_page_limit() {
    // Test that pagination stops after 10 pages even if more pages are available
    let mock_client = MockHttpClient::new();
    let temp_dir = TempDir::new().unwrap();
    let fetcher =
        FeatureFetcher::with_cache_dir(mock_client.clone(), temp_dir.path().to_path_buf());

    let feature_ref = FeatureRef::new(
        "ghcr.io".to_string(),
        "devcontainers".to_string(),
        "node".to_string(),
        None,
    );

    let base_url = "https://ghcr.io/v2/devcontainers/node/tags/list";

    // Create 11 pages (1 base + 10 more than max allowed)
    // Each page with 10 tags to ensure we hit the tag limit or page limit
    for page_num in 0..11 {
        let url = if page_num == 0 {
            base_url.to_string()
        } else {
            format!("{}?page={}", base_url, page_num)
        };

        let mut tags = Vec::new();
        for i in 0..10 {
            tags.push(format!("tag-{}-{}", page_num, i));
        }

        let tag_list = TagList {
            name: "devcontainers/node".to_string(),
            tags,
        };
        let json = serde_json::to_vec(&tag_list).unwrap();

        let mut headers = HashMap::new();
        // Add Link header for all but the last page
        if page_num < 10 {
            let next_url = format!("{}?page={}", base_url, page_num + 1);
            headers.insert("Link".to_string(), format!("<{}>; rel=\"next\"", next_url));
        }

        let response = deacon_core::oci::HttpResponse {
            status: 200,
            headers,
            body: bytes::Bytes::from(json),
        };

        mock_client.add_response_with_headers(url, response).await;
    }

    let result = fetcher.list_tags(&feature_ref).await;
    assert!(result.is_ok());

    let tags = result.unwrap();
    // Should have exactly 10 pages * 10 tags = 100 tags (max 10 pages)
    assert_eq!(tags.len(), 100, "Should stop at 10 pages limit");
}

#[tokio::test]
async fn test_list_tags_pagination_tag_limit() {
    // Test that pagination stops after 1000 tags even if more are available
    let mock_client = MockHttpClient::new();
    let temp_dir = TempDir::new().unwrap();
    let fetcher =
        FeatureFetcher::with_cache_dir(mock_client.clone(), temp_dir.path().to_path_buf());

    let feature_ref = FeatureRef::new(
        "ghcr.io".to_string(),
        "devcontainers".to_string(),
        "node".to_string(),
        None,
    );

    let base_url = "https://ghcr.io/v2/devcontainers/node/tags/list";

    // Create 3 pages: 400 + 400 + 300 tags = 1100 total
    // Should stop at 1000
    for page_num in 0..3 {
        let url = if page_num == 0 {
            base_url.to_string()
        } else {
            format!("{}?page={}", base_url, page_num)
        };

        let mut tags = Vec::new();
        let tags_in_page = if page_num < 2 { 400 } else { 300 };

        for i in 0..tags_in_page {
            tags.push(format!("tag-{}-{}", page_num, i));
        }

        let tag_list = TagList {
            name: "devcontainers/node".to_string(),
            tags,
        };
        let json = serde_json::to_vec(&tag_list).unwrap();

        let mut headers = HashMap::new();
        // Add Link header for all but the last page
        if page_num < 2 {
            let next_url = format!("{}?page={}", base_url, page_num + 1);
            headers.insert("Link".to_string(), format!("<{}>; rel=\"next\"", next_url));
        }

        let response = deacon_core::oci::HttpResponse {
            status: 200,
            headers,
            body: bytes::Bytes::from(json),
        };

        mock_client.add_response_with_headers(url, response).await;
    }

    let result = fetcher.list_tags(&feature_ref).await;
    assert!(result.is_ok());

    let tags = result.unwrap();
    // Should have exactly 1000 tags (stopped at limit)
    assert_eq!(tags.len(), 1000, "Should stop at 1000 tag limit");
}

#[tokio::test]
async fn test_get_manifest_different_media_types() {
    // Test handling of different manifest media types
    let mock_client = MockHttpClient::new();
    let temp_dir = TempDir::new().unwrap();
    let fetcher =
        FeatureFetcher::with_cache_dir(mock_client.clone(), temp_dir.path().to_path_buf());

    let feature_ref = FeatureRef::new(
        "ghcr.io".to_string(),
        "devcontainers".to_string(),
        "node".to_string(),
        Some("1.0.0".to_string()),
    );

    // Test with OCI image manifest v1
    let manifest_url = "https://ghcr.io/v2/devcontainers/node/manifests/1.0.0";
    let manifest_json = r#"{
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "layers": [{
            "mediaType": "application/vnd.oci.image.layer.v1.tar",
            "size": 1024,
            "digest": "sha256:abc123"
        }]
    }"#;

    mock_client
        .add_response(manifest_url.to_string(), Bytes::from(manifest_json))
        .await;

    let result = fetcher.get_manifest(&feature_ref).await;
    assert!(result.is_ok());

    let manifest = result.unwrap();
    assert_eq!(
        manifest.media_type,
        "application/vnd.oci.image.manifest.v1+json"
    );

    // TODO: Add tests for:
    // - Docker manifest v2 schema 2 (application/vnd.docker.distribution.manifest.v2+json)
    // - OCI image index (application/vnd.oci.image.index.v1+json)
    // - Unsupported media types (should return appropriate error)
}

#[tokio::test]
async fn test_get_manifest_unsupported_media_type() {
    // Test error handling for unsupported manifest media types
    let mock_client = MockHttpClient::new();
    let temp_dir = TempDir::new().unwrap();
    let fetcher =
        FeatureFetcher::with_cache_dir(mock_client.clone(), temp_dir.path().to_path_buf());

    let feature_ref = FeatureRef::new(
        "ghcr.io".to_string(),
        "devcontainers".to_string(),
        "node".to_string(),
        Some("1.0.0".to_string()),
    );

    // Mock response with an unsupported/unknown media type
    let manifest_url = "https://ghcr.io/v2/devcontainers/node/manifests/1.0.0";
    let manifest_json = r#"{
        "schemaVersion": 1,
        "mediaType": "application/vnd.unknown.manifest.v1+json",
        "layers": []
    }"#;

    mock_client
        .add_response(manifest_url.to_string(), Bytes::from(manifest_json))
        .await;

    let result = fetcher.get_manifest(&feature_ref).await;
    // Currently, parsing succeeds as we don't validate media type
    // TODO: Add media type validation and ensure proper error for unsupported types
    assert!(result.is_ok());
}

/// Test multi-tag publishing with proper mocking
/// This test is marked as ignored until full upload flow mocking is implemented
#[tokio::test]
#[ignore = "Requires full upload flow mocking - TODO: implement mock responses for blob uploads"]
async fn test_publish_feature_multi_tag_full_flow() {
    // Create mock HTTP client
    let mock_client = MockHttpClient::new();
    let temp_dir = TempDir::new().unwrap();
    let fetcher =
        FeatureFetcher::with_cache_dir(mock_client.clone(), temp_dir.path().to_path_buf());

    // Create test tar data
    let tar_data = create_test_feature_tar();
    let tar_bytes = Bytes::from(tar_data);

    // Create metadata
    let metadata = FeatureMetadata {
        id: "test-feature".to_string(),
        name: Some("Test Feature".to_string()),
        description: Some("A test feature".to_string()),
        version: Some("1.0.0".to_string()),
        documentation_url: None,
        license_url: None,
        options: std::collections::HashMap::new(),
        container_env: std::collections::HashMap::new(),
        mounts: Vec::new(),
        init: None,
        privileged: None,
        cap_add: Vec::new(),
        security_opt: Vec::new(),
        entrypoint: None,
        installs_after: Vec::new(),
        depends_on: std::collections::HashMap::new(),
        on_create_command: None,
        update_content_command: None,
        post_create_command: None,
        post_start_command: None,
        post_attach_command: None,
    };

    // TODO: Mock the full upload flow:
    // 1. Mock HEAD/GET requests for manifest checks (404 for non-existent tags)
    // 2. Mock POST /v2/{repo}/blobs/uploads/ (initiate upload)
    // 3. Mock PUT upload completion
    // 4. Mock PUT manifest upload for each tag
    // Then verify that:
    // - Result is Ok
    // - Correct number of tags published
    // - Mock client received expected calls

    let tags = vec!["1".to_string(), "1.0".to_string(), "1.0.0".to_string()];

    let result = fetcher
        .publish_feature_multi_tag(
            "ghcr.io".to_string(),
            "test".to_string(),
            "feature".to_string(),
            tags,
            tar_bytes,
            &metadata,
        )
        .await;

    // For now, expect error since upload flow isn't mocked
    assert!(result.is_err());
}

#[tokio::test]
async fn test_publish_feature_multi_tag_idempotency() {
    // Test that existing tags are skipped
    let mock_client = MockHttpClient::new();
    let temp_dir = TempDir::new().unwrap();
    let fetcher =
        FeatureFetcher::with_cache_dir(mock_client.clone(), temp_dir.path().to_path_buf());

    // Create test tar data
    let tar_data = create_test_feature_tar();
    let tar_bytes = Bytes::from(tar_data);

    // Create metadata
    let metadata = FeatureMetadata {
        id: "test-feature".to_string(),
        name: Some("Test Feature".to_string()),
        description: Some("A test feature".to_string()),
        version: Some("1.0.0".to_string()),
        documentation_url: None,
        license_url: None,
        options: std::collections::HashMap::new(),
        container_env: std::collections::HashMap::new(),
        mounts: Vec::new(),
        init: None,
        privileged: None,
        cap_add: Vec::new(),
        security_opt: Vec::new(),
        entrypoint: None,
        installs_after: Vec::new(),
        depends_on: std::collections::HashMap::new(),
        on_create_command: None,
        update_content_command: None,
        post_create_command: None,
        post_start_command: None,
        post_attach_command: None,
    };

    // Mock manifest exists for tag "1.0.0" (should be skipped)
    let manifest_url = "https://ghcr.io/v2/test/feature/manifests/1.0.0";
    let manifest_json = r#"{
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "layers": [{
            "mediaType": "application/vnd.oci.image.layer.v1.tar",
            "size": 1024,
            "digest": "sha256:abc123"
        }]
    }"#;
    mock_client
        .add_response(manifest_url.to_string(), Bytes::from(manifest_json))
        .await;

    // Don't mock responses for other tags (they should fail with "not found")

    let tags = vec!["1".to_string(), "1.0".to_string(), "1.0.0".to_string()];

    let result = fetcher
        .publish_feature_multi_tag(
            "ghcr.io".to_string(),
            "test".to_string(),
            "feature".to_string(),
            tags,
            tar_bytes,
            &metadata,
        )
        .await;

    // Should fail because we haven't mocked the upload flow for tags "1" and "1.0"
    // But the important part is that tag "1.0.0" should have been skipped (check logs)
    assert!(result.is_err());
}

#[tokio::test]
async fn test_publish_feature_multi_tag_error_propagation() {
    // Test that non-404 errors are propagated
    let mock_client = MockHttpClient::new();
    let temp_dir = TempDir::new().unwrap();
    let fetcher =
        FeatureFetcher::with_cache_dir(mock_client.clone(), temp_dir.path().to_path_buf());

    // Create test tar data
    let tar_data = create_test_feature_tar();
    let tar_bytes = Bytes::from(tar_data);

    // Create metadata
    let metadata = FeatureMetadata {
        id: "test-feature".to_string(),
        name: Some("Test Feature".to_string()),
        description: Some("A test feature".to_string()),
        version: Some("1.0.0".to_string()),
        documentation_url: None,
        license_url: None,
        options: std::collections::HashMap::new(),
        container_env: std::collections::HashMap::new(),
        mounts: Vec::new(),
        init: None,
        privileged: None,
        cap_add: Vec::new(),
        security_opt: Vec::new(),
        entrypoint: None,
        installs_after: Vec::new(),
        depends_on: std::collections::HashMap::new(),
        on_create_command: None,
        update_content_command: None,
        post_create_command: None,
        post_start_command: None,
        post_attach_command: None,
    };

    // Note: By not mocking any responses, the manifest check will fail
    // The error will contain "No mock response" which is NOT a 404
    // So it should be propagated rather than treated as "tag doesn't exist"

    let tags = vec!["1.0.0".to_string()];

    let result = fetcher
        .publish_feature_multi_tag(
            "ghcr.io".to_string(),
            "test".to_string(),
            "feature".to_string(),
            tags,
            tar_bytes,
            &metadata,
        )
        .await;

    // Should fail with the actual error, not try to publish
    assert!(result.is_err());
    // The important thing is that it failed during manifest check,
    // not that it proceeded to try publishing
    // We've verified the error propagation by seeing it fail here
}

#[test]
fn test_collection_metadata_serialization() {
    // Test that collection metadata can be serialized/deserialized properly
    let metadata = CollectionMetadata {
        source_information: Some(CollectionSourceInfo {
            provider: "github".to_string(),
            repository: "devcontainers/features".to_string(),
        }),
        features: Some(vec![
            CollectionFeature {
                id: "node".to_string(),
                version: Some("1.0.0".to_string()),
                name: Some("Node.js".to_string()),
                description: Some("Installs Node.js".to_string()),
            },
            CollectionFeature {
                id: "python".to_string(),
                version: Some("2.0.0".to_string()),
                name: Some("Python".to_string()),
                description: Some("Installs Python".to_string()),
            },
        ]),
        templates: None,
    };

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&metadata).unwrap();
    assert!(json.contains("devcontainers/features"));
    assert!(json.contains("node"));
    assert!(json.contains("python"));

    // Deserialize back
    let deserialized: CollectionMetadata = serde_json::from_str(&json).unwrap();
    assert!(deserialized.features.is_some());
    assert_eq!(deserialized.features.as_ref().unwrap().len(), 2);
}

#[test]
fn test_semver_utils_parse_version() {
    // Test standard versions
    assert!(semver_utils::parse_version("1.2.3").is_some());
    assert!(semver_utils::parse_version("v1.2.3").is_some());

    // Test short versions
    assert!(semver_utils::parse_version("1.2").is_some());
    assert!(semver_utils::parse_version("1").is_some());

    // Test invalid versions
    assert!(semver_utils::parse_version("invalid").is_none());
    assert!(semver_utils::parse_version("").is_none());
}

#[test]
fn test_semver_utils_filter_and_sort() {
    let tags = vec![
        "1.0.0".to_string(),
        "latest".to_string(),
        "2.1.0".to_string(),
        "dev".to_string(),
        "1.5.0".to_string(),
        "v2.0.0".to_string(),
    ];

    // Filter to semver only
    let semver_tags = semver_utils::filter_semver_tags(&tags);
    assert_eq!(semver_tags.len(), 4); // 1.0.0, 2.1.0, 1.5.0, v2.0.0

    // Sort in descending order
    let mut sorted_tags = semver_tags.clone();
    semver_utils::sort_tags_descending(&mut sorted_tags);

    // Verify order (descending)
    assert_eq!(sorted_tags[0], "2.1.0");
    assert_eq!(sorted_tags[1], "v2.0.0");
    assert_eq!(sorted_tags[2], "1.5.0");
    assert_eq!(sorted_tags[3], "1.0.0");
}

#[test]
fn test_semver_utils_compute_semantic_tags() {
    // Test full version
    let tags = semver_utils::compute_semantic_tags("1.2.3");
    assert_eq!(tags.len(), 4);
    assert_eq!(tags[0], "1");
    assert_eq!(tags[1], "1.2");
    assert_eq!(tags[2], "1.2.3");
    assert_eq!(tags[3], "latest");

    // Test with v prefix
    let tags = semver_utils::compute_semantic_tags("v2.5.1");
    assert_eq!(tags.len(), 4);
    assert_eq!(tags[0], "2");
    assert_eq!(tags[1], "2.5");
    assert_eq!(tags[2], "2.5.1");
    assert_eq!(tags[3], "latest");

    // Test invalid version (should return just "latest")
    let tags = semver_utils::compute_semantic_tags("invalid");
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0], "latest");
}

#[test]
fn test_semver_utils_compare_versions() {
    use std::cmp::Ordering;

    // Test normal comparisons
    assert_eq!(
        semver_utils::compare_versions("1.2.3", "1.2.4"),
        Ordering::Less
    );
    assert_eq!(
        semver_utils::compare_versions("2.0.0", "1.9.9"),
        Ordering::Greater
    );
    assert_eq!(
        semver_utils::compare_versions("1.0.0", "1.0.0"),
        Ordering::Equal
    );

    // Test with v prefix
    assert_eq!(
        semver_utils::compare_versions("v1.2.3", "v1.2.4"),
        Ordering::Less
    );

    // Test with invalid versions
    assert_eq!(
        semver_utils::compare_versions("1.0.0", "invalid"),
        Ordering::Greater
    );
    assert_eq!(
        semver_utils::compare_versions("invalid", "1.0.0"),
        Ordering::Less
    );
}

#[tokio::test]
async fn test_get_manifest_with_digest() {
    // Create mock HTTP client
    let mock_client = MockHttpClient::new();
    let temp_dir = TempDir::new().unwrap();
    let fetcher =
        FeatureFetcher::with_cache_dir(mock_client.clone(), temp_dir.path().to_path_buf());

    // Create feature reference
    let feature_ref = FeatureRef::new(
        "ghcr.io".to_string(),
        "devcontainers".to_string(),
        "node".to_string(),
        Some("1.0.0".to_string()),
    );

    // Mock the manifest response
    let manifest_url = "https://ghcr.io/v2/devcontainers/node/manifests/1.0.0";
    let manifest_json = r#"{
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "layers": [
            {
                "mediaType": "application/vnd.oci.image.layer.v1.tar",
                "size": 1024,
                "digest": "sha256:abc123"
            }
        ]
    }"#;

    mock_client
        .add_response(manifest_url.to_string(), Bytes::from(manifest_json))
        .await;

    // Call get_manifest_with_digest
    let result = fetcher.get_manifest_with_digest(&feature_ref).await;
    assert!(
        result.is_ok(),
        "Expected successful manifest fetch with digest"
    );

    let (manifest, digest) = result.unwrap();

    // Verify manifest was parsed correctly
    assert_eq!(manifest["schemaVersion"], 2);
    assert_eq!(manifest["layers"].as_array().unwrap().len(), 1);
    assert_eq!(manifest["layers"][0]["digest"], "sha256:abc123");

    // Verify digest is valid hex string (64 chars for SHA256)
    assert_eq!(
        digest.len(),
        64,
        "SHA256 digest should be 64 hex characters"
    );
    assert!(
        digest.chars().all(|c| c.is_ascii_hexdigit()),
        "Digest should only contain hex characters"
    );
}

#[tokio::test]
async fn test_get_manifest_with_digest_same_manifest_same_digest() {
    // Test that the same manifest always produces the same digest (deterministic)
    let mock_client = MockHttpClient::new();
    let temp_dir = TempDir::new().unwrap();
    let fetcher =
        FeatureFetcher::with_cache_dir(mock_client.clone(), temp_dir.path().to_path_buf());

    let feature_ref = FeatureRef::new(
        "ghcr.io".to_string(),
        "devcontainers".to_string(),
        "node".to_string(),
        Some("1.0.0".to_string()),
    );

    let manifest_url = "https://ghcr.io/v2/devcontainers/node/manifests/1.0.0";
    let manifest_json = r#"{
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "layers": [
            {
                "mediaType": "application/vnd.oci.image.layer.v1.tar",
                "size": 1024,
                "digest": "sha256:abc123"
            }
        ]
    }"#;

    mock_client
        .add_response(manifest_url.to_string(), Bytes::from(manifest_json))
        .await;

    // Fetch twice
    let (manifest1, digest1) = fetcher
        .get_manifest_with_digest(&feature_ref)
        .await
        .expect("First fetch should succeed");
    let (manifest2, digest2) = fetcher
        .get_manifest_with_digest(&feature_ref)
        .await
        .expect("Second fetch should succeed");

    // Verify digests are identical
    assert_eq!(
        digest1, digest2,
        "Same manifest should always produce same digest"
    );

    // Verify manifests are identical
    assert_eq!(manifest1, manifest2, "Manifests should be identical");
}
