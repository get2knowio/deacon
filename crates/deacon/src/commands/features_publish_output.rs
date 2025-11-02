//! Publish output models for the features publish command
//!
//! Defines the JSON output structure for `deacon features publish` following
//! the contract in `specs/003-features-publish-compliance/contracts/publish-output.schema.json`.
//!
//! The output conforms to the JSON schema defined in the contracts directory,
//! ensuring compatibility with downstream tools and automation.
//!
//! # Schema Compliance
//!
//! This module implements the JSON schema at:
//! `specs/003-features-publish-compliance/contracts/publish-output.schema.json`
//!
//! Key schema constraints:
//! - `featureId`: Non-empty string identifying the feature (e.g., "owner/repo/featureId")
//! - `version`: Semantic version string matching pattern `^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$`
//! - `digest`: SHA256 digest string matching pattern `^sha256:[a-f0-9]{64}$`
//! - `publishedTags`/`skippedTags`: Arrays of unique non-empty strings
//! - `movedLatest`: Boolean indicating if latest tag was updated
//! - `registry`/`namespace`: Non-empty strings identifying publish location
//! - `collection.digest`: Optional SHA256 digest for collection metadata
//! - `summary`: Statistics with non-negative integer counts
//!
//! # Stability Guarantee
//!
//! The output structure and field names are stable and must not change without
//! updating the JSON schema and coordinating with downstream consumers.

use serde::{Deserialize, Serialize};

/// Result for a single feature publish operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct PublishFeatureResult {
    /// Unique identifier for the feature (e.g., "owner/repo/featureId")
    pub feature_id: String,
    /// Semantic version of the feature
    pub version: String,
    /// SHA256 digest of the published manifest
    pub digest: String,
    /// Tags that were successfully published
    pub published_tags: Vec<String>,
    /// Tags that were skipped (already existed)
    pub skipped_tags: Vec<String>,
    /// Whether the `latest` tag was moved/created
    pub moved_latest: bool,
    /// Registry where the feature was published
    pub registry: String,
    /// Namespace within the registry
    pub namespace: String,
}

/// Result for collection metadata publish (optional)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct PublishCollectionResult {
    /// SHA256 digest of the published collection manifest
    pub digest: String,
}

/// Summary statistics across all published features
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct PublishSummary {
    /// Number of features processed
    pub features: usize,
    /// Total number of tags published across all features
    pub published_tags: usize,
    /// Total number of tags skipped across all features
    pub skipped_tags: usize,
}

/// Top-level output structure for features publish command
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct PublishOutput {
    /// Results for each feature that was published
    pub features: Vec<PublishFeatureResult>,
    /// Collection metadata result (present if collection was published)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection: Option<PublishCollectionResult>,
    /// Summary statistics
    pub summary: PublishSummary,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that PublishOutput JSON serialization includes correct skippedTags structure for all-skipped case
    #[test]
    fn test_publish_output_json_includes_skipped_tags() {
        // Create a PublishOutput that simulates all tags being skipped
        let output = PublishOutput {
            features: vec![PublishFeatureResult {
                feature_id: "test-feature".to_string(),
                version: "1.0.0".to_string(),
                digest: "sha256:abc123".to_string(),
                published_tags: vec![],
                skipped_tags: vec![
                    "1".to_string(),
                    "1.0".to_string(),
                    "1.0.0".to_string(),
                    "latest".to_string(),
                ],
                moved_latest: false,
                registry: "ghcr.io".to_string(),
                namespace: "testuser".to_string(),
            }],
            collection: None,
            summary: PublishSummary {
                features: 1,
                published_tags: 0,
                skipped_tags: 4,
            },
        };

        // Test JSON serialization structure
        let json_output = serde_json::to_string_pretty(&output).unwrap();

        // Parse the JSON and verify structure
        let parsed: serde_json::Value = serde_json::from_str(&json_output).unwrap();

        // Check features array
        assert!(
            parsed.get("features").is_some(),
            "Should have features array"
        );
        let features = parsed.get("features").unwrap().as_array().unwrap();
        assert_eq!(features.len(), 1, "Should have 1 feature");

        let feature = &features[0];
        assert_eq!(feature.get("featureId").unwrap(), "test-feature");
        assert_eq!(feature.get("version").unwrap(), "1.0.0");
        assert_eq!(feature.get("digest").unwrap(), "sha256:abc123");

        // Check publishedTags is empty array
        let published_tags = feature.get("publishedTags").unwrap().as_array().unwrap();
        assert!(published_tags.is_empty(), "publishedTags should be empty");

        // Check skippedTags has the expected tags
        let skipped_tags = feature.get("skippedTags").unwrap().as_array().unwrap();
        assert_eq!(skipped_tags.len(), 4, "Should have 4 skipped tags");
        assert!(skipped_tags.contains(&serde_json::Value::String("1".to_string())));
        assert!(skipped_tags.contains(&serde_json::Value::String("1.0".to_string())));
        assert!(skipped_tags.contains(&serde_json::Value::String("1.0.0".to_string())));
        assert!(skipped_tags.contains(&serde_json::Value::String("latest".to_string())));

        assert_eq!(feature.get("movedLatest").unwrap(), false);

        // Check summary
        let summary = parsed.get("summary").unwrap();
        assert_eq!(summary.get("features").unwrap(), 1);
        assert_eq!(summary.get("publishedTags").unwrap(), 0);
        assert_eq!(summary.get("skippedTags").unwrap(), 4);
    }
}
