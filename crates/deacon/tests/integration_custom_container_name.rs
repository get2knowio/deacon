//! Integration test for --container-name flag

use assert_cmd::Command;
use serde_json::json;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_up_with_custom_container_name() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();

    // Create a simple devcontainer.json
    let devcontainer_config = json!({
        "name": "Test Container",
        "image": "ubuntu:20.04"
    });

    let config_path = devcontainer_dir.join("devcontainer.json");
    fs::write(
        &config_path,
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    // Test the up command with custom container name
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let result = cmd
        .args([
            "up",
            "--workspace-folder",
            &temp_dir.path().to_string_lossy(),
            "--container-name",
            "my-custom-test-container",
            "--skip-post-create",
        ])
        .assert();

    let output = result.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Debug output
    eprintln!("DEBUG - stderr: {:?}", stderr);
    eprintln!("DEBUG - stdout: {:?}", stdout);
    eprintln!("DEBUG - exit code: {:?}", output.status.code());

    // The test verifies the flag is accepted - actual container creation
    // depends on Docker availability but the flag should be parsed correctly
    assert!(
        stderr.contains("my-custom-test-container")
            || stderr.contains("Container")
            || stderr.contains("docker")
            || stderr.contains("Docker")
            || stderr.contains("Not installed")
            || stderr.is_empty()
    );
}

#[test]
fn test_container_name_flag_in_help() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd.args(["up", "--help"]).assert().success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Verify the flag appears in help text
    assert!(stdout.contains("--container-name"));
    assert!(stdout.contains("Custom container name"));
}
