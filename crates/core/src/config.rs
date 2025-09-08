//! Configuration resolution and parsing
//!
//! This module handles devcontainer.json parsing following the Development Containers Specification.
//! It supports JSON-with-comments (JSONC) parsing using the json5 crate to handle comments and
//! trailing commas commonly found in devcontainer configuration files.
//!
//! The configuration model mirrors the subset of fields needed for early implementation,
//! with full type safety for known fields and flexibility for future extensions.
//!
//! ## Configuration Resolution Workflow
//!
//! The configuration resolution follows the workflow outlined in the CLI specification:
//! 1. Load base configuration from devcontainer.json/devcontainer.jsonc
//! 2. Parse and validate known fields
//! 3. Log unknown fields at DEBUG level for future compatibility
//! 4. Apply basic validation rules
//! 5. Return strongly typed configuration
//!
//! ## References
//!
//! This implementation aligns with the [Development Containers Specification](https://containers.dev/implementors/spec/)
//! and follows the configuration resolution workflow defined in the CLI specification.

use crate::errors::{DeaconError, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, instrument};

/// Default function to return an empty JSON object for serde defaults.
fn default_empty_object() -> serde_json::Value {
    serde_json::Value::Object(Default::default())
}

/// DevContainer configuration structure following the Development Containers Specification.
///
/// This struct represents the subset of fields needed for early implementation, mirroring
/// the configuration schema defined at containers.dev.
///
/// Optional arrays default to empty vectors and maps default to empty hash maps for
/// ergonomic usage. Features and customizations are kept as raw `serde_json::Value`
/// for initial implementation flexibility.
///
/// ## References
///
/// - [DevContainer Configuration Reference](https://containers.dev/implementors/json_reference/)
/// - [Container Configuration](https://containers.dev/implementors/json_reference/#container-configuration)
/// - [Lifecycle Commands](https://containers.dev/implementors/json_reference/#lifecycle-scripts)
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevContainerConfig {
    /// Human-readable name for the development container.
    ///
    /// Reference: [Container Configuration - name](https://containers.dev/implementors/json_reference/#name)
    pub name: Option<String>,

    /// Container image to use.
    ///
    /// Reference: [Container Configuration - image](https://containers.dev/implementors/json_reference/#image)
    pub image: Option<String>,

    /// Path to Dockerfile relative to devcontainer.json.
    ///
    /// Reference: [Container Configuration - dockerFile](https://containers.dev/implementors/json_reference/#dockerfile)
    #[serde(rename = "dockerFile")]
    pub dockerfile: Option<String>,

    /// Build configuration when using a Dockerfile.
    ///
    /// Reference: [Container Configuration - build](https://containers.dev/implementors/json_reference/#build)
    pub build: Option<serde_json::Value>,

    /// Features to install in the container.
    ///
    /// Kept as raw JSON value for initial implementation. Will be strongly typed in future iterations.
    ///
    /// Reference: [Features](https://containers.dev/implementors/json_reference/#features)
    #[serde(default = "default_empty_object")]
    pub features: serde_json::Value,

    /// Tool-specific customizations.
    ///
    /// Kept as raw JSON value for initial implementation.
    ///
    /// Reference: [Customizations](https://containers.dev/implementors/json_reference/#customizations)
    #[serde(default = "default_empty_object")]
    pub customizations: serde_json::Value,

    /// Path to workspace folder inside the container.
    ///
    /// Reference: [Workspace Configuration - workspaceFolder](https://containers.dev/implementors/json_reference/#workspace-folder)
    pub workspace_folder: Option<String>,

    /// Additional mount points for the container.
    ///
    /// Reference: [Container Configuration - mounts](https://containers.dev/implementors/json_reference/#mounts)
    #[serde(default)]
    pub mounts: Vec<serde_json::Value>,

    /// Environment variables to set in the container.
    ///
    /// Reference: [Environment Variables - containerEnv](https://containers.dev/implementors/json_reference/#container-env)
    #[serde(default)]
    pub container_env: HashMap<String, String>,

    /// Environment variables to set in the remote environment.
    ///
    /// Reference: [Environment Variables - remoteEnv](https://containers.dev/implementors/json_reference/#remote-env)
    #[serde(default)]
    pub remote_env: HashMap<String, Option<String>>,

    /// Ports to forward from the container.
    ///
    /// Reference: [Port Configuration - forwardPorts](https://containers.dev/implementors/json_reference/#forward-ports)
    #[serde(default)]
    pub forward_ports: Vec<serde_json::Value>,

    /// Primary application port.
    ///
    /// Reference: [Port Configuration - appPort](https://containers.dev/implementors/json_reference/#app-port)
    pub app_port: Option<serde_json::Value>,

    /// Additional arguments to pass to docker run.
    ///
    /// Reference: [Container Configuration - runArgs](https://containers.dev/implementors/json_reference/#run-args)
    #[serde(default)]
    pub run_args: Vec<String>,

    /// Action to take when shutting down the container.
    ///
    /// Reference: [Container Configuration - shutdownAction](https://containers.dev/implementors/json_reference/#shutdown-action)
    pub shutdown_action: Option<String>,

    /// Whether to override the default command.
    ///
    /// Reference: [Container Configuration - overrideCommand](https://containers.dev/implementors/json_reference/#override-command)
    pub override_command: Option<bool>,

    /// Command to run once after the container is created.
    ///
    /// Reference: [Lifecycle Commands - onCreateCommand](https://containers.dev/implementors/json_reference/#lifecycle-scripts)
    pub on_create_command: Option<serde_json::Value>,

    /// Command to run each time the container starts.
    ///
    /// Reference: [Lifecycle Commands - postStartCommand](https://containers.dev/implementors/json_reference/#lifecycle-scripts)
    pub post_start_command: Option<serde_json::Value>,

    /// Command to run after the container is created and connected.
    ///
    /// Reference: [Lifecycle Commands - postCreateCommand](https://containers.dev/implementors/json_reference/#lifecycle-scripts)
    pub post_create_command: Option<serde_json::Value>,

    /// Command to run each time a tool attaches to the container.
    ///
    /// Reference: [Lifecycle Commands - postAttachCommand](https://containers.dev/implementors/json_reference/#lifecycle-scripts)
    pub post_attach_command: Option<serde_json::Value>,

    /// Command to run before other commands when the container is created.
    ///
    /// Reference: [Lifecycle Commands - initializeCommand](https://containers.dev/implementors/json_reference/#lifecycle-scripts)
    pub initialize_command: Option<serde_json::Value>,

    /// Command to run when updating content (e.g., git pull).
    ///
    /// Reference: [Lifecycle Commands - updateContentCommand](https://containers.dev/implementors/json_reference/#lifecycle-scripts)
    pub update_content_command: Option<serde_json::Value>,
}

impl Default for DevContainerConfig {
    fn default() -> Self {
        Self {
            name: None,
            image: None,
            dockerfile: None,
            build: None,
            features: default_empty_object(),
            customizations: default_empty_object(),
            workspace_folder: None,
            mounts: Vec::new(),
            container_env: HashMap::new(),
            remote_env: HashMap::new(),
            forward_ports: Vec::new(),
            app_port: None,
            run_args: Vec::new(),
            shutdown_action: None,
            override_command: None,
            on_create_command: None,
            post_start_command: None,
            post_create_command: None,
            post_attach_command: None,
            initialize_command: None,
            update_content_command: None,
        }
    }
}

/// Configuration loader for DevContainer configurations.
///
/// Provides methods to load and parse devcontainer.json/devcontainer.jsonc files
/// with support for JSON-with-comments parsing and comprehensive error handling.
///
/// ## Example
///
/// ```rust
/// use deacon_core::config::ConfigLoader;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// let config = ConfigLoader::load_from_path(Path::new("devcontainer.jsonc"))?;
/// println!("Loaded configuration: {}", config.name.unwrap_or_default());
/// # Ok(())
/// # }
/// ```
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load DevContainer configuration from a file path.
    ///
    /// This method:
    /// 1. Reads the file as UTF-8 text
    /// 2. Parses JSON-with-comments using json5
    /// 3. Deserializes into strongly typed configuration
    /// 4. Logs unknown top-level keys at DEBUG level
    /// 5. Performs basic validation
    ///
    /// ## Arguments
    ///
    /// * `path` - Path to the devcontainer.json or devcontainer.jsonc file
    ///
    /// ## Returns
    ///
    /// Returns `Ok(DevContainerConfig)` on success, or various error types:
    /// - `ConfigurationNotFound` if the file doesn't exist
    /// - `ConfigurationIo` for I/O errors
    /// - `ConfigurationParse` for JSON parsing errors
    /// - `ConfigurationValidation` for validation errors
    /// - `NotImplemented` if unsupported features are encountered
    ///
    /// ## Example
    ///
    /// ```rust
    /// use deacon_core::config::ConfigLoader;
    /// use std::path::Path;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let config = ConfigLoader::load_from_path(Path::new(".devcontainer/devcontainer.json"))?;
    /// if let Some(name) = &config.name {
    ///     println!("Container name: {}", name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip_all, fields(path = %path.display()))]
    pub fn load_from_path(path: &Path) -> Result<DevContainerConfig> {
        debug!("Loading DevContainer configuration from {}", path.display());

        // Check if file exists
        if !path.exists() {
            return Err(DeaconError::ConfigurationNotFound {
                path: path.display().to_string(),
            });
        }

        // Read file content
        let content = std::fs::read_to_string(path).map_err(|e| {
            debug!("Failed to read configuration file: {}", e);
            DeaconError::ConfigurationIo { source: e }
        })?;

        // Parse JSON5 (JSON with comments and trailing commas)
        let raw_value: serde_json::Value = json5::from_str(&content).map_err(|e| {
            debug!("Failed to parse configuration file: {}", e);
            DeaconError::ConfigurationParse {
                message: format!("JSON parsing error: {}", e),
            }
        })?;

        // Log unknown top-level keys at DEBUG level
        if let serde_json::Value::Object(obj) = &raw_value {
            Self::log_unknown_keys(obj);
        }

        // Check for extends field (not yet implemented)
        if let serde_json::Value::Object(obj) = &raw_value {
            if obj.contains_key("extends") {
                return Err(DeaconError::NotImplemented {
                    feature: "extends configuration".to_string(),
                });
            }
        }

        // Deserialize into strongly typed structure
        let config: DevContainerConfig = serde_json::from_value(raw_value).map_err(|e| {
            debug!("Failed to deserialize configuration: {}", e);
            DeaconError::ConfigurationValidation {
                message: format!("Deserialization error: {}", e),
            }
        })?;

        // Basic validation
        Self::validate_config(&config)?;

        debug!(
            "Successfully loaded configuration with name: {:?}",
            config.name
        );
        Ok(config)
    }

    /// Log unknown top-level keys at DEBUG level.
    ///
    /// This helps with forward compatibility by informing users of configuration
    /// keys that are not yet supported without failing the configuration load.
    fn log_unknown_keys(obj: &serde_json::Map<String, serde_json::Value>) {
        let known_keys = [
            "name",
            "image",
            "dockerFile",
            "build",
            "features",
            "customizations",
            "workspaceFolder",
            "mounts",
            "containerEnv",
            "remoteEnv",
            "forwardPorts",
            "appPort",
            "runArgs",
            "shutdownAction",
            "overrideCommand",
            "onCreateCommand",
            "postStartCommand",
            "postCreateCommand",
            "postAttachCommand",
            "initializeCommand",
            "updateContentCommand",
        ];

        for key in obj.keys() {
            if !known_keys.contains(&key.as_str()) {
                debug!("Unknown configuration key '{}' - will be ignored", key);
            }
        }
    }

    /// Perform basic validation on the loaded configuration.
    ///
    /// Validates that the configuration is internally consistent and contains
    /// valid combinations of fields.
    fn validate_config(config: &DevContainerConfig) -> Result<()> {
        // Validate that either image or dockerfile is specified (but not both)
        match (&config.image, &config.dockerfile) {
            (Some(_), Some(_)) => {
                return Err(DeaconError::ConfigurationValidation {
                    message: "Cannot specify both 'image' and 'dockerFile' - choose one"
                        .to_string(),
                });
            }
            (None, None) => {
                debug!("Neither 'image' nor 'dockerFile' specified - this may be intended for extends or compose configurations");
            }
            _ => {
                // Valid: exactly one is specified
            }
        }

        // Validate shutdown action values
        if let Some(action) = &config.shutdown_action {
            match action.as_str() {
                "none" | "stopContainer" => {
                    // Valid values
                }
                _ => {
                    return Err(DeaconError::ConfigurationValidation {
                        message: format!(
                            "Invalid shutdownAction '{}' - must be 'none' or 'stopContainer'",
                            action
                        ),
                    });
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_config_default() {
        let config = DevContainerConfig::default();
        assert_eq!(config.name, None);
        assert_eq!(config.image, None);
        assert_eq!(config.dockerfile, None);
        assert_eq!(config.mounts.len(), 0);
        assert_eq!(config.container_env.len(), 0);
        assert_eq!(config.remote_env.len(), 0);
        assert_eq!(config.forward_ports.len(), 0);
        assert_eq!(config.run_args.len(), 0);
        assert!(config.features.is_object());
        assert!(config.customizations.is_object());
    }

    #[test]
    fn test_load_valid_config_with_comments() -> anyhow::Result<()> {
        let config_content = r#"{
            // This is a comment
            "name": "Test Container",
            "image": "ubuntu:20.04",
            "features": {
                "ghcr.io/devcontainers/features/common-utils:1": {}
            },
            "customizations": {
                "vscode": {
                    "extensions": ["rust-lang.rust-analyzer"]
                }
            },
            "forwardPorts": [3000, 8080],
            "containerEnv": {
                "ENVIRONMENT": "development"
            },
            "runArgs": ["--init"], // trailing comma
        }"#;

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(config_content.as_bytes())?;

        let config = ConfigLoader::load_from_path(temp_file.path())?;

        assert_eq!(config.name, Some("Test Container".to_string()));
        assert_eq!(config.image, Some("ubuntu:20.04".to_string()));
        assert_eq!(config.dockerfile, None);
        assert_eq!(config.forward_ports.len(), 2);
        assert_eq!(
            config.container_env.get("ENVIRONMENT"),
            Some(&"development".to_string())
        );
        assert_eq!(config.run_args, vec!["--init"]);

        Ok(())
    }

    #[test]
    fn test_load_file_not_found() {
        let result = ConfigLoader::load_from_path(Path::new("nonexistent.json"));
        assert!(result.is_err());
        match result.unwrap_err() {
            DeaconError::ConfigurationNotFound { path } => {
                assert!(path.contains("nonexistent.json"));
            }
            _ => panic!("Expected ConfigurationNotFound error"),
        }
    }

    #[test]
    fn test_load_invalid_json() -> anyhow::Result<()> {
        let config_content = r#"{
            "name": "Test",
            "invalid": json syntax
        }"#;

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(config_content.as_bytes())?;

        let result = ConfigLoader::load_from_path(temp_file.path());
        assert!(result.is_err());
        match result.unwrap_err() {
            DeaconError::ConfigurationParse { message } => {
                assert!(message.contains("JSON parsing error"));
            }
            _ => panic!("Expected ConfigurationParse error"),
        }

        Ok(())
    }

    #[test]
    fn test_validation_both_image_and_dockerfile() -> anyhow::Result<()> {
        let config_content = r#"{
            "name": "Test",
            "image": "ubuntu:20.04",
            "dockerFile": "Dockerfile"
        }"#;

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(config_content.as_bytes())?;

        let result = ConfigLoader::load_from_path(temp_file.path());
        assert!(result.is_err());
        match result.unwrap_err() {
            DeaconError::ConfigurationValidation { message } => {
                assert!(message.contains("Cannot specify both 'image' and 'dockerFile'"));
            }
            _ => panic!("Expected ConfigurationValidation error"),
        }

        Ok(())
    }

    #[test]
    fn test_validation_invalid_shutdown_action() -> anyhow::Result<()> {
        let config_content = r#"{
            "name": "Test",
            "image": "ubuntu:20.04",
            "shutdownAction": "invalid"
        }"#;

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(config_content.as_bytes())?;

        let result = ConfigLoader::load_from_path(temp_file.path());
        assert!(result.is_err());
        match result.unwrap_err() {
            DeaconError::ConfigurationValidation { message } => {
                assert!(message.contains("Invalid shutdownAction"));
            }
            _ => panic!("Expected ConfigurationValidation error"),
        }

        Ok(())
    }

    #[test]
    fn test_extends_not_implemented() -> anyhow::Result<()> {
        let config_content = r#"{
            "name": "Test",
            "extends": "../base/devcontainer.json"
        }"#;

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(config_content.as_bytes())?;

        let result = ConfigLoader::load_from_path(temp_file.path());
        assert!(result.is_err());
        match result.unwrap_err() {
            DeaconError::NotImplemented { feature } => {
                assert!(feature.contains("extends"));
            }
            _ => panic!("Expected NotImplemented error"),
        }

        Ok(())
    }

    #[test]
    fn test_unknown_keys_logged() -> anyhow::Result<()> {
        let config_content = r#"{
            "name": "Test",
            "image": "ubuntu:20.04",
            "unknownField": "some value",
            "anotherUnknown": 42
        }"#;

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(config_content.as_bytes())?;

        // This should succeed despite unknown keys
        let config = ConfigLoader::load_from_path(temp_file.path())?;
        assert_eq!(config.name, Some("Test".to_string()));
        assert_eq!(config.image, Some("ubuntu:20.04".to_string()));

        Ok(())
    }

    #[test]
    fn test_empty_arrays_and_objects_default() -> anyhow::Result<()> {
        let config_content = r#"{
            "name": "Test",
            "image": "ubuntu:20.04"
        }"#;

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(config_content.as_bytes())?;

        let config = ConfigLoader::load_from_path(temp_file.path())?;

        // Arrays should default to empty
        assert_eq!(config.mounts.len(), 0);
        assert_eq!(config.forward_ports.len(), 0);
        assert_eq!(config.run_args.len(), 0);

        // Maps should default to empty
        assert_eq!(config.container_env.len(), 0);
        assert_eq!(config.remote_env.len(), 0);

        // JSON objects should default to empty objects
        assert!(config.features.is_object());
        assert!(config.customizations.is_object());

        Ok(())
    }
}
