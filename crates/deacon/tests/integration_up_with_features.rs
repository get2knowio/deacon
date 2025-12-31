//! Integration coverage for the with-features example to ensure BuildKit feature
//! installs work across the documented scenarios.
//!
//! This module also includes unit tests for feature metadata serialization that
//! verify mergedConfiguration JSON contains proper feature_metadata entries
//! without requiring Docker execution.

use assert_cmd::Command;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

// ============================================================================
// Unit Tests: Feature Metadata Serialization (No Docker Required)
// ============================================================================
// These tests verify that EnrichedMergedConfiguration correctly serializes
// feature_metadata to JSON per User Story 3 (metadata available downstream).

/// Test that EnrichedMergedConfiguration includes featureMetadata when features are present.
#[test]
fn test_enriched_config_serializes_feature_metadata() {
    use deacon_core::config::merge::{
        EnrichedMergedConfiguration, FeatureMetadataEntry, MergedDevContainerConfig, Provenance,
    };
    use deacon_core::config::DevContainerConfig;

    // Create a base merged configuration
    let base_config = DevContainerConfig {
        name: Some("test-project".to_string()),
        image: Some("alpine:3.18".to_string()),
        features: serde_json::json!({
            "ghcr.io/devcontainers/features/node:1": {"version": "20"},
            "ghcr.io/devcontainers/features/python:1": {}
        }),
        ..Default::default()
    };

    let merged = MergedDevContainerConfig {
        config: base_config,
        meta: None,
    };

    // Create feature metadata entries for all declared features
    let features = vec![
        FeatureMetadataEntry {
            id: "ghcr.io/devcontainers/features/node:1".to_string(),
            version: Some("1.5.0".to_string()),
            name: Some("Node.js".to_string()),
            description: Some("Installs Node.js, nvm, and yarn".to_string()),
            documentation_url: Some(
                "https://github.com/devcontainers/features/tree/main/src/node".to_string(),
            ),
            options: Some(serde_json::json!({"version": "20"})),
            installs_after: None,
            depends_on: None,
            mounts: None,
            container_env: None,
            customizations: None,
            provenance: Some(Provenance {
                source: Some("ghcr.io/devcontainers/features/node:1".to_string()),
                service: None,
                order: Some(0),
            }),
        },
        FeatureMetadataEntry {
            id: "ghcr.io/devcontainers/features/python:1".to_string(),
            version: Some("1.2.0".to_string()),
            name: Some("Python".to_string()),
            description: Some("Installs Python and pip".to_string()),
            documentation_url: None,
            options: None, // Empty options from {}
            installs_after: None,
            depends_on: None,
            mounts: None,
            container_env: None,
            customizations: None,
            provenance: Some(Provenance {
                source: Some("ghcr.io/devcontainers/features/python:1".to_string()),
                service: None,
                order: Some(1),
            }),
        },
    ];

    let enriched = EnrichedMergedConfiguration::from_merged(merged).with_feature_metadata(features);

    // Serialize to JSON
    let json = serde_json::to_value(&enriched).expect("serialization should succeed");

    // Verify featureMetadata is present and is an array
    assert!(
        json.get("featureMetadata").is_some(),
        "featureMetadata field should be present when features exist"
    );
    let feature_metadata = json["featureMetadata"]
        .as_array()
        .expect("featureMetadata should be an array");

    // Verify all features have entries
    assert_eq!(
        feature_metadata.len(),
        2,
        "featureMetadata should contain entries for all features"
    );

    // Verify first feature entry
    assert_eq!(
        feature_metadata[0]["id"],
        "ghcr.io/devcontainers/features/node:1"
    );
    assert_eq!(feature_metadata[0]["version"], "1.5.0");
    assert_eq!(feature_metadata[0]["name"], "Node.js");

    // Verify second feature entry
    assert_eq!(
        feature_metadata[1]["id"],
        "ghcr.io/devcontainers/features/python:1"
    );
    assert_eq!(feature_metadata[1]["version"], "1.2.0");
}

/// Test that all features have metadata entries even when metadata is minimal/empty.
/// Per spec: "Features without metadata must still appear in mergedConfiguration
/// with an empty or minimal metadata placeholder so consumers see a complete list."
#[test]
fn test_all_features_have_metadata_entries_even_if_empty() {
    use deacon_core::config::merge::{
        EnrichedMergedConfiguration, FeatureMetadataEntry, MergedDevContainerConfig,
    };
    use deacon_core::config::DevContainerConfig;

    let base_config = DevContainerConfig {
        name: Some("minimal-test".to_string()),
        image: Some("alpine:3.18".to_string()),
        features: serde_json::json!({
            "ghcr.io/devcontainers/features/git:1": {},
            "ghcr.io/devcontainers/features/github-cli:1": {},
            "./local-feature": {}
        }),
        ..Default::default()
    };

    let merged = MergedDevContainerConfig {
        config: base_config,
        meta: None,
    };

    // Create minimal metadata entries (simulating features without rich metadata)
    let features = vec![
        FeatureMetadataEntry::from_config_entry(
            "ghcr.io/devcontainers/features/git:1".to_string(),
            serde_json::json!({}),
            0,
        ),
        FeatureMetadataEntry::from_config_entry(
            "ghcr.io/devcontainers/features/github-cli:1".to_string(),
            serde_json::json!({}),
            1,
        ),
        FeatureMetadataEntry::from_config_entry(
            "./local-feature".to_string(),
            serde_json::json!({}),
            2,
        ),
    ];

    let enriched = EnrichedMergedConfiguration::from_merged(merged).with_feature_metadata(features);
    let json = serde_json::to_value(&enriched).expect("serialization should succeed");

    let feature_metadata = json["featureMetadata"]
        .as_array()
        .expect("featureMetadata should be an array");

    // All three features must have entries
    assert_eq!(
        feature_metadata.len(),
        3,
        "all declared features must have metadata entries"
    );

    // Each entry should have at least an id and provenance
    for (idx, entry) in feature_metadata.iter().enumerate() {
        assert!(
            entry.get("id").is_some(),
            "feature entry {} should have an id",
            idx
        );
        assert!(
            entry.get("provenance").is_some(),
            "feature entry {} should have provenance",
            idx
        );
        // Verify provenance order matches declaration order
        assert_eq!(
            entry["provenance"]["order"], idx,
            "feature entry {} should have correct order",
            idx
        );
    }
}

/// Test that FeatureMetadataEntry fields serialize with correct camelCase names.
/// Per spec: "JSON fields must use camelCase."
#[test]
fn test_feature_metadata_uses_camel_case_field_names() {
    use deacon_core::config::merge::{FeatureMetadataEntry, Provenance};

    let entry = FeatureMetadataEntry {
        id: "ghcr.io/devcontainers/features/node:1".to_string(),
        version: Some("1.0.0".to_string()),
        name: Some("Node.js".to_string()),
        description: Some("Installs Node.js".to_string()),
        documentation_url: Some("https://example.com/docs".to_string()),
        options: Some(serde_json::json!({"version": "20"})),
        installs_after: Some(vec!["common-utils".to_string()]),
        depends_on: Some(vec!["base".to_string()]),
        mounts: Some(vec![serde_json::json!("/host:/container")]),
        container_env: Some(
            [("NODE_VERSION".to_string(), "20".to_string())]
                .into_iter()
                .collect(),
        ),
        customizations: Some(serde_json::json!({"vscode": {}})),
        provenance: Some(Provenance {
            source: Some("oci://ghcr.io/devcontainers/features/node:1".to_string()),
            service: Some("app".to_string()),
            order: Some(0),
        }),
    };

    let json_string = serde_json::to_string(&entry).expect("serialization should succeed");

    // Verify camelCase field names (NOT snake_case)
    assert!(
        json_string.contains("documentationUrl"),
        "should use documentationUrl (camelCase)"
    );
    assert!(
        !json_string.contains("documentation_url"),
        "should NOT use documentation_url (snake_case)"
    );

    assert!(
        json_string.contains("installsAfter"),
        "should use installsAfter (camelCase)"
    );
    assert!(
        !json_string.contains("installs_after"),
        "should NOT use installs_after (snake_case)"
    );

    assert!(
        json_string.contains("dependsOn"),
        "should use dependsOn (camelCase)"
    );
    assert!(
        !json_string.contains("depends_on"),
        "should NOT use depends_on (snake_case)"
    );

    assert!(
        json_string.contains("containerEnv"),
        "should use containerEnv (camelCase)"
    );
    assert!(
        !json_string.contains("container_env"),
        "should NOT use container_env (snake_case)"
    );
}

/// Test that featureMetadata field is absent when no features are present.
/// Per spec: "skip_serializing_if = Option::is_none" ensures clean output.
#[test]
fn test_no_feature_metadata_when_empty() {
    use deacon_core::config::merge::{EnrichedMergedConfiguration, MergedDevContainerConfig};
    use deacon_core::config::DevContainerConfig;

    let base_config = DevContainerConfig {
        name: Some("no-features".to_string()),
        image: Some("alpine:3.18".to_string()),
        // No features declared
        ..Default::default()
    };

    let merged = MergedDevContainerConfig {
        config: base_config,
        meta: None,
    };

    // with_feature_metadata(vec![]) should result in None
    let enriched = EnrichedMergedConfiguration::from_merged(merged).with_feature_metadata(vec![]);
    let json = serde_json::to_value(&enriched).expect("serialization should succeed");

    // featureMetadata should be absent (not null, not empty array)
    assert!(
        json.get("featureMetadata").is_none(),
        "featureMetadata should be absent when no features exist"
    );
}

/// Test feature metadata ordering is preserved in JSON output.
/// Per spec: "Ordering from the user configuration/lockfile must be preserved."
#[test]
fn test_feature_metadata_preserves_declaration_order() {
    use deacon_core::config::merge::{
        EnrichedMergedConfiguration, FeatureMetadataEntry, MergedDevContainerConfig,
    };
    use deacon_core::config::DevContainerConfig;

    let base_config = DevContainerConfig {
        name: Some("ordering-test".to_string()),
        image: Some("alpine:3.18".to_string()),
        // Declaration order: zebra, apple, mango
        features: serde_json::json!({
            "zebra-feature": {},
            "apple-feature": {},
            "mango-feature": {}
        }),
        ..Default::default()
    };

    let merged = MergedDevContainerConfig {
        config: base_config,
        meta: None,
    };

    // Create entries in declaration order (not alphabetical)
    let features = vec![
        FeatureMetadataEntry::from_config_entry(
            "zebra-feature".to_string(),
            serde_json::json!({}),
            0,
        ),
        FeatureMetadataEntry::from_config_entry(
            "apple-feature".to_string(),
            serde_json::json!({}),
            1,
        ),
        FeatureMetadataEntry::from_config_entry(
            "mango-feature".to_string(),
            serde_json::json!({}),
            2,
        ),
    ];

    let enriched = EnrichedMergedConfiguration::from_merged(merged).with_feature_metadata(features);
    let json = serde_json::to_value(&enriched).expect("serialization should succeed");

    let feature_metadata = json["featureMetadata"]
        .as_array()
        .expect("featureMetadata should be an array");

    // Order should be preserved (NOT sorted alphabetically)
    assert_eq!(
        feature_metadata[0]["id"], "zebra-feature",
        "first feature should be zebra (declaration order)"
    );
    assert_eq!(
        feature_metadata[1]["id"], "apple-feature",
        "second feature should be apple (declaration order)"
    );
    assert_eq!(
        feature_metadata[2]["id"], "mango-feature",
        "third feature should be mango (declaration order)"
    );
}

/// Test JSON roundtrip for EnrichedMergedConfiguration with features.
/// Verifies serialization produces parseable JSON that preserves all data.
#[test]
fn test_enriched_config_json_roundtrip() {
    use deacon_core::config::merge::{
        EnrichedMergedConfiguration, FeatureMetadataEntry, MergedDevContainerConfig, Provenance,
    };
    use deacon_core::config::DevContainerConfig;

    let base_config = DevContainerConfig {
        name: Some("roundtrip-test".to_string()),
        image: Some("node:18".to_string()),
        remote_user: Some("developer".to_string()),
        ..Default::default()
    };

    let merged = MergedDevContainerConfig {
        config: base_config,
        meta: None,
    };

    let features = vec![FeatureMetadataEntry {
        id: "ghcr.io/devcontainers/features/node:1".to_string(),
        version: Some("1.5.0".to_string()),
        name: Some("Node.js".to_string()),
        description: None, // Intentionally None to test null handling
        documentation_url: None,
        options: Some(serde_json::json!({"version": "20"})),
        installs_after: None,
        depends_on: None,
        mounts: None,
        container_env: Some(
            [("NODE_PATH".to_string(), "/usr/local".to_string())]
                .into_iter()
                .collect(),
        ),
        customizations: None,
        provenance: Some(Provenance {
            source: Some("ghcr.io/devcontainers/features/node:1".to_string()),
            service: None,
            order: Some(0),
        }),
    }];

    let original = EnrichedMergedConfiguration::from_merged(merged).with_feature_metadata(features);

    // Serialize to JSON string
    let json_string = serde_json::to_string(&original).expect("serialization should succeed");

    // Deserialize back
    let deserialized: EnrichedMergedConfiguration =
        serde_json::from_str(&json_string).expect("deserialization should succeed");

    // Verify roundtrip preserves data
    assert_eq!(original, deserialized, "roundtrip should preserve data");
}

// ============================================================================
// Docker-Based Integration Tests
// ============================================================================

const WORKSPACE: &str = "examples/up/with-features";

#[test]
fn test_up_with_features_basic() {
    if !docker_available() {
        eprintln!("Skipping: docker not available");
        return;
    }

    let guard = ContainerGuard::new();
    let (container_id, image_tag) = run_up(&[], &guard);
    assert!(
        image_tag.starts_with("deacon-devcontainer-features:"),
        "expected feature-extended image tag, got {image_tag}"
    );
    guard.register(container_id);
}

#[test]
fn test_up_with_additional_features() {
    if !docker_available() {
        eprintln!("Skipping: docker not available");
        return;
    }

    let guard = ContainerGuard::new();
    let (container_id, image_tag) = run_up(
        &[
            "--additional-features",
            r#"{"ghcr.io/devcontainers/features/docker-in-docker:2":{"version":"latest"}}"#,
        ],
        &guard,
    );
    assert!(
        image_tag.starts_with("deacon-devcontainer-features:"),
        "expected feature-extended image tag, got {image_tag}"
    );
    guard.register(container_id);
}

#[test]
fn test_up_with_skip_feature_auto_mapping() {
    if !docker_available() {
        eprintln!("Skipping: docker not available");
        return;
    }

    let guard = ContainerGuard::new();
    let (container_id, image_tag) = run_up(&["--skip-feature-auto-mapping"], &guard);
    assert!(
        image_tag.starts_with("deacon-devcontainer-features:"),
        "expected feature-extended image tag, got {image_tag}"
    );
    guard.register(container_id);
}

fn run_up(extra_args: &[&str], guard: &ContainerGuard) -> (String, String) {
    let workspace = workspace_path();
    let workspace_str = workspace.to_string_lossy();

    let mut cmd = Command::cargo_bin("deacon").expect("deacon binary");
    let assert = cmd
        .current_dir(&workspace)
        .env("DEACON_LOG", "warn")
        .args([
            "up",
            "--workspace-folder",
            &*workspace_str,
            "--mount-workspace-git-root=false",
            "--remove-existing-container",
            "--skip-post-create",
        ])
        .args(extra_args)
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let trimmed = stdout.trim();
    let value: Value = serde_json::from_str::<Value>(trimmed)
        .ok()
        .or_else(|| {
            trimmed
                .rfind('{')
                .and_then(|idx| serde_json::from_str::<Value>(&trimmed[idx..]).ok())
        })
        .unwrap_or_else(|| panic!("valid JSON output\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}"));
    let container_id = value["containerId"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        !container_id.is_empty(),
        "expected containerId in output: {value:?}"
    );

    // Inspect the container to capture the image tag used.
    let inspect_output = StdCommand::new("docker")
        .args(["inspect", "-f", "{{.Config.Image}}", &container_id])
        .output()
        .expect("docker inspect");
    let image_tag = String::from_utf8_lossy(&inspect_output.stdout)
        .trim()
        .to_string();
    assert!(
        !image_tag.is_empty(),
        "docker inspect returned empty image tag for {container_id}"
    );

    // Track for cleanup.
    guard.register(container_id.clone());

    (container_id, image_tag)
}

fn docker_available() -> bool {
    StdCommand::new("docker")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Drop guard to clean up containers created by this test module.
struct ContainerGuard {
    container_ids: std::cell::RefCell<Vec<String>>,
}

impl ContainerGuard {
    fn new() -> Self {
        Self {
            container_ids: std::cell::RefCell::new(Vec::new()),
        }
    }

    fn register(&self, id: String) {
        if !id.is_empty() {
            self.container_ids.borrow_mut().push(id);
        }
    }
}

fn workspace_path() -> PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = crate_dir
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    workspace_root.join(WORKSPACE)
}

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        for id in self.container_ids.borrow().iter() {
            let _ = StdCommand::new("docker").args(["rm", "-f", id]).output();
        }
    }
}
