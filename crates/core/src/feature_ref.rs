//! Feature reference type detection and parsing
//!
//! This module provides types and functions for detecting and parsing different types
//! of feature references used in devcontainer.json files. Features can be referenced
//! via OCI registries, local filesystem paths, or HTTPS tarball URLs.
//!
//! # Reference Types
//!
//! - **OCI Registry**: Standard registry references (e.g., `ghcr.io/devcontainers/features/node:18`)
//! - **Local Path**: Relative paths starting with `./` or `../` (e.g., `./my-feature`)
//! - **HTTPS Tarball**: Direct HTTPS URLs to feature tarballs (e.g., `https://example.com/feature.tgz`)
//!
//! # Examples
//!
//! ```
//! use deacon_core::feature_ref::parse_feature_reference;
//!
//! // OCI registry reference
//! let oci_ref = parse_feature_reference("ghcr.io/devcontainers/features/node:18");
//! assert!(oci_ref.is_ok());
//!
//! // Local path reference
//! let local_ref = parse_feature_reference("./my-feature");
//! assert!(local_ref.is_ok());
//!
//! // HTTPS tarball reference
//! let https_ref = parse_feature_reference("https://example.com/feature.tgz");
//! assert!(https_ref.is_ok());
//! ```

use crate::registry_parser::parse_registry_reference;
use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur when parsing feature references
#[derive(Error, Debug)]
pub enum FeatureRefError {
    /// HTTP URLs are not supported (HTTPS required)
    #[error("HTTP not supported, use HTTPS: {0}")]
    HttpNotSupported(String),

    /// Invalid HTTPS URL format
    #[error("Invalid HTTPS URL: {0}")]
    InvalidHttpsUrl(String),

    /// Invalid OCI reference format
    #[error("Invalid OCI reference: {0}")]
    InvalidOciReference(String),

    /// Absolute paths are not supported (must be relative)
    #[error("Absolute paths not supported, use relative paths (./path or ../path): {0}")]
    AbsolutePathNotSupported(String),
}

/// Result type for feature reference operations
pub type Result<T> = std::result::Result<T, FeatureRefError>;

/// Discriminated union for feature reference types
///
/// Feature references in devcontainer.json can be specified in multiple formats.
/// This enum captures the three supported formats and their parsed components.
///
/// # Variants
///
/// - `Oci`: OCI registry reference (e.g., `ghcr.io/devcontainers/features/node:18`)
/// - `LocalPath`: Relative filesystem path (e.g., `./my-feature`, `../shared/feature`)
/// - `HttpsTarball`: HTTPS URL to a feature tarball (e.g., `https://example.com/feature.tgz`)
///
/// # Examples
///
/// ```
/// use deacon_core::feature_ref::{FeatureRefType, parse_feature_reference};
///
/// // Parse different reference types
/// let oci = parse_feature_reference("ghcr.io/devcontainers/features/node:18").unwrap();
/// let local = parse_feature_reference("./my-feature").unwrap();
/// let https = parse_feature_reference("https://example.com/feature.tgz").unwrap();
///
/// // Pattern match on the type
/// match oci {
///     FeatureRefType::Oci(ref_info) => {
///         println!("OCI feature: {}", ref_info.name);
///     },
///     _ => unreachable!(),
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum FeatureRefType {
    /// OCI registry reference with parsed components
    Oci(OciFeatureRef),
    /// Local filesystem path (relative)
    LocalPath(PathBuf),
    /// HTTPS tarball URL
    HttpsTarball(String),
}

/// Parsed components of an OCI feature reference
///
/// OCI references follow the pattern: `[registry/][namespace/]name[:tag]`
/// where registry and namespace may have default values.
///
/// # Examples
///
/// ```
/// use deacon_core::feature_ref::{OciFeatureRef, parse_feature_reference, FeatureRefType};
///
/// let result = parse_feature_reference("ghcr.io/devcontainers/features/node:18").unwrap();
///
/// match result {
///     FeatureRefType::Oci(ref_info) => {
///         assert_eq!(ref_info.registry, "ghcr.io");
///         assert_eq!(ref_info.namespace, "devcontainers/features");
///         assert_eq!(ref_info.name, "node");
///         assert_eq!(ref_info.tag, Some("18".to_string()));
///     },
///     _ => panic!("Expected OCI reference"),
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OciFeatureRef {
    /// Registry hostname (e.g., "ghcr.io")
    pub registry: String,
    /// Namespace path (e.g., "devcontainers/features")
    pub namespace: String,
    /// Feature name (e.g., "node")
    pub name: String,
    /// Optional tag (e.g., "18"), None for "latest"
    pub tag: Option<String>,
}

/// Parse a feature reference string into a typed reference
///
/// Detects and parses feature references from devcontainer.json into one of three types:
/// OCI registry references, local paths, or HTTPS tarball URLs.
///
/// # Detection Rules
///
/// 1. **Local Path**: References starting with `./` or `../` are treated as local paths
/// 2. **HTTPS URL**: References starting with `https://` are treated as tarball URLs
/// 3. **OCI Reference**: All other references are treated as OCI registry references
///
/// # Arguments
///
/// * `reference` - Feature reference string from devcontainer.json features key
///
/// # Returns
///
/// * `Ok(FeatureRefType)` - Successfully parsed and typed reference
/// * `Err(FeatureRefError)` - Invalid reference format
///
/// # Errors
///
/// Returns an error if:
/// - The reference uses `http://` instead of `https://` (HTTP not supported)
/// - The HTTPS URL is malformed
/// - The OCI reference cannot be parsed
/// - An absolute path is provided instead of a relative path
///
/// # Examples
///
/// ```
/// use deacon_core::feature_ref::{parse_feature_reference, FeatureRefType};
///
/// // OCI registry reference
/// let oci = parse_feature_reference("ghcr.io/devcontainers/features/node:18").unwrap();
/// assert!(matches!(oci, FeatureRefType::Oci(_)));
///
/// // Local path reference
/// let local = parse_feature_reference("./my-feature").unwrap();
/// assert!(matches!(local, FeatureRefType::LocalPath(_)));
///
/// // HTTPS tarball reference
/// let https = parse_feature_reference("https://example.com/feature.tgz").unwrap();
/// assert!(matches!(https, FeatureRefType::HttpsTarball(_)));
///
/// // HTTP is not supported (returns error)
/// let result = parse_feature_reference("http://example.com/feature.tgz");
/// assert!(result.is_err());
/// ```
pub fn parse_feature_reference(reference: &str) -> Result<FeatureRefType> {
    // Rule 1: Local path detection (starts with ./ or ../)
    if reference.starts_with("./") || reference.starts_with("../") {
        return Ok(FeatureRefType::LocalPath(PathBuf::from(reference)));
    }

    // Check for absolute paths (not allowed)
    if reference.starts_with('/') {
        return Err(FeatureRefError::AbsolutePathNotSupported(
            reference.to_string(),
        ));
    }

    // Rule 2: HTTPS URL detection (starts with https://)
    if reference.starts_with("https://") {
        // Validate the URL format
        // We use a simple validation here - reqwest::Url will do more thorough validation later
        if reference == "https://" || reference.len() <= 8 {
            return Err(FeatureRefError::InvalidHttpsUrl(reference.to_string()));
        }
        return Ok(FeatureRefType::HttpsTarball(reference.to_string()));
    }

    // Check for HTTP (not allowed - must use HTTPS)
    if reference.starts_with("http://") {
        return Err(FeatureRefError::HttpNotSupported(reference.to_string()));
    }

    // Rule 3: OCI reference (default for everything else)
    let (registry, namespace, name, tag) = parse_registry_reference(reference)
        .map_err(|e| FeatureRefError::InvalidOciReference(format!("{}: {}", reference, e)))?;

    Ok(FeatureRefType::Oci(OciFeatureRef {
        registry,
        namespace,
        name,
        tag,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_oci_reference() {
        // Full OCI reference
        let result = parse_feature_reference("ghcr.io/devcontainers/features/node:18").unwrap();
        match result {
            FeatureRefType::Oci(ref_info) => {
                assert_eq!(ref_info.registry, "ghcr.io");
                assert_eq!(ref_info.namespace, "devcontainers/features");
                assert_eq!(ref_info.name, "node");
                assert_eq!(ref_info.tag, Some("18".to_string()));
            }
            _ => panic!("Expected OCI reference"),
        }

        // Short form with defaults
        let result = parse_feature_reference("node:18").unwrap();
        match result {
            FeatureRefType::Oci(ref_info) => {
                assert_eq!(ref_info.registry, "ghcr.io");
                assert_eq!(ref_info.namespace, "devcontainers");
                assert_eq!(ref_info.name, "node");
                assert_eq!(ref_info.tag, Some("18".to_string()));
            }
            _ => panic!("Expected OCI reference"),
        }

        // Namespace/name form
        let result = parse_feature_reference("myteam/myfeature").unwrap();
        match result {
            FeatureRefType::Oci(ref_info) => {
                assert_eq!(ref_info.registry, "ghcr.io");
                assert_eq!(ref_info.namespace, "myteam");
                assert_eq!(ref_info.name, "myfeature");
                assert_eq!(ref_info.tag, None);
            }
            _ => panic!("Expected OCI reference"),
        }
    }

    #[test]
    fn test_parse_local_path() {
        // Current directory relative
        let result = parse_feature_reference("./my-feature").unwrap();
        match result {
            FeatureRefType::LocalPath(path) => {
                assert_eq!(path, PathBuf::from("./my-feature"));
            }
            _ => panic!("Expected LocalPath reference"),
        }

        // Parent directory relative
        let result = parse_feature_reference("../shared/feature").unwrap();
        match result {
            FeatureRefType::LocalPath(path) => {
                assert_eq!(path, PathBuf::from("../shared/feature"));
            }
            _ => panic!("Expected LocalPath reference"),
        }

        // Deeply nested path
        let result = parse_feature_reference("./deeply/nested/feature").unwrap();
        match result {
            FeatureRefType::LocalPath(path) => {
                assert_eq!(path, PathBuf::from("./deeply/nested/feature"));
            }
            _ => panic!("Expected LocalPath reference"),
        }
    }

    #[test]
    fn test_parse_https_url() {
        // Simple HTTPS URL
        let result = parse_feature_reference("https://example.com/feature.tgz").unwrap();
        match result {
            FeatureRefType::HttpsTarball(url) => {
                assert_eq!(url, "https://example.com/feature.tgz");
            }
            _ => panic!("Expected HttpsTarball reference"),
        }

        // GitHub release URL
        let result = parse_feature_reference(
            "https://github.com/user/repo/releases/download/v1.0/feature.tar.gz",
        )
        .unwrap();
        match result {
            FeatureRefType::HttpsTarball(url) => {
                assert_eq!(
                    url,
                    "https://github.com/user/repo/releases/download/v1.0/feature.tar.gz"
                );
            }
            _ => panic!("Expected HttpsTarball reference"),
        }
    }

    #[test]
    fn test_http_not_supported() {
        let result = parse_feature_reference("http://example.com/feature.tgz");
        assert!(result.is_err());
        assert!(matches!(result, Err(FeatureRefError::HttpNotSupported(_))));
    }

    #[test]
    fn test_absolute_path_not_supported() {
        let result = parse_feature_reference("/absolute/path/feature");
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(FeatureRefError::AbsolutePathNotSupported(_))
        ));
    }

    #[test]
    fn test_invalid_https_url() {
        // Just the protocol
        let result = parse_feature_reference("https://");
        assert!(result.is_err());
        assert!(matches!(result, Err(FeatureRefError::InvalidHttpsUrl(_))));
    }

    #[test]
    fn test_feature_ref_type_equality() {
        let ref1 = parse_feature_reference("./my-feature").unwrap();
        let ref2 = parse_feature_reference("./my-feature").unwrap();
        assert_eq!(ref1, ref2);

        let ref3 = parse_feature_reference("ghcr.io/devcontainers/features/node:18").unwrap();
        let ref4 = parse_feature_reference("ghcr.io/devcontainers/features/node:18").unwrap();
        assert_eq!(ref3, ref4);
    }

    #[test]
    fn test_oci_feature_ref_components() {
        let result = parse_feature_reference("registry.io/org/suborg/name:tag").unwrap();
        match result {
            FeatureRefType::Oci(ref_info) => {
                assert_eq!(ref_info.registry, "registry.io");
                assert_eq!(ref_info.namespace, "org/suborg");
                assert_eq!(ref_info.name, "name");
                assert_eq!(ref_info.tag, Some("tag".to_string()));
            }
            _ => panic!("Expected OCI reference"),
        }
    }
}
