//! Core OCI types for DevContainer features and templates

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::features::FeatureMetadata;

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

    /// Get the full reference string
    pub fn reference(&self) -> String {
        format!("{}/{}:{}", self.registry, self.repository(), self.tag())
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

    /// Get the full reference string
    pub fn reference(&self) -> String {
        format!("{}/{}:{}", self.registry, self.repository(), self.tag())
    }
}

/// Downloaded and extracted feature data
#[derive(Debug, Clone)]
pub struct DownloadedFeature {
    /// Extracted feature directory
    pub path: PathBuf,
    /// Feature metadata
    pub metadata: FeatureMetadata,
    /// Feature digest for caching
    pub digest: String,
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

/// Result of publishing an artifact to an OCI registry
#[derive(Debug, Clone)]
pub struct PublishResult {
    /// Registry URL where the artifact was published
    pub registry: String,
    /// Repository name
    pub repository: String,
    /// Tag used for publishing
    pub tag: String,
    /// Digest of the published manifest
    pub digest: String,
    /// Size of the published artifact in bytes
    pub size: u64,
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
