//! Publish output models for the features publish command
//!
//! Defines the JSON output structure for `deacon features publish` following
//! the contract in `specs/003-features-publish-compliance/contracts/publish-output.schema.json`.
//!
//! The output conforms to the JSON schema defined in the contracts directory,
//! ensuring compatibility with downstream tools and automation.

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
