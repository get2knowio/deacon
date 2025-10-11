//! Integration tests for initializeCommand execution in `deacon up`
//!
//! These tests verify that initializeCommand runs on the host before container creation.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

/// Test that initializeCommand runs on the host before container creation
#[test]
fn test_initialize_command_creates_host_marker() {
    let temp_dir = TempDir::new().unwrap();

    // Create a marker file path on the host
    let marker_path = temp_dir.path().join("init_marker.txt");

    // Create a devcontainer.json with initializeCommand that creates a file on the host
    let devcontainer_config = format!(
        r#"{{
    "name": "Initialize Command Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "initializeCommand": "echo 'initialized' > {}",
    "postCreateCommand": "echo 'Container created'"
}}"#,
        marker_path.display()
    );

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run deacon up
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    // Check that the command succeeded
    assert!(
        up_output.status.success(),
        "deacon up failed: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );

    // Verify that the marker file was created on the host
    assert!(
        marker_path.exists(),
        "initializeCommand marker file was not created on host at {}",
        marker_path.display()
    );

    // Verify the content
    let marker_content = fs::read_to_string(&marker_path).unwrap();
    assert!(
        marker_content.contains("initialized"),
        "Marker file content incorrect: {}",
        marker_content
    );
}

/// Test that initializeCommand with array syntax works
#[test]
fn test_initialize_command_array_syntax() {
    let temp_dir = TempDir::new().unwrap();

    let marker1_path = temp_dir.path().join("init_marker1.txt");
    let marker2_path = temp_dir.path().join("init_marker2.txt");

    // Create a devcontainer.json with initializeCommand as array
    let devcontainer_config = format!(
        r#"{{
    "name": "Initialize Command Array Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "initializeCommand": [
        "echo 'first' > {}",
        "echo 'second' > {}"
    ],
    "postCreateCommand": "echo 'Container created'"
}}"#,
        marker1_path.display(),
        marker2_path.display()
    );

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run deacon up
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    // Check that the command succeeded
    assert!(
        up_output.status.success(),
        "deacon up failed: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );

    // Verify both marker files were created
    assert!(
        marker1_path.exists(),
        "First marker file was not created on host"
    );
    assert!(
        marker2_path.exists(),
        "Second marker file was not created on host"
    );

    // Verify contents
    let marker1_content = fs::read_to_string(&marker1_path).unwrap();
    assert!(marker1_content.contains("first"));

    let marker2_content = fs::read_to_string(&marker2_path).unwrap();
    assert!(marker2_content.contains("second"));
}

/// Test that initializeCommand runs before container creation
#[test]
fn test_initialize_command_runs_before_container() {
    let temp_dir = TempDir::new().unwrap();

    let marker_path = temp_dir.path().join("init_order.txt");

    // Create a devcontainer.json where initializeCommand creates a host file
    // and postCreateCommand attempts to read it (should fail since it runs in container)
    let devcontainer_config = format!(
        r#"{{
    "name": "Initialize Order Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "initializeCommand": "echo 'host-init' > {}",
    "postCreateCommand": "echo 'Container created after host init'"
}}"#,
        marker_path.display()
    );

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run deacon up
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    // Check that the command succeeded
    assert!(
        up_output.status.success(),
        "deacon up failed: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );

    // Verify the marker file exists on the host (created before container)
    assert!(
        marker_path.exists(),
        "initializeCommand marker file was not created before container"
    );
}

/// Test that compose configurations also run initializeCommand
#[test]
fn test_initialize_command_with_compose() {
    let temp_dir = TempDir::new().unwrap();

    let marker_path = temp_dir.path().join("compose_init_marker.txt");

    // Create a simple docker-compose.yml
    let compose_config = r#"
version: '3.8'
services:
  app:
    image: alpine:3.19
    command: sleep infinity
"#;

    fs::write(temp_dir.path().join("docker-compose.yml"), compose_config).unwrap();

    // Create a devcontainer.json with compose and initializeCommand
    let devcontainer_config = format!(
        r#"{{
    "name": "Compose Initialize Test",
    "dockerComposeFile": "docker-compose.yml",
    "service": "app",
    "workspaceFolder": "/workspace",
    "initializeCommand": "echo 'compose-init' > {}",
    "postCreateCommand": "echo 'Compose container created'"
}}"#,
        marker_path.display()
    );

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run deacon up
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    // Check that the command succeeded
    assert!(
        up_output.status.success(),
        "deacon up with compose failed: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );

    // Verify the marker file was created on the host
    assert!(
        marker_path.exists(),
        "initializeCommand marker file was not created for compose config"
    );

    // Verify content
    let marker_content = fs::read_to_string(&marker_path).unwrap();
    assert!(
        marker_content.contains("compose-init"),
        "Marker file content incorrect: {}",
        marker_content
    );
}
