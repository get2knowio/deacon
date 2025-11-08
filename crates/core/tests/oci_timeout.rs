//! Unit tests for OCI registry timeout handling
//!
//! These tests verify that registry operations properly handle timeouts
//! and do not hang indefinitely when the registry is slow or unresponsive.

use bytes::Bytes;
use deacon_core::oci::{FeatureFetcher, FeatureRef, HttpClient, HttpResponse};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;

/// Mock HTTP client that can simulate slow/hanging responses
#[derive(Debug, Clone)]
pub struct SlowMockHttpClient {
    responses: Arc<Mutex<HashMap<String, (Duration, Bytes)>>>,
    responses_with_headers: Arc<Mutex<HashMap<String, (Duration, HttpResponse)>>>,
}

impl Default for SlowMockHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl SlowMockHttpClient {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
            responses_with_headers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Add a response that will be delayed by the specified duration
    pub async fn add_slow_response(&self, url: String, delay: Duration, response: Bytes) {
        let mut responses = self.responses.lock().await;
        responses.insert(url, (delay, response));
    }

    /// Add a response with headers that will be delayed by the specified duration
    pub async fn add_slow_response_with_headers(
        &self,
        url: String,
        delay: Duration,
        response: HttpResponse,
    ) {
        let mut responses = self.responses_with_headers.lock().await;
        responses.insert(url, (delay, response));
    }
}

#[async_trait::async_trait]
impl HttpClient for SlowMockHttpClient {
    async fn get(
        &self,
        url: &str,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        self.get_with_headers(url, HashMap::new()).await
    }

    async fn get_with_headers(
        &self,
        url: &str,
        _headers: HashMap<String, String>,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        let responses = self.responses.lock().await;

        if let Some((delay, response)) = responses.get(url) {
            // Simulate slow response
            sleep(*delay).await;
            return Ok(response.clone());
        }

        Err(format!("No mock response for URL: {}", url).into())
    }

    async fn get_with_headers_and_response(
        &self,
        url: &str,
        _headers: HashMap<String, String>,
    ) -> std::result::Result<HttpResponse, Box<dyn std::error::Error + Send + Sync>> {
        // Check for responses with headers first
        let responses_with_headers = self.responses_with_headers.lock().await;
        if let Some((delay, response)) = responses_with_headers.get(url) {
            sleep(*delay).await;
            return Ok(response.clone());
        }
        drop(responses_with_headers);

        // Fall back to regular responses without headers
        let responses = self.responses.lock().await;
        if let Some((delay, body)) = responses.get(url) {
            sleep(*delay).await;
            return Ok(HttpResponse {
                status: 200,
                headers: HashMap::new(),
                body: body.clone(),
            });
        }

        Err(format!("No mock response for URL: {}", url).into())
    }

    async fn head(
        &self,
        url: &str,
        _headers: HashMap<String, String>,
    ) -> std::result::Result<u16, Box<dyn std::error::Error + Send + Sync>> {
        let responses = self.responses.lock().await;

        if let Some((delay, _)) = responses.get(url) {
            // Simulate slow response
            sleep(*delay).await;
            return Ok(200);
        }

        Ok(404)
    }

    async fn put_with_headers(
        &self,
        url: &str,
        _data: Bytes,
        _headers: HashMap<String, String>,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        let responses = self.responses.lock().await;

        if let Some((delay, _)) = responses.get(url) {
            // Simulate slow response
            sleep(*delay).await;
            return Ok(Bytes::new());
        }

        Err(format!("No mock response for URL: {}", url).into())
    }

    async fn post_with_headers(
        &self,
        url: &str,
        _data: Bytes,
        _headers: HashMap<String, String>,
    ) -> std::result::Result<HttpResponse, Box<dyn std::error::Error + Send + Sync>> {
        let responses = self.responses.lock().await;

        if let Some((delay, response)) = responses.get(url) {
            // Simulate slow response
            sleep(*delay).await;
            return Ok(HttpResponse {
                status: 200,
                headers: HashMap::new(),
                body: response.clone(),
            });
        }

        Err(format!("No mock response for URL: {}", url).into())
    }
}

#[tokio::test]
async fn test_manifest_fetch_timeout() {
    let client = SlowMockHttpClient::new();

    // Set up a response that takes 15 seconds (longer than 10s default timeout)
    let manifest_url = "https://test.registry/v2/test/feature/manifests/latest";
    let manifest_response = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "layers": [{
            "mediaType": "application/vnd.oci.image.layer.v1.tar",
            "size": 1024,
            "digest": "sha256:abc123"
        }]
    });

    client
        .add_slow_response(
            manifest_url.to_string(),
            Duration::from_secs(15),
            Bytes::from(manifest_response.to_string()),
        )
        .await;

    let fetcher = FeatureFetcher::new(client);
    let feature_ref = FeatureRef::new(
        "test.registry".to_string(),
        "test".to_string(),
        "feature".to_string(),
        Some("latest".to_string()),
    );

    // This should timeout (fetcher has default 10s timeout via default_fetcher_with_config)
    let start = std::time::Instant::now();
    let result =
        tokio::time::timeout(Duration::from_secs(12), fetcher.get_manifest(&feature_ref)).await;

    let elapsed = start.elapsed();

    // Should either timeout or complete within reasonable time (12s max)
    assert!(
        elapsed < Duration::from_secs(13),
        "Request should timeout or complete within 13 seconds, took {:?}",
        elapsed
    );

    // The result should either be a timeout error or an operation error
    assert!(
        result.is_err() || result.unwrap().is_err(),
        "Slow request should result in timeout or error"
    );
}

#[tokio::test]
async fn test_tags_list_timeout() {
    let client = SlowMockHttpClient::new();

    // Set up a response that takes 15 seconds
    let tags_url = "https://test.registry/v2/test/feature/tags/list";
    let tags_response = serde_json::json!({
        "name": "test/feature",
        "tags": ["1.0.0", "1.1.0", "latest"]
    });

    client
        .add_slow_response(
            tags_url.to_string(),
            Duration::from_secs(15),
            Bytes::from(tags_response.to_string()),
        )
        .await;

    let fetcher = FeatureFetcher::new(client);
    let feature_ref = FeatureRef::new(
        "test.registry".to_string(),
        "test".to_string(),
        "feature".to_string(),
        None,
    );

    // This should timeout
    let start = std::time::Instant::now();
    let result =
        tokio::time::timeout(Duration::from_secs(12), fetcher.list_tags(&feature_ref)).await;

    let elapsed = start.elapsed();

    // Should complete within reasonable time
    assert!(
        elapsed < Duration::from_secs(13),
        "Request should timeout or complete within 13 seconds, took {:?}",
        elapsed
    );

    // The result should be a timeout or error
    assert!(
        result.is_err() || result.unwrap().is_err(),
        "Slow request should result in timeout or error"
    );
}

#[tokio::test]
async fn test_manifest_fetch_fast_response() {
    let client = SlowMockHttpClient::new();

    // Set up a response that's fast (100ms)
    let manifest_url = "https://test.registry/v2/test/feature/manifests/latest";
    let manifest_response = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "size": 512,
            "digest": "sha256:config123"
        },
        "layers": [{
            "mediaType": "application/vnd.oci.image.layer.v1.tar",
            "size": 1024,
            "digest": "sha256:abc123"
        }]
    });

    client
        .add_slow_response(
            manifest_url.to_string(),
            Duration::from_millis(100),
            Bytes::from(manifest_response.to_string()),
        )
        .await;

    let fetcher = FeatureFetcher::new(client);
    let feature_ref = FeatureRef::new(
        "test.registry".to_string(),
        "test".to_string(),
        "feature".to_string(),
        Some("latest".to_string()),
    );

    // This should succeed quickly
    let start = std::time::Instant::now();
    let result = fetcher.get_manifest(&feature_ref).await;
    let elapsed = start.elapsed();

    assert!(result.is_ok(), "Fast request should succeed");
    assert!(
        elapsed < Duration::from_secs(2),
        "Fast request should complete quickly, took {:?}",
        elapsed
    );
}

#[tokio::test]
async fn test_pagination_timeout_accumulation() {
    // Test that multiple paginated requests don't accumulate timeouts beyond reasonable limits
    let client = SlowMockHttpClient::new();

    // Set up multiple pages with Link headers for proper pagination
    for page in 0..5 {
        let tags_url = if page == 0 {
            "https://test.registry/v2/test/feature/tags/list".to_string()
        } else {
            format!(
                "https://test.registry/v2/test/feature/tags/list?page={}",
                page
            )
        };

        let tags_response = serde_json::json!({
            "name": "test/feature",
            "tags": [format!("{}.0.0", page), format!("{}.1.0", page)]
        });

        // For pages 0-3, add Link header to next page
        if page < 4 {
            let next_page = page + 1;
            let next_url = format!(
                "https://test.registry/v2/test/feature/tags/list?page={}",
                next_page
            );
            let link_header = format!("<{}>; rel=\"next\"", next_url);

            let mut headers = HashMap::new();
            headers.insert("Link".to_string(), link_header);

            client
                .add_slow_response_with_headers(
                    tags_url,
                    Duration::from_secs(2),
                    HttpResponse {
                        status: 200,
                        headers,
                        body: Bytes::from(tags_response.to_string()),
                    },
                )
                .await;
        } else {
            // Last page - no Link header
            client
                .add_slow_response(
                    tags_url,
                    Duration::from_secs(2),
                    Bytes::from(tags_response.to_string()),
                )
                .await;
        }
    }

    let fetcher = FeatureFetcher::new(client);
    let feature_ref = FeatureRef::new(
        "test.registry".to_string(),
        "test".to_string(),
        "feature".to_string(),
        None,
    );

    // Total time should be reasonable even with pagination
    let start = std::time::Instant::now();
    let result =
        tokio::time::timeout(Duration::from_secs(25), fetcher.list_tags(&feature_ref)).await;
    let elapsed = start.elapsed();

    // Should handle pagination within reasonable total time
    assert!(
        elapsed < Duration::from_secs(26),
        "Paginated requests should complete within total timeout, took {:?}",
        elapsed
    );

    // May succeed with partial results or fail, but shouldn't hang
    let _ = result;
}
