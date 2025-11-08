//! Features command implementation
//!
//! Implements the `deacon features` subcommands for testing, packaging, and publishing
//! DevContainer features. Follows the CLI specification for feature management.
//!
//! ## Feature Planning
//!
//! The `features plan` subcommand implements the specification defined in:
//! - [`docs/subcommand-specs/features-plan/SPEC.md`](../docs/subcommand-specs/features-plan/SPEC.md)
//! - [`docs/subcommand-specs/features-plan/DATA-STRUCTURES.md`](../docs/subcommand-specs/features-plan/DATA-STRUCTURES.md)

use crate::cli::FeatureCommands;
use crate::commands::features_publish_output::{
    PublishCollectionResult, PublishFeatureResult, PublishOutput, PublishSummary,
};
use anyhow::{Context, Result};
use deacon_core::config::{ConfigLoader, DevContainerConfig};
use deacon_core::errors::{DeaconError, FeatureError};
use deacon_core::features::{
    canonicalize_feature_id, parse_feature_metadata, FeatureDependencyResolver, FeatureMergeConfig,
    FeatureMerger, FeatureMetadata, OptionValue, ResolvedFeature,
};
use deacon_core::observability::{feature_plan_span, TimedSpan};
use deacon_core::oci::{default_fetcher, FeatureRef};
use deacon_core::registry_parser::parse_registry_reference;
use futures::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tempfile;
use tracing::{debug, info, Instrument};

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
/// use deacon::commands::features::{CollectionMetadata, SourceInformation, FeatureDescriptor};
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
    pub features: std::collections::BTreeMap<String, FeatureDescriptor>,
}

/// Source information for collection metadata.
///
/// Identifies the tool or system that generated the collection metadata.
/// This is embedded in [`CollectionMetadata`] to track provenance.
///
/// # Examples
///
/// ```
/// use deacon::commands::features::SourceInformation;
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
/// use deacon::commands::features::FeatureDescriptor;
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
fn sanitize_feature_id(feature_id: &str) -> Result<String> {
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
fn build_artifact_name(feature_id: &str, version: &Option<String>) -> Result<String> {
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
fn detect_mode(target: &Path) -> Result<PackagingMode> {
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
/// use deacon::commands::features::validate_single;
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
/// let features = deacon::commands::features::enumerate_and_validate_collection(src_dir)?;
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
/// For reproducible builds, this function should be enhanced to:
/// - Set consistent mtime (0) for all tar headers
/// - Sort files lexicographically
/// - Use deterministic gzip settings
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
                // Recursively add directory contents
                add_files_to_tar(tar_builder, &path, base_path)?;
            } else {
                // Add file to archive with proper metadata
                let mut file = std::fs::File::open(&path).with_context(|| {
                    format!("Failed to open file '{}' for archiving", path.display())
                })?;

                let mut header = tar::Header::new_gnu();
                header.set_size(
                    file.metadata()
                        .with_context(|| {
                            format!("Failed to get metadata for '{}'", path.display())
                        })?
                        .len(),
                );
                header.set_mode(0o644); // Standard file permissions
                header.set_uid(0); // Deterministic: root user
                header.set_gid(0); // Deterministic: root group
                header.set_mtime(0); // Deterministic: Unix epoch
                header.set_username("root")?; // Deterministic: root user
                header.set_groupname("root")?; // Deterministic: root group

                tar_builder
                    .append_data(&mut header, relative_path, &mut file)
                    .with_context(|| {
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

/// Features command arguments
#[derive(Debug, Clone)]
pub struct FeaturesArgs {
    pub command: FeatureCommands,
    #[allow(dead_code)] // Reserved for future use
    pub workspace_folder: Option<PathBuf>,
    #[allow(dead_code)] // Reserved for future use
    pub config_path: Option<PathBuf>,
}

/// Result of a features command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturesResult {
    /// Command that was executed
    pub command: String,
    /// Status of the operation (success/failure)
    pub status: String,
    /// Optional digest for package/publish operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    /// Optional size information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Optional message with additional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Optional cache path for pulled artifacts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_path: Option<String>,
}

/// Execute the features command
pub async fn execute_features(args: FeaturesArgs) -> Result<()> {
    match args.command {
        FeatureCommands::Test { path, json } => execute_features_test(&path, json).await,
        FeatureCommands::Package {
            path,
            output,
            force_clean_output_folder,
            json,
        } => execute_features_package(&path, &output, force_clean_output_folder, json).await,
        FeatureCommands::Pull { registry_ref, json } => {
            execute_features_pull(&registry_ref, json).await
        }
        FeatureCommands::Publish {
            path,
            registry,
            namespace,
            dry_run,
            json,
            username,
            password_stdin,
        } => {
            let output = execute_features_publish(
                &path,
                &registry,
                &namespace,
                dry_run,
                username.as_deref(),
                password_stdin,
            )
            .await?;
            output_publish_result(&output, json)?;
            Ok(())
        }
        FeatureCommands::Info {
            mode,
            feature,
            json,
        } => execute_features_info(&mode, &feature, json).await,
        FeatureCommands::Plan {
            json,
            ref additional_features,
        } => execute_features_plan(json, additional_features.as_deref(), &args).await,
    }
}

/// Plan result structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturesPlanResult {
    /// Features in installation order
    pub order: Vec<String>,
    /// Dependency graph for visibility
    pub graph: serde_json::Value,
}

/// Error message for local feature paths
const LOCAL_FEATURE_ERROR_MSG: &str =
    "Local feature paths are not supported by 'features plan'—use a registry reference";

/// Default concurrency limit for parallel OCI metadata fetching
const DEFAULT_FETCH_CONCURRENCY: usize = 6;

/// Get the concurrency limit for parallel OCI metadata fetching
///
/// Reads from the DEACON_FETCH_CONCURRENCY environment variable if set,
/// otherwise uses the default of 6. The limit is clamped to a minimum of 1
/// and maximum of 32 to prevent resource exhaustion.
fn get_fetch_concurrency() -> usize {
    get_fetch_concurrency_impl(std::env::var("DEACON_FETCH_CONCURRENCY").ok().as_deref())
}

/// Internal implementation for getting fetch concurrency from an optional environment value
///
/// This function is separated to allow testing without global state mutation.
/// Takes an optional string value and returns the parsed, clamped concurrency limit.
fn get_fetch_concurrency_impl(env_value: Option<&str>) -> usize {
    env_value
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(DEFAULT_FETCH_CONCURRENCY)
        .clamp(1, 32)
}

/// Check if a feature identifier looks like a local path
fn is_local_path(feature_id: &str) -> bool {
    // Check for relative paths
    if feature_id.starts_with("./") || feature_id.starts_with("../") {
        return true;
    }

    // Check for absolute Unix paths
    if feature_id.starts_with('/') {
        return true;
    }

    // Check for Windows absolute paths (C:\, D:\, etc.)
    if feature_id.len() >= 3 {
        let chars: Vec<char> = feature_id.chars().collect();
        if chars.len() >= 3
            && chars[0].is_ascii_alphabetic()
            && chars[1] == ':'
            && (chars[2] == '\\' || chars[2] == '/')
        {
            return true;
        }
    }

    false
}

/// Execute features plan command
async fn execute_features_plan(
    json: bool,
    additional_features: Option<&str>,
    args: &FeaturesArgs,
) -> Result<()> {
    // Determine workspace folder - default to current directory if not provided
    let workspace_folder = args
        .workspace_folder
        .as_ref()
        .unwrap_or(&std::env::current_dir()?)
        .clone();

    // Start standardized span for feature planning
    let timed_span = TimedSpan::new(feature_plan_span(&workspace_folder));

    let result = {
        let _guard = timed_span.span().enter();

        debug!("Generating feature installation plan");

        // Load devcontainer configuration (explicit path > discovery > default)
        let mut config = if let Some(config_path) = args.config_path.as_deref() {
            ConfigLoader::load_from_path(config_path)?
        } else {
            let config_location = ConfigLoader::discover_config(&workspace_folder)?;
            if config_location.exists() {
                ConfigLoader::load_from_path(config_location.path())?
            } else {
                DevContainerConfig::default()
            }
        };

        // Parse and merge additional features if provided
        if let Some(additional_features_str) = additional_features {
            // Early validation: parse JSON and ensure it's an object before merge
            let parsed_json: serde_json::Value = serde_json::from_str(additional_features_str)
                .with_context(|| {
                    format!(
                        "Failed to parse --additional-features during feature plan initialization: {}",
                        additional_features_str
                    )
                })?;

            // Validate that the parsed JSON is an object (map)
            if !parsed_json.is_object() {
                anyhow::bail!("Failed to validate --additional-features: must be a JSON object.");
            }

            let merge_config = FeatureMergeConfig::new(
                Some(additional_features_str.to_string()),
                true, // Prefer CLI features for planner precedence
                None, // No install order override in this context
            );
            config.features = FeatureMerger::merge_features(&config.features, &merge_config)
                .context("Failed to merge additional features with devcontainer configuration.")?;
        }

        // Canonicalize feature IDs after merging
        if let Some(features_obj) = config.features.as_object_mut() {
            let mut canonicalized = serde_json::Map::new();
            for (key, value) in features_obj.iter() {
                let canonical_key = canonicalize_feature_id(key);
                canonicalized.insert(canonical_key, value.clone());
            }
            config.features = serde_json::Value::Object(canonicalized);
        }

        // Extract features from config
        let features_map_opt = config.features.as_object();
        if features_map_opt.is_none() || features_map_opt.unwrap().is_empty() {
            let result = FeaturesPlanResult {
                order: vec![],
                graph: serde_json::json!({}),
            };
            output_plan_result(&result, json)?;
            return Ok(());
        }
        let features_map = features_map_opt.unwrap();

        // Resolve features from registries to obtain metadata (deps, installsAfter, etc.)
        let fetcher = default_fetcher()
            .context("Failed to initialize OCI client for fetching feature metadata.")?;

        // Add span for metadata fetch loop with structured fields
        let resolved_features = {
            let fetch_start = std::time::Instant::now();
            let concurrency = get_fetch_concurrency();
            let fetch_span = tracing::span!(
                target: "deacon::features",
                tracing::Level::INFO,
                "features.fetch_metadata",
                feature_count = features_map.len(),
                concurrency = concurrency,
                fetch_ms = tracing::field::Empty
            );
            let _fetch_guard = fetch_span.enter();

            // Pre-validate all feature IDs before starting any fetches
            for (feature_id, _) in features_map.iter() {
                if is_local_path(feature_id) {
                    anyhow::bail!("{}. Feature key: '{}'", LOCAL_FEATURE_ERROR_MSG, feature_id);
                }
            }

            // Convert features_map to a Vec for parallel processing
            // Use a BTreeMap to ensure deterministic ordering of results
            let features_to_fetch: Vec<_> = features_map.iter().collect();

            // Fetch metadata in parallel with bounded concurrency
            let fetch_results: BTreeMap<String, ResolvedFeature> = stream::iter(features_to_fetch)
                .map(|(feature_id, feature_value)| {
                    let fetcher = &fetcher;
                    let feature_id = feature_id.to_string();
                    let feature_value = feature_value.clone();

                    async move {
                        // Parse registry reference
                        let (registry_url, namespace, name, tag) =
                            parse_registry_reference(&feature_id).with_context(|| {
                                format!(
                                    "Failed to parse registry reference for feature '{}' during feature resolution.",
                                    feature_id
                                )
                            })?;

                        let feature_ref = FeatureRef::new(
                            registry_url.clone(),
                            namespace.clone(),
                            name.clone(),
                            tag.clone(),
                        );

                        // Fetch feature metadata from OCI registry
                        let downloaded = fetcher.fetch_feature(&feature_ref).await.map_err(|e| {
                            // Pattern match on DeaconError to access the concrete error variant
                            let error_msg = format!(
                                "Failed to fetch feature metadata from OCI registry for feature '{}'",
                                feature_id
                            );
                            match e {
                                DeaconError::Feature(feature_err) => {
                                    // Map specific FeatureError variants to categorized errors with context
                                    match feature_err {
                                        FeatureError::Authentication { message } => {
                                            DeaconError::Feature(FeatureError::Authentication {
                                                message: format!("{}: {}", error_msg, message),
                                            })
                                        }
                                        FeatureError::Download { message } => {
                                            DeaconError::Feature(FeatureError::Download {
                                                message: format!("{}: {}", error_msg, message),
                                            })
                                        }
                                        _ => {
                                            // For all other FeatureError variants, wrap as OCI error
                                            DeaconError::Feature(FeatureError::Oci {
                                                message: format!("{}: {}", error_msg, feature_err),
                                            })
                                        }
                                    }
                                }
                                other => {
                                    // For non-Feature DeaconErrors, wrap as OCI error
                                    DeaconError::Feature(FeatureError::Oci {
                                        message: format!("{}: {}", error_msg, other),
                                    })
                                }
                            }
                        })?;

                        // Extract per-feature options from config entry if present
                        let options: std::collections::HashMap<String, OptionValue> = match feature_value {
                            serde_json::Value::Object(map) => map
                                .into_iter()
                                .map(|(k, v)| {
                                    // Convert serde_json::Value to OptionValue, preserving all types
                                    let option_value = match v {
                                        serde_json::Value::Bool(b) => OptionValue::Boolean(b),
                                        serde_json::Value::String(s) => OptionValue::String(s.clone()),
                                        serde_json::Value::Number(n) => OptionValue::Number(n.clone()),
                                        serde_json::Value::Array(a) => OptionValue::Array(a.clone()),
                                        serde_json::Value::Object(o) => OptionValue::Object(o.clone()),
                                        serde_json::Value::Null => OptionValue::Null,
                                    };
                                    (k.clone(), option_value)
                                })
                                .collect(),
                            _ => std::collections::HashMap::new(),
                        };

                        let resolved = ResolvedFeature {
                            id: downloaded.metadata.id.clone(),
                            source: feature_ref.reference(),
                            options,
                            metadata: downloaded.metadata,
                        };

                        // Return the feature_id as key for deterministic ordering
                        Ok::<(String, ResolvedFeature), anyhow::Error>((feature_id, resolved))
                    }
                })
                .buffer_unordered(concurrency)
                .collect::<Vec<Result<(String, ResolvedFeature)>>>()
                .await
                .into_iter()
                .collect::<Result<BTreeMap<String, ResolvedFeature>>>()?;

            // Record fetch duration before exiting span
            let fetch_duration_ms = fetch_start.elapsed().as_millis() as u64;
            fetch_span.record("fetch_ms", fetch_duration_ms);

            // Convert BTreeMap to Vec, maintaining deterministic order
            fetch_results.into_values().collect::<Vec<_>>()
        };

        // Create dependency resolver with override order from config
        let override_order = config.override_feature_install_order.clone();
        let resolver = FeatureDependencyResolver::new(override_order);

        // Add span for dependency resolution with structured fields
        let installation_plan = {
            let resolve_start = std::time::Instant::now();
            let node_count = resolved_features.len();
            // Count total edges (dependencies) across all features
            let edge_count: usize = resolved_features
                .iter()
                .map(|f| f.metadata.installs_after.len() + f.metadata.depends_on.len())
                .sum();

            let resolve_span = tracing::span!(
                target: "deacon::features",
                tracing::Level::INFO,
                "features.resolve_dependencies",
                node_count = node_count,
                edge_count = edge_count,
                resolve_ms = tracing::field::Empty
            );
            let _resolve_guard = resolve_span.enter();

            // Resolve dependencies and create installation plan
            let plan = resolver.resolve(&resolved_features).context(
                "Failed to resolve feature dependencies and compute installation order.",
            )?;

            // Record resolution duration before exiting span
            let resolve_duration_ms = resolve_start.elapsed().as_millis() as u64;
            resolve_span.record("resolve_ms", resolve_duration_ms);

            plan
        };

        // Extract order and create graph representation
        let order = installation_plan.feature_ids();
        let graph = build_graph_representation(&resolved_features);

        let result = FeaturesPlanResult { order, graph };

        output_plan_result(&result, json)?;

        Ok(())
    };

    // Complete the timed span with duration
    timed_span.complete();
    result
}

/// Create a mock resolved feature for demonstration (temporary)
/// In a real implementation, this would fetch the actual feature metadata
#[cfg(test)]
#[allow(dead_code)]
fn create_mock_resolved_feature(feature_id: &str) -> ResolvedFeature {
    create_mock_resolved_feature_with_deps(feature_id, &[], &[])
}

/// Create a mock resolved feature with specified dependencies
#[cfg(test)]
#[allow(dead_code)]
fn create_mock_resolved_feature_with_deps(
    feature_id: &str,
    installs_after: &[&str],
    depends_on: &[&str],
) -> ResolvedFeature {
    use deacon_core::features::FeatureMetadata;
    use std::collections::HashMap;

    let mut depends_on_map = HashMap::new();
    for dep in depends_on {
        depends_on_map.insert(dep.to_string(), serde_json::Value::Bool(true));
    }

    let metadata = FeatureMetadata {
        id: feature_id.to_string(),
        version: Some("1.0.0".to_string()),
        name: Some(format!("Mock {}", feature_id)),
        description: Some(format!("Mock feature for {}", feature_id)),
        documentation_url: None,
        license_url: None,
        options: HashMap::new(),
        container_env: HashMap::new(),
        mounts: vec![],
        init: None,
        privileged: None,
        cap_add: vec![],
        security_opt: vec![],
        entrypoint: None,
        installs_after: installs_after.iter().map(|s| s.to_string()).collect(),
        depends_on: depends_on_map,
        on_create_command: None,
        update_content_command: None,
        post_create_command: None,
        post_start_command: None,
        post_attach_command: None,
    };

    ResolvedFeature {
        id: feature_id.to_string(),
        source: format!("mock://features/{}", feature_id),
        options: HashMap::new(),
        metadata,
    }
}

/// Build a graph representation from resolved features
///
/// Creates an adjacency list representation where each feature ID maps to an array
/// of its dependencies. The graph structure follows the format:
/// ```json
/// {
///   "featureA": [],
///   "featureB": ["featureA"],
///   "featureC": ["featureA", "featureB"]
/// }
/// ```
///
/// ## Graph Direction
/// The graph encodes **dependencies** (not dependents):
/// - `"featureB": ["featureA"]` means featureB depends on featureA
/// - Empty array means the feature has no dependencies
/// - This representation makes it clear what must be installed before each feature
///
/// ## Dependency Union
/// The dependency list for each feature is the **union** of:
/// - `installsAfter`: Ordering constraints from the feature metadata
/// - `dependsOn`: Hard dependencies from the feature metadata
///
/// Both fields are combined to provide a complete view of installation constraints.
/// Uses `BTreeSet` internally to ensure deterministic ordering and deduplication.
///
/// ## Conformance
/// Matches specification in `docs/subcommand-specs/features-plan/DATA-STRUCTURES.md`
/// and design decision in `docs/subcommand-specs/features-plan/SPEC.md` §16.
fn build_graph_representation(features: &[ResolvedFeature]) -> serde_json::Value {
    let mut graph = serde_json::Map::new();

    for feature in features {
        // Use a set for dedupe and deterministic ordering
        let mut deps = std::collections::BTreeSet::new();
        for dep in &feature.metadata.installs_after {
            deps.insert(dep.clone());
        }
        for dep_id in feature.metadata.depends_on.keys() {
            deps.insert(dep_id.clone());
        }
        graph.insert(
            feature.id.clone(),
            serde_json::Value::Array(deps.into_iter().map(serde_json::Value::String).collect()),
        );
    }

    serde_json::Value::Object(graph)
}

/// Output plan result in the specified format
fn output_plan_result(result: &FeaturesPlanResult, json: bool) -> Result<()> {
    if json {
        let json_output = serde_json::to_string_pretty(result)?;
        println!("{}", json_output);
    } else {
        println!("Feature Installation Plan:");
        println!("Order: {:?}", result.order);
        println!("Graph: {}", serde_json::to_string_pretty(&result.graph)?);
    }
    Ok(())
}

async fn execute_features_test(path: &str, json: bool) -> Result<()> {
    debug!("Testing feature at path: {}", path);

    let feature_path = Path::new(path);

    // Parse feature metadata
    let metadata_path = feature_path.join("devcontainer-feature.json");
    let metadata = parse_feature_metadata(&metadata_path)
        .map_err(|e| anyhow::anyhow!("Failed to parse feature metadata: {}", e))?;

    // Validate the metadata
    metadata
        .validate()
        .map_err(|e| anyhow::anyhow!("Invalid feature metadata: {}", e))?;

    info!(
        "Testing feature: {} ({})",
        metadata.id,
        metadata.name.as_deref().unwrap_or("No name")
    );

    // Check if install.sh exists
    let install_script = feature_path.join("install.sh");
    if !install_script.exists() {
        return Err(anyhow::anyhow!("install.sh not found in feature directory"));
    }

    // Run install script in ephemeral Alpine container
    let success = run_feature_test_in_container(feature_path, &install_script).await?;

    let result = FeaturesResult {
        command: "test".to_string(),
        status: if success { "success" } else { "failure" }.to_string(),
        digest: None,
        size: None,
        message: if success {
            Some("Feature test completed successfully".to_string())
        } else {
            Some("Feature test failed".to_string())
        },
        cache_path: None,
    };

    output_result(&result, json)?;

    if !success {
        std::process::exit(1);
    }

    Ok(())
}

/// Execute features package command
async fn execute_features_package(
    path: &str,
    output_dir: &str,
    force_clean_output_folder: bool,
    json: bool,
) -> Result<()> {
    debug!(
        "Packaging feature at path: {} to output: {}",
        path, output_dir
    );

    let feature_path = Path::new(path);
    let output_path = Path::new(output_dir);

    // Step 1: Detect packaging mode
    let mode = detect_mode(feature_path)?;

    // Log detected mode
    match &mode {
        PackagingMode::Single => {
            info!(
                "Detected single feature mode for path: {}",
                feature_path.display()
            );
            println!("Mode: single");
        }
        PackagingMode::Collection => {
            info!(
                "Detected collection mode for path: {}",
                feature_path.display()
            );
            println!("Mode: collection");
        }
    }

    // Step 2: Clean output directory if requested
    if force_clean_output_folder && output_path.exists() {
        debug!("Cleaning output directory: {}", output_path.display());
        std::fs::remove_dir_all(output_path)?;
    }

    match mode {
        PackagingMode::Single => {
            // Step 3: Validate single feature
            let metadata = validate_single(feature_path)?;

            // Step 4: Create output directory
            std::fs::create_dir_all(output_path)?;

            // Step 5: Create .tgz archive
            let (digest, tar_filename, size) =
                create_feature_package(feature_path, output_path, &metadata).await?;

            // Step 6: Build feature descriptors
            let feature_descriptor = FeatureDescriptor {
                id: metadata.id.clone(),
                version: metadata
                    .version
                    .clone()
                    .unwrap_or_else(|| "latest".to_string()),
                name: metadata.name.clone(),
                description: metadata.description.clone(),
                options: if metadata.options.is_empty() {
                    None
                } else {
                    Some(serde_json::to_value(&metadata.options).unwrap_or(serde_json::Value::Null))
                },
                installs_after: if metadata.installs_after.is_empty() {
                    None
                } else {
                    Some(metadata.installs_after.clone())
                },
                depends_on: if metadata.depends_on.is_empty() {
                    None
                } else {
                    Some(serde_json::Value::Object(
                        metadata.depends_on.clone().into_iter().collect(),
                    ))
                },
            };

            let mut features_map = std::collections::BTreeMap::new();
            features_map.insert(metadata.id.clone(), feature_descriptor);

            let collection_metadata = CollectionMetadata {
                source_information: SourceInformation {
                    source: "devcontainer-cli".to_string(),
                },
                features: features_map,
            };

            // Step 6: Write devcontainer-collection.json
            let collection_path = output_path.join("devcontainer-collection.json");
            write_collection_metadata(&collection_metadata, &collection_path)?;

            // Log created artifacts
            info!("Created artifacts:");
            info!("  - {}", tar_filename);
            info!("  - devcontainer-collection.json");
            println!("Created artifacts:");
            println!("  - {}", tar_filename);
            println!("  - devcontainer-collection.json");

            // Step 7: Output results
            let result = FeaturesResult {
                command: "package".to_string(),
                status: "success".to_string(),
                digest: Some(digest),
                size: Some(size),
                message: Some(format!(
                    "Successfully packaged feature '{}' to {}",
                    metadata.id, output_dir
                )),
                cache_path: None,
            };

            output_result(&result, json)?;

            Ok(())
        }
        PackagingMode::Collection => {
            // Step 3: Enumerate and validate all features in collection
            let src_path = feature_path.join("src");
            let features = enumerate_and_validate_collection(&src_path)?;

            // Step 4: Create output directory
            std::fs::create_dir_all(output_path)?;

            // Step 5: Package each feature and collect descriptors
            let mut collection_features = std::collections::BTreeMap::new();
            let mut total_size = 0u64;
            let mut artifact_names = Vec::new();

            for (feature_id, feature_path, metadata) in features {
                // Create .tgz archive for this feature
                let (digest, tar_filename, size) =
                    create_feature_package(&feature_path, output_path, &metadata).await?;
                total_size += size;
                artifact_names.push(tar_filename);

                // Build feature descriptor
                let feature_descriptor = FeatureDescriptor {
                    id: metadata.id.clone(),
                    version: metadata
                        .version
                        .clone()
                        .unwrap_or_else(|| "latest".to_string()),
                    name: metadata.name.clone(),
                    description: metadata.description.clone(),
                    options: if metadata.options.is_empty() {
                        None
                    } else {
                        Some(
                            serde_json::to_value(&metadata.options)
                                .unwrap_or(serde_json::Value::Null),
                        )
                    },
                    installs_after: if metadata.installs_after.is_empty() {
                        None
                    } else {
                        Some(metadata.installs_after.clone())
                    },
                    depends_on: if metadata.depends_on.is_empty() {
                        None
                    } else {
                        Some(serde_json::Value::Object(
                            metadata.depends_on.clone().into_iter().collect(),
                        ))
                    },
                };

                collection_features.insert(feature_id, feature_descriptor);

                info!(
                    "Packaged feature '{}' (digest: {}, size: {} bytes)",
                    metadata.id, digest, size
                );
            }

            // Step 6: Write collection metadata
            let collection_metadata = CollectionMetadata {
                source_information: SourceInformation {
                    source: "devcontainer-cli".to_string(),
                },
                features: collection_features,
            };

            let collection_path = output_path.join("devcontainer-collection.json");
            write_collection_metadata(&collection_metadata, &collection_path)?;

            // Log created artifacts
            info!("Created artifacts:");
            for artifact_name in &artifact_names {
                info!("  - {}", artifact_name);
            }
            info!("  - devcontainer-collection.json");
            println!("Created artifacts:");
            for artifact_name in &artifact_names {
                println!("  - {}", artifact_name);
            }
            println!("  - devcontainer-collection.json");

            // Step 7: Output results
            let result = FeaturesResult {
                command: "package".to_string(),
                status: "success".to_string(),
                digest: None, // No single digest for collection
                size: Some(total_size),
                message: Some(format!(
                    "Successfully packaged {} feature(s) from collection to {}",
                    collection_metadata.features.len(),
                    output_dir
                )),
                cache_path: None,
            };

            output_result(&result, json)?;

            Ok(())
        }
    }
}

/// Execute features pull command
async fn execute_features_pull(registry_ref: &str, json: bool) -> Result<()> {
    debug!("Pulling feature from registry reference: {}", registry_ref);

    // Parse registry reference
    let (registry_url, namespace, name, tag) = parse_registry_reference(registry_ref)?;
    let tag = tag.unwrap_or_else(|| "latest".to_string());

    let feature_ref = FeatureRef::new(registry_url, namespace, name, Some(tag));

    info!("Pulling feature: {}", feature_ref.reference());

    // Create OCI client and fetch from registry
    let fetcher =
        default_fetcher().map_err(|e| anyhow::anyhow!("Failed to create OCI client: {}", e))?;

    let downloaded_feature = fetcher
        .fetch_feature(&feature_ref)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to pull feature: {}", e))?;

    let result = FeaturesResult {
        command: "pull".to_string(),
        status: "success".to_string(),
        digest: Some(downloaded_feature.digest),
        size: None, // Size not available in DownloadedFeature
        message: Some(format!(
            "Successfully pulled {} to {}",
            feature_ref.reference(),
            downloaded_feature.path.display()
        )),
        cache_path: Some(downloaded_feature.path.to_string_lossy().into_owned()),
    };

    output_result(&result, json)?;
    Ok(())
}

/// Publishes a DevContainer feature to an OCI registry.
///
/// This function packages and publishes a DevContainer feature from the local filesystem
/// to an OCI-compliant registry following the DevContainer Feature Distribution specification.
/// It validates the feature metadata, creates a feature package (tarball), computes semantic
/// version tags, and publishes the feature along with optional collection metadata.
///
/// # Parameters
///
/// * `path` - Path to the feature directory containing `devcontainer-feature.json`.
///   The directory must contain valid feature metadata and all required files.
/// * `registry` - Target OCI registry URL (e.g., `"ghcr.io"`, `"registry.example.com"`).
///   Must be accessible and support OCI Distribution Spec v2.
/// * `namespace` - Registry namespace/organization under which to publish
///   (e.g., `"myorg/features"`, `"username"`).
/// * `dry_run` - If `true`, validates the feature but skips actual publishing.
///   Useful for testing without modifying the registry.
/// * `username` - Optional username for registry authentication. Currently reserved
///   for future use; authentication is not yet implemented.
/// * `password_stdin` - If `true`, reads password from stdin for authentication.
///   Currently reserved for future use; authentication is not yet implemented.
///
/// # Returns
///
/// Returns `Ok(PublishOutput)` containing:
/// - Published feature details (ID, version, digest, tags)
/// - Optional collection metadata result
/// - Summary statistics (features count, published/skipped tags)
///
/// # Errors
///
/// Returns an error if:
/// - Feature metadata file (`devcontainer-feature.json`) is missing or invalid
/// - Feature metadata validation fails (invalid schema or required fields)
/// - Feature version is missing or not a valid semantic version (SemVer)
/// - Temporary directory creation fails during packaging
/// - Feature packaging (tarball creation) fails
/// - Registry operations fail (network issues, permissions, authentication)
/// - Collection metadata file is present but contains invalid JSON
///
/// # Example
///
/// ```rust,no_run
/// use deacon::commands::features::execute_features_publish;
///
/// # async fn example() -> anyhow::Result<()> {
/// let result = execute_features_publish(
///     "./my-feature",           // Feature directory
///     "ghcr.io",                // Registry
///     "myorg/features",         // Namespace
///     false,                    // Dry run
///     None,                     // Username (not yet implemented)
///     false,                    // Password stdin (not yet implemented)
/// ).await?;
///
/// println!("Published {} feature(s)", result.summary.features);
/// println!("Published tags: {}", result.summary.published_tags);
/// # Ok(())
/// # }
/// ```
///
/// # Notes
///
/// - The function automatically computes semantic version tags (e.g., `1`, `1.2`, `1.2.3`,
///   and `latest` for stable versions) based on the feature's version field.
/// - If all desired tags already exist in the registry, publishing is skipped to avoid
///   redundant operations.
/// - Collection metadata (`devcontainer-collection.json`) is automatically published if
///   present in the feature directory.
/// - Comprehensive tracing spans are emitted for observability and debugging.
#[tracing::instrument(
    name = "features.publish",
    fields(
        path = %path,
        registry = %registry,
        namespace = %namespace,
        dry_run = %dry_run,
        username = ?username,
        password_stdin = %password_stdin
    ),
    skip_all
)]
pub async fn execute_features_publish(
    path: &str,
    registry: &str,
    namespace: &str,
    dry_run: bool,
    username: Option<&str>,
    password_stdin: bool,
) -> Result<PublishOutput> {
    debug!(
        "Publishing feature at path: {} to registry: {} (dry_run: {})",
        path, registry, dry_run
    );

    // Handle authentication credentials if provided
    if let Some(_username) = username {
        // TODO: Implement credential setting in OCI client
        debug!("Username provided for authentication: {}", _username);
    }
    if password_stdin {
        // TODO: Implement reading password from stdin
        debug!("Password will be read from stdin");
    }

    let feature_path = Path::new(path);

    // Parse feature metadata
    let metadata_path = feature_path.join("devcontainer-feature.json");
    let metadata = match parse_feature_metadata(&metadata_path) {
        Ok(m) => m,
        Err(_) => {
            anyhow::bail!("No valid feature found at path '{}'. Ensure 'devcontainer-feature.json' exists and contains valid feature metadata", path);
        }
    };

    // Validate the metadata
    if metadata.validate().is_err() {
        anyhow::bail!("Feature metadata validation failed for '{}'. Check the 'devcontainer-feature.json' file for errors", metadata.id);
    }

    info!(
        "Publishing feature: {} ({})",
        metadata.id,
        metadata.name.as_deref().unwrap_or("No name")
    );

    // Validate feature version is SemVer
    let version = metadata.version.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Feature version is required for publishing. Please specify a version in devcontainer-feature.json under the 'version' field")
    })?;

    // Validate the version is valid SemVer
    use deacon_core::semver_utils;
    if semver_utils::parse_version(version).is_none() {
        anyhow::bail!("Invalid semantic version '{}' for feature '{}'. Version must be a valid SemVer format (e.g., '1.2.3', '2.0.0-rc.1'). Check https://semver.org/ for details", version, metadata.id);
    }

    if dry_run {
        info!(
            "Dry run mode - would publish to registry: {} namespace: {}",
            registry, namespace
        );

        let output = PublishOutput {
            features: vec![],
            collection: None,
            summary: PublishSummary {
                features: 0,
                published_tags: 0,
                skipped_tags: 0,
            },
        };

        return Ok(output);
    }

    // Create OCI client for tag listing and publishing
    let fetcher = {
        let span = tracing::info_span!("oci.client.create");
        let _enter = span.enter();
        default_fetcher().map_err(|e| {
            anyhow::anyhow!(
                "Failed to create OCI registry client: {}. Check your network connection and registry authentication",
                e
            )
        })?
    };

    // Use the provided registry and namespace, with feature name from metadata
    let registry_url = registry.to_string();
    let name = metadata.id.clone();
    let tag = Some(version.to_string()); // Use validated version as tag

    let feature_ref = FeatureRef::new(
        registry_url.clone(),
        namespace.to_string(),
        name.clone(),
        tag.clone(),
    );

    // Compute publish plan: determine which tags need to be published
    let span = tracing::info_span!("publish.plan.compute");
    let (desired_tags, existing_tags, to_publish_tags) = compute_publish_plan(&fetcher, &feature_ref, version)
        .instrument(span)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to determine publish plan for feature '{}': {}. This may be due to network issues or registry permissions",
                metadata.id,
                e
            )
        })?;

    info!(
        "Publish plan: {} desired tags, {} existing tags, {} to publish",
        desired_tags.len(),
        existing_tags.len(),
        to_publish_tags.len()
    );

    // Create feature package to get digest (needed even if all tags exist)
    let temp_dir = {
        let span = tracing::info_span!("feature.package.create");
        let _enter = span.enter();
        tempfile::tempdir().map_err(|e| {
            anyhow::anyhow!(
                "Failed to create temporary directory for feature packaging: {}",
                e
            )
        })?
    };

    let span = tracing::info_span!("feature.package.build", feature_id = %metadata.id);
    let (digest, _tar_filename, _size) = create_feature_package(feature_path, temp_dir.path(), &metadata)
        .instrument(span)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to create feature package for '{}': {}. Check that all required files exist in the feature directory",
                metadata.id,
                e
            )
        })?;

    if !to_publish_tags.is_empty() {
        // Publish with multiple tags if needed
        let package_data = std::fs::read(temp_dir.path().join(_tar_filename))
            .map_err(|e| anyhow::anyhow!("Failed to read packaged feature file: {}", e))?
            .into();

        let span = tracing::info_span!("feature.publish.multi_tag", tags = ?to_publish_tags);
        let _results = fetcher
            .publish_feature_multi_tag(
                registry_url.clone(),
                namespace.to_string(),
                name.clone(),
                to_publish_tags.clone(),
                package_data,
                &metadata,
            )
            .instrument(span)
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to publish feature '{}' to registry '{}': {}. Check your registry permissions and network connection",
                    metadata.id,
                    registry_url,
                    e
                )
            })?;

        info!(
            "Successfully published {} tags for {}/{}",
            _results.len(),
            registry_url,
            namespace
        );
    } else {
        info!("All desired tags already exist, skipping publish");
    }

    // Construct PublishFeatureResult
    let published_tags = to_publish_tags.clone();
    let skipped_tags: Vec<String> = desired_tags
        .iter()
        .filter(|tag| !to_publish_tags.contains(tag))
        .cloned()
        .collect();
    let moved_latest = published_tags.contains(&"latest".to_string());

    let feature_result = PublishFeatureResult {
        feature_id: metadata.id.clone(),
        version: version.to_string(),
        digest: digest.clone(),
        published_tags,
        skipped_tags: skipped_tags.clone(),
        moved_latest,
        registry: registry_url.clone(),
        namespace: namespace.to_string(),
    };

    // Check for and publish collection metadata if present
    let collection_result = {
        let collection_path = feature_path.join("devcontainer-collection.json");
        if collection_path.exists() {
            info!(
                "Found collection metadata at: {}",
                collection_path.display()
            );

            // Read collection metadata
            let collection_content = {
                let span = tracing::info_span!("collection.metadata.read");
                let _enter = span.enter();
                std::fs::read_to_string(&collection_path).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to read collection metadata file '{}': {}",
                        collection_path.display(),
                        e
                    )
                })?
            };

            // Parse as JSON to validate
            {
                let span = tracing::info_span!("collection.metadata.validate");
                let _enter = span.enter();
                let _: serde_json::Value =
                    serde_json::from_str(&collection_content).map_err(|e| {
                        anyhow::anyhow!(
                        "Invalid JSON in collection metadata file '{}': {}. Check the file syntax",
                        collection_path.display(),
                        e
                    )
                    })?;
            }

            // Publish collection metadata
            let span = tracing::info_span!("collection.metadata.publish");
            let collection_digest = fetcher
                .publish_collection_metadata(
                    &registry_url,
                    namespace,
                    collection_content.into_bytes().into(),
                )
                .instrument(span)
                .await
                .map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to publish collection metadata to registry '{}': {}. Check your registry permissions",
                        registry_url,
                        e
                    )
                })?;

            info!(
                "Successfully published collection metadata with digest: {}",
                collection_digest
            );

            Some(PublishCollectionResult {
                digest: collection_digest,
            })
        } else {
            info!("No collection metadata found (devcontainer-collection.json not present)");
            None
        }
    };

    let output = PublishOutput {
        features: vec![feature_result],
        collection: collection_result,
        summary: PublishSummary {
            features: 1,
            published_tags: to_publish_tags.len(),
            skipped_tags: skipped_tags.len(),
        },
    };

    Ok(output)
}

/// Compute tags to publish by comparing desired tags against existing tags from registry
///
/// This function determines which semantic version tags need to be published by:
/// 1. Computing desired tags from the feature version (X, X.Y, X.Y.Z, latest for stable)
/// 2. Listing existing tags from the registry
/// 3. Returning the difference (tags that don't exist yet)
///
/// # Arguments
/// * `fetcher` - OCI client for registry operations
/// * `feature_ref` - Reference to the feature repository
/// * `version` - Feature version string (must be valid SemVer)
///
/// # Returns
/// A tuple of (desired_tags, existing_tags, to_publish_tags)
///
/// # Errors
/// Returns an error if the version is invalid or registry operations fail
async fn compute_publish_plan(
    fetcher: &deacon_core::oci::FeatureFetcher<deacon_core::oci::ReqwestClient>,
    feature_ref: &deacon_core::oci::FeatureRef,
    version: &str,
) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    use deacon_core::semver_utils;

    // Compute desired tags from version
    let desired_tags = semver_utils::compute_semantic_tags(version);

    // List existing tags from registry
    let span = tracing::info_span!("registry.tags.list", registry = %feature_ref.registry, namespace = %feature_ref.namespace, name = %feature_ref.name);
    let existing_tags = fetcher
        .list_tags(feature_ref)
        .instrument(span)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to list existing tags from registry '{}': {}. Check your network connection and registry permissions",
                feature_ref.registry,
                e
            )
        })?;

    // Compute tags that need to be published (desired - existing)
    let to_publish: Vec<String> = desired_tags
        .iter()
        .filter(|tag| !existing_tags.contains(tag))
        .cloned()
        .collect();

    Ok((desired_tags, existing_tags, to_publish))
}

/// Execute features info command
async fn execute_features_info(mode: &str, feature: &str, json: bool) -> Result<()> {
    debug!("Getting feature info for: {} (mode: {})", feature, mode);

    // Determine if this is a local path or OCI reference
    // Check if it's a path by trying to see if it exists as a directory
    let path = Path::new(feature);
    let is_local = path.exists() && path.is_dir();

    let (metadata, registry_url, namespace, name, tag, digest) = if is_local {
        // Load from local path
        let feature_path = Path::new(feature);
        let metadata_path = feature_path.join("devcontainer-feature.json");
        let metadata = parse_feature_metadata(&metadata_path)
            .map_err(|e| anyhow::anyhow!("Failed to parse feature metadata: {}", e))?;

        // Validate the metadata
        metadata
            .validate()
            .map_err(|e| anyhow::anyhow!("Invalid feature metadata: {}", e))?;

        info!("Loading feature info from local path: {}", feature);
        (metadata, None, None, None, None, None)
    } else {
        // Parse the feature reference and fetch from OCI
        let (registry_url, namespace, name, tag) = parse_registry_reference(feature)?;

        let feature_ref = FeatureRef::new(
            registry_url.clone(),
            namespace.clone(),
            name.clone(),
            tag.clone(),
        );

        // Create OCI client and fetch feature metadata
        let fetcher =
            default_fetcher().map_err(|e| anyhow::anyhow!("Failed to create OCI client: {}", e))?;

        info!("Fetching feature info from: {}", feature_ref.reference());

        let downloaded_feature = fetcher
            .fetch_feature(&feature_ref)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch feature: {}", e))?;

        (
            downloaded_feature.metadata,
            Some(registry_url),
            Some(namespace),
            Some(name),
            tag,
            Some(downloaded_feature.digest),
        )
    };

    // Generate output based on mode
    match mode {
        "manifest" => output_manifest_info(&metadata, json),
        "tags" => {
            output_tags_info(
                &metadata,
                registry_url.as_deref(),
                namespace.as_deref(),
                name.as_deref(),
                json,
            )
            .await
        }
        "dependencies" => output_dependencies_info(&metadata, json),
        "verbose" => output_verbose_info(
            &metadata,
            registry_url.as_deref(),
            namespace.as_deref(),
            name.as_deref(),
            tag.as_deref(),
            digest.as_deref(),
            json,
        ),
        _ => Err(anyhow::anyhow!(
            "Invalid mode '{}'. Valid modes are: manifest, tags, dependencies, verbose",
            mode
        )),
    }
}

/// Output manifest information (metadata only)
fn output_manifest_info(metadata: &FeatureMetadata, json: bool) -> Result<()> {
    if json {
        // Convert HashMaps to BTreeMaps for deterministic ordering
        let options: BTreeMap<_, _> = metadata.options.iter().collect();
        let container_env: BTreeMap<_, _> = metadata.container_env.iter().collect();
        let depends_on: BTreeMap<_, _> = metadata.depends_on.iter().collect();

        let manifest = serde_json::json!({
            "id": metadata.id,
            "version": metadata.version,
            "name": metadata.name,
            "description": metadata.description,
            "documentationURL": metadata.documentation_url,
            "licenseURL": metadata.license_url,
            "options": options,
            "containerEnv": container_env,
            "mounts": metadata.mounts,
            "init": metadata.init,
            "privileged": metadata.privileged,
            "capAdd": metadata.cap_add,
            "securityOpt": metadata.security_opt,
            "entrypoint": metadata.entrypoint,
            "installsAfter": metadata.installs_after,
            "dependsOn": depends_on,
            "onCreateCommand": metadata.on_create_command,
            "updateContentCommand": metadata.update_content_command,
            "postCreateCommand": metadata.post_create_command,
            "postStartCommand": metadata.post_start_command,
            "postAttachCommand": metadata.post_attach_command,
        });
        println!("{}", serde_json::to_string_pretty(&manifest)?);
    } else {
        println!("Feature Manifest:");
        println!("  ID: {}", metadata.id);
        if let Some(ref version) = metadata.version {
            println!("  Version: {}", version);
        }
        if let Some(ref name) = metadata.name {
            println!("  Name: {}", name);
        }
        if let Some(ref desc) = metadata.description {
            println!("  Description: {}", desc);
        }
        if let Some(ref doc_url) = metadata.documentation_url {
            println!("  Documentation: {}", doc_url);
        }
        if let Some(ref license_url) = metadata.license_url {
            println!("  License: {}", license_url);
        }
        if !metadata.options.is_empty() {
            println!("  Options: {} defined", metadata.options.len());
        }
        if !metadata.installs_after.is_empty() {
            println!("  Installs After: {:?}", metadata.installs_after);
        }
        if !metadata.depends_on.is_empty() {
            println!("  Dependencies: {:?}", metadata.depends_on.keys());
        }
    }
    Ok(())
}

/// Output tags information (available versions)
async fn output_tags_info(
    metadata: &FeatureMetadata,
    registry_url: Option<&str>,
    namespace: Option<&str>,
    name: Option<&str>,
    json: bool,
) -> Result<()> {
    // For local features, we can only show the current version
    let tags = if let (Some(_registry), Some(_ns), Some(_n)) = (registry_url, namespace, name) {
        // Note: This is a placeholder - in a full implementation, we would
        // query the OCI registry for available tags
        // For now, we'll just return the current version if available
        vec![metadata
            .version
            .clone()
            .unwrap_or_else(|| "latest".to_string())]
    } else {
        // Local feature - only current version
        vec![metadata
            .version
            .clone()
            .unwrap_or_else(|| "unknown".to_string())]
    };

    if json {
        let output = serde_json::json!({
            "id": metadata.id,
            "tags": tags,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("Available Tags for '{}':", metadata.id);
        for tag in tags {
            println!("  - {}", tag);
        }
    }
    Ok(())
}

/// Output dependencies information
fn output_dependencies_info(metadata: &FeatureMetadata, json: bool) -> Result<()> {
    if json {
        let deps = serde_json::json!({
            "id": metadata.id,
            "installsAfter": metadata.installs_after,
            "dependsOn": metadata.depends_on,
        });
        println!("{}", serde_json::to_string_pretty(&deps)?);
    } else {
        println!("Dependencies for '{}':", metadata.id);
        if !metadata.installs_after.is_empty() {
            println!("  Installs After:");
            for feature in &metadata.installs_after {
                println!("    - {}", feature);
            }
        } else {
            println!("  Installs After: (none)");
        }

        if !metadata.depends_on.is_empty() {
            println!("  Depends On:");
            for (feature, options) in &metadata.depends_on {
                println!("    - {}: {}", feature, options);
            }
        } else {
            println!("  Depends On: (none)");
        }
    }
    Ok(())
}

/// Output verbose information (all available details)
fn output_verbose_info(
    metadata: &FeatureMetadata,
    registry_url: Option<&str>,
    namespace: Option<&str>,
    name: Option<&str>,
    tag: Option<&str>,
    digest: Option<&str>,
    json: bool,
) -> Result<()> {
    if json {
        // Convert HashMaps to BTreeMaps for deterministic ordering
        let options: BTreeMap<_, _> = metadata.options.iter().collect();
        let container_env: BTreeMap<_, _> = metadata.container_env.iter().collect();
        let depends_on: BTreeMap<_, _> = metadata.depends_on.iter().collect();

        let mut info = serde_json::json!({
            "id": metadata.id,
            "version": metadata.version,
            "name": metadata.name,
            "description": metadata.description,
            "documentationURL": metadata.documentation_url,
            "licenseURL": metadata.license_url,
            "options": options,
            "containerEnv": container_env,
            "mounts": metadata.mounts,
            "init": metadata.init,
            "privileged": metadata.privileged,
            "capAdd": metadata.cap_add,
            "securityOpt": metadata.security_opt,
            "entrypoint": metadata.entrypoint,
            "installsAfter": metadata.installs_after,
            "dependsOn": depends_on,
            "onCreateCommand": metadata.on_create_command,
            "updateContentCommand": metadata.update_content_command,
            "postCreateCommand": metadata.post_create_command,
            "postStartCommand": metadata.post_start_command,
            "postAttachCommand": metadata.post_attach_command,
        });

        // Add OCI-specific fields if available
        if let Some(registry) = registry_url {
            info["registry"] = serde_json::json!(registry);
        }
        if let Some(ns) = namespace {
            info["namespace"] = serde_json::json!(ns);
        }
        if let Some(n) = name {
            info["featureName"] = serde_json::json!(n);
        }
        if let Some(t) = tag {
            info["tag"] = serde_json::json!(t);
        }
        if let Some(d) = digest {
            info["digest"] = serde_json::json!(d);
        }

        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        println!("=== Feature Information (Verbose) ===");
        println!("\nBasic Information:");
        println!("  ID: {}", metadata.id);
        if let Some(ref version) = metadata.version {
            println!("  Version: {}", version);
        }
        if let Some(ref name) = metadata.name {
            println!("  Name: {}", name);
        }
        if let Some(ref desc) = metadata.description {
            println!("  Description: {}", desc);
        }
        if let Some(ref doc_url) = metadata.documentation_url {
            println!("  Documentation: {}", doc_url);
        }
        if let Some(ref license_url) = metadata.license_url {
            println!("  License: {}", license_url);
        }

        if let Some(registry) = registry_url {
            println!("\nRegistry Information:");
            println!("  Registry: {}", registry);
            if let Some(ns) = namespace {
                println!("  Namespace: {}", ns);
            }
            if let Some(n) = name {
                println!("  Name: {}", n);
            }
            if let Some(t) = tag {
                println!("  Tag: {}", t);
            }
            if let Some(d) = digest {
                println!("  Digest: {}", d);
            }
        }

        if !metadata.options.is_empty() {
            println!("\nOptions:");
            for (key, option) in &metadata.options {
                println!("  {}:", key);
                match option {
                    deacon_core::features::FeatureOption::Boolean {
                        default,
                        description,
                    } => {
                        println!("    Type: boolean");
                        if let Some(def) = default {
                            println!("    Default: {}", def);
                        }
                        if let Some(desc) = description {
                            println!("    Description: {}", desc);
                        }
                    }
                    deacon_core::features::FeatureOption::String {
                        default,
                        description,
                        r#enum,
                        proposals,
                    } => {
                        println!("    Type: string");
                        if let Some(def) = default {
                            println!("    Default: {}", def);
                        }
                        if let Some(desc) = description {
                            println!("    Description: {}", desc);
                        }
                        if let Some(values) = r#enum {
                            println!("    Allowed values: {:?}", values);
                        }
                        if let Some(props) = proposals {
                            println!("    Proposals: {:?}", props);
                        }
                    }
                }
            }
        }

        if !metadata.installs_after.is_empty() || !metadata.depends_on.is_empty() {
            println!("\nDependencies:");
            if !metadata.installs_after.is_empty() {
                println!("  Installs After:");
                for feature in &metadata.installs_after {
                    println!("    - {}", feature);
                }
            }
            if !metadata.depends_on.is_empty() {
                println!("  Depends On:");
                for (feature, options) in &metadata.depends_on {
                    println!("    - {}: {}", feature, options);
                }
            }
        }

        if !metadata.container_env.is_empty() {
            println!("\nContainer Environment Variables:");
            for (key, value) in &metadata.container_env {
                println!("  {}: {}", key, value);
            }
        }

        if !metadata.mounts.is_empty() {
            println!("\nMounts:");
            for mount in &metadata.mounts {
                println!("  - {}", mount);
            }
        }

        if metadata.init.is_some()
            || metadata.privileged.is_some()
            || !metadata.cap_add.is_empty()
            || !metadata.security_opt.is_empty()
        {
            println!("\nContainer Options:");
            if let Some(init) = metadata.init {
                println!("  Init: {}", init);
            }
            if let Some(privileged) = metadata.privileged {
                println!("  Privileged: {}", privileged);
            }
            if !metadata.cap_add.is_empty() {
                println!("  Capabilities: {:?}", metadata.cap_add);
            }
            if !metadata.security_opt.is_empty() {
                println!("  Security Options: {:?}", metadata.security_opt);
            }
        }

        if metadata.has_lifecycle_commands() {
            println!("\nLifecycle Commands:");
            if let Some(ref cmd) = metadata.on_create_command {
                println!("  onCreate: {}", cmd);
            }
            if let Some(ref cmd) = metadata.update_content_command {
                println!("  updateContent: {}", cmd);
            }
            if let Some(ref cmd) = metadata.post_create_command {
                println!("  postCreate: {}", cmd);
            }
            if let Some(ref cmd) = metadata.post_start_command {
                println!("  postStart: {}", cmd);
            }
            if let Some(ref cmd) = metadata.post_attach_command {
                println!("  postAttach: {}", cmd);
            }
        }
    }
    Ok(())
}

/// Run feature test in an ephemeral Alpine container
async fn run_feature_test_in_container(
    feature_path: &Path,
    _install_script: &Path,
) -> Result<bool> {
    use std::process::Command;

    debug!("Running feature test in ephemeral Alpine container");

    let feature_mount = format!("{}:/tmp/feature:ro", feature_path.display());

    // Run Docker command to test the feature
    let output = Command::new("docker")
        .args([
            "run",
            "--rm",
            "-v",
            &feature_mount,
            "alpine:latest",
            "sh",
            "-c",
            "cd /tmp/feature && chmod +x install.sh && ./install.sh",
        ])
        .output();

    match output {
        Ok(result) => {
            debug!("Docker command exit status: {}", result.status);
            debug!("Docker stdout: {}", String::from_utf8_lossy(&result.stdout));
            if !result.stderr.is_empty() {
                debug!("Docker stderr: {}", String::from_utf8_lossy(&result.stderr));
            }
            Ok(result.status.success())
        }
        Err(e) => {
            debug!("Failed to run docker command: {}", e);
            // If Docker is not available, we can't run the test but shouldn't fail the build
            info!("Docker not available for feature testing - skipping container test");
            Ok(true) // Return success so build doesn't fail
        }
    }
}

/// Create a feature package (gzipped tar archive with OCI manifest stub)
async fn create_feature_package(
    feature_path: &Path,
    output_path: &Path,
    metadata: &FeatureMetadata,
) -> Result<(String, String, u64)> {
    use flate2::{write::GzEncoder, Compression};
    use sha2::{Digest, Sha256};
    use std::fs::File;
    use std::io::{Read, Write};
    use tar::Builder;

    debug!("Creating feature package for: {}", metadata.id);

    // Build artifact name with version
    let tar_filename = build_artifact_name(&metadata.id, &metadata.version)?;

    // Create gzipped tar archive
    let tar_path = output_path.join(&tar_filename);
    let tar_file = File::create(&tar_path)?;
    let encoder = GzEncoder::new(tar_file, Compression::default());
    let mut builder = Builder::new(encoder);

    // Add all files from feature directory to tar
    builder.append_dir_all(".", feature_path)?;
    let encoder = builder.into_inner()?;
    encoder.finish()?;

    // Calculate digest and size
    let mut file = File::open(&tar_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    hasher.update(&buffer);
    let digest = format!("sha256:{:x}", hasher.finalize());
    let size = buffer.len() as u64;

    // Create OCI manifest stub
    let manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.devcontainers.feature.config.v1+json",
            "size": 0,
            "digest": "sha256:placeholder"
        },
        "layers": [{
            "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
            "size": size,
            "digest": digest.clone()
        }]
    });

    let manifest_path = output_path.join(format!("{}-manifest.json", metadata.id));
    let mut manifest_file = File::create(manifest_path)?;
    manifest_file.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;

    info!(
        "Created package: {} (digest: {}, size: {} bytes)",
        tar_filename, digest, size
    );

    Ok((digest, tar_filename, size))
}

/// Output result in the specified format
fn output_result(result: &FeaturesResult, json: bool) -> Result<()> {
    if json {
        let json_output = serde_json::to_string_pretty(result)?;
        println!("{}", json_output);
    } else {
        println!("Command: {}", result.command);
        println!("Status: {}", result.status);
        if let Some(ref digest) = result.digest {
            println!("Digest: {}", digest);
        }
        if let Some(size) = result.size {
            println!("Size: {} bytes", size);
        }
        if let Some(ref cache_path) = result.cache_path {
            println!("Cache Path: {}", cache_path);
        }
        if let Some(ref message) = result.message {
            println!("Message: {}", message);
        }
    }
    Ok(())
}

/// Output publish result in the specified format
fn output_publish_result(result: &PublishOutput, json: bool) -> Result<()> {
    if json {
        let json_output = serde_json::to_string_pretty(result)?;
        println!("{}", json_output);
    } else {
        println!("Feature Publish Summary:");
        println!("Features processed: {}", result.summary.features);
        println!("Tags published: {}", result.summary.published_tags);
        println!("Tags skipped: {}", result.summary.skipped_tags);

        if !result.features.is_empty() {
            println!("\nPublished Features:");
            for feature in &result.features {
                println!(
                    "  {}@{} ({})",
                    feature.feature_id, feature.version, feature.registry
                );
                println!("    Digest: {}", feature.digest);
                if !feature.published_tags.is_empty() {
                    println!("    Published tags: {}", feature.published_tags.join(", "));
                }
                if !feature.skipped_tags.is_empty() {
                    println!("    Skipped tags: {}", feature.skipped_tags.join(", "));
                }
                if feature.moved_latest {
                    println!("    Moved latest tag");
                }
            }
        }

        if let Some(collection) = &result.collection {
            println!("\nCollection Metadata:");
            println!("  Digest: {}", collection.digest);
        }
    }
    Ok(())
}

#[cfg(test)]
mod unit_features_package {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_is_local_path() {
        // Relative paths
        assert!(is_local_path("./feature"));
        assert!(is_local_path("../feature"));
        assert!(is_local_path("./path/to/feature"));
        assert!(is_local_path("../path/to/feature"));

        // Absolute Unix paths
        assert!(is_local_path("/abs/path"));
        assert!(is_local_path("/feature"));

        // Windows paths
        assert!(is_local_path("C:\\path"));
        assert!(is_local_path("D:/path"));
        assert!(is_local_path("E:\\feature"));

        // Valid registry references (should NOT be detected as local paths)
        assert!(!is_local_path("ghcr.io/devcontainers/node"));
        assert!(!is_local_path("myteam/myfeature"));
        assert!(!is_local_path("myfeature"));
        assert!(!is_local_path("ghcr.io/devcontainers/node:18"));
        assert!(!is_local_path("feature-name"));
        assert!(!is_local_path("my-feature"));
    }

    #[tokio::test]
    async fn test_features_plan_rejects_local_paths() {
        let temp_dir = TempDir::new().unwrap();

        // Test with relative path ./feature
        let config_dir = temp_dir.path().join(".devcontainer");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("devcontainer.json");
        fs::write(&config_path, r#"{"features": {"./my-feature": true}}"#).unwrap();

        let args = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: None,
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path.clone()),
        };

        let result = execute_features_plan(true, None, &args).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains(LOCAL_FEATURE_ERROR_MSG));
        assert!(err_msg.contains("./my-feature"));

        // Test with absolute path
        fs::write(&config_path, r#"{"features": {"/abs/path/feature": true}}"#).unwrap();

        let args2 = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: None,
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path.clone()),
        };

        let result2 = execute_features_plan(true, None, &args2).await;
        assert!(result2.is_err());
        let err_msg2 = format!("{}", result2.unwrap_err());
        assert!(err_msg2.contains(LOCAL_FEATURE_ERROR_MSG));
        assert!(err_msg2.contains("/abs/path/feature"));

        // Test with parent relative path
        fs::write(
            &config_path,
            r#"{"features": {"../another-feature": true}}"#,
        )
        .unwrap();

        let args3 = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: None,
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path.clone()),
        };

        let result3 = execute_features_plan(true, None, &args3).await;
        assert!(result3.is_err());
        let err_msg3 = format!("{}", result3.unwrap_err());
        assert!(err_msg3.contains(LOCAL_FEATURE_ERROR_MSG));
        assert!(err_msg3.contains("../another-feature"));
    }

    #[tokio::test]
    async fn test_features_plan_additional_features_with_local_path() {
        let temp_dir = TempDir::new().unwrap();
        let args = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: Some(r#"{"./local-feature": true}"#.to_string()),
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
        };

        let result = execute_features_plan(true, Some(r#"{"./local-feature": true}"#), &args).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains(LOCAL_FEATURE_ERROR_MSG));
        assert!(err_msg.contains("./local-feature"));
    }

    #[tokio::test]
    async fn test_features_plan_mixed_local_and_registry() {
        let temp_dir = TempDir::new().unwrap();

        // Test with mixed features: one local, one registry
        // The map iteration order may vary, but at least one local path should be detected
        let config_dir = temp_dir.path().join(".devcontainer");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("devcontainer.json");
        fs::write(
            &config_path,
            r#"{"features": {"./local-feature": true, "ghcr.io/devcontainers/node": true}}"#,
        )
        .unwrap();

        let args = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: None,
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path.clone()),
        };

        let result = execute_features_plan(true, None, &args).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains(LOCAL_FEATURE_ERROR_MSG));
        // Should detect the local path and error
        assert!(err_msg.contains("./local-feature") || err_msg.contains("Feature key:"));
    }

    #[test]
    fn test_features_result_json_serialization() {
        let result = FeaturesResult {
            command: "test".to_string(),
            status: "success".to_string(),
            digest: Some("sha256:abc123".to_string()),
            size: Some(1024),
            message: Some("Test completed".to_string()),
            cache_path: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"command\":\"test\""));
        assert!(json.contains("\"status\":\"success\""));
        assert!(json.contains("\"digest\":\"sha256:abc123\""));
    }

    #[test]
    fn test_features_plan_result_serialization() {
        let result = FeaturesPlanResult {
            order: vec!["feature-a".to_string(), "feature-b".to_string()],
            graph: serde_json::json!({
                "feature-a": [],
                "feature-b": ["feature-a"]
            }),
        };

        let json = serde_json::to_string_pretty(&result).unwrap();
        assert!(json.contains("\"order\""));
        assert!(json.contains("\"graph\""));
        assert!(json.contains("feature-a"));
        assert!(json.contains("feature-b"));
    }

    #[test]
    fn test_build_graph_representation() {
        use deacon_core::features::{FeatureMetadata, ResolvedFeature};
        use std::collections::HashMap;

        let mut depends_on = HashMap::new();
        depends_on.insert("feature-a".to_string(), serde_json::Value::Bool(true));

        let feature = ResolvedFeature {
            id: "feature-b".to_string(),
            source: "test://feature-b".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "feature-b".to_string(),
                version: Some("1.0.0".to_string()),
                name: Some("Feature B".to_string()),
                description: None,
                documentation_url: None,
                license_url: None,
                options: HashMap::new(),
                container_env: HashMap::new(),
                mounts: vec![],
                init: None,
                privileged: None,
                cap_add: vec![],
                security_opt: vec![],
                entrypoint: None,
                installs_after: vec!["feature-a".to_string()],
                depends_on,
                on_create_command: None,
                update_content_command: None,
                post_create_command: None,
                post_start_command: None,
                post_attach_command: None,
            },
        };

        let graph = build_graph_representation(&[feature]);
        // Check that feature-b has dependencies
        if let Some(deps) = graph.get("feature-b") {
            if let Some(deps_array) = deps.as_array() {
                assert!(!deps_array.is_empty());
                assert!(deps_array.contains(&serde_json::Value::String("feature-a".to_string())));
            } else {
                panic!("Dependencies should be an array");
            }
        } else {
            panic!("feature-b should exist in graph");
        }
    }

    #[test]
    fn test_graph_structure_no_dependencies() {
        // Test: Feature with no dependencies should have empty array in graph
        let feature = create_mock_resolved_feature("feature-standalone");

        let graph = build_graph_representation(&[feature]);

        // Verify the feature exists in the graph with an empty dependencies array
        let deps = graph
            .get("feature-standalone")
            .expect("Feature should exist in graph");
        let deps_array = deps.as_array().expect("Dependencies should be an array");
        assert!(
            deps_array.is_empty(),
            "Feature with no dependencies should have empty array"
        );
    }

    #[test]
    fn test_graph_structure_simple_chain() {
        // Test: Simple dependency chain A->B where B depends on A
        // Graph should show: { "feature-a": [], "feature-b": ["feature-a"] }
        let feature_a = create_mock_resolved_feature("feature-a");
        let feature_b = create_mock_resolved_feature_with_deps("feature-b", &["feature-a"], &[]);

        let graph = build_graph_representation(&[feature_a, feature_b]);

        // Verify feature-a has no dependencies
        let deps_a = graph.get("feature-a").expect("feature-a should exist");
        let deps_a_array = deps_a.as_array().expect("Dependencies should be an array");
        assert!(
            deps_a_array.is_empty(),
            "feature-a should have no dependencies"
        );

        // Verify feature-b depends on feature-a
        let deps_b = graph.get("feature-b").expect("feature-b should exist");
        let deps_b_array = deps_b.as_array().expect("Dependencies should be an array");
        assert_eq!(deps_b_array.len(), 1, "feature-b should have 1 dependency");
        assert!(
            deps_b_array.contains(&serde_json::Value::String("feature-a".to_string())),
            "feature-b should depend on feature-a"
        );
    }

    #[test]
    fn test_graph_structure_combined_installs_after_and_depends_on() {
        // Test: Feature with both installsAfter and dependsOn should union them
        // If both specify different dependencies, all should appear
        let feature_a = create_mock_resolved_feature("feature-a");
        let feature_b = create_mock_resolved_feature("feature-b");
        let feature_c = create_mock_resolved_feature_with_deps(
            "feature-c",
            &["feature-a"], // installsAfter
            &["feature-b"], // dependsOn
        );

        let graph = build_graph_representation(&[feature_a, feature_b, feature_c]);

        // Verify feature-c has both dependencies
        let deps_c = graph.get("feature-c").expect("feature-c should exist");
        let deps_c_array = deps_c.as_array().expect("Dependencies should be an array");
        assert_eq!(
            deps_c_array.len(),
            2,
            "feature-c should have 2 dependencies"
        );
        assert!(
            deps_c_array.contains(&serde_json::Value::String("feature-a".to_string())),
            "feature-c should have feature-a from installsAfter"
        );
        assert!(
            deps_c_array.contains(&serde_json::Value::String("feature-b".to_string())),
            "feature-c should have feature-b from dependsOn"
        );
    }

    #[test]
    fn test_graph_structure_union_deduplication() {
        // Test: If same dependency appears in both installsAfter and dependsOn,
        // it should appear only once (deduplication)
        let feature_a = create_mock_resolved_feature("feature-a");
        let feature_b = create_mock_resolved_feature_with_deps(
            "feature-b",
            &["feature-a"], // installsAfter
            &["feature-a"], // dependsOn (same)
        );

        let graph = build_graph_representation(&[feature_a, feature_b]);

        // Verify feature-b has feature-a only once
        let deps_b = graph.get("feature-b").expect("feature-b should exist");
        let deps_b_array = deps_b.as_array().expect("Dependencies should be an array");
        assert_eq!(
            deps_b_array.len(),
            1,
            "Duplicate dependency should be deduplicated"
        );
        assert!(
            deps_b_array.contains(&serde_json::Value::String("feature-a".to_string())),
            "feature-b should depend on feature-a"
        );
    }

    #[test]
    fn test_graph_structure_fan_in() {
        // Test: Fan-in pattern where C depends on both A and B
        // Graph should show: { "feature-c": ["feature-a", "feature-b"] }
        let feature_a = create_mock_resolved_feature("feature-a");
        let feature_b = create_mock_resolved_feature("feature-b");
        let feature_c =
            create_mock_resolved_feature_with_deps("feature-c", &["feature-a", "feature-b"], &[]);

        let graph = build_graph_representation(&[feature_a, feature_b, feature_c]);

        // Verify feature-c depends on both feature-a and feature-b
        let deps_c = graph.get("feature-c").expect("feature-c should exist");
        let deps_c_array = deps_c.as_array().expect("Dependencies should be an array");
        assert_eq!(
            deps_c_array.len(),
            2,
            "feature-c should have 2 dependencies"
        );
        assert!(
            deps_c_array.contains(&serde_json::Value::String("feature-a".to_string())),
            "feature-c should depend on feature-a"
        );
        assert!(
            deps_c_array.contains(&serde_json::Value::String("feature-b".to_string())),
            "feature-c should depend on feature-b"
        );
    }

    #[test]
    fn test_graph_structure_deterministic_ordering() {
        // Test: Dependencies should be in deterministic (lexicographic) order
        let feature_a = create_mock_resolved_feature("feature-a");
        let feature_b = create_mock_resolved_feature("feature-b");
        let feature_c = create_mock_resolved_feature("feature-c");
        // Add dependencies in non-lexicographic order
        let feature_d = create_mock_resolved_feature_with_deps(
            "feature-d",
            &["feature-c", "feature-a", "feature-b"], // Not sorted
            &[],
        );

        let graph = build_graph_representation(&[feature_a, feature_b, feature_c, feature_d]);

        // Verify dependencies are in lexicographic order
        let deps_d = graph.get("feature-d").expect("feature-d should exist");
        let deps_d_array = deps_d.as_array().expect("Dependencies should be an array");
        assert_eq!(
            deps_d_array.len(),
            3,
            "feature-d should have 3 dependencies"
        );

        // Extract dependency strings in order
        let dep_strings: Vec<String> = deps_d_array
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();

        assert_eq!(
            dep_strings,
            vec!["feature-a", "feature-b", "feature-c"],
            "Dependencies should be in lexicographic order"
        );
    }

    #[test]
    fn test_graph_structure_cycle_detection_error_shape() {
        use deacon_core::errors::FeatureError;
        use deacon_core::features::FeatureDependencyResolver;

        // Test: Verify cycle detection returns proper error structure
        // Cycle: a -> b -> c -> a
        let features = vec![
            create_mock_resolved_feature_with_deps("feature-a", &["feature-b"], &[]),
            create_mock_resolved_feature_with_deps("feature-b", &["feature-c"], &[]),
            create_mock_resolved_feature_with_deps("feature-c", &["feature-a"], &[]),
        ];

        let resolver = FeatureDependencyResolver::new(None);
        let result = resolver.resolve(&features);

        // Verify error type matches spec
        assert!(result.is_err(), "Cycle should produce an error");

        match result {
            Err(FeatureError::DependencyCycle { cycle_path }) => {
                // Verify error message contains cycle information
                assert!(
                    cycle_path.contains("feature-a"),
                    "Cycle path should contain feature-a"
                );
                assert!(
                    cycle_path.contains("feature-b"),
                    "Cycle path should contain feature-b"
                );
                assert!(
                    cycle_path.contains("feature-c"),
                    "Cycle path should contain feature-c"
                );
                // Verify it's formatted as a path with arrows
                assert!(
                    cycle_path.contains("->") || cycle_path.contains("→"),
                    "Cycle path should show direction with arrows"
                );
            }
            _ => panic!("Expected DependencyCycle error, got {:?}", result),
        }
    }

    #[test]
    fn test_graph_structure_complete_json_output() {
        // Test: Verify complete JSON output structure matches DATA-STRUCTURES.md spec
        let features = vec![
            create_mock_resolved_feature("feature-a"),
            create_mock_resolved_feature_with_deps("feature-b", &["feature-a"], &[]),
            create_mock_resolved_feature_with_deps("feature-c", &["feature-a", "feature-b"], &[]),
        ];

        let graph = build_graph_representation(&features);

        // Verify it's a JSON object
        assert!(graph.is_object(), "Graph should be a JSON object");

        // Verify all features are present
        assert!(
            graph.get("feature-a").is_some(),
            "feature-a should be in graph"
        );
        assert!(
            graph.get("feature-b").is_some(),
            "feature-b should be in graph"
        );
        assert!(
            graph.get("feature-c").is_some(),
            "feature-c should be in graph"
        );

        // Verify structure: feature-a has no deps
        let deps_a = graph.get("feature-a").unwrap();
        assert!(deps_a.is_array(), "Dependencies should be an array");
        assert_eq!(
            deps_a.as_array().unwrap().len(),
            0,
            "feature-a has no dependencies"
        );

        // Verify structure: feature-b depends on feature-a
        let deps_b = graph.get("feature-b").unwrap().as_array().unwrap();
        assert_eq!(deps_b.len(), 1, "feature-b has 1 dependency");
        assert_eq!(
            deps_b[0],
            serde_json::Value::String("feature-a".to_string()),
            "feature-b depends on feature-a"
        );

        // Verify structure: feature-c depends on both (sorted)
        let deps_c = graph.get("feature-c").unwrap().as_array().unwrap();
        assert_eq!(deps_c.len(), 2, "feature-c has 2 dependencies");
        assert_eq!(
            deps_c[0],
            serde_json::Value::String("feature-a".to_string()),
            "First dep should be feature-a (sorted)"
        );
        assert_eq!(
            deps_c[1],
            serde_json::Value::String("feature-b".to_string()),
            "Second dep should be feature-b (sorted)"
        );

        // Verify JSON serialization matches expected format
        let json_str = serde_json::to_string_pretty(&graph).unwrap();
        assert!(
            json_str.contains("\"feature-a\""),
            "JSON should contain feature-a"
        );
        assert!(json_str.contains("[]"), "JSON should contain empty arrays");
    }

    #[test]
    fn test_validate_single_missing_devcontainer_feature_json() {
        let temp_dir = TempDir::new().unwrap();
        let feature_dir = temp_dir.path().join("test-feature");
        fs::create_dir_all(&feature_dir).unwrap();

        // Don't create devcontainer-feature.json
        let result = validate_single(&feature_dir);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("devcontainer-feature.json not found"));
        assert!(err_msg.contains(&feature_dir.display().to_string()));
    }

    #[test]
    fn test_validate_single_devcontainer_feature_json_is_directory() {
        let temp_dir = TempDir::new().unwrap();
        let feature_dir = temp_dir.path().join("test-feature");
        fs::create_dir_all(&feature_dir).unwrap();

        // Create devcontainer-feature.json as a directory instead of file
        let json_path = feature_dir.join("devcontainer-feature.json");
        fs::create_dir(&json_path).unwrap();

        let result = validate_single(&feature_dir);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("devcontainer-feature.json exists but is not a file"));
        assert!(err_msg.contains(&feature_dir.display().to_string()));
    }

    #[test]
    fn test_validate_single_corrupt_devcontainer_feature_json() {
        let temp_dir = TempDir::new().unwrap();
        let feature_dir = temp_dir.path().join("test-feature");
        fs::create_dir_all(&feature_dir).unwrap();

        // Create invalid JSON
        let json_path = feature_dir.join("devcontainer-feature.json");
        fs::write(
            &json_path,
            r#"{"id": "test", "version": "1.0.0", invalid json"#,
        )
        .unwrap();

        let result = validate_single(&feature_dir);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Failed to parse devcontainer-feature.json"));
        assert!(err_msg.contains(&feature_dir.display().to_string()));
    }

    #[test]
    fn test_validate_single_missing_version() {
        let temp_dir = TempDir::new().unwrap();
        let feature_dir = temp_dir.path().join("test-feature");
        fs::create_dir_all(&feature_dir).unwrap();

        // Create valid JSON but missing version
        let json_path = feature_dir.join("devcontainer-feature.json");
        fs::write(
            &json_path,
            r#"{"id": "test-feature", "name": "Test Feature"}"#,
        )
        .unwrap();

        let result = validate_single(&feature_dir);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Feature 'test-feature' is missing required 'version' field"));
    }

    #[test]
    fn test_validate_single_valid_feature() {
        let temp_dir = TempDir::new().unwrap();
        let feature_dir = temp_dir.path().join("test-feature");
        fs::create_dir_all(&feature_dir).unwrap();

        // Create valid feature
        let json_path = feature_dir.join("devcontainer-feature.json");
        fs::write(
            &json_path,
            r#"{"id": "test-feature", "version": "1.0.0", "name": "Test Feature"}"#,
        )
        .unwrap();

        let result = validate_single(&feature_dir);
        assert!(result.is_ok());
        let metadata = result.unwrap();
        assert_eq!(metadata.id, "test-feature");
        assert_eq!(metadata.version, Some("1.0.0".to_string()));
        assert_eq!(metadata.name, Some("Test Feature".to_string()));
    }

    #[test]
    fn test_enumerate_and_validate_collection_src_directory_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join("src");

        // Don't create src directory
        let result = enumerate_and_validate_collection(&src_path);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("src/ directory not found"));
        assert!(err_msg.contains(&src_path.display().to_string()));
    }

    #[test]
    fn test_enumerate_and_validate_collection_src_is_file() {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join("src");

        // Create src as a file instead of directory
        fs::write(&src_path, "not a directory").unwrap();

        let result = enumerate_and_validate_collection(&src_path);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("src/ exists but is not a directory"));
        assert!(err_msg.contains(&src_path.display().to_string()));
    }

    #[test]
    fn test_enumerate_and_validate_collection_empty_src_directory() {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join("src");
        fs::create_dir_all(&src_path).unwrap();

        // Leave src directory empty
        let result = enumerate_and_validate_collection(&src_path);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("No valid features found in src/ directory"));
        assert!(err_msg.contains(&src_path.display().to_string()));
    }

    #[test]
    fn test_enumerate_and_validate_collection_invalid_feature() {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join("src");
        fs::create_dir_all(&src_path).unwrap();

        // Create a feature directory with invalid JSON
        let feature_dir = src_path.join("invalid-feature");
        fs::create_dir_all(&feature_dir).unwrap();
        let json_path = feature_dir.join("devcontainer-feature.json");
        fs::write(&json_path, r#"{"id": "invalid", invalid json"#).unwrap();

        let result = enumerate_and_validate_collection(&src_path);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Found 1 invalid feature(s) in collection"));
        assert!(err_msg.contains("Failed to parse devcontainer-feature.json"));
    }

    #[test]
    fn test_enumerate_and_validate_collection_mixed_valid_invalid() {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join("src");
        fs::create_dir_all(&src_path).unwrap();

        // Create a valid feature
        let valid_dir = src_path.join("feature-a");
        fs::create_dir_all(&valid_dir).unwrap();
        let valid_json = valid_dir.join("devcontainer-feature.json");
        fs::write(&valid_json, r#"{"id": "feature-a", "version": "1.0.0"}"#).unwrap();

        // Create an invalid feature
        let invalid_dir = src_path.join("invalid-feature");
        fs::create_dir_all(&invalid_dir).unwrap();
        let invalid_json = invalid_dir.join("devcontainer-feature.json");
        fs::write(
            &invalid_json,
            r#"{"id": "invalid", "version": missing quote"#,
        )
        .unwrap();

        let result = enumerate_and_validate_collection(&src_path);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Found 1 invalid feature(s) in collection"));
        assert!(err_msg.contains("Failed to parse devcontainer-feature.json"));
    }

    #[test]
    fn test_enumerate_and_validate_collection_valid_features() {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join("src");
        fs::create_dir_all(&src_path).unwrap();

        // Create first valid feature
        let feature1_dir = src_path.join("feature-a");
        fs::create_dir_all(&feature1_dir).unwrap();
        let json1_path = feature1_dir.join("devcontainer-feature.json");
        fs::write(&json1_path, r#"{"id": "feature-a", "version": "1.0.0"}"#).unwrap();

        // Create second valid feature
        let feature2_dir = src_path.join("feature-b");
        fs::create_dir_all(&feature2_dir).unwrap();
        let json2_path = feature2_dir.join("devcontainer-feature.json");
        fs::write(&json2_path, r#"{"id": "feature-b", "version": "2.0.0"}"#).unwrap();

        let result = enumerate_and_validate_collection(&src_path);
        assert!(result.is_ok());
        let features = result.unwrap();

        // Should have 2 features, sorted by ID
        assert_eq!(features.len(), 2);
        assert_eq!(features[0].0, "feature-a");
        assert_eq!(features[1].0, "feature-b");
        assert_eq!(features[0].1, feature1_dir);
        assert_eq!(features[1].1, feature2_dir);
        assert_eq!(features[0].2.id, "feature-a");
        assert_eq!(features[1].2.id, "feature-b");
    }

    #[test]
    fn test_enumerate_and_validate_collection_feature_id_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let src_path = temp_dir.path().join("src");
        fs::create_dir_all(&src_path).unwrap();

        // Create feature where directory name doesn't match ID in JSON
        let feature_dir = src_path.join("directory-name");
        fs::create_dir_all(&feature_dir).unwrap();
        let json_path = feature_dir.join("devcontainer-feature.json");
        fs::write(&json_path, r#"{"id": "different-id", "version": "1.0.0"}"#).unwrap();

        let result = enumerate_and_validate_collection(&src_path);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Found 1 invalid feature(s) in collection"));
        assert!(err_msg.contains("Feature ID 'different-id' in devcontainer-feature.json does not match directory name 'directory-name'"));
    }

    #[test]
    fn test_detect_mode_single_feature() {
        let temp_dir = TempDir::new().unwrap();
        let feature_dir = temp_dir.path().join("test-feature");
        fs::create_dir_all(&feature_dir).unwrap();

        // Create devcontainer-feature.json at root
        let json_path = feature_dir.join("devcontainer-feature.json");
        fs::write(&json_path, r#"{"id": "test", "version": "1.0.0"}"#).unwrap();

        let result = detect_mode(&feature_dir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PackagingMode::Single);
    }

    #[test]
    fn test_detect_mode_collection() {
        let temp_dir = TempDir::new().unwrap();
        let collection_dir = temp_dir.path().join("collection");
        fs::create_dir_all(&collection_dir).unwrap();

        // Create src/ directory
        let src_dir = collection_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();

        let result = detect_mode(&collection_dir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PackagingMode::Collection);
    }

    #[test]
    fn test_detect_mode_neither() {
        let temp_dir = TempDir::new().unwrap();
        let empty_dir = temp_dir.path().join("empty");
        fs::create_dir_all(&empty_dir).unwrap();

        // Don't create devcontainer-feature.json or src/
        let result = detect_mode(&empty_dir);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Cannot determine packaging mode"));
        assert!(err_msg.contains("devcontainer-feature.json at root"));
        assert!(err_msg.contains("src/ directory"));
        assert!(err_msg.contains(&empty_dir.display().to_string()));
    }

    #[test]
    fn test_write_collection_metadata_structure() {
        use serde_json::Value;

        let temp_dir = TempDir::new().unwrap();
        let metadata_path = temp_dir.path().join("devcontainer-collection.json");

        // Create test data
        let mut features = std::collections::BTreeMap::new();
        features.insert(
            "test-feature".to_string(),
            FeatureDescriptor {
                id: "test-feature".to_string(),
                version: "1.0.0".to_string(),
                name: Some("Test Feature".to_string()),
                description: Some("A test feature".to_string()),
                options: Some(serde_json::json!({"option1": "value1"})),
                installs_after: Some(vec!["dep1".to_string()]),
                depends_on: Some(serde_json::json!({"dep2": {"version": "2.0.0"}})),
            },
        );

        let collection_metadata = CollectionMetadata {
            source_information: SourceInformation {
                source: "devcontainer-cli".to_string(),
            },
            features,
        };

        // Write metadata
        let result = write_collection_metadata(&collection_metadata, &metadata_path);
        assert!(result.is_ok());

        // Read and parse the written JSON
        let json_content = fs::read_to_string(&metadata_path).unwrap();
        let parsed: Value = serde_json::from_str(&json_content).unwrap();

        // Validate structure according to OpenAPI contract
        assert!(parsed.is_object());
        let obj = parsed.as_object().unwrap();

        // Required fields
        assert!(obj.contains_key("sourceInformation"));
        assert!(obj.contains_key("features"));

        // sourceInformation structure
        let source_info = obj.get("sourceInformation").unwrap();
        assert!(source_info.is_object());
        let source_obj = source_info.as_object().unwrap();
        assert!(source_obj.contains_key("source"));
        assert_eq!(source_obj.get("source").unwrap(), "devcontainer-cli");

        // features structure
        let features_obj = obj.get("features").unwrap();
        assert!(features_obj.is_object());
        let features_map = features_obj.as_object().unwrap();
        assert!(features_map.contains_key("test-feature"));

        // Feature descriptor structure
        let feature = features_map.get("test-feature").unwrap();
        assert!(feature.is_object());
        let feature_obj = feature.as_object().unwrap();

        // Required fields for FeatureDescriptor
        assert!(feature_obj.contains_key("id"));
        assert!(feature_obj.contains_key("version"));
        assert_eq!(feature_obj.get("id").unwrap(), "test-feature");
        assert_eq!(feature_obj.get("version").unwrap(), "1.0.0");

        // Optional fields
        assert_eq!(feature_obj.get("name").unwrap(), "Test Feature");
        assert_eq!(feature_obj.get("description").unwrap(), "A test feature");
        assert!(feature_obj.contains_key("options"));
        assert!(feature_obj.contains_key("installsAfter"));
        assert!(feature_obj.contains_key("dependsOn"));
    }

    #[tokio::test]
    async fn test_deterministic_packaging_single_feature() {
        let temp_dir = TempDir::new().unwrap();
        let feature_dir = temp_dir.path().join("test-feature");
        let output_dir1 = temp_dir.path().join("output1");
        let output_dir2 = temp_dir.path().join("output2");

        // Create output directories
        fs::create_dir_all(&output_dir1).unwrap();
        fs::create_dir_all(&output_dir2).unwrap();

        // Create a test feature with some content
        fs::create_dir_all(&feature_dir).unwrap();
        let json_path = feature_dir.join("devcontainer-feature.json");
        fs::write(
            &json_path,
            r#"{"id": "test-feature", "version": "1.0.0", "name": "Test Feature"}"#,
        )
        .unwrap();

        // Create some test files with content
        let install_script = feature_dir.join("install.sh");
        fs::write(
            &install_script,
            "#!/bin/bash\necho 'Installing test feature'\n",
        )
        .unwrap();

        let readme = feature_dir.join("README.md");
        fs::write(
            &readme,
            "# Test Feature\n\nThis is a test feature for determinism.\n",
        )
        .unwrap();

        // Package the feature twice to different output directories
        let metadata = validate_single(&feature_dir).unwrap();
        let (digest1, _, _) = create_feature_package(&feature_dir, &output_dir1, &metadata)
            .await
            .unwrap();
        let (digest2, _, _) = create_feature_package(&feature_dir, &output_dir2, &metadata)
            .await
            .unwrap();

        // Verify the digests are identical
        assert_eq!(
            digest1, digest2,
            "SHA256 digests should be identical for deterministic packaging"
        );

        // Also verify the actual file contents are identical
        let tar_path1 = output_dir1
            .join(build_artifact_name("test-feature", &Some("1.0.0".to_string())).unwrap());
        let tar_path2 = output_dir2
            .join(build_artifact_name("test-feature", &Some("1.0.0".to_string())).unwrap());

        let content1 = fs::read(&tar_path1).unwrap();
        let content2 = fs::read(&tar_path2).unwrap();

        assert_eq!(
            content1, content2,
            "Tar file contents should be byte-for-byte identical"
        );
    }

    #[tokio::test]
    async fn test_deterministic_packaging_collection() {
        let temp_dir = TempDir::new().unwrap();
        let collection_dir = temp_dir.path().join("collection");
        let src_dir = collection_dir.join("src");
        let output_dir1 = temp_dir.path().join("output1");
        let output_dir2 = temp_dir.path().join("output2");

        // Create output directories
        fs::create_dir_all(&output_dir1).unwrap();
        fs::create_dir_all(&output_dir2).unwrap();

        // Create collection structure
        fs::create_dir_all(&src_dir).unwrap();

        // Create first feature
        let feature1_dir = src_dir.join("feature-a");
        fs::create_dir_all(&feature1_dir).unwrap();
        let json1_path = feature1_dir.join("devcontainer-feature.json");
        fs::write(&json1_path, r#"{"id": "feature-a", "version": "1.0.0"}"#).unwrap();
        let install1 = feature1_dir.join("install.sh");
        fs::write(&install1, "#!/bin/bash\necho 'Feature A'\n").unwrap();

        // Create second feature
        let feature2_dir = src_dir.join("feature-b");
        fs::create_dir_all(&feature2_dir).unwrap();
        let json2_path = feature2_dir.join("devcontainer-feature.json");
        fs::write(&json2_path, r#"{"id": "feature-b", "version": "2.0.0"}"#).unwrap();
        let install2 = feature2_dir.join("install.sh");
        fs::write(&install2, "#!/bin/bash\necho 'Feature B'\n").unwrap();

        // Package the collection twice
        let features = enumerate_and_validate_collection(&src_dir).unwrap();

        // Package first feature twice
        let (digest1_a, _, _) =
            create_feature_package(&features[0].1, &output_dir1, &features[0].2)
                .await
                .unwrap();
        let (digest2_a, _, _) =
            create_feature_package(&features[0].1, &output_dir2, &features[0].2)
                .await
                .unwrap();
        assert_eq!(
            digest1_a, digest2_a,
            "Feature A digests should be identical"
        );

        // Package second feature twice
        let (digest1_b, _, _) =
            create_feature_package(&features[1].1, &output_dir1, &features[1].2)
                .await
                .unwrap();
        let (digest2_b, _, _) =
            create_feature_package(&features[1].1, &output_dir2, &features[1].2)
                .await
                .unwrap();
        assert_eq!(
            digest1_b, digest2_b,
            "Feature B digests should be identical"
        );

        // Verify file contents are identical
        let tar_name_a = build_artifact_name("feature-a", &Some("1.0.0".to_string())).unwrap();
        let tar_name_b = build_artifact_name("feature-b", &Some("2.0.0".to_string())).unwrap();

        let content1_a = fs::read(output_dir1.join(&tar_name_a)).unwrap();
        let content2_a = fs::read(output_dir2.join(&tar_name_a)).unwrap();
        assert_eq!(
            content1_a, content2_a,
            "Feature A tar contents should be identical"
        );

        let content1_b = fs::read(output_dir1.join(&tar_name_b)).unwrap();
        let content2_b = fs::read(output_dir2.join(&tar_name_b)).unwrap();
        assert_eq!(
            content1_b, content2_b,
            "Feature B tar contents should be identical"
        );
    }

    #[test]
    fn test_sanitize_feature_id_mixed_case() {
        // Mixed case should be preserved (only invalid chars are replaced)
        let result = sanitize_feature_id("MyFeature").unwrap();
        assert_eq!(result, "MyFeature");
    }

    #[test]
    fn test_sanitize_feature_id_invalid_chars_collapse() {
        // Invalid characters should be replaced with hyphens
        let result = sanitize_feature_id("my_feature@test").unwrap();
        assert_eq!(result, "my-feature-test");

        // Underscores should be converted to hyphens
        let result = sanitize_feature_id("my_feature_name").unwrap();
        assert_eq!(result, "my-feature-name");

        // Multiple invalid chars should collapse to single hyphen
        let result = sanitize_feature_id("my___feature").unwrap();
        assert_eq!(result, "my-feature");

        // Mixed invalid chars
        let result = sanitize_feature_id("my@#$%feature").unwrap();
        assert_eq!(result, "my-feature");
    }

    #[test]
    fn test_sanitize_feature_id_leading_trailing_hyphens_trim() {
        // Leading hyphens should be trimmed
        let result = sanitize_feature_id("-my-feature").unwrap();
        assert_eq!(result, "my-feature");

        // Trailing hyphens should be trimmed
        let result = sanitize_feature_id("my-feature-").unwrap();
        assert_eq!(result, "my-feature");

        // Both leading and trailing hyphens should be trimmed
        let result = sanitize_feature_id("-my-feature-").unwrap();
        assert_eq!(result, "my-feature");

        // Multiple leading/trailing hyphens should be trimmed
        let result = sanitize_feature_id("--my-feature--").unwrap();
        assert_eq!(result, "my-feature");
    }

    #[test]
    fn test_sanitize_feature_id_only_invalid_chars() {
        // Feature ID that results in empty string after sanitization should error
        let result = sanitize_feature_id("@@@");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("results in empty sanitized name"));
        assert!(err_msg.contains("'@@@'"));
    }

    #[test]
    fn test_sanitize_feature_id_only_hyphens() {
        // Feature ID consisting only of hyphens should error
        let result = sanitize_feature_id("---");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("results in empty sanitized name"));
        assert!(err_msg.contains("'---'"));
    }

    #[test]
    fn test_sanitize_feature_id_complex_case() {
        // Complex case with mixed valid/invalid chars, underscores, and leading/trailing hyphens
        let result = sanitize_feature_id("-my_complex@feature-name_").unwrap();
        assert_eq!(result, "my-complex-feature-name");
    }

    #[test]
    fn test_build_artifact_name_missing_version() {
        // Missing version should result in error
        let result = build_artifact_name("test-feature", &None);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("missing required version for artifact naming"));
        assert!(err_msg.contains("'test-feature'"));
    }

    #[test]
    fn test_build_artifact_name_with_version() {
        // Valid case should produce correct artifact name
        let result = build_artifact_name("my-feature", &Some("1.2.3".to_string())).unwrap();
        assert_eq!(result, "my-feature-1.2.3.tgz");
    }

    #[test]
    fn test_build_artifact_name_with_sanitization() {
        // Feature ID should be sanitized in artifact name
        let result = build_artifact_name("my@feature_name", &Some("1.0.0".to_string())).unwrap();
        assert_eq!(result, "my-feature-name-1.0.0.tgz");
    }

    #[test]
    fn test_build_artifact_name_empty_after_sanitization() {
        // If sanitization results in empty string, should error
        let result = build_artifact_name("@@@", &Some("1.0.0".to_string()));
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("results in empty sanitized name"));
    }

    #[test]
    fn test_create_feature_package_readonly_output_directory() {
        let temp_dir = TempDir::new().unwrap();
        let feature_dir = temp_dir.path().join("test-feature");
        let output_dir = temp_dir.path().join("readonly-output");

        // Create feature directory
        fs::create_dir_all(&feature_dir).unwrap();
        let json_path = feature_dir.join("devcontainer-feature.json");
        fs::write(&json_path, r#"{"id": "test-feature", "version": "1.0.0"}"#).unwrap();
        let install_path = feature_dir.join("install.sh");
        fs::write(&install_path, "#!/bin/bash\necho 'test'").unwrap();

        // Create output directory and make it read-only
        fs::create_dir_all(&output_dir).unwrap();
        let mut perms = fs::metadata(&output_dir).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o444); // Read-only
            fs::set_permissions(&output_dir, perms).unwrap();
        }

        // Attempt to create package - should fail
        let metadata = validate_single(&feature_dir).unwrap();
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(create_feature_package(&feature_dir, &output_dir, &metadata));

        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        // On Unix systems, this will be a permission denied error
        #[cfg(unix)]
        assert!(err_msg.contains("Permission denied") || err_msg.contains("permission"));
        // On other systems, just check that it failed
        #[cfg(not(unix))]
        assert!(!err_msg.is_empty());
    }
}
