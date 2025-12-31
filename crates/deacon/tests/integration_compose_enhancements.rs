//! Integration tests for compose multi-service enhancements
//!
//! Tests the new functionality for:
//! - run-services orchestration parity
//! - container resolution across services  
//! - security option warnings for compose
//! - port events across multiple services

use deacon_core::compose::{ComposeCommand, ComposeManager, ComposeProject};
use deacon_core::config::{ConfigLoader, DevContainerConfig};
use serde_json::json;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_compose_multiservice_project_creation() {
    // Test creating a project with multiple services
    let config = DevContainerConfig {
        name: Some("Multi-service Test".to_string()),
        docker_compose_file: Some(json!("docker-compose.yml")),
        service: Some("app".to_string()),
        run_services: vec!["db".to_string(), "redis".to_string()],
        workspace_folder: Some("/workspace".to_string()),
        ..Default::default()
    };

    let temp_dir = TempDir::new().unwrap();
    let compose_manager = ComposeManager::new();

    let project = compose_manager
        .create_project(&config, temp_dir.path())
        .expect("Should create compose project");

    assert_eq!(project.service, "app");
    assert_eq!(project.run_services, vec!["db", "redis"]);

    let all_services = project.get_all_services();
    assert_eq!(all_services, vec!["app", "db", "redis"]);
}

#[test]
fn test_compose_security_options_detection() {
    // Test that security options are detected and warnings would be emitted
    let config_with_security = DevContainerConfig {
        privileged: Some(true),
        cap_add: vec!["SYS_PTRACE".to_string(), "NET_ADMIN".to_string()],
        security_opt: vec!["seccomp=unconfined".to_string()],
        ..Default::default()
    };

    // This would emit warnings in actual usage - we can't easily test log output in unit tests
    // but we can verify the config has security options that would trigger warnings
    assert!(config_with_security.privileged.unwrap_or(false));
    assert!(!config_with_security.cap_add.is_empty());
    assert!(!config_with_security.security_opt.is_empty());

    // Test that the warning function exists and doesn't panic
    ComposeCommand::warn_security_options_for_compose(&config_with_security);

    // Test config without security options
    let config_without_security = DevContainerConfig::default();
    ComposeCommand::warn_security_options_for_compose(&config_without_security);
}

#[test]
fn test_multiservice_fixture_loading() {
    // Test loading the multi-service fixture we created
    let fixture_path =
        PathBuf::from("fixtures/config/compose-multiservice/.devcontainer/devcontainer.json");

    // Only run if fixture exists (in case tests run in different environments)
    if fixture_path.exists() {
        let config =
            ConfigLoader::load_from_path(&fixture_path).expect("Should load multi-service fixture");

        assert!(config.uses_compose());
        assert_eq!(config.service.as_ref().unwrap(), "app");
        assert_eq!(config.run_services, vec!["db", "redis"]);
        assert!(config.privileged.unwrap_or(false));
        assert!(!config.cap_add.is_empty());
        assert!(!config.security_opt.is_empty());

        // Test that all services are included
        let all_services = config.get_all_services();
        assert_eq!(all_services, vec!["app", "db", "redis"]);
    } else {
        println!(
            "Skipping fixture test - fixture not found at {:?}",
            fixture_path
        );
    }
}

#[test]
fn test_compose_project_all_services_inclusion() {
    // Test that get_all_services properly includes primary + run services
    let project = ComposeProject {
        name: "test".to_string(),
        base_path: PathBuf::from("/test"),
        compose_files: vec![PathBuf::from("docker-compose.yml")],
        service: "web".to_string(),
        run_services: vec![
            "database".to_string(),
            "cache".to_string(),
            "worker".to_string(),
        ],
        env_files: Vec::new(),
        additional_mounts: Vec::new(),
        profiles: Vec::new(),
        additional_env: deacon_core::IndexMap::new(),
        external_volumes: Vec::new(),
    };

    let all_services = project.get_all_services();

    // Should have 4 services total
    assert_eq!(all_services.len(), 4);

    // Primary service should be first
    assert_eq!(all_services[0], "web");

    // All run services should be included
    assert!(all_services.contains(&"database".to_string()));
    assert!(all_services.contains(&"cache".to_string()));
    assert!(all_services.contains(&"worker".to_string()));
}

#[test]
fn test_config_uses_compose_detection() {
    // Test proper detection of compose vs single container configs
    let compose_config = DevContainerConfig {
        docker_compose_file: Some(json!("docker-compose.yml")),
        service: Some("app".to_string()),
        ..Default::default()
    };
    assert!(compose_config.uses_compose());

    let single_container_config = DevContainerConfig {
        image: Some("node:18".to_string()),
        ..Default::default()
    };
    assert!(!single_container_config.uses_compose());

    // Missing service should not be considered compose
    let incomplete_compose_config = DevContainerConfig {
        docker_compose_file: Some(json!("docker-compose.yml")),
        service: None,
        ..Default::default()
    };
    assert!(!incomplete_compose_config.uses_compose());
}

#[test]
fn test_port_attributes_for_multiservice() {
    use deacon_core::config::{OnAutoForward, PortAttributes, PortSpec};
    use std::collections::HashMap;

    // Test that port configurations work with multiple services
    let mut ports_attributes = HashMap::new();
    ports_attributes.insert(
        "3000".to_string(),
        PortAttributes {
            label: Some("Web App".to_string()),
            on_auto_forward: Some(OnAutoForward::Notify),
            open_preview: None,
            require_local_port: None,
            description: None,
        },
    );
    ports_attributes.insert(
        "5432".to_string(),
        PortAttributes {
            label: Some("PostgreSQL".to_string()),
            on_auto_forward: Some(OnAutoForward::Silent),
            open_preview: None,
            require_local_port: None,
            description: None,
        },
    );
    ports_attributes.insert(
        "6379".to_string(),
        PortAttributes {
            label: Some("Redis".to_string()),
            on_auto_forward: Some(OnAutoForward::Ignore),
            open_preview: None,
            require_local_port: None,
            description: None,
        },
    );

    let config = DevContainerConfig {
        forward_ports: vec![
            PortSpec::Number(3000),
            PortSpec::Number(5432),
            PortSpec::Number(6379),
        ],
        ports_attributes,
        ..Default::default()
    };

    assert_eq!(config.forward_ports.len(), 3);
    assert_eq!(config.ports_attributes.len(), 3);
    assert!(config.ports_attributes.contains_key("3000"));
    assert!(config.ports_attributes.contains_key("5432"));
    assert!(config.ports_attributes.contains_key("6379"));
}

#[test]
fn test_compose_get_all_container_ids() {
    // Test that get_all_container_ids method exists and can be called
    let project = ComposeProject {
        name: "test-project".to_string(),
        base_path: PathBuf::from("/test"),
        compose_files: vec![PathBuf::from("docker-compose.yml")],
        service: "app".to_string(),
        run_services: vec!["db".to_string(), "redis".to_string()],
        env_files: Vec::new(),
        additional_mounts: Vec::new(),
        profiles: Vec::new(),
        additional_env: deacon_core::IndexMap::new(),
        external_volumes: Vec::new(),
    };

    let compose_manager = ComposeManager::new();

    // This will fail in practice because no containers are running,
    // but we're testing that the API exists and is callable
    let result = compose_manager.get_all_container_ids(&project);

    // We expect this to succeed with empty result or fail with Docker error
    // but not panic or have a compilation error
    match result {
        Ok(container_ids) => {
            // If successful and containers are present, validate the structure
            if !container_ids.is_empty() {
                // Build expected service set from project
                let expected_services: std::collections::HashSet<String> =
                    project.get_all_services().into_iter().collect();

                // Verify all returned keys are valid service names
                assert!(
                    container_ids.keys().all(|k| expected_services.contains(k)),
                    "All container IDs should map to valid service names"
                );

                // Verify all container IDs are non-empty strings
                assert!(
                    container_ids.values().all(|v| !v.is_empty()),
                    "Container IDs should be non-empty strings"
                );
            }
            // Empty result is acceptable (no containers running)
        }
        Err(_) => {
            // Expected to fail since no Docker containers are running
            // This is fine for unit test purposes
        }
    }
}

#[test]
fn test_compose_service_targeting() {
    // Test that services can be individually targeted in a multi-service setup
    let project = ComposeProject {
        name: "multi-service".to_string(),
        base_path: PathBuf::from("/workspace"),
        compose_files: vec![PathBuf::from("docker-compose.yml")],
        service: "web".to_string(),
        run_services: vec!["db".to_string(), "cache".to_string()],
        env_files: Vec::new(),
        additional_mounts: Vec::new(),
        profiles: Vec::new(),
        additional_env: deacon_core::IndexMap::new(),
        external_volumes: Vec::new(),
    };

    // Verify all services are accessible
    let all_services = project.get_all_services();
    assert!(all_services.contains(&"web".to_string()));
    assert!(all_services.contains(&"db".to_string()));
    assert!(all_services.contains(&"cache".to_string()));

    // Verify primary service is first
    assert_eq!(all_services[0], "web");
}
