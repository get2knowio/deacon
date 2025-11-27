//! HTTP client implementations for OCI registry communication

use bytes::Bytes;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, warn};

use super::auth::{RegistryAuth, RegistryCredentials};
use super::types::HttpResponse;

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

    /// PUT request with data and headers
    async fn put_with_headers(
        &self,
        url: &str,
        data: Bytes,
        headers: HashMap<String, String>,
    ) -> std::result::Result<Bytes, Box<dyn std::error::Error + Send + Sync>>;

    /// POST request with data and headers, returns full response with headers
    async fn post_with_headers(
        &self,
        url: &str,
        data: Bytes,
        headers: HashMap<String, String>,
    ) -> std::result::Result<HttpResponse, Box<dyn std::error::Error + Send + Sync>>;
}

/// Default HTTP client implementation using reqwest
#[derive(Debug, Clone)]
pub struct ReqwestClient {
    client: reqwest::Client,
    auth: RegistryAuth,
}

impl ReqwestClient {
    /// Create a new ReqwestClient with default configuration (no timeout)
    pub fn new() -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
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
        let mut client_builder = reqwest::Client::builder();

        // Configure timeout if specified
        if let Some(timeout_duration) = timeout {
            client_builder = client_builder.timeout(timeout_duration);
            debug!(
                "Configured HTTP client with timeout: {:?}",
                timeout_duration
            );
        }

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
        auth.load_from_env()?;
        auth.load_from_docker_config()?;

        Ok(Self { client, auth })
    }

    /// Create a new ReqwestClient with custom authentication configuration
    pub fn with_auth_config(
        auth: RegistryAuth,
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client_builder = reqwest::Client::builder();

        // Note: We don't load custom CA certificates here since this method is for explicit config
        // If CA certificates are needed, they should be handled by the caller

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
    ) -> std::result::Result<HttpResponse, Box<dyn std::error::Error + Send + Sync>> {
        // Add authentication header if available
        let credentials = self.get_credentials_for_url(url);
        if let Some(auth_header) = credentials.to_auth_header() {
            headers.insert("Authorization".to_string(), auth_header);
        }

        let mut request = self.client.post(url).body(data);
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

        let status = response.status().as_u16();

        // Extract headers
        let mut response_headers = HashMap::new();
        for (key, value) in response.headers() {
            if let Ok(value_str) = value.to_str() {
                response_headers.insert(key.to_string(), value_str.to_string());
            }
        }

        // Handle 401 authentication errors
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(format!("Authentication failed for URL: {}", url).into());
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
}
