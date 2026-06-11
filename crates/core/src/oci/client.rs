//! HTTP client implementations for OCI registry communication

use bytes::Bytes;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::debug;

use super::auth::{RegistryAuth, RegistryCredentials};
use super::types::HttpResponse;
use crate::host_ca::enumerate_host_roots;

/// Layer the **host OS trust store** roots and the additive
/// `DEACON_CUSTOM_CA_BUNDLE` onto a reqwest client builder, on top of the
/// webpki public roots that `rustls-tls` already provides.
///
/// This is what makes deacon's own feature/template pulls "just work" behind a
/// corporate TLS-intercepting proxy (016, US1): the resulting trust set is the
/// **union** of webpki public roots + host roots + the custom bundle. Host-root
/// enumeration failure is logged and tolerated (the webpki base still works for
/// public registries) rather than failing every pull; individual host certs
/// reqwest cannot parse are skipped.
fn apply_host_and_custom_roots(
    mut builder: reqwest::ClientBuilder,
) -> std::result::Result<reqwest::ClientBuilder, Box<dyn std::error::Error + Send + Sync>> {
    // Host OS trust store (union with the webpki base). Best-effort: a store
    // read failure is logged and tolerated so public-registry pulls still work.
    match enumerate_host_roots() {
        Ok(ders) => {
            let mut added = 0usize;
            for der in &ders {
                match reqwest::Certificate::from_der(der) {
                    Ok(cert) => {
                        builder = builder.add_root_certificate(cert);
                        added += 1;
                    }
                    Err(e) => debug!("Skipping unparseable host root for HTTP client: {}", e),
                }
            }
            debug!("Added {} host trust-store root(s) to HTTP client", added);
        }
        Err(e) => {
            debug!(
                "Host trust-store enumeration unavailable for HTTP client (continuing with public roots): {}",
                e
            );
        }
    }

    // Additive custom CA bundle. The user explicitly pointed at this file, so a
    // misconfiguration fails fast (unchanged from prior behavior — FR-002).
    if let Ok(ca_bundle_path) = env::var("DEACON_CUSTOM_CA_BUNDLE") {
        let ca_bundle = fs::read(&ca_bundle_path)?;
        let cert = reqwest::Certificate::from_pem(&ca_bundle)?;
        builder = builder.add_root_certificate(cert);
        debug!("Added custom CA certificate from: {}", ca_bundle_path);
    }

    Ok(builder)
}

/// HTTP client trait for OCI registry operations
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

    /// GET with custom headers that returns full response including headers (for pagination)
    async fn get_with_headers_and_response(
        &self,
        url: &str,
        headers: HashMap<String, String>,
    ) -> std::result::Result<HttpResponse, Box<dyn std::error::Error + Send + Sync>>;

    /// HEAD request to check resource existence without downloading body
    async fn head(
        &self,
        url: &str,
        headers: HashMap<String, String>,
    ) -> std::result::Result<u16, Box<dyn std::error::Error + Send + Sync>>;
}

/// Default HTTP client implementation using reqwest
#[derive(Debug, Clone)]
pub struct ReqwestClient {
    client: reqwest::Client,
    auth: RegistryAuth,
}

/// Default total request timeout for OCI HTTP operations.
///
/// Covers the slowest expected operation — large blob downloads. The
/// previous `None` (no timeout) default risked indefinite hangs on a
/// stalled connection, which is precisely the failure mode CI is supposed
/// to surface quickly. Callers needing a different bound should construct
/// via [`ReqwestClient::with_timeout`].
const DEFAULT_TOTAL_TIMEOUT: Duration = Duration::from_secs(300);

/// Default connect-phase timeout. Connect should be fast on a healthy
/// network — 10s is generous but bounds DNS + TCP handshake hangs.
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

impl ReqwestClient {
    /// Create a new ReqwestClient with sensible default timeouts
    /// (`DEFAULT_TOTAL_TIMEOUT` total, `DEFAULT_CONNECT_TIMEOUT` connect).
    /// Use [`Self::with_timeout`] for a custom total timeout, or
    /// [`Self::with_no_timeout`] for the legacy "wait forever" behavior
    /// (discouraged; reserved for explicit opt-in by callers that handle
    /// their own watchdogs).
    pub fn new() -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::with_timeout(Some(DEFAULT_TOTAL_TIMEOUT))
    }

    /// Create a client with timeouts disabled. Only call this when an
    /// upstream layer enforces its own timeout / cancellation token.
    pub fn with_no_timeout() -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>>
    {
        Self::with_timeout(None)
    }

    /// Parse WWW-Authenticate header and exchange for token
    ///
    /// Implements OCI Distribution Spec token authentication for anonymous access.
    /// Parses the Bearer challenge from WWW-Authenticate header and exchanges for a token.
    async fn exchange_token(
        &self,
        www_authenticate: &str,
    ) -> std::result::Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // Parse Bearer challenge: Bearer realm="...",service="...",scope="..."
        let mut realm = None;
        let mut service = None;
        let mut scope = None;

        // Simple parser for Bearer challenge parameters
        if let Some(bearer_params) = www_authenticate.strip_prefix("Bearer ") {
            for param in bearer_params.split(',') {
                let param = param.trim();
                if let Some((key, value)) = param.split_once('=') {
                    let value = value.trim_matches('"');
                    match key {
                        "realm" => realm = Some(value.to_string()),
                        "service" => service = Some(value.to_string()),
                        "scope" => scope = Some(value.to_string()),
                        _ => {}
                    }
                }
            }
        }

        let realm = realm.ok_or("Missing realm in WWW-Authenticate header")?;

        // Build token URL
        let mut token_url = realm;
        let mut params = Vec::new();
        if let Some(service) = service {
            params.push(format!("service={}", service));
        }
        if let Some(scope) = scope {
            params.push(format!("scope={}", scope));
        }
        if !params.is_empty() {
            token_url.push('?');
            token_url.push_str(&params.join("&"));
        }

        debug!("Exchanging for anonymous token at: {}", token_url);

        // Make token request (anonymous - no credentials)
        let response = self.client.get(&token_url).send().await?;

        if !response.status().is_success() {
            return Err(format!("Token exchange failed with status: {}", response.status()).into());
        }

        let token_response: serde_json::Value = response.json().await?;

        // Extract token from response
        let token = token_response
            .get("token")
            .or_else(|| token_response.get("access_token"))
            .and_then(|t| t.as_str())
            .ok_or("Token not found in response")?
            .to_string();

        debug!("Successfully obtained anonymous access token");
        Ok(token)
    }

    /// Create a new ReqwestClient with custom timeout configuration
    ///
    /// # Arguments
    /// * `timeout` - Optional timeout for all requests. If None, no timeout is applied.
    ///
    /// # Examples
    /// ```
    /// use deacon_core::oci::ReqwestClient;
    /// use std::time::Duration;
    ///
    /// // Create client with 2 second timeout
    /// let client = ReqwestClient::with_timeout(Some(Duration::from_secs(2)));
    /// ```
    pub fn with_timeout(
        timeout: Option<Duration>,
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Always bound the connect phase — DNS + TCP handshake should never
        // hang indefinitely even when the caller chose `None` for the total
        // timeout. The original `None`-without-connect-bound configuration
        // could turn a stalled registry into a CI deadlock.
        let mut client_builder =
            reqwest::Client::builder().connect_timeout(DEFAULT_CONNECT_TIMEOUT);

        // Configure timeout if specified
        if let Some(timeout_duration) = timeout {
            client_builder = client_builder.timeout(timeout_duration);
            debug!(
                "Configured HTTP client with timeout: {:?} (connect: {:?})",
                timeout_duration, DEFAULT_CONNECT_TIMEOUT
            );
        } else {
            debug!(
                "HTTP client with no total timeout (connect: {:?})",
                DEFAULT_CONNECT_TIMEOUT
            );
        }

        // Union the host OS trust store + additive DEACON_CUSTOM_CA_BUNDLE onto
        // the webpki base so deacon's own pulls trust a corporate proxy CA (US1).
        client_builder = apply_host_and_custom_roots(client_builder)?;

        // Build the client
        let client = client_builder.build()?;

        let mut auth = RegistryAuth::new();

        // Load authentication from environment and Docker config
        auth.load_from_env()?;
        auth.load_from_docker_config()?;

        Ok(Self { client, auth })
    }

    /// Create a new ReqwestClient with custom authentication configuration
    pub fn with_auth_config(
        auth: RegistryAuth,
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client_builder = reqwest::Client::builder();

        // Trust is consistent across every constructor: union the host OS trust
        // store + additive DEACON_CUSTOM_CA_BUNDLE here too (US1) so an
        // auth-configured client also validates a corporate proxy CA.
        let client_builder = apply_host_and_custom_roots(client_builder)?;

        // Build the client
        let client = client_builder.build()?;

        Ok(Self { client, auth })
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

// NOTE: deliberately no `Default` impl. The previous impl silently fell
// back to an unauthenticated client whenever auth setup failed (e.g. a
// malformed `~/.docker/config.json`) — the opposite of fail-fast. Callers
// must use [`ReqwestClient::new`] and propagate the `Result` so any auth-
// loading failure surfaces immediately and isn't masked as an anonymous
// 401 against a private registry.

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
        for (key, value) in &headers {
            request = request.header(key, value);
        }

        let response = request.send().await.map_err(|e| {
            // Improve error messages for common network issues
            if e.is_timeout() {
                format!("Request timeout for URL: {}. Check network connectivity.", url)
            } else if e.is_connect() {
                format!(
                    "Connection failed for URL: {}. Check if the registry is accessible and network connectivity is available.",
                    url
                )
            } else if e.is_request() {
                format!("Request error for URL: {}: {}", url, e)
            } else {
                format!("Network error for URL: {}: {}", url, e)
            }
        })?;

        // Handle 401 authentication errors with token exchange
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            // Try to get WWW-Authenticate header for token exchange
            if let Some(www_auth) = response.headers().get("www-authenticate") {
                if let Ok(www_auth_str) = www_auth.to_str() {
                    if www_auth_str.starts_with("Bearer ") {
                        debug!("Got 401 with Bearer challenge, attempting token exchange");

                        // Attempt token exchange for anonymous access
                        if let Ok(token) = self.exchange_token(www_auth_str).await {
                            // Retry request with the obtained token
                            let mut retry_headers = headers.clone();
                            retry_headers
                                .insert("Authorization".to_string(), format!("Bearer {}", token));

                            let mut retry_request = self.client.get(url);
                            for (key, value) in retry_headers {
                                retry_request = retry_request.header(&key, &value);
                            }

                            let retry_response = retry_request.send().await?;

                            if retry_response.status().is_success() {
                                return Ok(retry_response.bytes().await?);
                            }
                        }
                    }
                }
            }

            return Err(format!("Authentication failed for URL: {}", url).into());
        }

        // Handle other HTTP errors
        if !response.status().is_success() {
            return Err(format!("HTTP {} for URL: {}", response.status(), url).into());
        }

        let bytes = response.bytes().await?;
        Ok(bytes)
    }

    async fn get_with_headers_and_response(
        &self,
        url: &str,
        mut headers: HashMap<String, String>,
    ) -> std::result::Result<HttpResponse, Box<dyn std::error::Error + Send + Sync>> {
        // Add authentication header if available
        let credentials = self.get_credentials_for_url(url);
        if let Some(auth_header) = credentials.to_auth_header() {
            headers.insert("Authorization".to_string(), auth_header);
        }

        let mut request = self.client.get(url);
        for (key, value) in &headers {
            request = request.header(key, value);
        }

        let response = request.send().await.map_err(|e| {
            // Improve error messages for common network issues
            if e.is_timeout() {
                format!("Request timeout for URL: {}. Check network connectivity.", url)
            } else if e.is_connect() {
                format!(
                    "Connection failed for URL: {}. Check if the registry is accessible and network connectivity is available.",
                    url
                )
            } else if e.is_request() {
                format!("Request error for URL: {}: {}", url, e)
            } else {
                format!("Network error for URL: {}: {}", url, e)
            }
        })?;

        // Handle 401 authentication errors with token exchange
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            // Try to get WWW-Authenticate header for token exchange
            if let Some(www_auth) = response.headers().get("www-authenticate") {
                if let Ok(www_auth_str) = www_auth.to_str() {
                    if www_auth_str.starts_with("Bearer ") {
                        debug!("Got 401 with Bearer challenge, attempting token exchange");

                        // Attempt token exchange for anonymous access
                        if let Ok(token) = self.exchange_token(www_auth_str).await {
                            // Retry request with the obtained token
                            let mut retry_headers = headers.clone();
                            retry_headers
                                .insert("Authorization".to_string(), format!("Bearer {}", token));

                            let mut retry_request = self.client.get(url);
                            for (key, value) in &retry_headers {
                                retry_request = retry_request.header(key, value);
                            }

                            let retry_response = retry_request.send().await?;
                            let status = retry_response.status().as_u16();

                            // Extract headers from retry response
                            let mut response_headers = HashMap::new();
                            for (key, value) in retry_response.headers() {
                                if let Ok(value_str) = value.to_str() {
                                    response_headers.insert(key.to_string(), value_str.to_string());
                                }
                            }

                            if retry_response.status().is_success() {
                                let bytes = retry_response.bytes().await?;
                                return Ok(HttpResponse {
                                    status,
                                    headers: response_headers,
                                    body: bytes,
                                });
                            }
                        }
                    }
                }
            }

            return Err(format!("Authentication failed for URL: {}", url).into());
        }

        let status = response.status().as_u16();

        // Extract headers
        let mut response_headers = HashMap::new();
        for (key, value) in response.headers() {
            if let Ok(value_str) = value.to_str() {
                response_headers.insert(key.to_string(), value_str.to_string());
            }
        }

        // Handle other HTTP errors
        if !response.status().is_success() {
            return Err(format!("HTTP {} for URL: {}", response.status(), url).into());
        }

        let bytes = response.bytes().await?;
        Ok(HttpResponse {
            status,
            headers: response_headers,
            body: bytes,
        })
    }

    async fn head(
        &self,
        url: &str,
        mut headers: HashMap<String, String>,
    ) -> std::result::Result<u16, Box<dyn std::error::Error + Send + Sync>> {
        // Add authentication header if available
        let credentials = self.get_credentials_for_url(url);
        if let Some(auth_header) = credentials.to_auth_header() {
            headers.insert("Authorization".to_string(), auth_header);
        }

        let mut request = self.client.head(url);
        for (key, value) in headers {
            request = request.header(&key, &value);
        }

        let response = request.send().await.map_err(|e| {
            // Improve error messages for common network issues
            if e.is_timeout() {
                format!("Request timeout for URL: {}. Check network connectivity.", url)
            } else if e.is_connect() {
                format!(
                    "Connection failed for URL: {}. Check if the registry is accessible and network connectivity is available.",
                    url
                )
            } else if e.is_request() {
                format!("Request error for URL: {}: {}", url, e)
            } else {
                format!("Network error for URL: {}: {}", url, e)
            }
        })?;

        Ok(response.status().as_u16())
    }
}

/// Mock HTTP client for testing
#[derive(Debug, Clone)]
pub struct MockHttpClient {
    responses: Arc<Mutex<HashMap<String, Bytes>>>,
    response_with_headers: Arc<Mutex<HashMap<String, HttpResponse>>>,
    head_responses: Arc<Mutex<HashMap<String, u16>>>,
}

impl MockHttpClient {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
            response_with_headers: Arc::new(Mutex::new(HashMap::new())),
            head_responses: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn add_response(&self, url: String, response: Bytes) {
        let mut responses = self.responses.lock().await;
        responses.insert(url, response);
    }

    pub async fn add_response_with_headers(&self, url: String, response: HttpResponse) {
        let mut responses = self.response_with_headers.lock().await;
        responses.insert(url, response);
    }

    pub async fn add_head_response(&self, url: String, status: u16) {
        let mut responses = self.head_responses.lock().await;
        responses.insert(url, status);
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

    async fn get_with_headers_and_response(
        &self,
        url: &str,
        _headers: HashMap<String, String>,
    ) -> std::result::Result<HttpResponse, Box<dyn std::error::Error + Send + Sync>> {
        // Check for response with headers first
        let response_with_headers = self.response_with_headers.lock().await;
        if let Some(response) = response_with_headers.get(url) {
            return Ok(response.clone());
        }
        drop(response_with_headers);

        // Fall back to simple response
        let responses = self.responses.lock().await;
        responses
            .get(url)
            .cloned()
            .map(|body| HttpResponse {
                status: 200,
                headers: HashMap::new(),
                body,
            })
            .ok_or_else(|| format!("No mock response for URL: {}", url).into())
    }

    async fn head(
        &self,
        url: &str,
        _headers: HashMap<String, String>,
    ) -> std::result::Result<u16, Box<dyn std::error::Error + Send + Sync>> {
        let responses = self.head_responses.lock().await;
        responses
            .get(url)
            .copied()
            .ok_or_else(|| format!("No mock HEAD response for URL: {}", url).into())
    }
}

#[cfg(test)]
mod host_ca_trust_tests {
    use super::*;
    use crate::host_ca::enumerate_host_roots;

    /// A valid corporate CA PEM (shared with the host_ca discovery fixtures).
    const CORPORATE_CA_PEM: &str = include_str!("../host_ca/test_fixtures/corporate_ca.pem");

    #[test]
    fn host_root_enumeration_succeeds() {
        // US1: the same enumeration the HTTP client unions in. It must not
        // error on a normal host; on a CA-equipped host the count is > 0.
        let roots = enumerate_host_roots().expect("host root enumeration");
        // Don't hard-require > 0 (minimal CI images may have an empty store),
        // but where present the union is non-trivial.
        if !roots.is_empty() {
            assert!(reqwest::Certificate::from_der(&roots[0]).is_ok());
        }
    }

    #[test]
    fn client_builds_with_custom_bundle_additive() {
        let tmp = tempfile::TempDir::new().unwrap();
        let bundle = tmp.path().join("extra-roots.pem");
        std::fs::write(&bundle, CORPORATE_CA_PEM).unwrap();

        // temp-env serializes + restores the env var (edition-2024 safe).
        temp_env::with_var(
            "DEACON_CUSTOM_CA_BUNDLE",
            Some(bundle.to_str().unwrap()),
            || {
                // The custom PEM is layered on top of host + webpki roots.
                ReqwestClient::new().expect("client builds with additive custom bundle");
                ReqwestClient::with_no_timeout()
                    .expect("no-timeout client builds with additive custom bundle");
            },
        );
    }

    #[test]
    fn unreadable_custom_bundle_fails_fast() {
        // An explicitly-configured bundle path that can't be read must surface
        // (No Silent Fallbacks) rather than silently degrade to host+webpki.
        temp_env::with_var(
            "DEACON_CUSTOM_CA_BUNDLE",
            Some("/nonexistent/deacon/extra-roots.pem"),
            || {
                assert!(ReqwestClient::new().is_err());
            },
        );
    }
}
