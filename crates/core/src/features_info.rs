//! Data contracts for Features Info subcommand output
//!
//! This module defines the JSON serialization structures used by the
//! `deacon features info` subcommand for both text and JSON output formats.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// JSON output structure for verbose features info
/// Contains manifest, canonicalId, and publishedTags fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerboseJson {
    /// The OCI manifest JSON
    pub manifest: serde_json::Value,
    /// The canonical identifier (digest) or null for local features
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
    /// List of published tags
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub published_tags: Vec<String>,
    /// Error information for partial failures
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub errors: std::collections::HashMap<String, String>,
}

/// JSON output structure for published tags info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedTagsJson {
    /// List of published tags
    pub published_tags: Vec<String>,
}

/// JSON output structure for manifest and canonical ID info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestJson {
    /// The OCI manifest JSON
    pub manifest: serde_json::Value,
    /// The canonical identifier (digest) or null for local features
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_verbose_json_serialization() {
        let manifest = json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "layers": []
        });

        let verbose = VerboseJson {
            manifest: manifest.clone(),
            canonical_id: Some("sha256:abc123".to_string()),
            published_tags: vec!["v1.0".to_string(), "latest".to_string()],
            errors: std::collections::HashMap::new(),
        };

        let json_str = serde_json::to_string(&verbose).unwrap();
        let deserialized: VerboseJson = serde_json::from_str(&json_str).unwrap();

        assert_eq!(deserialized.canonical_id, Some("sha256:abc123".to_string()));
        assert_eq!(deserialized.published_tags.len(), 2);
        assert!(deserialized.errors.is_empty());
    }

    #[test]
    fn test_published_tags_json_serialization() {
        let tags = PublishedTagsJson {
            published_tags: vec!["v1.0".to_string(), "v1.1".to_string()],
        };

        let json_str = serde_json::to_string(&tags).unwrap();
        let deserialized: PublishedTagsJson = serde_json::from_str(&json_str).unwrap();

        assert_eq!(
            deserialized.published_tags,
            vec!["v1.0".to_string(), "v1.1".to_string()]
        );
    }

    #[test]
    fn test_manifest_json_serialization() {
        let manifest = json!({
            "schemaVersion": 2,
            "layers": []
        });

        let manifest_json = ManifestJson {
            manifest: manifest.clone(),
            canonical_id: Some("sha256:def456".to_string()),
        };

        let json_str = serde_json::to_string(&manifest_json).unwrap();
        let deserialized: ManifestJson = serde_json::from_str(&json_str).unwrap();

        assert_eq!(deserialized.canonical_id, Some("sha256:def456".to_string()));
        assert_eq!(deserialized.manifest["schemaVersion"], 2);
    }

    #[test]
    fn test_optional_fields_omitted() {
        let verbose = VerboseJson {
            manifest: json!({"test": true}),
            canonical_id: None,
            published_tags: vec![],
            errors: std::collections::HashMap::new(),
        };

        let json_value = serde_json::to_value(&verbose).unwrap();
        assert!(!json_value.as_object().unwrap().contains_key("canonical_id"));
        assert!(!json_value
            .as_object()
            .unwrap()
            .contains_key("published_tags"));
        assert!(!json_value.as_object().unwrap().contains_key("errors"));
    }
}
