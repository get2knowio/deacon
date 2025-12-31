//! Semantic version utilities for OCI registry operations
//!
//! This module provides utilities for parsing, filtering, sorting, and comparing
//! semantic version tags commonly found in OCI registries.
//!
//! ## Features
//!
//! - Parse versions from various formats ("v1.2.3", "1.2.3", "1.2", "1")
//! - Filter tags to only semantic versions
//! - Sort tags in semantic version order
//! - Compute semantic tags (e.g., "1.2.3" → ["1", "1.2", "1.2.3", "latest"])
//! - Compare versions
//!
//! ## Examples
//!
//! ```rust
//! use deacon_core::semver_utils;
//!
//! // Parse a version
//! let version = semver_utils::parse_version("v1.2.3");
//! assert!(version.is_some());
//!
//! // Filter and sort tags
//! let tags = vec!["1.0.0".to_string(), "latest".to_string(), "2.0.0".to_string()];
//! let semver_tags = semver_utils::filter_semver_tags(&tags);
//! let mut sorted_tags = semver_tags.clone();
//! semver_utils::sort_tags_descending(&mut sorted_tags);
//!
//! // Compute semantic tags for publishing
//! let tags = semver_utils::compute_semantic_tags("1.2.3");
//! assert_eq!(tags, vec!["1", "1.2", "1.2.3", "latest"]);
//! ```

use semver::Version;
use std::cmp::Ordering;

/// Parse a semantic version from a tag string
///
/// Handles tags like "v1.2.3", "1.2.3", "1.2", "1"
///
/// # Examples
///
/// ```rust
/// use deacon_core::semver_utils::parse_version;
///
/// assert!(parse_version("1.2.3").is_some());
/// assert!(parse_version("v1.2.3").is_some());
/// assert!(parse_version("1.2").is_some());
/// assert!(parse_version("1").is_some());
/// assert!(parse_version("invalid").is_none());
/// ```
pub fn parse_version(tag: &str) -> Option<Version> {
    // Strip leading 'v' if present
    let version_str = tag.strip_prefix('v').unwrap_or(tag);

    // Try direct parse first
    if let Ok(version) = Version::parse(version_str) {
        return Some(version);
    }

    // Try with .0 suffix for major.minor versions
    if let Ok(version) = Version::parse(&format!("{}.0", version_str)) {
        return Some(version);
    }

    // Try with .0.0 suffix for major versions
    if let Ok(version) = Version::parse(&format!("{}.0.0", version_str)) {
        return Some(version);
    }

    None
}

/// Filter tags to only semantic versions
///
/// # Examples
///
/// ```rust
/// use deacon_core::semver_utils::filter_semver_tags;
///
/// let tags = vec![
///     "1.2.3".to_string(),
///     "latest".to_string(),
///     "v2.0.0".to_string(),
///     "dev".to_string(),
/// ];
/// let filtered = filter_semver_tags(&tags);
/// assert_eq!(filtered.len(), 2);
/// ```
pub fn filter_semver_tags(tags: &[String]) -> Vec<String> {
    tags.iter()
        .filter(|tag| parse_version(tag).is_some())
        .cloned()
        .collect()
}

/// Sort tags in descending semantic version order
///
/// Valid semantic versions are sorted first in descending order,
/// followed by non-semantic version tags in reverse lexical order.
///
/// # Examples
///
/// ```rust
/// use deacon_core::semver_utils::sort_tags_descending;
///
/// let mut tags = vec![
///     "1.0.0".to_string(),
///     "2.1.0".to_string(),
///     "1.5.0".to_string(),
/// ];
/// sort_tags_descending(&mut tags);
/// assert_eq!(tags[0], "2.1.0");
/// assert_eq!(tags[1], "1.5.0");
/// assert_eq!(tags[2], "1.0.0");
/// ```
pub fn sort_tags_descending(tags: &mut [String]) {
    tags.sort_by(|a, b| {
        match (parse_version(a), parse_version(b)) {
            (Some(v_a), Some(v_b)) => v_b.cmp(&v_a), // Reverse for descending
            (Some(_), None) => Ordering::Less,       // Valid versions come first
            (None, Some(_)) => Ordering::Greater,
            (None, None) => b.cmp(a), // Fallback to string comparison
        }
    });
}

/// Compute semantic version tags from a version string
///
/// Returns `[major, major.minor, major.minor.patch]` and optionally `latest` for stable versions.
/// Pre-release versions (with pre-release identifiers) do not include `latest`.
///
/// # Examples
///
/// ```rust
/// use deacon_core::semver_utils::compute_semantic_tags;
///
/// let tags = compute_semantic_tags("1.2.3");
/// assert_eq!(tags, vec!["1", "1.2", "1.2.3", "latest"]);
///
/// let tags = compute_semantic_tags("2.0.0-rc.1");
/// assert_eq!(tags, vec!["2", "2.0", "2.0.0-rc.1"]);
///
/// // Invalid versions return only "latest"
/// let tags = compute_semantic_tags("invalid");
/// assert_eq!(tags, vec!["latest"]);
/// ```
pub fn compute_semantic_tags(version: &str) -> Vec<String> {
    let version = parse_version(version);
    if version.is_none() {
        return vec!["latest".to_string()];
    }

    let version = version.unwrap();
    let mut tags = vec![
        format!("{}", version.major),
        format!("{}.{}", version.major, version.minor),
        version.to_string(),
    ];

    // Only include "latest" for stable releases (no pre-release identifiers)
    if version.pre.is_empty() {
        tags.push("latest".to_string());
    }

    tags
}

/// Compare two version tags
///
/// Returns `Ordering::Greater` if a > b, `Ordering::Less` if a < b, `Ordering::Equal` if equal.
/// Valid semantic versions are considered greater than non-semantic versions.
///
/// # Examples
///
/// ```rust
/// use deacon_core::semver_utils::compare_versions;
/// use std::cmp::Ordering;
///
/// assert_eq!(compare_versions("1.2.3", "1.2.4"), Ordering::Less);
/// assert_eq!(compare_versions("2.0.0", "1.9.9"), Ordering::Greater);
/// assert_eq!(compare_versions("1.0.0", "1.0.0"), Ordering::Equal);
///
/// // Valid version is greater than invalid
/// assert_eq!(compare_versions("1.0.0", "invalid"), Ordering::Greater);
/// ```
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    match (parse_version(a), parse_version(b)) {
        (Some(v_a), Some(v_b)) => v_a.cmp(&v_b),
        (Some(_), None) => Ordering::Greater, // Valid version is greater than invalid
        (None, Some(_)) => Ordering::Less,
        (None, None) => a.cmp(b), // Fallback to string comparison
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_standard() {
        assert!(parse_version("1.2.3").is_some());
        assert!(parse_version("v1.2.3").is_some());
        assert_eq!(parse_version("1.2.3").unwrap().to_string(), "1.2.3");
    }

    #[test]
    fn test_parse_version_short() {
        assert!(parse_version("1.2").is_some());
        assert!(parse_version("1").is_some());
        assert_eq!(parse_version("1.2").unwrap().to_string(), "1.2.0");
        assert_eq!(parse_version("1").unwrap().to_string(), "1.0.0");
    }

    #[test]
    fn test_parse_version_invalid() {
        assert!(parse_version("invalid").is_none());
        assert!(parse_version("v").is_none());
        assert!(parse_version("").is_none());
    }

    #[test]
    fn test_filter_semver_tags() {
        let tags = vec![
            "1.2.3".to_string(),
            "v2.0.0".to_string(),
            "latest".to_string(),
            "dev".to_string(),
            "1.0".to_string(),
        ];
        let filtered = filter_semver_tags(&tags);
        assert_eq!(filtered.len(), 3);
        assert!(filtered.contains(&"1.2.3".to_string()));
        assert!(filtered.contains(&"v2.0.0".to_string()));
        assert!(filtered.contains(&"1.0".to_string()));
    }

    #[test]
    fn test_sort_tags_descending() {
        let mut tags = vec![
            "1.0.0".to_string(),
            "2.1.0".to_string(),
            "1.5.0".to_string(),
            "2.0.0".to_string(),
        ];
        sort_tags_descending(&mut tags);
        assert_eq!(tags[0], "2.1.0");
        assert_eq!(tags[1], "2.0.0");
        assert_eq!(tags[2], "1.5.0");
        assert_eq!(tags[3], "1.0.0");
    }

    #[test]
    fn test_compute_semantic_tags() {
        let tags = compute_semantic_tags("1.2.3");
        assert_eq!(tags.len(), 4);
        assert_eq!(tags[0], "1");
        assert_eq!(tags[1], "1.2");
        assert_eq!(tags[2], "1.2.3");
        assert_eq!(tags[3], "latest");
    }

    #[test]
    fn test_compute_semantic_tags_with_v_prefix() {
        let tags = compute_semantic_tags("v2.5.1");
        assert_eq!(tags.len(), 4);
        assert_eq!(tags[0], "2");
        assert_eq!(tags[1], "2.5");
        assert_eq!(tags[2], "2.5.1");
        assert_eq!(tags[3], "latest");
    }

    #[test]
    fn test_compute_semantic_tags_pre_release() {
        let tags = compute_semantic_tags("1.2.3-rc.1");
        assert_eq!(tags.len(), 3);
        assert_eq!(tags[0], "1");
        assert_eq!(tags[1], "1.2");
        assert_eq!(tags[2], "1.2.3-rc.1");
        // No "latest" for pre-release
    }

    #[test]
    fn test_compare_versions() {
        assert_eq!(compare_versions("1.2.3", "1.2.4"), Ordering::Less);
        assert_eq!(compare_versions("2.0.0", "1.9.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.0", "1.0.0"), Ordering::Equal);
    }

    #[test]
    fn test_compare_versions_with_invalid() {
        // Valid version is greater than invalid
        assert_eq!(compare_versions("1.0.0", "invalid"), Ordering::Greater);
        assert_eq!(compare_versions("invalid", "1.0.0"), Ordering::Less);
    }
}
