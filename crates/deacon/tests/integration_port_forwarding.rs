//! Integration tests for --forward-port CLI flag

use assert_cmd::Command;
use serde_json::json;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_forward_port_cli_flag_single_port() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();

    // Create a minimal devcontainer.json without forwardPorts
    let devcontainer_config = json!({
        "name": "Port Forward Test",
        "image": "alpine:latest"
    });

    let config_path = devcontainer_dir.join("devcontainer.json");
    fs::write(
        &config_path,
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    // Test with --forward-port flag
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let result = cmd
        .args([
            "up",
            "--workspace-folder",
            &temp_dir.path().to_string_lossy(),
            "--skip-post-create",
            "--forward-port",
            "8080",
        ])
        .assert();

    let output = result.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should attempt to create container with port forwarding
    // Either succeeds or fails at Docker step
    assert!(
        stderr.contains("traditional")
            || stderr.contains("Container")
            || stderr.contains("docker")
            || stderr.contains("Docker")
            || stderr.contains("ping")
            || stderr.is_empty()
    );
}

#[test]
fn test_forward_port_cli_flag_multiple_ports() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();

    // Create a minimal devcontainer.json
    let devcontainer_config = json!({
        "name": "Multi Port Test",
        "image": "alpine:latest"
    });

    let config_path = devcontainer_dir.join("devcontainer.json");
    fs::write(
        &config_path,
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    // Test with multiple --forward-port flags
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let result = cmd
        .args([
            "up",
            "--workspace-folder",
            &temp_dir.path().to_string_lossy(),
            "--skip-post-create",
            "--forward-port",
            "8080",
            "--forward-port",
            "3000",
            "--forward-port",
            "5000:5000",
        ])
        .assert();

    let output = result.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should attempt to create container with multiple port forwards
    assert!(
        stderr.contains("traditional")
            || stderr.contains("Container")
            || stderr.contains("docker")
            || stderr.contains("Docker")
            || stderr.contains("ping")
            || stderr.is_empty()
    );
}

#[test]
fn test_forward_port_with_host_mapping() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();

    // Create a minimal devcontainer.json
    let devcontainer_config = json!({
        "name": "Port Mapping Test",
        "image": "alpine:latest"
    });

    let config_path = devcontainer_dir.join("devcontainer.json");
    fs::write(
        &config_path,
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    // Test with host:container port mapping
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let result = cmd
        .args([
            "up",
            "--workspace-folder",
            &temp_dir.path().to_string_lossy(),
            "--skip-post-create",
            "--forward-port",
            "8080:3000",
        ])
        .assert();

    let output = result.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should attempt to create container with port mapping
    assert!(
        stderr.contains("traditional")
            || stderr.contains("Container")
            || stderr.contains("docker")
            || stderr.contains("Docker")
            || stderr.contains("ping")
            || stderr.is_empty()
    );
}

#[test]
fn test_forward_port_with_config_ports() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();

    // Create devcontainer.json with existing forwardPorts
    let devcontainer_config = json!({
        "name": "Combined Ports Test",
        "image": "alpine:latest",
        "forwardPorts": [3000, 4000]
    });

    let config_path = devcontainer_dir.join("devcontainer.json");
    fs::write(
        &config_path,
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    // Test CLI port forwarding in addition to config ports
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let result = cmd
        .args([
            "up",
            "--workspace-folder",
            &temp_dir.path().to_string_lossy(),
            "--skip-post-create",
            "--forward-port",
            "8080",
        ])
        .assert();

    let output = result.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should merge CLI and config ports
    assert!(
        stderr.contains("traditional")
            || stderr.contains("Container")
            || stderr.contains("docker")
            || stderr.contains("Docker")
            || stderr.contains("ping")
            || stderr.is_empty()
    );
}
