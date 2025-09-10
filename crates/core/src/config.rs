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

use crate::errors::{ConfigError, DeaconError, Result};
use crate::variable::{SubstitutionContext, SubstitutionReport, VariableSubstitution};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, instrument};

/// Default function to return an empty JSON object for serde defaults.
fn default_empty_object() -> serde_json::Value {
    serde_json::Value::Object(Default::default())
}

/// Configuration file location information
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigLocation {
    /// Path to the configuration file
    pub path: PathBuf,
    /// Whether the file exists
    pub exists: bool,
}

impl ConfigLocation {
    /// Create a new ConfigLocation
    pub fn new(path: PathBuf) -> Self {
        let exists = path.exists();
        Self { path, exists }
    }

    /// Get the path as a reference
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Check if the configuration file exists
    pub fn exists(&self) -> bool {
        self.exists
    }
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

    /// Docker Compose file(s) to use for multi-container environments.
    ///
    /// Can be a single file path or an array of file paths.
    /// Reference: [Container Configuration - dockerComposeFile](https://containers.dev/implementors/json_reference/#docker-compose-file)
    #[serde(rename = "dockerComposeFile")]
    pub docker_compose_file: Option<serde_json::Value>,

    /// Name of the Docker Compose service to connect to as the primary development container.
    ///
    /// Reference: [Container Configuration - service](https://containers.dev/implementors/json_reference/#service)
    pub service: Option<String>,

    /// Array of additional Docker Compose services to start alongside the primary service.
    ///
    /// Reference: [Container Configuration - runServices](https://containers.dev/implementors/json_reference/#run-services)
    #[serde(default)]
    pub run_services: Vec<String>,

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

impl DevContainerConfig {
    /// Apply variable substitution to configuration fields
    ///
    /// This method applies variable substitution to the following fields:
    /// - `workspace_folder`
    /// - `mounts` (string forms)
    /// - Lifecycle commands (string or array entries)
    /// - `run_args`
    /// - `container_env` values
    ///
    /// ## Arguments
    ///
    /// * `context` - Substitution context with variable values
    ///
    /// ## Returns
    ///
    /// Returns a tuple of (substituted_config, substitution_report).
    ///
    /// ## Example
    ///
    /// ```rust
    /// use deacon_core::config::DevContainerConfig;
    /// use deacon_core::variable::SubstitutionContext;
    /// use std::path::Path;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let mut config = DevContainerConfig::default();
    /// config.workspace_folder = Some("${localWorkspaceFolder}/src".to_string());
    ///
    /// let context = SubstitutionContext::new(Path::new("/workspace"))?;
    /// let (substituted_config, report) = config.apply_variable_substitution(&context);
    ///
    /// println!("Substituted workspace folder: {:?}", substituted_config.workspace_folder);
    /// println!("Substitutions made: {}", report.replacements.len());
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip_all)]
    pub fn apply_variable_substitution(
        &self,
        context: &SubstitutionContext,
    ) -> (Self, SubstitutionReport) {
        let mut report = SubstitutionReport::new();
        let mut config = self.clone();

        debug!("Applying variable substitution to DevContainer configuration");

        // Substitute workspace_folder
        if let Some(ref workspace_folder) = config.workspace_folder {
            config.workspace_folder = Some(VariableSubstitution::substitute_string(
                workspace_folder,
                context,
                &mut report,
            ));
        }

        // Substitute mounts (JSON values that may contain strings)
        config.mounts = config
            .mounts
            .iter()
            .map(|mount| VariableSubstitution::substitute_json_value(mount, context, &mut report))
            .collect();

        // Substitute run_args
        config.run_args = config
            .run_args
            .iter()
            .map(|arg| VariableSubstitution::substitute_string(arg, context, &mut report))
            .collect();

        // Substitute container_env values
        config.container_env = config
            .container_env
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    VariableSubstitution::substitute_string(value, context, &mut report),
                )
            })
            .collect();

        // Substitute lifecycle commands
        if let Some(ref cmd) = config.on_create_command {
            config.on_create_command = Some(VariableSubstitution::substitute_json_value(
                cmd,
                context,
                &mut report,
            ));
        }

        if let Some(ref cmd) = config.post_create_command {
            config.post_create_command = Some(VariableSubstitution::substitute_json_value(
                cmd,
                context,
                &mut report,
            ));
        }

        if let Some(ref cmd) = config.post_start_command {
            config.post_start_command = Some(VariableSubstitution::substitute_json_value(
                cmd,
                context,
                &mut report,
            ));
        }

        if let Some(ref cmd) = config.post_attach_command {
            config.post_attach_command = Some(VariableSubstitution::substitute_json_value(
                cmd,
                context,
                &mut report,
            ));
        }

        if let Some(ref cmd) = config.initialize_command {
            config.initialize_command = Some(VariableSubstitution::substitute_json_value(
                cmd,
                context,
                &mut report,
            ));
        }

        if let Some(ref cmd) = config.update_content_command {
            config.update_content_command = Some(VariableSubstitution::substitute_json_value(
                cmd,
                context,
                &mut report,
            ));
        }

        debug!(
            "Variable substitution complete - {} replacements, {} unknown variables",
            report.replacements.len(),
            report.unknown_variables.len()
        );

        (config, report)
    }

    /// Get Docker Compose files as a vector of strings
    ///
    /// Parses the `docker_compose_file` field which can be either a string or an array of strings.
    ///
    /// ## Returns
    ///
    /// Returns a vector of compose file paths. Empty vector if no compose files are specified.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use deacon_core::config::DevContainerConfig;
    /// use serde_json::json;
    ///
    /// let mut config = DevContainerConfig::default();
    /// config.docker_compose_file = Some(json!("docker-compose.yml"));
    /// assert_eq!(config.get_compose_files(), vec!["docker-compose.yml"]);
    ///
    /// config.docker_compose_file = Some(json!(["docker-compose.yml", "docker-compose.override.yml"]));
    /// assert_eq!(config.get_compose_files(), vec!["docker-compose.yml", "docker-compose.override.yml"]);
    /// ```
    pub fn get_compose_files(&self) -> Vec<String> {
        match &self.docker_compose_file {
            Some(serde_json::Value::String(file)) => vec![file.clone()],
            Some(serde_json::Value::Array(files)) => files
                .iter()
                .filter_map(|f| f.as_str())
                .map(|s| s.to_string())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Check if this configuration uses Docker Compose
    ///
    /// ## Returns
    ///
    /// Returns true if `docker_compose_file` is specified and `service` is specified.
    pub fn uses_compose(&self) -> bool {
        self.docker_compose_file.is_some() && self.service.is_some()
    }

    /// Get all services to start (primary service + run services)
    ///
    /// ## Returns
    ///
    /// Returns a vector containing the primary service (if specified) followed by any run services.
    pub fn get_all_services(&self) -> Vec<String> {
        let mut services = Vec::new();
        if let Some(ref service) = self.service {
            services.push(service.clone());
        }
        services.extend(self.run_services.clone());
        services
    }

    /// Check if the configuration specifies stopCompose shutdown action
    ///
    /// ## Returns
    ///
    /// Returns true if shutdown_action is set to "stopCompose".
    pub fn has_stop_compose_shutdown(&self) -> bool {
        self.shutdown_action
            .as_ref()
            .map(|action| action == "stopCompose")
            .unwrap_or(false)
    }
}

impl Default for DevContainerConfig {
    fn default() -> Self {
        Self {
            name: None,
            image: None,
            dockerfile: None,
            build: None,
            docker_compose_file: None,
            service: None,
            run_services: Vec::new(),
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
    /// Discover DevContainer configuration file in workspace
    ///
    /// This method implements the configuration discovery rules:
    /// 1. Search for `.devcontainer/devcontainer.json` first
    /// 2. Then search for `.devcontainer.json` in workspace root
    /// 3. Return the first file found (may not exist)
    ///
    /// ## Arguments
    ///
    /// * `workspace` - Path to the workspace folder
    ///
    /// ## Returns
    ///
    /// Returns `Ok(ConfigLocation)` with the discovered configuration path.
    /// The returned location may indicate a non-existent file if no configuration
    /// is found, allowing callers to decide how to handle missing configurations.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use deacon_core::config::ConfigLoader;
    /// use std::path::Path;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let location = ConfigLoader::discover_config(Path::new("/workspace"))?;
    /// if location.exists() {
    ///     println!("Found config at: {}", location.path().display());
    /// } else {
    ///     println!("No config found, would use: {}", location.path().display());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip_all, fields(workspace = %workspace.display()))]
    pub fn discover_config(workspace: &Path) -> Result<ConfigLocation> {
        debug!(
            "Discovering DevContainer configuration in workspace: {}",
            workspace.display()
        );

        // Check if workspace exists
        if !workspace.exists() {
            return Err(DeaconError::Config(ConfigError::NotFound {
                path: workspace.display().to_string(),
            }));
        }

        // Search order: .devcontainer/devcontainer.json then .devcontainer.json
        let search_paths = [
            workspace.join(".devcontainer").join("devcontainer.json"),
            workspace.join(".devcontainer.json"),
        ];

        for path in &search_paths {
            debug!("Checking for configuration file: {}", path.display());
            if path.exists() {
                debug!("Found configuration file: {}", path.display());
                return Ok(ConfigLocation::new(path.clone()));
            }
        }

        // Return the first preference even if it doesn't exist
        let default_path = search_paths[0].clone();
        debug!(
            "No configuration file found, defaulting to: {}",
            default_path.display()
        );
        Ok(ConfigLocation::new(default_path))
    }
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
            return Err(DeaconError::Config(ConfigError::NotFound {
                path: path.display().to_string(),
            }));
        }

        // Read file content
        let content = std::fs::read_to_string(path).map_err(|e| {
            debug!("Failed to read configuration file: {}", e);
            DeaconError::Config(ConfigError::Io(e))
        })?;

        // Parse JSON5 (JSON with comments and trailing commas)
        let raw_value: serde_json::Value = json5::from_str(&content).map_err(|e| {
            debug!("Failed to parse configuration file: {}", e);
            DeaconError::Config(ConfigError::Parsing {
                message: format!("JSON parsing error: {}", e),
            })
        })?;

        // Log unknown top-level keys at DEBUG level
        if let serde_json::Value::Object(obj) = &raw_value {
            Self::log_unknown_keys(obj);
        }

        // Check for extends field (not yet implemented)
        if let serde_json::Value::Object(obj) = &raw_value {
            if obj.contains_key("extends") {
                return Err(DeaconError::Config(ConfigError::NotImplemented {
                    feature: "extends configuration".to_string(),
                }));
            }
        }

        // Deserialize into strongly typed structure
        let config: DevContainerConfig = serde_json::from_value(raw_value).map_err(|e| {
            debug!("Failed to deserialize configuration: {}", e);
            DeaconError::Config(ConfigError::Validation {
                message: format!("Deserialization error: {}", e),
            })
        })?;

        // Basic validation
        Self::validate_config(&config)?;

        debug!(
            "Successfully loaded configuration with name: {:?}",
            config.name
        );
        Ok(config)
    }

    /// Load configuration with variable substitution applied
    ///
    /// This is a convenience method that combines configuration loading and
    /// variable substitution in a single call.
    ///
    /// ## Arguments
    ///
    /// * `path` - Path to the devcontainer.json or devcontainer.jsonc file
    /// * `workspace` - Workspace path for variable substitution context
    ///
    /// ## Returns
    ///
    /// Returns `Ok((DevContainerConfig, SubstitutionReport))` on success with
    /// variable substitution applied.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use deacon_core::config::ConfigLoader;
    /// use std::path::Path;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let (config, report) = ConfigLoader::load_with_substitution(
    ///     Path::new(".devcontainer/devcontainer.json"),
    ///     Path::new("/workspace")
    /// )?;
    ///
    /// println!("Loaded config with {} substitutions", report.replacements.len());
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip_all, fields(path = %path.display(), workspace = %workspace.display()))]
    pub fn load_with_substitution(
        path: &Path,
        workspace: &Path,
    ) -> Result<(DevContainerConfig, SubstitutionReport)> {
        debug!(
            "Loading configuration with substitution from {}",
            path.display()
        );

        // Load base configuration
        let config = Self::load_from_path(path)?;

        // Create substitution context
        let context = SubstitutionContext::new(workspace)?;

        // Apply variable substitution
        let (substituted_config, report) = config.apply_variable_substitution(&context);

        debug!(
            "Configuration loaded and substituted - {} replacements",
            report.replacements.len()
        );

        Ok((substituted_config, report))
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
                return Err(DeaconError::Config(ConfigError::Validation {
                    message: "Cannot specify both 'image' and 'dockerFile' - choose one"
                        .to_string(),
                }));
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
                "none" | "stopContainer" | "stopCompose" => {
                    // Valid values
                }
                _ => {
                    return Err(DeaconError::Config(ConfigError::Validation {
                        message: format!(
                            "Invalid shutdownAction '{}' - must be 'none', 'stopContainer', or 'stopCompose'",
                            action
                        ),
                    }));
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
    use tempfile::{NamedTempFile, TempDir};

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
            DeaconError::Config(ConfigError::NotFound { path }) => {
                assert!(path.contains("nonexistent.json"));
            }
            _ => panic!("Expected Config(NotFound) error"),
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
            DeaconError::Config(ConfigError::Parsing { message }) => {
                assert!(message.contains("JSON parsing error"));
            }
            _ => panic!("Expected Config(Parsing) error"),
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
            DeaconError::Config(ConfigError::Validation { message }) => {
                assert!(message.contains("Cannot specify both 'image' and 'dockerFile'"));
            }
            _ => panic!("Expected Config(Validation) error"),
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
            DeaconError::Config(ConfigError::Validation { message }) => {
                assert!(message.contains("Invalid shutdownAction"));
            }
            _ => panic!("Expected Config(Validation) error"),
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
            DeaconError::Config(ConfigError::NotImplemented { feature }) => {
                assert!(feature.contains("extends"));
            }
            _ => panic!("Expected Config(NotImplemented) error"),
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

    #[test]
    fn test_discover_config_devcontainer_dir() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let devcontainer_dir = workspace.join(".devcontainer");
        std::fs::create_dir_all(&devcontainer_dir)?;

        let config_path = devcontainer_dir.join("devcontainer.json");
        std::fs::write(&config_path, r#"{"name": "Test"}"#)?;

        let location = ConfigLoader::discover_config(workspace)?;
        assert!(location.exists());
        assert_eq!(location.path(), &config_path);

        Ok(())
    }

    #[test]
    fn test_discover_config_root_file() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let config_path = workspace.join(".devcontainer.json");
        std::fs::write(&config_path, r#"{"name": "Test"}"#)?;

        let location = ConfigLoader::discover_config(workspace)?;
        assert!(location.exists());
        assert_eq!(location.path(), &config_path);

        Ok(())
    }

    #[test]
    fn test_discover_config_preference_order() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        let devcontainer_dir = workspace.join(".devcontainer");
        std::fs::create_dir_all(&devcontainer_dir)?;

        // Create both files
        let dir_config_path = devcontainer_dir.join("devcontainer.json");
        let root_config_path = workspace.join(".devcontainer.json");
        std::fs::write(&dir_config_path, r#"{"name": "Dir Config"}"#)?;
        std::fs::write(&root_config_path, r#"{"name": "Root Config"}"#)?;

        let location = ConfigLoader::discover_config(workspace)?;
        assert!(location.exists());
        // Should prefer .devcontainer/devcontainer.json
        assert_eq!(location.path(), &dir_config_path);

        Ok(())
    }

    #[test]
    fn test_discover_config_no_file_exists() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();

        let location = ConfigLoader::discover_config(workspace)?;
        assert!(!location.exists());
        // Should return preferred path even if it doesn't exist
        assert_eq!(
            location.path(),
            &workspace.join(".devcontainer").join("devcontainer.json")
        );

        Ok(())
    }

    #[test]
    fn test_discover_config_workspace_not_exists() {
        let result = ConfigLoader::discover_config(Path::new("/nonexistent/workspace"));
        assert!(result.is_err());
        match result.unwrap_err() {
            DeaconError::Config(ConfigError::NotFound { path }) => {
                assert!(path.contains("nonexistent"));
            }
            _ => panic!("Expected Config(NotFound) error"),
        }
    }

    #[test]
    fn test_load_with_substitution() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();
        // Use canonical path for comparisons to avoid platform-specific symlink prefixes
        // (e.g., macOS may canonicalize /var/... to /private/var/...).
        let workspace_canonical = workspace.canonicalize()?;
        let workspace_canonical_str = workspace_canonical.to_str().unwrap();

        let config_content = r#"{
            "name": "Test Container",
            "workspaceFolder": "${localWorkspaceFolder}/src",
            "mounts": [
                "source=${localWorkspaceFolder}/.cargo,target=/cargo,type=bind"
            ],
            "containerEnv": {
                "WORKSPACE_ROOT": "${localWorkspaceFolder}",
                "CONTAINER_ID": "${devcontainerId}"
            },
            "runArgs": ["--name", "${devcontainerId}"],
            "postCreateCommand": "echo 'Workspace: ${localWorkspaceFolder}'"
        }"#;

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(config_content.as_bytes())?;

        let (config, report) = ConfigLoader::load_with_substitution(temp_file.path(), workspace)?;

        // Check that substitution was applied
        assert!(report.has_substitutions());
        assert!(report.replacements.len() >= 2); // At least localWorkspaceFolder and devcontainerId

        // Check specific substitutions
        if let Some(workspace_folder) = &config.workspace_folder {
            assert!(workspace_folder.starts_with(workspace_canonical_str));
            assert!(workspace_folder.ends_with("/src"));
        }

        // Check container env substitution
        assert!(config
            .container_env
            .get("WORKSPACE_ROOT")
            .unwrap()
            .starts_with(workspace_canonical_str));

        // Check mounts substitution
        if !config.mounts.is_empty() {
            if let serde_json::Value::String(mount_str) = &config.mounts[0] {
                assert!(mount_str.contains(workspace_canonical_str));
            }
        }

        Ok(())
    }
}
