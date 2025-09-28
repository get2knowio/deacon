//! Integration tests for Docker Compose functionality
//!
//! These tests verify Docker Compose integration with minimal alpine services.

use deacon_core::compose::ComposeManager;
use deacon_core::config::{ConfigLoader, DevContainerConfig};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to compute expected compose project name using same rules as core
fn expected_project_name_for_path(base_path: &std::path::Path) -> String {
    const FALLBACK: &str = "deacon-compose";
    let original = base_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(FALLBACK);

    if original.is_empty() || original.chars().all(|c| c == '.') {
        return FALLBACK.to_string();
    }

    let mut sanitized = String::with_capacity(original.len());
    let mut last_was_dash = false;
    for ch in original.chars() {
        let lc = ch.to_ascii_lowercase();
        if lc.is_ascii_alphanumeric() {
            sanitized.push(lc);
            last_was_dash = false;
        } else if lc == '-' || lc == '_' {
            if !(sanitized.is_empty() && (lc == '-' || lc == '_')) {
                sanitized.push(lc);
            }
            last_was_dash = lc == '-';
        } else if !last_was_dash {
            sanitized.push('-');
            last_was_dash = true;
        }
    }

    let sanitized = sanitized
        .trim_matches(|c: char| c == '-' || c == '_')
        .to_string();

    match sanitized.chars().next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => sanitized,
        Some(_) => format!("d{}", sanitized),
        None => FALLBACK.to_string(),
    }
}

/// Create a minimal docker-compose.yml file for testing
fn create_test_compose_file(dir: &std::path::Path) -> PathBuf {
    let compose_content = r#"
services:
  app:
    image: alpine:latest
    command: sleep 3600
    environment:
      - CONTAINER_NAME=test-app
    working_dir: /workspace
    volumes:
      - .:/workspace

  db:
    image: alpine:latest
    command: sleep 3600
    environment:
      - CONTAINER_NAME=test-db

  redis:
    image: alpine:latest
    command: sleep 3600
    environment:
      - CONTAINER_NAME=test-redis
"#;

    let compose_file = dir.join("docker-compose.yml");
    fs::write(&compose_file, compose_content).expect("Failed to write compose file");
    compose_file
}

/// Create a test devcontainer.json with compose configuration
fn create_test_devcontainer_config(compose_file: &str, service: &str) -> DevContainerConfig {
    DevContainerConfig {
        name: Some("Test Compose Container".to_string()),
        docker_compose_file: Some(json!(compose_file)),
        service: Some(service.to_string()),
        run_services: vec!["db".to_string()],
        shutdown_action: Some("stopCompose".to_string()),
        workspace_folder: Some("/workspace".to_string()),
        ..Default::default()
    }
}

#[test]
fn test_compose_project_creation() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let base_path = temp_dir.path().to_path_buf();

    let _compose_file = create_test_compose_file(&base_path);
    let config = create_test_devcontainer_config("docker-compose.yml", "app");

    let manager = ComposeManager::new();
    let project = manager
        .create_project(&config, &base_path)
        .expect("Failed to create compose project");

    assert_eq!(project.service, "app");
    assert_eq!(project.run_services, vec!["db"]);
    assert_eq!(project.get_all_services(), vec!["app", "db"]);
    assert_eq!(project.compose_files.len(), 1);
    assert!(project.compose_files[0].ends_with("docker-compose.yml"));
    assert!(project.compose_files[0].exists());
}

#[test]
fn test_compose_project_multiple_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let base_path = temp_dir.path().to_path_buf();

    // Create main compose file
    let _compose_file = create_test_compose_file(&base_path);

    // Create override file
    let override_content = r#"
services:
  app:
    environment:
      - OVERRIDE_VAR=true
"#;
    let override_file = base_path.join("docker-compose.override.yml");
    fs::write(&override_file, override_content).expect("Failed to write override file");

    let config = DevContainerConfig {
        docker_compose_file: Some(json!(["docker-compose.yml", "docker-compose.override.yml"])),
        service: Some("app".to_string()),
        run_services: vec!["db".to_string(), "redis".to_string()],
        ..Default::default()
    };

    let manager = ComposeManager::new();
    let project = manager
        .create_project(&config, &base_path)
        .expect("Failed to create compose project");

    assert_eq!(project.compose_files.len(), 2);
    assert!(project.compose_files[0].ends_with("docker-compose.yml"));
    assert!(project.compose_files[1].ends_with("docker-compose.override.yml"));
    assert_eq!(project.get_all_services(), vec!["app", "db", "redis"]);
}

#[test]
fn test_compose_command_building() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let base_path = temp_dir.path().to_path_buf();

    let _compose_file = create_test_compose_file(&base_path);
    let config = create_test_devcontainer_config("docker-compose.yml", "app");

    let manager = ComposeManager::new();
    let project = manager
        .create_project(&config, &base_path)
        .expect("Failed to create compose project");

    let command_builder = manager.get_command(&project);
    let command = command_builder.build_command(&["ps", "--format", "json"]);

    let args: Vec<String> = command
        .get_args()
        .map(|s| s.to_string_lossy().to_string())
        .collect();

    // Verify the command structure
    assert!(args.contains(&"compose".to_string()));
    assert!(args.contains(&"-f".to_string()));
    assert!(args.contains(&"-p".to_string()));
    assert!(args.contains(&"ps".to_string()));
    assert!(args.contains(&"--format".to_string()));
    assert!(args.contains(&"json".to_string()));

    // Check that the project name is included
    let project_name_index = args.iter().position(|arg| arg == "-p").unwrap();
    assert_eq!(args[project_name_index + 1], project.name);
}

#[test]
fn test_config_validation_without_compose() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let base_path = temp_dir.path().to_path_buf();

    // Configuration without compose setup
    let config = DevContainerConfig {
        name: Some("Regular Container".to_string()),
        image: Some("alpine:latest".to_string()),
        ..Default::default()
    };

    let manager = ComposeManager::new();
    let result = manager.create_project(&config, &base_path);

    assert!(result.is_err());
    let error_msg = format!("{:?}", result.unwrap_err());
    println!("Error message: {}", error_msg);
    assert!(error_msg.contains("does not specify Docker Compose setup"));
}

#[test]
fn test_config_validation_missing_service() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let base_path = temp_dir.path().to_path_buf();

    // Configuration with compose file but no service
    let config = DevContainerConfig {
        docker_compose_file: Some(json!("docker-compose.yml")),
        ..Default::default()
    };

    let manager = ComposeManager::new();
    let result = manager.create_project(&config, &base_path);

    assert!(result.is_err());
    let error_msg = format!("{:?}", result.unwrap_err());
    println!("Error message: {}", error_msg);
    assert!(error_msg.contains("No service specified"));
}

#[test]
fn test_config_compose_helper_methods() {
    let config = create_test_devcontainer_config("docker-compose.yml", "app");

    assert!(config.uses_compose());
    assert_eq!(config.get_compose_files(), vec!["docker-compose.yml"]);
    assert_eq!(config.get_all_services(), vec!["app", "db"]);
    assert!(config.has_stop_compose_shutdown());

    // Test configuration without compose
    let regular_config = DevContainerConfig {
        image: Some("alpine:latest".to_string()),
        ..Default::default()
    };

    assert!(!regular_config.uses_compose());
    assert!(regular_config.get_compose_files().is_empty());
    assert!(!regular_config.has_stop_compose_shutdown());
}

#[test]
fn test_config_loading_from_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&config_dir).expect("Failed to create config dir");

    // Create a devcontainer.json with compose configuration
    let devcontainer_content = json!({
        "name": "Compose Dev Container",
        "dockerComposeFile": "docker-compose.yml",
        "service": "app",
        "runServices": ["db", "redis"],
        "shutdownAction": "stopCompose",
        "workspaceFolder": "/workspace"
    });

    let config_file = config_dir.join("devcontainer.json");
    fs::write(&config_file, devcontainer_content.to_string()).expect("Failed to write config file");

    // Load configuration using ConfigLoader
    let loaded_config =
        ConfigLoader::load_from_path(&config_file).expect("Failed to load configuration");

    assert_eq!(
        loaded_config.name,
        Some("Compose Dev Container".to_string())
    );
    assert!(loaded_config.uses_compose());
    assert_eq!(loaded_config.service, Some("app".to_string()));
    assert_eq!(loaded_config.run_services, vec!["db", "redis"]);
    assert!(loaded_config.has_stop_compose_shutdown());
    assert_eq!(
        loaded_config.workspace_folder,
        Some("/workspace".to_string())
    );
}

#[test]
fn test_compose_project_name_generation() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let base_path = temp_dir.path().to_path_buf();

    let _compose_file = create_test_compose_file(&base_path);
    let config = create_test_devcontainer_config("docker-compose.yml", "app");

    let manager = ComposeManager::new();
    let project = manager
        .create_project(&config, &base_path)
        .expect("Failed to create compose project");

    // Project name should be derived from directory name and sanitized for compose
    let expected_name = expected_project_name_for_path(&base_path);
    assert_eq!(project.name, expected_name);
}

/// Integration test that validates Docker Compose configuration parsing
/// and project setup without actually running Docker commands.
#[test]
fn test_full_compose_integration_setup() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_dir = temp_dir.path().join("my-project");
    fs::create_dir_all(&workspace_dir).expect("Failed to create workspace dir");

    // Create compose files
    let _main_compose = create_test_compose_file(&workspace_dir);

    let override_content = r#"
services:
  app:
    environment:
      - DEBUG=true
      - NODE_ENV=development
    ports:
      - "3000:3000"
"#;
    let override_file = workspace_dir.join("docker-compose.override.yml");
    fs::write(&override_file, override_content).expect("Failed to write override file");

    // Create devcontainer configuration
    let devcontainer_dir = workspace_dir.join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).expect("Failed to create devcontainer dir");

    let devcontainer_config = json!({
        "name": "Full Stack Development",
        "dockerComposeFile": ["../docker-compose.yml", "../docker-compose.override.yml"],
        "service": "app",
        "runServices": ["db", "redis"],
        "workspaceFolder": "/workspace",
        "shutdownAction": "stopCompose",
        "customizations": {
            "vscode": {
                "extensions": ["ms-vscode.vscode-typescript-next"]
            }
        },
        "forwardPorts": [3000, 5432, 6379],
        "containerEnv": {
            "NODE_ENV": "development"
        }
    });

    let config_file = devcontainer_dir.join("devcontainer.json");
    fs::write(&config_file, devcontainer_config.to_string())
        .expect("Failed to write devcontainer config");

    // Load and validate configuration
    let config =
        ConfigLoader::load_from_path(&config_file).expect("Failed to load devcontainer config");

    assert_eq!(config.name, Some("Full Stack Development".to_string()));
    assert!(config.uses_compose());
    assert_eq!(config.service, Some("app".to_string()));
    assert_eq!(config.run_services, vec!["db", "redis"]);
    assert_eq!(config.workspace_folder, Some("/workspace".to_string()));
    assert!(config.has_stop_compose_shutdown());

    // Verify compose files parsing
    let compose_files = config.get_compose_files();
    assert_eq!(compose_files.len(), 2);
    assert!(compose_files[0].contains("docker-compose.yml"));
    assert!(compose_files[1].contains("docker-compose.override.yml"));

    // Create compose project
    let manager = ComposeManager::new();
    let project = manager
        .create_project(&config, &devcontainer_dir)
        .expect("Failed to create compose project");

    // Project name should be sanitized (leading dot removed)
    assert_eq!(
        project.name,
        expected_project_name_for_path(&devcontainer_dir)
    );
    assert_eq!(project.service, "app");
    assert_eq!(project.run_services, vec!["db", "redis"]);
    assert_eq!(project.get_all_services(), vec!["app", "db", "redis"]);

    // Verify relative path resolution
    assert_eq!(project.compose_files.len(), 2);
    assert!(project.compose_files[0].exists());
    assert!(project.compose_files[1].exists());
}
