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
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, instrument, warn};

/// Default function to return an empty JSON object for serde defaults.
fn default_empty_object() -> serde_json::Value {
    serde_json::Value::Object(Default::default())
}

/// Port specification that can be either a number or a string.
///
/// Supports port numbers (e.g., 3000) and port mappings (e.g., "3000:3000").
/// For now, stores the original value for future parsing.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PortSpec {
    /// Port number
    Number(u16),
    /// Port string (for mappings like "3000:3000" or just "3000")
    String(String),
}

impl PortSpec {
    /// Get the primary port number from this specification.
    /// For strings like "3000:8080", returns the first port (3000).
    /// For numbers, returns the number directly.
    pub fn primary_port(&self) -> Option<u16> {
        match self {
            PortSpec::Number(port) => Some(*port),
            PortSpec::String(s) => {
                // Try to parse as number first
                if let Ok(port) = s.parse::<u16>() {
                    return Some(port);
                }
                // Try to parse as port mapping (e.g., "3000:8080")
                if let Some(first_part) = s.split(':').next() {
                    first_part.parse::<u16>().ok()
                } else {
                    None
                }
            }
        }
    }

    /// Get the string representation of this port spec for validation.
    pub fn as_string(&self) -> String {
        match self {
            PortSpec::Number(port) => port.to_string(),
            PortSpec::String(s) => s.clone(),
        }
    }
}

/// Action to take when a port is auto-forwarded.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OnAutoForward {
    /// Do nothing when port is auto-forwarded
    Silent,
    /// Show a notification when port is auto-forwarded  
    Notify,
    /// Open the port in a browser when auto-forwarded
    OpenBrowser,
    /// Open the port in a preview panel when auto-forwarded
    OpenPreview,
    /// Ignore the port (don't auto-forward)
    Ignore,
}

/// Attributes for port configuration.
///
/// Defines how ports should be handled when forwarded, including
/// labeling, auto-forward behavior, and preview options.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortAttributes {
    /// Human-readable label for the port
    pub label: Option<String>,

    /// Action to take when the port is auto-forwarded
    pub on_auto_forward: Option<OnAutoForward>,

    /// Whether to open a preview of the port automatically
    pub open_preview: Option<bool>,

    /// Whether to require a specific local port for forwarding
    pub require_local_port: Option<bool>,

    /// Description of what this port is used for
    pub description: Option<String>,
}

/// Custom deserializer for extends field that handles both string and array of strings
fn deserialize_extends<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    let value: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(vec![s])),
        Some(serde_json::Value::Array(arr)) => {
            let mut result = Vec::new();
            for item in arr {
                match item {
                    serde_json::Value::String(s) => result.push(s),
                    _ => return Err(D::Error::custom("extends array must contain only strings")),
                }
            }
            Ok(Some(result))
        }
        Some(_) => Err(D::Error::custom(
            "extends must be a string or array of strings",
        )),
    }
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

/// System resource specification with support for units.
///
/// Supports numeric values with optional unit suffixes:
/// - CPU: number of cores (e.g., "2", "4")  
/// - Memory: bytes with units (e.g., "4GB", "512MB", "1024")
/// - Storage: bytes with units (e.g., "10GB", "500MB")
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ResourceSpec {
    /// Numeric value (interpreted as base unit)
    Number(f64),
    /// String with optional unit suffix
    String(String),
}

impl ResourceSpec {
    /// Parse the resource specification to a numeric value in base units.
    ///
    /// For memory and storage, returns bytes.
    /// For CPU, returns number of cores.
    pub fn parse_bytes(&self) -> Result<u64> {
        match self {
            ResourceSpec::Number(n) => Ok(*n as u64),
            ResourceSpec::String(s) => parse_resource_string(s),
        }
    }

    /// Parse CPU cores from the resource specification.
    pub fn parse_cpu_cores(&self) -> Result<f64> {
        match self {
            ResourceSpec::Number(n) => Ok(*n),
            ResourceSpec::String(s) => s.parse::<f64>().map_err(|e| {
                ConfigError::Validation {
                    message: format!("Invalid CPU specification '{}': {}", s, e),
                }
                .into()
            }),
        }
    }
}

/// Host system requirements for the development environment.
///
/// Specifies minimum system resources required to run the development container.
/// All fields are optional - only specified requirements will be validated.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct HostRequirements {
    /// Minimum CPU cores required (e.g., "2", "4.0")
    pub cpus: Option<ResourceSpec>,
    /// Minimum memory required (e.g., "4GB", "512MB")
    pub memory: Option<ResourceSpec>,
    /// Minimum storage space required (e.g., "10GB", "500MB")
    pub storage: Option<ResourceSpec>,
}

/// Parse a resource string with unit suffix to bytes.
///
/// Supports units: B, KB, MB, GB, TB (1000-based) and KiB, MiB, GiB, TiB (1024-based)
fn parse_resource_string(s: &str) -> Result<u64> {
    let s = s.trim();

    // Try to parse as plain number first
    if let Ok(n) = s.parse::<f64>() {
        return Ok(n as u64);
    }

    // Extract number and unit
    let re = regex::Regex::new(r"^(\d+(?:\.\d+)?)\s*([a-zA-Z]+)$").map_err(|e| {
        ConfigError::Validation {
            message: format!("Invalid regex pattern: {}", e),
        }
    })?;
    let captures = re.captures(s).ok_or_else(|| ConfigError::Validation {
        message: format!("Invalid resource format: {}", s),
    })?;

    let number: f64 = captures[1].parse().map_err(|e| ConfigError::Validation {
        message: format!("Invalid number in resource specification '{}': {}", s, e),
    })?;
    let unit = captures[2].to_lowercase();

    let multiplier = match unit.as_str() {
        "b" => 1,
        "kb" => 1_000,
        "mb" => 1_000_000,
        "gb" => 1_000_000_000,
        "tb" => 1_000_000_000_000,
        "kib" => 1_024,
        "mib" => 1_024 * 1_024,
        "gib" => 1_024 * 1_024 * 1_024,
        "tib" => 1_024_u64.pow(4),
        _ => {
            return Err(ConfigError::Validation {
                message: format!("Unknown unit: {}", unit),
            }
            .into())
        }
    };

    Ok((number * multiplier as f64) as u64)
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
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevContainerConfig {
    /// Paths to extend from. Can be a single path or array of paths.
    ///
    /// Reference: [Configuration - extends](https://containers.dev/implementors/json_reference/#extends)
    #[serde(default, deserialize_with = "deserialize_extends")]
    pub extends: Option<Vec<String>>,

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

    /// Override the default feature installation order.
    ///
    /// Specifies the order in which features should be installed, overriding the
    /// default topological sort order while still respecting dependencies.
    ///
    /// Reference: [Feature Configuration - overrideFeatureInstallOrder](https://containers.dev/implementors/json_reference/#override-feature-install-order)
    #[serde(default)]
    pub override_feature_install_order: Option<Vec<String>>,

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

    /// Mount configuration for the workspace folder.
    ///
    /// Reference: [Container Configuration - workspaceMount](https://containers.dev/implementors/json_reference/#workspace-mount)
    #[serde(rename = "workspaceMount")]
    pub workspace_mount: Option<String>,

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

    /// User to run commands as inside the container.
    ///
    /// Reference: [User Configuration - containerUser](https://containers.dev/implementors/json_reference/#container-user)
    pub container_user: Option<String>,

    /// User to run commands as in the remote environment.
    ///
    /// Reference: [User Configuration - remoteUser](https://containers.dev/implementors/json_reference/#remote-user)
    pub remote_user: Option<String>,

    /// Whether to update the remote user's UID/GID to match the host user.
    ///
    /// Reference: [User Configuration - updateRemoteUserUID](https://containers.dev/implementors/json_reference/#update-remote-user-uid)
    #[serde(rename = "updateRemoteUserUID")]
    pub update_remote_user_uid: Option<bool>,

    /// Ports to forward from the container.
    ///
    /// Reference: [Port Configuration - forwardPorts](https://containers.dev/implementors/json_reference/#forward-ports)
    #[serde(default)]
    pub forward_ports: Vec<PortSpec>,

    /// Primary application port.
    ///
    /// Reference: [Port Configuration - appPort](https://containers.dev/implementors/json_reference/#app-port)
    pub app_port: Option<PortSpec>,

    /// Attributes for specific ports.
    ///
    /// Maps port specifications to their attributes. Keys are port numbers or
    /// port/protocol combinations (e.g., "3000", "3000/tcp").
    #[serde(default)]
    pub ports_attributes: HashMap<String, PortAttributes>,

    /// Default attributes for ports not explicitly configured.
    ///
    /// These attributes are applied to any forwarded ports that don't have
    /// specific entries in ports_attributes.
    pub other_ports_attributes: Option<PortAttributes>,

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

    /// Host requirements for the development environment.
    ///
    /// Specifies minimum system requirements (CPU, memory, storage) that the host
    /// must meet to successfully run the development container.
    #[serde(rename = "hostRequirements")]
    pub host_requirements: Option<HostRequirements>,

    /// Whether to run the container in privileged mode.
    ///
    /// Reference: [Container Configuration - privileged](https://containers.dev/implementors/json_reference/#privileged)
    #[serde(default)]
    pub privileged: Option<bool>,

    /// Linux capabilities to add to the container.
    ///
    /// Reference: [Container Configuration - capAdd](https://containers.dev/implementors/json_reference/#cap-add)
    #[serde(default, rename = "capAdd")]
    pub cap_add: Vec<String>,

    /// Security options for the container.
    ///
    /// Reference: [Container Configuration - securityOpt](https://containers.dev/implementors/json_reference/#security-opt)
    #[serde(default, rename = "securityOpt")]
    pub security_opt: Vec<String>,
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

        // Substitute workspace_mount
        if let Some(ref workspace_mount) = config.workspace_mount {
            config.workspace_mount = Some(VariableSubstitution::substitute_string(
                workspace_mount,
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

    /// Apply variable substitution to the configuration with advanced options
    ///
    /// This method applies variable substitution to all string fields in the configuration
    /// using advanced substitution features including multi-pass resolution, cycle detection,
    /// and strict mode.
    ///
    /// ## Arguments
    ///
    /// * `context` - Substitution context containing variable values
    /// * `options` - Advanced substitution options
    /// * `report` - Mutable report to track substitutions
    ///
    /// ## Returns
    ///
    /// Returns the substituted configuration.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use deacon_core::config::DevContainerConfig;
    /// use deacon_core::variable::{SubstitutionContext, SubstitutionOptions, SubstitutionReport};
    /// use std::path::Path;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let mut config = DevContainerConfig::default();
    /// config.workspace_folder = Some("${localWorkspaceFolder}/src".to_string());
    ///
    /// let context = SubstitutionContext::new(Path::new("/workspace"))?;
    /// let options = SubstitutionOptions::default();
    /// let mut report = SubstitutionReport::new();
    ///
    /// let substituted_config = config.apply_variable_substitution_advanced(&context, &options, &mut report)?;
    ///
    /// println!("Substituted workspace folder: {:?}", substituted_config.workspace_folder);
    /// println!("Substitutions made: {}", report.replacements.len());
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip_all)]
    pub fn apply_variable_substitution_advanced(
        &self,
        context: &SubstitutionContext,
        options: &crate::variable::SubstitutionOptions,
        report: &mut SubstitutionReport,
    ) -> crate::errors::Result<Self> {
        let mut config = self.clone();

        debug!("Applying advanced variable substitution to DevContainer configuration");

        // Substitute workspace_folder
        if let Some(ref workspace_folder) = config.workspace_folder {
            config.workspace_folder = Some(VariableSubstitution::substitute_string_advanced(
                workspace_folder,
                context,
                options,
                report,
            )?);
        }

        // Substitute workspace_mount
        if let Some(ref workspace_mount) = config.workspace_mount {
            config.workspace_mount = Some(VariableSubstitution::substitute_string_advanced(
                workspace_mount,
                context,
                options,
                report,
            )?);
        }

        // Substitute mounts (JSON values that may contain strings)
        let mut substituted_mounts = Vec::new();
        for mount in &config.mounts {
            substituted_mounts.push(VariableSubstitution::substitute_json_value_with_options(
                mount, context, options, report,
            )?);
        }
        config.mounts = substituted_mounts;

        // Substitute run_args
        let mut substituted_run_args = Vec::new();
        for arg in &config.run_args {
            substituted_run_args.push(VariableSubstitution::substitute_string_advanced(
                arg, context, options, report,
            )?);
        }
        config.run_args = substituted_run_args;

        // Substitute container environment variables
        let mut substituted_container_env = HashMap::new();
        for (key, value) in &config.container_env {
            let substituted_value =
                VariableSubstitution::substitute_string_advanced(value, context, options, report)?;
            substituted_container_env.insert(key.clone(), substituted_value);
        }
        config.container_env = substituted_container_env;

        // Substitute remote environment variables
        let mut substituted_remote_env = HashMap::new();
        for (key, value) in &config.remote_env {
            if let Some(val) = value {
                let substituted_value = VariableSubstitution::substitute_string_advanced(
                    val, context, options, report,
                )?;
                substituted_remote_env.insert(key.clone(), Some(substituted_value));
            } else {
                substituted_remote_env.insert(key.clone(), None);
            }
        }
        config.remote_env = substituted_remote_env;

        // Substitute lifecycle commands
        if let Some(ref on_create_command) = config.on_create_command {
            config.on_create_command =
                Some(VariableSubstitution::substitute_json_value_with_options(
                    on_create_command,
                    context,
                    options,
                    report,
                )?);
        }

        if let Some(ref update_content_command) = config.update_content_command {
            config.update_content_command =
                Some(VariableSubstitution::substitute_json_value_with_options(
                    update_content_command,
                    context,
                    options,
                    report,
                )?);
        }

        if let Some(ref post_create_command) = config.post_create_command {
            config.post_create_command =
                Some(VariableSubstitution::substitute_json_value_with_options(
                    post_create_command,
                    context,
                    options,
                    report,
                )?);
        }

        if let Some(ref post_start_command) = config.post_start_command {
            config.post_start_command =
                Some(VariableSubstitution::substitute_json_value_with_options(
                    post_start_command,
                    context,
                    options,
                    report,
                )?);
        }

        if let Some(ref post_attach_command) = config.post_attach_command {
            config.post_attach_command =
                Some(VariableSubstitution::substitute_json_value_with_options(
                    post_attach_command,
                    context,
                    options,
                    report,
                )?);
        }

        debug!(
            "Advanced variable substitution completed: {} substitutions, {} unknown variables",
            report.replacements.len(),
            report.unknown_variables.len()
        );

        Ok(config)
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
            extends: None,
            name: None,
            image: None,
            dockerfile: None,
            build: None,
            docker_compose_file: None,
            service: None,
            run_services: Vec::new(),
            features: default_empty_object(),
            override_feature_install_order: None,
            customizations: default_empty_object(),
            workspace_folder: None,
            workspace_mount: None,
            mounts: Vec::new(),
            container_env: HashMap::new(),
            remote_env: HashMap::new(),
            container_user: None,
            remote_user: None,
            update_remote_user_uid: None,
            forward_ports: Vec::new(),
            app_port: None,
            ports_attributes: HashMap::new(),
            other_ports_attributes: None,
            run_args: Vec::new(),
            shutdown_action: None,
            override_command: None,
            on_create_command: None,
            post_start_command: None,
            post_create_command: None,
            post_attach_command: None,
            initialize_command: None,
            update_content_command: None,
            host_requirements: None,
            privileged: None,
            cap_add: Vec::new(),
            security_opt: Vec::new(),
        }
    }
}

/// Configuration merger that implements the DevContainer specification merge rules.
///
/// The merger follows these rules:
/// - Arrays override (no concatenation) for lifecycle commands
/// - Maps deep-merge with later precedence
/// - Features map merge keyed by id
/// - containerEnv & remoteEnv last-writer-wins
/// - runArgs concatenate
pub struct ConfigMerger;

impl ConfigMerger {
    /// Merge multiple configurations in order, with later configs taking precedence.
    ///
    /// ## Arguments
    ///
    /// * `configs` - Configurations to merge in order (first = lowest precedence)
    ///
    /// ## Returns
    ///
    /// Returns the merged configuration following DevContainer merge rules.
    #[instrument(skip_all)]
    pub fn merge_configs(configs: &[DevContainerConfig]) -> DevContainerConfig {
        if configs.is_empty() {
            return DevContainerConfig::default();
        }

        if configs.len() == 1 {
            return configs[0].clone();
        }

        debug!("Merging {} configurations", configs.len());

        let mut result = DevContainerConfig::default();

        for (i, config) in configs.iter().enumerate() {
            debug!("Merging configuration {} of {}", i + 1, configs.len());
            result = Self::merge_two_configs(&result, config);
        }

        debug!("Configuration merge complete");
        result
    }

    /// Merge two configurations with the second taking precedence.
    fn merge_two_configs(
        base: &DevContainerConfig,
        overlay: &DevContainerConfig,
    ) -> DevContainerConfig {
        DevContainerConfig {
            // extends is not merged - it's resolved before merging
            extends: overlay.extends.clone().or_else(|| base.extends.clone()),

            // Simple field overrides (last writer wins)
            name: overlay.name.clone().or_else(|| base.name.clone()),
            image: overlay.image.clone().or_else(|| base.image.clone()),
            dockerfile: overlay
                .dockerfile
                .clone()
                .or_else(|| base.dockerfile.clone()),
            build: overlay.build.clone().or_else(|| base.build.clone()),
            workspace_folder: overlay
                .workspace_folder
                .clone()
                .or_else(|| base.workspace_folder.clone()),
            workspace_mount: overlay
                .workspace_mount
                .clone()
                .or_else(|| base.workspace_mount.clone()),
            app_port: overlay.app_port.clone().or_else(|| base.app_port.clone()),
            shutdown_action: overlay
                .shutdown_action
                .clone()
                .or_else(|| base.shutdown_action.clone()),
            override_command: overlay.override_command.or(base.override_command),
            // Docker Compose fields
            docker_compose_file: overlay
                .docker_compose_file
                .clone()
                .or_else(|| base.docker_compose_file.clone()),
            service: overlay.service.clone().or_else(|| base.service.clone()),
            run_services: if overlay.run_services.is_empty() {
                base.run_services.clone()
            } else {
                overlay.run_services.clone()
            },
            // Features: deep merge as objects
            features: Self::merge_json_objects(&base.features, &overlay.features),

            // Override feature install order: last writer wins
            override_feature_install_order: overlay
                .override_feature_install_order
                .clone()
                .or_else(|| base.override_feature_install_order.clone()),

            // Customizations: deep merge as objects
            customizations: Self::merge_json_objects(&base.customizations, &overlay.customizations),

            // Arrays that override (no concat) - lifecycle commands
            mounts: if overlay.mounts.is_empty() {
                base.mounts.clone()
            } else {
                overlay.mounts.clone()
            },
            forward_ports: if overlay.forward_ports.is_empty() {
                base.forward_ports.clone()
            } else {
                overlay.forward_ports.clone()
            },
            on_create_command: overlay
                .on_create_command
                .clone()
                .or_else(|| base.on_create_command.clone()),
            post_start_command: overlay
                .post_start_command
                .clone()
                .or_else(|| base.post_start_command.clone()),
            post_create_command: overlay
                .post_create_command
                .clone()
                .or_else(|| base.post_create_command.clone()),
            post_attach_command: overlay
                .post_attach_command
                .clone()
                .or_else(|| base.post_attach_command.clone()),
            initialize_command: overlay
                .initialize_command
                .clone()
                .or_else(|| base.initialize_command.clone()),
            update_content_command: overlay
                .update_content_command
                .clone()
                .or_else(|| base.update_content_command.clone()),

            // Maps: last writer wins for env vars
            container_env: Self::merge_string_maps(&base.container_env, &overlay.container_env),
            remote_env: Self::merge_optional_string_maps(&base.remote_env, &overlay.remote_env),

            // User configuration: last writer wins
            container_user: overlay
                .container_user
                .clone()
                .or_else(|| base.container_user.clone()),
            remote_user: overlay
                .remote_user
                .clone()
                .or_else(|| base.remote_user.clone()),
            update_remote_user_uid: overlay
                .update_remote_user_uid
                .or(base.update_remote_user_uid),

            // runArgs: concatenate arrays
            run_args: Self::concat_string_arrays(&base.run_args, &overlay.run_args),

            // Port attributes: deep merge maps
            ports_attributes: Self::merge_port_attributes_maps(
                &base.ports_attributes,
                &overlay.ports_attributes,
            ),
            other_ports_attributes: overlay
                .other_ports_attributes
                .clone()
                .or_else(|| base.other_ports_attributes.clone()),

            // Host requirements: last writer wins
            host_requirements: overlay
                .host_requirements
                .clone()
                .or_else(|| base.host_requirements.clone()),

            // Security options: last writer wins for privileged, concatenate arrays for capabilities and security opts
            privileged: overlay.privileged.or(base.privileged),
            cap_add: Self::concat_string_arrays(&base.cap_add, &overlay.cap_add),
            security_opt: Self::concat_string_arrays(&base.security_opt, &overlay.security_opt),
        }
    }

    /// Merge two JSON objects deeply
    fn merge_json_objects(
        base: &serde_json::Value,
        overlay: &serde_json::Value,
    ) -> serde_json::Value {
        match (base, overlay) {
            (serde_json::Value::Object(base_obj), serde_json::Value::Object(overlay_obj)) => {
                let mut result = base_obj.clone();
                for (key, value) in overlay_obj {
                    match result.get(key) {
                        Some(existing) => {
                            result.insert(key.clone(), Self::merge_json_objects(existing, value));
                        }
                        None => {
                            result.insert(key.clone(), value.clone());
                        }
                    }
                }
                serde_json::Value::Object(result)
            }
            (_, overlay) => overlay.clone(),
        }
    }

    /// Merge string maps with overlay taking precedence
    fn merge_string_maps(
        base: &HashMap<String, String>,
        overlay: &HashMap<String, String>,
    ) -> HashMap<String, String> {
        let mut result = base.clone();
        result.extend(overlay.clone());
        result
    }

    /// Merge optional string maps with overlay taking precedence
    fn merge_optional_string_maps(
        base: &HashMap<String, Option<String>>,
        overlay: &HashMap<String, Option<String>>,
    ) -> HashMap<String, Option<String>> {
        let mut result = base.clone();
        result.extend(overlay.clone());
        result
    }

    /// Concatenate string arrays
    fn concat_string_arrays(base: &[String], overlay: &[String]) -> Vec<String> {
        let mut result = base.to_vec();
        result.extend_from_slice(overlay);
        result
    }

    /// Merge port attributes maps with overlay taking precedence
    fn merge_port_attributes_maps(
        base: &HashMap<String, PortAttributes>,
        overlay: &HashMap<String, PortAttributes>,
    ) -> HashMap<String, PortAttributes> {
        let mut result = base.clone();
        result.extend(overlay.clone());
        result
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

    /// Load configuration with extends resolution applied
    ///
    /// This method loads a configuration and resolves its extends chain,
    /// returning the fully merged configuration.
    ///
    /// ## Arguments
    ///
    /// * `path` - Path to the devcontainer.json or devcontainer.jsonc file
    ///
    /// ## Returns
    ///
    /// Returns `Ok(DevContainerConfig)` with extends chain resolved and merged.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use deacon_core::config::ConfigLoader;
    /// use std::path::Path;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let config = ConfigLoader::load_with_extends(Path::new(".devcontainer/devcontainer.json"))?;
    /// println!("Loaded merged config: {:?}", config.name);
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip_all, fields(path = %path.display()))]
    pub fn load_with_extends(path: &Path) -> Result<DevContainerConfig> {
        debug!(
            "Loading configuration with extends resolution from {}",
            path.display()
        );

        let mut visited = HashSet::new();
        let configs = Self::resolve_extends_chain(path, &mut visited)?;

        debug!(
            "Resolved extends chain with {} configurations",
            configs.len()
        );

        // Merge all configurations in order (base to overlay)
        let merged = ConfigMerger::merge_configs(&configs);

        debug!("Configuration loading with extends complete");
        Ok(merged)
    }

    /// Recursively resolve the extends chain for a configuration
    ///
    /// This method loads a configuration and recursively resolves all configurations
    /// in its extends chain, performing cycle detection.
    ///
    /// ## Arguments
    ///
    /// * `config_path` - Path to the configuration file to resolve
    /// * `visited` - Set of already visited paths for cycle detection
    ///
    /// ## Returns
    ///
    /// Returns a vector of configurations in merge order (base first, overlay last).
    #[instrument(skip_all, fields(path = %config_path.display()))]
    fn resolve_extends_chain(
        config_path: &Path,
        visited: &mut HashSet<PathBuf>,
    ) -> Result<Vec<DevContainerConfig>> {
        let canonical_path = config_path.canonicalize().map_err(|e| {
            debug!(
                "Failed to canonicalize path {}: {}",
                config_path.display(),
                e
            );
            DeaconError::Config(ConfigError::NotFound {
                path: config_path.display().to_string(),
            })
        })?;

        // Check for cycles
        if visited.contains(&canonical_path) {
            let chain = visited
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(" -> ");
            let cycle_chain = format!("{} -> {}", chain, canonical_path.display());

            return Err(DeaconError::Config(ConfigError::ExtendsCycle {
                chain: cycle_chain,
            }));
        }

        visited.insert(canonical_path.clone());

        // Load the current configuration
        let config = Self::load_from_path(&canonical_path)?;

        let mut all_configs = Vec::new();

        // Recursively resolve extends
        if let Some(extends_paths) = &config.extends {
            debug!("Resolving {} extends paths", extends_paths.len());

            for extend_path in extends_paths {
                // Check for OCI references (not yet implemented)
                if extend_path.contains("://")
                    || extend_path.starts_with("ghcr.io/")
                    || extend_path.starts_with("mcr.microsoft.com/")
                {
                    warn!(
                        "OCI extends reference detected but not yet implemented: {}",
                        extend_path
                    );
                    return Err(DeaconError::Config(ConfigError::NotImplemented {
                        feature: format!("OCI extends reference: {}", extend_path),
                    }));
                }

                // Resolve relative path
                let base_dir = canonical_path.parent().unwrap_or(&canonical_path);
                let resolved_path = base_dir.join(extend_path);

                debug!(
                    "Resolving extends path: {} -> {}",
                    extend_path,
                    resolved_path.display()
                );

                // Recursively resolve the extended configuration
                let mut extended_configs = Self::resolve_extends_chain(&resolved_path, visited)?;
                all_configs.append(&mut extended_configs);
            }
        }

        // Add the current config last (highest precedence)
        let mut config_without_extends = config.clone();
        config_without_extends.extends = None; // Remove extends from final config
        all_configs.push(config_without_extends);

        visited.remove(&canonical_path);

        debug!(
            "Resolved extends chain for {}: {} total configs",
            canonical_path.display(),
            all_configs.len()
        );

        Ok(all_configs)
    }

    /// Load configuration with extends resolution and metadata tracking
    ///
    /// This method enhances the standard extends resolution by tracking the source
    /// and metadata of each configuration layer for debugging purposes.
    ///
    /// ## Arguments
    ///
    /// * `path` - Path to the base configuration file
    /// * `include_metadata` - Whether to include layer metadata in the result
    ///
    /// ## Returns
    ///
    /// Returns the merged configuration with optional metadata about the layers.
    #[instrument(skip_all, fields(path = %path.display()))]
    pub fn load_with_extends_and_metadata(
        path: &Path,
        include_metadata: bool,
    ) -> Result<crate::config::merge::MergedDevContainerConfig> {
        debug!(
            "Loading configuration with extends resolution and metadata from {}",
            path.display()
        );

        let mut visited = HashSet::new();
        let configs_with_paths = Self::resolve_extends_chain_with_paths(path, &mut visited)?;

        debug!(
            "Resolved extends chain with {} configurations",
            configs_with_paths.len()
        );

        // Use the layered merger with provenance tracking
        let result = crate::config::merge::LayeredConfigMerger::merge_with_provenance(
            &configs_with_paths
                .iter()
                .map(|(config, path)| (config.clone(), path.as_path()))
                .collect::<Vec<_>>(),
            include_metadata,
        );

        debug!("Configuration loading with extends and metadata complete");
        Ok(result)
    }

    /// Recursively resolve the extends chain for a configuration with path tracking
    ///
    /// This method loads a configuration and recursively resolves all configurations
    /// in its extends chain, performing cycle detection while preserving path information.
    ///
    /// ## Arguments
    ///
    /// * `config_path` - Path to the configuration file to resolve
    /// * `visited` - Set of already visited paths for cycle detection
    ///
    /// ## Returns
    ///
    /// Returns a vector of configurations with their source paths in merge order (base first, overlay last).
    #[instrument(skip_all, fields(path = %config_path.display()))]
    fn resolve_extends_chain_with_paths(
        config_path: &Path,
        visited: &mut HashSet<PathBuf>,
    ) -> Result<Vec<(DevContainerConfig, PathBuf)>> {
        let canonical_path = config_path.canonicalize().map_err(|e| {
            debug!(
                "Failed to canonicalize path {}: {}",
                config_path.display(),
                e
            );
            DeaconError::Config(ConfigError::NotFound {
                path: config_path.display().to_string(),
            })
        })?;

        // Check for cycles
        if visited.contains(&canonical_path) {
            let chain = visited
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(" -> ");
            let cycle_chain = format!("{} -> {}", chain, canonical_path.display());

            return Err(DeaconError::Config(ConfigError::ExtendsCycle {
                chain: cycle_chain,
            }));
        }

        visited.insert(canonical_path.clone());

        // Load the current configuration
        let config = Self::load_from_path(&canonical_path)?;

        let mut all_configs = Vec::new();

        // Recursively resolve extends
        if let Some(extends_paths) = &config.extends {
            debug!("Resolving {} extends paths", extends_paths.len());

            for extend_path in extends_paths {
                // Check for OCI references (not yet implemented)
                if extend_path.contains("://")
                    || extend_path.starts_with("ghcr.io/")
                    || extend_path.starts_with("mcr.microsoft.com/")
                {
                    warn!(
                        "OCI extends reference detected but not yet implemented: {}",
                        extend_path
                    );
                    return Err(DeaconError::Config(ConfigError::NotImplemented {
                        feature: format!("OCI extends reference: {}", extend_path),
                    }));
                }

                // Resolve relative path
                let base_dir = canonical_path.parent().unwrap_or(&canonical_path);
                let resolved_path = base_dir.join(extend_path);

                debug!(
                    "Resolving extends path: {} -> {}",
                    extend_path,
                    resolved_path.display()
                );

                // Recursively resolve the extended configuration
                let mut extended_configs =
                    Self::resolve_extends_chain_with_paths(&resolved_path, visited)?;
                all_configs.append(&mut extended_configs);
            }
        }

        // Add the current config last (highest precedence)
        let mut config_without_extends = config.clone();
        config_without_extends.extends = None; // Remove extends from final config
        all_configs.push((config_without_extends, canonical_path.clone()));

        visited.remove(&canonical_path);

        debug!(
            "Resolved extends chain for {}: {} total configs",
            canonical_path.display(),
            all_configs.len()
        );

        Ok(all_configs)
    }

    /// Enhanced load with overrides, substitution, and metadata tracking
    ///
    /// This method combines the full layered configuration resolution with metadata tracking.
    /// It loads the base configuration, resolves extends chain, applies overrides, performs
    /// variable substitution, and optionally includes layer metadata for debugging.
    ///
    /// ## Arguments
    ///
    /// * `path` - Path to the base configuration file
    /// * `override_config_path` - Optional path to override configuration file  
    /// * `secrets` - Optional secrets collection for variable substitution
    /// * `workspace_path` - Workspace path for variable substitution context
    /// * `include_metadata` - Whether to include layer metadata in the result
    ///
    /// ## Returns
    ///
    /// Returns the merged configuration with optional metadata and substitution report.
    #[instrument(skip_all, fields(path = %path.display(), override_path = ?override_config_path.as_ref().map(|p| p.display())))]
    pub fn load_with_full_resolution(
        path: &Path,
        override_config_path: Option<&Path>,
        secrets: Option<&crate::secrets::SecretsCollection>,
        workspace_path: &Path,
        include_metadata: bool,
    ) -> Result<(
        crate::config::merge::MergedDevContainerConfig,
        crate::variable::SubstitutionReport,
    )> {
        debug!(
            "Loading configuration with full resolution from {}",
            path.display()
        );

        // Load base config with extends resolution and path tracking
        let mut configs_with_paths = {
            let mut visited = HashSet::new();
            Self::resolve_extends_chain_with_paths(path, &mut visited)?
        };

        // Add override config if provided
        if let Some(override_path) = override_config_path {
            debug!(
                "Loading override configuration from {}",
                override_path.display()
            );
            let override_config = Self::load_from_path(override_path)?;
            configs_with_paths.push((override_config, override_path.to_path_buf()));
        }

        debug!(
            "Resolved configuration chain with {} configs (including override)",
            configs_with_paths.len()
        );

        // Use the layered merger with provenance tracking
        let merged_result = crate::config::merge::LayeredConfigMerger::merge_with_provenance(
            &configs_with_paths
                .iter()
                .map(|(config, path)| (config.clone(), path.as_path()))
                .collect::<Vec<_>>(),
            include_metadata,
        );

        // Apply variable substitution with secrets
        let mut substitution_context = crate::variable::SubstitutionContext::new(workspace_path)?;

        // Add secrets to local environment for substitution
        if let Some(secrets) = secrets {
            for (key, value) in secrets.as_env_vars() {
                substitution_context
                    .local_env
                    .insert(key.clone(), value.clone());
            }
        }

        let (substituted_config, substitution_report) = merged_result
            .config
            .apply_variable_substitution(&substitution_context);

        // Reconstruct the result with the substituted config
        let final_result = crate::config::merge::MergedDevContainerConfig {
            config: substituted_config,
            meta: merged_result.meta,
        };

        debug!("Configuration loading with full resolution complete");
        Ok((final_result, substitution_report))
    }

    /// Load configuration with extends resolution and optional override config
    ///
    /// This method loads the base configuration, resolves extends chain,
    /// and optionally applies an override configuration with the highest precedence.
    /// It supports variable substitution with secrets integration.
    ///
    /// ## Arguments
    ///
    /// * `path` - Path to the base configuration file
    /// * `override_config_path` - Optional path to override configuration file  
    /// * `secrets` - Optional secrets collection for variable substitution
    /// * `workspace_path` - Workspace path for variable substitution context
    ///
    /// ## Returns
    ///
    /// Returns the merged and substituted configuration with substitution report.
    #[instrument(skip_all, fields(path = %path.display(), override_path = ?override_config_path.as_ref().map(|p| p.display())))]
    pub fn load_with_overrides_and_substitution(
        path: &Path,
        override_config_path: Option<&Path>,
        secrets: Option<&crate::secrets::SecretsCollection>,
        workspace_path: &Path,
    ) -> Result<(DevContainerConfig, crate::variable::SubstitutionReport)> {
        debug!(
            "Loading configuration with overrides and substitution from {}",
            path.display()
        );

        // Load base config with extends resolution
        let mut configs = {
            let mut visited = HashSet::new();
            Self::resolve_extends_chain(path, &mut visited)?
        };

        // Add override config if provided
        if let Some(override_path) = override_config_path {
            debug!(
                "Loading override configuration from {}",
                override_path.display()
            );
            let override_config = Self::load_from_path(override_path)?;
            configs.push(override_config);
        }

        debug!(
            "Resolved configuration chain with {} configs (including override)",
            configs.len()
        );

        // Merge all configurations in order (base to overlay to override)
        let merged = ConfigMerger::merge_configs(&configs);

        // Apply variable substitution with secrets
        let mut substitution_context = crate::variable::SubstitutionContext::new(workspace_path)?;

        // Add secrets to local environment for substitution
        if let Some(secrets) = secrets {
            for (key, value) in secrets.as_env_vars() {
                substitution_context
                    .local_env
                    .insert(key.clone(), value.clone());
            }
        }

        let (substituted_config, substitution_report) =
            merged.apply_variable_substitution(&substitution_context);

        debug!("Configuration loading with overrides and substitution complete");
        Ok((substituted_config, substitution_report))
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
            "extends",
            "name",
            "image",
            "dockerFile",
            "build",
            "dockerComposeFile",
            "service",
            "runServices",
            "features",
            "customizations",
            "workspaceFolder",
            "mounts",
            "containerEnv",
            "remoteEnv",
            "containerUser",
            "remoteUser",
            "updateRemoteUserUID",
            "forwardPorts",
            "appPort",
            "portsAttributes",
            "otherPortsAttributes",
            "runArgs",
            "shutdownAction",
            "overrideCommand",
            "onCreateCommand",
            "postStartCommand",
            "postCreateCommand",
            "postAttachCommand",
            "initializeCommand",
            "updateContentCommand",
            "hostRequirements",
            "privileged",
            "capAdd",
            "securityOpt",
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

        // Validate port attributes references
        Self::validate_port_attributes(config)?;

        Ok(())
    }

    /// Validate that ports referenced in port attributes exist in forwardPorts or appPort.
    ///
    /// This method checks that all ports specified in `ports_attributes` have corresponding
    /// entries in `forward_ports` or match the `app_port`. Issues warnings for missing references.
    fn validate_port_attributes(config: &DevContainerConfig) -> Result<()> {
        if config.ports_attributes.is_empty() {
            return Ok(());
        }

        // Collect all valid port references
        let mut valid_ports = std::collections::HashSet::new();

        // Add ports from forward_ports
        for port_spec in &config.forward_ports {
            if let Some(port_num) = port_spec.primary_port() {
                valid_ports.insert(port_num.to_string());
                // Also add with /tcp suffix which is common
                valid_ports.insert(format!("{}/tcp", port_num));
            }
            // Also add the string representation for exact matching
            valid_ports.insert(port_spec.as_string());
        }

        // Add app_port if specified
        if let Some(app_port) = &config.app_port {
            if let Some(port_num) = app_port.primary_port() {
                valid_ports.insert(port_num.to_string());
                valid_ports.insert(format!("{}/tcp", port_num));
            }
            valid_ports.insert(app_port.as_string());
        }

        // Check each port attribute reference
        for port_key in config.ports_attributes.keys() {
            if !valid_ports.contains(port_key) {
                // Try parsing as just a port number
                if let Ok(port_num) = port_key.parse::<u16>() {
                    if !valid_ports.contains(&port_num.to_string()) {
                        warn!(
                            "Port '{}' in portsAttributes does not match any port in forwardPorts or appPort",
                            port_key
                        );
                    }
                } else {
                    warn!(
                        "Port '{}' in portsAttributes does not match any port in forwardPorts or appPort",
                        port_key
                    );
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
    fn test_extends_field_parsing() -> anyhow::Result<()> {
        // Test string extends
        let config_content = r#"{
            "name": "Test",
            "extends": "../base/devcontainer.json"
        }"#;

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(config_content.as_bytes())?;

        let result = ConfigLoader::load_from_path(temp_file.path());
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(
            config.extends,
            Some(vec!["../base/devcontainer.json".to_string()])
        );

        // Test array extends
        let config_content = r#"{
            "name": "Test",
            "extends": ["../base1/devcontainer.json", "../base2/devcontainer.json"]
        }"#;

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(config_content.as_bytes())?;

        let result = ConfigLoader::load_from_path(temp_file.path());
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(
            config.extends,
            Some(vec![
                "../base1/devcontainer.json".to_string(),
                "../base2/devcontainer.json".to_string()
            ])
        );

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

    #[test]
    fn test_port_spec_number() {
        let port = PortSpec::Number(3000);
        assert_eq!(port.primary_port(), Some(3000));
        assert_eq!(port.as_string(), "3000");
    }

    #[test]
    fn test_port_spec_string_number() {
        let port = PortSpec::String("3000".to_string());
        assert_eq!(port.primary_port(), Some(3000));
        assert_eq!(port.as_string(), "3000");
    }

    #[test]
    fn test_port_spec_string_mapping() {
        let port = PortSpec::String("3000:8080".to_string());
        assert_eq!(port.primary_port(), Some(3000));
        assert_eq!(port.as_string(), "3000:8080");
    }

    #[test]
    fn test_port_spec_string_invalid() {
        let port = PortSpec::String("invalid".to_string());
        assert_eq!(port.primary_port(), None);
        assert_eq!(port.as_string(), "invalid");
    }

    #[test]
    fn test_port_spec_deserialization() -> anyhow::Result<()> {
        // Test deserializing a number
        let json_number = "3000";
        let port: PortSpec = serde_json::from_str(json_number)?;
        assert_eq!(port, PortSpec::Number(3000));

        // Test deserializing a string
        let json_string = r#""3000:8080""#;
        let port: PortSpec = serde_json::from_str(json_string)?;
        assert_eq!(port, PortSpec::String("3000:8080".to_string()));

        Ok(())
    }

    #[test]
    fn test_on_auto_forward_deserialization() -> anyhow::Result<()> {
        let test_cases = [
            ("\"silent\"", OnAutoForward::Silent),
            ("\"notify\"", OnAutoForward::Notify),
            ("\"openBrowser\"", OnAutoForward::OpenBrowser),
            ("\"openPreview\"", OnAutoForward::OpenPreview),
            ("\"ignore\"", OnAutoForward::Ignore),
        ];

        for (json, expected) in test_cases {
            let parsed: OnAutoForward = serde_json::from_str(json)?;
            assert_eq!(parsed, expected);
        }

        Ok(())
    }

    #[test]
    fn test_port_attributes_deserialization() -> anyhow::Result<()> {
        let json = r#"{
            "label": "Web Server",
            "onAutoForward": "openBrowser",
            "openPreview": true,
            "requireLocalPort": false,
            "description": "Main application port"
        }"#;

        let attrs: PortAttributes = serde_json::from_str(json)?;
        assert_eq!(attrs.label, Some("Web Server".to_string()));
        assert_eq!(attrs.on_auto_forward, Some(OnAutoForward::OpenBrowser));
        assert_eq!(attrs.open_preview, Some(true));
        assert_eq!(attrs.require_local_port, Some(false));
        assert_eq!(attrs.description, Some("Main application port".to_string()));

        Ok(())
    }

    #[test]
    fn test_port_attributes_partial() -> anyhow::Result<()> {
        let json = r#"{
            "label": "API Server"
        }"#;

        let attrs: PortAttributes = serde_json::from_str(json)?;
        assert_eq!(attrs.label, Some("API Server".to_string()));
        assert_eq!(attrs.on_auto_forward, None);
        assert_eq!(attrs.open_preview, None);
        assert_eq!(attrs.require_local_port, None);
        assert_eq!(attrs.description, None);

        Ok(())
    }

    #[test]
    fn test_config_with_ports_and_attributes() -> anyhow::Result<()> {
        let config_content = r#"{
            "name": "Test Container",
            "image": "node:18",
            "forwardPorts": [3000, "8080:8080"],
            "appPort": 3000,
            "portsAttributes": {
                "3000": {
                    "label": "Web Server",
                    "onAutoForward": "openBrowser",
                    "description": "Main web application"
                },
                "8080": {
                    "label": "API Server",
                    "onAutoForward": "notify"
                }
            },
            "otherPortsAttributes": {
                "label": "Other Service",
                "onAutoForward": "silent"
            }
        }"#;

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(config_content.as_bytes())?;

        let config = ConfigLoader::load_from_path(temp_file.path())?;

        assert_eq!(config.name, Some("Test Container".to_string()));
        assert_eq!(config.forward_ports.len(), 2);
        assert_eq!(config.app_port, Some(PortSpec::Number(3000)));

        // Check port attributes
        assert_eq!(config.ports_attributes.len(), 2);

        let port_3000_attrs = config.ports_attributes.get("3000").unwrap();
        assert_eq!(port_3000_attrs.label, Some("Web Server".to_string()));
        assert_eq!(
            port_3000_attrs.on_auto_forward,
            Some(OnAutoForward::OpenBrowser)
        );
        assert_eq!(
            port_3000_attrs.description,
            Some("Main web application".to_string())
        );

        let port_8080_attrs = config.ports_attributes.get("8080").unwrap();
        assert_eq!(port_8080_attrs.label, Some("API Server".to_string()));
        assert_eq!(port_8080_attrs.on_auto_forward, Some(OnAutoForward::Notify));

        // Check other ports attributes
        let other_attrs = config.other_ports_attributes.as_ref().unwrap();
        assert_eq!(other_attrs.label, Some("Other Service".to_string()));
        assert_eq!(other_attrs.on_auto_forward, Some(OnAutoForward::Silent));

        Ok(())
    }

    #[test]
    fn test_port_validation_valid_references() -> anyhow::Result<()> {
        let config_content = r#"{
            "name": "Test Container",
            "image": "node:18",
            "forwardPorts": [3000, 8080],
            "appPort": 9000,
            "portsAttributes": {
                "3000": { "label": "Web" },
                "8080": { "label": "API" },
                "9000": { "label": "App" }
            }
        }"#;

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(config_content.as_bytes())?;

        // This should not fail validation
        let config = ConfigLoader::load_from_path(temp_file.path())?;
        assert_eq!(config.ports_attributes.len(), 3);

        Ok(())
    }

    #[test]
    fn test_port_validation_with_string_ports() -> anyhow::Result<()> {
        let config_content = r#"{
            "name": "Test Container",
            "image": "node:18", 
            "forwardPorts": ["3000:3000", "8080"],
            "portsAttributes": {
                "3000:3000": { "label": "Web" },
                "8080": { "label": "API" }
            }
        }"#;

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(config_content.as_bytes())?;

        let config = ConfigLoader::load_from_path(temp_file.path())?;
        assert_eq!(config.ports_attributes.len(), 2);

        Ok(())
    }

    #[test]
    fn test_port_validation_missing_references() -> anyhow::Result<()> {
        let config_content = r#"{
            "name": "Test Container",
            "image": "node:18",
            "forwardPorts": [3000],
            "portsAttributes": {
                "3000": { "label": "Web" },
                "8080": { "label": "Missing Port" }
            }
        }"#;

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(config_content.as_bytes())?;

        // This should load but log warnings about missing port 8080
        let config = ConfigLoader::load_from_path(temp_file.path())?;
        assert_eq!(config.ports_attributes.len(), 2);

        Ok(())
    }

    #[test]
    fn test_config_default_includes_new_fields() {
        let config = DevContainerConfig::default();
        assert_eq!(config.ports_attributes.len(), 0);
        assert_eq!(config.other_ports_attributes, None);
        assert_eq!(config.forward_ports.len(), 0);
        assert_eq!(config.app_port, None);
        assert_eq!(config.container_user, None);
        assert_eq!(config.remote_user, None);
        assert_eq!(config.update_remote_user_uid, None);
    }

    #[test]
    fn test_config_with_user_mapping_fields() -> anyhow::Result<()> {
        let config_content = r#"{
            "name": "Test Container with User Mapping",
            "image": "ubuntu:20.04",
            "containerUser": "1000",
            "remoteUser": "vscode",
            "updateRemoteUserUID": true
        }"#;

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(config_content.as_bytes())?;

        let config = ConfigLoader::load_from_path(temp_file.path())?;

        assert_eq!(
            config.name,
            Some("Test Container with User Mapping".to_string())
        );
        assert_eq!(config.image, Some("ubuntu:20.04".to_string()));
        assert_eq!(config.container_user, Some("1000".to_string()));
        assert_eq!(config.remote_user, Some("vscode".to_string()));
        assert_eq!(config.update_remote_user_uid, Some(true));

        Ok(())
    }

    #[test]
    fn test_config_user_mapping_merge() {
        let base_config = DevContainerConfig {
            name: Some("Base".to_string()),
            image: Some("ubuntu:20.04".to_string()),
            container_user: Some("root".to_string()),
            remote_user: Some("user".to_string()),
            update_remote_user_uid: Some(false),
            ..DevContainerConfig::default()
        };

        let overlay_config = DevContainerConfig {
            name: Some("Overlay".to_string()),
            remote_user: Some("vscode".to_string()),
            update_remote_user_uid: Some(true),
            ..DevContainerConfig::default()
        };

        let merged = ConfigMerger::merge_configs(&[base_config, overlay_config]);

        assert_eq!(merged.name, Some("Overlay".to_string()));
        assert_eq!(merged.image, Some("ubuntu:20.04".to_string()));
        assert_eq!(merged.container_user, Some("root".to_string())); // From base
        assert_eq!(merged.remote_user, Some("vscode".to_string())); // From overlay
        assert_eq!(merged.update_remote_user_uid, Some(true)); // From overlay
    }
}

pub mod merge {
    //! Configuration merge engine with layered provenance tracking
    //!
    //! This module implements the full layered configuration resolution as specified
    //! in the CLI specification: defaults → base → extends chain(s) → workspace overrides
    //! → runtime substitutions.
    //!
    //! The merge engine tracks the source and hash of each configuration layer to provide
    //! full debugging provenance when requested via `--include-merged-configuration`.

    use super::DevContainerConfig;
    use serde::{Deserialize, Serialize};
    use std::path::Path;
    use tracing::{debug, instrument};

    /// Metadata about a configuration layer for debugging and provenance tracking
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct ConfigLayer {
        /// Source path or identifier for this configuration layer
        pub source: String,
        /// SHA-256 hash of the configuration content for integrity checking
        pub hash: String,
        /// Order in the merge chain (0 = lowest precedence, higher = higher precedence)
        pub precedence: u32,
    }

    /// Extended configuration that includes merge metadata
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct MergedDevContainerConfig {
        /// The merged configuration data
        #[serde(flatten)]
        pub config: DevContainerConfig,
        /// Metadata about the configuration layers
        #[serde(rename = "__meta")]
        pub meta: Option<ConfigMeta>,
    }

    /// Metadata container for merged configurations
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct ConfigMeta {
        /// List of configuration layers in merge order
        pub layers: Vec<ConfigLayer>,
    }

    /// Enhanced configuration merger that tracks layer provenance
    pub struct LayeredConfigMerger;

    impl LayeredConfigMerger {
        /// Merge multiple configurations with full provenance tracking
        ///
        /// ## Arguments
        ///
        /// * `configs_with_sources` - Configurations with their source information in merge order
        /// * `include_metadata` - Whether to include layer metadata in the result
        ///
        /// ## Returns
        ///
        /// Returns the merged configuration with optional metadata about the layers.
        #[instrument(skip_all)]
        pub fn merge_with_provenance(
            configs_with_sources: &[(DevContainerConfig, &Path)],
            include_metadata: bool,
        ) -> MergedDevContainerConfig {
            debug!(
                "Merging {} configurations with metadata={}",
                configs_with_sources.len(),
                include_metadata
            );

            if configs_with_sources.is_empty() {
                return MergedDevContainerConfig {
                    config: DevContainerConfig::default(),
                    meta: if include_metadata {
                        Some(ConfigMeta { layers: vec![] })
                    } else {
                        None
                    },
                };
            }

            // Extract just the configs for the existing merge logic
            let configs: Vec<&DevContainerConfig> = configs_with_sources
                .iter()
                .map(|(config, _)| config)
                .collect();

            // Use existing merge logic
            let merged_config = super::ConfigMerger::merge_configs(
                &configs.into_iter().cloned().collect::<Vec<_>>(),
            );

            // Build metadata if requested
            let meta = if include_metadata {
                let layers: Vec<ConfigLayer> = configs_with_sources
                    .iter()
                    .enumerate()
                    .map(|(index, (config, source))| {
                        // Prefer hashing raw source file bytes for deterministic provenance.
                        // Fallback to hashing canonicalized JSON if reading the file fails
                        // (e.g., the file was removed between discovery and hashing).
                        let hash = Self::calculate_file_hash(source).unwrap_or_else(|_| {
                            let config_json = serde_json::to_string(config).unwrap_or_default();
                            Self::calculate_hash(&config_json)
                        });

                        ConfigLayer {
                            source: source.display().to_string(),
                            hash,
                            precedence: index as u32,
                        }
                    })
                    .collect();

                Some(ConfigMeta { layers })
            } else {
                None
            };

            MergedDevContainerConfig {
                config: merged_config,
                meta,
            }
        }

        /// Calculate SHA-256 hash of configuration content
        fn calculate_hash(content: &str) -> String {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(content.as_bytes());
            format!("{:x}", hasher.finalize())
        }

        /// Calculate SHA-256 hash of a file's raw bytes. Returns io::Error on failure.
        fn calculate_file_hash(path: &Path) -> std::io::Result<String> {
            use sha2::{Digest, Sha256};
            let bytes = std::fs::read(path)?;
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            Ok(format!("{:x}", hasher.finalize()))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use tempfile::TempDir;

        fn create_test_config(name: &str, image: &str) -> DevContainerConfig {
            DevContainerConfig {
                name: Some(name.to_string()),
                image: Some(image.to_string()),
                ..Default::default()
            }
        }

        #[test]
        fn test_merge_empty_configs() {
            let result = LayeredConfigMerger::merge_with_provenance(&[], false);
            assert_eq!(result.config, DevContainerConfig::default());
            assert!(result.meta.is_none());
        }

        #[test]
        fn test_merge_empty_configs_with_metadata() {
            let result = LayeredConfigMerger::merge_with_provenance(&[], true);
            assert_eq!(result.config, DevContainerConfig::default());
            assert!(result.meta.is_some());
            assert_eq!(result.meta.unwrap().layers.len(), 0);
        }

        #[test]
        fn test_merge_single_config() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join("devcontainer.json");
            let config = create_test_config("test", "ubuntu:20.04");

            let result = LayeredConfigMerger::merge_with_provenance(
                &[(config.clone(), &config_path)],
                false,
            );
            assert_eq!(result.config, config);
            assert!(result.meta.is_none());
        }

        #[test]
        fn test_merge_single_config_with_metadata() {
            let temp_dir = TempDir::new().unwrap();
            let config_path = temp_dir.path().join("devcontainer.json");
            let config = create_test_config("test", "ubuntu:20.04");

            let result =
                LayeredConfigMerger::merge_with_provenance(&[(config.clone(), &config_path)], true);
            assert_eq!(result.config, config);
            assert!(result.meta.is_some());

            let meta = result.meta.unwrap();
            assert_eq!(meta.layers.len(), 1);
            assert_eq!(meta.layers[0].source, config_path.display().to_string());
            assert_eq!(meta.layers[0].precedence, 0);
            assert!(!meta.layers[0].hash.is_empty());
        }

        #[test]
        fn test_merge_multiple_configs_with_metadata() {
            let temp_dir = TempDir::new().unwrap();
            let base_path = temp_dir.path().join("base/devcontainer.json");
            let app_path = temp_dir.path().join("app/devcontainer.json");

            let base_config = DevContainerConfig {
                name: Some("Base".to_string()),
                image: Some("ubuntu:20.04".to_string()),
                container_env: [("BASE_VAR".to_string(), "base_value".to_string())]
                    .iter()
                    .cloned()
                    .collect(),
                ..Default::default()
            };

            let app_config = DevContainerConfig {
                name: Some("App".to_string()),
                container_env: [("APP_VAR".to_string(), "app_value".to_string())]
                    .iter()
                    .cloned()
                    .collect(),
                ..Default::default()
            };

            let configs_with_sources = vec![
                (base_config, base_path.as_path()),
                (app_config, app_path.as_path()),
            ];

            let result = LayeredConfigMerger::merge_with_provenance(&configs_with_sources, true);

            // Check merged config
            assert_eq!(result.config.name, Some("App".to_string())); // Override
            assert_eq!(result.config.image, Some("ubuntu:20.04".to_string())); // From base
            assert_eq!(
                result.config.container_env.get("BASE_VAR"),
                Some(&"base_value".to_string())
            );
            assert_eq!(
                result.config.container_env.get("APP_VAR"),
                Some(&"app_value".to_string())
            );

            // Check metadata
            assert!(result.meta.is_some());
            let meta = result.meta.unwrap();
            assert_eq!(meta.layers.len(), 2);

            // Check layer metadata
            assert_eq!(meta.layers[0].source, base_path.display().to_string());
            assert_eq!(meta.layers[0].precedence, 0);
            assert_eq!(meta.layers[1].source, app_path.display().to_string());
            assert_eq!(meta.layers[1].precedence, 1);

            // Verify hashes are different
            assert_ne!(meta.layers[0].hash, meta.layers[1].hash);
        }

        #[test]
        fn test_hash_calculation() {
            let hash1 = LayeredConfigMerger::calculate_hash("test content");
            let hash2 = LayeredConfigMerger::calculate_hash("test content");
            let hash3 = LayeredConfigMerger::calculate_hash("different content");

            // Same content should produce same hash
            assert_eq!(hash1, hash2);
            // Different content should produce different hash
            assert_ne!(hash1, hash3);
            // Hash should be 64 characters (SHA-256 in hex)
            assert_eq!(hash1.len(), 64);
        }

        #[test]
        fn test_merge_precedence_order() {
            let temp_dir = TempDir::new().unwrap();
            let path1 = temp_dir.path().join("config1.json");
            let path2 = temp_dir.path().join("config2.json");
            let path3 = temp_dir.path().join("config3.json");

            let config1 = DevContainerConfig {
                name: Some("Config1".to_string()),
                container_env: [("VAR".to_string(), "value1".to_string())]
                    .iter()
                    .cloned()
                    .collect(),
                ..Default::default()
            };

            let config2 = DevContainerConfig {
                name: Some("Config2".to_string()),
                container_env: [("VAR".to_string(), "value2".to_string())]
                    .iter()
                    .cloned()
                    .collect(),
                ..Default::default()
            };

            let config3 = DevContainerConfig {
                name: Some("Config3".to_string()),
                ..Default::default()
            };

            let configs_with_sources = vec![
                (config1, path1.as_path()),
                (config2, path2.as_path()),
                (config3, path3.as_path()),
            ];

            let result = LayeredConfigMerger::merge_with_provenance(&configs_with_sources, true);

            // Config3 should have highest precedence (last in chain)
            assert_eq!(result.config.name, Some("Config3".to_string()));
            // VAR should be from Config2 (Config3 doesn't override it)
            assert_eq!(
                result.config.container_env.get("VAR"),
                Some(&"value2".to_string())
            );

            // Check metadata precedence
            let meta = result.meta.unwrap();
            assert_eq!(meta.layers[0].precedence, 0);
            assert_eq!(meta.layers[1].precedence, 1);
            assert_eq!(meta.layers[2].precedence, 2);
        }
    }
}
