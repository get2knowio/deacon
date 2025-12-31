//! Shared types and helper functions for features subcommands
//!
//! This module contains common types, utilities, and helpers used across
//! all features subcommands (plan, package, publish, test).

use anyhow::{Context, Result};
use deacon_core::features::{parse_feature_metadata, FeatureMetadata};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Packaging mode for feature detection and processing.
///
/// Determines how feature directories are structured and should be packaged.
/// This enum represents the two supported DevContainer feature organization patterns.
#[derive(Debug, Clone, PartialEq)]
pub enum PackagingMode {
    /// Single feature directory with `devcontainer-feature.json` at root.
    ///
    /// Used when the directory contains a single feature with its metadata file
    /// (`devcontainer-feature.json`) located at the root of the directory.
    /// This is the simplest feature organization pattern.
    Single,
    /// Collection of features under `src/` subdirectories.
    ///
    /// Used when the directory contains multiple features organized as a collection,
    /// where each feature resides in its own subdirectory under `src/`, each containing
    /// its own `devcontainer-feature.json` metadata file. The root directory should
    /// contain a `devcontainer-collection.json` file describing the collection.
    Collection,
}

/// Collection metadata structure for `devcontainer-collection.json`.
///
/// This structure represents the metadata file generated during feature packaging
/// for collections (when multiple features are organized under `src/` subdirectories).
/// The metadata file describes the collection and includes descriptors for each packaged feature.
///
/// # JSON Serialization
///
/// Fields use serde rename attributes to match the expected JSON schema:
/// - `source_information` → `"sourceInformation"` in JSON
///
/// # Examples
///
/// ```no_run
/// use std::collections::BTreeMap;
/// use deacon::commands::features::shared::{CollectionMetadata, SourceInformation, FeatureDescriptor};
///
/// // Create collection metadata for packaging output
/// let mut features = BTreeMap::new();
/// features.insert(
///     "my-feature".to_string(),
///     FeatureDescriptor {
///         id: "my-feature".to_string(),
///         version: "1.0.0".to_string(),
///         name: Some("My Feature".to_string()),
///         description: Some("A sample feature".to_string()),
///         options: None,
///         installs_after: None,
///         depends_on: None,
///     }
/// );
///
/// let collection = CollectionMetadata {
///     source_information: SourceInformation {
///         source: "devcontainer-cli".to_string(),
///     },
///     features,
/// };
///
/// // Serialize to JSON (pretty-printed)
/// let json = serde_json::to_string_pretty(&collection).unwrap();
/// println!("{}", json);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionMetadata {
    /// Source information identifying the tool that generated this collection.
    ///
    /// In JSON: serialized as `"sourceInformation"`.
    /// Typically contains a [`SourceInformation`] with source set to `"devcontainer-cli"`.
    #[serde(rename = "sourceInformation")]
    pub source_information: SourceInformation,

    /// Map of feature descriptors keyed by feature ID.
    ///
    /// Each entry describes a packaged feature in the collection with its metadata
    /// (ID, version, name, description, options, and dependencies).
    /// Keys match the feature IDs (directory names under `src/`).
    pub features: BTreeMap<String, FeatureDescriptor>,
}

/// Source information for collection metadata.
///
/// Identifies the tool or system that generated the collection metadata.
/// This is embedded in [`CollectionMetadata`] to track provenance.
///
/// # Examples
///
/// ```
/// use deacon::commands::features::shared::SourceInformation;
///
/// let source_info = SourceInformation {
///     source: "devcontainer-cli".to_string(),
/// };
///
/// assert_eq!(source_info.source, "devcontainer-cli");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInformation {
    /// Source identifier.
    ///
    /// Always set to `"devcontainer-cli"` for collections packaged by this implementation.
    pub source: String,
}

/// Feature descriptor for collection metadata.
///
/// Describes a single feature within a collection, including its ID, version, name,
/// description, options, and dependency relationships. This structure is used as values
/// in the `features` map of [`CollectionMetadata`].
///
/// # JSON Serialization
///
/// Fields use serde attributes for proper JSON schema compliance:
/// - `installs_after` → `"installsAfter"` in JSON
/// - `depends_on` → `"dependsOn"` in JSON
/// - Optional fields are omitted from JSON when `None` (via `skip_serializing_if`)
///
/// # Examples
///
/// ```
/// use deacon::commands::features::shared::FeatureDescriptor;
///
/// let descriptor = FeatureDescriptor {
///     id: "rust".to_string(),
///     version: "1.0.0".to_string(),
///     name: Some("Rust Toolchain".to_string()),
///     description: Some("Installs Rust and Cargo".to_string()),
///     options: None,
///     installs_after: Some(vec!["common-utils".to_string()]),
///     depends_on: None,
/// };
///
/// // Serialize to verify JSON structure
/// let json = serde_json::to_string_pretty(&descriptor).unwrap();
/// assert!(json.contains("\"installsAfter\""));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureDescriptor {
    /// Feature identifier.
    ///
    /// Must match the feature's directory name and the `id` field in its
    /// `devcontainer-feature.json` metadata file.
    pub id: String,

    /// Feature version.
    ///
    /// Semantic version string for the feature (e.g., `"1.0.0"`).
    pub version: String,

    /// Optional human-readable feature name.
    ///
    /// Omitted from JSON serialization if `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Optional feature description.
    ///
    /// Brief explanation of what the feature does. Omitted from JSON if `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional feature options/configuration schema.
    ///
    /// JSON value representing the feature's configurable options.
    /// Omitted from JSON serialization if `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,

    /// Optional installation order dependencies.
    ///
    /// In JSON: serialized as `"installsAfter"`.
    /// List of feature IDs that must be installed before this feature.
    /// Omitted from JSON if `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "installsAfter")]
    pub installs_after: Option<Vec<String>>,

    /// Optional hard dependencies.
    ///
    /// In JSON: serialized as `"dependsOn"`.
    /// Map of feature IDs to configuration options that this feature requires.
    /// Omitted from JSON if `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "dependsOn")]
    pub depends_on: Option<serde_json::Value>,
}

/// Sanitize a feature ID for use in artifact names
///
/// Maps invalid characters to `-`, collapses repeats, trims leading/trailing hyphens.
/// Only allows `[a-z0-9-]` characters (underscores are converted to hyphens).
/// Returns an error if the result would be empty.
#[allow(dead_code)]
pub(crate) fn sanitize_feature_id(feature_id: &str) -> Result<String> {
    // Replace invalid characters with hyphens (including underscores)
    let sanitized = feature_id
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();

    // Collapse multiple consecutive hyphens
    let collapsed = sanitized
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<&str>>()
        .join("-");

    // Trim leading/trailing hyphens
    let trimmed = collapsed.trim_matches('-');

    if trimmed.is_empty() {
        return Err(anyhow::anyhow!(
            "Feature ID '{}' results in empty sanitized name",
            feature_id
        ));
    }

    Ok(trimmed.to_string())
}

/// Build artifact name for a feature package
///
/// Format: `<featureId>-<version>.tgz` where featureId is sanitized.
/// Returns an error if version is missing or sanitization results in empty string.
#[allow(dead_code)]
pub(crate) fn build_artifact_name(feature_id: &str, version: &Option<String>) -> Result<String> {
    let version = version.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "Feature '{}' is missing required version for artifact naming",
            feature_id
        )
    })?;

    let sanitized_id = sanitize_feature_id(feature_id)?;
    Ok(format!("{}-{}.tgz", sanitized_id, version))
}

/// Detect packaging mode for a given path
///
/// Returns PackagingMode::Single if devcontainer-feature.json exists at root,
/// PackagingMode::Collection if src/ directory exists with feature subdirectories.
/// Fails if neither condition is met.
#[allow(dead_code)]
pub(crate) fn detect_mode(target: &Path) -> Result<PackagingMode> {
    // Check for single feature mode: devcontainer-feature.json at root
    let feature_json = target.join("devcontainer-feature.json");
    if feature_json.exists() && feature_json.is_file() {
        return Ok(PackagingMode::Single);
    }

    // Check for collection mode: src/ directory exists
    let src_dir = target.join("src");
    if src_dir.exists() && src_dir.is_dir() {
        return Ok(PackagingMode::Collection);
    }

    // Neither condition met - cannot determine mode
    Err(anyhow::anyhow!(
        "Cannot determine packaging mode for path '{}'. Expected either:\n\
         - devcontainer-feature.json at root (single feature mode)\n\
         - src/ directory with feature subdirectories (collection mode)",
        target.display()
    ))
}

/// Validates a single feature directory and returns its parsed metadata.
///
/// This function performs comprehensive validation of a feature directory:
/// - Verifies that `devcontainer-feature.json` exists in the target directory
/// - Confirms the metadata file is a regular file (not a directory or symlink)
/// - Parses and validates the JSON metadata structure
/// - Ensures required fields (particularly `version`) are present for packaging
///
/// # Arguments
///
/// * `target` - Path to the feature directory to validate
///
/// # Returns
///
/// * `Ok(FeatureMetadata)` - Parsed and validated feature metadata
/// * `Err(_)` - If the metadata file is missing, invalid, or lacks required fields
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use deacon::commands::features::shared::validate_single;
///
/// let feature_path = Path::new("/path/to/feature");
/// match validate_single(feature_path) {
///     Ok(metadata) => {
///         println!("Feature '{}' validated successfully", metadata.id);
///         println!("Version: {:?}", metadata.version);
///     }
///     Err(e) => eprintln!("Validation failed: {}", e),
/// }
/// ```
#[allow(dead_code)]
pub fn validate_single(target: &Path) -> Result<FeatureMetadata> {
    let feature_json = target.join("devcontainer-feature.json");

    // Check file exists
    if !feature_json.exists() {
        return Err(anyhow::anyhow!(
            "devcontainer-feature.json not found in '{}'",
            target.display()
        ));
    }

    // Check it's a file (not a directory)
    if !feature_json.is_file() {
        return Err(anyhow::anyhow!(
            "devcontainer-feature.json exists but is not a file in '{}'",
            target.display()
        ));
    }

    // Parse and validate metadata
    let metadata = parse_feature_metadata(&feature_json).with_context(|| {
        format!(
            "Failed to parse devcontainer-feature.json in '{}'",
            target.display()
        )
    })?;

    // Validate required fields for packaging
    if metadata.version.is_none() {
        return Err(anyhow::anyhow!(
            "Feature '{}' is missing required 'version' field in devcontainer-feature.json",
            metadata.id
        ));
    }

    Ok(metadata)
}

/// Enumerate and validate all features in a collection directory.
///
/// Scans the `src/` directory for valid feature subdirectories, validates each feature's
/// structure and metadata, and returns a vector of `(feature_id, path, metadata)` tuples.
///
/// # Arguments
///
/// * `src` - Path to the `src/` directory containing feature subdirectories
///
/// # Returns
///
/// * `Ok(Vec<(String, PathBuf, FeatureMetadata)>)` - Sorted vector of validated features, each containing:
///   - Feature ID (directory name)
///   - Full path to the feature directory
///   - Parsed feature metadata from `devcontainer-feature.json`
///
/// # Errors
///
/// Returns an error if:
/// - The `src/` directory does not exist or is not a directory
/// - Any subdirectory is invalid (missing metadata, malformed JSON, etc.)
/// - Feature ID in `devcontainer-feature.json` doesn't match directory name
/// - No valid features are found in the collection
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// # use anyhow::Result;
/// # fn main() -> Result<()> {
/// let src_dir = Path::new("my-collection/src");
/// let features = deacon::commands::features::shared::enumerate_and_validate_collection(src_dir)?;
///
/// for (feature_id, path, metadata) in features {
///     println!("Found feature: {} at {:?}", feature_id, path);
///     if let Some(version) = &metadata.version {
///         println!("  Version: {}", version);
///     }
/// }
/// # Ok(())
/// # }
/// ```
#[allow(dead_code)]
pub fn enumerate_and_validate_collection(
    src: &Path,
) -> Result<Vec<(String, PathBuf, FeatureMetadata)>> {
    // Check that src directory exists
    if !src.exists() {
        return Err(anyhow::anyhow!(
            "src/ directory not found in '{}'",
            src.display()
        ));
    }

    if !src.is_dir() {
        return Err(anyhow::anyhow!(
            "src/ exists but is not a directory in '{}'",
            src.display()
        ));
    }

    // Read directory entries
    let entries = std::fs::read_dir(src)
        .with_context(|| format!("Failed to read src/ directory '{}'", src.display()))?;

    let mut features = Vec::new();
    let mut invalid_features = Vec::new();

    for entry in entries {
        let entry = entry
            .with_context(|| format!("Failed to read directory entry in '{}'", src.display()))?;
        let path = entry.path();

        // Skip non-directories
        if !path.is_dir() {
            continue;
        }

        // Get feature ID from directory name
        let feature_id = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => {
                invalid_features.push(format!("Invalid directory name: '{}'", path.display()));
                continue;
            }
        };

        // Validate the feature
        match validate_single(&path) {
            Ok(metadata) => {
                // Verify feature ID matches directory name
                if metadata.id != feature_id {
                    invalid_features.push(format!(
                        "Feature ID '{}' in devcontainer-feature.json does not match directory name '{}' in '{}'",
                        metadata.id, feature_id, path.display()
                    ));
                    continue;
                }
                features.push((feature_id, path, metadata));
            }
            Err(e) => {
                invalid_features.push(format!("{}: {}", path.display(), e));
                continue;
            }
        }
    }

    // If any features were invalid, fail the entire operation
    if !invalid_features.is_empty() {
        return Err(anyhow::anyhow!(
            "Found {} invalid feature(s) in collection:\n{}",
            invalid_features.len(),
            invalid_features.join("\n")
        ));
    }

    // Check if collection is empty
    if features.is_empty() {
        return Err(anyhow::anyhow!(
            "No valid features found in src/ directory '{}'",
            src.display()
        ));
    }

    // Sort features by ID for deterministic ordering
    features.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(features)
}

/// Write collection metadata to a JSON file.
///
/// Serializes the provided [`CollectionMetadata`] to pretty-printed JSON and writes it
/// to the specified destination path. If the parent directory does not exist, it will
/// be created automatically.
///
/// # Parameters
///
/// * `metadata` - The collection metadata to serialize and write
/// * `dest` - The destination path where the JSON file will be written
///
/// # Returns
///
/// Returns `Ok(())` on success, or an error if:
/// - The parent directory cannot be created
/// - The metadata cannot be serialized to JSON
/// - The file cannot be written to disk
///
/// # Errors
///
/// This function will return an error if:
/// - Parent directory creation fails due to permissions or I/O errors
/// - JSON serialization fails (unlikely with valid CollectionMetadata)
/// - File write fails due to permissions, disk space, or I/O errors
#[allow(dead_code)]
pub fn write_collection_metadata(metadata: &CollectionMetadata, dest: &Path) -> Result<()> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory '{}'", parent.display()))?;
    }

    // Serialize to JSON with pretty printing
    let json = serde_json::to_string_pretty(metadata)
        .with_context(|| "Failed to serialize collection metadata to JSON")?;

    // Write to file
    std::fs::write(dest, json).with_context(|| {
        format!(
            "Failed to write collection metadata to '{}'",
            dest.display()
        )
    })?;

    Ok(())
}

/// Create a .tgz archive from a feature directory and return SHA256 digest
///
/// Creates a compressed tar archive from the source directory, writing it to dest.
/// Returns the SHA256 digest of the archive content.
/// The archive will contain the feature files with paths relative to the source directory.
///
/// # Archive Structure
/// The tar archive contains all files from the source directory with relative paths.
/// For example, if source is `/path/to/feature/` containing `install.sh` and `devcontainer-feature.json`,
/// the archive will contain `install.sh` and `devcontainer-feature.json` at the root level.
///
/// # Compression
/// Uses gzip compression with default compression level for balance of speed and size.
///
/// # Determinism
/// This function uses deterministic settings for reproducible builds:
/// - Sets consistent mtime (0) for all tar headers
/// - Sorts files lexicographically
/// - Uses deterministic gzip settings
#[allow(dead_code)]
pub fn create_feature_tgz(src: &Path, dest: &Path) -> Result<String> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use sha2::{Digest, Sha256};
    use std::io::Write;

    // Create parent directory if it doesn't exist
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory '{}'", parent.display()))?;
    }

    // Create the output file
    let file = std::fs::File::create(dest)
        .with_context(|| format!("Failed to create archive file '{}'", dest.display()))?;

    // Create gzip encoder with deterministic settings
    let encoder = flate2::GzBuilder::new()
        .mtime(0) // Deterministic: Unix epoch
        .write(file, Compression::default());

    // Create tar builder
    let mut tar_builder = tar::Builder::new(encoder);

    // Add all files from the source directory
    // We need to walk the directory and add files with relative paths
    fn add_files_to_tar(
        tar_builder: &mut tar::Builder<GzEncoder<std::fs::File>>,
        src: &Path,
        base_path: &Path,
    ) -> Result<()> {
        // Collect all entries first to sort them lexicographically
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(src)
            .with_context(|| format!("Failed to read directory '{}'", src.display()))?
        {
            let entry = entry.with_context(|| {
                format!("Failed to read directory entry in '{}'", src.display())
            })?;
            entries.push(entry);
        }

        // Sort entries lexicographically by file name for deterministic ordering
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let path = entry.path();

            // Get relative path from base
            let relative_path = path.strip_prefix(base_path).with_context(|| {
                format!(
                    "Failed to make path '{}' relative to '{}'",
                    path.display(),
                    base_path.display()
                )
            })?;

            if path.is_dir() {
                // Add directory entry with mode 0o755 before recursing
                // Ensure directory path ends with / for tar convention
                let dir_path = if relative_path.as_os_str().is_empty() {
                    PathBuf::from("./")
                } else {
                    let p = relative_path.to_path_buf();
                    let path_str = p.to_string_lossy().to_string();
                    if !path_str.ends_with('/') {
                        PathBuf::from(format!("{}/", path_str))
                    } else {
                        p
                    }
                };

                let mut header = tar::Header::new_gnu();
                header.set_path(&dir_path)?;
                header.set_entry_type(tar::EntryType::Directory);
                header.set_size(0);
                header.set_mode(0o755); // Deterministic: directory permissions
                header.set_mtime(0); // Deterministic: Unix epoch
                header.set_uid(0); // Deterministic: root user
                header.set_gid(0); // Deterministic: root group
                header.set_username("")?; // Deterministic: empty username
                header.set_groupname("")?; // Deterministic: empty groupname
                header.set_cksum();

                tar_builder
                    .append(&header, &mut std::io::empty())
                    .with_context(|| {
                        format!("Failed to add directory '{}' to archive", path.display())
                    })?;

                // Recursively add directory contents
                add_files_to_tar(tar_builder, &path, base_path)?;
            } else {
                // Add file to archive with proper metadata
                let mut file = std::fs::File::open(&path).with_context(|| {
                    format!("Failed to open file '{}' for archiving", path.display())
                })?;

                let metadata = file
                    .metadata()
                    .with_context(|| format!("Failed to get metadata for '{}'", path.display()))?;

                let mut header = tar::Header::new_gnu();
                header.set_path(relative_path)?;
                header.set_size(metadata.len());

                // Detect executable files on Unix and set appropriate mode
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = metadata.permissions().mode();
                    // Check if any executable bit is set (owner, group, or other)
                    if mode & 0o111 != 0 {
                        header.set_mode(0o755); // Executable: rwxr-xr-x
                    } else {
                        header.set_mode(0o644); // Regular file: rw-r--r--
                    }
                }
                #[cfg(not(unix))]
                {
                    header.set_mode(0o644); // Default to regular file on non-Unix
                }

                header.set_mtime(0); // Deterministic: Unix epoch
                header.set_uid(0); // Deterministic: root user
                header.set_gid(0); // Deterministic: root group
                header.set_username("")?; // Deterministic: empty username
                header.set_groupname("")?; // Deterministic: empty groupname
                header.set_cksum();

                tar_builder.append(&header, &mut file).with_context(|| {
                    format!("Failed to add file '{}' to archive", path.display())
                })?;
            }
        }

        Ok(())
    }

    // Add all files from the source directory
    add_files_to_tar(&mut tar_builder, src, src)?;

    // Finish the tar archive
    let encoder = tar_builder
        .into_inner()
        .with_context(|| "Failed to finish tar archive creation")?;

    // Finish gzip compression and get the file back
    let mut file = encoder
        .finish()
        .with_context(|| "Failed to finish gzip compression")?;

    // Calculate SHA256 digest by reading the entire file
    file.flush()
        .with_context(|| "Failed to flush archive file")?;
    file.sync_all()
        .with_context(|| "Failed to sync archive file")?;

    // Re-open file to calculate digest
    let mut file = std::fs::File::open(dest).with_context(|| {
        format!(
            "Failed to re-open archive file '{}' for digest calculation",
            dest.display()
        )
    })?;

    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)
        .with_context(|| format!("Failed to calculate SHA256 digest for '{}'", dest.display()))?;

    let digest = hasher.finalize();
    let digest_hex = format!("sha256:{:x}", digest);

    Ok(digest_hex)
}
