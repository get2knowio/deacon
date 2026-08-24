//! Mount parsing and validation
//!
//! This module handles parsing of DevContainer mount specifications into structured types
//! that can be converted to Docker CLI mount arguments. It supports the following mount
//! formats and types:
//!
//! ## Mount Types
//! - `bind`: Bind mount from host filesystem (always requires a `source`)
//! - `volume`: Docker volume — named when a `source` is given, ANONYMOUS when it is
//!   omitted, exactly as Docker's own `--mount` flag defines it (#617)
//! - `tmpfs`: Temporary filesystem in memory (never has a `source`)
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

/// Format one `key=value` field of a `--mount` argument, quoting it when the value
/// contains a comma.
///
/// A `--mount` argument is CSV, so a value holding a comma has to be quoted or Docker reads
/// the text after the comma as a further field (`invalid field 'ma' must be a key=value
/// pair`). The reference CLI quotes the whole `key=value` — not just the value — when and
/// only when the value contains a comma (`spec-node/utils.ts:412-417`); matching that keeps
/// every comma-free mount string byte-identical to what deacon emitted before (#663).
pub fn format_mount_field(key: &str, value: &str) -> String {
    if value.contains(',') {
        format!("\"{}={}\"", key, value)
    } else {
        format!("{}={}", key, value)
    }
}

/// Split a `--mount` specification into its fields, honouring the double-quoting above.
///
/// Docker parses the argument with Go's `encoding/csv`, so a field that *starts* with a
/// double quote runs to its closing quote (with `""` for a literal quote) and commas inside
/// it are data. A user may write that form in `mounts` directly — the reference forwards an
/// authored mount string verbatim, and deacon parses and re-emits it, so its parser has to
/// understand what Docker accepts (#663).
fn split_mount_fields(spec: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = spec.chars().peekable();
    let mut at_field_start = true;
    let mut in_quotes = false;

    while let Some(c) = chars.next() {
        if at_field_start {
            if c == ' ' || c == '\t' {
                continue;
            }
            at_field_start = false;
            if c == '"' {
                in_quotes = true;
                continue;
            }
        }

        if in_quotes {
            if c == '"' {
                // `""` inside a quoted field is a literal quote; a lone one closes it.
                if chars.peek() == Some(&'"') {
                    chars.next();
                    current.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                current.push(c);
            }
        } else if c == ',' {
            fields.push(std::mem::take(&mut current));
            at_field_start = true;
        } else {
            current.push(c);
        }
    }
    fields.push(current);
    fields
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

        // Add source for bind and named-volume mounts. An absent source on a
        // `type=volume` mount is left absent on purpose — that is Docker's own
        // spelling for an anonymous volume (#617).
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
            mount_str.push_str(&format!(",{}", format_mount_field("source", &source_path)));
        }

        // Add target
        mount_str.push_str(&format!(",{}", format_mount_field("target", &self.target)));

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

        // Add additional options.
        //
        // Per #119, `external` is a deacon-internal marker (externally-
        // managed volume — don't create/manage) and is not a valid Docker
        // `--mount` option. Filter it out before handing to docker.
        for (key, value) in &self.options {
            if key == "external" {
                continue;
            }
            if value.is_empty() {
                mount_str.push_str(&format!(",{}", key));
            } else {
                mount_str.push_str(&format!(",{}", format_mount_field(key, value)));
            }
        }

        vec!["--mount".to_string(), mount_str]
    }

    /// Validate mount specification
    ///
    /// Checks for common configuration issues and logs warnings for unsupported fields.
    pub fn validate(&self) -> Result<()> {
        // Source rules follow Docker's `--mount` flag, which the spec defers to
        // verbatim ("Each value is a string that accepts the same values as the
        // Docker CLI `--mount` flag" —
        // `parity/spec/113500f4/devcontainerjson-reference.md:27`):
        //
        // - `bind` genuinely requires a source: there is no host path to bind
        //   without one.
        // - `volume` does NOT. Omitting `source` is precisely how Docker's own
        //   `--mount` asks for an ANONYMOUS volume, and the reference CLI passes
        //   that shape straight through (#617). Rejecting it was deacon's bug.
        // - `tmpfs` never has one.
        //
        // An explicitly EMPTY `source=` is a different thing from an absent one:
        // it is a typo, not a request, and stays a hard error for every type
        // that can carry a source.
        match self.mount_type {
            MountType::Bind => match self.source.as_deref() {
                None | Some("") => {
                    return Err(ConfigError::Validation {
                        message: format!("{} mount requires a source", self.mount_type),
                    }
                    .into());
                }
                Some(_) => {}
            },
            MountType::Volume => match self.source.as_deref() {
                Some("") => {
                    return Err(ConfigError::Validation {
                        message: format!(
                            "{} mount source must not be empty (omit `source` entirely for an anonymous volume)",
                            self.mount_type
                        ),
                    }
                    .into());
                }
                None => {
                    debug!(
                        target = %self.target,
                        "volume mount without a source: requesting an anonymous Docker volume"
                    );
                }
                Some(_) => {}
            },
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
                // Deacon-internal: externally-managed volume marker, filtered
                // out before handing to docker. Per #119.
                "external" => {
                    debug!("external=... is a deacon-internal volume marker");
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
///
/// # Variable substitution
/// When `substitution_context` is `Some(_)`, feature-provided mount values
/// (including `source`, `target`, and other string fields) are run through
/// variable substitution before parsing — per #122 and the spec rule that
/// variables expand in any string value. Callers from production paths
/// should pass `Some(&context)` so tokens like `${devcontainerId}` resolve.
/// Tests may pass `None` to skip substitution.
#[instrument(skip(config_mounts, features, substitution_context))]
pub fn merge_mounts(
    config_mounts: &[serde_json::Value],
    features: &[crate::features::ResolvedFeature],
    substitution_context: Option<&crate::variable::SubstitutionContext>,
) -> Result<MergedMounts> {
    use std::collections::HashMap;

    // Map to deduplicate by target path
    // The value is a tuple of (mount_string, insertion_index) to preserve order
    let mut mount_map: HashMap<String, (String, usize)> = HashMap::new();
    let mut insertion_index = 0;

    // Process feature mounts in installation order
    for feature in features {
        for mount_value in &feature.metadata.mounts {
            // Per #122, substitute variables (e.g. `${devcontainerId}`) in
            // the mount value before stringifying — feature mounts must
            // round-trip through substitution like every other string in
            // the resolved config.
            let substituted_mount_value: serde_json::Value;
            let mount_value = if let Some(ctx) = substitution_context {
                let mut report = crate::variable::SubstitutionReport::new();
                substituted_mount_value =
                    crate::variable::VariableSubstitution::substitute_json_value(
                        mount_value,
                        ctx,
                        &mut report,
                    );
                &substituted_mount_value
            } else {
                mount_value
            };
            let mount_str = crate::features::feature_mount_to_string(mount_value).map_err(|e| {
                warn!(
                    feature_id = %feature.id,
                    mount_spec = ?mount_value,
                    error = %e,
                    "Failed to convert mount from feature"
                );
                ConfigError::Validation {
                    message: format!("Invalid mount in feature {}: {}", feature.id, e),
                }
            })?;

            // Parse the mount to get the target and validate it
            let mount = MountParser::parse_mount(&mount_str).map_err(|e| {
                warn!(
                    feature_id = %feature.id,
                    mount_spec = %mount_str,
                    error = %e,
                    "Failed to parse mount from feature"
                );
                ConfigError::Validation {
                    message: format!(
                        "Invalid mount in feature {}: {}: {}",
                        feature.id, mount_str, e
                    ),
                }
            })?;

            // Normalize the mount to Docker CLI string format
            let normalized_str = normalize_mount_to_string(&mount);

            // Store in map, keyed by target (later overwrites earlier)
            // When overwriting, keep the original insertion index to preserve order
            match mount_map.get_mut(&mount.target) {
                Some((s, _idx)) => {
                    // Target already exists, update the mount string but keep the index
                    debug!(
                        feature_id = %feature.id,
                        target = %mount.target,
                        previous_mount = %s,
                        new_mount = %normalized_str,
                        "Feature mount overriding previous mount for same target"
                    );
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
        // Config mounts are normally substituted upstream during config load.
        // But mounts contributed by the image's `devcontainer.metadata` LABEL
        // are merged in *after* that pass (see
        // `merge_image_metadata_after_image_ready`), so they can still carry
        // literal tokens like `${devcontainerId}`. Run them through the same
        // substitution context as feature mounts before parsing — substitution
        // is idempotent, so already-resolved config mounts are unaffected (#224).
        let substituted_mount_value: serde_json::Value;
        let mount_value = if let Some(ctx) = substitution_context {
            let mut report = crate::variable::SubstitutionReport::new();
            substituted_mount_value = crate::variable::VariableSubstitution::substitute_json_value(
                mount_value,
                ctx,
                &mut report,
            );
            &substituted_mount_value
        } else {
            mount_value
        };
        let mount_str = match mount_value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Object(obj) => {
                // Convert object format to string format for parsing
                convert_object_mount_to_string(obj).map_err(|e| {
                    warn!(
                        mount_spec = ?obj,
                        error = %e,
                        "Failed to convert object mount from config to string format"
                    );
                    ConfigError::Validation {
                        message: format!("Invalid mount in config: {}", e),
                    }
                })?
            }
            _ => {
                warn!(
                    mount_spec = ?mount_value,
                    "Invalid mount specification type in config, expected string or object"
                );
                return Err(ConfigError::Validation {
                    message: "Invalid mount specification type, expected string or object"
                        .to_string(),
                }
                .into());
            }
        };

        // Parse the mount to get the target and validate it
        let mount = MountParser::parse_mount(&mount_str).map_err(|e| {
            warn!(
                mount_spec = %mount_str,
                error = %e,
                "Failed to parse mount from config"
            );
            ConfigError::Validation {
                message: format!("Invalid mount in config: {}: {}", mount_str, e),
            }
        })?;

        // Normalize the mount to Docker CLI string format
        let normalized_str = normalize_mount_to_string(&mount);

        // Store in map, overwriting any feature mount with same target
        // When overwriting, keep the original insertion index to preserve order
        match mount_map.get_mut(&mount.target) {
            Some((s, _idx)) => {
                // Target already exists, update the mount string but keep the index
                debug!(
                    target = %mount.target,
                    previous_mount = %s,
                    config_mount = %normalized_str,
                    "Config mount overriding feature mount for same target (config takes precedence)"
                );
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

    debug!(
        merged_count = mounts.len(),
        "Mount merging completed successfully"
    );

    Ok(MergedMounts { mounts })
}

/// Normalize a parsed Mount to Docker CLI string format
///
/// Converts a Mount struct to the standard Docker CLI string format:
/// `type={type},source={source},target={target}[,readonly][,...]`
fn normalize_mount_to_string(mount: &Mount) -> String {
    let mut parts = vec![format!("type={}", mount.mount_type)];

    // Add source for bind and named-volume mounts. An anonymous volume
    // (`type=volume` with no source) keeps its source absent (#617).
    if let Some(ref source) = mount.source {
        parts.push(format_mount_field("source", source));
    }

    // Add target
    parts.push(format_mount_field("target", &mount.target));

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
            parts.push(format_mount_field(key, value));
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
/// ```text
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
        parts.push(format_mount_field("source", source));
    }

    // Extract target (required)
    let target =
        obj.get("target")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ConfigError::Validation {
                message: "Mount object must have 'target' field".to_string(),
            })?;
    parts.push(format_mount_field("target", target));

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
                    parts.push(format_mount_field(key, str_value));
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

        for part in split_mount_fields(mount_spec) {
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

        // An empty first component (`:/container/path`) is not a Docker short-form
        // spelling of anything — the anonymous-volume shape is the object/`--mount`
        // form `type=volume,target=…` (#617), never a leading colon. Reject it here
        // so it does not fall through to `MountType::Volume` with no source and get
        // silently accepted as an anonymous volume.
        if parts[0].is_empty() {
            return Err(ConfigError::Validation {
                message: format!(
                    "Volume mount specification '{}' has an empty source; use 'type=volume,target=...' for an anonymous volume",
                    mount_spec
                ),
            }
            .into());
        }
        let source = Some(parts[0].to_string());

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

        // Determine mount type based on source. The empty-source case was rejected
        // above, so `source` is always `Some(non-empty)` here.
        let src = parts[0];
        let mount_type = if src.starts_with('/') || src.starts_with('.') || src.contains('\\') {
            MountType::Bind
        } else {
            MountType::Volume
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
        let mut mounts = Vec::new();

        for value in mount_values {
            match value {
                serde_json::Value::String(s) => match Self::parse_mount(s) {
                    Ok(mount) => mounts.push(mount),
                    Err(e) => warn!("Failed to parse mount '{}': {}", s, e),
                },
                serde_json::Value::Object(object) => match Self::parse_mount_object(object) {
                    Ok(mount) => mounts.push(mount),
                    Err(e) => warn!("Failed to parse object mount: {}", e),
                },
                _ => {
                    warn!("Invalid mount specification type, expected string or object");
                }
            }
        }

        mounts
    }

    fn parse_mount_object(object: &serde_json::Map<String, serde_json::Value>) -> Result<Mount> {
        fn string_field(
            object: &serde_json::Map<String, serde_json::Value>,
            names: &[&str],
        ) -> Option<String> {
            names
                .iter()
                .find_map(|name| object.get(*name).and_then(|v| v.as_str()))
                .map(ToOwned::to_owned)
        }

        let mount_type = string_field(object, &["type"])
            .ok_or_else(|| ConfigError::Validation {
                message: "Object mount specification must include 'type' field".to_string(),
            })?
            .parse::<MountType>()?;

        let source = string_field(object, &["source", "src"]);
        let target = string_field(object, &["target", "dst", "destination"]).ok_or_else(|| {
            ConfigError::Validation {
                message: "Object mount specification must include 'target' field".to_string(),
            }
        })?;

        let mode = if object
            .get("readOnly")
            .or_else(|| object.get("readonly"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            MountMode::ReadOnly
        } else {
            MountMode::ReadWrite
        };

        let consistency = string_field(object, &["consistency"])
            .map(|value| value.parse::<MountConsistency>())
            .transpose()?;

        let mut options = HashMap::new();
        for (key, value) in object {
            if matches!(
                key.as_str(),
                "type"
                    | "source"
                    | "src"
                    | "target"
                    | "dst"
                    | "destination"
                    | "readOnly"
                    | "readonly"
                    | "consistency"
            ) {
                continue;
            }

            match value {
                serde_json::Value::Bool(true) => {
                    options.insert(key.clone(), String::new());
                }
                serde_json::Value::Bool(false) | serde_json::Value::Null => {}
                serde_json::Value::String(s) => {
                    options.insert(key.clone(), s.clone());
                }
                other => {
                    options.insert(key.clone(), other.to_string());
                }
            }
        }

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
}

/// Extract the container-side target path from a single mount specification, supporting both
/// string forms (Docker `--mount` syntax and short `source:target[:options]` syntax) and
/// object form (`{ "type": ..., "source": ..., "target": ... }`).
///
/// Returns `None` for inputs whose target cannot be determined (non-string, non-object values
/// or malformed strings missing a target). Callers should treat untargeted mounts as opaque
/// and preserve them in declaration order rather than deduplicating them.
pub fn extract_mount_target(mount: &serde_json::Value) -> Option<String> {
    match mount {
        serde_json::Value::Object(map) => map
            .get("target")
            .or_else(|| map.get("destination"))
            .or_else(|| map.get("dst"))
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        serde_json::Value::String(s) => {
            if s.contains('=') {
                for part in s.split(',') {
                    let part = part.trim();
                    if let Some((key, value)) = part.split_once('=') {
                        if matches!(key.trim(), "target" | "dst" | "destination") {
                            return Some(value.trim().to_string());
                        }
                    }
                }
                None
            } else {
                // Volume syntax `source:target[:options]` — second component is the target.
                let parts: Vec<&str> = s.split(':').collect();
                if parts.len() >= 2 && !parts[1].is_empty() {
                    Some(parts[1].to_string())
                } else {
                    None
                }
            }
        }
        _ => None,
    }
}

/// Merge two mount lists with per-target deduplication, mirroring the upstream `mergeMounts`
/// in `devcontainers/cli/src/spec-node/imageMetadata.ts`.
///
/// Semantics:
/// - Both lists are concatenated in declaration order (base first, then overlay).
/// - For each container-side target that appears more than once, only the LAST occurrence is
///   kept; earlier occurrences are dropped.
/// - Mounts whose target cannot be parsed are preserved verbatim in their original position.
/// - Surviving mounts retain their original declaration order.
pub fn union_mounts_by_target(
    base: &[serde_json::Value],
    overlay: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    let combined: Vec<serde_json::Value> = base.iter().chain(overlay.iter()).cloned().collect();
    if combined.is_empty() {
        return combined;
    }

    let mut keep = vec![true; combined.len()];
    let mut last_index_by_target: HashMap<String, usize> = HashMap::new();
    for (idx, mount) in combined.iter().enumerate() {
        if let Some(target) = extract_mount_target(mount) {
            if let Some(&prev_idx) = last_index_by_target.get(&target) {
                keep[prev_idx] = false;
            }
            last_index_by_target.insert(target, idx);
        }
    }

    combined
        .into_iter()
        .zip(keep)
        .filter_map(|(m, k)| if k { Some(m) } else { None })
        .collect()
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
    fn test_parse_object_mount_from_json() {
        let mounts = MountParser::parse_mounts_from_json(&[serde_json::json!({
            "type": "bind",
            "source": "/host/path",
            "target": "/container/path",
            "readOnly": true,
            "consistency": "cached"
        })]);

        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].mount_type, MountType::Bind);
        assert_eq!(mounts[0].source.as_deref(), Some("/host/path"));
        assert_eq!(mounts[0].target, "/container/path");
        assert_eq!(mounts[0].mode, MountMode::ReadOnly);
        assert_eq!(mounts[0].consistency, Some(MountConsistency::Cached));
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

    /// #617: `type=volume` with no `source` is Docker's own spelling for an
    /// ANONYMOUS volume, and the spec defers `mounts` to the `--mount` flag
    /// verbatim. It must parse, validate, and reach Docker with its source
    /// still absent — deacon used to reject it.
    #[test]
    fn test_anonymous_volume_docker_syntax_accepted() {
        let mount = MountParser::parse_mount("type=volume,target=/home/anon")
            .expect("anonymous volume must parse");

        assert_eq!(mount.mount_type, MountType::Volume);
        assert_eq!(mount.source, None);
        assert_eq!(mount.target, "/home/anon");

        // The source must stay ABSENT on the wire; `source=` with an empty value
        // is rejected by Docker.
        assert_eq!(
            mount.to_docker_args(),
            vec![
                "--mount".to_string(),
                "type=volume,target=/home/anon".to_string()
            ]
        );
    }

    /// The same shape in the object form the reference's own fixture uses
    /// (`src/test/configs/image-with-mounts`).
    #[test]
    fn test_anonymous_volume_object_form_accepted() {
        let mounts = MountParser::parse_mounts_from_json(&[serde_json::json!({
            "target": "/home/test_devcontainer_config",
            "type": "volume"
        })]);

        assert_eq!(mounts.len(), 1, "object-form anonymous volume must parse");
        assert_eq!(mounts[0].mount_type, MountType::Volume);
        assert_eq!(mounts[0].source, None);
        assert_eq!(mounts[0].target, "/home/test_devcontainer_config");
    }

    /// An anonymous volume may still carry the options Docker allows on one.
    #[test]
    fn test_anonymous_volume_readonly_round_trips() {
        let mount = MountParser::parse_mount("type=volume,target=/home/anon,ro")
            .expect("anonymous volume with ro must parse");

        assert_eq!(mount.mode, MountMode::ReadOnly);
        assert_eq!(
            mount.to_docker_args(),
            vec![
                "--mount".to_string(),
                "type=volume,target=/home/anon,ro".to_string()
            ]
        );
    }

    #[test]
    fn test_anonymous_volume_validates() {
        let mount = Mount {
            mount_type: MountType::Volume,
            source: None,
            target: "/home/anon".to_string(),
            mode: MountMode::ReadWrite,
            consistency: None,
            options: HashMap::new(),
        };

        assert!(mount.validate().is_ok());
    }

    /// The allowance is targeted: `bind` still genuinely requires a source, so
    /// dropping it stays a hard error rather than becoming an anonymous volume.
    #[test]
    fn test_mount_validation_bind_without_source_still_rejected() {
        assert!(MountParser::parse_mount("type=bind,target=/container/path").is_err());
    }

    /// An explicitly EMPTY `source=` is a typo, not a request for an anonymous
    /// volume — Docker rejects an empty source value on either type.
    #[test]
    fn test_mount_validation_empty_source_rejected() {
        for spec in [
            "type=volume,source=,target=/container/path",
            "type=bind,source=,target=/container/path",
        ] {
            assert!(
                MountParser::parse_mount(spec).is_err(),
                "empty source must be rejected: {spec}"
            );
        }
    }

    /// The short `source:target` form has no anonymous spelling, so a leading
    /// colon stays an error instead of silently becoming an anonymous volume.
    #[test]
    fn test_short_form_empty_source_rejected() {
        let err = MountParser::parse_mount(":/container/path")
            .expect_err("leading-colon short form must be rejected");
        assert!(
            err.to_string().contains("empty source"),
            "expected an empty-source diagnostic, got: {err}"
        );
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
mod target_dedup_tests {
    //! Unit tests for [`extract_mount_target`] and [`union_mounts_by_target`].
    //!
    //! Mirrors upstream `mergeMounts` semantics in
    //! `devcontainers/cli/src/spec-node/imageMetadata.ts`: per-target dedupe with the LAST
    //! occurrence winning, surviving entries keeping original declaration order.

    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_target_object_form() {
        let mount = json!({ "type": "bind", "source": "/host", "target": "/container" });
        assert_eq!(extract_mount_target(&mount).as_deref(), Some("/container"));
    }

    #[test]
    fn test_extract_target_object_form_alias_keys() {
        // Both `destination` and `dst` are accepted aliases per Docker syntax.
        let with_dst = json!({ "type": "bind", "source": "/x", "dst": "/y" });
        assert_eq!(extract_mount_target(&with_dst).as_deref(), Some("/y"));
        let with_destination = json!({ "type": "bind", "source": "/a", "destination": "/b" });
        assert_eq!(
            extract_mount_target(&with_destination).as_deref(),
            Some("/b")
        );
    }

    #[test]
    fn test_extract_target_docker_string_syntax() {
        let mount = json!("type=bind,source=/host,target=/container,ro");
        assert_eq!(extract_mount_target(&mount).as_deref(), Some("/container"));
    }

    #[test]
    fn test_extract_target_volume_syntax() {
        let mount = json!("/host/path:/container/path:ro");
        assert_eq!(
            extract_mount_target(&mount).as_deref(),
            Some("/container/path")
        );
    }

    #[test]
    fn test_extract_target_missing_target() {
        // No target field in object → None.
        let mount = json!({ "type": "tmpfs", "source": "/host" });
        assert_eq!(extract_mount_target(&mount), None);
        // String with only one component → None.
        let bare = json!("/host/path");
        assert_eq!(extract_mount_target(&bare), None);
        // Non-string, non-object → None.
        let arr = json!(["not", "a", "mount"]);
        assert_eq!(extract_mount_target(&arr), None);
    }

    #[test]
    fn test_union_mounts_last_wins_for_overlapping_target() {
        let m1 = json!({ "type": "bind", "source": "/v1", "target": "/data" });
        let m2 = json!({ "type": "bind", "source": "/v2", "target": "/data" });
        let result = union_mounts_by_target(&[m1], std::slice::from_ref(&m2));
        assert_eq!(result, vec![m2]);
    }

    #[test]
    fn test_union_mounts_preserves_order_of_survivors() {
        // [A1, B, A2] → drop A1 (superseded by A2), preserve [B, A2] in original order.
        let a1 = json!({ "type": "bind", "source": "/a1", "target": "/a" });
        let b = json!({ "type": "bind", "source": "/b", "target": "/b" });
        let a2 = json!({ "type": "bind", "source": "/a2", "target": "/a" });
        let result = union_mounts_by_target(&[a1, b.clone()], std::slice::from_ref(&a2));
        assert_eq!(result, vec![b, a2]);
    }

    #[test]
    fn test_union_mounts_dedupes_within_a_single_list() {
        // Duplicates within `base` alone must also dedupe (covers feature-loop accumulation).
        let m1 = json!({ "type": "bind", "source": "/v1", "target": "/data" });
        let m2 = json!({ "type": "bind", "source": "/v2", "target": "/data" });
        let result = union_mounts_by_target(&[m1, m2.clone()], &[]);
        assert_eq!(result, vec![m2]);
    }

    #[test]
    fn test_union_mounts_string_vs_object_form_same_target_dedupes() {
        // String and object forms with the same target → object wins (later in declaration order).
        let string_form = json!("type=bind,source=/v1,target=/data");
        let object_form = json!({ "type": "bind", "source": "/v2", "target": "/data" });
        let result = union_mounts_by_target(&[string_form], std::slice::from_ref(&object_form));
        assert_eq!(result, vec![object_form]);
    }

    #[test]
    fn test_union_mounts_preserves_unparseable_entries() {
        // A mount with no extractable target is kept as-is and does not cause panics.
        let untargeted = json!({ "type": "tmpfs", "source": "/x" });
        let m_with_target = json!({ "type": "bind", "source": "/v", "target": "/data" });
        let result = union_mounts_by_target(
            std::slice::from_ref(&untargeted),
            std::slice::from_ref(&m_with_target),
        );
        assert_eq!(result, vec![untargeted, m_with_target]);
    }

    #[test]
    fn test_union_mounts_empty_inputs() {
        assert!(union_mounts_by_target(&[], &[]).is_empty());
        let m = json!({ "type": "bind", "source": "/v", "target": "/t" });
        assert_eq!(
            union_mounts_by_target(std::slice::from_ref(&m), &[]),
            vec![m.clone()]
        );
        assert_eq!(
            union_mounts_by_target(&[], std::slice::from_ref(&m)),
            vec![m]
        );
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
            customizations: None,
            mounts: mounts.into_iter().map(serde_json::Value::String).collect(),
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
        assert_eq!(result.mounts.len(), 2);
        assert!(
            result
                .mounts
                .contains(&"type=bind,source=/host/data,target=/data".to_string())
        );
        assert!(
            result
                .mounts
                .contains(&"type=volume,source=cache,target=/cache".to_string())
        );
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
        assert_eq!(result.mounts.len(), 2);
        assert!(
            result
                .mounts
                .contains(&"type=volume,source=vol1,target=/vol1".to_string())
        );
        assert!(
            result
                .mounts
                .contains(&"type=volume,source=vol2,target=/vol2".to_string())
        );
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
        assert_eq!(result.mounts.len(), 2);
        assert!(
            result
                .mounts
                .contains(&"type=volume,source=cache,target=/cache".to_string())
        );
        assert!(
            result
                .mounts
                .contains(&"type=bind,source=/host/data,target=/data".to_string())
        );
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
        assert_eq!(result.mounts.len(), 3);
        assert!(
            result
                .mounts
                .contains(&"type=volume,source=vol1,target=/vol1".to_string())
        );
        assert!(
            result
                .mounts
                .contains(&"type=volume,source=vol2,target=/vol2".to_string())
        );
        assert!(
            result
                .mounts
                .contains(&"type=volume,source=override-shared,target=/shared".to_string())
        );
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
        assert_eq!(result.mounts.len(), 1);
        assert!(result.mounts[0].contains("target=/container/path"));
        assert!(result.mounts[0].contains("ro"));
    }

    #[test]
    fn test_merge_mounts_normalize_named_volume() {
        // Named volume syntax should be normalized
        let config_mounts = vec![serde_json::Value::String("myvolume:/data".to_string())];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
        assert_eq!(result.mounts.len(), 1);
        assert!(result.mounts[0].contains("target=/data"));
        assert!(result.mounts[0].contains("myvolume"));
    }

    // ==================== Edge Cases ====================

    #[test]
    fn test_merge_mounts_feature_mount_with_devcontainer_id_substitution() {
        // Per #122: feature mount sources containing variables (e.g.
        // ${devcontainerId}, used by docker-in-docker for its volume name)
        // must be substituted before being handed to Docker. Without
        // substitution, Docker rejects the volume name as containing
        // invalid characters (literal '$').
        let temp = tempfile::TempDir::new().unwrap();
        let mut ctx = crate::variable::SubstitutionContext::new(temp.path()).unwrap();
        ctx.devcontainer_id = "abc123def456".to_string();

        let config_mounts: Vec<serde_json::Value> = vec![];
        let features = vec![create_feature_with_mounts(
            "dind",
            vec![
                "type=volume,source=dind-var-lib-docker-${devcontainerId},target=/var/lib/docker"
                    .to_string(),
            ],
        )];

        let result = merge_mounts(&config_mounts, &features, Some(&ctx)).unwrap();
        assert_eq!(result.mounts.len(), 1);
        let mount = &result.mounts[0];
        assert!(
            mount.contains("dind-var-lib-docker-abc123def456"),
            "expected substituted volume name; got: {mount}"
        );
        assert!(
            !mount.contains("${devcontainerId}"),
            "literal token must not survive into docker mount string"
        );
    }

    #[test]
    fn test_merge_mounts_config_mount_with_devcontainer_id_substitution() {
        // Per #224: a config mount source containing ${devcontainerId} (e.g.
        // contributed by an image's devcontainer.metadata label, which is
        // merged into config.mounts AFTER the up substitution pass) must also
        // be substituted before reaching Docker. Substitution is idempotent, so
        // already-resolved config mounts are unaffected.
        let temp = tempfile::TempDir::new().unwrap();
        let mut ctx = crate::variable::SubstitutionContext::new(temp.path()).unwrap();
        ctx.devcontainer_id = "abc123def456".to_string();

        let config_mounts = vec![serde_json::json!({
            "source": "dind-var-lib-docker-${devcontainerId}",
            "target": "/var/lib/docker",
            "type": "volume"
        })];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features, Some(&ctx)).unwrap();
        assert_eq!(result.mounts.len(), 1);
        let mount = &result.mounts[0];
        assert!(
            mount.contains("dind-var-lib-docker-abc123def456"),
            "expected substituted config volume name; got: {mount}"
        );
        assert!(
            !mount.contains("${devcontainerId}"),
            "literal token must not survive into docker mount string"
        );
    }

    #[test]
    fn test_merge_mounts_empty_feature_mounts() {
        // Feature with empty mounts array
        let config_mounts = vec![serde_json::Value::String(
            "type=bind,source=/host/data,target=/data".to_string(),
        )];
        let features = vec![create_feature_with_mounts("feature1", vec![])];

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
        assert_eq!(result.mounts.len(), 3);
        assert!(
            result
                .mounts
                .contains(&"type=volume,source=vol1,target=/vol1".to_string())
        );
        assert!(
            result
                .mounts
                .contains(&"type=volume,source=vol2,target=/vol2".to_string())
        );
        assert!(
            result
                .mounts
                .contains(&"type=tmpfs,target=/tmp".to_string())
        );
    }

    #[test]
    fn test_merge_mounts_tmpfs_mount() {
        // tmpfs mounts should work correctly
        let config_mounts = vec![serde_json::Value::String(
            "type=tmpfs,target=/tmp".to_string(),
        )];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
        assert_eq!(result.mounts.len(), 4);
        // Config mounts should be present
        assert!(
            result
                .mounts
                .contains(&"type=bind,source=/host/workspace,target=/workspace".to_string())
        );
        assert!(
            result
                .mounts
                .contains(&"type=bind,source=/host/override,target=/data".to_string())
        );
        // Feature2's cache should override feature1's cache
        assert!(
            result
                .mounts
                .contains(&"type=volume,source=cache2,target=/cache".to_string())
        );
        // Feature2's tmpfs should be present
        assert!(
            result
                .mounts
                .contains(&"type=tmpfs,target=/tmp".to_string())
        );
    }

    // ==================== Error Handling Tests ====================

    #[test]
    fn test_merge_mounts_invalid_mount_string() {
        // Invalid mount string should return error
        let config_mounts = vec![serde_json::Value::String("invalid-mount-spec".to_string())];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_mounts_missing_target() {
        // Mount without target should return error
        let config_mounts = vec![serde_json::Value::String(
            "type=bind,source=/host/path".to_string(),
        )];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features, None);
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

        let result = merge_mounts(&config_mounts, &features, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_mounts_relative_target() {
        // Mount with relative target should return error
        let config_mounts = vec![serde_json::Value::String(
            "type=bind,source=/host/path,target=relative/path".to_string(),
        )];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_mounts_bind_without_source() {
        // Bind mount without source should return error
        let config_mounts = vec![serde_json::Value::String(
            "type=bind,target=/container/path".to_string(),
        )];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_mounts_error_attribution_config() {
        // Error from config mount should include "config" in message
        let config_mounts = vec![serde_json::Value::String(
            "type=bind,target=/container/path".to_string(), // Missing source
        )];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features, None);
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

        let result = merge_mounts(&config_mounts, &features, None);
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
        assert_eq!(result.mounts.len(), 1);
        assert!(result.mounts[0].contains("type=volume"));
        assert!(result.mounts[0].contains("source=myvolume"));
        assert!(result.mounts[0].contains("target=/data"));
    }

    /// #617: the exact config `up` receives from
    /// `fx-upstream-mount-object-anonymous-volume`. `merge_mounts` is the single
    /// path every mount takes to `docker create --mount`, so the normalized
    /// string it emits must carry NO `source` key at all.
    #[test]
    fn test_merge_mounts_object_format_anonymous_volume() {
        let config_mounts = vec![serde_json::json!({
            "target": "/home/test_devcontainer_config",
            "type": "volume"
        })];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features, None)
            .expect("anonymous volume mount must merge");
        assert_eq!(
            result.mounts,
            vec!["type=volume,target=/home/test_devcontainer_config".to_string()]
        );
    }

    /// The string form of the same shape, which `deacon up --mount` produces.
    #[test]
    fn test_merge_mounts_string_form_anonymous_volume() {
        let config_mounts = vec![serde_json::Value::String(
            "type=volume,target=/home/anon".to_string(),
        )];
        let features = vec![];

        let result =
            merge_mounts(&config_mounts, &features, None).expect("anonymous volume must merge");
        assert_eq!(
            result.mounts,
            vec!["type=volume,target=/home/anon".to_string()]
        );
    }

    /// A feature may request one too — the merge path is shared.
    #[test]
    fn test_merge_mounts_feature_anonymous_volume() {
        let config_mounts: Vec<serde_json::Value> = vec![];
        let features = vec![create_feature_with_mounts(
            "anon",
            vec!["type=volume,target=/scratch".to_string()],
        )];

        let result = merge_mounts(&config_mounts, &features, None)
            .expect("feature anonymous volume must merge");
        assert_eq!(
            result.mounts,
            vec!["type=volume,target=/scratch".to_string()]
        );
    }

    /// Targeted allowance: an object-form mount with an EMPTY `source` is still
    /// an error, and so is an object-form `bind` with no source at all.
    #[test]
    fn test_merge_mounts_object_format_invalid_sources_still_rejected() {
        for mount in [
            serde_json::json!({ "type": "volume", "source": "", "target": "/data" }),
            serde_json::json!({ "type": "bind", "target": "/data" }),
        ] {
            let result = merge_mounts(std::slice::from_ref(&mount), &[], None);
            assert!(result.is_err(), "must be rejected: {mount}");
        }
    }

    #[test]
    fn test_merge_mounts_object_format_tmpfs() {
        // Object format for tmpfs mount (no source)
        let config_mounts = vec![serde_json::json!({
            "type": "tmpfs",
            "target": "/tmp"
        })];
        let features = vec![];

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
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

        let result = merge_mounts(&config_mounts, &features, None);
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

        let result = merge_mounts(&config_mounts, &features, None);
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
        assert_eq!(result.mounts.len(), 3);
        // The exact order may vary based on implementation, but all should be present
        assert!(
            result
                .mounts
                .contains(&"type=volume,source=vol1,target=/vol1".to_string())
        );
        assert!(
            result
                .mounts
                .contains(&"type=volume,source=vol2,target=/vol2".to_string())
        );
        assert!(
            result
                .mounts
                .contains(&"type=volume,source=vol3,target=/vol3".to_string())
        );
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

        let result = merge_mounts(&config_mounts, &features, None).unwrap();
        assert_eq!(result.mounts.len(), 3);
        // All mounts should be present
        assert!(
            result
                .mounts
                .contains(&"type=volume,source=vol1,target=/vol1".to_string())
        );
        assert!(
            result
                .mounts
                .contains(&"type=volume,source=vol2,target=/vol2".to_string())
        );
        assert!(
            result
                .mounts
                .contains(&"type=volume,source=vol3,target=/vol3".to_string())
        );
    }
}

#[cfg(test)]
mod comma_quoting_tests {
    //! A `--mount` argument is CSV, so a value holding a comma has to be quoted on the way
    //! out and unquoted on the way in — the reference CLI quotes it (`spec-node/utils.ts`)
    //! and Docker parses the argument with Go's `encoding/csv`. Without both halves, a
    //! workspace whose path contains a comma cannot be mounted at all (#663).

    use super::*;

    #[test]
    fn a_field_is_quoted_only_when_its_value_holds_a_comma() {
        assert_eq!(
            format_mount_field("source", "/host/path"),
            "source=/host/path"
        );
        assert_eq!(
            format_mount_field("source", "/host/com,ma"),
            "\"source=/host/com,ma\""
        );
        // The quotes wrap the whole `key=value`, as the reference writes it.
        assert_eq!(
            format_mount_field("target", "/workspaces/a,b"),
            "\"target=/workspaces/a,b\""
        );
    }

    #[test]
    fn splitting_honours_quoted_fields() {
        assert_eq!(
            split_mount_fields("type=bind,source=/a,target=/b"),
            vec!["type=bind", "source=/a", "target=/b"]
        );
        assert_eq!(
            split_mount_fields("type=bind,\"source=/a,b\",\"target=/c,d\",ro"),
            vec!["type=bind", "source=/a,b", "target=/c,d", "ro"]
        );
        // `""` inside a quoted field is a literal quote.
        assert_eq!(
            split_mount_fields("\"source=/a\"\"b\",target=/c"),
            vec!["source=/a\"b", "target=/c"]
        );
    }

    #[test]
    fn a_comma_bearing_workspace_path_round_trips() {
        let spec = "type=bind,\"source=/host/com,ma\",\"target=/workspaces/com,ma\"";
        let mount = MountParser::parse_mount(spec).unwrap();
        assert_eq!(mount.source.as_deref(), Some("/host/com,ma"));
        assert_eq!(mount.target, "/workspaces/com,ma");
        assert_eq!(normalize_mount_to_string(&mount), spec);
        // `to_docker_args` additionally resolves a bind source against the cwd and applies
        // Docker Desktop path conversion, so its OUTPUT is platform-shaped; the quoting is
        // not. Assert the whole string only where the path shape is the one written here.
        #[cfg(unix)]
        assert_eq!(
            mount.to_docker_args(),
            vec!["--mount".to_string(), spec.to_string()]
        );
    }

    #[test]
    fn a_comma_free_mount_is_emitted_exactly_as_before() {
        let mount = Mount {
            mount_type: MountType::Bind,
            source: Some("/host/path".to_string()),
            target: "/container/path".to_string(),
            mode: MountMode::ReadWrite,
            consistency: None,
            options: HashMap::new(),
        };
        assert_eq!(
            normalize_mount_to_string(&mount),
            "type=bind,source=/host/path,target=/container/path"
        );
        #[cfg(unix)]
        assert_eq!(
            mount.to_docker_args(),
            vec![
                "--mount".to_string(),
                "type=bind,source=/host/path,target=/container/path".to_string()
            ]
        );
    }

    #[test]
    fn an_object_mount_with_a_comma_is_normalized_quoted() {
        let object = serde_json::json!({
            "type": "bind",
            "source": "/host/com,ma",
            "target": "/opt/com,ma",
        });
        let rendered = convert_object_mount_to_string(object.as_object().unwrap())
            .expect("object mount should normalize");
        assert_eq!(
            rendered,
            "type=bind,\"source=/host/com,ma\",\"target=/opt/com,ma\""
        );
        // …and Docker's own reader gets the paths back intact.
        let mount = MountParser::parse_mount(&rendered).unwrap();
        assert_eq!(mount.source.as_deref(), Some("/host/com,ma"));
        assert_eq!(mount.target, "/opt/com,ma");
    }
}
