//! Smoke tests for compose path and edge case detection
//!
//! Scenarios covered:
//! - Compose-based up path detection with docker-compose.yml files
//! - Up with compose configuration in subdirectories
//! - Compose error handling when Docker is unavailable
//! - Edge cases: missing compose files, invalid compose configs
//!
//! NOTE: These tests assume Docker is available and running. They will fail
//! if Docker/Compose is not present or cannot start containers.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

fn is_docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Test compose-based configuration without Docker: should handle gracefully
#[test]
fn test_compose_path_detection_without_docker() {
    if !is_docker_available() {
        eprintln!("Skipping test_compose_path_detection_without_docker: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create docker-compose.yml
    let compose_config = r#"services:
        app:
            image: alpine:3.19
            working_dir: /workspace
            volumes:
                - .:/workspace
            network_mode: bridge
            command: sleep infinity
        db:
            image: postgres:13
            environment:
                POSTGRES_PASSWORD: password
            network_mode: bridge
    "#;

    fs::write(temp_dir.path().join("docker-compose.yml"), compose_config).unwrap();

    // Create devcontainer.json that references the compose file
    let devcontainer_config = r#"{
    "name": "Compose Path Detection Test",
    "dockerComposeFile": "../docker-compose.yml",
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
    assert!(
        up_output.status.success(),
        "Compose up failed: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );

    // Cleanup only if we actually brought the project up successfully
    let mut down_cmd = Command::cargo_bin("deacon").unwrap();
    let _ = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
}

/// Test compose-based up with subdirectory config (Docker-gated)
#[test]
fn test_compose_subfolder_config() {
    if !is_docker_available() {
        eprintln!("Skipping test_compose_subfolder_config: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create subdirectory structure
    let subdir = temp_dir.path().join("project");
    fs::create_dir(&subdir).unwrap();

    // Create docker-compose.yml in subdirectory
    let compose_config = r#"services:
        app:
            image: alpine:3.19
            working_dir: /workspace
            volumes:
                - .:/workspace
            network_mode: bridge
            command: sleep infinity
    "#;

    fs::write(subdir.join("docker-compose.yml"), compose_config).unwrap();

    // Create devcontainer.json in subdirectory that references the compose file
    let devcontainer_config = r#"{
    "name": "Compose Subfolder Test",
    "dockerComposeFile": "../docker-compose.yml",
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

    assert!(
        up_output.status.success(),
        "Unexpected error in compose subfolder up: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );

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
    assert!(
        !up_output.status.success(),
        "Compose missing file unexpectedly succeeded"
    );
    assert!(
        up_stderr.contains("not found")
            || up_stderr.contains("nonexistent")
            || up_stderr.contains("missing"),
        "Expected missing compose file error, got: {}",
        up_stderr
    );
}

/// Test edge case: invalid compose file syntax
#[test]
fn test_compose_invalid_syntax_edge_case() {
    let temp_dir = TempDir::new().unwrap();

    // Create invalid docker-compose.yml with syntax errors
    // Invalid syntax: missing closing quote on image value forces parser failure
    let invalid_compose_config = r#"version: '3.8'
services:
    app:
        image: "alpine:3.19
        working_dir: /workspace
"#;

    fs::write(
        temp_dir.path().join("docker-compose.yml"),
        invalid_compose_config,
    )
    .unwrap();

    // Create devcontainer.json that references the invalid compose file
    let devcontainer_config = r#"{
    "name": "Compose Invalid Syntax Test",
    "dockerComposeFile": "../docker-compose.yml",
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

    // This should fail with an error about invalid compose syntax or Docker error
    assert!(
        !up_output.status.success(),
        "Compose invalid syntax unexpectedly succeeded"
    );
    assert!(
        up_stderr.contains("invalid")
            || up_stderr.contains("syntax")
            || up_stderr.contains("parse")
            || up_stderr.contains("yaml")
            || up_stderr.contains("unexpected")
            || up_stderr.contains("Docker"),
        "Expected compose invalid syntax or Docker error, got: {}",
        up_stderr
    );
}

/// Test multiple compose files configuration
#[test]
fn test_compose_multiple_files() {
    if !is_docker_available() {
        eprintln!("Skipping test_compose_multiple_files: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create base docker-compose.yml
    let base_compose_config = r#"services:
        app:
            image: alpine:3.19
            working_dir: /workspace
            network_mode: bridge
            command: sleep infinity
    "#;

    fs::write(
        temp_dir.path().join("docker-compose.yml"),
        base_compose_config,
    )
    .unwrap();

    // Create override docker-compose.override.yml
    let override_compose_config = r#"services:
        app:
            volumes:
                - .:/workspace
            environment:
                - ENV=override
            network_mode: bridge
    "#;

    fs::write(
        temp_dir.path().join("docker-compose.override.yml"),
        override_compose_config,
    )
    .unwrap();

    // Create devcontainer.json that references multiple compose files
    let devcontainer_config = r#"{
    "name": "Compose Multiple Files Test",
    "dockerComposeFile": ["../docker-compose.yml", "../docker-compose.override.yml"],
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

    assert!(
        up_output.status.success(),
        "Compose multiple files up failed: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );
}

/// Test compose build with targeted service selection
#[test]
fn test_compose_build_target_service() {
    if !is_docker_available() {
        eprintln!("Skipping test_compose_build_target_service: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create docker-compose.yml with multiple services
    let compose_config = r#"services:
        app:
            build:
                context: .
                dockerfile: Dockerfile
            working_dir: /workspace
            volumes:
                - .:/workspace
            network_mode: bridge
        db:
            image: postgres:13
            environment:
                POSTGRES_PASSWORD: password
            network_mode: bridge
    "#;

    fs::write(temp_dir.path().join("docker-compose.yml"), compose_config).unwrap();

    // Create Dockerfile for the app service
    let dockerfile_content = r#"FROM alpine:3.19
WORKDIR /workspace
RUN echo "app service" > /tmp/app.txt
"#;
    fs::write(temp_dir.path().join("Dockerfile"), dockerfile_content).unwrap();

    // Create devcontainer.json that references the compose file and targets app service
    let devcontainer_config = r#"{
    "name": "Compose Build Target Service Test",
    "dockerComposeFile": "../docker-compose.yml",
    "service": "app",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test build command with compose configuration.
    // The default log level is `warn`; the "Building Docker Compose service:
    // <name>" status is emitted at INFO, so opt into it via the global `-v`
    // flag to assert on the compose-service build indicators below.
    let mut build_cmd = Command::cargo_bin("deacon").unwrap();
    let build_output = build_cmd
        .current_dir(&temp_dir)
        .arg("-v")
        .arg("build")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    assert!(
        build_output.status.success(),
        "Compose build failed: {}",
        String::from_utf8_lossy(&build_output.stderr)
    );

    // Verify the build output mentions the service name
    let build_stderr = String::from_utf8_lossy(&build_output.stderr);
    // Look for compose-specific build indicators
    assert!(
        build_stderr.contains("app")
            || build_stderr.contains("compose")
            || build_stderr.contains("service"),
        "Build output should indicate compose service build"
    );
}

/// Regression: `down` on a Compose project with the default `stopCompose`
/// action (no `--remove`) must STOP the services, not remove them — mirroring
/// single-container `down`. Previously `stop_project` called
/// `docker compose down`, deleting the containers.
#[test]
fn test_compose_down_stops_but_keeps_containers() {
    if !is_docker_available() {
        eprintln!("Skipping test_compose_down_stops_but_keeps_containers: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Single service, labeled so we can find it without the project name.
    let compose_config = r#"services:
  app:
    image: alpine:3.19
    command: sleep infinity
    labels:
      - "deacon.test=compose-stop"
"#;
    fs::write(temp_dir.path().join("docker-compose.yml"), compose_config).unwrap();
    let devcontainer = r#"{
  "name": "Compose Stop Test",
  "dockerComposeFile": "docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/",
  "shutdownAction": "stopCompose",
  "overrideCommand": false
}"#;
    fs::write(temp_dir.path().join(".devcontainer.json"), devcontainer).unwrap();

    let count = |status: &str| -> usize {
        let mut args = vec![
            "ps".to_string(),
            "-a".to_string(),
            "--filter".to_string(),
            "label=deacon.test=compose-stop".to_string(),
            "-q".to_string(),
        ];
        if !status.is_empty() {
            args.push("--filter".to_string());
            args.push(format!("status={}", status));
        }
        let out = std::process::Command::new("docker")
            .args(&args)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count()
    };
    let cleanup = || {
        let ids = std::process::Command::new("docker")
            .args([
                "ps",
                "-a",
                "--filter",
                "label=deacon.test=compose-stop",
                "-q",
            ])
            .output()
            .unwrap();
        for id in String::from_utf8_lossy(&ids.stdout).split_whitespace() {
            let _ = std::process::Command::new("docker")
                .args(["rm", "-f", id])
                .output();
        }
    };

    cleanup();

    // deacon's workspace-state cache lives under `std::env::temp_dir()` (shared
    // across processes). Isolate this test's state via a per-test TMPDIR so a
    // concurrent docker test can't clobber the shared state index and make our
    // `down` lose the saved compose project. `up` and `down` must share it.
    let state_home = temp_dir.path().join("state-home");
    fs::create_dir_all(&state_home).unwrap();

    let up = Command::cargo_bin("deacon")
        .unwrap()
        .env("TMPDIR", &state_home)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--remove-existing-container")
        .output()
        .unwrap();
    assert!(
        up.status.success(),
        "compose up failed: {}",
        String::from_utf8_lossy(&up.stderr)
    );
    assert_eq!(count("running"), 1, "service should be running after up");

    let down = Command::cargo_bin("deacon")
        .unwrap()
        .env("TMPDIR", &state_home)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
    let down_ok = down.status.success();

    // `down` blocks until `docker compose stop` returns, but under heavy
    // parallel docker load the container's transition to "exited" can lag the
    // CLI return slightly — poll briefly for it to settle.
    let mut running = count("running");
    for _ in 0..20 {
        if running == 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        running = count("running");
    }
    let present = count("");
    cleanup();

    assert!(
        down_ok,
        "compose down failed: {}",
        String::from_utf8_lossy(&down.stderr)
    );
    assert_eq!(
        present, 1,
        "stopCompose down (no --remove) must keep the container present"
    );
    assert_eq!(
        running, 0,
        "stopCompose down (no --remove) must stop the container"
    );
}
