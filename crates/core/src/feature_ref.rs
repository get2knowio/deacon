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

    /// A bare (single-segment) id that is not one of the deprecated v1 Features
    /// the reference CLI still maps. Wording mirrors the reference's own
    /// diagnostic so the two CLIs give a user the same guidance.
    #[error(
        "Legacy feature '{0}' not supported. Please check https://containers.dev/features for replacements.\n\
         If you were hoping to use local Features, remember to prepend your Feature name with \"./\". \
         Please check https://containers.dev/implementors/features-distribution/#addendum-locally-referenced for more information."
    )]
    LegacyFeatureNotSupported(String),
}

/// The v2 collection the deprecated v1 (GitHub Release era) Feature ids live in.
const V1_FEATURE_COLLECTION: &str = "ghcr.io/devcontainers/features";

/// The major version the reference CLI pins a mapped v1 id to.
const V1_FEATURE_MAJOR: &str = "1";

/// Deprecated v1 Feature ids that kept their name in the v2 collection.
///
/// Mirrors `getBackwardCompatibleFeatureId` in devcontainers/cli (measured at
/// v0.87.0). The list is exact, not a prefix rule: an id outside it is rejected
/// rather than turned into a registry path that does not exist.
const DEPRECATED_V1_FEATURE_IDS: &[&str] = &[
    "aws-cli",
    "azure-cli",
    "desktop-lite",
    "docker-in-docker",
    "docker-from-docker",
    "dotnet",
    "git",
    "git-lfs",
    "github-cli",
    "java",
    "kubectl-helm-minikube",
    "node",
    "powershell",
    "python",
    "ruby",
    "rust",
    "sshd",
    "terraform",
];

/// Deprecated v1 Feature ids that were renamed on the way into the v2 collection.
const RENAMED_V1_FEATURE_IDS: &[(&str, &str)] = &[("golang", "go"), ("common", "common-utils")];

/// Canonicalize a `features` map key into the OCI reference to fetch.
///
/// A key carrying any path segment (`ghcr.io/devcontainers/features/node:1`,
/// `myorg/myfeature`) is already an OCI reference and passes through untouched.
/// A bare single-segment key is the deprecated v1 (GitHub Release) form, which
/// the reference CLI resolves through a fixed table onto
/// `ghcr.io/devcontainers/features/<name>:1` — measured at oracle 0.87.0, and
/// applied unconditionally there. `--skip-feature-auto-mapping` does not gate it:
/// the gate exists as `processFeatureIdentifier`'s sixth parameter, but
/// `generateFeaturesConfig` calls that function with five arguments and never
/// supplies it, so the flag is inert (#498). Anything else bare is rejected at
/// ingress, which is the spec's "fatal
/// error" option for a deprecated identifier; deriving a registry path from it
/// only yields a 401 against a repository that does not exist.
///
/// Callers must handle local paths (`./`, `../`, absolute) and `https://`
/// tarballs before reaching here — those are not OCI references at all.
///
/// # Examples
///
/// ```
/// use deacon_core::feature_ref::canonicalize_user_feature_id;
///
/// assert_eq!(
///     canonicalize_user_feature_id("terraform").unwrap(),
///     "ghcr.io/devcontainers/features/terraform:1"
/// );
/// // Renamed on the way into the v2 collection.
/// assert_eq!(
///     canonicalize_user_feature_id("golang").unwrap(),
///     "ghcr.io/devcontainers/features/go:1"
/// );
/// // Already qualified: untouched.
/// assert_eq!(
///     canonicalize_user_feature_id("ghcr.io/devcontainers/features/node:1").unwrap(),
///     "ghcr.io/devcontainers/features/node:1"
/// );
/// // Not a known deprecated id: rejected, not mis-derived.
/// assert!(canonicalize_user_feature_id("my-feature").is_err());
/// ```
pub fn canonicalize_user_feature_id(user_feature_id: &str) -> Result<std::borrow::Cow<'_, str>> {
    if user_feature_id.contains('/') {
        return Ok(std::borrow::Cow::Borrowed(user_feature_id));
    }

    let mapped = if DEPRECATED_V1_FEATURE_IDS.contains(&user_feature_id) {
        Some(user_feature_id)
    } else {
        RENAMED_V1_FEATURE_IDS
            .iter()
            .find(|(old, _)| *old == user_feature_id)
            .map(|(_, new)| *new)
    };

    match mapped {
        Some(name) => Ok(std::borrow::Cow::Owned(format!(
            "{}/{}:{}",
            V1_FEATURE_COLLECTION, name, V1_FEATURE_MAJOR
        ))),
        None => Err(FeatureRefError::LegacyFeatureNotSupported(
            user_feature_id.to_string(),
        )),
    }
}

/// Strip the version component from a `features` map key, the way the reference
/// CLI derives `sourceInformation.userFeatureIdWithoutVersion` from
/// `userFeatureId`.
///
/// The reference's rule is one regular expression, `/[:@][^/]*$/`, applied to the
/// id the CONFIGURATION wrote: everything from the last `:` (tag) or `@` (digest)
/// that is not inside a path segment is dropped, and an id carrying no version at
/// all is returned unchanged. It is deliberately NOT a re-derivation from the
/// resolved reference — reporting `…/git:latest` for an author who wrote `…/git`
/// puts a resolved value in a field whose contract is the declared one (#490,
/// same class as #407 and #491).
///
/// # Examples
///
/// ```
/// use deacon_core::feature_ref::strip_user_feature_id_version;
///
/// assert_eq!(
///     strip_user_feature_id_version("ghcr.io/devcontainers/features/git:1"),
///     "ghcr.io/devcontainers/features/git"
/// );
/// assert_eq!(
///     strip_user_feature_id_version("ghcr.io/devcontainers/features/git@sha256:abc"),
///     "ghcr.io/devcontainers/features/git"
/// );
/// // No version component: unchanged, tag NOT invented.
/// assert_eq!(
///     strip_user_feature_id_version("ghcr.io/devcontainers/features/git"),
///     "ghcr.io/devcontainers/features/git"
/// );
/// // A `:` inside an earlier path segment (a registry port) is not a version.
/// assert_eq!(
///     strip_user_feature_id_version("localhost:5000/features/git"),
///     "localhost:5000/features/git"
/// );
/// // Tag AND digest: the reference's match is LEFTMOST, so both go.
/// assert_eq!(
///     strip_user_feature_id_version("ghcr.io/devcontainers/features/git:1@sha256:abc"),
///     "ghcr.io/devcontainers/features/git"
/// );
/// ```
pub fn strip_user_feature_id_version(user_feature_id: &str) -> &str {
    // The regex matches leftmost-first, so the cut is at the FIRST `:`/`@` of the
    // final path segment — `git:1@sha256:abc` loses the tag and the digest alike.
    let segment_start = user_feature_id
        .rfind('/')
        .map(|idx| idx + 1)
        .unwrap_or_default();
    match user_feature_id[segment_start..].find([':', '@']) {
        Some(idx) => &user_feature_id[..segment_start + idx],
        None => user_feature_id,
    }
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
    // Reject empty or whitespace-only references
    let trimmed = reference.trim();
    if trimmed.is_empty() {
        return Err(FeatureRefError::InvalidOciReference(
            "Feature reference cannot be empty".to_string(),
        ));
    }

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

    // Rule 3: OCI reference (default for everything else). A bare id is the
    // deprecated v1 form and is mapped (or rejected) before parsing.
    let canonical = canonicalize_user_feature_id(reference)?;
    let (registry, namespace, name, tag) = parse_registry_reference(&canonical)
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

        // Bare deprecated v1 id: mapped onto the v2 collection at major 1, the
        // same reference the CLI resolves it to (#491). A bare id carrying a tag
        // (`node:18`) is NOT in the table and is rejected — see
        // `test_bare_id_with_tag_is_rejected`.
        let result = parse_feature_reference("node").unwrap();
        match result {
            FeatureRefType::Oci(ref_info) => {
                assert_eq!(ref_info.registry, "ghcr.io");
                assert_eq!(ref_info.namespace, "devcontainers/features");
                assert_eq!(ref_info.name, "node");
                assert_eq!(ref_info.tag, Some("1".to_string()));
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

    // === Additional Comprehensive Tests ===

    #[test]
    fn test_oci_reference_without_tag() {
        // OCI reference with no tag should parse successfully
        let result = parse_feature_reference("ghcr.io/devcontainers/features/node").unwrap();
        match result {
            FeatureRefType::Oci(ref_info) => {
                assert_eq!(ref_info.registry, "ghcr.io");
                assert_eq!(ref_info.namespace, "devcontainers/features");
                assert_eq!(ref_info.name, "node");
                assert_eq!(ref_info.tag, None);
            }
            _ => panic!("Expected OCI reference"),
        }
    }

    #[test]
    fn test_oci_reference_minimal() {
        // A bare id is only accepted when it is one of the deprecated v1
        // Features; `common` is the renamed one that becomes `common-utils`.
        // `common-utils` itself is NOT a v1 id and must be written in full.
        let result = parse_feature_reference("common").unwrap();
        match result {
            FeatureRefType::Oci(ref_info) => {
                assert_eq!(ref_info.registry, "ghcr.io");
                assert_eq!(ref_info.namespace, "devcontainers/features");
                assert_eq!(ref_info.name, "common-utils");
                assert_eq!(ref_info.tag, Some("1".to_string()));
            }
            _ => panic!("Expected OCI reference"),
        }
        assert!(parse_feature_reference("common-utils").is_err());
    }

    #[test]
    fn test_bare_id_with_tag_is_rejected() {
        // The reference CLI's table is keyed on the exact id, so `node:18`
        // misses it and is rejected rather than mis-derived (#491).
        let err = parse_feature_reference("node:18").unwrap_err();
        assert!(
            matches!(err, FeatureRefError::LegacyFeatureNotSupported(ref id) if id == "node:18"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_unknown_bare_id_is_rejected_with_guidance() {
        let err = canonicalize_user_feature_id("my-feature").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Legacy feature 'my-feature' not supported"),
            "{msg}"
        );
        assert!(
            msg.contains("prepend your Feature name with \"./\""),
            "{msg}"
        );
    }

    #[test]
    fn test_canonicalize_passes_through_qualified_ids() {
        for id in [
            "ghcr.io/devcontainers/features/node:1",
            "myteam/myfeature",
            "ghcr.io/devcontainers/features/docker-in-docker@sha256:abc123",
        ] {
            assert_eq!(canonicalize_user_feature_id(id).unwrap(), id);
        }
    }

    #[test]
    fn test_oci_reference_with_port() {
        // Registry with port number
        let result = parse_feature_reference("localhost:5000/myfeature:latest").unwrap();
        match result {
            FeatureRefType::Oci(ref_info) => {
                assert_eq!(ref_info.registry, "localhost:5000");
                assert_eq!(ref_info.namespace, "devcontainers");
                assert_eq!(ref_info.name, "myfeature");
                assert_eq!(ref_info.tag, Some("latest".to_string()));
            }
            _ => panic!("Expected OCI reference"),
        }
    }

    #[test]
    fn test_oci_reference_complex_tag() {
        // Tag with semver and metadata
        let result =
            parse_feature_reference("ghcr.io/org/feature:1.2.3-alpha.1+build.123").unwrap();
        match result {
            FeatureRefType::Oci(ref_info) => {
                assert_eq!(ref_info.registry, "ghcr.io");
                assert_eq!(ref_info.namespace, "org");
                assert_eq!(ref_info.name, "feature");
                assert_eq!(ref_info.tag, Some("1.2.3-alpha.1+build.123".to_string()));
            }
            _ => panic!("Expected OCI reference"),
        }
    }

    #[test]
    fn test_local_path_nested_directories() {
        // Deeply nested local path
        let result = parse_feature_reference("./features/dev/custom").unwrap();
        match result {
            FeatureRefType::LocalPath(path) => {
                assert_eq!(path, PathBuf::from("./features/dev/custom"));
            }
            _ => panic!("Expected LocalPath reference"),
        }
    }

    #[test]
    fn test_local_path_parent_multiple_levels() {
        // Multiple parent directory references
        let result = parse_feature_reference("../../shared/features/common").unwrap();
        match result {
            FeatureRefType::LocalPath(path) => {
                assert_eq!(path, PathBuf::from("../../shared/features/common"));
            }
            _ => panic!("Expected LocalPath reference"),
        }
    }

    #[test]
    fn test_local_path_current_dir_only() {
        // Just current directory marker with a name
        let result = parse_feature_reference("./feature").unwrap();
        match result {
            FeatureRefType::LocalPath(path) => {
                assert_eq!(path, PathBuf::from("./feature"));
            }
            _ => panic!("Expected LocalPath reference"),
        }
    }

    #[test]
    fn test_local_path_with_spaces() {
        // Path with spaces (valid PathBuf)
        let result = parse_feature_reference("./my feature with spaces").unwrap();
        match result {
            FeatureRefType::LocalPath(path) => {
                assert_eq!(path, PathBuf::from("./my feature with spaces"));
            }
            _ => panic!("Expected LocalPath reference"),
        }
    }

    #[test]
    fn test_https_url_with_port() {
        // HTTPS URL with custom port
        let result = parse_feature_reference("https://example.com:8443/feature.tgz").unwrap();
        match result {
            FeatureRefType::HttpsTarball(url) => {
                assert_eq!(url, "https://example.com:8443/feature.tgz");
            }
            _ => panic!("Expected HttpsTarball reference"),
        }
    }

    #[test]
    fn test_https_url_with_query_params() {
        // HTTPS URL with query parameters
        let result =
            parse_feature_reference("https://example.com/feature.tgz?version=1.0&arch=amd64")
                .unwrap();
        match result {
            FeatureRefType::HttpsTarball(url) => {
                assert_eq!(
                    url,
                    "https://example.com/feature.tgz?version=1.0&arch=amd64"
                );
            }
            _ => panic!("Expected HttpsTarball reference"),
        }
    }

    #[test]
    fn test_https_url_with_fragment() {
        // HTTPS URL with fragment
        let result = parse_feature_reference("https://example.com/feature.tgz#section").unwrap();
        match result {
            FeatureRefType::HttpsTarball(url) => {
                assert_eq!(url, "https://example.com/feature.tgz#section");
            }
            _ => panic!("Expected HttpsTarball reference"),
        }
    }

    #[test]
    fn test_https_url_with_authentication() {
        // HTTPS URL with user info (though not recommended for security)
        let result = parse_feature_reference("https://user:pass@example.com/feature.tgz").unwrap();
        match result {
            FeatureRefType::HttpsTarball(url) => {
                assert_eq!(url, "https://user:pass@example.com/feature.tgz");
            }
            _ => panic!("Expected HttpsTarball reference"),
        }
    }

    #[test]
    fn test_https_url_github_raw() {
        // GitHub raw content URL
        let result = parse_feature_reference(
            "https://raw.githubusercontent.com/user/repo/main/feature.tar.gz",
        )
        .unwrap();
        match result {
            FeatureRefType::HttpsTarball(url) => {
                assert_eq!(
                    url,
                    "https://raw.githubusercontent.com/user/repo/main/feature.tar.gz"
                );
            }
            _ => panic!("Expected HttpsTarball reference"),
        }
    }

    #[test]
    fn test_https_url_long_path() {
        // HTTPS URL with long nested path
        let result = parse_feature_reference(
            "https://cdn.example.com/releases/2024/01/features/v1.0/stable/feature.tgz",
        )
        .unwrap();
        match result {
            FeatureRefType::HttpsTarball(url) => {
                assert_eq!(
                    url,
                    "https://cdn.example.com/releases/2024/01/features/v1.0/stable/feature.tgz"
                );
            }
            _ => panic!("Expected HttpsTarball reference"),
        }
    }

    // === Invalid Input Tests ===

    #[test]
    fn test_http_with_port_not_supported() {
        // HTTP with port should still be rejected
        let result = parse_feature_reference("http://localhost:8080/feature.tgz");
        assert!(result.is_err());
        assert!(matches!(result, Err(FeatureRefError::HttpNotSupported(_))));
    }

    #[test]
    fn test_absolute_path_unix() {
        // Unix absolute path
        let result = parse_feature_reference("/usr/local/features/custom");
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(FeatureRefError::AbsolutePathNotSupported(_))
        ));
    }

    #[test]
    fn test_absolute_path_with_spaces() {
        // Absolute path with spaces
        let result = parse_feature_reference("/usr/local/my feature");
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(FeatureRefError::AbsolutePathNotSupported(_))
        ));
    }

    #[test]
    fn test_windows_style_path_rejected() {
        // Windows-style absolute path (should be treated as OCI and fail parsing)
        // Note: This will be parsed as an OCI reference and likely fail in parse_registry_reference
        let result = parse_feature_reference("C:\\features\\custom");
        // The exact error depends on how parse_registry_reference handles this
        // It should either be InvalidOciReference or could potentially succeed with weird parsing
        // Let's just verify it doesn't become a LocalPath
        // Any result other than LocalPath is acceptable (error or weird OCI parse)
        if let Ok(FeatureRefType::LocalPath(_)) = result {
            panic!("Windows paths should not be recognized as local paths")
        }
    }

    #[test]
    fn test_invalid_https_empty_host() {
        // HTTPS with just slashes
        let result = parse_feature_reference("https:///feature.tgz");
        // This might pass our simple validation but would fail later
        // Our current implementation just checks length > 8
        match result {
            Ok(FeatureRefType::HttpsTarball(url)) => {
                assert_eq!(url, "https:///feature.tgz");
            }
            Err(FeatureRefError::InvalidHttpsUrl(_)) => {
                // Also acceptable
            }
            _ => panic!("Unexpected result type"),
        }
    }

    #[test]
    fn test_https_short_but_valid() {
        // Short but technically valid HTTPS URL
        let result = parse_feature_reference("https://a.b/c").unwrap();
        match result {
            FeatureRefType::HttpsTarball(url) => {
                assert_eq!(url, "https://a.b/c");
            }
            _ => panic!("Expected HttpsTarball reference"),
        }
    }

    #[test]
    fn test_mixed_slashes_not_local_path() {
        // Path that doesn't start with ./ or ../
        let result = parse_feature_reference("some/path/feature").unwrap();
        // Should be parsed as OCI reference (namespace/name format)
        match result {
            FeatureRefType::Oci(ref_info) => {
                assert_eq!(ref_info.namespace, "some/path");
                assert_eq!(ref_info.name, "feature");
            }
            _ => panic!("Expected OCI reference"),
        }
    }

    #[test]
    fn test_dot_only_not_local_path() {
        // A single dot without a slash is not a local path. It falls through to
        // the OCI branch, where — being bare and not a known deprecated v1 id —
        // it is rejected rather than silently pointed at a registry (#491).
        let err = parse_feature_reference(".feature").unwrap_err();
        assert!(
            matches!(err, FeatureRefError::LegacyFeatureNotSupported(_)),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_double_dot_only_not_local_path() {
        // Same as above for a leading `..` with no slash.
        let err = parse_feature_reference("..feature").unwrap_err();
        assert!(
            matches!(err, FeatureRefError::LegacyFeatureNotSupported(_)),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_oci_with_digest() {
        // OCI reference with digest instead of tag
        let result =
            parse_feature_reference("ghcr.io/devcontainers/features/node@sha256:abc123def456")
                .unwrap();
        match result {
            FeatureRefType::Oci(ref_info) => {
                assert_eq!(ref_info.registry, "ghcr.io");
                assert_eq!(ref_info.namespace, "devcontainers/features");
                assert_eq!(ref_info.name, "node");
                // Digest handling depends on parse_registry_reference implementation
                // Just verify it's parsed as OCI
            }
            _ => panic!("Expected OCI reference"),
        }
    }

    #[test]
    fn test_feature_ref_clone() {
        // Test that FeatureRefType implements Clone properly
        let ref1 = parse_feature_reference("./my-feature").unwrap();
        let ref2 = ref1.clone();
        assert_eq!(ref1, ref2);
    }

    #[test]
    fn test_feature_ref_debug() {
        // Test that FeatureRefType implements Debug properly
        let ref1 = parse_feature_reference("ghcr.io/test/feature:1.0").unwrap();
        let debug_str = format!("{:?}", ref1);
        assert!(debug_str.contains("Oci"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_oci_feature_ref_equality() {
        // Test OciFeatureRef Eq implementation
        let ref1 = parse_feature_reference("ghcr.io/test/feature:1.0").unwrap();
        let ref2 = parse_feature_reference("ghcr.io/test/feature:1.0").unwrap();

        match (ref1, ref2) {
            (FeatureRefType::Oci(oci1), FeatureRefType::Oci(oci2)) => {
                assert_eq!(oci1, oci2);
            }
            _ => panic!("Expected OCI references"),
        }
    }

    #[test]
    fn test_local_path_with_trailing_slash() {
        // Local path with trailing slash
        let result = parse_feature_reference("./my-feature/").unwrap();
        match result {
            FeatureRefType::LocalPath(path) => {
                assert_eq!(path, PathBuf::from("./my-feature/"));
            }
            _ => panic!("Expected LocalPath reference"),
        }
    }

    #[test]
    fn test_parent_path_with_trailing_slash() {
        // Parent path with trailing slash
        let result = parse_feature_reference("../shared-feature/").unwrap();
        match result {
            FeatureRefType::LocalPath(path) => {
                assert_eq!(path, PathBuf::from("../shared-feature/"));
            }
            _ => panic!("Expected LocalPath reference"),
        }
    }

    #[test]
    fn test_https_url_with_multiple_extensions() {
        // HTTPS URL with multiple file extensions
        let result = parse_feature_reference("https://example.com/feature.tar.gz.gpg").unwrap();
        match result {
            FeatureRefType::HttpsTarball(url) => {
                assert_eq!(url, "https://example.com/feature.tar.gz.gpg");
            }
            _ => panic!("Expected HttpsTarball reference"),
        }
    }

    #[test]
    fn test_ftp_not_supported() {
        // FTP should be parsed as OCI (and likely fail)
        let result = parse_feature_reference("ftp://example.com/feature.tgz");
        match result {
            Ok(FeatureRefType::HttpsTarball(_)) => {
                panic!("FTP should not be recognized as HTTPS tarball")
            }
            Ok(FeatureRefType::LocalPath(_)) => {
                panic!("FTP should not be recognized as local path")
            }
            _ => {} // OCI or error is acceptable
        }
    }

    #[test]
    fn test_file_url_not_supported() {
        // file:// URL should be parsed as OCI (and likely fail)
        let result = parse_feature_reference("file:///path/to/feature");
        match result {
            Ok(FeatureRefType::LocalPath(_)) => {
                panic!("file:// URLs should not be recognized as local paths")
            }
            Ok(FeatureRefType::HttpsTarball(_)) => {
                panic!("file:// URLs should not be recognized as HTTPS tarballs")
            }
            _ => {} // OCI or error is acceptable
        }
    }

    #[test]
    fn test_empty_string_error() {
        // Empty string should fail
        let result = parse_feature_reference("");
        assert!(result.is_err());
    }

    #[test]
    fn test_whitespace_only_error() {
        // Whitespace-only string should fail
        let result = parse_feature_reference("   ");
        assert!(result.is_err());
    }

    #[test]
    fn test_local_path_with_special_chars() {
        // Local path with hyphens, underscores, dots
        let result = parse_feature_reference("./my-custom_feature.v2").unwrap();
        match result {
            FeatureRefType::LocalPath(path) => {
                assert_eq!(path, PathBuf::from("./my-custom_feature.v2"));
            }
            _ => panic!("Expected LocalPath reference"),
        }
    }

    #[test]
    fn test_oci_reference_uppercase() {
        // OCI references with uppercase (should be handled by parse_registry_reference)
        let result = parse_feature_reference("GHCR.IO/ORG/FEATURE:TAG").unwrap();
        match result {
            FeatureRefType::Oci(_) => {
                // Parsed successfully as OCI
            }
            _ => panic!("Expected OCI reference"),
        }
    }

    #[test]
    fn test_https_url_localhost() {
        // HTTPS URL to localhost
        let result = parse_feature_reference("https://localhost/feature.tgz").unwrap();
        match result {
            FeatureRefType::HttpsTarball(url) => {
                assert_eq!(url, "https://localhost/feature.tgz");
            }
            _ => panic!("Expected HttpsTarball reference"),
        }
    }

    #[test]
    fn test_https_url_ip_address() {
        // HTTPS URL with IP address
        let result = parse_feature_reference("https://192.168.1.1/feature.tgz").unwrap();
        match result {
            FeatureRefType::HttpsTarball(url) => {
                assert_eq!(url, "https://192.168.1.1/feature.tgz");
            }
            _ => panic!("Expected HttpsTarball reference"),
        }
    }

    #[test]
    fn test_https_url_ipv6() {
        // HTTPS URL with IPv6 address
        let result = parse_feature_reference("https://[::1]/feature.tgz").unwrap();
        match result {
            FeatureRefType::HttpsTarball(url) => {
                assert_eq!(url, "https://[::1]/feature.tgz");
            }
            _ => panic!("Expected HttpsTarball reference"),
        }
    }

    #[test]
    fn test_case_sensitivity_oci() {
        // Test case sensitivity for OCI references
        let lower = parse_feature_reference("myorg/myfeature:v1").unwrap();
        let upper = parse_feature_reference("MyOrg/MyFeature:v1").unwrap();
        // They should both parse but be different
        assert_ne!(lower, upper);
    }

    /// #490: `userFeatureIdWithoutVersion` is derived from the id the
    /// CONFIGURATION wrote by stripping its version component — the reference's
    /// `/[:@][^/]*$/`, leftmost-first — and never re-derived from the resolved
    /// reference, which would invent a `:latest` for an untagged id.
    #[test]
    fn test_strip_user_feature_id_version() {
        for (authored, expected) in [
            // Untagged: unchanged. The whole point of #490.
            (
                "ghcr.io/devcontainers/features/git",
                "ghcr.io/devcontainers/features/git",
            ),
            (
                "ghcr.io/devcontainers/features/git:1",
                "ghcr.io/devcontainers/features/git",
            ),
            (
                "ghcr.io/devcontainers/features/git@sha256:abc",
                "ghcr.io/devcontainers/features/git",
            ),
            // Tag AND digest: the reference's match is leftmost, so both go.
            (
                "ghcr.io/devcontainers/features/git:1@sha256:abc",
                "ghcr.io/devcontainers/features/git",
            ),
            // A registry port lives in an earlier segment and is not a version.
            ("localhost:5000/features/git", "localhost:5000/features/git"),
            (
                "localhost:5000/features/git:2",
                "localhost:5000/features/git",
            ),
            // A bare deprecated v1 id carries no version at all (#491).
            ("terraform", "terraform"),
        ] {
            assert_eq!(
                strip_user_feature_id_version(authored),
                expected,
                "stripping {authored}"
            );
        }
    }

    #[test]
    fn test_special_oci_names() {
        // OCI reference with underscores, hyphens, dots
        let result = parse_feature_reference("my.org/my_namespace/my-feature.name:v1.0").unwrap();
        match result {
            FeatureRefType::Oci(ref_info) => {
                assert_eq!(ref_info.registry, "my.org");
                assert_eq!(ref_info.namespace, "my_namespace");
                assert_eq!(ref_info.name, "my-feature.name");
            }
            _ => panic!("Expected OCI reference"),
        }
    }
}
