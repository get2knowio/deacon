//! Core helpers for the `outdated` subcommand
//!
//! Provides data structures and helper functions used by the CLI command
//! implementation in `crates/deacon` to compute current/wanted/latest versions
//! for Dev Container Features.

use crate::lockfile;
use crate::oci::{FeatureRef, default_fetcher};
use crate::semver_utils;
use semver::Version;

/// Per-feature version information reported by `outdated`
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FeatureVersionInfo {
    /// The feature identifier exactly as the configuration declared it, tag and
    /// all (e.g. `"ghcr.io/devcontainers/features/node:1.6.1"`).
    ///
    /// This is the DECLARED reference, not the canonical untagged id, and the
    /// distinction is load-bearing (#407). Two Features that differ only by tag
    /// are distinct keys in `devcontainer.json` but collapse to one canonical
    /// id, so reporting canonically dropped a declared Feature from the report
    /// entirely — with a zero exit and no warning. It also matches the reference
    /// CLI, which echoes the declared reference.
    ///
    /// The lockfile lookup still keys on the canonical id (see
    /// [`derive_current_version`]); only what is REPORTED changed.
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

/// Check if a feature reference is a valid OCI registry reference.
///
/// Returns `true` for OCI registry references (e.g., `ghcr.io/owner/feature:tag`),
/// `false` for local paths (e.g., `./local-feature`), direct URLs (e.g., `https://...`),
/// or other non-OCI identifiers.
///
/// Per spec §9 and §14, only OCI-based identifiers can be enumerated for versions.
/// Local paths, direct tarballs, and legacy identifiers are not versionable and should
/// be filtered out.
///
/// # Examples
///
/// ```
/// use deacon_core::outdated::is_oci_feature_ref;
///
/// // Valid OCI references
/// assert!(is_oci_feature_ref("ghcr.io/devcontainers/features/node:1.2.3"));
/// assert!(is_oci_feature_ref("ghcr.io/devcontainers/features/node"));
/// assert!(is_oci_feature_ref("registry.io/ns/feature@sha256:abcd"));
/// assert!(is_oci_feature_ref("devcontainers/features/node")); // implicit registry
///
/// // Invalid non-OCI references (should be skipped)
/// assert!(!is_oci_feature_ref("./local-feature"));
/// assert!(!is_oci_feature_ref("../relative/path"));
/// assert!(!is_oci_feature_ref("/absolute/path"));
/// assert!(!is_oci_feature_ref("https://example.com/feature.tgz"));
/// assert!(!is_oci_feature_ref("http://example.com/feature.tgz"));
/// assert!(!is_oci_feature_ref("file:///path/to/feature"));
/// ```
pub fn is_oci_feature_ref(reference: &str) -> bool {
    // Filter out local file paths
    if reference.starts_with("./") || reference.starts_with("../") || reference.starts_with('/') {
        return false;
    }

    // Filter out direct HTTP(S) URLs
    if reference.starts_with("http://") || reference.starts_with("https://") {
        return false;
    }

    // Filter out file:// URLs
    if reference.starts_with("file://") {
        return false;
    }

    // Must contain at least one '/' to be a valid OCI ref (namespace/name at minimum)
    // This filters out bare names like "myfeature" which are legacy forms
    // Note: Actually, per the registry_parser.rs, bare names are allowed and default
    // to ghcr.io/devcontainers/myfeature, so we should allow them
    // The key discriminator is really the scheme-based checks above

    // Additional check: if it looks like a Windows path (e.g., C:\path), skip it
    #[cfg(target_os = "windows")]
    {
        if reference.len() >= 3
            && reference.chars().nth(1) == Some(':')
            && reference.chars().nth(2) == Some('\\')
        {
            return false;
        }
    }

    true
}

pub use crate::features::canonical_feature_id;

/// Compute the "wanted" version for a declared feature reference.
///
/// Extracts version information from tag-based references. Leading `v` prefixes are stripped
/// for normalization (e.g., `v1.2.3` becomes `1.2.3`).
///
/// # Returns
///
/// - `Some(version)` if a tag is present (e.g., `:1.2.3`)
/// - `None` if digest-based (`@sha256:...`) or no version specified
///
/// # Examples
///
/// ```
/// use deacon_core::outdated::compute_wanted_version;
///
/// // Tag with version
/// assert_eq!(compute_wanted_version("ghcr.io/devcontainers/features/node:18"), Some("18".to_string()));
///
/// // Tag with v prefix
/// assert_eq!(compute_wanted_version("ghcr.io/devcontainers/features/node:v1.2.3"), Some("1.2.3".to_string()));
///
/// // Digest reference
/// assert_eq!(compute_wanted_version("ghcr.io/devcontainers/features/node@sha256:abcd"), None);
///
/// // No version
/// assert_eq!(compute_wanted_version("ghcr.io/devcontainers/features/node"), None);
/// ```
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
///
/// # Examples
///
/// ```
/// use deacon_core::outdated::wanted_major;
///
/// assert_eq!(wanted_major(&Some("1.2.3".to_string())), Some("1".to_string()));
/// assert_eq!(wanted_major(&Some("2.0.0".to_string())), Some("2".to_string()));
/// assert_eq!(wanted_major(&None), None);
/// assert_eq!(wanted_major(&Some("invalid".to_string())), None);
/// ```
pub fn wanted_major(version: &Option<String>) -> Option<String> {
    version
        .as_ref()
        .and_then(|v| semver_utils::parse_version(v).map(|ver: Version| format!("{}", ver.major)))
}

/// Derive the "current" version for a declared feature given an optional lockfile.
///
/// Determines the currently installed version by checking the lockfile first,
/// then falling back to the wanted version from the configuration.
///
/// # Logic
///
/// 1. If lockfile contains an entry for the canonical feature ID, return its `version` field
/// 2. Otherwise, fall back to `compute_wanted_version(reference)`
///
/// # Examples
///
/// ```
/// use deacon_core::outdated::derive_current_version;
/// use deacon_core::lockfile::{Lockfile, LockfileFeature};
/// use std::collections::HashMap;
///
/// // Without lockfile, falls back to wanted version
/// let reference = "ghcr.io/devcontainers/features/node:18";
/// assert_eq!(derive_current_version(reference, None, &[]), Some("18".to_string()));
///
/// // With lockfile entry
/// let mut features = HashMap::new();
/// features.insert(
///     "ghcr.io/devcontainers/features/node".to_string(),
///     LockfileFeature {
///         version: "16.0.0".to_string(),
///         resolved: "ghcr.io/devcontainers/features/node@sha256:abc".to_string(),
///         integrity: "sha256:abc".to_string(),
///         depends_on: None,
///     }
/// );
/// let lockfile = Lockfile { features };
/// assert_eq!(derive_current_version(reference, Some(&lockfile), &[]), Some("16.0.0".to_string()));
/// ```
pub fn derive_current_version(
    reference: &str,
    lockfile_opt: Option<&lockfile::Lockfile>,
    stable_tags: &[String],
) -> Option<String> {
    let canonical = canonical_feature_id(reference);
    if let Some(ld) = lockfile_opt {
        if let Some(entry) = ld.features.get(&canonical) {
            return Some(entry.version.clone());
        }
    }

    // No lockfile entry: what is installed is whatever the declared reference resolves
    // to, which for a partial pin is the newest published version satisfying it — the
    // same resolution `wanted` performs (#407). Returning the pin's literal text here
    // reported `"current": "1"`, which is not a version.
    resolve_wanted_version(reference, stable_tags)
}

/// Fetch the latest stable semver tag for the given declared feature reference.
///
/// This performs a registry `list_tags` via the core `FeatureFetcher` and uses
/// `semver_utils` to filter and sort tags. Returns `None` on any error or if
/// no stable semver tags are present.
pub async fn fetch_stable_tags_descending(reference: &str) -> Option<Vec<String>> {
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
    let name = repo_parts.pop()?.to_string();
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
    Some(semver_tags)
}

/// The highest stable published version, i.e. the first of
/// [`fetch_stable_tags_descending`].
pub async fn fetch_latest_stable_version(reference: &str) -> Option<String> {
    fetch_stable_tags_descending(reference)
        .await?
        .into_iter()
        .next()
}

/// Resolve the version the declared reference ASKS FOR, given the published stable tags
/// in descending order.
///
/// `wanted` means "the newest published version satisfying what the configuration
/// declared" — which is what the neighbouring `latest` and `wantedMajor` fields imply,
/// and what the reference CLI reports. deacon used to return the declared tag STRING
/// verbatim, so a configuration pinning `git:1` reported `"wanted": "1"` — not a version
/// at all, and unusable for deciding whether an upgrade is available (#407 divergence 2).
///
/// An exactly-pinned reference resolves to itself and needs no tag list; only a partial
/// pin (`1`, `1.3`) has to consult the registry. Non-numeric tags (`latest`, a named
/// channel) are returned as declared: they name a moving target the registry alone
/// resolves, and guessing would be worse than echoing.
///
/// deacon's own `upgrade` already resolved ranges this way, so the two subcommands
/// disagreed with each other as well as with the reference — which is what settled the
/// direction.
pub fn resolve_wanted_version(reference: &str, stable_tags: &[String]) -> Option<String> {
    let declared = compute_wanted_version(reference)?;

    // A full `major.minor.patch` pin is already the answer.
    let parts: Vec<&str> = declared.split('.').collect();
    if parts.len() >= 3 || !parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
        return Some(declared);
    }
    if parts.iter().any(|p| p.is_empty()) {
        return Some(declared);
    }

    // A partial pin selects the highest published version sharing its prefix. Tags are
    // already sorted descending, so the first match wins.
    let prefix: Vec<&str> = parts;
    let matched = stable_tags.iter().find(|tag| {
        let tag_parts: Vec<&str> = tag.trim_start_matches('v').split('.').collect();
        tag_parts.len() >= prefix.len()
            && prefix
                .iter()
                .zip(tag_parts.iter())
                .all(|(want, have)| want == have)
    });

    // No published tag satisfies the pin (or the registry was unreachable): echo what was
    // declared rather than reporting nothing. Degrading to today's answer is better than
    // turning a transient lookup failure into a missing field.
    Some(matched.cloned().unwrap_or(declared))
}

/// Return the major portion of the latest stable version string.
///
/// # Examples
///
/// ```
/// use deacon_core::outdated::latest_major;
///
/// assert_eq!(latest_major(&Some("3.11.2".to_string())), Some("3".to_string()));
/// assert_eq!(latest_major(&Some("20.0.1".to_string())), Some("20".to_string()));
/// assert_eq!(latest_major(&None), None);
/// ```
pub fn latest_major(version: &Option<String>) -> Option<String> {
    version
        .as_ref()
        .and_then(|v| semver_utils::parse_version(v).map(|ver: Version| format!("{}", ver.major)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // T037: Unit tests for core helpers

    // is_oci_feature_ref tests
    #[test]
    fn test_is_oci_feature_ref_valid_full() {
        assert!(is_oci_feature_ref(
            "ghcr.io/devcontainers/features/node:1.2.3"
        ));
        assert!(is_oci_feature_ref("ghcr.io/devcontainers/features/node"));
        assert!(is_oci_feature_ref("registry.io/ns/feature@sha256:abcd"));
    }

    #[test]
    fn test_is_oci_feature_ref_valid_implicit_registry() {
        assert!(is_oci_feature_ref("devcontainers/features/node"));
        assert!(is_oci_feature_ref("myorg/myfeature:1.0.0"));
    }

    #[test]
    fn test_is_oci_feature_ref_invalid_local_paths() {
        // Relative paths
        assert!(!is_oci_feature_ref("./local-feature"));
        assert!(!is_oci_feature_ref("../relative/path"));
        assert!(!is_oci_feature_ref("./feature-a"));

        // Absolute paths
        assert!(!is_oci_feature_ref("/absolute/path"));
        assert!(!is_oci_feature_ref("/usr/local/feature"));
    }

    #[test]
    fn test_is_oci_feature_ref_invalid_http_urls() {
        assert!(!is_oci_feature_ref("https://example.com/feature.tgz"));
        assert!(!is_oci_feature_ref("http://example.com/feature.tar.gz"));
        assert!(!is_oci_feature_ref(
            "https://github.com/user/repo/releases/download/v1.0.0/feature.tgz"
        ));
    }

    #[test]
    fn test_is_oci_feature_ref_invalid_file_urls() {
        assert!(!is_oci_feature_ref("file:///path/to/feature"));
        assert!(!is_oci_feature_ref("file://./local/feature"));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_is_oci_feature_ref_invalid_windows_paths() {
        assert!(!is_oci_feature_ref("C:\\path\\to\\feature"));
        assert!(!is_oci_feature_ref("D:\\features\\myfeature"));
    }

    // canonical_feature_id tests
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
    fn test_canonical_feature_id_no_version() {
        let input = "ghcr.io/devcontainers/features/node";
        assert_eq!(canonical_feature_id(input), input);
    }

    #[test]
    fn test_canonical_feature_id_with_port() {
        // Registry with port should not be confused with tag separator
        let input = "localhost:5000/myfeature/test:1.0.0";
        assert_eq!(canonical_feature_id(input), "localhost:5000/myfeature/test");
    }

    #[test]
    fn test_canonical_feature_id_nested_namespace() {
        let input = "ghcr.io/org/team/project/feature:v2.1.0";
        assert_eq!(
            canonical_feature_id(input),
            "ghcr.io/org/team/project/feature"
        );
    }

    // compute_wanted_version tests
    #[test]
    fn test_compute_wanted_version_tag() {
        let input = "ghcr.io/devcontainers/features/node:18";
        assert_eq!(compute_wanted_version(input).unwrap(), "18");
    }

    #[test]
    fn test_compute_wanted_version_tag_with_v_prefix() {
        let input = "ghcr.io/devcontainers/features/node:v1.2.3";
        assert_eq!(compute_wanted_version(input).unwrap(), "1.2.3");
    }

    #[test]
    fn test_compute_wanted_version_digest() {
        let input = "ghcr.io/devcontainers/features/node@sha256:abcdef";
        assert!(compute_wanted_version(input).is_none());
    }

    #[test]
    fn test_compute_wanted_version_no_version() {
        let input = "ghcr.io/devcontainers/features/node";
        assert!(compute_wanted_version(input).is_none());
    }

    #[test]
    fn test_compute_wanted_version_semver() {
        let input = "ghcr.io/devcontainers/features/python:3.11.2";
        assert_eq!(compute_wanted_version(input).unwrap(), "3.11.2");
    }

    // wanted_major tests
    // -- resolve_wanted_version (#407 divergence 2) ---------------------------------

    fn tags(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// A partial pin resolves to the newest published version satisfying it — NOT the
    /// pin's literal text, which is what deacon used to report.
    #[test]
    fn wanted_resolves_a_major_only_pin_to_the_newest_matching_version() {
        let published = tags(&["2.1.0", "1.7.1", "1.6.0", "1.3.2"]);
        assert_eq!(
            resolve_wanted_version("ghcr.io/x/y/node:1", &published),
            Some("1.7.1".to_string())
        );
    }

    #[test]
    fn wanted_resolves_a_minor_pin_within_that_minor() {
        let published = tags(&["1.7.1", "1.3.8", "1.3.2"]);
        assert_eq!(
            resolve_wanted_version("ghcr.io/x/y/git:1.3", &published),
            Some("1.3.8".to_string())
        );
    }

    /// An exact pin is already the answer and must not drift to a newer patch.
    #[test]
    fn wanted_leaves_an_exact_pin_alone() {
        let published = tags(&["1.3.8", "1.3.2"]);
        assert_eq!(
            resolve_wanted_version("ghcr.io/x/y/git:1.3.2", &published),
            Some("1.3.2".to_string())
        );
    }

    /// `1` must not match `10.x`: the comparison is per component, not textual prefix.
    #[test]
    fn wanted_does_not_match_a_longer_major() {
        let published = tags(&["10.0.0", "1.2.0"]);
        assert_eq!(
            resolve_wanted_version("ghcr.io/x/y/z:1", &published),
            Some("1.2.0".to_string())
        );
    }

    /// An unreachable registry (or a pin nothing published satisfies) degrades to the
    /// declared text rather than reporting nothing — a transient lookup failure must not
    /// turn into a missing field.
    #[test]
    fn wanted_falls_back_to_the_declared_pin_when_nothing_matches() {
        assert_eq!(
            resolve_wanted_version("ghcr.io/x/y/z:9", &tags(&["1.0.0"])),
            Some("9".to_string())
        );
        assert_eq!(
            resolve_wanted_version("ghcr.io/x/y/z:1", &[]),
            Some("1".to_string())
        );
    }

    /// A non-numeric tag names a moving target only the registry resolves; echoing it is
    /// better than guessing which published version it points at.
    #[test]
    fn wanted_echoes_a_non_numeric_tag() {
        assert_eq!(
            resolve_wanted_version("ghcr.io/x/y/z:latest", &tags(&["2.0.0"])),
            Some("latest".to_string())
        );
    }

    /// A digest reference carries no version to want.
    #[test]
    fn wanted_is_none_for_a_digest_reference() {
        assert_eq!(
            resolve_wanted_version("ghcr.io/x/y/z@sha256:abc", &tags(&["2.0.0"])),
            None
        );
    }

    /// Without a lockfile, `current` resolves the same way `wanted` does — reporting the
    /// pin's literal text said `"current": "1"`, which is not a version.
    #[test]
    fn current_without_a_lockfile_resolves_the_pin() {
        assert_eq!(
            derive_current_version("ghcr.io/x/y/node:1", None, &tags(&["1.7.1", "1.6.0"])),
            Some("1.7.1".to_string())
        );
    }

    #[test]
    fn test_wanted_major_valid() {
        assert_eq!(
            wanted_major(&Some("1.2.3".to_string())),
            Some("1".to_string())
        );
        assert_eq!(
            wanted_major(&Some("18.0.0".to_string())),
            Some("18".to_string())
        );
    }

    #[test]
    fn test_wanted_major_none() {
        assert_eq!(wanted_major(&None), None);
    }

    #[test]
    fn test_wanted_major_invalid() {
        assert_eq!(wanted_major(&Some("invalid".to_string())), None);
        assert_eq!(wanted_major(&Some("".to_string())), None);
    }

    // latest_major tests
    #[test]
    fn test_latest_major_valid() {
        assert_eq!(
            latest_major(&Some("20.1.0".to_string())),
            Some("20".to_string())
        );
        assert_eq!(
            latest_major(&Some("3.11.5".to_string())),
            Some("3".to_string())
        );
    }

    #[test]
    fn test_latest_major_none() {
        assert_eq!(latest_major(&None), None);
    }

    #[test]
    fn test_latest_major_invalid() {
        assert_eq!(latest_major(&Some("not-semver".to_string())), None);
    }

    // derive_current_version tests
    #[test]
    fn test_derive_current_version_no_lockfile() {
        let reference = "ghcr.io/devcontainers/features/node:18";
        assert_eq!(
            derive_current_version(reference, None, &[]),
            Some("18".to_string())
        );
    }

    #[test]
    fn test_derive_current_version_with_lockfile_entry() {
        let reference = "ghcr.io/devcontainers/features/node:20";
        let mut features = HashMap::new();
        features.insert(
            "ghcr.io/devcontainers/features/node".to_string(),
            lockfile::LockfileFeature {
                version: "18.5.0".to_string(),
                resolved: "ghcr.io/devcontainers/features/node@sha256:abc123".to_string(),
                integrity: "sha256:abc123".to_string(),
                depends_on: None,
            },
        );
        let lockfile = lockfile::Lockfile { features };

        // Should return lockfile version, not wanted version
        assert_eq!(
            derive_current_version(reference, Some(&lockfile), &[]),
            Some("18.5.0".to_string())
        );
    }

    #[test]
    fn test_derive_current_version_lockfile_no_match() {
        let reference = "ghcr.io/devcontainers/features/python:3.11";
        let mut features = HashMap::new();
        features.insert(
            "ghcr.io/devcontainers/features/node".to_string(),
            lockfile::LockfileFeature {
                version: "18.0.0".to_string(),
                resolved: "ghcr.io/devcontainers/features/node@sha256:def456".to_string(),
                integrity: "sha256:def456".to_string(),
                depends_on: None,
            },
        );
        let lockfile = lockfile::Lockfile { features };

        // No match in lockfile, should fall back to wanted
        assert_eq!(
            derive_current_version(reference, Some(&lockfile), &[]),
            Some("3.11".to_string())
        );
    }

    #[test]
    fn test_derive_current_version_digest_no_lockfile() {
        let reference = "ghcr.io/devcontainers/features/node@sha256:abcdef";
        // Digest with no lockfile should return None (can't derive version)
        assert_eq!(derive_current_version(reference, None, &[]), None);
    }

    // FeatureVersionInfo serialization tests
    #[test]
    fn test_feature_version_info_serialization() {
        let info = FeatureVersionInfo {
            id: "ghcr.io/devcontainers/features/node".to_string(),
            current: Some("18.0.0".to_string()),
            wanted: Some("18".to_string()),
            latest: Some("20.1.0".to_string()),
            wanted_major: Some("18".to_string()),
            latest_major: Some("20".to_string()),
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: FeatureVersionInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, info.id);
        assert_eq!(deserialized.current, info.current);
        assert_eq!(deserialized.wanted, info.wanted);
        assert_eq!(deserialized.latest, info.latest);
    }

    #[test]
    fn test_feature_version_info_with_nulls() {
        let info = FeatureVersionInfo {
            id: "ghcr.io/devcontainers/features/node".to_string(),
            current: None,
            wanted: None,
            latest: None,
            wanted_major: None,
            latest_major: None,
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: FeatureVersionInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, info.id);
        assert_eq!(deserialized.current, None);
        assert_eq!(deserialized.wanted, None);
        assert_eq!(deserialized.latest, None);
    }
}
