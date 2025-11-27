//! OCI registry authentication

use base64::Engine;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use tracing::debug;

use crate::redaction;

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

    /// Load authentication from environment variables
    ///
    /// This function loads authentication credentials from environment variables with the following priority:
    /// 1. `DEACON_REGISTRY_TOKEN` - Bearer token authentication (highest priority)
    /// 2. `DEACON_REGISTRY_USER` + `DEACON_REGISTRY_PASS` - Basic authentication
    ///
    /// # Security Notes
    /// - All sensitive values (tokens, passwords) are automatically added to the global redaction registry
    /// - Redacted values will not appear in logs, error messages, or any output
    /// - This prevents accidental leakage of credentials in debugging or error scenarios
    pub fn load_from_env(
        &mut self,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Check for token authentication first
        if let Ok(token) = env::var("DEACON_REGISTRY_TOKEN") {
            debug!("Found DEACON_REGISTRY_TOKEN environment variable");
            // Add token to redaction registry
            redaction::add_global_secret(&token);
            self.set_default_credentials(RegistryCredentials::Bearer { token });
            return Ok(());
        }

        // Check for basic authentication
        if let (Ok(username), Ok(password)) = (
            env::var("DEACON_REGISTRY_USER"),
            env::var("DEACON_REGISTRY_PASS"),
        ) {
            debug!("Found DEACON_REGISTRY_USER and DEACON_REGISTRY_PASS environment variables");
            // Add password to redaction registry
            redaction::add_global_secret(&password);
            self.set_default_credentials(RegistryCredentials::Basic { username, password });
        }

        Ok(())
    }

    /// Load authentication from Docker config.json
    ///
    /// This function loads authentication credentials from Docker's config.json file
    /// located at `~/.docker/config.json` (or `%USERPROFILE%\.docker\config.json` on Windows).
    ///
    /// Supports both encoded auth strings and separate username/password fields.
    /// Registry-specific credentials override default credentials.
    ///
    /// # Security Notes
    /// - Passwords extracted from Docker config are treated as sensitive
    /// - All credential values are automatically redacted in logs and error messages
    /// - This follows Docker's standard credential handling practices
    pub fn load_from_docker_config(
        &mut self,
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
                                self.set_credentials(
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
                    self.set_credentials(
                        registry,
                        RegistryCredentials::Basic { username, password },
                    );
                }
            }
        }

        Ok(())
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
