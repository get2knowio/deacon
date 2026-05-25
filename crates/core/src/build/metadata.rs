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
    /// Per the devcontainer spec (devcontainers/cli#1199, v0.86.0), the
    /// `devcontainer.metadata` label is always a JSON array, even for a single
    /// metadata entry. Readers iterate the array and merge entries in order.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String> {
        // Always wrap in an array per the upstream spec, even with a single entry.
        let array = [self];
        serde_json::to_string(&array).map_err(|e| {
            crate::errors::DeaconError::Internal(crate::errors::InternalError::Generic {
                message: format!("Failed to serialize devcontainer metadata: {}", e),
            })
        })
    }

    /// Deserializes metadata from a JSON string (for reading from images).
    ///
    /// Accepts both the spec array form `[{...}]` and the legacy single-object
    /// form `{...}` emitted by older Deacon builds. Always returns a vector;
    /// callers merge entries in order.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn from_json(json: &str) -> Result<Vec<Self>> {
        let value: serde_json::Value = serde_json::from_str(json).map_err(|e| {
            crate::errors::DeaconError::Internal(crate::errors::InternalError::Generic {
                message: format!("Failed to parse devcontainer metadata JSON: {}", e),
            })
        })?;
        let result = match value {
            serde_json::Value::Array(_) => serde_json::from_value::<Vec<Self>>(value),
            _ => serde_json::from_value::<Self>(value).map(|v| vec![v]),
        };
        result.map_err(|e| {
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

    fn sample_metadata() -> DevcontainerMetadata {
        DevcontainerMetadata {
            config: serde_json::json!({"name": "test"}),
            features: vec![],
            customizations: None,
            lockfile_hash: None,
        }
    }

    #[test]
    fn test_metadata_roundtrip() {
        let metadata = sample_metadata();

        let json = metadata.to_json().unwrap();
        let parsed = DevcontainerMetadata::from_json(&json).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0], metadata);
    }

    #[test]
    fn test_to_json_emits_array() {
        let metadata = sample_metadata();
        let json = metadata.to_json().unwrap();

        // Per spec (devcontainers/cli#1199), the label value is always a JSON array
        // even for a single entry.
        assert!(json.starts_with('['), "expected array form, got: {}", json);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_from_json_handles_legacy_object_form() {
        // Older Deacon builds emitted a single JSON object, not an array.
        // The reader must remain tolerant for backwards compatibility.
        let legacy = serde_json::json!({
            "config": {"name": "legacy"},
            "features": [],
            "customizations": null,
            "lockfile_hash": null,
        })
        .to_string();

        let parsed = DevcontainerMetadata::from_json(&legacy).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].config, serde_json::json!({"name": "legacy"}));
    }

    #[test]
    fn test_from_json_handles_array_form_with_multiple_entries() {
        let array = serde_json::json!([
            {"config": {"name": "first"},  "features": [], "customizations": null, "lockfile_hash": null},
            {"config": {"name": "second"}, "features": [], "customizations": null, "lockfile_hash": null},
        ])
        .to_string();

        let parsed = DevcontainerMetadata::from_json(&array).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].config, serde_json::json!({"name": "first"}));
        assert_eq!(parsed[1].config, serde_json::json!({"name": "second"}));
    }

    #[test]
    fn test_merge_labels_emits_array_label_value() {
        let user_labels = vec![("custom.label".to_string(), "value".to_string())];
        let metadata = sample_metadata();

        let labels = merge_labels(&user_labels, &metadata).unwrap();

        assert!(labels.contains_key("custom.label"));
        let label_value = labels
            .get("devcontainer.metadata")
            .expect("devcontainer.metadata label must be present");
        assert!(
            label_value.starts_with('['),
            "label value must be a JSON array per spec, got: {}",
            label_value
        );
    }
}
