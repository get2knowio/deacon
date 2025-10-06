//! Integration tests for entrypoint merge semantics with compose and features
//!
//! Tests verify the deterministic entrypoint merge behavior when using
//! compose-based devcontainers with features that define entrypoints.

use deacon_core::entrypoint::{EntrypointMergeStrategy, EntrypointMerger};
use deacon_core::features::FeatureMetadata;
use std::collections::HashMap;

fn create_feature_with_entrypoint(id: &str, entrypoint: Option<String>) -> FeatureMetadata {
    FeatureMetadata {
        id: id.to_string(),
        version: Some("1.0.0".to_string()),
        name: Some(format!("Feature {}", id)),
        description: None,
        documentation_url: None,
        license_url: None,
        options: HashMap::new(),
        container_env: HashMap::new(),
        mounts: vec![],
        init: None,
        privileged: None,
        cap_add: vec![],
        security_opt: vec![],
        entrypoint,
        installs_after: vec![],
        depends_on: HashMap::new(),
        on_create_command: None,
        update_content_command: None,
        post_create_command: None,
        post_start_command: None,
        post_attach_command: None,
    }
}

#[test]
fn test_compose_entrypoint_precedence_over_features() {
    // Scenario: Compose service has explicit entrypoint, feature also has entrypoint
    // Expected: Compose entrypoint takes precedence
    let feature = create_feature_with_entrypoint("node", Some("/usr/local/bin/node".to_string()));
    let features = vec![&feature];

    let result = EntrypointMerger::merge_entrypoints(
        Some("/docker-entrypoint.sh"),
        &features,
        Some("/bin/bash"),
        EntrypointMergeStrategy::Wrap,
    );

    assert_eq!(result.entrypoint, Some("/docker-entrypoint.sh".to_string()));
    assert!(result.wrapper_script_path.is_none());
    assert!(result.description.contains("Compose service"));
}

#[test]
fn test_feature_entrypoint_wraps_base() {
    // Scenario: Feature has entrypoint, base image has entrypoint, no compose override
    // Expected: Wrapper script that invokes feature then base
    let feature = create_feature_with_entrypoint("node", Some("/usr/local/bin/node".to_string()));
    let features = vec![&feature];

    let result = EntrypointMerger::merge_entrypoints(
        None,
        &features,
        Some("/bin/bash"),
        EntrypointMergeStrategy::Wrap,
    );

    assert!(result.entrypoint.is_some());
    assert!(result.wrapper_script_path.is_some());
    assert!(result.description.contains("Wrapper"));
    assert!(result.description.contains("node"));
}

#[test]
fn test_multiple_features_wrap_in_order() {
    // Scenario: Multiple features with entrypoints
    // Expected: Wrapper script invokes all feature entrypoints in order, then base
    let feature1 = create_feature_with_entrypoint("node", Some("/setup-node.sh".to_string()));
    let feature2 = create_feature_with_entrypoint("python", Some("/setup-python.sh".to_string()));
    let feature3 = create_feature_with_entrypoint("rust", Some("/setup-rust.sh".to_string()));
    let features = vec![&feature1, &feature2, &feature3];

    let result = EntrypointMerger::merge_entrypoints(
        None,
        &features,
        Some("/bin/bash"),
        EntrypointMergeStrategy::Wrap,
    );

    assert!(result.entrypoint.is_some());
    assert!(result.wrapper_script_path.is_some());
    assert!(result.description.contains("Wrapper"));
    assert!(result.description.contains("node"));
    assert!(result.description.contains("python"));
    assert!(result.description.contains("rust"));

    // Generate actual script and verify order
    let script = EntrypointMerger::generate_wrapper_script(&features, Some("/bin/bash"));
    assert!(script.contains("setup-node.sh"));
    assert!(script.contains("setup-python.sh"));
    assert!(script.contains("setup-rust.sh"));

    // Verify feature order in script
    let node_pos = script.find("setup-node.sh").unwrap();
    let python_pos = script.find("setup-python.sh").unwrap();
    let rust_pos = script.find("setup-rust.sh").unwrap();
    let bash_pos = script.find("/bin/bash").unwrap();

    assert!(node_pos < python_pos, "node should come before python");
    assert!(python_pos < rust_pos, "python should come before rust");
    assert!(rust_pos < bash_pos, "rust should come before bash");
}

#[test]
fn test_ignore_strategy_skips_feature_entrypoints() {
    // Scenario: Feature has entrypoint but strategy is Ignore
    // Expected: Only base entrypoint is used
    let feature = create_feature_with_entrypoint("node", Some("/usr/local/bin/node".to_string()));
    let features = vec![&feature];

    let result = EntrypointMerger::merge_entrypoints(
        None,
        &features,
        Some("/bin/bash"),
        EntrypointMergeStrategy::Ignore,
    );

    assert_eq!(result.entrypoint, Some("/bin/bash".to_string()));
    assert!(result.wrapper_script_path.is_none());
    assert!(result.description.contains("ignored"));
}

#[test]
fn test_replace_strategy_uses_last_feature() {
    // Scenario: Multiple features with entrypoints and Replace strategy
    // Expected: Last feature's entrypoint replaces base
    let feature1 = create_feature_with_entrypoint("node", Some("/setup-node.sh".to_string()));
    let feature2 = create_feature_with_entrypoint("python", Some("/setup-python.sh".to_string()));
    let features = vec![&feature1, &feature2];

    let result = EntrypointMerger::merge_entrypoints(
        None,
        &features,
        Some("/bin/bash"),
        EntrypointMergeStrategy::Replace,
    );

    assert_eq!(result.entrypoint, Some("/setup-python.sh".to_string()));
    assert!(result.wrapper_script_path.is_none());
    assert!(result.description.contains("python"));
    assert!(result.description.contains("replaced"));
}

#[test]
fn test_no_base_entrypoint_with_feature() {
    // Scenario: Feature has entrypoint but no base entrypoint
    // Expected: Wrapper script only invokes feature
    let feature = create_feature_with_entrypoint("node", Some("/usr/local/bin/node".to_string()));
    let features = vec![&feature];

    let result =
        EntrypointMerger::merge_entrypoints(None, &features, None, EntrypointMergeStrategy::Wrap);

    assert!(result.entrypoint.is_some());
    assert!(result.wrapper_script_path.is_some());
    assert!(!result.description.contains("base"));

    let script = EntrypointMerger::generate_wrapper_script(&features, None);
    assert!(script.contains("node"));
    assert!(script.contains("exec \"$@\""));
    assert!(!script.contains("Original entrypoint"));
}

#[test]
fn test_mixed_features_some_with_entrypoint() {
    // Scenario: Some features have entrypoints, others don't
    // Expected: Only features with entrypoints are included in wrapper
    let feature1 = create_feature_with_entrypoint("node", Some("/setup-node.sh".to_string()));
    let feature2 = create_feature_with_entrypoint("tools", None); // No entrypoint
    let feature3 = create_feature_with_entrypoint("python", Some("/setup-python.sh".to_string()));
    let features = vec![&feature1, &feature2, &feature3];

    let result = EntrypointMerger::merge_entrypoints(
        None,
        &features,
        Some("/bin/bash"),
        EntrypointMergeStrategy::Wrap,
    );

    assert!(result.entrypoint.is_some());
    assert!(result.wrapper_script_path.is_some());
    assert!(result.description.contains("node"));
    assert!(!result.description.contains("tools"));
    assert!(result.description.contains("python"));

    // Only features with entrypoints should be in the script
    let features_with_ep: Vec<&FeatureMetadata> = features
        .iter()
        .filter(|f| f.entrypoint.is_some())
        .copied()
        .collect();

    let script = EntrypointMerger::generate_wrapper_script(&features_with_ep, Some("/bin/bash"));
    assert!(script.contains("setup-node.sh"));
    assert!(script.contains("setup-python.sh"));
    assert!(!script.contains("tools"));
}

#[test]
fn test_wrapper_script_structure() {
    // Verify the generated wrapper script has correct structure
    let feature = create_feature_with_entrypoint(
        "test-feature",
        Some("/usr/local/bin/feature-init.sh".to_string()),
    );
    let features = vec![&feature];

    let script = EntrypointMerger::generate_wrapper_script(&features, Some("/bin/bash"));

    // Check for shell shebang
    assert!(script.starts_with("#!/bin/sh"));

    // Check for error handling
    assert!(script.contains("set -e"));

    // Check for feature execution
    assert!(script.contains("test-feature"));
    assert!(script.contains("/usr/local/bin/feature-init.sh"));

    // Check for original entrypoint execution
    assert!(script.contains("exec /bin/bash \"$@\""));

    // Should have proper comments
    assert!(script.contains("# DevContainer entrypoint wrapper"));
}

#[test]
fn test_validation_allows_multiple_features() {
    // Validation should allow multiple features with entrypoints
    let feature1 = create_feature_with_entrypoint("node", Some("/setup-node.sh".to_string()));
    let feature2 = create_feature_with_entrypoint("python", Some("/setup-python.sh".to_string()));
    let features = vec![&feature1, &feature2];

    let result = EntrypointMerger::validate_merge(None, &features, Some("/bin/bash"));

    assert!(result.is_ok());
}

#[test]
fn test_validation_with_compose_always_succeeds() {
    // When compose entrypoint is present, validation always succeeds
    let feature = create_feature_with_entrypoint("node", Some("/setup-node.sh".to_string()));
    let features = vec![&feature];

    let result =
        EntrypointMerger::validate_merge(Some("/compose-ep.sh"), &features, Some("/bin/bash"));

    assert!(result.is_ok());
}

#[test]
fn test_entrypoint_merge_strategy_parsing() {
    // Verify strategy parsing is case-insensitive and handles all variants
    assert_eq!(
        "wrap".parse::<EntrypointMergeStrategy>().unwrap(),
        EntrypointMergeStrategy::Wrap
    );
    assert_eq!(
        "WRAP".parse::<EntrypointMergeStrategy>().unwrap(),
        EntrypointMergeStrategy::Wrap
    );
    assert_eq!(
        "ignore".parse::<EntrypointMergeStrategy>().unwrap(),
        EntrypointMergeStrategy::Ignore
    );
    assert_eq!(
        "IGNORE".parse::<EntrypointMergeStrategy>().unwrap(),
        EntrypointMergeStrategy::Ignore
    );
    assert_eq!(
        "replace".parse::<EntrypointMergeStrategy>().unwrap(),
        EntrypointMergeStrategy::Replace
    );
    assert_eq!(
        "REPLACE".parse::<EntrypointMergeStrategy>().unwrap(),
        EntrypointMergeStrategy::Replace
    );

    // Invalid strategy
    assert!("invalid".parse::<EntrypointMergeStrategy>().is_err());
}
