//! End-to-end CLI test for override config and secrets functionality

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_cli_with_override_config_and_secrets() {
    let temp_dir = TempDir::new().unwrap();

    // Create base devcontainer.json
    let base_config = temp_dir.path().join("devcontainer.json");
    let base_content = r#"{
        "name": "base-container",
        "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
        "containerEnv": {
            "BASE_VAR": "base-value",
            "SECRET_KEY": "${localEnv:SECRET_KEY}"
        }
    }"#;
    fs::write(&base_config, base_content).unwrap();

    // Create override config (must use valid devcontainer filename)
    let override_config = temp_dir.path().join(".devcontainer.json");
    let override_content = r#"{
        "name": "override-container",
        "containerEnv": {
            "OVERRIDE_VAR": "override-value",
            "API_TOKEN": "${localEnv:API_TOKEN}"
        }
    }"#;
    fs::write(&override_config, override_content).unwrap();

    // Create secrets file
    let secrets_file = temp_dir.path().join("secrets.env");
    let secrets_content = r#"
# Test secrets
SECRET_KEY=my-secret-key
API_TOKEN=abc123xyz
"#;
    fs::write(&secrets_file, secrets_content).unwrap();

    // Run read-configuration with override config and secrets
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(&temp_dir).args([
        "--config",
        base_config.to_str().unwrap(),
        "--workspace-folder",
        temp_dir.path().to_str().unwrap(),
        "--override-config",
        override_config.to_str().unwrap(),
        "--secrets-file",
        secrets_file.to_str().unwrap(),
        "read-configuration",
    ]);

    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    // Verify the override config took precedence
    assert!(stdout.contains("\"name\":\"override-container\""));

    // Verify base config fields are still present
    assert!(stdout.contains("mcr.microsoft.com/devcontainers/base:ubuntu"));

    // Verify environment variables are merged
    assert!(stdout.contains("BASE_VAR"));
    assert!(stdout.contains("OVERRIDE_VAR"));

    // Verify secrets were substituted
    assert!(stdout.contains("my-secret-key"));
    assert!(stdout.contains("abc123xyz"));
}

#[test]
fn test_cli_with_multiple_secrets_files() {
    let temp_dir = TempDir::new().unwrap();

    // Create base devcontainer.json
    let base_config = temp_dir.path().join("devcontainer.json");
    let base_content = r#"{
        "name": "test-container",
        "image": "ubuntu:latest",
        "containerEnv": {
            "KEY1": "${localEnv:KEY1}",
            "KEY2": "${localEnv:KEY2}",
            "KEY3": "${localEnv:KEY3}"
        }
    }"#;
    fs::write(&base_config, base_content).unwrap();

    // Create first secrets file
    let secrets1 = temp_dir.path().join("secrets1.env");
    fs::write(&secrets1, "KEY1=value1\nKEY2=value2\n").unwrap();

    // Create second secrets file (should override KEY2)
    let secrets2 = temp_dir.path().join("secrets2.env");
    fs::write(&secrets2, "KEY2=new_value2\nKEY3=value3\n").unwrap();

    // Run with multiple secrets files
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(&temp_dir).args([
        "--config",
        base_config.to_str().unwrap(),
        "--workspace-folder",
        temp_dir.path().to_str().unwrap(),
        "--secrets-file",
        secrets1.to_str().unwrap(),
        "--secrets-file",
        secrets2.to_str().unwrap(),
        "read-configuration",
    ]);

    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    // Verify all keys are present with later files winning conflicts
    assert!(stdout.contains("value1")); // KEY1 from first file
    assert!(stdout.contains("new_value2")); // KEY2 from second file (wins)
    assert!(stdout.contains("value3")); // KEY3 from second file
    assert!(!stdout.contains("\"KEY2\": \"value2\"")); // Old value should not be present
}

#[test]
fn test_cli_missing_secrets_file_continues() {
    let temp_dir = TempDir::new().unwrap();

    // Create base devcontainer.json
    let base_config = temp_dir.path().join("devcontainer.json");
    let base_content = r#"{
        "name": "test-container",
        "image": "ubuntu:latest"
    }"#;
    fs::write(&base_config, base_content).unwrap();

    // Reference non-existent secrets file
    let missing_secrets = temp_dir.path().join("missing.env");

    // Should not fail even with missing secrets file
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(&temp_dir).args([
        "--config",
        base_config.to_str().unwrap(),
        "--workspace-folder",
        temp_dir.path().to_str().unwrap(),
        "--secrets-file",
        missing_secrets.to_str().unwrap(),
        "read-configuration",
    ]);

    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    // Should still output the config
    assert!(stdout.contains("test-container"));
}
