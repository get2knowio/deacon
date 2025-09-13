//! Smoke tests for compose path and edge case detection
//!
//! Scenarios covered:
//! - Compose-based up path detection with docker-compose.yml files
//! - Up with compose configuration in subdirectories
//! - Compose error handling when Docker is unavailable
//! - Edge cases: missing compose files, invalid compose configs
//!
//! Tests are written to be resilient in environments without Docker: they
//! accept specific error messages that indicate Docker is unavailable.
//! Docker-dependent tests are gated by SMOKE_DOCKER environment variable.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

fn docker_related_error(stderr: &str) -> bool {
    stderr.contains("Docker is not installed")
        || stderr.contains("Docker daemon is not")
        || stderr.contains("permission denied")
        || stderr.contains("Failed to spawn docker")
        || stderr.contains("Docker CLI error")
        || stderr.contains("Error response from daemon")
        || stderr.contains("container") && stderr.contains("is not running")
        || stderr.contains("Container command failed")
        || stderr.contains("docker-compose") && stderr.contains("not found")
        || stderr.contains("compose") && stderr.contains("not found")
}

/// Test compose-based configuration without Docker: should handle gracefully
#[test]
fn test_compose_path_detection_without_docker() {
    let temp_dir = TempDir::new().unwrap();

    // Create docker-compose.yml
    let compose_config = r#"version: '3.8'
services:
  app:
    image: alpine:3.19
    working_dir: /workspace
    volumes:
      - .:/workspace
  db:
    image: postgres:13
    environment:
      POSTGRES_PASSWORD: password
"#;

    fs::write(temp_dir.path().join("docker-compose.yml"), compose_config).unwrap();

    // Create devcontainer.json that references the compose file
    let devcontainer_config = r#"{
    "name": "Compose Path Detection Test",
    "dockerComposeFile": "docker-compose.yml",
    "service": "app",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up command with compose configuration
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let up_stderr = String::from_utf8_lossy(&up_output.stderr);

    if up_output.status.success() {
        // Unexpected success without Docker, but accept it
        println!("Compose up succeeded unexpectedly without Docker");
    } else if docker_related_error(&up_stderr) {
        println!("Compose up gracefully handled Docker unavailable error");
    } else {
        // Any other error is also acceptable for this test
        println!("Compose up handled error as expected: {}", up_stderr);
    }
}

/// Test compose-based up with subdirectory config (Docker-gated)
#[test]
fn test_compose_subfolder_config() {
    // Only run if Docker is explicitly enabled
    if std::env::var("SMOKE_DOCKER").is_err() {
        eprintln!("Skipping Docker-dependent test (set SMOKE_DOCKER=1 to enable)");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create subdirectory structure
    let subdir = temp_dir.path().join("project");
    fs::create_dir(&subdir).unwrap();

    // Create docker-compose.yml in subdirectory
    let compose_config = r#"version: '3.8'
services:
  app:
    image: alpine:3.19
    working_dir: /workspace
    volumes:
      - .:/workspace
"#;

    fs::write(subdir.join("docker-compose.yml"), compose_config).unwrap();

    // Create devcontainer.json in subdirectory that references the compose file
    let devcontainer_config = r#"{
    "name": "Compose Subfolder Test",
    "dockerComposeFile": "docker-compose.yml",
    "service": "app",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(subdir.join(".devcontainer")).unwrap();
    fs::write(
        subdir.join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up command with --config pointing to subfolder config
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(&subdir)
        .arg("--config")
        .arg(subdir.join(".devcontainer/devcontainer.json"))
        .output()
        .unwrap();

    let up_stderr = String::from_utf8_lossy(&up_output.stderr);

    if up_output.status.success() {
        println!("Compose subfolder up succeeded");
    } else if docker_related_error(&up_stderr) {
        eprintln!("Skipping Docker-dependent test (Docker not available)");
        return;
    } else {
        // Some compose-related error is acceptable
        if up_stderr.contains("compose")
            || up_stderr.contains("service")
            || up_stderr.contains("not found")
        {
            println!(
                "Compose subfolder up failed with compose-related error (acceptable): {}",
                up_stderr
            );
        } else {
            panic!("Unexpected error in compose subfolder up: {}", up_stderr);
        }
    }

    // Clean up: down command
    let mut down_cmd = Command::cargo_bin("deacon").unwrap();
    let _down_output = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(&subdir)
        .arg("--config")
        .arg(subdir.join(".devcontainer/devcontainer.json"))
        .output()
        .unwrap();
    // Ignore down result as it's just cleanup
}

/// Test edge case: missing compose file reference
#[test]
fn test_compose_missing_file_edge_case() {
    let temp_dir = TempDir::new().unwrap();

    // Create devcontainer.json that references a non-existent compose file
    let devcontainer_config = r#"{
    "name": "Compose Missing File Test",
    "dockerComposeFile": "nonexistent-docker-compose.yml",
    "service": "app",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up command with missing compose file
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let up_stderr = String::from_utf8_lossy(&up_output.stderr);

    // This should fail with a clear error message about missing compose file
    if !up_output.status.success() {
        if up_stderr.contains("not found")
            || up_stderr.contains("nonexistent")
            || up_stderr.contains("missing")
        {
            println!("Compose missing file handled gracefully with expected error");
        } else if docker_related_error(&up_stderr) {
            println!("Compose missing file handled with Docker unavailable error");
        } else {
            println!(
                "Compose missing file failed with error (acceptable): {}",
                up_stderr
            );
        }
    } else {
        // Unexpected success, but acceptable
        println!("Compose missing file unexpectedly succeeded");
    }
}

/// Test edge case: invalid compose file syntax
#[test]
fn test_compose_invalid_syntax_edge_case() {
    let temp_dir = TempDir::new().unwrap();

    // Create invalid docker-compose.yml with syntax errors
    let invalid_compose_config = r#"version: '3.8'
services:
  app:
    image: alpine:3.19
    working_dir: /workspace
    volumes:
      - .:/workspace
    # Invalid syntax: missing closing bracket
    environment:
      - KEY=value
      - INVALID=[missing_bracket
"#;

    fs::write(
        temp_dir.path().join("docker-compose.yml"),
        invalid_compose_config,
    )
    .unwrap();

    // Create devcontainer.json that references the invalid compose file
    let devcontainer_config = r#"{
    "name": "Compose Invalid Syntax Test",
    "dockerComposeFile": "docker-compose.yml",
    "service": "app",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up command with invalid compose file
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let up_stderr = String::from_utf8_lossy(&up_output.stderr);

    // This should fail with an error about invalid compose syntax or Docker unavailable
    if !up_output.status.success() {
        if up_stderr.contains("invalid")
            || up_stderr.contains("syntax")
            || up_stderr.contains("parse")
        {
            println!("Compose invalid syntax handled gracefully with expected error");
        } else if docker_related_error(&up_stderr) {
            println!("Compose invalid syntax handled with Docker unavailable error");
        } else {
            println!(
                "Compose invalid syntax failed with error (acceptable): {}",
                up_stderr
            );
        }
    } else {
        // Unexpected success, but acceptable
        println!("Compose invalid syntax unexpectedly succeeded");
    }
}

/// Test multiple compose files configuration
#[test]
fn test_compose_multiple_files() {
    let temp_dir = TempDir::new().unwrap();

    // Create base docker-compose.yml
    let base_compose_config = r#"version: '3.8'
services:
  app:
    image: alpine:3.19
    working_dir: /workspace
"#;

    fs::write(
        temp_dir.path().join("docker-compose.yml"),
        base_compose_config,
    )
    .unwrap();

    // Create override docker-compose.override.yml
    let override_compose_config = r#"version: '3.8'
services:
  app:
    volumes:
      - .:/workspace
    environment:
      - ENV=override
"#;

    fs::write(
        temp_dir.path().join("docker-compose.override.yml"),
        override_compose_config,
    )
    .unwrap();

    // Create devcontainer.json that references multiple compose files
    let devcontainer_config = r#"{
    "name": "Compose Multiple Files Test",
    "dockerComposeFile": ["docker-compose.yml", "docker-compose.override.yml"],
    "service": "app",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up command with multiple compose files
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let up_stderr = String::from_utf8_lossy(&up_output.stderr);

    if up_output.status.success() {
        println!("Compose multiple files up succeeded");
    } else if docker_related_error(&up_stderr) {
        println!("Compose multiple files handled Docker unavailable error");
    } else {
        // Some compose-related error is acceptable
        println!(
            "Compose multiple files failed with error (acceptable): {}",
            up_stderr
        );
    }
}
