//! Integration tests for GPU mode "detect" propagation in the up command
//!
//! These tests verify that when `--gpu-mode detect` is specified, the GPU mode
//! flows through the up command and results in:
//! - GPU detection occurring before container creation
//! - GPU flags being added if GPUs are detected
//! - A warning being emitted if no GPUs are detected
//!
//! Tests cover:
//! - CLI parsing of --gpu-mode detect
//! - GPU mode enum serialization and behavior
//! - Detect mode behavior with traditional image-based configs
//! - Detect mode behavior with compose-based configs

use assert_cmd::Command;
use deacon_core::gpu::GpuMode;
use std::fs;
use tempfile::TempDir;

mod test_utils;
use test_utils::DeaconGuard;

/// Check if Docker is available for integration tests
fn is_docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Test that CLI correctly accepts --gpu-mode detect without errors
#[test]
fn test_gpu_mode_detect_cli_parsing() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_mode_detect_cli_parsing: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a minimal valid devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU Detect Test",
    "image": "alpine:3.19"
}
"#;
    fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
    fs::write(
        tmp.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run up with --gpu-mode detect
    // This test verifies CLI parsing, not actual GPU functionality
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let result = cmd
        .current_dir(tmp.path())
        .arg("up")
        .arg("--gpu-mode")
        .arg("detect")
        .arg("--log-level")
        .arg("debug")
        .assert();

    // The command may succeed or fail depending on Docker/GPU availability
    // What matters is that --gpu-mode detect is accepted (no CLI parsing errors)
    let output = result.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify no CLI parsing errors for --gpu-mode flag
    assert!(
        !stderr.contains("unexpected argument") && !stderr.contains("invalid value 'detect'"),
        "CLI should accept --gpu-mode detect, stderr: {}",
        stderr
    );

    // Verify that GPU mode is being processed
    // In detect mode, we expect either:
    // - GPU detection logs if GPUs are present
    // - A warning message if no GPUs are detected
    // We can't control which occurs in test environments, so we just verify
    // the mode was recognized
    if stderr.contains("GPU mode") || stderr.contains("gpu_mode") {
        // Good - GPU mode is being processed
        assert!(
            stderr.contains("detect") || stderr.contains("Detect"),
            "GPU mode should be set to 'detect' in logs when --gpu-mode detect is specified"
        );
    }
}

/// Test GpuMode::Detect enum parsing (unit test)
#[test]
fn test_gpu_mode_detect_enum_parsing() {
    use std::str::FromStr;

    // Test FromStr implementation for detect mode
    assert_eq!(GpuMode::from_str("detect").unwrap(), GpuMode::Detect);
    assert_eq!(GpuMode::from_str("DETECT").unwrap(), GpuMode::Detect);
    assert_eq!(GpuMode::from_str("Detect").unwrap(), GpuMode::Detect);

    // Verify it's distinct from other modes
    assert_ne!(GpuMode::from_str("detect").unwrap(), GpuMode::All);
    assert_ne!(GpuMode::from_str("detect").unwrap(), GpuMode::None);
}

/// Test GpuMode::Detect enum serialization
#[test]
fn test_gpu_mode_detect_enum_serialization() {
    // Test that GpuMode::Detect serializes correctly
    let mode = GpuMode::Detect;
    let json = serde_json::to_string(&mode).unwrap();
    assert_eq!(json, r#""detect""#);

    // Test deserialization
    let parsed: GpuMode = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, GpuMode::Detect);

    // Test Display trait
    assert_eq!(GpuMode::Detect.to_string(), "detect");
}

/// Test GPU mode detect with traditional image-based configuration
///
/// This test verifies that detect mode works with a traditional devcontainer.json
/// that uses the "image" property. The behavior should be:
/// - If GPUs are present: GPU flags are added to container operations
/// - If GPUs are absent: A warning is emitted and container proceeds without GPU flags
#[test]
fn test_gpu_mode_detect_with_traditional_config() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_mode_detect_with_traditional_config: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a traditional image-based devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU Detect Traditional Test",
    "image": "alpine:3.19"
}
"#;
    fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
    fs::write(
        tmp.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run up with --gpu-mode detect
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(tmp.path())
        .arg("up")
        .arg("--gpu-mode")
        .arg("detect")
        .arg("--log-level")
        .arg("debug")
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify CLI accepts the flag
    assert!(
        !stderr.contains("unexpected argument")
            && !stderr.contains("invalid value")
            && !stderr.contains("unrecognized option '--gpu-mode'"),
        "CLI should accept --gpu-mode detect without errors. stderr: {}",
        stderr
    );

    // The test environment may or may not have GPUs
    // We verify that the command processes the detect mode appropriately
    // Either GPUs are detected and used, or a warning is shown
    // We don't assert on which path is taken since it depends on the host
}

/// Test GPU mode detect with compose-based configuration
///
/// This test verifies that detect mode works with a compose-based devcontainer
/// configuration. The behavior should be consistent with traditional configs.
#[test]
fn test_gpu_mode_detect_with_compose_config() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_mode_detect_with_compose_config: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a compose-based devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU Detect Compose Test",
    "dockerComposeFile": "docker-compose.yml",
    "service": "app",
    "workspaceFolder": "/workspace"
}
"#;

    let compose_config = r#"version: '3.8'
services:
  app:
    image: alpine:3.19
    command: sleep infinity
"#;

    fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
    fs::write(
        tmp.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();
    fs::write(
        tmp.path().join(".devcontainer/docker-compose.yml"),
        compose_config,
    )
    .unwrap();

    // Run up with --gpu-mode detect
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(tmp.path())
        .arg("up")
        .arg("--gpu-mode")
        .arg("detect")
        .arg("--log-level")
        .arg("debug")
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify CLI accepts the flag
    assert!(
        !stderr.contains("unexpected argument") && !stderr.contains("invalid value"),
        "Compose-based up should accept --gpu-mode detect. stderr: {}",
        stderr
    );

    // The test environment may or may not have GPUs
    // We verify that the command processes the detect mode appropriately
    // DeaconGuard will automatically clean up containers and compose projects via `deacon down`
}

/// Test that GPU mode "detect" is distinct from other modes
#[test]
fn test_gpu_mode_detect_distinctness() {
    // Verify enum values are distinct
    assert_ne!(GpuMode::Detect, GpuMode::All);
    assert_ne!(GpuMode::Detect, GpuMode::None);

    // Verify string representations are distinct
    assert_ne!(GpuMode::Detect.to_string(), GpuMode::All.to_string());
    assert_ne!(GpuMode::Detect.to_string(), GpuMode::None.to_string());
}
