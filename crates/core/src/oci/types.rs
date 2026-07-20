//! Core OCI types for DevContainer features and templates

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::features::FeatureMetadata;

/// Join a repository path with a version into a reference string that
/// [`crate::registry_parser::parse_registry_reference`] parses back to the SAME parts.
///
/// A digest version (`sha256:<hex>`) MUST be joined with `@`, not `:`. Joining a digest
/// with `:` yields `…/git:sha256:<hex>`, which re-parses on the last colon into name
/// `git:sha256` + tag `<hex>` and requests
/// `/v2/…/git:sha256/manifests/<hex>` → 404. Reference strings round-trip through
/// parse/render in several flows (notably `read-configuration`'s
/// `--include-features-configuration`), so the join has to be lossless.
fn join_reference(registry: &str, repository: &str, version: &str) -> String {
    if is_digest(version) {
        format!("{registry}/{repository}@{version}")
    } else {
        format!("{registry}/{repository}:{version}")
    }
}

/// Whether a version string is a content digest (`<algorithm>:<hex>`) rather than a tag.
///
/// An OCI tag may not contain `:`, so any colon marks a digest. Kept deliberately broad
/// (not `sha256`-only) so future algorithms are handled without another round-trip bug.
fn is_digest(version: &str) -> bool {
    version.contains(':')
}

/// Reference to a feature in an OCI registry
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureRef {
    /// Registry hostname (e.g., "ghcr.io")
    pub registry: String,
    /// Namespace (e.g., "devcontainers")
    pub namespace: String,
    /// Feature name (e.g., "node")
    pub name: String,
    /// Version (optional, defaults to "latest")
    pub version: Option<String>,
}

impl FeatureRef {
    /// Create a new FeatureRef
    pub fn new(registry: String, namespace: String, name: String, version: Option<String>) -> Self {
        Self {
            registry,
            namespace,
            name,
            version,
        }
    }

    /// Get the tag for this feature reference
    pub fn tag(&self) -> &str {
        self.version.as_deref().unwrap_or("latest")
    }

    /// Get the repository name for this feature
    pub fn repository(&self) -> String {
        format!("{}/{}", self.namespace, self.name)
    }

    /// Get the full reference string.
    ///
    /// Round-trips through [`crate::registry_parser::parse_registry_reference`]: a
    /// digest-pinned version is joined with `@`, a tag with `:` (see
    /// [`join_reference`]).
    pub fn reference(&self) -> String {
        join_reference(&self.registry, &self.repository(), self.tag())
    }
}

/// Reference to a template in an OCI registry
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateRef {
    /// Registry hostname (e.g., "ghcr.io")
    pub registry: String,
    /// Namespace (e.g., "devcontainers")
    pub namespace: String,
    /// Template name (e.g., "python")
    pub name: String,
    /// Version (optional, defaults to "latest")
    pub version: Option<String>,
}

impl TemplateRef {
    /// Create a new TemplateRef
    pub fn new(registry: String, namespace: String, name: String, version: Option<String>) -> Self {
        Self {
            registry,
            namespace,
            name,
            version,
        }
    }

    /// Get the tag for this template reference
    pub fn tag(&self) -> &str {
        self.version.as_deref().unwrap_or("latest")
    }

    /// Get the repository name for this template
    pub fn repository(&self) -> String {
        format!("{}/{}", self.namespace, self.name)
    }

    /// Get the full reference string.
    ///
    /// Same digest-safe join as [`FeatureRef::reference`] — templates are parsed by the
    /// same `parse_registry_reference`, so they carry the identical round-trip hazard.
    pub fn reference(&self) -> String {
        join_reference(&self.registry, &self.repository(), self.tag())
    }
}

/// Downloaded and extracted feature data
#[derive(Debug, Clone)]
pub struct DownloadedFeature {
    /// Extracted feature directory
    pub path: PathBuf,
    /// Feature metadata
    pub metadata: FeatureMetadata,
    /// Layer (blob) digest, used as the on-disk cache key and for blob
    /// integrity verification. This is NOT the digest the reference CLI
    /// expects in lockfile `resolved`/`integrity` fields — use
    /// `manifest_digest` for that.
    pub digest: String,
    /// `sha256:`-prefixed digest of the OCI manifest body. This is the value
    /// the reference `@devcontainers/cli` records in lockfile `resolved`
    /// (`{registry}/{repository}@{manifest_digest}`) and `integrity` fields.
    pub manifest_digest: String,
}

/// Downloaded and extracted template data
#[derive(Debug)]
pub struct DownloadedTemplate {
    /// Extracted template directory
    pub path: PathBuf,
    /// Template metadata
    pub metadata: crate::templates::TemplateMetadata,
    /// Template digest for caching
    pub digest: String,
}

/// OCI manifest structure (minimal)
#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
pub struct Manifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub layers: Vec<Layer>,
}

/// OCI layer structure (minimal)
#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
pub struct Layer {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub size: u64,
    pub digest: String,
}

/// OCI tag list response structure
#[derive(Debug, Deserialize, Serialize)]
pub struct TagList {
    /// Repository name
    pub name: String,
    /// List of tags
    pub tags: Vec<String>,
}

/// DevContainer collection metadata structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionMetadata {
    /// Source information (e.g., GitHub repository)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_information: Option<CollectionSourceInfo>,
    /// List of features in the collection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<Vec<CollectionFeature>>,
    /// List of templates in the collection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub templates: Option<Vec<CollectionTemplate>>,
}

/// Source information for a collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSourceInfo {
    /// Source provider (e.g., "github")
    pub provider: String,
    /// Repository or source identifier
    pub repository: String,
}

/// Feature entry in a collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionFeature {
    /// Feature identifier
    pub id: String,
    /// Feature version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Feature name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Feature description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Template entry in a collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionTemplate {
    /// Template identifier
    pub id: String,
    /// Template version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Template name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Template description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// HTTP response with status, headers, and body
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Bytes,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry_parser::parse_registry_reference;

    /// A digest-pinned reference must survive render → parse → render unchanged.
    ///
    /// Regression guard for the `name:tag@digest` round trip (originally fixed in #131,
    /// re-broken via `reference()` joining with `:`): `read-configuration
    /// --include-features-configuration` renders a resolved feature's source with
    /// `reference()` and then parses it back with `parse_registry_reference`. Joining a
    /// digest with `:` made that round trip lossy, producing repository
    /// `devcontainers/features/git:sha256` and a 404 on the manifest fetch.
    #[test]
    fn digest_pinned_reference_round_trips() {
        const DIGEST: &str =
            "sha256:fd75977de13a9979000e0e78baf949adb0ca71d2398995fa22e0a36d7e7e7fe2";

        let original = FeatureRef::new(
            "ghcr.io".to_string(),
            "devcontainers/features".to_string(),
            "git".to_string(),
            Some(DIGEST.to_string()),
        );

        // A digest joins with '@', never ':'.
        let rendered = original.reference();
        assert_eq!(
            rendered,
            format!("ghcr.io/devcontainers/features/git@{DIGEST}"),
            "a digest version must be joined with '@'"
        );

        // …and parses back to the SAME parts.
        let (registry, namespace, name, version) = parse_registry_reference(&rendered).unwrap();
        assert_eq!(registry, "ghcr.io");
        assert_eq!(namespace, "devcontainers/features");
        assert_eq!(name, "git", "the digest must not bleed into the name");
        assert_eq!(version.as_deref(), Some(DIGEST));

        // The re-parsed ref renders identically — the round trip is a fixed point.
        let reparsed = FeatureRef::new(registry, namespace, name, version);
        assert_eq!(reparsed.reference(), rendered);
        assert_eq!(
            reparsed.repository(),
            "devcontainers/features/git",
            "repository must not absorb the digest algorithm"
        );
    }

    /// The pre-existing tag behavior is unchanged: tags still join with ':'.
    #[test]
    fn tagged_reference_round_trips_unchanged() {
        let r = FeatureRef::new(
            "ghcr.io".to_string(),
            "devcontainers/features".to_string(),
            "git".to_string(),
            Some("1".to_string()),
        );
        assert_eq!(r.reference(), "ghcr.io/devcontainers/features/git:1");

        let (registry, namespace, name, version) =
            parse_registry_reference(&r.reference()).unwrap();
        assert_eq!((name.as_str(), version.as_deref()), ("git", Some("1")));
        assert_eq!(
            FeatureRef::new(registry, namespace, name, version).reference(),
            r.reference()
        );
    }

    /// An absent version still defaults to the `latest` TAG (colon join).
    #[test]
    fn missing_version_defaults_to_latest_tag() {
        let r = FeatureRef::new(
            "ghcr.io".to_string(),
            "devcontainers".to_string(),
            "node".to_string(),
            None,
        );
        assert_eq!(r.reference(), "ghcr.io/devcontainers/node:latest");
    }

    /// `TemplateRef` carries the identical hazard and the identical fix.
    #[test]
    fn template_digest_reference_round_trips() {
        const DIGEST: &str = "sha256:abc123";
        let t = TemplateRef::new(
            "ghcr.io".to_string(),
            "devcontainers/templates".to_string(),
            "python".to_string(),
            Some(DIGEST.to_string()),
        );
        assert_eq!(
            t.reference(),
            format!("ghcr.io/devcontainers/templates/python@{DIGEST}")
        );
        let (_, _, name, version) = parse_registry_reference(&t.reference()).unwrap();
        assert_eq!(name, "python");
        assert_eq!(version.as_deref(), Some(DIGEST));
    }
}
