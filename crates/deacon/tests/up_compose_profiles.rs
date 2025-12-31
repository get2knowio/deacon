//! Integration tests for compose mount conversion and profile selection
//!
//! Tests User Story 3 requirements:
//! - Additional mounts converted to compose volumes
//! - Profile selection and application from compose files
//! - Project name propagation from .env files
//! - Mount format conversion (bind -> compose volume)
//!
//! Related spec: specs/001-up-gap-spec/contracts/up.md
//! Task: T018 [P] [US3]

use assert_cmd::Command;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Test that additional mounts from CLI are properly converted to compose volumes
#[test]
#[ignore] // TODO: Enable when T020 is implemented
fn test_compose_mount_conversion_bind_to_volume() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Create a minimal compose setup
    let devcontainer_json = json!({
        "name": "Mount Conversion Test",
        "dockerComposeFile": "docker-compose.yml",
        "service": "app",
        "workspaceFolder": "/workspace"
    });

    let compose_yml = r#"
version: '3.8'
services:
  app:
    image: mcr.microsoft.com/devcontainers/base:ubuntu
    volumes:
      - ../..:/workspace:cached
    command: sleep infinity
"#;

    fs::write(
        workspace.join("devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_json).unwrap(),
    )
    .unwrap();
    fs::write(workspace.join("docker-compose.yml"), compose_yml).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--mount")
        .arg("type=bind,source=/host/cache,target=/cache")
        .arg("--mount")
        .arg("type=volume,source=mydata,target=/data,external=true");

    // When implemented, this should:
    // 1. Convert bind mounts to compose volume declarations
    // 2. Handle external volumes properly
    // 3. Emit success JSON with proper volume configuration
    cmd.assert().success();

    // TODO: Add assertions for JSON output structure when T020 is complete
}

/// Test that compose profiles are properly selected and applied
#[test]
#[ignore] // TODO: Enable when T020 is implemented
fn test_compose_profile_selection() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Create compose setup with profiles
    let devcontainer_json = json!({
        "name": "Profile Test",
        "dockerComposeFile": "docker-compose.yml",
        "service": "app",
        "workspaceFolder": "/workspace",
        "runServices": ["app", "db"]
    });

    let compose_yml = r#"
version: '3.8'
services:
  app:
    image: mcr.microsoft.com/devcontainers/base:ubuntu
    volumes:
      - ../..:/workspace:cached
    command: sleep infinity
    profiles:
      - dev

  db:
    image: postgres:15
    environment:
      - POSTGRES_PASSWORD=test
    profiles:
      - dev

  cache:
    image: redis:7
    profiles:
      - full
"#;

    fs::write(
        workspace.join("devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_json).unwrap(),
    )
    .unwrap();
    fs::write(workspace.join("docker-compose.yml"), compose_yml).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--include-configuration");

    // When implemented, this should:
    // 1. Detect profiles from services in runServices
    // 2. Apply "dev" profile automatically (app + db services both have it)
    // 3. Not start "cache" service (has different profile "full")
    cmd.assert().success();

    // TODO: Verify only app and db services are started, not cache
}

/// Test that project name is properly propagated from .env file
#[test]
#[ignore] // TODO: Enable when T020 is implemented
fn test_compose_project_name_from_env_file() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Create compose setup with .env file
    let devcontainer_json = json!({
        "name": "Env Project Name Test",
        "dockerComposeFile": "docker-compose.yml",
        "service": "app",
        "workspaceFolder": "/workspace"
    });

    let compose_yml = r#"
version: '3.8'
services:
  app:
    image: mcr.microsoft.com/devcontainers/base:ubuntu
    volumes:
      - ../..:/workspace:cached
    command: sleep infinity
"#;

    let env_file = r#"
COMPOSE_PROJECT_NAME=my-custom-project
OTHER_VAR=value
"#;

    fs::write(
        workspace.join("devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_json).unwrap(),
    )
    .unwrap();
    fs::write(workspace.join("docker-compose.yml"), compose_yml).unwrap();
    fs::write(workspace.join(".env"), env_file).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--include-configuration");

    // When implemented, this should:
    // 1. Read .env file from workspace
    // 2. Extract COMPOSE_PROJECT_NAME
    // 3. Use it as the compose project name
    // 4. Return it in composeProjectName field of success JSON
    cmd.assert().success();

    // TODO: Parse JSON output and verify composeProjectName == "my-custom-project"
}

/// Test that mount conversion works with existing fixtures
#[test]
#[ignore] // TODO: Enable when T020 is implemented and fixtures are ready
fn test_compose_mount_conversion_with_fixture() {
    let fixture_path = PathBuf::from("fixtures/devcontainer-up/compose-with-profiles");

    // Only run if fixture exists
    if !fixture_path.exists() {
        println!(
            "Skipping fixture test - fixture not found at {:?}",
            fixture_path
        );
        return;
    }

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(&fixture_path)
        .arg("--mount")
        .arg("type=bind,source=/tmp/extra,target=/extra")
        .arg("--include-configuration");

    // Should successfully convert mount and apply profiles from fixture
    cmd.assert().success();
}

/// Test mount conversion with volume type and external flag
#[test]
#[ignore] // TODO: Enable when T020 is implemented
fn test_compose_external_volume_conversion() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    let devcontainer_json = json!({
        "name": "External Volume Test",
        "dockerComposeFile": "docker-compose.yml",
        "service": "app",
        "workspaceFolder": "/workspace"
    });

    let compose_yml = r#"
version: '3.8'
services:
  app:
    image: mcr.microsoft.com/devcontainers/base:ubuntu
    volumes:
      - ../..:/workspace:cached
    command: sleep infinity
"#;

    fs::write(
        workspace.join("devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_json).unwrap(),
    )
    .unwrap();
    fs::write(workspace.join("docker-compose.yml"), compose_yml).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--mount")
        .arg("type=volume,source=shared-data,target=/data,external=true");

    // When implemented, this should:
    // 1. Recognize external=true flag
    // 2. Configure compose volume with external: true
    // 3. Not attempt to create the volume (use existing)
    cmd.assert().success();
}

/// Test that profile selection works with multiple profiles per service
#[test]
#[ignore] // TODO: Enable when T020 is implemented
fn test_compose_multiple_profiles_per_service() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    let devcontainer_json = json!({
        "name": "Multiple Profiles Test",
        "dockerComposeFile": "docker-compose.yml",
        "service": "app",
        "workspaceFolder": "/workspace",
        "runServices": ["app"]
    });

    let compose_yml = r#"
version: '3.8'
services:
  app:
    image: mcr.microsoft.com/devcontainers/base:ubuntu
    volumes:
      - ../..:/workspace:cached
    command: sleep infinity
    profiles:
      - dev
      - test
"#;

    fs::write(
        workspace.join("devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_json).unwrap(),
    )
    .unwrap();
    fs::write(workspace.join("docker-compose.yml"), compose_yml).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--include-configuration");

    // Should handle services with multiple profiles correctly
    cmd.assert().success();
}

/// Test project name fallback when no .env file exists
#[test]
#[ignore] // TODO: Enable when T020 is implemented
fn test_compose_project_name_fallback_without_env() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    let devcontainer_json = json!({
        "name": "No Env Fallback Test",
        "dockerComposeFile": "docker-compose.yml",
        "service": "app",
        "workspaceFolder": "/workspace"
    });

    let compose_yml = r#"
version: '3.8'
services:
  app:
    image: mcr.microsoft.com/devcontainers/base:ubuntu
    volumes:
      - ../..:/workspace:cached
    command: sleep infinity
"#;

    fs::write(
        workspace.join("devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_json).unwrap(),
    )
    .unwrap();
    fs::write(workspace.join("docker-compose.yml"), compose_yml).unwrap();

    // Note: No .env file created

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--include-configuration");

    // Should fall back to default project name (typically directory name)
    cmd.assert().success();

    // TODO: Verify composeProjectName in JSON output uses fallback naming
}
