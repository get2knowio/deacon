//! Core helpers for the `outdated` subcommand
//!
//! Provides data structures and helper functions used by the CLI command
//! implementation in `crates/deacon` to compute current/wanted/latest versions
//! for Dev Container Features.

use crate::lockfile;
use crate::oci::{default_fetcher, FeatureRef};
use crate::semver_utils;
use semver::Version;

/// Per-feature version information reported by `outdated`
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FeatureVersionInfo {
    /// Canonical fully-qualified feature id without version (e.g. "ghcr.io/devcontainers/features/node")
    pub id: String,

    /// Current version (lockfile or wanted fallback)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,

    /// Wanted version derived from configuration (tags/digest rules)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wanted: Option<String>,

    /// Latest stable semver tag discovered in registry
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest: Option<String>,

    /// Wanted major version (e.g., "1" or "2")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wanted_major: Option<String>,

    /// Latest major version (e.g., "1" or "2")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_major: Option<String>,
}

/// Aggregate outdated result (simple vector form)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutdatedResult {
    pub features: Vec<FeatureVersionInfo>,
}

/// Compute a canonical feature id with no version information.
///
/// Examples:
/// - `ghcr.io/devcontainers/features/node:1.2.3` -> `ghcr.io/devcontainers/features/node`
/// - `ghcr.io/devcontainers/features/node@sha256:abcd...` -> `ghcr.io/devcontainers/features/node`
pub fn canonical_feature_id(reference: &str) -> String {
    // If contains '@', split there first
    if let Some(idx) = reference.find('@') {
        return reference[..idx].to_string();
    }

    // Otherwise, we need to strip a tag if present. Tags are separated by ':' but registry hostnames
    // may contain ':' for ports. We decide based on the position of the last '/' vs last ':'
    let last_slash = reference.rfind('/');
    let last_colon = reference.rfind(':');

    if let (Some(slash_idx), Some(colon_idx)) = (last_slash, last_colon) {
        if colon_idx > slash_idx {
            // colon after last slash -> this is a tag separator
            return reference[..colon_idx].to_string();
        }
    }

    // No digest or tag detected; return as-is
    reference.to_string()
}

/// Compute the "wanted" version for a declared feature reference.
///
/// Heuristics:
/// - If the declared reference contains a tag (e.g., `:1.2.3`), return that tag without `v` prefix
/// - If the declared reference contains a digest (`@sha256:...`), return `None` (can't infer wanted)
/// - Otherwise return `None`
pub fn compute_wanted_version(reference: &str) -> Option<String> {
    if reference.contains('@') {
        // Digest-based reference: wanted cannot be derived without registry metadata
        return None;
    }

    // Try to find a tag after the last ':' when it's after the last '/'
    if let Some(last_colon) = reference.rfind(':') {
        if let Some(last_slash) = reference.rfind('/') {
            if last_colon > last_slash {
                let mut tag = reference[last_colon + 1..].to_string();
                // Strip leading 'v' for normalization (e.g., v1.2.3 -> 1.2.3)
                if tag.starts_with('v') {
                    tag = tag.trim_start_matches('v').to_string();
                }
                return Some(tag);
            }
        }
    }

    None
}

/// Return the major portion of a version string, if parseable as semver.
pub fn wanted_major(version: &Option<String>) -> Option<String> {
    version
        .as_ref()
        .and_then(|v| semver_utils::parse_version(v).map(|ver: Version| format!("{}", ver.major)))
}

/// Derive the "current" version for a declared feature given an optional lockfile.
///
/// Logic:
/// - If lockfile contains an entry for the canonical id, return its `version` field
/// - Otherwise, fall back to `compute_wanted_version(reference)`
pub fn derive_current_version(
    reference: &str,
    lockfile_opt: Option<&lockfile::Lockfile>,
) -> Option<String> {
    let canonical = canonical_feature_id(reference);
    if let Some(ld) = lockfile_opt {
        if let Some(entry) = ld.features.get(&canonical) {
            return Some(entry.version.clone());
        }
    }

    // Fallback
    compute_wanted_version(reference)
}

/// Fetch the latest stable semver tag for the given declared feature reference.
///
/// This performs a registry `list_tags` via the core `FeatureFetcher` and uses
/// `semver_utils` to filter and sort tags. Returns `None` on any error or if
/// no stable semver tags are present.
pub async fn fetch_latest_stable_version(reference: &str) -> Option<String> {
    // Build a FeatureRef for listing tags. We make a best-effort parse: registry is the
    // first path segment, repository is the remainder. For nested namespaces we keep them.
    let canonical = canonical_feature_id(reference);
    let mut parts = canonical.splitn(2, '/');
    let registry = parts.next()?.to_string();
    let remainder = parts.next()?; // e.g., "devcontainers/features/node"

    // Namespace is everything except the final segment
    let mut repo_parts: Vec<&str> = remainder.split('/').collect();
    if repo_parts.is_empty() {
        return None;
    }
    let name = if let Some(last) = repo_parts.pop() {
        last.to_string()
    } else {
        return None;
    };
    let namespace = repo_parts.join("/");

    // Try to create a feature ref with no specific version (uses default tag 'latest')
    let feature_ref = FeatureRef::new(registry, namespace, name, None);

    // Use default fetcher with sensible defaults
    let fetcher = match default_fetcher() {
        Ok(f) => f,
        Err(_) => return None,
    };

    // list_tags is async and may fail; swallow errors and return None per spec (nulls in JSON)
    let tags = match fetcher.list_tags(&feature_ref).await {
        Ok(t) => t,
        Err(_) => return None,
    };

    // Filter semver tags and sort descending (exclude pre-releases from "latest")
    let mut semver_tags: Vec<String> = semver_utils::filter_semver_tags(&tags)
        .into_iter()
        .filter(|t| {
            if let Some(ver) = semver_utils::parse_version(t) {
                ver.pre.is_empty()
            } else {
                false
            }
        })
        .collect();
    if semver_tags.is_empty() {
        return None;
    }
    semver_utils::sort_tags_descending(&mut semver_tags);

    // First element is the highest stable semver
    semver_tags.into_iter().next()
}

/// Return the major portion of the latest stable version string
pub fn latest_major(version: &Option<String>) -> Option<String> {
    version
        .as_ref()
        .and_then(|v| semver_utils::parse_version(v).map(|ver: Version| format!("{}", ver.major)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonical_feature_id_tag() {
        let input = "ghcr.io/devcontainers/features/node:1.2.3";
        assert_eq!(
            canonical_feature_id(input),
            "ghcr.io/devcontainers/features/node"
        );
    }

    #[test]
    fn test_canonical_feature_id_digest() {
        let input = "ghcr.io/devcontainers/features/node@sha256:abcdef";
        assert_eq!(
            canonical_feature_id(input),
            "ghcr.io/devcontainers/features/node"
        );
    }

    #[test]
    fn test_compute_wanted_version_tag() {
        let input = "ghcr.io/devcontainers/features/node:18";
        assert_eq!(compute_wanted_version(input).unwrap(), "18");
    }

    #[test]
    fn test_compute_wanted_version_digest() {
        let input = "ghcr.io/devcontainers/features/node@sha256:abcdef";
        assert!(compute_wanted_version(input).is_none());
    }
}
