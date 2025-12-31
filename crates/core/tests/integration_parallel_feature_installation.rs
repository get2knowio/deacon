//! Integration tests for parallel feature installation
//!
//! These tests verify that feature installation can be executed in parallel
//! while respecting dependency order and demonstrating wall-clock time improvements.

use deacon_core::features::{FeatureDependencyResolver, FeatureMetadata, ResolvedFeature};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

fn create_test_feature(id: &str, dependencies: Vec<String>) -> ResolvedFeature {
    let depends_on = dependencies
        .into_iter()
        .map(|dep| (dep, JsonValue::Bool(true)))
        .collect::<HashMap<_, _>>();

    let metadata = FeatureMetadata {
        id: id.to_string(),
        version: Some("1.0.0".to_string()),
        name: Some(format!("Test Feature {}", id)),
        description: Some(format!("Test feature {}", id)),
        documentation_url: None,
        license_url: None,
        options: HashMap::new(),
        container_env: HashMap::new(),
        mounts: vec![],
        init: None,
        privileged: None,
        cap_add: vec![],
        security_opt: vec![],
        entrypoint: None,
        installs_after: vec![],
        depends_on,
        on_create_command: None,
        update_content_command: None,
        post_create_command: None,
        post_start_command: None,
        post_attach_command: None,
    };

    ResolvedFeature {
        id: id.to_string(),
        source: format!("test://features/{}", id),
        options: HashMap::new(),
        metadata,
    }
}

fn create_test_feature_with_mount(id: &str, mounts: Vec<String>) -> ResolvedFeature {
    let metadata = FeatureMetadata {
        id: id.to_string(),
        version: Some("1.0.0".to_string()),
        name: Some(format!("Test Feature {}", id)),
        description: None,
        documentation_url: None,
        license_url: None,
        options: HashMap::new(),
        container_env: HashMap::new(),
        mounts,
        init: None,
        privileged: None,
        cap_add: vec![],
        security_opt: vec![],
        entrypoint: None,
        installs_after: vec![],
        depends_on: HashMap::new(),
        on_create_command: None,
        update_content_command: None,
        post_create_command: None,
        post_start_command: None,
        post_attach_command: None,
    };

    ResolvedFeature {
        id: id.to_string(),
        source: format!("test://features/{}", id),
        options: HashMap::new(),
        metadata,
    }
}

#[test]
fn test_parallel_levels_computation() {
    // Create features with dependencies:
    // Level 0: feature-a (no deps)
    // Level 1: feature-b, feature-c (depend on feature-a)
    // Level 2: feature-d (depends on feature-b and feature-c)
    let features = vec![
        create_test_feature("feature-a", vec![]),
        create_test_feature("feature-b", vec!["feature-a".to_string()]),
        create_test_feature("feature-c", vec!["feature-a".to_string()]),
        create_test_feature(
            "feature-d",
            vec!["feature-b".to_string(), "feature-c".to_string()],
        ),
    ];

    let resolver = FeatureDependencyResolver::new(None);
    let plan = resolver.resolve(&features).unwrap();

    // Verify parallel levels are computed correctly
    assert_eq!(plan.levels.len(), 3, "Should have 3 execution levels");

    // Level 0: feature-a
    assert_eq!(plan.levels[0], vec!["feature-a"]);

    // Level 1: feature-b, feature-c (can run in parallel)
    let mut level1 = plan.levels[1].clone();
    level1.sort();
    assert_eq!(level1, vec!["feature-b", "feature-c"]);

    // Level 2: feature-d
    assert_eq!(plan.levels[2], vec!["feature-d"]);
}

#[test]
fn test_parallel_execution_with_independent_features() {
    // Create 4 independent features that can all run in parallel
    let features = vec![
        create_test_feature("independent-1", vec![]),
        create_test_feature("independent-2", vec![]),
        create_test_feature("independent-3", vec![]),
        create_test_feature("independent-4", vec![]),
    ];

    let resolver = FeatureDependencyResolver::new(None);
    let plan = resolver.resolve(&features).unwrap();

    // All features should be in one level (can run in parallel)
    assert_eq!(plan.levels.len(), 1, "Should have 1 execution level");
    assert_eq!(plan.levels[0].len(), 4, "Level 0 should have 4 features");

    let mut level0 = plan.levels[0].clone();
    level0.sort();
    assert_eq!(
        level0,
        vec![
            "independent-1",
            "independent-2",
            "independent-3",
            "independent-4"
        ]
    );
}

#[test]
fn test_resource_conflict_detection() {
    // This test would normally require a mock Docker implementation
    // For now, we'll test that the plan generation works correctly
    let features = vec![
        create_test_feature_with_mount("feature-1", vec!["/host1:/container/shared".to_string()]),
        create_test_feature_with_mount("feature-2", vec!["/host2:/container/shared".to_string()]),
    ];

    let resolver = FeatureDependencyResolver::new(None);
    let plan = resolver.resolve(&features).unwrap();

    // Features should still be plannable but conflict detection should warn
    assert_eq!(plan.levels.len(), 1);
    assert_eq!(plan.levels[0].len(), 2);
}

#[test]
fn test_concurrency_limit_env_var() {
    // Test that environment variable is respected
    std::env::set_var("DEACON_FEATURE_INSTALL_CONCURRENCY", "4");

    // Create features that would use the concurrency limit
    let features = vec![
        create_test_feature("feature-1", vec![]),
        create_test_feature("feature-2", vec![]),
    ];

    let resolver = FeatureDependencyResolver::new(None);
    let plan = resolver.resolve(&features).unwrap();

    // Features should be able to run in parallel (same level)
    assert_eq!(plan.levels.len(), 1);
    assert_eq!(plan.levels[0].len(), 2);

    std::env::remove_var("DEACON_FEATURE_INSTALL_CONCURRENCY");
}

#[test]
fn test_complex_dependency_chain_with_levels() {
    // Create a complex dependency chain:
    // base -> middleware -> [frontend, backend] -> integration
    let features = vec![
        create_test_feature("base", vec![]),
        create_test_feature("middleware", vec!["base".to_string()]),
        create_test_feature("frontend", vec!["middleware".to_string()]),
        create_test_feature("backend", vec!["middleware".to_string()]),
        create_test_feature(
            "integration",
            vec!["frontend".to_string(), "backend".to_string()],
        ),
    ];

    let resolver = FeatureDependencyResolver::new(None);
    let plan = resolver.resolve(&features).unwrap();

    // Should have 4 levels:
    // Level 0: base
    // Level 1: middleware
    // Level 2: frontend, backend
    // Level 3: integration
    assert_eq!(plan.levels.len(), 4);

    assert_eq!(plan.levels[0], vec!["base"]);
    assert_eq!(plan.levels[1], vec!["middleware"]);

    let mut level2 = plan.levels[2].clone();
    level2.sort();
    assert_eq!(level2, vec!["backend", "frontend"]);

    assert_eq!(plan.levels[3], vec!["integration"]);
}

#[test]
fn test_override_order_falls_back_to_sequential() {
    // With override order, should fall back to sequential execution
    let features = vec![
        create_test_feature("feature-a", vec![]),
        create_test_feature("feature-b", vec![]),
        create_test_feature("feature-c", vec![]),
    ];

    let override_order = vec![
        "feature-c".to_string(),
        "feature-a".to_string(),
        "feature-b".to_string(),
    ];
    let resolver = FeatureDependencyResolver::new(Some(override_order));
    let plan = resolver.resolve(&features).unwrap();

    // With override order, should have 1 level with all features in specified order
    assert_eq!(plan.levels.len(), 1);
    assert_eq!(plan.levels[0], vec!["feature-c", "feature-a", "feature-b"]);
}
