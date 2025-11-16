//! Build metadata serialization and label management
//!
//! This module handles serialization of devcontainer metadata and feature
//! customizations into image labels for downstream tooling.

use crate::errors::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Devcontainer metadata stored in image labels.
///
/// This struct represents the canonical metadata format that is serialized
/// into the `devcontainer.metadata` label on built images.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DevcontainerMetadata {
    /// Original configuration (subset of fields)
    pub config: serde_json::Value,

    /// Applied features with their configurations
    pub features: Vec<FeatureMetadata>,

    /// Persisted customizations (if not skipped)
    pub customizations: Option<HashMap<String, serde_json::Value>>,

    /// Lockfile hash (if lockfile enabled)
    pub lockfile_hash: Option<String>,
}

/// Feature metadata in the image label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureMetadata {
    /// Feature identifier
    pub id: String,

    /// Feature version
    pub version: Option<String>,

    /// Feature options
    pub options: HashMap<String, serde_json::Value>,
}

impl DevcontainerMetadata {
    /// Serializes metadata to a JSON string for inclusion in image labels.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| {
            crate::errors::DeaconError::Internal(crate::errors::InternalError::Generic {
                message: format!("Failed to serialize devcontainer metadata: {}", e),
            })
        })
    }

    /// Deserializes metadata from a JSON string (for reading from images).
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| {
            crate::errors::DeaconError::Internal(crate::errors::InternalError::Generic {
                message: format!("Failed to deserialize devcontainer metadata: {}", e),
            })
        })
    }
}

/// Merges user labels with system-generated metadata labels.
///
/// # Arguments
///
/// * `user_labels` - User-provided labels from `--label`
/// * `metadata` - Devcontainer metadata to include
///
/// # Returns
///
/// A map of all labels to apply to the image.
pub fn merge_labels(
    user_labels: &[(String, String)],
    metadata: &DevcontainerMetadata,
) -> Result<HashMap<String, String>> {
    let mut labels = HashMap::new();

    // Add user labels
    for (key, value) in user_labels {
        labels.insert(key.clone(), value.clone());
    }

    // Add metadata label
    labels.insert("devcontainer.metadata".to_string(), metadata.to_json()?);

    Ok(labels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_roundtrip() {
        let metadata = DevcontainerMetadata {
            config: serde_json::json!({"name": "test"}),
            features: vec![],
            customizations: None,
            lockfile_hash: None,
        };

        let json = metadata.to_json().unwrap();
        let parsed = DevcontainerMetadata::from_json(&json).unwrap();

        assert_eq!(metadata, parsed);
    }

    #[test]
    fn test_merge_labels() {
        let user_labels = vec![("custom.label".to_string(), "value".to_string())];

        let metadata = DevcontainerMetadata {
            config: serde_json::json!({"name": "test"}),
            features: vec![],
            customizations: None,
            lockfile_hash: None,
        };

        let labels = merge_labels(&user_labels, &metadata).unwrap();

        assert!(labels.contains_key("custom.label"));
        assert!(labels.contains_key("devcontainer.metadata"));
    }
}
