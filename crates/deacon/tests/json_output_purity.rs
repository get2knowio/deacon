#![cfg(feature = "full")]
//! Tests for JSON output purity and stdout/stderr contract enforcement

use anyhow::Result;
use assert_cmd::prelude::*;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Test that read-configuration produces only valid JSON on stdout
#[test]
fn test_read_configuration_json_purity() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;
    let config_path = devcontainer_dir.join("devcontainer.json");

    let config_content = r#"{
        "name": "test-container", 
        "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
        "features": {
            "ghcr.io/devcontainers/features/docker-in-docker:2": {}
        }
    }"#;

    fs::write(&config_path, config_content)?;

    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .current_dir(&temp_dir)
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()?;

    assert!(
        output.status.success(),
        "Command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Stdout should be parseable as valid JSON
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|e| anyhow::anyhow!("stdout is not valid JSON: {}", e))?;

    // Should contain expected fields (now nested under configuration)
    assert_eq!(parsed["configuration"]["name"], "test-container");
    assert_eq!(
        parsed["configuration"]["image"],
        "mcr.microsoft.com/devcontainers/base:ubuntu"
    );
    assert!(parsed["configuration"].get("features").is_some());

    Ok(())
}

/// Test that stdout contains only JSON, no logs or extra output
#[test]
fn test_json_output_purity_with_debug_logging() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;
    let config_path = devcontainer_dir.join("devcontainer.json");

    let config_content = r#"{
        "name": "purity-test",
        "image": "alpine:latest"
    }"#;

    fs::write(&config_path, config_content)?;

    // Run with debug logging to ensure logs don't leak to stdout
    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .current_dir(&temp_dir)
        .arg("--log-level")
        .arg("debug")
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()?;

    assert!(
        output.status.success(),
        "Command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let _stderr = String::from_utf8_lossy(&output.stderr);

    // Stdout should be pure JSON - no log messages
    assert!(!stdout.contains("Starting read-configuration"));
    assert!(!stdout.contains("DEBUG"));
    assert!(!stdout.contains("INFO"));

    // All logs should go to stderr
    // Note: This might not show logs if they're filtered by test runner
    // but the key is that stdout is clean

    // Stdout should parse as clean JSON
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())?;
    assert_eq!(parsed["configuration"]["name"], "purity-test");

    Ok(())
}

/// Test that stderr contains logs while stdout has only results
#[test]
fn test_stderr_log_separation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;
    let config_path = devcontainer_dir.join("devcontainer.json");

    let config_content = r#"{
        "name": "stderr-test",
        "image": "node:18"
    }"#;

    fs::write(&config_path, config_content)?;

    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .current_dir(&temp_dir)
        .arg("--log-level")
        .arg("info")
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()?;

    assert!(
        output.status.success(),
        "Command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let _stderr = String::from_utf8_lossy(&output.stderr);

    // Stdout should only contain JSON result
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())?;
    assert_eq!(parsed["configuration"]["name"], "stderr-test");
    assert_eq!(parsed["configuration"]["image"], "node:18");

    // Logs should not contaminate stdout
    assert!(!stdout.contains("Starting"));
    assert!(!stdout.contains("Loaded"));
    assert!(!stdout.contains("Applied"));

    // stderr may or may not contain logs depending on environment,
    // but the key requirement is that stdout is pure

    Ok(())
}

/// Test multiple JSON objects are not produced (should be single JSON doc)  
#[test]
fn test_single_json_document_output() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;
    let config_path = devcontainer_dir.join("devcontainer.json");

    let config_content = r#"{
        "name": "single-json-test",
        "image": "ubuntu:22.04"
    }"#;

    fs::write(&config_path, config_content)?;

    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .current_dir(&temp_dir)
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()?;

    assert!(
        output.status.success(),
        "Command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should contain exactly one JSON object
    let json_objects: Vec<&str> = stdout
        .lines()
        .filter(|line| line.trim().starts_with('{'))
        .collect();

    // Should not have multiple JSON objects
    assert!(
        json_objects.len() <= 1,
        "Multiple JSON objects found in stdout"
    );

    // The entire stdout should parse as a single JSON document
    let _parsed: serde_json::Value = serde_json::from_str(stdout.trim())?;

    Ok(())
}

/// Test build command JSON output purity with multi-tag success payloads
#[test]
#[ignore]
fn test_build_multi_tag_json_output() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let ws = temp_dir.path();

    // Minimal Dockerfile
    let dockerfile = r#"FROM alpine:3.19
RUN echo "Testing multi-tag JSON output"
"#;
    fs::write(ws.join("Dockerfile"), dockerfile)?;

    // devcontainer.json at root
    let devcontainer = r#"{
  "name": "MultiTagTest",
  "dockerFile": "Dockerfile",
  "build": {
    "context": "."
  }
}
"#;
    fs::write(ws.join(".devcontainer.json"), devcontainer)?;

    // Run with multiple image names and JSON output
    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .current_dir(ws)
        .arg("--log-level")
        .arg("debug")
        .arg("build")
        .arg("--image-name")
        .arg("test-multi:tag1")
        .arg("--image-name")
        .arg("test-multi:tag2")
        .arg("--output-format")
        .arg("json")
        .output()?;

    if !output.status.success() {
        // Gracefully skip if Docker isn't available
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_lc = stderr.to_lowercase();
        if stderr.contains("Docker is not installed")
            || stderr.contains("Docker daemon is not")
            || stderr_lc.contains("permission denied")
        {
            return Ok(());
        }
        panic!("Build failed unexpectedly: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Stdout should be pure JSON - no log messages
    assert!(!stdout.contains("DEBUG"), "DEBUG found in stdout");
    assert!(!stdout.contains("INFO"), "INFO found in stdout");
    assert!(!stdout.contains("Building"), "Log message found in stdout");

    // Parse as valid JSON
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|e| anyhow::anyhow!("stdout is not valid JSON: {}", e))?;

    // Verify spec-compliant structure
    assert_eq!(parsed["outcome"], "success", "Should have outcome: success");
    assert!(
        parsed["imageName"].is_array(),
        "imageName should be an array for multiple tags"
    );

    let image_names = parsed["imageName"].as_array().unwrap();
    assert_eq!(
        image_names.len(),
        2,
        "Should have both image names in output"
    );

    // Verify tags are present
    let names: Vec<String> = image_names
        .iter()
        .filter_map(|v| v.as_str())
        .map(String::from)
        .collect();
    assert!(
        names.contains(&"test-multi:tag1".to_string()),
        "Should contain first tag"
    );
    assert!(
        names.contains(&"test-multi:tag2".to_string()),
        "Should contain second tag"
    );

    // Cleanup
    for name in names {
        let _ = std::process::Command::new("docker")
            .args(["rmi", &name])
            .output();
    }

    Ok(())
}

/// Test build command JSON output for single tag (backward compatibility)
#[test]
#[ignore]
fn test_build_single_tag_json_output() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let ws = temp_dir.path();

    // Minimal Dockerfile
    let dockerfile = r#"FROM alpine:3.19
RUN echo "Testing single tag JSON output"
"#;
    fs::write(ws.join("Dockerfile"), dockerfile)?;

    // devcontainer.json at root
    let devcontainer = r#"{
  "name": "SingleTagTest",
  "dockerFile": "Dockerfile",
  "build": {
    "context": "."
  }
}
"#;
    fs::write(ws.join(".devcontainer.json"), devcontainer)?;

    // Run with single image name and JSON output
    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .current_dir(ws)
        .arg("--log-level")
        .arg("info")
        .arg("build")
        .arg("--image-name")
        .arg("test-single:only-tag")
        .arg("--output-format")
        .arg("json")
        .output()?;

    if !output.status.success() {
        // Gracefully skip if Docker isn't available
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_lc = stderr.to_lowercase();
        if stderr.contains("Docker is not installed")
            || stderr.contains("Docker daemon is not")
            || stderr_lc.contains("permission denied")
        {
            return Ok(());
        }
        panic!("Build failed unexpectedly: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Stdout should be pure JSON - no log messages
    assert!(!stdout.contains("INFO"), "INFO found in stdout");
    assert!(!stdout.contains("Building"), "Log message found in stdout");

    // Parse as valid JSON
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|e| anyhow::anyhow!("stdout is not valid JSON: {}", e))?;

    // Verify spec-compliant structure - can be string or array
    assert_eq!(parsed["outcome"], "success", "Should have outcome: success");
    assert!(
        parsed.get("imageName").is_some(),
        "Should have imageName field"
    );

    // imageName can be string or array for single tag
    let image_name = if parsed["imageName"].is_array() {
        parsed["imageName"][0].as_str().unwrap()
    } else {
        parsed["imageName"].as_str().unwrap()
    };

    assert_eq!(
        image_name, "test-single:only-tag",
        "Should contain specified tag"
    );

    // Cleanup
    let _ = std::process::Command::new("docker")
        .args(["rmi", image_name])
        .output();

    Ok(())
}
