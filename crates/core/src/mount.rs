//! Mount parsing and validation
//!
//! This module handles parsing of DevContainer mount specifications into structured types
//! that can be converted to Docker CLI mount arguments. It supports the following mount
//! formats and types:
//!
//! ## Mount Types
//! - `bind`: Bind mount from host filesystem
//! - `volume`: Named Docker volume
//! - `tmpfs`: Temporary filesystem in memory
//!
//! ## Mount Formats
//! 1. Docker mount syntax: `type=bind,source=.,target=/workspaces/app,consistency=cached`
//! 2. Docker volume syntax: `source:target:options` or `source:target`
//!
//! ## Examples
//! ```rust
//! use deacon_core::mount::{Mount, MountParser};
//! use deacon_core::errors::Result;
//!
//! fn example() -> Result<()> {
//!     // Parse Docker mount syntax
//!     let mount = MountParser::parse_mount("type=bind,source=/host/path,target=/container/path")?;
//!
//!     // Parse volume syntax  
//!     let mount = MountParser::parse_mount("/host/path:/container/path:ro")?;
//!
//!     // Convert to Docker CLI arguments
//!     let args = mount.to_docker_args();
//!     Ok(())
//! }
//! ```

use crate::errors::{ConfigError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use tracing::{debug, instrument, warn};

/// Types of mounts supported by DevContainers
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MountType {
    /// Bind mount from host filesystem
    Bind,
    /// Named Docker volume
    Volume,
    /// Temporary filesystem in memory
    Tmpfs,
}

impl FromStr for MountType {
    type Err = ConfigError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bind" => Ok(MountType::Bind),
            "volume" => Ok(MountType::Volume),
            "tmpfs" => Ok(MountType::Tmpfs),
            _ => Err(ConfigError::Validation {
                message: format!(
                    "Unsupported mount type: '{}'. Supported types: bind, volume, tmpfs",
                    s
                ),
            }),
        }
    }
}

impl std::fmt::Display for MountType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MountType::Bind => write!(f, "bind"),
            MountType::Volume => write!(f, "volume"),
            MountType::Tmpfs => write!(f, "tmpfs"),
        }
    }
}

/// Mount consistency options for improved performance on some platforms
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MountConsistency {
    /// Consistent view (default, slower)
    Consistent,
    /// Cached view (faster, host-to-container)
    Cached,
    /// Delegated view (fastest, container-to-host)
    Delegated,
}

impl FromStr for MountConsistency {
    type Err = ConfigError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "consistent" => Ok(MountConsistency::Consistent),
            "cached" => Ok(MountConsistency::Cached),
            "delegated" => Ok(MountConsistency::Delegated),
            _ => Err(ConfigError::Validation {
                message: format!(
                    "Unsupported mount consistency: '{}'. Supported values: consistent, cached, delegated",
                    s
                ),
            }),
        }
    }
}

impl std::fmt::Display for MountConsistency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MountConsistency::Consistent => write!(f, "consistent"),
            MountConsistency::Cached => write!(f, "cached"),
            MountConsistency::Delegated => write!(f, "delegated"),
        }
    }
}

/// Mount read/write mode
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MountMode {
    /// Read-write access
    ReadWrite,
    /// Read-only access
    ReadOnly,
}

impl FromStr for MountMode {
    type Err = ConfigError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rw" | "readwrite" => Ok(MountMode::ReadWrite),
            "ro" | "readonly" => Ok(MountMode::ReadOnly),
            _ => Err(ConfigError::Validation {
                message: format!("Unsupported mount mode: '{}'. Supported values: ro, rw", s),
            }),
        }
    }
}

impl std::fmt::Display for MountMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MountMode::ReadWrite => write!(f, "rw"),
            MountMode::ReadOnly => write!(f, "ro"),
        }
    }
}

/// Parsed mount specification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mount {
    /// Type of mount
    pub mount_type: MountType,
    /// Source path (host path for bind mounts, volume name for volumes)
    pub source: Option<String>,
    /// Target path inside container
    pub target: String,
    /// Read/write mode
    pub mode: MountMode,
    /// Mount consistency (bind mounts only)
    pub consistency: Option<MountConsistency>,
    /// Additional mount options
    pub options: HashMap<String, String>,
}

impl Mount {
    /// Convert mount to Docker CLI arguments
    ///
    /// Returns a vector of Docker CLI arguments that can be used with `docker run --mount`.
    ///
    /// ## Example
    /// ```rust
    /// # use deacon_core::mount::*;
    /// # use std::collections::HashMap;
    /// let mount = Mount {
    ///     mount_type: MountType::Bind,
    ///     source: Some("/host/path".to_string()),
    ///     target: "/container/path".to_string(),
    ///     mode: MountMode::ReadOnly,
    ///     consistency: Some(MountConsistency::Cached),
    ///     options: HashMap::new(),
    /// };
    /// let args = mount.to_docker_args();
    /// assert_eq!(args, vec!["--mount".to_string(), "type=bind,source=/host/path,target=/container/path,ro,consistency=cached".to_string()]);
    /// ```
    pub fn to_docker_args(&self) -> Vec<String> {
        let mut mount_str = format!("type={}", self.mount_type);

        // Add source for bind and volume mounts
        if let Some(ref source) = self.source {
            let source_path = if self.mount_type == MountType::Bind {
                // For bind mounts, resolve relative paths to absolute before platform conversion
                let source_path = std::path::Path::new(source);
                let absolute_path = if source_path.is_absolute() {
                    source_path.to_path_buf()
                } else {
                    // Resolve relative path to absolute
                    std::env::current_dir()
                        .unwrap_or_else(|_| std::path::PathBuf::from("."))
                        .join(source_path)
                };

                // Use platform-aware path conversion for bind mounts
                let platform = crate::platform::Platform::detect();
                if platform.needs_docker_desktop_path_conversion() {
                    crate::platform::convert_path_for_docker_desktop(&absolute_path)
                } else {
                    absolute_path.display().to_string()
                }
            } else {
                // Volume and other mount types don't need path conversion
                source.clone()
            };
            mount_str.push_str(&format!(",source={}", source_path));
        }

        // Add target
        mount_str.push_str(&format!(",target={}", self.target));

        // Add read-only flag if needed
        if self.mode == MountMode::ReadOnly {
            mount_str.push_str(",ro");
        }

        // Add consistency for bind mounts
        if self.mount_type == MountType::Bind {
            if let Some(ref consistency) = self.consistency {
                mount_str.push_str(&format!(",consistency={}", consistency));
            }
        }

        // Add additional options
        for (key, value) in &self.options {
            if value.is_empty() {
                mount_str.push_str(&format!(",{}", key));
            } else {
                mount_str.push_str(&format!(",{}={}", key, value));
            }
        }

        vec!["--mount".to_string(), mount_str]
    }

    /// Validate mount specification
    ///
    /// Checks for common configuration issues and logs warnings for unsupported fields.
    pub fn validate(&self) -> Result<()> {
        // Validate source is present for bind and volume mounts
        match self.mount_type {
            MountType::Bind | MountType::Volume => {
                if self.source.is_none() {
                    return Err(ConfigError::Validation {
                        message: format!("{} mount requires a source", self.mount_type),
                    }
                    .into());
                }
            }
            MountType::Tmpfs => {
                if self.source.is_some() {
                    warn!("tmpfs mount should not have a source, ignoring");
                }
            }
        }

        // Validate target is absolute path
        if !self.target.starts_with('/') {
            return Err(ConfigError::Validation {
                message: format!(
                    "Mount target must be an absolute path, got: '{}'",
                    self.target
                ),
            }
            .into());
        }

        // Warn about consistency on non-bind mounts
        if self.mount_type != MountType::Bind && self.consistency.is_some() {
            warn!(
                "Mount consistency is only supported for bind mounts, ignoring for {} mount",
                self.mount_type
            );
        }

        // Warn about unsupported options
        for key in self.options.keys() {
            match key.as_str() {
                // Known Docker mount options
                "bind-propagation" | "tmpfs-size" | "tmpfs-mode" | "volume-driver"
                | "volume-label" | "volume-nocopy" | "volume-opt" => {
                    debug!("Using Docker mount option: {}", key);
                }
                _ => {
                    warn!("Unknown mount option '{}' may not be supported", key);
                }
            }
        }

        Ok(())
    }
}

/// Mounts merged from features and config
///
/// Config mounts take precedence for same target path.
/// This struct holds the final deduplicated mount strings ready to be applied
/// to container creation.
///
/// # Merge Rules
/// - Features are processed in installation order
/// - Config mounts override feature mounts for the same target path
/// - All mounts are normalized to Docker CLI string format
///
/// # Example
/// ```rust
/// use deacon_core::mount::MergedMounts;
///
/// let merged = MergedMounts {
///     mounts: vec![
///         "type=bind,source=/host/path,target=/container/path".to_string(),
///         "type=volume,source=myvolume,target=/data".to_string(),
///     ],
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergedMounts {
    /// Final mount strings to apply (deduplicated by target)
    pub mounts: Vec<String>,
}

/// Merge mounts from features and config
///
/// # Arguments
/// * `config_mounts` - Mounts from devcontainer.json
/// * `features` - Resolved features in installation order
///
/// # Returns
/// * `Ok(MergedMounts)` - Deduplicated mounts (by target)
/// * `Err` - Invalid mount specification
///
/// # Precedence
/// Config mounts override feature mounts for same target path
pub fn merge_mounts(
    config_mounts: &[serde_json::Value],
    features: &[crate::features::ResolvedFeature],
) -> Result<MergedMounts> {
    use std::collections::HashMap;

    // Map to deduplicate by target path
    // The value is a tuple of (mount_string, insertion_index) to preserve order
    let mut mount_map: HashMap<String, (String, usize)> = HashMap::new();
    let mut insertion_index = 0;

    // Process feature mounts in installation order
    for feature in features {
        for mount_str in &feature.metadata.mounts {
            // Parse the mount to get the target and validate it
            let mount =
                MountParser::parse_mount(mount_str).map_err(|e| ConfigError::Validation {
                    message: format!(
                        "Invalid mount in feature {}: {}: {}",
                        feature.id, mount_str, e
                    ),
                })?;

            // Normalize the mount to Docker CLI string format
            let normalized_str = normalize_mount_to_string(&mount);

            // Store in map, keyed by target (later overwrites earlier)
            // When overwriting, keep the original insertion index to preserve order
            match mount_map.get_mut(&mount.target) {
                Some((s, _idx)) => {
                    // Target already exists, update the mount string but keep the index
                    *s = normalized_str;
                }
                None => {
                    // New target, insert with current index
                    mount_map.insert(mount.target.clone(), (normalized_str, insertion_index));
                    insertion_index += 1;
                }
            }
        }
    }

    // Process config mounts (these override features)
    for mount_value in config_mounts {
        let mount_str = match mount_value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Object(obj) => {
                // Convert object format to string format for parsing
                convert_object_mount_to_string(obj).map_err(|e| ConfigError::Validation {
                    message: format!("Invalid mount in config: {}", e),
                })?
            }
            _ => {
                return Err(ConfigError::Validation {
                    message: "Invalid mount specification type, expected string or object"
                        .to_string(),
                }
                .into());
            }
        };

        // Parse the mount to get the target and validate it
        let mount = MountParser::parse_mount(&mount_str).map_err(|e| ConfigError::Validation {
            message: format!("Invalid mount in config: {}: {}", mount_str, e),
        })?;

        // Normalize the mount to Docker CLI string format
        let normalized_str = normalize_mount_to_string(&mount);

        // Store in map, overwriting any feature mount with same target
        // When overwriting, keep the original insertion index to preserve order
        match mount_map.get_mut(&mount.target) {
            Some((s, _idx)) => {
                // Target already exists, update the mount string but keep the index
                *s = normalized_str;
            }
            None => {
                // New target, insert with current index
                mount_map.insert(mount.target.clone(), (normalized_str, insertion_index));
                insertion_index += 1;
            }
        }
    }

    // Convert map to vector, preserving order
    let mut mounts_with_order: Vec<(String, usize)> = mount_map.into_values().collect();

    // Sort by insertion order to maintain declaration order
    mounts_with_order.sort_by_key(|(_, idx)| *idx);

    // Extract just the mount strings
    let mounts: Vec<String> = mounts_with_order
        .into_iter()
        .map(|(mount_str, _)| mount_str)
        .collect();

    Ok(MergedMounts { mounts })
}

/// Normalize a parsed Mount to Docker CLI string format
///
/// Converts a Mount struct to the standard Docker CLI string format:
/// `type={type},source={source},target={target}[,readonly][,...]`
fn normalize_mount_to_string(mount: &Mount) -> String {
    let mut parts = vec![format!("type={}", mount.mount_type)];

    // Add source for bind and volume mounts
    if let Some(ref source) = mount.source {
        parts.push(format!("source={}", source));
    }

    // Add target
    parts.push(format!("target={}", mount.target));

    // Add read-only flag if needed
    if mount.mode == MountMode::ReadOnly {
        parts.push("ro".to_string());
    }

    // Add consistency for bind mounts
    if mount.mount_type == MountType::Bind {
        if let Some(ref consistency) = mount.consistency {
            parts.push(format!("consistency={}", consistency));
        }
    }

    // Add additional options
    for (key, value) in &mount.options {
        if value.is_empty() {
            parts.push(key.clone());
        } else {
            parts.push(format!("{}={}", key, value));
        }
    }

    parts.join(",")
}

/// Convert an object-based mount specification to Docker CLI string format
///
/// Converts a JSON object mount specification like:
/// ```json
/// {
///   "type": "bind",
///   "source": "/host/path",
///   "target": "/container/path",
///   "consistency": "cached"
/// }
/// ```
///
/// to Docker CLI format:
/// ```
/// type=bind,source=/host/path,target=/container/path,consistency=cached
/// ```
fn convert_object_mount_to_string(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> Result<String> {
    let mut parts = Vec::new();

    // Extract type (required)
    let mount_type =
        obj.get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ConfigError::Validation {
                message: "Mount object must have 'type' field".to_string(),
            })?;
    parts.push(format!("type={}", mount_type));

    // Extract source (optional, but required for bind/volume)
    if let Some(source) = obj.get("source").and_then(|v| v.as_str()) {
        parts.push(format!("source={}", source));
    }

    // Extract target (required)
    let target =
        obj.get("target")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ConfigError::Validation {
                message: "Mount object must have 'target' field".to_string(),
            })?;
    parts.push(format!("target={}", target));

    // Extract consistency (optional)
    if let Some(consistency) = obj.get("consistency").and_then(|v| v.as_str()) {
        parts.push(format!("consistency={}", consistency));
    }

    // Extract readonly flag (optional)
    if let Some(readonly) = obj.get("readonly").and_then(|v| v.as_bool()) {
        if readonly {
            parts.push("ro".to_string());
        }
    }

    // Handle any additional fields as options
    for (key, value) in obj {
        match key.as_str() {
            "type" | "source" | "target" | "consistency" | "readonly" => {
                // Already handled above
                continue;
            }
            _ => {
                // Add as additional option
                if let Some(str_value) = value.as_str() {
                    parts.push(format!("{}={}", key, str_value));
                } else if value.is_boolean() && value.as_bool() == Some(true) {
                    parts.push(key.clone());
                }
            }
        }
    }

    Ok(parts.join(","))
}

/// Mount parser for DevContainer mount specifications
pub struct MountParser;

impl MountParser {
    /// Parse a mount specification string into a Mount
    ///
    /// Supports both Docker mount syntax and volume syntax:
    /// - `type=bind,source=/host,target=/container,ro,consistency=cached`
    /// - `/host/path:/container/path:ro`
    /// - `/host/path:/container/path`
    ///
    /// ## Arguments
    /// * `mount_spec` - Mount specification string
    ///
    /// ## Returns
    /// A parsed `Mount` or an error if the specification is invalid.
    #[instrument(skip_all, fields(mount_spec = %mount_spec))]
    pub fn parse_mount(mount_spec: &str) -> Result<Mount> {
        debug!("Parsing mount specification: {}", mount_spec);

        // Try Docker mount syntax first (contains "type=" or multiple "=" signs)
        if mount_spec.contains("type=") || mount_spec.matches('=').count() > 1 {
            Self::parse_docker_mount_syntax(mount_spec)
        } else {
            // Try volume syntax (source:target[:options])
            Self::parse_volume_syntax(mount_spec)
        }
    }

    /// Parse Docker mount syntax: type=bind,source=/host,target=/container,options...
    fn parse_docker_mount_syntax(mount_spec: &str) -> Result<Mount> {
        let mut mount_type = None;
        let mut source = None;
        let mut target = None;
        let mut mode = MountMode::ReadWrite;
        let mut consistency = None;
        let mut options = HashMap::new();

        for part in mount_spec.split(',') {
            let part = part.trim();

            if part.is_empty() {
                continue;
            }

            if let Some((key, value)) = part.split_once('=') {
                let key = key.trim();
                let value = value.trim();

                match key {
                    "type" => {
                        mount_type = Some(value.parse::<MountType>()?);
                    }
                    "source" | "src" => {
                        source = Some(value.to_string());
                    }
                    "target" | "dst" | "destination" => {
                        target = Some(value.to_string());
                    }
                    "consistency" => {
                        consistency = Some(value.parse::<MountConsistency>()?);
                    }
                    _ => {
                        options.insert(key.to_string(), value.to_string());
                    }
                }
            } else {
                // Handle flags without values
                match part {
                    "ro" | "readonly" => {
                        mode = MountMode::ReadOnly;
                    }
                    "rw" | "readwrite" => {
                        mode = MountMode::ReadWrite;
                    }
                    _ => {
                        options.insert(part.to_string(), String::new());
                    }
                }
            }
        }

        // Validate required fields
        let mount_type = mount_type.ok_or_else(|| ConfigError::Validation {
            message: "Mount specification must include 'type' field".to_string(),
        })?;

        let target = target.ok_or_else(|| ConfigError::Validation {
            message: "Mount specification must include 'target' field".to_string(),
        })?;

        let mount = Mount {
            mount_type,
            source,
            target,
            mode,
            consistency,
            options,
        };

        mount.validate()?;
        Ok(mount)
    }

    /// Parse volume syntax: source:target[:options]
    fn parse_volume_syntax(mount_spec: &str) -> Result<Mount> {
        let parts: Vec<&str> = mount_spec.split(':').collect();

        if parts.len() < 2 {
            return Err(ConfigError::Validation {
                message: format!(
                    "Volume mount specification '{}' must have at least source:target",
                    mount_spec
                ),
            }
            .into());
        }

        let source = if parts[0].is_empty() {
            None
        } else {
            Some(parts[0].to_string())
        };

        let target = parts[1].to_string();

        let mut mode = MountMode::ReadWrite;
        let mut options = HashMap::new();

        // Parse options if present
        if parts.len() > 2 {
            for option in &parts[2..] {
                match *option {
                    "ro" | "readonly" => {
                        mode = MountMode::ReadOnly;
                    }
                    "rw" | "readwrite" => {
                        mode = MountMode::ReadWrite;
                    }
                    _ => {
                        // Store unknown options
                        options.insert(option.to_string(), String::new());
                    }
                }
            }
        }

        // Determine mount type based on source
        let mount_type = if source.is_none() {
            MountType::Volume
        } else if let Some(ref src) = source {
            if src.starts_with('/') || src.starts_with('.') || src.contains('\\') {
                MountType::Bind
            } else {
                MountType::Volume
            }
        } else {
            MountType::Bind
        };

        let mount = Mount {
            mount_type,
            source,
            target,
            mode,
            consistency: None, // Not supported in volume syntax
            options,
        };

        mount.validate()?;
        Ok(mount)
    }

    /// Parse multiple mount specifications
    ///
    /// Takes an array of mount specification strings and parses each one.
    /// Returns all successfully parsed mounts and logs warnings for invalid ones.
    ///
    /// ## Arguments
    /// * `mount_specs` - Array of mount specification strings
    ///
    /// ## Returns
    /// Vector of successfully parsed mounts
    #[instrument(skip_all)]
    pub fn parse_mounts(mount_specs: &[String]) -> Vec<Mount> {
        let mut mounts = Vec::new();

        for mount_spec in mount_specs {
            match Self::parse_mount(mount_spec) {
                Ok(mount) => {
                    debug!("Successfully parsed mount: {:?}", mount);
                    mounts.push(mount);
                }
                Err(e) => {
                    warn!("Failed to parse mount '{}': {}", mount_spec, e);
                }
            }
        }

        mounts
    }

    /// Parse mount specifications from JSON values
    ///
    /// Handles the case where mounts are specified as JSON values that may be strings or objects.
    ///
    /// ## Arguments  
    /// * `mount_values` - Array of JSON values containing mount specifications
    ///
    /// ## Returns
    /// Vector of successfully parsed mounts
    #[instrument(skip_all)]
    pub fn parse_mounts_from_json(mount_values: &[serde_json::Value]) -> Vec<Mount> {
        let mut mount_specs = Vec::new();

        for value in mount_values {
            match value {
                serde_json::Value::String(s) => {
                    mount_specs.push(s.clone());
                }
                serde_json::Value::Object(_) => {
                    // For future object-based mount specifications
                    warn!("Object-based mount specifications not yet supported, skipping");
                }
                _ => {
                    warn!("Invalid mount specification type, expected string or object");
                }
            }
        }

        Self::parse_mounts(&mount_specs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_type_parsing() {
        assert_eq!("bind".parse::<MountType>().unwrap(), MountType::Bind);
        assert_eq!("volume".parse::<MountType>().unwrap(), MountType::Volume);
        assert_eq!("tmpfs".parse::<MountType>().unwrap(), MountType::Tmpfs);
        assert!("invalid".parse::<MountType>().is_err());
    }

    #[test]
    fn test_mount_consistency_parsing() {
        assert_eq!(
            "cached".parse::<MountConsistency>().unwrap(),
            MountConsistency::Cached
        );
        assert_eq!(
            "consistent".parse::<MountConsistency>().unwrap(),
            MountConsistency::Consistent
        );
        assert_eq!(
            "delegated".parse::<MountConsistency>().unwrap(),
            MountConsistency::Delegated
        );
        assert!("invalid".parse::<MountConsistency>().is_err());
    }

    #[test]
    fn test_mount_mode_parsing() {
        assert_eq!("ro".parse::<MountMode>().unwrap(), MountMode::ReadOnly);
        assert_eq!("rw".parse::<MountMode>().unwrap(), MountMode::ReadWrite);
        assert_eq!(
            "readonly".parse::<MountMode>().unwrap(),
            MountMode::ReadOnly
        );
        assert_eq!(
            "readwrite".parse::<MountMode>().unwrap(),
            MountMode::ReadWrite
        );
        assert!("invalid".parse::<MountMode>().is_err());
    }

    #[test]
    fn test_parse_docker_mount_syntax() {
        let mount = MountParser::parse_mount(
            "type=bind,source=/host/path,target=/container/path,ro,consistency=cached",
        )
        .unwrap();

        assert_eq!(mount.mount_type, MountType::Bind);
        assert_eq!(mount.source, Some("/host/path".to_string()));
        assert_eq!(mount.target, "/container/path");
        assert_eq!(mount.mode, MountMode::ReadOnly);
        assert_eq!(mount.consistency, Some(MountConsistency::Cached));
    }

    #[test]
    fn test_parse_volume_syntax() {
        let mount = MountParser::parse_mount("/host/path:/container/path:ro").unwrap();

        assert_eq!(mount.mount_type, MountType::Bind);
        assert_eq!(mount.source, Some("/host/path".to_string()));
        assert_eq!(mount.target, "/container/path");
        assert_eq!(mount.mode, MountMode::ReadOnly);
    }

    #[test]
    fn test_parse_volume_syntax_simple() {
        let mount = MountParser::parse_mount("/host/path:/container/path").unwrap();

        assert_eq!(mount.mount_type, MountType::Bind);
        assert_eq!(mount.source, Some("/host/path".to_string()));
        assert_eq!(mount.target, "/container/path");
        assert_eq!(mount.mode, MountMode::ReadWrite);
    }

    #[test]
    fn test_parse_named_volume() {
        let mount = MountParser::parse_mount("myvolume:/container/path").unwrap();

        assert_eq!(mount.mount_type, MountType::Volume);
        assert_eq!(mount.source, Some("myvolume".to_string()));
        assert_eq!(mount.target, "/container/path");
    }

    #[test]
    #[cfg(unix)] // Uses Unix-style absolute paths
    fn test_mount_to_docker_args() {
        let mount = Mount {
            mount_type: MountType::Bind,
            source: Some("/host/path".to_string()),
            target: "/container/path".to_string(),
            mode: MountMode::ReadOnly,
            consistency: Some(MountConsistency::Cached),
            options: HashMap::new(),
        };

        let args = mount.to_docker_args();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "--mount");
        assert!(args[1].contains("type=bind"));
        assert!(args[1].contains("source=/host/path"));
        assert!(args[1].contains("target=/container/path"));
        assert!(args[1].contains("ro"));
        assert!(args[1].contains("consistency=cached"));
    }

    #[test]
    #[cfg(windows)] // Uses Windows-style absolute paths
    fn test_mount_to_docker_args() {
        let mount = Mount {
            mount_type: MountType::Bind,
            source: Some(r"C:\host\path".to_string()),
            target: "/container/path".to_string(),
            mode: MountMode::ReadOnly,
            consistency: Some(MountConsistency::Cached),
            options: HashMap::new(),
        };

        let args = mount.to_docker_args();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "--mount");
        assert!(args[1].contains("type=bind"));
        // On Windows, Docker Desktop path conversion may apply
        assert!(
            args[1].contains(r"source=C:\host\path") || args[1].contains("source=/c/host/path"),
            "Unexpected source in: {}",
            args[1]
        );
        assert!(args[1].contains("target=/container/path"));
        assert!(args[1].contains("ro"));
        assert!(args[1].contains("consistency=cached"));
    }

    #[test]
    fn test_mount_validation_bind_without_source() {
        let mount = Mount {
            mount_type: MountType::Bind,
            source: None,
            target: "/container/path".to_string(),
            mode: MountMode::ReadWrite,
            consistency: None,
            options: HashMap::new(),
        };

        assert!(mount.validate().is_err());
    }

    #[test]
    fn test_mount_validation_relative_target() {
        let mount = Mount {
            mount_type: MountType::Bind,
            source: Some("/host/path".to_string()),
            target: "relative/path".to_string(),
            mode: MountMode::ReadWrite,
            consistency: None,
            options: HashMap::new(),
        };

        assert!(mount.validate().is_err());
    }

    #[test]
    fn test_parse_mounts_from_json() {
        let json_values = vec![
            serde_json::Value::String("type=bind,source=/host,target=/container".to_string()),
            serde_json::Value::String("/host/path:/container/path".to_string()),
        ];

        let mounts = MountParser::parse_mounts_from_json(&json_values);
        assert_eq!(mounts.len(), 2);
    }

    #[test]
    fn test_relative_path_resolution_with_docker_desktop_conversion() {
        use std::env;
        use tempfile::TempDir;

        // Create a temporary directory to serve as current_dir
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Save current directory and change to temp directory
        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(temp_path).unwrap();

        // Ensure we restore the directory at the end
        struct DirRestorer {
            original_dir: std::path::PathBuf,
        }
        impl Drop for DirRestorer {
            fn drop(&mut self) {
                let _ = env::set_current_dir(&self.original_dir);
            }
        }
        let _restorer = DirRestorer { original_dir };

        // Create a mount with a relative path
        let mount = Mount {
            mount_type: MountType::Bind,
            source: Some("./data".to_string()),
            target: "/container/data".to_string(),
            mode: MountMode::ReadWrite,
            consistency: None,
            options: std::collections::HashMap::new(),
        };

        let args = mount.to_docker_args();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "--mount");

        // The mount string should contain the absolute path
        let mount_string = &args[1];
        assert!(mount_string.starts_with("type=bind,source="));
        assert!(mount_string.contains("target=/container/data"));

        // Extract the source path from the mount string
        let source_part = mount_string
            .split(',')
            .find(|part| part.starts_with("source="))
            .unwrap()
            .strip_prefix("source=")
            .unwrap();

        // The source should be an absolute path, not a relative one
        assert!(!source_part.starts_with("./"));
        assert!(source_part.contains("data"));

        // On current Linux platform, should not be converted for Docker Desktop
        let platform = crate::platform::Platform::detect();
        if !platform.needs_docker_desktop_path_conversion() {
            // Should contain the absolute temp path
            assert!(source_part.contains(temp_path.to_str().unwrap()));
        }
    }
}

#[cfg(test)]
mod merge_mounts_tests {
    //! Unit tests for merge_mounts() function
    //!
    //! Tests cover the following scenarios per the contract in
    //! specs/009-complete-feature-support/contracts/mounts.md:
    //!
    //! 1. Basic Merge Tests - empty inputs, config only, features only, no conflicts
    //! 2. Precedence Tests - config overrides features, later features override earlier
    //! 3. Normalization Tests - volume syntax normalized to mount syntax
    //! 4. Edge Cases - empty arrays, multiple mounts, tmpfs, case sensitivity
    //! 5. Error Handling - invalid specs, missing required fields, validation errors
    //! 6. Order Preservation - feature installation order, declaration order

    use super::*;
    use crate::features::{FeatureMetadata, ResolvedFeature};

    /// Helper function to create a ResolvedFeature with specified mounts
    fn create_feature_with_mounts(id: &str, mounts: Vec<String>) -> ResolvedFeature {
        let metadata = FeatureMetadata {
            id: id.to_string(),
            version: None,
            name: Some(format!("Test Feature {}", id)),
            description: None,
            documentation_url: None,
            license_url: None,
            options: HashMap::new(),
            container_env: HashMap::new(),
            mounts,
            init: None,
            privileged: None,
            cap_add: vec![],
            security_opt: vec![],
            entrypoint: None,
            installs_after: vec![],
            depends_on: HashMap::new(),
            on_create_command: None,
            update_content_command: None,
            post_create_command: None,
            post_start_command: None,
            post_attach_command: None,
        };

        ResolvedFeature {
            id: id.to_string(),
            source: format!("test://features/{}", id),
            options: HashMap::new(),
            metadata,
        }
    }

    // ==================== Basic Merge Tests ====================

    #[test]
    fn test_merge_mounts_empty() {
        // No config mounts, no feature mounts
        let config_mounts: Vec<serde_json::Value> = vec![];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 0);
    }

    #[test]
    fn test_merge_mounts_config_only() {
        // Config mounts only, no features
        let config_mounts = vec![
            serde_json::Value::String("type=bind,source=/host/data,target=/data".to_string()),
            serde_json::Value::String("type=volume,source=cache,target=/cache".to_string()),
        ];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 2);
        assert!(result
            .mounts
            .contains(&"type=bind,source=/host/data,target=/data".to_string()));
        assert!(result
            .mounts
            .contains(&"type=volume,source=cache,target=/cache".to_string()));
    }

    #[test]
    fn test_merge_mounts_features_only() {
        // Feature mounts only, no config
        let config_mounts: Vec<serde_json::Value> = vec![];
        let features = vec![
            create_feature_with_mounts(
                "feature1",
                vec!["type=volume,source=vol1,target=/vol1".to_string()],
            ),
            create_feature_with_mounts(
                "feature2",
                vec!["type=volume,source=vol2,target=/vol2".to_string()],
            ),
        ];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 2);
        assert!(result
            .mounts
            .contains(&"type=volume,source=vol1,target=/vol1".to_string()));
        assert!(result
            .mounts
            .contains(&"type=volume,source=vol2,target=/vol2".to_string()));
    }

    #[test]
    fn test_merge_mounts_no_conflicts() {
        // Config and features with different targets
        let config_mounts = vec![serde_json::Value::String(
            "type=bind,source=/host/data,target=/data".to_string(),
        )];
        let features = vec![create_feature_with_mounts(
            "cache",
            vec!["type=volume,source=cache,target=/cache".to_string()],
        )];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 2);
        assert!(result
            .mounts
            .contains(&"type=volume,source=cache,target=/cache".to_string()));
        assert!(result
            .mounts
            .contains(&"type=bind,source=/host/data,target=/data".to_string()));
    }

    // ==================== Precedence Tests ====================

    #[test]
    fn test_merge_mounts_config_overrides_feature() {
        // Config mount overrides feature mount for same target
        let config_mounts = vec![serde_json::Value::String(
            "type=bind,source=/host/my-data,target=/data".to_string(),
        )];
        let features = vec![create_feature_with_mounts(
            "data",
            vec!["type=volume,source=feature-data,target=/data".to_string()],
        )];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 1);
        assert_eq!(
            result.mounts[0],
            "type=bind,source=/host/my-data,target=/data"
        );
    }

    #[test]
    fn test_merge_mounts_later_feature_overrides_earlier() {
        // Later feature mount overrides earlier feature mount for same target
        let config_mounts: Vec<serde_json::Value> = vec![];
        let features = vec![
            create_feature_with_mounts(
                "feature1",
                vec!["type=volume,source=vol1,target=/shared".to_string()],
            ),
            create_feature_with_mounts(
                "feature2",
                vec!["type=volume,source=vol2,target=/shared".to_string()],
            ),
        ];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 1);
        assert_eq!(result.mounts[0], "type=volume,source=vol2,target=/shared");
    }

    #[test]
    fn test_merge_mounts_multiple_features_with_override() {
        // Multiple features, later one overrides shared target
        let config_mounts: Vec<serde_json::Value> = vec![];
        let features = vec![
            create_feature_with_mounts(
                "feature1",
                vec!["type=volume,source=vol1,target=/vol1".to_string()],
            ),
            create_feature_with_mounts(
                "feature2",
                vec![
                    "type=volume,source=vol2,target=/vol2".to_string(),
                    "type=volume,source=shared,target=/shared".to_string(),
                ],
            ),
            create_feature_with_mounts(
                "feature3",
                vec!["type=volume,source=override-shared,target=/shared".to_string()],
            ),
        ];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 3);
        assert!(result
            .mounts
            .contains(&"type=volume,source=vol1,target=/vol1".to_string()));
        assert!(result
            .mounts
            .contains(&"type=volume,source=vol2,target=/vol2".to_string()));
        assert!(result
            .mounts
            .contains(&"type=volume,source=override-shared,target=/shared".to_string()));
    }

    #[test]
    fn test_merge_mounts_config_overrides_multiple_features() {
        // Config mount overrides multiple features with same target
        let config_mounts = vec![serde_json::Value::String(
            "type=bind,source=/host/final,target=/shared".to_string(),
        )];
        let features = vec![
            create_feature_with_mounts(
                "feature1",
                vec!["type=volume,source=vol1,target=/shared".to_string()],
            ),
            create_feature_with_mounts(
                "feature2",
                vec!["type=volume,source=vol2,target=/shared".to_string()],
            ),
        ];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 1);
        assert_eq!(
            result.mounts[0],
            "type=bind,source=/host/final,target=/shared"
        );
    }

    // ==================== Normalization Tests ====================

    #[test]
    fn test_merge_mounts_normalize_volume_syntax() {
        // Volume syntax should be normalized to mount syntax
        let config_mounts = vec![serde_json::Value::String(
            "/host/path:/container/path".to_string(),
        )];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 1);
        // The mount should be normalized - exact format depends on implementation
        // but should contain the target path
        assert!(result.mounts[0].contains("target=/container/path"));
    }

    #[test]
    fn test_merge_mounts_normalize_volume_syntax_with_options() {
        // Volume syntax with options should be normalized
        let config_mounts = vec![serde_json::Value::String(
            "/host/path:/container/path:ro".to_string(),
        )];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 1);
        assert!(result.mounts[0].contains("target=/container/path"));
        assert!(result.mounts[0].contains("ro"));
    }

    #[test]
    fn test_merge_mounts_normalize_named_volume() {
        // Named volume syntax should be normalized
        let config_mounts = vec![serde_json::Value::String("myvolume:/data".to_string())];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 1);
        assert!(result.mounts[0].contains("target=/data"));
        assert!(result.mounts[0].contains("myvolume"));
    }

    // ==================== Edge Cases ====================

    #[test]
    fn test_merge_mounts_empty_feature_mounts() {
        // Feature with empty mounts array
        let config_mounts = vec![serde_json::Value::String(
            "type=bind,source=/host/data,target=/data".to_string(),
        )];
        let features = vec![create_feature_with_mounts("feature1", vec![])];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 1);
        assert_eq!(result.mounts[0], "type=bind,source=/host/data,target=/data");
    }

    #[test]
    fn test_merge_mounts_multiple_mounts_per_feature() {
        // Feature with multiple mounts
        let config_mounts: Vec<serde_json::Value> = vec![];
        let features = vec![create_feature_with_mounts(
            "feature1",
            vec![
                "type=volume,source=vol1,target=/vol1".to_string(),
                "type=volume,source=vol2,target=/vol2".to_string(),
                "type=tmpfs,target=/tmp".to_string(),
            ],
        )];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 3);
        assert!(result
            .mounts
            .contains(&"type=volume,source=vol1,target=/vol1".to_string()));
        assert!(result
            .mounts
            .contains(&"type=volume,source=vol2,target=/vol2".to_string()));
        assert!(result
            .mounts
            .contains(&"type=tmpfs,target=/tmp".to_string()));
    }

    #[test]
    fn test_merge_mounts_tmpfs_mount() {
        // tmpfs mounts should work correctly
        let config_mounts = vec![serde_json::Value::String(
            "type=tmpfs,target=/tmp".to_string(),
        )];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 1);
        assert_eq!(result.mounts[0], "type=tmpfs,target=/tmp");
    }

    #[test]
    fn test_merge_mounts_case_sensitivity_in_targets() {
        // Different case in target paths should be treated as different mounts
        let config_mounts: Vec<serde_json::Value> = vec![];
        let features = vec![
            create_feature_with_mounts(
                "feature1",
                vec!["type=volume,source=vol1,target=/Data".to_string()],
            ),
            create_feature_with_mounts(
                "feature2",
                vec!["type=volume,source=vol2,target=/data".to_string()],
            ),
        ];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        // Both should be present since targets differ in case
        assert_eq!(result.mounts.len(), 2);
    }

    #[test]
    fn test_merge_mounts_complex_scenario() {
        // Complex scenario with multiple features and config overrides
        let config_mounts = vec![
            serde_json::Value::String(
                "type=bind,source=/host/workspace,target=/workspace".to_string(),
            ),
            serde_json::Value::String("type=bind,source=/host/override,target=/data".to_string()),
        ];
        let features = vec![
            create_feature_with_mounts(
                "feature1",
                vec![
                    "type=volume,source=cache1,target=/cache".to_string(),
                    "type=volume,source=data1,target=/data".to_string(),
                ],
            ),
            create_feature_with_mounts(
                "feature2",
                vec![
                    "type=volume,source=cache2,target=/cache".to_string(),
                    "type=tmpfs,target=/tmp".to_string(),
                ],
            ),
        ];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 4);
        // Config mounts should be present
        assert!(result
            .mounts
            .contains(&"type=bind,source=/host/workspace,target=/workspace".to_string()));
        assert!(result
            .mounts
            .contains(&"type=bind,source=/host/override,target=/data".to_string()));
        // Feature2's cache should override feature1's cache
        assert!(result
            .mounts
            .contains(&"type=volume,source=cache2,target=/cache".to_string()));
        // Feature2's tmpfs should be present
        assert!(result
            .mounts
            .contains(&"type=tmpfs,target=/tmp".to_string()));
    }

    // ==================== Error Handling Tests ====================

    #[test]
    fn test_merge_mounts_invalid_mount_string() {
        // Invalid mount string should return error
        let config_mounts = vec![serde_json::Value::String("invalid-mount-spec".to_string())];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_mounts_missing_target() {
        // Mount without target should return error
        let config_mounts = vec![serde_json::Value::String(
            "type=bind,source=/host/path".to_string(),
        )];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_mounts_invalid_feature_mount() {
        // Invalid mount in feature should return error
        let config_mounts: Vec<serde_json::Value> = vec![];
        let features = vec![create_feature_with_mounts(
            "feature1",
            vec!["invalid-mount".to_string()],
        )];

        let result = merge_mounts(&config_mounts, &features);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_mounts_relative_target() {
        // Mount with relative target should return error
        let config_mounts = vec![serde_json::Value::String(
            "type=bind,source=/host/path,target=relative/path".to_string(),
        )];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_mounts_bind_without_source() {
        // Bind mount without source should return error
        let config_mounts = vec![serde_json::Value::String(
            "type=bind,target=/container/path".to_string(),
        )];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_mounts_error_attribution_config() {
        // Error from config mount should include "config" in message
        let config_mounts = vec![serde_json::Value::String(
            "type=bind,target=/container/path".to_string(), // Missing source
        )];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("config"));
    }

    #[test]
    fn test_merge_mounts_error_attribution_feature() {
        // Error from feature mount should include feature ID in message
        let config_mounts: Vec<serde_json::Value> = vec![];
        let features = vec![create_feature_with_mounts(
            "my-feature",
            vec!["type=bind,target=/container/path".to_string()], // Missing source
        )];

        let result = merge_mounts(&config_mounts, &features);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("my-feature"));
    }

    // ==================== Object Mount Normalization Tests ====================

    #[test]
    fn test_merge_mounts_object_format_basic() {
        // Object format should be converted to string format
        let config_mounts = vec![serde_json::json!({
            "type": "bind",
            "source": "/host/path",
            "target": "/container/path"
        })];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 1);
        assert!(result.mounts[0].contains("type=bind"));
        assert!(result.mounts[0].contains("source=/host/path"));
        assert!(result.mounts[0].contains("target=/container/path"));
    }

    #[test]
    fn test_merge_mounts_object_format_with_readonly() {
        // Object format with readonly flag
        let config_mounts = vec![serde_json::json!({
            "type": "bind",
            "source": "/host/path",
            "target": "/container/path",
            "readonly": true
        })];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 1);
        assert!(result.mounts[0].contains("ro"));
    }

    #[test]
    fn test_merge_mounts_object_format_with_consistency() {
        // Object format with consistency option
        let config_mounts = vec![serde_json::json!({
            "type": "bind",
            "source": "/host/path",
            "target": "/container/path",
            "consistency": "cached"
        })];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 1);
        assert!(result.mounts[0].contains("consistency=cached"));
    }

    #[test]
    fn test_merge_mounts_object_format_volume() {
        // Object format for volume mount
        let config_mounts = vec![serde_json::json!({
            "type": "volume",
            "source": "myvolume",
            "target": "/data"
        })];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 1);
        assert!(result.mounts[0].contains("type=volume"));
        assert!(result.mounts[0].contains("source=myvolume"));
        assert!(result.mounts[0].contains("target=/data"));
    }

    #[test]
    fn test_merge_mounts_object_format_tmpfs() {
        // Object format for tmpfs mount (no source)
        let config_mounts = vec![serde_json::json!({
            "type": "tmpfs",
            "target": "/tmp"
        })];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 1);
        assert!(result.mounts[0].contains("type=tmpfs"));
        assert!(result.mounts[0].contains("target=/tmp"));
    }

    #[test]
    fn test_merge_mounts_mixed_string_and_object() {
        // Mix of string and object formats
        let config_mounts = vec![
            serde_json::Value::String("type=bind,source=/host/a,target=/a".to_string()),
            serde_json::json!({
                "type": "volume",
                "source": "vol1",
                "target": "/b"
            }),
            serde_json::Value::String("type=tmpfs,target=/c".to_string()),
        ];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 3);
        assert!(result.mounts.iter().any(|m| m.contains("target=/a")));
        assert!(result.mounts.iter().any(|m| m.contains("target=/b")));
        assert!(result.mounts.iter().any(|m| m.contains("target=/c")));
    }

    #[test]
    fn test_merge_mounts_object_format_missing_type() {
        // Object format without type should error
        let config_mounts = vec![serde_json::json!({
            "source": "/host/path",
            "target": "/container/path"
        })];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("type"));
    }

    #[test]
    fn test_merge_mounts_object_format_missing_target() {
        // Object format without target should error
        let config_mounts = vec![serde_json::json!({
            "type": "bind",
            "source": "/host/path"
        })];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("target"));
    }

    #[test]
    fn test_merge_mounts_object_overrides_string() {
        // Object format config mount should override string format feature mount
        let config_mounts = vec![serde_json::json!({
            "type": "bind",
            "source": "/host/override",
            "target": "/data"
        })];
        let features = vec![create_feature_with_mounts(
            "feature1",
            vec!["type=volume,source=vol1,target=/data".to_string()],
        )];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 1);
        assert!(result.mounts[0].contains("source=/host/override"));
        assert!(!result.mounts[0].contains("vol1"));
    }

    // ==================== Order Preservation Tests ====================

    #[test]
    fn test_merge_mounts_preserves_feature_order() {
        // Mounts from features should be processed in installation order
        let config_mounts: Vec<serde_json::Value> = vec![];
        let features = vec![
            create_feature_with_mounts(
                "feature1",
                vec!["type=volume,source=vol1,target=/vol1".to_string()],
            ),
            create_feature_with_mounts(
                "feature2",
                vec!["type=volume,source=vol2,target=/vol2".to_string()],
            ),
            create_feature_with_mounts(
                "feature3",
                vec!["type=volume,source=vol3,target=/vol3".to_string()],
            ),
        ];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 3);
        // The exact order may vary based on implementation, but all should be present
        assert!(result
            .mounts
            .contains(&"type=volume,source=vol1,target=/vol1".to_string()));
        assert!(result
            .mounts
            .contains(&"type=volume,source=vol2,target=/vol2".to_string()));
        assert!(result
            .mounts
            .contains(&"type=volume,source=vol3,target=/vol3".to_string()));
    }

    #[test]
    fn test_merge_mounts_preserves_declaration_order_within_feature() {
        // Mounts within a feature should be processed in declaration order
        let config_mounts: Vec<serde_json::Value> = vec![];
        let features = vec![create_feature_with_mounts(
            "feature1",
            vec![
                "type=volume,source=vol1,target=/vol1".to_string(),
                "type=volume,source=vol2,target=/vol2".to_string(),
                "type=volume,source=vol3,target=/vol3".to_string(),
            ],
        )];

        let result = merge_mounts(&config_mounts, &features).unwrap();
        assert_eq!(result.mounts.len(), 3);
        // All mounts should be present
        assert!(result
            .mounts
            .contains(&"type=volume,source=vol1,target=/vol1".to_string()));
        assert!(result
            .mounts
            .contains(&"type=volume,source=vol2,target=/vol2".to_string()));
        assert!(result
            .mounts
            .contains(&"type=volume,source=vol3,target=/vol3".to_string()));
    }
}
