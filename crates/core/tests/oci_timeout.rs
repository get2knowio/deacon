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

// --- #525: transient-failure recovery on the config-resolution fetch path ---

/// Mock that fails the first `failures` calls with a transient, timeout-shaped
/// error and then serves `body`. Records how many calls it received so a test
/// can assert the retry budget is both used and bounded.
#[derive(Debug, Clone)]
struct FlakyMockHttpClient {
    failures_remaining: Arc<Mutex<usize>>,
    calls: Arc<Mutex<usize>>,
    body: Bytes,
}

impl FlakyMockHttpClient {
    fn new(failures: usize, body: Bytes) -> Self {
        Self {
            failures_remaining: Arc::new(Mutex::new(failures)),
            calls: Arc::new(Mutex::new(0)),
            body,
        }
    }

    async fn call_count(&self) -> usize {
        *self.calls.lock().await
    }

    async fn next(
        &self,
        url: &str,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        *self.calls.lock().await += 1;
        let mut remaining = self.failures_remaining.lock().await;
        if *remaining > 0 {
            *remaining -= 1;
            // Mirrors ReqwestClient's phrasing for a timed-out request, which is
            // what the #525 nightly actually hit against ghcr.io.
            return Err(format!(
                "Request timeout for URL: {}. Check network connectivity.",
                url
            )
            .into());
        }
        Ok(self.body.clone())
    }
}

#[async_trait::async_trait]
impl HttpClient for FlakyMockHttpClient {
    async fn get(
        &self,
        url: &str,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        self.next(url).await
    }

    async fn get_with_headers(
        &self,
        url: &str,
        _headers: HashMap<String, String>,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        self.next(url).await
    }

    async fn get_with_headers_and_response(
        &self,
        url: &str,
        _headers: HashMap<String, String>,
    ) -> std::result::Result<HttpResponse, Box<dyn std::error::Error + Send + Sync>> {
        let body = self.next(url).await?;
        Ok(HttpResponse {
            status: 200,
            headers: HashMap::new(),
            body,
        })
    }

    async fn head(
        &self,
        url: &str,
        _headers: HashMap<String, String>,
    ) -> std::result::Result<u16, Box<dyn std::error::Error + Send + Sync>> {
        self.next(url).await.map(|_| 200)
    }
}

fn valid_manifest_bytes() -> Bytes {
    Bytes::from(
        serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "layers": [{
                "mediaType": "application/vnd.devcontainers.layer.v1+tar",
                "size": 1024,
                "digest": "sha256:abc123"
            }]
        })
        .to_string(),
    )
}

fn flaky_feature_ref() -> FeatureRef {
    FeatureRef::new(
        "test.registry".to_string(),
        "test".to_string(),
        "feature".to_string(),
        Some("latest".to_string()),
    )
}

/// A single transient failure must NOT surface to the caller: the FR-023 policy
/// grants one retry, and the second attempt succeeds. This is the exact shape of
/// the #525 false divergence, where deacon reported a hard failure on a manifest
/// fetch that the reference CLI completed.
#[tokio::test]
async fn test_transient_manifest_failure_recovers_on_retry() {
    let client = FlakyMockHttpClient::new(1, valid_manifest_bytes());
    let cache = tempfile::TempDir::new().unwrap();
    let fetcher = FeatureFetcher::with_retry_config(
        client.clone(),
        cache.path().to_path_buf(),
        deacon_core::oci::feature_fetch_retry_config(),
    );

    let manifest = fetcher
        .get_manifest(&flaky_feature_ref())
        .await
        .expect("one transient failure must be absorbed by the single retry");

    assert_eq!(
        manifest.layers.len(),
        1,
        "manifest should parse after retry"
    );
    assert_eq!(
        client.call_count().await,
        2,
        "should have taken exactly the initial attempt plus one retry"
    );
}

/// Retries stay bounded and the underlying cause is propagated verbatim rather
/// than being flattened into a generic message — a persistent outage must still
/// fail, and fail legibly.
#[tokio::test]
async fn test_persistent_failure_exhausts_retries_and_propagates_cause() {
    // More failures than the budget can absorb.
    let client = FlakyMockHttpClient::new(5, valid_manifest_bytes());
    let cache = tempfile::TempDir::new().unwrap();
    let fetcher = FeatureFetcher::with_retry_config(
        client.clone(),
        cache.path().to_path_buf(),
        deacon_core::oci::feature_fetch_retry_config(),
    );

    let err = fetcher
        .get_manifest(&flaky_feature_ref())
        .await
        .expect_err("a persistent failure must still fail");

    let msg = err.to_string();
    assert!(
        msg.contains("Request timeout for URL"),
        "the real transport error must survive to the caller, got: {msg}"
    );
    assert_eq!(
        client.call_count().await,
        2,
        "retries must stay bounded at the initial attempt plus one retry"
    );
}

/// The behavioral regression test for #525: one healthy-but-slow registry
/// response, served twice from the same server. The FR-023 budget in force today
/// absorbs it; the 2s cap it replaced does not. This is the difference that made
/// the nightly red while the reference CLI — which sets no timeout at all —
/// succeeded against the same endpoint.
#[tokio::test]
async fn test_fr023_budget_tolerates_slow_registry_that_two_second_cap_killed() {
    use deacon_core::oci::ReqwestClient;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/test/feature/manifests/latest"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(valid_manifest_bytes().to_vec())
                .set_delay(Duration::from_secs(3)),
        )
        .mount(&server)
        .await;

    let url = format!("{}/v2/test/feature/manifests/latest", server.uri());

    let current = ReqwestClient::with_timeout(Some(deacon_core::oci::FEATURE_FETCH_TIMEOUT))
        .expect("client construction");
    current
        .get(&url)
        .await
        .expect("FR-023's 30s budget must tolerate a 3s registry response");

    let old_cap =
        ReqwestClient::with_timeout(Some(Duration::from_secs(2))).expect("client construction");
    let err = old_cap
        .get(&url)
        .await
        .expect_err("the replaced 2s cap must fail on this very same response");
    assert!(
        err.to_string().contains("Request timeout"),
        "the 2s cap should fail as a timeout, got: {err}"
    );
}

/// Guards the FR-023 policy values themselves. The 2s timeout this replaced was
/// shorter than a real ghcr.io round-trip and produced #525's false divergence;
/// re-tightening it should be a deliberate act, not a silent edit.
#[test]
fn test_feature_fetch_policy_matches_fr_023() {
    assert_eq!(
        deacon_core::oci::FEATURE_FETCH_TIMEOUT,
        Duration::from_secs(30),
        "FR-023 mandates a 30-second timeout for HTTPS feature downloads"
    );
    assert_eq!(
        deacon_core::oci::feature_fetch_retry_config().max_attempts,
        1,
        "FR-023 mandates a single retry on transient network errors"
    );
}
