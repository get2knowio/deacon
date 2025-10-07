//! Integration tests for feature dependency resolution system
//!
//! Tests the complete workflow of feature dependency graph resolution,
//! topological sorting, and installation plan generation.

use deacon_core::features::{FeatureDependencyResolver, FeatureMetadata, ResolvedFeature};
use std::collections::HashMap;

#[test]
fn test_real_world_feature_dependencies() {
    // Simulate a real-world scenario with multiple features that have dependencies
    let features = create_sample_features();

    let resolver = FeatureDependencyResolver::new(None);
    let plan = resolver.resolve(&features).unwrap();

    let ids = plan.feature_ids();

    // Verify installation order respects dependencies
    let node_index = ids.iter().position(|x| x == "node").unwrap();
    let docker_index = ids.iter().position(|x| x == "docker-in-docker").unwrap();
    let git_index = ids.iter().position(|x| x == "git").unwrap();
    let common_utils_index = ids.iter().position(|x| x == "common-utils").unwrap();
    let dev_tools_index = ids.iter().position(|x| x == "dev-tools").unwrap();

    // node should install after common-utils
    assert!(common_utils_index < node_index);

    // dev-tools should install after git, node, and docker-in-docker
    assert!(git_index < dev_tools_index);
    assert!(node_index < dev_tools_index);
    assert!(docker_index < dev_tools_index);
}

#[test]
fn test_override_feature_install_order() {
    let features = create_sample_features();

    // Override to install docker-in-docker first (still respecting dependencies)
    let override_order = vec!["docker-in-docker".to_string(), "node".to_string()];

    let resolver = FeatureDependencyResolver::new(Some(override_order));
    let plan = resolver.resolve(&features).unwrap();

    let ids = plan.feature_ids();

    // Dependencies should still be respected
    let common_utils_index = ids.iter().position(|x| x == "common-utils").unwrap();
    let docker_index = ids.iter().position(|x| x == "docker-in-docker").unwrap();
    let node_index = ids.iter().position(|x| x == "node").unwrap();

    // Override order should be followed where possible
    assert!(docker_index < node_index);

    // But dependencies should still be respected
    assert!(common_utils_index < node_index);
}

#[test]
fn test_complex_dependency_chain() {
    // Create a more complex dependency scenario
    let mut features = vec![
        create_feature("base", vec![], HashMap::new()),
        create_feature("middleware", vec!["base".to_string()], HashMap::new()),
        create_feature("frontend", vec!["middleware".to_string()], HashMap::new()),
        create_feature("backend", vec!["middleware".to_string()], HashMap::new()),
    ];

    // Add a feature that depends on both frontend and backend
    let mut depends_on = HashMap::new();
    depends_on.insert("frontend".to_string(), serde_json::Value::Bool(true));
    depends_on.insert("backend".to_string(), serde_json::Value::Bool(true));
    features.push(create_feature("integration", vec![], depends_on));

    let resolver = FeatureDependencyResolver::new(None);
    let plan = resolver.resolve(&features).unwrap();

    let ids = plan.feature_ids();

    // Verify proper ordering
    let base_index = ids.iter().position(|x| x == "base").unwrap();
    let middleware_index = ids.iter().position(|x| x == "middleware").unwrap();
    let frontend_index = ids.iter().position(|x| x == "frontend").unwrap();
    let backend_index = ids.iter().position(|x| x == "backend").unwrap();
    let integration_index = ids.iter().position(|x| x == "integration").unwrap();

    assert!(base_index < middleware_index);
    assert!(middleware_index < frontend_index);
    assert!(middleware_index < backend_index);
    assert!(frontend_index < integration_index);
    assert!(backend_index < integration_index);
}

#[test]
fn test_installs_after_constraints() {
    // Test features with installsAfter constraints
    let features = vec![
        create_feature("security", vec![], HashMap::new()),
        create_feature("networking", vec!["security".to_string()], HashMap::new()),
        create_feature("storage", vec!["security".to_string()], HashMap::new()),
        create_feature(
            "compute",
            vec!["networking".to_string(), "storage".to_string()],
            HashMap::new(),
        ),
    ];

    let resolver = FeatureDependencyResolver::new(None);
    let plan = resolver.resolve(&features).unwrap();

    let ids = plan.feature_ids();
    assert_eq!(ids.len(), 4);

    // Security should be first
    assert_eq!(ids[0], "security");

    // Networking and storage should come before compute
    let networking_index = ids.iter().position(|x| x == "networking").unwrap();
    let storage_index = ids.iter().position(|x| x == "storage").unwrap();
    let compute_index = ids.iter().position(|x| x == "compute").unwrap();

    assert!(networking_index < compute_index);
    assert!(storage_index < compute_index);
}

/// Create sample features that simulate real devcontainer features
fn create_sample_features() -> Vec<ResolvedFeature> {
    vec![
        create_feature("common-utils", vec![], HashMap::new()),
        create_feature("git", vec![], HashMap::new()),
        create_feature("node", vec!["common-utils".to_string()], HashMap::new()),
        create_feature("docker-in-docker", vec![], HashMap::new()),
        {
            // dev-tools depends on git, node, and docker-in-docker
            let mut depends_on = HashMap::new();
            depends_on.insert("git".to_string(), serde_json::Value::Bool(true));
            depends_on.insert("node".to_string(), serde_json::Value::Bool(true));
            depends_on.insert(
                "docker-in-docker".to_string(),
                serde_json::Value::Bool(true),
            );
            create_feature("dev-tools", vec![], depends_on)
        },
    ]
}

/// Helper function to create a test feature
fn create_feature(
    id: &str,
    installs_after: Vec<String>,
    depends_on: HashMap<String, serde_json::Value>,
) -> ResolvedFeature {
    let metadata = FeatureMetadata {
        id: id.to_string(),
        version: Some("1.0.0".to_string()),
        name: Some(format!("Test {}", id)),
        description: Some(format!("Test feature for {}", id)),
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
        installs_after,
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
