//! Response types for the up command.
//!
//! This module contains:
//! - `UpSuccess` - Success response structure
//! - `UpError` - Error response structure
//! - `UpResult` - Union type for up command results
//! - `UpContainerInfo` - Internal container information structure

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Success response for the up command, emitted as JSON to stdout.
///
/// Per the `deacon up` contract (specs/001-up-gap-spec/contracts/up.md),
/// exactly one JSON document MUST be written to stdout on success with exit code 0.
/// All logs go to stderr.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpSuccess {
    /// Always "success" for successful outcomes
    pub outcome: String,

    /// ID of the created or reused container
    pub container_id: String,

    /// Compose project name (only present for compose-based configurations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compose_project_name: Option<String>,

    /// Remote user inside the container
    pub remote_user: String,

    /// Remote workspace folder path inside the container
    pub remote_workspace_folder: String,

    /// Configuration object (only when includeConfiguration flag is set)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration: Option<serde_json::Value>,

    /// Merged configuration object (only when includeMergedConfiguration flag is set)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merged_configuration: Option<serde_json::Value>,
}

/// Error response for the up command, emitted as JSON to stdout.
///
/// Per the `deacon up` contract (specs/001-up-gap-spec/contracts/up.md),
/// exactly one JSON document MUST be written to stdout on error with exit code 1.
/// All logs go to stderr. Secrets must be redacted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpError {
    /// Always "error" for error outcomes
    pub outcome: String,

    /// Short error message
    pub message: String,

    /// Detailed error description
    pub description: String,

    /// Container ID (if container was created before error)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,

    /// Disallowed feature ID (if error was due to disallowed feature)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disallowed_feature_id: Option<String>,

    /// Whether the container was stopped during error handling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub did_stop_container: Option<bool>,

    /// Optional URL for more information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub learn_more_url: Option<String>,
}

/// Union type for up command results to enforce stdout JSON contract.
///
/// The contract requires exactly one JSON document on stdout (success or error).
/// This type provides builder methods and serialization helpers to emit the correct format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum UpResult {
    Success(UpSuccess),
    Error(UpError),
}

impl UpResult {
    /// Create a success result
    pub fn success(
        container_id: String,
        remote_user: String,
        remote_workspace_folder: String,
    ) -> Self {
        UpResult::Success(UpSuccess {
            outcome: "success".to_string(),
            container_id,
            compose_project_name: None,
            remote_user,
            remote_workspace_folder,
            configuration: None,
            merged_configuration: None,
        })
    }

    /// Create an error result
    pub fn error(message: String, description: String) -> Self {
        UpResult::Error(UpError {
            outcome: "error".to_string(),
            message,
            description,
            container_id: None,
            disallowed_feature_id: None,
            did_stop_container: None,
            learn_more_url: None,
        })
    }

    /// Add compose project name to a success result
    pub fn with_compose_project_name(mut self, project_name: String) -> Self {
        if let UpResult::Success(ref mut success) = self {
            success.compose_project_name = Some(project_name);
        }
        self
    }

    /// Add configuration to a success result
    pub fn with_configuration(mut self, configuration: serde_json::Value) -> Self {
        if let UpResult::Success(ref mut success) = self {
            success.configuration = Some(configuration);
        }
        self
    }

    /// Add merged configuration to a success result
    pub fn with_merged_configuration(mut self, merged_configuration: serde_json::Value) -> Self {
        if let UpResult::Success(ref mut success) = self {
            success.merged_configuration = Some(merged_configuration);
        }
        self
    }

    /// Add container ID to an error result
    #[allow(dead_code)] // TODO: Will be used in T011 for error scenarios
    pub fn with_container_id(mut self, container_id: String) -> Self {
        match self {
            UpResult::Success(_) => self,
            UpResult::Error(ref mut error) => {
                error.container_id = Some(container_id);
                self
            }
        }
    }

    /// Add disallowed feature ID to an error result
    #[allow(dead_code)] // TODO: Will be used in T029 for disallowed features
    pub fn with_disallowed_feature_id(mut self, feature_id: String) -> Self {
        if let UpResult::Error(ref mut error) = self {
            error.disallowed_feature_id = Some(feature_id);
        }
        self
    }

    /// Mark that container was stopped during error handling
    #[allow(dead_code)] // TODO: Will be used in T011 for error scenarios
    pub fn with_did_stop_container(mut self, stopped: bool) -> Self {
        if let UpResult::Error(ref mut error) = self {
            error.did_stop_container = Some(stopped);
        }
        self
    }

    /// Add learn more URL to an error result
    #[allow(dead_code)] // TODO: Will be used in T011 for error scenarios
    pub fn with_learn_more_url(mut self, url: String) -> Self {
        if let UpResult::Error(ref mut error) = self {
            error.learn_more_url = Some(url);
        }
        self
    }

    /// Emit this result as JSON to stdout and return appropriate exit code.
    ///
    /// Per contract: stdout receives exactly one JSON document, stderr receives logs.
    /// Returns 0 for success, 1 for error.
    #[allow(dead_code)] // TODO: Alternative to inline JSON emission in cli.rs
    pub fn emit(&self) -> Result<i32> {
        let json = serde_json::to_string_pretty(self)?;
        println!("{}", json);

        match self {
            UpResult::Success(_) => Ok(0),
            UpResult::Error(_) => Ok(1),
        }
    }

    /// Check if this is a success result
    #[allow(dead_code)] // TODO: Helper method for future use
    pub fn is_success(&self) -> bool {
        matches!(self, UpResult::Success(_))
    }

    /// Check if this is an error result
    #[allow(dead_code)] // TODO: Helper method for future use
    pub fn is_error(&self) -> bool {
        matches!(self, UpResult::Error(_))
    }

    /// Map an anyhow::Error to a standardized user-facing error message.
    ///
    /// This provides consistent, actionable error messages following the contract
    /// in specs/001-up-gap-spec/contracts/up.md and the fail-fast validation strategy
    /// from research.md.
    ///
    /// Error categories:
    /// - Config errors (NotFound, Validation, Parsing): User-facing messages for invalid inputs
    /// - Docker/Runtime errors: Clear messages about container/image issues
    /// - Feature errors: Disallowed features or feature resolution failures
    /// - Network/Authentication: Connection and auth issues
    /// - Generic errors: Fallback with debug info
    pub fn from_error(error: anyhow::Error) -> Self {
        use deacon_core::errors::{ConfigError, DeaconError, DockerError};

        // Try to downcast to DeaconError for specific handling
        if let Some(deacon_error) = error.downcast_ref::<DeaconError>() {
            match deacon_error {
                DeaconError::Config(config_error) => match config_error {
                    ConfigError::NotFound { path } => UpResult::error(
                        "No devcontainer.json found in workspace".to_string(),
                        format!("Configuration file not found: {}", path),
                    ),
                    ConfigError::Validation { message } => UpResult::error(
                        "Invalid configuration or arguments".to_string(),
                        message.clone(),
                    ),
                    ConfigError::Parsing { message } => UpResult::error(
                        "Failed to parse configuration file".to_string(),
                        message.clone(),
                    ),
                    ConfigError::ExtendsCycle { chain } => UpResult::error(
                        "Configuration extends cycle detected".to_string(),
                        format!("Cycle in extends chain: {}", chain),
                    ),
                    ConfigError::NotImplemented { feature } => UpResult::error(
                        "Feature not implemented".to_string(),
                        format!("Feature '{}' is not yet implemented", feature),
                    ),
                    ConfigError::Io(io_err) => UpResult::error(
                        "Failed to read configuration file".to_string(),
                        format!("{}", io_err),
                    ),
                },
                DeaconError::Docker(docker_error) => match docker_error {
                    DockerError::NotInstalled => UpResult::error(
                        "Docker is not installed or not accessible".to_string(),
                        "Please ensure Docker is installed and running".to_string(),
                    ),
                    DockerError::CLIError(msg) => {
                        UpResult::error("Docker CLI operation failed".to_string(), msg.clone())
                    }
                    DockerError::ContainerNotFound { id } => UpResult::error(
                        "Container not found".to_string(),
                        format!("Container with ID '{}' was not found", id),
                    ),
                    DockerError::ExecFailed { code } => UpResult::error(
                        "Container command failed".to_string(),
                        format!("Command exited with code {}", code),
                    ),
                    DockerError::TTYFailed { reason } => {
                        UpResult::error("TTY allocation failed".to_string(), reason.clone())
                    }
                },
                DeaconError::Network { message } => {
                    UpResult::error("Network error".to_string(), message.clone())
                }
                DeaconError::Authentication { message } => {
                    UpResult::error("Authentication failed".to_string(), message.clone())
                }
                _ => {
                    // Other DeaconError variants - use generic formatting
                    let message = format!("{}", deacon_error);
                    let description = format!("{:?}", deacon_error);
                    UpResult::error(message, description)
                }
            }
        } else {
            // Generic error fallback
            let message = format!("{:#}", error);
            let description = format!("{:?}", error);
            UpResult::error(message, description)
        }
    }
}

/// Internal structure to pass container information from execute_up_with_runtime
#[derive(Debug, Clone)]
pub struct UpContainerInfo {
    pub container_id: String,
    pub remote_user: String,
    pub remote_workspace_folder: String,
    pub compose_project_name: Option<String>,
    pub configuration: Option<serde_json::Value>,
    pub merged_configuration: Option<serde_json::Value>,
}
