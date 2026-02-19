//! Integration tests for feature entrypoint chaining
//!
//! Tests the `build_entrypoint_chain()` and `generate_wrapper_script()` functions
//! with realistic feature configurations. These tests exercise the core entrypoint
//! chaining logic used during `deacon up` when features declare entrypoints.
//!
//! This is part of the "docker-shared" nextest group. These tests do NOT require
//! Docker -- they test library functions directly.
//!
//! Test Coverage:
//! - T039: Integration tests for entrypoint chaining behavior

use std::collections::HashMap;

use deacon_core::features::{
    build_entrypoint_chain, generate_wrapper_script, EntrypointChain, FeatureMetadata, OptionValue,
    ResolvedFeature,
};

/// Helper to create a ResolvedFeature with an optional entrypoint.
///
/// All metadata fields that are not relevant to entrypoint chaining
/// are set to sensible defaults (empty collections, None values).
fn make_feature(id: &str, entrypoint: Option<&str>) -> ResolvedFeature {
    ResolvedFeature {
        id: id.to_string(),
        source: format!("ghcr.io/test/{}", id),
        options: HashMap::new(),
        metadata: FeatureMetadata {
            id: id.to_string(),
            version: Some("1.0.0".to_string()),
            name: Some(format!("Test Feature {}", id)),
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
            entrypoint: entrypoint.map(|s| s.to_string()),
            installs_after: vec![],
            depends_on: HashMap::new(),
            on_create_command: None,
            update_content_command: None,
            post_create_command: None,
            post_start_command: None,
            post_attach_command: None,
        },
    }
}

// ============================================================================
// build_entrypoint_chain tests
// ============================================================================

/// No features, no config entrypoint => EntrypointChain::None
#[test]
fn test_entrypoint_chain_no_features_no_config() {
    let features: Vec<ResolvedFeature> = vec![];
    let chain = build_entrypoint_chain(&features, None);
    assert_eq!(chain, EntrypointChain::None);
}

/// No features, config entrypoint present => Single with the config entrypoint
#[test]
fn test_entrypoint_chain_no_features_with_config_entrypoint() {
    let features: Vec<ResolvedFeature> = vec![];
    let chain = build_entrypoint_chain(&features, Some("/custom/init.sh"));
    assert_eq!(
        chain,
        EntrypointChain::Single("/custom/init.sh".to_string())
    );
}

/// Single feature with no entrypoint, no config entrypoint => None
#[test]
fn test_entrypoint_chain_single_feature_no_entrypoint() {
    let features = vec![make_feature("node", None)];
    let chain = build_entrypoint_chain(&features, None);
    assert_eq!(chain, EntrypointChain::None);
}

/// Simulates docker-in-docker feature which needs an init script.
/// Single feature with entrypoint, no config entrypoint => Single
#[test]
fn test_entrypoint_chain_docker_in_docker_scenario() {
    let features = vec![make_feature(
        "docker-in-docker",
        Some("/usr/local/share/docker-init.sh"),
    )];
    let chain = build_entrypoint_chain(&features, None);
    assert_eq!(
        chain,
        EntrypointChain::Single("/usr/local/share/docker-init.sh".to_string())
    );
}

/// Docker-in-docker + SSH agent both need initialization.
/// Multiple features with entrypoints => Chained
#[test]
fn test_entrypoint_chain_multiple_features_with_init() {
    let features = vec![
        make_feature("docker-in-docker", Some("/usr/local/share/docker-init.sh")),
        make_feature("ssh-agent", Some("/usr/local/share/ssh-init.sh")),
    ];
    let chain = build_entrypoint_chain(&features, None);
    match chain {
        EntrypointChain::Chained {
            entrypoints,
            wrapper_path,
        } => {
            assert_eq!(entrypoints.len(), 2);
            assert_eq!(entrypoints[0], "/usr/local/share/docker-init.sh");
            assert_eq!(entrypoints[1], "/usr/local/share/ssh-init.sh");
            // Wrapper path should be the default path
            assert!(
                !wrapper_path.is_empty(),
                "Wrapper path should be set for chained entrypoints"
            );
        }
        other => panic!("Expected Chained, got {:?}", other),
    }
}

/// Mixed features: node (no entrypoint) + docker-in-docker (has entrypoint) + python (no entrypoint).
/// Only the feature with an entrypoint should appear => Single
#[test]
fn test_entrypoint_chain_mixed_features_some_without_entrypoint() {
    let features = vec![
        make_feature("node", None),
        make_feature("docker-in-docker", Some("/usr/local/share/docker-init.sh")),
        make_feature("python", None),
    ];
    let chain = build_entrypoint_chain(&features, None);
    assert_eq!(
        chain,
        EntrypointChain::Single("/usr/local/share/docker-init.sh".to_string())
    );
}

/// Single feature with entrypoint + config entrypoint => Chained (feature first, config last)
#[test]
fn test_entrypoint_chain_with_config_entrypoint() {
    let features = vec![make_feature(
        "docker-in-docker",
        Some("/usr/local/share/docker-init.sh"),
    )];
    let chain = build_entrypoint_chain(&features, Some("/custom/init.sh"));
    match chain {
        EntrypointChain::Chained {
            entrypoints,
            wrapper_path,
        } => {
            assert_eq!(entrypoints.len(), 2);
            assert_eq!(
                entrypoints[0], "/usr/local/share/docker-init.sh",
                "Feature entrypoint should come first"
            );
            assert_eq!(
                entrypoints[1], "/custom/init.sh",
                "Config entrypoint should come last"
            );
            assert!(!wrapper_path.is_empty());
        }
        other => panic!("Expected Chained, got {:?}", other),
    }
}

/// Multiple features with entrypoints + config entrypoint => Chained with all three in order
#[test]
fn test_entrypoint_chain_multiple_features_plus_config() {
    let features = vec![
        make_feature("docker-in-docker", Some("/usr/local/share/docker-init.sh")),
        make_feature("ssh-agent", Some("/usr/local/share/ssh-init.sh")),
    ];
    let chain = build_entrypoint_chain(&features, Some("/custom/init.sh"));
    match chain {
        EntrypointChain::Chained {
            entrypoints,
            wrapper_path,
        } => {
            assert_eq!(entrypoints.len(), 3);
            assert_eq!(entrypoints[0], "/usr/local/share/docker-init.sh");
            assert_eq!(entrypoints[1], "/usr/local/share/ssh-init.sh");
            assert_eq!(entrypoints[2], "/custom/init.sh");
            assert!(!wrapper_path.is_empty());
        }
        other => panic!("Expected Chained, got {:?}", other),
    }
}

/// All features have no entrypoints, but config has one => Single with config entrypoint
#[test]
fn test_entrypoint_chain_all_features_no_entrypoint_config_has_one() {
    let features = vec![
        make_feature("node", None),
        make_feature("python", None),
        make_feature("go", None),
    ];
    let chain = build_entrypoint_chain(&features, Some("/config/entrypoint.sh"));
    assert_eq!(
        chain,
        EntrypointChain::Single("/config/entrypoint.sh".to_string())
    );
}

/// Feature installation order is preserved in the chain.
/// The entrypoint order must match the features array order.
#[test]
fn test_entrypoint_chain_preserves_installation_order() {
    let features = vec![
        make_feature("alpha", Some("/alpha/init.sh")),
        make_feature("beta", Some("/beta/init.sh")),
        make_feature("gamma", Some("/gamma/init.sh")),
    ];
    let chain = build_entrypoint_chain(&features, None);
    match chain {
        EntrypointChain::Chained { entrypoints, .. } => {
            assert_eq!(entrypoints.len(), 3);
            assert_eq!(entrypoints[0], "/alpha/init.sh");
            assert_eq!(entrypoints[1], "/beta/init.sh");
            assert_eq!(entrypoints[2], "/gamma/init.sh");
        }
        other => panic!("Expected Chained, got {:?}", other),
    }
}

// ============================================================================
// generate_wrapper_script tests
// ============================================================================

/// Wrapper script for two entrypoints should be a valid shell script
#[test]
fn test_wrapper_script_is_valid_shell() {
    let entrypoints = vec![
        "/usr/local/share/docker-init.sh".to_string(),
        "/usr/local/share/ssh-init.sh".to_string(),
    ];
    let script = generate_wrapper_script(&entrypoints);

    // Verify it starts with a shebang
    assert!(
        script.starts_with("#!/bin/sh\n"),
        "Script should start with #!/bin/sh shebang"
    );

    // Verify fail-fast semantics
    assert!(
        script.contains("|| exit $?"),
        "Script should contain fail-fast error handling (|| exit $?)"
    );

    // Verify it ends with exec "$@" to pass through user command
    assert!(
        script.ends_with("exec \"$@\"\n"),
        "Script should end with exec \"$@\" to pass through arguments"
    );

    // Verify ordering: docker-init before ssh-init
    let docker_pos = script
        .find("docker-init.sh")
        .expect("Script should contain docker-init.sh");
    let ssh_pos = script
        .find("ssh-init.sh")
        .expect("Script should contain ssh-init.sh");
    assert!(
        docker_pos < ssh_pos,
        "Docker init should come before SSH init in the wrapper script"
    );
}

/// Wrapper script with a single entrypoint still includes exec "$@"
#[test]
fn test_wrapper_script_single_entrypoint() {
    let entrypoints = vec!["/usr/local/share/docker-init.sh".to_string()];
    let script = generate_wrapper_script(&entrypoints);

    assert!(script.starts_with("#!/bin/sh\n"));
    assert!(script.contains("/usr/local/share/docker-init.sh"));
    assert!(script.contains("|| exit $?"));
    assert!(script.ends_with("exec \"$@\"\n"));
}

/// Wrapper script with empty entrypoints list produces minimal script
#[test]
fn test_wrapper_script_empty_entrypoints() {
    let entrypoints: Vec<String> = vec![];
    let script = generate_wrapper_script(&entrypoints);

    assert!(script.starts_with("#!/bin/sh\n"));
    assert!(script.ends_with("exec \"$@\"\n"));
    // No "|| exit $?" lines since there are no entrypoints
    assert!(
        !script.contains("|| exit $?"),
        "Empty entrypoints should produce no fail-fast lines"
    );
}

/// Wrapper script with three entrypoints has them in correct order
#[test]
fn test_wrapper_script_three_entrypoints_ordering() {
    let entrypoints = vec![
        "/first/init.sh".to_string(),
        "/second/init.sh".to_string(),
        "/third/init.sh".to_string(),
    ];
    let script = generate_wrapper_script(&entrypoints);

    let first_pos = script
        .find("/first/init.sh")
        .expect("Should contain first entrypoint");
    let second_pos = script
        .find("/second/init.sh")
        .expect("Should contain second entrypoint");
    let third_pos = script
        .find("/third/init.sh")
        .expect("Should contain third entrypoint");
    let exec_pos = script
        .find("exec \"$@\"")
        .expect("Should contain exec passthrough");

    assert!(first_pos < second_pos, "First should come before second");
    assert!(second_pos < third_pos, "Second should come before third");
    assert!(
        third_pos < exec_pos,
        "Third should come before exec passthrough"
    );
}

/// Wrapper script contains one fail-fast line per entrypoint
#[test]
fn test_wrapper_script_fail_fast_per_entrypoint() {
    let entrypoints = vec![
        "/a/init.sh".to_string(),
        "/b/init.sh".to_string(),
        "/c/init.sh".to_string(),
    ];
    let script = generate_wrapper_script(&entrypoints);

    // Count occurrences of "|| exit $?"
    let fail_fast_count = script.matches("|| exit $?").count();
    assert_eq!(
        fail_fast_count,
        entrypoints.len(),
        "Each entrypoint should have one fail-fast line"
    );
}

// ============================================================================
// End-to-end chain building with mock features
// ============================================================================

/// Realistic scenario: devcontainer with node, docker-in-docker, and python features.
/// Only docker-in-docker declares an entrypoint.
#[test]
fn test_realistic_devcontainer_feature_set() {
    let features = vec![
        make_feature("node", None),
        make_feature("docker-in-docker", Some("/usr/local/share/docker-init.sh")),
        make_feature("python", None),
    ];

    let chain = build_entrypoint_chain(&features, None);

    // Only docker-in-docker has an entrypoint => Single
    assert_eq!(
        chain,
        EntrypointChain::Single("/usr/local/share/docker-init.sh".to_string()),
        "Only docker-in-docker's entrypoint should be used"
    );
}

/// Realistic scenario: devcontainer with docker-in-docker (entrypoint) and
/// config init.sh entrypoint. Both should be chained.
#[test]
fn test_realistic_docker_in_docker_with_config_init() {
    let features = vec![make_feature(
        "docker-in-docker",
        Some("/usr/local/share/docker-init.sh"),
    )];

    let chain = build_entrypoint_chain(&features, Some("/usr/local/share/app-init.sh"));

    match &chain {
        EntrypointChain::Chained {
            entrypoints,
            wrapper_path,
        } => {
            assert_eq!(entrypoints.len(), 2);
            assert_eq!(entrypoints[0], "/usr/local/share/docker-init.sh");
            assert_eq!(entrypoints[1], "/usr/local/share/app-init.sh");

            // Now generate the wrapper and verify it is a valid script
            let script = generate_wrapper_script(entrypoints);
            assert!(script.starts_with("#!/bin/sh\n"));
            assert!(script.contains("docker-init.sh"));
            assert!(script.contains("app-init.sh"));

            // Docker init should come before app init
            let docker_pos = script.find("docker-init.sh").unwrap();
            let app_pos = script.find("app-init.sh").unwrap();
            assert!(
                docker_pos < app_pos,
                "Feature entrypoint should come before config entrypoint"
            );

            assert!(!wrapper_path.is_empty());
        }
        other => panic!("Expected Chained, got {:?}", other),
    }
}

/// Realistic scenario: many features, only some with entrypoints, plus config entrypoint.
/// Verifies the complete pipeline from feature list to wrapper script.
#[test]
fn test_end_to_end_chain_to_script_generation() {
    let features = vec![
        make_feature("node", None),
        make_feature("docker-in-docker", Some("/usr/local/share/docker-init.sh")),
        make_feature("python", None),
        make_feature("ssh-agent", Some("/usr/local/share/ssh-init.sh")),
        make_feature("go", None),
    ];

    let chain = build_entrypoint_chain(&features, Some("/app/startup.sh"));

    match &chain {
        EntrypointChain::Chained {
            entrypoints,
            wrapper_path,
        } => {
            // 2 feature entrypoints + 1 config entrypoint = 3
            assert_eq!(entrypoints.len(), 3);
            assert_eq!(entrypoints[0], "/usr/local/share/docker-init.sh");
            assert_eq!(entrypoints[1], "/usr/local/share/ssh-init.sh");
            assert_eq!(entrypoints[2], "/app/startup.sh");

            // Generate the wrapper script and perform structural validation
            let script = generate_wrapper_script(entrypoints);

            // Structural checks
            assert!(script.starts_with("#!/bin/sh\n"));
            assert!(script.ends_with("exec \"$@\"\n"));
            assert_eq!(script.matches("|| exit $?").count(), 3);

            // Order checks
            let docker_pos = script.find("docker-init.sh").unwrap();
            let ssh_pos = script.find("ssh-init.sh").unwrap();
            let app_pos = script.find("startup.sh").unwrap();
            let exec_pos = script.find("exec \"$@\"").unwrap();

            assert!(docker_pos < ssh_pos);
            assert!(ssh_pos < app_pos);
            assert!(app_pos < exec_pos);

            assert!(!wrapper_path.is_empty());
        }
        other => panic!("Expected Chained, got {:?}", other),
    }
}

/// Verify that OptionValue import compiles -- this ensures the test's import list is correct
/// even if OptionValue is not directly used in entrypoint chain tests.
#[test]
fn test_option_value_import_compiles() {
    let _option = OptionValue::Boolean(true);
    let _option_str = OptionValue::String("test".to_string());
}
