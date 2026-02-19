//! Tests for aggregate_lifecycle_commands function
//!
//! Verifies that lifecycle commands from features and config are properly aggregated
//! with correct ordering and empty command filtering (T024 and T025).

use deacon_core::config::DevContainerConfig;
use deacon_core::container_lifecycle::{aggregate_lifecycle_commands, LifecycleCommandSource};
use deacon_core::features::{FeatureMetadata, ResolvedFeature};
use deacon_core::lifecycle::LifecyclePhase;
use serde_json::json;
use std::collections::HashMap;

#[test]
fn test_aggregate_lifecycle_commands_ordering() {
    // Create two features with onCreate commands
    let feature1 = ResolvedFeature {
        id: "node".to_string(),
        source: "ghcr.io/devcontainers/features/node".to_string(),
        options: HashMap::new(),
        metadata: FeatureMetadata {
            id: "node".to_string(),
            on_create_command: Some(json!("npm install")),
            ..Default::default()
        },
    };

    let feature2 = ResolvedFeature {
        id: "python".to_string(),
        source: "ghcr.io/devcontainers/features/python".to_string(),
        options: HashMap::new(),
        metadata: FeatureMetadata {
            id: "python".to_string(),
            on_create_command: Some(json!("pip install -r requirements.txt")),
            ..Default::default()
        },
    };

    // Create config with onCreate command
    let config = DevContainerConfig {
        on_create_command: Some(json!("echo ready")),
        ..Default::default()
    };

    let features = vec![feature1, feature2];
    let result = aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config);

    // Should have 3 commands: feature1, feature2, config
    assert_eq!(result.commands.len(), 3);

    // Verify order: features first (in installation order), then config
    assert_eq!(result.commands[0].command, json!("npm install"));
    match &result.commands[0].source {
        LifecycleCommandSource::Feature { id } => assert_eq!(id, "node"),
        _ => panic!("Expected Feature source"),
    }

    assert_eq!(
        result.commands[1].command,
        json!("pip install -r requirements.txt")
    );
    match &result.commands[1].source {
        LifecycleCommandSource::Feature { id } => assert_eq!(id, "python"),
        _ => panic!("Expected Feature source"),
    }

    assert_eq!(result.commands[2].command, json!("echo ready"));
    match &result.commands[2].source {
        LifecycleCommandSource::Config => {}
        _ => panic!("Expected Config source"),
    }
}

#[test]
fn test_aggregate_lifecycle_commands_filters_empty_null() {
    // Feature with null onCreate command
    let feature1 = ResolvedFeature {
        id: "node".to_string(),
        source: "ghcr.io/devcontainers/features/node".to_string(),
        options: HashMap::new(),
        metadata: FeatureMetadata {
            id: "node".to_string(),
            on_create_command: Some(json!(null)),
            ..Default::default()
        },
    };

    // Feature with valid command
    let feature2 = ResolvedFeature {
        id: "docker".to_string(),
        source: "ghcr.io/devcontainers/features/docker".to_string(),
        options: HashMap::new(),
        metadata: FeatureMetadata {
            id: "docker".to_string(),
            on_create_command: Some(json!("docker --version")),
            ..Default::default()
        },
    };

    // Config with null
    let config = DevContainerConfig {
        on_create_command: Some(json!(null)),
        ..Default::default()
    };

    let features = vec![feature1, feature2];
    let result = aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config);

    // Should only have 1 command (feature2's valid command) - null commands filtered
    assert_eq!(result.commands.len(), 1);
    assert_eq!(result.commands[0].command, json!("docker --version"));
    match &result.commands[0].source {
        LifecycleCommandSource::Feature { id } => assert_eq!(id, "docker"),
        _ => panic!("Expected Feature source"),
    }
}

#[test]
fn test_aggregate_lifecycle_commands_filters_empty_string() {
    // Feature with empty string command
    let feature1 = ResolvedFeature {
        id: "python".to_string(),
        source: "ghcr.io/devcontainers/features/python".to_string(),
        options: HashMap::new(),
        metadata: FeatureMetadata {
            id: "python".to_string(),
            on_create_command: Some(json!("")),
            ..Default::default()
        },
    };

    // Config with valid command
    let config = DevContainerConfig {
        on_create_command: Some(json!("echo ready")),
        ..Default::default()
    };

    let features = vec![feature1];
    let result = aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config);

    // Should only have config command - empty string filtered
    assert_eq!(result.commands.len(), 1);
    assert_eq!(result.commands[0].command, json!("echo ready"));
    match &result.commands[0].source {
        LifecycleCommandSource::Config => {}
        _ => panic!("Expected Config source"),
    }
}

#[test]
fn test_aggregate_lifecycle_commands_filters_empty_array() {
    // Feature with valid command
    let feature1 = ResolvedFeature {
        id: "node".to_string(),
        source: "ghcr.io/devcontainers/features/node".to_string(),
        options: HashMap::new(),
        metadata: FeatureMetadata {
            id: "node".to_string(),
            on_create_command: Some(json!("npm install")),
            ..Default::default()
        },
    };

    // Config with empty array command
    let config = DevContainerConfig {
        on_create_command: Some(json!([])),
        ..Default::default()
    };

    let features = vec![feature1];
    let result = aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config);

    // Should only have feature command - empty array filtered
    assert_eq!(result.commands.len(), 1);
    assert_eq!(result.commands[0].command, json!("npm install"));
    match &result.commands[0].source {
        LifecycleCommandSource::Feature { id } => assert_eq!(id, "node"),
        _ => panic!("Expected Feature source"),
    }
}

#[test]
fn test_aggregate_lifecycle_commands_filters_empty_object() {
    // Feature with empty object command
    let feature1 = ResolvedFeature {
        id: "node".to_string(),
        source: "ghcr.io/devcontainers/features/node".to_string(),
        options: HashMap::new(),
        metadata: FeatureMetadata {
            id: "node".to_string(),
            on_create_command: Some(json!({})),
            ..Default::default()
        },
    };

    // Config with valid command
    let config = DevContainerConfig {
        on_create_command: Some(json!("echo hello")),
        ..Default::default()
    };

    let features = vec![feature1];
    let result = aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config);

    // Should only have config command - empty object filtered
    assert_eq!(result.commands.len(), 1);
    assert_eq!(result.commands[0].command, json!("echo hello"));
    match &result.commands[0].source {
        LifecycleCommandSource::Config => {}
        _ => panic!("Expected Config source"),
    }
}

#[test]
fn test_aggregate_lifecycle_commands_all_empty() {
    // Feature with null command
    let feature1 = ResolvedFeature {
        id: "node".to_string(),
        source: "ghcr.io/devcontainers/features/node".to_string(),
        options: HashMap::new(),
        metadata: FeatureMetadata {
            id: "node".to_string(),
            on_create_command: Some(json!(null)),
            ..Default::default()
        },
    };

    // Config with empty string
    let config = DevContainerConfig {
        on_create_command: Some(json!("")),
        ..Default::default()
    };

    let features = vec![feature1];
    let result = aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config);

    // Should have no commands - all empty
    assert_eq!(result.commands.len(), 0);
}

#[test]
fn test_aggregate_lifecycle_commands_no_features() {
    // Config with onCreate command
    let config = DevContainerConfig {
        on_create_command: Some(json!("echo ready")),
        ..Default::default()
    };

    let features = vec![];
    let result = aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config);

    // Should only have config command
    assert_eq!(result.commands.len(), 1);
    assert_eq!(result.commands[0].command, json!("echo ready"));
    match &result.commands[0].source {
        LifecycleCommandSource::Config => {}
        _ => panic!("Expected Config source"),
    }
}

#[test]
fn test_aggregate_lifecycle_commands_complex_command_formats() {
    // Feature with object command (parallel commands)
    let feature1 = ResolvedFeature {
        id: "node".to_string(),
        source: "ghcr.io/devcontainers/features/node".to_string(),
        options: HashMap::new(),
        metadata: FeatureMetadata {
            id: "node".to_string(),
            on_create_command: Some(json!({
                "npm": "npm install",
                "build": "npm run build"
            })),
            ..Default::default()
        },
    };

    // Feature with array command
    let feature2 = ResolvedFeature {
        id: "python".to_string(),
        source: "ghcr.io/devcontainers/features/python".to_string(),
        options: HashMap::new(),
        metadata: FeatureMetadata {
            id: "python".to_string(),
            on_create_command: Some(json!(["pip", "install", "-r", "requirements.txt"])),
            ..Default::default()
        },
    };

    // Config with string command
    let config = DevContainerConfig {
        on_create_command: Some(json!("echo ready")),
        ..Default::default()
    };

    let features = vec![feature1, feature2];
    let result = aggregate_lifecycle_commands(LifecyclePhase::OnCreate, &features, &config);

    // Should have 3 commands with different formats preserved
    assert_eq!(result.commands.len(), 3);

    // Object command
    assert_eq!(
        result.commands[0].command,
        json!({
            "npm": "npm install",
            "build": "npm run build"
        })
    );

    // Array command
    assert_eq!(
        result.commands[1].command,
        json!(["pip", "install", "-r", "requirements.txt"])
    );

    // String command
    assert_eq!(result.commands[2].command, json!("echo ready"));
}
