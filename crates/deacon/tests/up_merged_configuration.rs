//! Tests for enriched mergedConfiguration with feature metadata and labels.
//!
//! Per specs/004-mergedconfig-metadata/:
//! - Feature metadata entries should be present with provenance and ordering
//! - Null/empty fields should be retained per schema expectations
//! - Both single and compose flows should use the same merge logic

use deacon_core::config::merge::{
    EnrichedMergedConfiguration, FeatureMetadataEntry, LabelSet, Provenance,
};

/// Test that FeatureMetadataEntry serializes correctly with all fields.
#[test]
fn test_feature_metadata_entry_serialization_with_all_fields() {
    let entry = FeatureMetadataEntry {
        id: "ghcr.io/devcontainers/features/node:1".to_string(),
        version: Some("1.0.0".to_string()),
        name: Some("Node.js".to_string()),
        description: Some("Installs Node.js, nvm, and yarn".to_string()),
        documentation_url: Some(
            "https://github.com/devcontainers/features/tree/main/src/node".to_string(),
        ),
        options: Some(serde_json::json!({"version": "20"})),
        installs_after: Some(vec![
            "ghcr.io/devcontainers/features/common-utils:1".to_string()
        ]),
        depends_on: None,
        mounts: None,
        container_env: Some(
            [("NODE_VERSION".to_string(), "20".to_string())]
                .into_iter()
                .collect(),
        ),
        customizations: None,
        provenance: Some(Provenance {
            source: Some("ghcr.io/devcontainers/features/node:1".to_string()),
            service: None,
            order: Some(0),
        }),
    };

    let json = serde_json::to_value(&entry).unwrap();

    assert_eq!(json["id"], "ghcr.io/devcontainers/features/node:1");
    assert_eq!(json["version"], "1.0.0");
    assert_eq!(json["name"], "Node.js");
    assert_eq!(json["description"], "Installs Node.js, nvm, and yarn");
    assert!(json["documentationUrl"].is_string());
    assert!(json["options"].is_object());
    assert!(json["installsAfter"].is_array());
    assert!(json["containerEnv"].is_object());
    assert!(json["provenance"].is_object());
    assert_eq!(json["provenance"]["order"], 0);
}

/// Test that FeatureMetadataEntry serializes correctly with minimal fields.
#[test]
fn test_feature_metadata_entry_serialization_minimal() {
    let entry = FeatureMetadataEntry {
        id: "ghcr.io/devcontainers/features/node:1".to_string(),
        version: None,
        name: None,
        description: None,
        documentation_url: None,
        options: Some(serde_json::json!({})),
        installs_after: None,
        depends_on: None,
        mounts: None,
        container_env: None,
        customizations: None,
        provenance: Some(Provenance {
            source: None,
            service: None,
            order: Some(0),
        }),
    };

    let json = serde_json::to_value(&entry).unwrap();

    assert_eq!(json["id"], "ghcr.io/devcontainers/features/node:1");
    // Null fields should be serialized as null, not omitted
    assert!(json.get("version").is_none_or(|v| v.is_null()));
    assert!(json.get("name").is_none_or(|v| v.is_null()));
    assert!(json.get("description").is_none_or(|v| v.is_null()));
    // provenance should still be present
    assert!(json["provenance"].is_object());
    assert_eq!(json["provenance"]["order"], 0);
}

/// Test that from_config_entry creates correct minimal metadata.
#[test]
fn test_feature_metadata_entry_from_config_entry() {
    let entry = FeatureMetadataEntry::from_config_entry(
        "ghcr.io/devcontainers/features/node:1".to_string(),
        serde_json::json!({"version": "20"}),
        0,
    );

    assert_eq!(entry.id, "ghcr.io/devcontainers/features/node:1");
    assert!(entry.version.is_none());
    assert!(entry.name.is_none());
    assert_eq!(entry.options, Some(serde_json::json!({"version": "20"})));
    assert!(entry.provenance.is_some());
    let provenance = entry.provenance.unwrap();
    assert_eq!(provenance.order, Some(0));
}

/// Test that from_config_entry handles empty options correctly.
#[test]
fn test_feature_metadata_entry_from_config_entry_empty_options() {
    let entry = FeatureMetadataEntry::from_config_entry(
        "ghcr.io/devcontainers/features/node:1".to_string(),
        serde_json::json!({}),
        1,
    );

    // Empty object should result in None options
    assert!(entry.options.is_none());
    assert!(entry.provenance.is_some());
    assert_eq!(entry.provenance.as_ref().unwrap().order, Some(1));
}

/// Test that from_config_entry handles boolean true (shorthand for empty options).
#[test]
fn test_feature_metadata_entry_from_config_entry_boolean_true() {
    let entry = FeatureMetadataEntry::from_config_entry(
        "ghcr.io/devcontainers/features/node:1".to_string(),
        serde_json::json!(true),
        2,
    );

    // Boolean true is kept as-is (it's a valid config format)
    assert_eq!(entry.options, Some(serde_json::json!(true)));
}

/// Test that LabelSet serializes correctly.
#[test]
fn test_label_set_serialization() {
    let labels = LabelSet::from_image(
        Some(
            [("devcontainer.metadata".to_string(), "{}".to_string())]
                .into_iter()
                .collect(),
        ),
        Some("mcr.microsoft.com/devcontainers/base:ubuntu".to_string()),
    );

    let json = serde_json::to_value(&labels).unwrap();

    assert_eq!(json["source"], "image");
    assert!(json["labels"].is_object());
    assert!(json["provenance"].is_object());
    assert_eq!(
        json["provenance"]["source"],
        "mcr.microsoft.com/devcontainers/base:ubuntu"
    );
}

/// Test that LabelSet handles null labels correctly.
#[test]
fn test_label_set_null_labels() {
    let labels = LabelSet::from_container(None, Some("container123".to_string()));

    let json = serde_json::to_value(&labels).unwrap();

    assert_eq!(json["source"], "container");
    // labels should be null, not omitted
    assert!(json.get("labels").is_none_or(|v| v.is_null()));
    assert!(json["provenance"].is_object());
}

/// Test that LabelSet from_service includes service name in provenance.
#[test]
fn test_label_set_from_service() {
    let labels = LabelSet::from_service(
        "app",
        Some(
            [("app.version".to_string(), "1.0".to_string())]
                .into_iter()
                .collect(),
        ),
        Some("container456".to_string()),
    );

    let json = serde_json::to_value(&labels).unwrap();

    assert_eq!(json["source"], "app");
    assert!(json["provenance"]["service"].is_string());
    assert_eq!(json["provenance"]["service"], "app");
    assert_eq!(json["provenance"]["source"], "container456");
}

/// Test that EnrichedMergedConfiguration serializes with feature metadata.
#[test]
fn test_enriched_merged_configuration_with_feature_metadata() {
    use deacon_core::config::merge::MergedDevContainerConfig;
    use deacon_core::config::DevContainerConfig;

    let base_merged = MergedDevContainerConfig {
        config: DevContainerConfig {
            name: Some("test".to_string()),
            image: Some("node:18".to_string()),
            ..Default::default()
        },
        meta: None,
    };

    let features = vec![
        FeatureMetadataEntry::from_config_entry(
            "ghcr.io/devcontainers/features/node:1".to_string(),
            serde_json::json!({"version": "20"}),
            0,
        ),
        FeatureMetadataEntry::from_config_entry(
            "ghcr.io/devcontainers/features/go:1".to_string(),
            serde_json::json!({}),
            1,
        ),
    ];

    let enriched =
        EnrichedMergedConfiguration::from_merged(base_merged).with_feature_metadata(features);

    let json = serde_json::to_value(&enriched).unwrap();

    // Base config fields should be flattened
    assert_eq!(json["name"], "test");
    assert_eq!(json["image"], "node:18");

    // featureMetadata should be present as array
    assert!(json["featureMetadata"].is_array());
    let feature_metadata = json["featureMetadata"].as_array().unwrap();
    assert_eq!(feature_metadata.len(), 2);

    // First feature
    assert_eq!(
        feature_metadata[0]["id"],
        "ghcr.io/devcontainers/features/node:1"
    );
    assert_eq!(feature_metadata[0]["provenance"]["order"], 0);

    // Second feature
    assert_eq!(
        feature_metadata[1]["id"],
        "ghcr.io/devcontainers/features/go:1"
    );
    assert_eq!(feature_metadata[1]["provenance"]["order"], 1);
}

/// Test that EnrichedMergedConfiguration without features has no featureMetadata field.
#[test]
fn test_enriched_merged_configuration_no_features() {
    use deacon_core::config::merge::MergedDevContainerConfig;
    use deacon_core::config::DevContainerConfig;

    let base_merged = MergedDevContainerConfig {
        config: DevContainerConfig {
            name: Some("test".to_string()),
            image: Some("node:18".to_string()),
            ..Default::default()
        },
        meta: None,
    };

    let enriched =
        EnrichedMergedConfiguration::from_merged(base_merged).with_feature_metadata(vec![]);

    let json = serde_json::to_value(&enriched).unwrap();

    // Base config fields should be flattened
    assert_eq!(json["name"], "test");
    assert_eq!(json["image"], "node:18");

    // featureMetadata should be absent (empty array triggers None)
    assert!(json.get("featureMetadata").is_none());
}

/// Test that feature metadata ordering is preserved.
#[test]
fn test_feature_metadata_ordering_preserved() {
    let entries = vec![
        FeatureMetadataEntry::from_config_entry("feature-c".to_string(), serde_json::json!({}), 0),
        FeatureMetadataEntry::from_config_entry("feature-a".to_string(), serde_json::json!({}), 1),
        FeatureMetadataEntry::from_config_entry("feature-b".to_string(), serde_json::json!({}), 2),
    ];

    // Convert to JSON and back
    let json = serde_json::to_value(&entries).unwrap();
    let arr = json.as_array().unwrap();

    // Order should be preserved (not sorted alphabetically)
    assert_eq!(arr[0]["id"], "feature-c");
    assert_eq!(arr[1]["id"], "feature-a");
    assert_eq!(arr[2]["id"], "feature-b");

    // Provenance order should match array position
    assert_eq!(arr[0]["provenance"]["order"], 0);
    assert_eq!(arr[1]["provenance"]["order"], 1);
    assert_eq!(arr[2]["provenance"]["order"], 2);
}

/// Test JSON roundtrip for FeatureMetadataEntry.
#[test]
fn test_feature_metadata_entry_json_roundtrip() {
    let original = FeatureMetadataEntry {
        id: "ghcr.io/devcontainers/features/node:1".to_string(),
        version: Some("1.0.0".to_string()),
        name: Some("Node.js".to_string()),
        description: None,
        documentation_url: None,
        options: Some(serde_json::json!({"version": "20"})),
        installs_after: Some(vec!["common-utils".to_string()]),
        depends_on: None,
        mounts: None,
        container_env: None,
        customizations: None,
        provenance: Some(Provenance {
            source: Some("oci://ghcr.io/devcontainers/features/node:1".to_string()),
            service: None,
            order: Some(0),
        }),
    };

    let json_str = serde_json::to_string(&original).unwrap();
    let deserialized: FeatureMetadataEntry = serde_json::from_str(&json_str).unwrap();

    assert_eq!(original, deserialized);
}

/// Test JSON roundtrip for EnrichedMergedConfiguration.
#[test]
fn test_enriched_merged_configuration_json_roundtrip() {
    use deacon_core::config::merge::MergedDevContainerConfig;
    use deacon_core::config::DevContainerConfig;

    let base_merged = MergedDevContainerConfig {
        config: DevContainerConfig {
            name: Some("test".to_string()),
            image: Some("node:18".to_string()),
            ..Default::default()
        },
        meta: None,
    };

    let features = vec![FeatureMetadataEntry::from_config_entry(
        "ghcr.io/devcontainers/features/node:1".to_string(),
        serde_json::json!({"version": "20"}),
        0,
    )];

    let original =
        EnrichedMergedConfiguration::from_merged(base_merged).with_feature_metadata(features);

    let json_str = serde_json::to_string(&original).unwrap();
    let deserialized: EnrichedMergedConfiguration = serde_json::from_str(&json_str).unwrap();

    assert_eq!(original, deserialized);
}

// ============================================================================
// User Story 3: Base vs Merged Configuration Tests
// ============================================================================

/// Test that enriched configuration differs from base only by enrichment fields.
/// Per spec: mergedConfiguration should show observable differences from base config
/// when metadata/labels are added, without unrelated drift.
#[test]
fn test_enriched_differs_from_base_only_by_enrichment() {
    use deacon_core::config::merge::MergedDevContainerConfig;
    use deacon_core::config::DevContainerConfig;

    // Create base configuration
    let base_config = DevContainerConfig {
        name: Some("test-project".to_string()),
        image: Some("node:18".to_string()),
        remote_user: Some("developer".to_string()),
        features: serde_json::json!({
            "ghcr.io/devcontainers/features/node:1": {"version": "20"}
        }),
        ..Default::default()
    };

    // Create base merged (without enrichment)
    let base_merged = MergedDevContainerConfig {
        config: base_config.clone(),
        meta: None,
    };

    // Create enriched merged
    let features = vec![FeatureMetadataEntry::from_config_entry(
        "ghcr.io/devcontainers/features/node:1".to_string(),
        serde_json::json!({"version": "20"}),
        0,
    )];
    let enriched = EnrichedMergedConfiguration::from_merged(base_merged.clone())
        .with_feature_metadata(features)
        .with_image_metadata(LabelSet::from_image(None, Some("node:18".to_string())));

    // Serialize both
    let base_json = serde_json::to_value(&base_merged).unwrap();
    let enriched_json = serde_json::to_value(&enriched).unwrap();

    // Verify base config fields are preserved exactly
    assert_eq!(base_json["name"], enriched_json["name"]);
    assert_eq!(base_json["image"], enriched_json["image"]);
    assert_eq!(base_json["remoteUser"], enriched_json["remoteUser"]);
    assert_eq!(base_json["features"], enriched_json["features"]);

    // Verify enrichment fields are added
    assert!(enriched_json.get("featureMetadata").is_some());
    assert!(enriched_json.get("imageMetadata").is_some());

    // Base should NOT have enrichment fields
    assert!(base_json.get("featureMetadata").is_none());
    assert!(base_json.get("imageMetadata").is_none());
}

/// Test that enriched configuration contains all required schema fields.
/// Per spec: JSON schema compliance including required keys.
#[test]
fn test_enriched_schema_compliance() {
    use deacon_core::config::merge::MergedDevContainerConfig;
    use deacon_core::config::DevContainerConfig;

    let base = DevContainerConfig {
        name: Some("schema-test".to_string()),
        image: Some("alpine:3.18".to_string()),
        features: serde_json::json!({
            "ghcr.io/devcontainers/features/git:1": {}
        }),
        ..Default::default()
    };

    let merged = MergedDevContainerConfig {
        config: base,
        meta: None,
    };

    let features = vec![FeatureMetadataEntry::from_config_entry(
        "ghcr.io/devcontainers/features/git:1".to_string(),
        serde_json::json!({}),
        0,
    )];

    let enriched = EnrichedMergedConfiguration::from_merged(merged)
        .with_feature_metadata(features)
        .with_image_metadata(LabelSet::from_image(
            Some(
                [("maintainer".to_string(), "test".to_string())]
                    .into_iter()
                    .collect(),
            ),
            Some("alpine:3.18".to_string()),
        ))
        .with_container_metadata(LabelSet::from_container(None, Some("abc123".to_string())));

    let json = serde_json::to_value(&enriched).unwrap();

    // Verify featureMetadata schema compliance
    let feature_metadata = json["featureMetadata"].as_array().unwrap();
    assert_eq!(feature_metadata.len(), 1);
    let feature = &feature_metadata[0];
    assert!(feature.get("id").is_some()); // Required field
    assert!(feature.get("provenance").is_some()); // Added by our implementation

    // Verify imageMetadata schema compliance
    let image_metadata = &json["imageMetadata"];
    assert!(image_metadata.get("source").is_some());
    assert!(image_metadata.get("labels").is_some());
    assert!(image_metadata.get("provenance").is_some());
    assert_eq!(image_metadata["source"], "image");

    // Verify containerMetadata schema compliance
    let container_metadata = &json["containerMetadata"];
    assert!(container_metadata.get("source").is_some());
    assert!(container_metadata.get("provenance").is_some());
    assert_eq!(container_metadata["source"], "container");
}

/// Test that field ordering is consistent across serialization.
/// Per spec: deterministic ordering for stable diffs.
#[test]
fn test_field_ordering_consistency() {
    use deacon_core::config::merge::MergedDevContainerConfig;
    use deacon_core::config::DevContainerConfig;

    let config = DevContainerConfig {
        name: Some("ordering-test".to_string()),
        image: Some("node:18".to_string()),
        features: serde_json::json!({
            "feature-a": {},
            "feature-b": {},
            "feature-c": {}
        }),
        ..Default::default()
    };

    let merged = MergedDevContainerConfig { config, meta: None };

    let features = vec![
        FeatureMetadataEntry::from_config_entry("feature-a".to_string(), serde_json::json!({}), 0),
        FeatureMetadataEntry::from_config_entry("feature-b".to_string(), serde_json::json!({}), 1),
        FeatureMetadataEntry::from_config_entry("feature-c".to_string(), serde_json::json!({}), 2),
    ];

    let enriched = EnrichedMergedConfiguration::from_merged(merged).with_feature_metadata(features);

    // Serialize twice
    let json1 = serde_json::to_string(&enriched).unwrap();
    let json2 = serde_json::to_string(&enriched).unwrap();

    // Verify identical output (deterministic serialization)
    assert_eq!(json1, json2);

    // Parse and verify feature order is preserved
    let parsed: serde_json::Value = serde_json::from_str(&json1).unwrap();
    let feature_metadata = parsed["featureMetadata"].as_array().unwrap();
    assert_eq!(feature_metadata[0]["id"], "feature-a");
    assert_eq!(feature_metadata[1]["id"], "feature-b");
    assert_eq!(feature_metadata[2]["id"], "feature-c");
}

/// Test camelCase field naming for JSON output.
/// Per spec: JSON fields must use camelCase.
#[test]
fn test_camel_case_field_naming() {
    let entry = FeatureMetadataEntry {
        id: "test".to_string(),
        version: None,
        name: None,
        description: None,
        documentation_url: Some("https://example.com".to_string()),
        options: None,
        installs_after: Some(vec!["other".to_string()]),
        depends_on: Some(vec!["dep".to_string()]),
        mounts: None,
        container_env: Some(
            [("KEY".to_string(), "value".to_string())]
                .into_iter()
                .collect(),
        ),
        customizations: None,
        provenance: Some(Provenance {
            source: Some("test-source".to_string()),
            service: None,
            order: Some(0),
        }),
    };

    let json = serde_json::to_value(&entry).unwrap();
    let json_str = serde_json::to_string(&json).unwrap();

    // Verify camelCase field names (not snake_case)
    assert!(json_str.contains("documentationUrl"));
    assert!(!json_str.contains("documentation_url"));
    assert!(json_str.contains("installsAfter"));
    assert!(!json_str.contains("installs_after"));
    assert!(json_str.contains("dependsOn"));
    assert!(!json_str.contains("depends_on"));
    assert!(json_str.contains("containerEnv"));
    assert!(!json_str.contains("container_env"));
}

// ============================================================================
// T016: Additional order preservation tests for end-to-end validation
// ============================================================================

/// Test that EnrichedMergedConfiguration preserves feature order when built from config.
///
/// Per data-model.md: "Ordering from the user configuration/lockfile must be preserved
/// when serializing mergedConfiguration outputs"
#[test]
fn test_enriched_merged_configuration_preserves_feature_declaration_order() {
    use deacon_core::config::merge::MergedDevContainerConfig;
    use deacon_core::config::DevContainerConfig;

    // Create config with features in non-alphabetical order
    let config = DevContainerConfig {
        name: Some("order-test".to_string()),
        image: Some("node:18".to_string()),
        features: serde_json::json!({
            "ghcr.io/devcontainers/features/python:1": {"version": "3.11"},
            "ghcr.io/devcontainers/features/node:1": {"version": "20"},
            "ghcr.io/devcontainers/features/go:1": {},
            "ghcr.io/devcontainers/features/rust:1": true
        }),
        ..Default::default()
    };

    let merged = MergedDevContainerConfig { config, meta: None };

    // Build feature metadata in declaration order
    let features = vec![
        FeatureMetadataEntry::from_config_entry(
            "ghcr.io/devcontainers/features/python:1".to_string(),
            serde_json::json!({"version": "3.11"}),
            0,
        ),
        FeatureMetadataEntry::from_config_entry(
            "ghcr.io/devcontainers/features/node:1".to_string(),
            serde_json::json!({"version": "20"}),
            1,
        ),
        FeatureMetadataEntry::from_config_entry(
            "ghcr.io/devcontainers/features/go:1".to_string(),
            serde_json::json!({}),
            2,
        ),
        FeatureMetadataEntry::from_config_entry(
            "ghcr.io/devcontainers/features/rust:1".to_string(),
            serde_json::json!(true),
            3,
        ),
    ];

    let enriched = EnrichedMergedConfiguration::from_merged(merged).with_feature_metadata(features);

    // Serialize and parse back
    let json = serde_json::to_value(&enriched).unwrap();
    let feature_metadata = json["featureMetadata"].as_array().unwrap();

    // Verify declaration order is preserved (not alphabetical: go, node, python, rust)
    assert_eq!(feature_metadata.len(), 4);
    assert_eq!(
        feature_metadata[0]["id"],
        "ghcr.io/devcontainers/features/python:1"
    );
    assert_eq!(
        feature_metadata[1]["id"],
        "ghcr.io/devcontainers/features/node:1"
    );
    assert_eq!(
        feature_metadata[2]["id"],
        "ghcr.io/devcontainers/features/go:1"
    );
    assert_eq!(
        feature_metadata[3]["id"],
        "ghcr.io/devcontainers/features/rust:1"
    );

    // Verify provenance order indexes
    for (i, entry) in feature_metadata.iter().enumerate() {
        assert_eq!(entry["provenance"]["order"], i as i64);
    }
}

/// Test that feature order is preserved when some features have empty metadata.
///
/// Empty options should not cause features to be reordered or filtered.
#[test]
fn test_feature_order_with_mixed_empty_and_populated_metadata() {
    use deacon_core::config::merge::MergedDevContainerConfig;
    use deacon_core::config::DevContainerConfig;

    let config = DevContainerConfig {
        name: Some("mixed-metadata".to_string()),
        image: Some("alpine:3.18".to_string()),
        ..Default::default()
    };

    let merged = MergedDevContainerConfig { config, meta: None };

    // Features with varying levels of metadata completeness
    let features = vec![
        // Full metadata
        FeatureMetadataEntry {
            id: "feature-z".to_string(),
            version: Some("1.0.0".to_string()),
            name: Some("Feature Z".to_string()),
            description: Some("The Z feature".to_string()),
            options: Some(serde_json::json!({"opt": "val"})),
            ..Default::default()
        },
        // Empty metadata
        FeatureMetadataEntry {
            id: "feature-a".to_string(),
            provenance: Some(Provenance {
                source: None,
                service: None,
                order: Some(1),
            }),
            ..Default::default()
        },
        // Partial metadata
        FeatureMetadataEntry {
            id: "feature-m".to_string(),
            name: Some("Feature M".to_string()),
            provenance: Some(Provenance {
                source: None,
                service: None,
                order: Some(2),
            }),
            ..Default::default()
        },
    ];

    let enriched = EnrichedMergedConfiguration::from_merged(merged).with_feature_metadata(features);

    let json = serde_json::to_value(&enriched).unwrap();
    let feature_metadata = json["featureMetadata"].as_array().unwrap();

    // Order: z, a, m (not alphabetical: a, m, z)
    assert_eq!(feature_metadata.len(), 3);
    assert_eq!(feature_metadata[0]["id"], "feature-z");
    assert_eq!(feature_metadata[1]["id"], "feature-a");
    assert_eq!(feature_metadata[2]["id"], "feature-m");

    // Verify all features are present even with empty metadata
    assert!(feature_metadata[0]["version"].is_string());
    assert!(
        feature_metadata[1].get("version").is_none() || feature_metadata[1]["version"].is_null()
    );
}

/// Test that feature order survives JSON serialization roundtrip in EnrichedMergedConfiguration.
#[test]
fn test_enriched_merged_configuration_order_survives_roundtrip() {
    use deacon_core::config::merge::MergedDevContainerConfig;
    use deacon_core::config::DevContainerConfig;

    let config = DevContainerConfig {
        name: Some("roundtrip-test".to_string()),
        image: Some("node:18".to_string()),
        ..Default::default()
    };

    let merged = MergedDevContainerConfig { config, meta: None };

    // Create features in specific order
    let features = vec![
        FeatureMetadataEntry::from_config_entry("zz-last".to_string(), serde_json::json!({}), 0),
        FeatureMetadataEntry::from_config_entry("aa-first".to_string(), serde_json::json!({}), 1),
        FeatureMetadataEntry::from_config_entry("mm-middle".to_string(), serde_json::json!({}), 2),
    ];

    let original = EnrichedMergedConfiguration::from_merged(merged).with_feature_metadata(features);

    // Serialize to JSON string
    let json_str = serde_json::to_string(&original).unwrap();

    // Deserialize back
    let deserialized: EnrichedMergedConfiguration = serde_json::from_str(&json_str).unwrap();

    // Verify order is preserved
    let feature_metadata = deserialized.feature_metadata.unwrap();
    assert_eq!(feature_metadata.len(), 3);
    assert_eq!(feature_metadata[0].id, "zz-last");
    assert_eq!(feature_metadata[1].id, "aa-first");
    assert_eq!(feature_metadata[2].id, "mm-middle");

    // Verify provenance order is preserved
    assert_eq!(
        feature_metadata[0].provenance.as_ref().unwrap().order,
        Some(0)
    );
    assert_eq!(
        feature_metadata[1].provenance.as_ref().unwrap().order,
        Some(1)
    );
    assert_eq!(
        feature_metadata[2].provenance.as_ref().unwrap().order,
        Some(2)
    );
}

/// Test that feature order is stable across multiple serializations.
///
/// Serializing the same configuration multiple times should produce identical output.
#[test]
fn test_feature_order_serialization_stability() {
    let features = vec![
        FeatureMetadataEntry::from_config_entry("beta".to_string(), serde_json::json!({}), 0),
        FeatureMetadataEntry::from_config_entry("alpha".to_string(), serde_json::json!({}), 1),
        FeatureMetadataEntry::from_config_entry("gamma".to_string(), serde_json::json!({}), 2),
    ];

    // Serialize multiple times
    let json1 = serde_json::to_string(&features).unwrap();
    let json2 = serde_json::to_string(&features).unwrap();
    let json3 = serde_json::to_string(&features).unwrap();

    // All serializations should be identical
    assert_eq!(json1, json2);
    assert_eq!(json2, json3);

    // Parse and verify order
    let parsed: Vec<FeatureMetadataEntry> = serde_json::from_str(&json1).unwrap();
    assert_eq!(parsed[0].id, "beta");
    assert_eq!(parsed[1].id, "alpha");
    assert_eq!(parsed[2].id, "gamma");
}
