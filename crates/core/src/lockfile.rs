//! Lockfile data structures and I/O operations
//!
//! This module provides complete lockfile support for DevContainer configurations,
//! following the DevContainer specification. It implements data structures for
//! representing feature lock entries and provides functions for reading, writing,
//! and merging lockfiles.
//!
//! ## Overview
//!
//! Lockfiles track resolved feature versions and their integrity information,
//! enabling reproducible container builds and version management.
//!
//! ## Data Structures
//!
//! - [`Lockfile`] - Top-level lockfile structure containing feature entries
//! - [`LockfileFeature`] - Individual feature lock entry with version and integrity info
//!
//! ## Path Derivation
//!
//! Lockfile names follow a convention based on the config file basename:
//! - Config starting with `.` → `.devcontainer-lock.json`
//! - Otherwise → `devcontainer-lock.json`
//! - Location: Same directory as config file
//!
//! ## Operations
//!
//! - [`get_lockfile_path`] - Derive lockfile path from config path
//! - [`read_lockfile`] - Read and parse lockfile (returns None if not found)
//! - [`write_lockfile`] - Write lockfile with atomic operation
//! - [`merge_lockfile_features`] - Merge two lockfiles with conflict resolution
//!
//! ## Examples
//!
//! ```rust
//! use deacon_core::lockfile::{Lockfile, LockfileFeature, get_lockfile_path};
//! use std::path::Path;
//! use std::collections::HashMap;
//!
//! // Determine lockfile path
//! let config_path = Path::new(".devcontainer/devcontainer.json");
//! let lockfile_path = get_lockfile_path(config_path);
//! assert_eq!(lockfile_path, Path::new(".devcontainer/devcontainer-lock.json"));
//!
//! // Create a new lockfile
//! let mut lockfile = Lockfile {
//!     features: HashMap::new(),
//! };
//!
//! lockfile.features.insert(
//!     "ghcr.io/devcontainers/features/node".to_string(),
//!     LockfileFeature {
//!         version: "1.2.3".to_string(),
//!         resolved: "ghcr.io/devcontainers/features/node@sha256:abc123".to_string(),
//!         integrity: "sha256:abc123".to_string(),
//!         depends_on: None,
//!     },
//! );
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Lockfile structure per DevContainer specification
///
/// Contains a map of feature identifiers to their lock entries.
/// The map keys are typically OCI references or feature identifiers.
///
/// # Examples
///
/// ```rust
/// use deacon_core::lockfile::{Lockfile, LockfileFeature};
/// use std::collections::HashMap;
///
/// let mut lockfile = Lockfile {
///     features: HashMap::new(),
/// };
///
/// lockfile.features.insert(
///     "ghcr.io/devcontainers/features/docker".to_string(),
///     LockfileFeature {
///         version: "2.0.0".to_string(),
///         resolved: "ghcr.io/devcontainers/features/docker@sha256:def456".to_string(),
///         integrity: "sha256:def456".to_string(),
///         depends_on: Some(vec!["ghcr.io/devcontainers/features/common".to_string()]),
///     },
/// );
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Lockfile {
    /// Map of feature identifiers to their lock entries
    pub features: HashMap<String, LockfileFeature>,
}

/// Individual feature lock entry
///
/// Contains version information and integrity data for a single feature.
///
/// # Examples
///
/// ```rust
/// use deacon_core::lockfile::LockfileFeature;
///
/// let feature = LockfileFeature {
///     version: "1.0.0".to_string(),
///     resolved: "ghcr.io/devcontainers/features/node@sha256:abc123".to_string(),
///     integrity: "sha256:abc123".to_string(),
///     depends_on: None,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LockfileFeature {
    /// Semantic version (e.g., "2.11.1")
    pub version: String,

    /// Full OCI reference with digest (e.g., "ghcr.io/owner/feature@sha256:...")
    pub resolved: String,

    /// SHA256 digest for integrity checking (e.g., "sha256:...")
    pub integrity: String,

    /// Optional feature dependencies
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,
}

/// Get lockfile path adjacent to config file
///
/// Implements the lockfile naming convention:
/// - If config basename starts with `.` → `.devcontainer-lock.json`
/// - Otherwise → `devcontainer-lock.json`
/// - Location: Same directory as config file
///
/// # Arguments
///
/// * `config_path` - Path to the DevContainer configuration file
///
/// # Returns
///
/// Path to the lockfile in the same directory as the config file
///
/// # Examples
///
/// ```rust
/// use deacon_core::lockfile::get_lockfile_path;
/// use std::path::Path;
///
/// // Config with dot prefix
/// let config = Path::new(".devcontainer/devcontainer.json");
/// let lockfile = get_lockfile_path(config);
/// assert_eq!(lockfile, Path::new(".devcontainer/devcontainer-lock.json"));
///
/// // Hidden config file
/// let config = Path::new(".devcontainer/.devcontainer.json");
/// let lockfile = get_lockfile_path(config);
/// assert_eq!(lockfile, Path::new(".devcontainer/.devcontainer-lock.json"));
/// ```
pub fn get_lockfile_path(config_path: &Path) -> PathBuf {
    let config_dir = config_path.parent().unwrap_or(Path::new("."));
    let config_basename = config_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("devcontainer.json");

    let lockfile_name = if config_basename.starts_with('.') {
        ".devcontainer-lock.json"
    } else {
        "devcontainer-lock.json"
    };

    config_dir.join(lockfile_name)
}

/// Read lockfile from disk
///
/// Reads and parses a lockfile from the specified path. Returns `None` if the
/// file doesn't exist (not an error condition). Invalid JSON or I/O errors
/// are returned as errors.
///
/// # Arguments
///
/// * `path` - Path to the lockfile
///
/// # Returns
///
/// - `Ok(Some(Lockfile))` if file exists and is valid
/// - `Ok(None)` if file doesn't exist
/// - `Err(...)` for I/O errors or invalid JSON
///
/// # Examples
///
/// ```rust
/// use deacon_core::lockfile::read_lockfile;
/// use std::path::Path;
///
/// // Non-existent file returns None (not an error)
/// let result = read_lockfile(Path::new("/tmp/nonexistent-lockfile.json")).unwrap();
/// assert!(result.is_none());
/// ```
pub fn read_lockfile(path: &Path) -> Result<Option<Lockfile>> {
    // Check if file exists
    if !path.exists() {
        return Ok(None);
    }

    // Read file contents
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read lockfile from {}", path.display()))?;

    // Parse JSON
    let lockfile: Lockfile = serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse lockfile from {}", path.display()))?;

    // Validate lockfile
    validate_lockfile(&lockfile)
        .with_context(|| format!("Lockfile validation failed for {}", path.display()))?;

    Ok(Some(lockfile))
}

/// Write lockfile to disk
///
/// Writes a lockfile to the specified path with atomic operation (write to temp,
/// then rename). Creates parent directories if needed. Formats JSON with 2-space
/// indentation for readability.
///
/// # Arguments
///
/// * `path` - Path to write the lockfile
/// * `lockfile` - Lockfile data to write
/// * `force_init` - If true, always write; if false, may skip in certain conditions
///
/// # Returns
///
/// Result indicating success or failure
///
/// # Examples
///
/// ```rust,no_run
/// use deacon_core::lockfile::{Lockfile, write_lockfile};
/// use std::collections::HashMap;
/// use std::path::Path;
///
/// let lockfile = Lockfile {
///     features: HashMap::new(),
/// };
///
/// write_lockfile(Path::new("/tmp/test-lock.json"), &lockfile, false).unwrap();
/// ```
pub fn write_lockfile(path: &Path, lockfile: &Lockfile, force_init: bool) -> Result<()> {
    // Check if file exists and force_init is false
    if path.exists() && !force_init {
        anyhow::bail!(
            "Lockfile already exists at {}. Use force_init=true to overwrite.",
            path.display()
        );
    }

    // Validate lockfile before writing
    validate_lockfile(lockfile).context("Lockfile validation failed before write")?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Convert to serde_json::Value for deterministic ordering
    let mut value =
        serde_json::to_value(lockfile).context("Failed to convert lockfile to JSON value")?;

    // Sort all object keys recursively for stable JSON output
    sort_json_object(&mut value);

    // Serialize with pretty printing (2-space indentation)
    let json =
        serde_json::to_string_pretty(&value).context("Failed to serialize lockfile to JSON")?;

    // Atomic write: write to temp file in same directory, then rename
    // Using same directory ensures same filesystem for atomic rename on all platforms
    let temp_path = if let Some(parent) = path.parent() {
        parent.join(format!(
            ".{}.tmp",
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("lockfile")
        ))
    } else {
        PathBuf::from(format!(
            ".{}.tmp",
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("lockfile")
        ))
    };

    fs::write(&temp_path, json.as_bytes()).with_context(|| {
        format!(
            "Failed to write temporary lockfile to {}",
            temp_path.display()
        )
    })?;

    // On Windows, remove destination file if it exists before rename
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path)
            .with_context(|| format!("Failed to remove existing lockfile at {}", path.display()))?;
    }

    fs::rename(&temp_path, path)
        .with_context(|| format!("Failed to rename temporary lockfile to {}", path.display()))?;

    Ok(())
}

/// Merge two lockfiles
///
/// Combines feature entries from two lockfiles. When a feature exists in both
/// lockfiles, the entry from `new` takes precedence. Features only in `existing`
/// are preserved.
///
/// # Arguments
///
/// * `existing` - Current lockfile
/// * `new` - New lockfile with updates
///
/// # Returns
///
/// Merged lockfile combining both inputs
///
/// # Examples
///
/// ```rust
/// use deacon_core::lockfile::{Lockfile, LockfileFeature, merge_lockfile_features};
/// use std::collections::HashMap;
///
/// let mut existing = Lockfile { features: HashMap::new() };
/// existing.features.insert(
///     "feature-a".to_string(),
///     LockfileFeature {
///         version: "1.0.0".to_string(),
///         resolved: "registry/feature-a@sha256:old".to_string(),
///         integrity: "sha256:old".to_string(),
///         depends_on: None,
///     },
/// );
///
/// let mut new = Lockfile { features: HashMap::new() };
/// new.features.insert(
///     "feature-a".to_string(),
///     LockfileFeature {
///         version: "2.0.0".to_string(),
///         resolved: "registry/feature-a@sha256:new".to_string(),
///         integrity: "sha256:new".to_string(),
///         depends_on: None,
///     },
/// );
///
/// let merged = merge_lockfile_features(&existing, &new);
/// assert_eq!(merged.features.get("feature-a").unwrap().version, "2.0.0");
/// ```
pub fn merge_lockfile_features(existing: &Lockfile, new: &Lockfile) -> Lockfile {
    let mut merged_features = existing.features.clone();

    // Overlay new features (new wins on conflicts)
    for (feature_id, feature_entry) in &new.features {
        merged_features.insert(feature_id.clone(), feature_entry.clone());
    }

    Lockfile {
        features: merged_features,
    }
}

/// Validate lockfile structure and contents
///
/// Checks that all fields contain valid data:
/// - Version fields are valid semver
/// - Resolved fields are valid OCI references
/// - Integrity fields are valid SHA256 digests
/// - Dependency references exist in the lockfile
/// - No circular dependencies
fn validate_lockfile(lockfile: &Lockfile) -> Result<()> {
    for (feature_id, feature) in &lockfile.features {
        // Validate version is valid semver
        validate_semver(&feature.version)
            .with_context(|| format!("Invalid version field for feature '{}'", feature_id))?;

        // Validate resolved is a valid OCI reference
        validate_oci_reference(&feature.resolved)
            .with_context(|| format!("Invalid resolved field for feature '{}'", feature_id))?;

        // Validate integrity is a valid SHA256 digest
        validate_sha256_digest(&feature.integrity)
            .with_context(|| format!("Invalid integrity field for feature '{}'", feature_id))?;

        // Validate dependencies exist in lockfile
        if let Some(deps) = &feature.depends_on {
            for dep in deps {
                if !lockfile.features.contains_key(dep) {
                    anyhow::bail!(
                        "Feature '{}' has dependency '{}' in depends_on field which is not present in the lockfile",
                        feature_id,
                        dep
                    );
                }
            }
        }
    }

    // Check for circular dependencies
    detect_dependency_cycles(lockfile)?;

    Ok(())
}

/// Detect circular dependencies in the lockfile
fn detect_dependency_cycles(lockfile: &Lockfile) -> Result<()> {
    use std::collections::HashSet;

    fn visit(
        feature_id: &str,
        lockfile: &Lockfile,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Result<()> {
        visited.insert(feature_id.to_string());
        rec_stack.insert(feature_id.to_string());
        path.push(feature_id.to_string());

        if let Some(feature) = lockfile.features.get(feature_id) {
            if let Some(deps) = &feature.depends_on {
                for dep in deps {
                    if !visited.contains(dep) {
                        visit(dep, lockfile, visited, rec_stack, path)?;
                    } else if rec_stack.contains(dep) {
                        // Found a cycle
                        path.push(dep.to_string());
                        let cycle_path = path.join(" -> ");
                        anyhow::bail!(
                            "Circular dependency detected in depends_on fields: {}",
                            cycle_path
                        );
                    }
                }
            }
        }

        path.pop();
        rec_stack.remove(feature_id);
        Ok(())
    }

    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();
    let mut path = Vec::new();

    for feature_id in lockfile.features.keys() {
        if !visited.contains(feature_id) {
            visit(
                feature_id,
                lockfile,
                &mut visited,
                &mut rec_stack,
                &mut path,
            )?;
        }
    }

    Ok(())
}

/// Recursively sort all keys in a JSON object for deterministic output
fn sort_json_object(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            // Convert to BTreeMap for sorted keys
            let sorted: std::collections::BTreeMap<_, _> = map.iter().collect();
            *map = sorted
                .into_iter()
                .map(|(k, v)| {
                    let mut v = v.clone();
                    sort_json_object(&mut v);
                    (k.clone(), v)
                })
                .collect();
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                sort_json_object(item);
            }
        }
        _ => {}
    }
}

/// Validate semantic version format
fn validate_semver(version: &str) -> Result<()> {
    // Use semver crate for proper validation
    use semver::Version;

    Version::parse(version).with_context(|| {
        format!(
            "Invalid semantic version '{}': must be in format X.Y.Z (e.g., '1.2.3')",
            version
        )
    })?;

    Ok(())
}

/// Validate OCI reference format
///
/// Basic validation that the reference contains required components
fn validate_oci_reference(reference: &str) -> Result<()> {
    // Must contain @ for digest-based reference
    if !reference.contains('@') {
        anyhow::bail!(
            "OCI reference '{}' must contain '@' separator with digest (expected format: 'registry/path@sha256:...')",
            reference
        );
    }

    // Must contain sha256: in the digest part
    if !reference.contains("sha256:") {
        anyhow::bail!(
            "OCI reference '{}' must contain 'sha256:' digest (expected format: 'registry/path@sha256:...')",
            reference
        );
    }

    Ok(())
}

/// Validate SHA256 digest format
fn validate_sha256_digest(digest: &str) -> Result<()> {
    // Must start with sha256:
    if !digest.starts_with("sha256:") {
        anyhow::bail!(
            "Digest '{}' must start with 'sha256:' (expected format: 'sha256:<64-hex-chars>')",
            digest
        );
    }

    // Extract hash part after sha256:
    let hash = digest.strip_prefix("sha256:").unwrap();

    // Hash should be 64 hex characters
    if hash.len() != 64 {
        anyhow::bail!(
            "SHA256 hash in '{}' must be exactly 64 characters, got {} (expected format: 'sha256:<64-hex-chars>')",
            digest,
            hash.len()
        );
    }

    // All characters should be valid hex
    if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        anyhow::bail!(
            "SHA256 hash in '{}' must contain only hexadecimal characters (0-9, a-f, A-F)",
            digest
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_get_lockfile_path_normal_config() {
        let config = Path::new(".devcontainer/devcontainer.json");
        let lockfile = get_lockfile_path(config);
        assert_eq!(lockfile, Path::new(".devcontainer/devcontainer-lock.json"));
    }

    #[test]
    fn test_get_lockfile_path_hidden_config() {
        let config = Path::new(".devcontainer/.devcontainer.json");
        let lockfile = get_lockfile_path(config);
        assert_eq!(lockfile, Path::new(".devcontainer/.devcontainer-lock.json"));
    }

    #[test]
    fn test_get_lockfile_path_root_directory() {
        let config = Path::new("devcontainer.json");
        let lockfile = get_lockfile_path(config);
        assert_eq!(lockfile, Path::new("devcontainer-lock.json"));
    }

    #[test]
    fn test_get_lockfile_path_hidden_root() {
        let config = Path::new(".devcontainer.json");
        let lockfile = get_lockfile_path(config);
        assert_eq!(lockfile, Path::new(".devcontainer-lock.json"));
    }

    #[test]
    fn test_lockfile_serialization_roundtrip() {
        let mut lockfile = Lockfile {
            features: HashMap::new(),
        };

        lockfile.features.insert(
            "ghcr.io/devcontainers/features/node".to_string(),
            LockfileFeature {
                version: "1.2.3".to_string(),
                resolved: "ghcr.io/devcontainers/features/node@sha256:abc123def456abc123def456abc123def456abc123def456abc123def456abcd".to_string(),
                integrity: "sha256:abc123def456abc123def456abc123def456abc123def456abc123def456abcd".to_string(),
                depends_on: None,
            },
        );

        // Serialize
        let json = serde_json::to_string_pretty(&lockfile).unwrap();

        // Deserialize
        let parsed: Lockfile = serde_json::from_str(&json).unwrap();

        // Verify equality
        assert_eq!(lockfile, parsed);
    }

    #[test]
    fn test_lockfile_with_dependencies() {
        let mut lockfile = Lockfile {
            features: HashMap::new(),
        };

        lockfile.features.insert(
            "feature-a".to_string(),
            LockfileFeature {
                version: "1.0.0".to_string(),
                resolved: "registry/feature-a@sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                integrity: "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                depends_on: Some(vec!["feature-b".to_string()]),
            },
        );

        lockfile.features.insert(
            "feature-b".to_string(),
            LockfileFeature {
                version: "2.0.0".to_string(),
                resolved: "registry/feature-b@sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
                integrity: "sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
                depends_on: None,
            },
        );

        // Validation should pass
        validate_lockfile(&lockfile).unwrap();
    }

    #[test]
    fn test_merge_lockfile_features_basic() {
        let mut existing = Lockfile {
            features: HashMap::new(),
        };
        existing.features.insert(
            "feature-a".to_string(),
            LockfileFeature {
                version: "1.0.0".to_string(),
                resolved: "registry/feature-a@sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                integrity: "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                depends_on: None,
            },
        );

        let mut new = Lockfile {
            features: HashMap::new(),
        };
        new.features.insert(
            "feature-b".to_string(),
            LockfileFeature {
                version: "2.0.0".to_string(),
                resolved: "registry/feature-b@sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
                integrity: "sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
                depends_on: None,
            },
        );

        let merged = merge_lockfile_features(&existing, &new);

        assert_eq!(merged.features.len(), 2);
        assert!(merged.features.contains_key("feature-a"));
        assert!(merged.features.contains_key("feature-b"));
    }

    #[test]
    fn test_merge_lockfile_features_conflict() {
        let mut existing = Lockfile {
            features: HashMap::new(),
        };
        existing.features.insert(
            "feature-a".to_string(),
            LockfileFeature {
                version: "1.0.0".to_string(),
                resolved: "registry/feature-a@sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                integrity: "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                depends_on: None,
            },
        );

        let mut new = Lockfile {
            features: HashMap::new(),
        };
        new.features.insert(
            "feature-a".to_string(),
            LockfileFeature {
                version: "2.0.0".to_string(),
                resolved: "registry/feature-a@sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
                integrity: "sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
                depends_on: None,
            },
        );

        let merged = merge_lockfile_features(&existing, &new);

        // New should win
        assert_eq!(merged.features.len(), 1);
        assert_eq!(merged.features.get("feature-a").unwrap().version, "2.0.0");
    }

    #[test]
    fn test_read_nonexistent_lockfile() {
        let temp_dir = TempDir::new().unwrap();
        let lockfile_path = temp_dir.path().join("nonexistent.json");

        let result = read_lockfile(&lockfile_path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_write_and_read_lockfile() {
        let temp_dir = TempDir::new().unwrap();
        let lockfile_path = temp_dir.path().join("test-lock.json");

        let mut lockfile = Lockfile {
            features: HashMap::new(),
        };
        lockfile.features.insert(
            "test-feature".to_string(),
            LockfileFeature {
                version: "1.0.0".to_string(),
                resolved: "registry/test@sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                integrity: "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                depends_on: None,
            },
        );

        // Write lockfile
        write_lockfile(&lockfile_path, &lockfile, false).unwrap();

        // Read it back
        let read_lockfile = read_lockfile(&lockfile_path).unwrap().unwrap();

        // Verify
        assert_eq!(lockfile, read_lockfile);
    }

    #[test]
    fn test_validate_semver() {
        assert!(validate_semver("1.2.3").is_ok());
        assert!(validate_semver("0.0.1").is_ok());
        assert!(validate_semver("10.20.30").is_ok());
        assert!(validate_semver("1.0.0-alpha").is_ok());
        assert!(validate_semver("1.0.0+build").is_ok());

        assert!(validate_semver("invalid").is_err());
        assert!(validate_semver("1.2").is_err());
        assert!(validate_semver("1").is_err());
    }

    #[test]
    fn test_validate_oci_reference() {
        assert!(validate_oci_reference(
            "ghcr.io/devcontainers/features/node@sha256:abc123def456abc123def456abc123def456abc123def456abc123def456abcd"
        )
        .is_ok());

        assert!(validate_oci_reference(
            "registry/path@sha256:1111111111111111111111111111111111111111111111111111111111111111"
        )
        .is_ok());

        assert!(validate_oci_reference("no-digest").is_err());
        assert!(validate_oci_reference("no-sha@digest:1234").is_err());
    }

    #[test]
    fn test_validate_sha256_digest() {
        assert!(validate_sha256_digest(
            "sha256:1111111111111111111111111111111111111111111111111111111111111111"
        )
        .is_ok());
        assert!(validate_sha256_digest(
            "sha256:abc123def456abc123def456abc123def456abc123def456abc123def456abcd"
        )
        .is_ok());

        assert!(validate_sha256_digest("no-prefix").is_err());
        assert!(validate_sha256_digest("sha256:tooshort").is_err());
        assert!(validate_sha256_digest(
            "sha256:1111111111111111111111111111111111111111111111111111111111111111111"
        )
        .is_err()); // too long
        assert!(validate_sha256_digest(
            "sha256:zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
        )
        .is_err()); // invalid hex
    }

    #[test]
    fn test_validate_lockfile_missing_dependency() {
        let mut lockfile = Lockfile {
            features: HashMap::new(),
        };

        lockfile.features.insert(
            "feature-a".to_string(),
            LockfileFeature {
                version: "1.0.0".to_string(),
                resolved: "registry/feature-a@sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                integrity: "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                depends_on: Some(vec!["missing-feature".to_string()]),
            },
        );

        let result = validate_lockfile(&lockfile);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing-feature"));
    }

    #[test]
    fn test_atomic_write_behavior() {
        let temp_dir = TempDir::new().unwrap();
        let lockfile_path = temp_dir.path().join("atomic-test.json");

        let mut lockfile = Lockfile {
            features: HashMap::new(),
        };
        lockfile.features.insert(
            "test".to_string(),
            LockfileFeature {
                version: "1.0.0".to_string(),
                resolved: "registry/test@sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                integrity: "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                depends_on: None,
            },
        );

        // Write lockfile
        write_lockfile(&lockfile_path, &lockfile, false).unwrap();

        // Verify temp file was cleaned up (new naming: .atomic-test.json.tmp)
        let temp_path = temp_dir.path().join(".atomic-test.json.tmp");
        assert!(!temp_path.exists());

        // Verify final file exists
        assert!(lockfile_path.exists());
    }

    #[test]
    fn test_unicode_handling() {
        let temp_dir = TempDir::new().unwrap();
        let lockfile_path = temp_dir.path().join("unicode-test.json");

        let mut lockfile = Lockfile {
            features: HashMap::new(),
        };
        // Feature ID with unicode characters
        lockfile.features.insert(
            "registry/feature-名前@sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            LockfileFeature {
                version: "1.0.0".to_string(),
                resolved: "registry/feature-名前@sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                integrity: "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                depends_on: None,
            },
        );

        // Write and read back
        write_lockfile(&lockfile_path, &lockfile, false).unwrap();
        let read_back = read_lockfile(&lockfile_path).unwrap().unwrap();

        assert_eq!(lockfile, read_back);
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut lockfile = Lockfile {
            features: HashMap::new(),
        };

        // Create a circular dependency: A -> B -> C -> A
        lockfile.features.insert(
            "feature-a".to_string(),
            LockfileFeature {
                version: "1.0.0".to_string(),
                resolved: "registry/feature-a@sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                integrity: "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                depends_on: Some(vec!["feature-b".to_string()]),
            },
        );

        lockfile.features.insert(
            "feature-b".to_string(),
            LockfileFeature {
                version: "2.0.0".to_string(),
                resolved: "registry/feature-b@sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
                integrity: "sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
                depends_on: Some(vec!["feature-c".to_string()]),
            },
        );

        lockfile.features.insert(
            "feature-c".to_string(),
            LockfileFeature {
                version: "3.0.0".to_string(),
                resolved: "registry/feature-c@sha256:3333333333333333333333333333333333333333333333333333333333333333".to_string(),
                integrity: "sha256:3333333333333333333333333333333333333333333333333333333333333333".to_string(),
                depends_on: Some(vec!["feature-a".to_string()]),
            },
        );

        let result = validate_lockfile(&lockfile);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Circular dependency"));
    }

    #[test]
    fn test_self_referencing_dependency() {
        let mut lockfile = Lockfile {
            features: HashMap::new(),
        };

        // Feature depends on itself
        lockfile.features.insert(
            "feature-a".to_string(),
            LockfileFeature {
                version: "1.0.0".to_string(),
                resolved: "registry/feature-a@sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                integrity: "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                depends_on: Some(vec!["feature-a".to_string()]),
            },
        );

        let result = validate_lockfile(&lockfile);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Circular dependency"));
    }

    #[test]
    fn test_deterministic_json_ordering() {
        let temp_dir = TempDir::new().unwrap();
        let lockfile_path = temp_dir.path().join("ordered-test.json");

        let mut lockfile = Lockfile {
            features: HashMap::new(),
        };

        // Add features in non-alphabetical order
        lockfile.features.insert(
            "z-feature".to_string(),
            LockfileFeature {
                version: "1.0.0".to_string(),
                resolved: "registry/z@sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                integrity: "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                depends_on: None,
            },
        );

        lockfile.features.insert(
            "a-feature".to_string(),
            LockfileFeature {
                version: "2.0.0".to_string(),
                resolved: "registry/a@sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
                integrity: "sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
                depends_on: None,
            },
        );

        lockfile.features.insert(
            "m-feature".to_string(),
            LockfileFeature {
                version: "3.0.0".to_string(),
                resolved: "registry/m@sha256:3333333333333333333333333333333333333333333333333333333333333333".to_string(),
                integrity: "sha256:3333333333333333333333333333333333333333333333333333333333333333".to_string(),
                depends_on: None,
            },
        );

        // Write twice and verify output is identical
        write_lockfile(&lockfile_path, &lockfile, false).unwrap();
        let content1 = std::fs::read_to_string(&lockfile_path).unwrap();

        std::fs::remove_file(&lockfile_path).unwrap();
        write_lockfile(&lockfile_path, &lockfile, false).unwrap();
        let content2 = std::fs::read_to_string(&lockfile_path).unwrap();

        assert_eq!(content1, content2);

        // Verify keys are in alphabetical order in the JSON
        assert!(content1.find("a-feature").unwrap() < content1.find("m-feature").unwrap());
        assert!(content1.find("m-feature").unwrap() < content1.find("z-feature").unwrap());
    }
}
