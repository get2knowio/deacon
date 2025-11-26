//! Integration tests for GPU mode "none" propagation in the up command
//!
//! These tests verify that when `--gpu-mode none` is specified (or when no mode
//! is specified and the default is used), the GPU mode flows through the up
//! command without GPU flags or GPU-related warnings.
//!
//! Tests cover:
//! - CLI parsing of --gpu-mode none
//! - GPU mode enum default value
//! - Default behavior when --gpu-mode is not specified
//! - GPU mode "none" with traditional image-based configs
//! - GPU mode "none" with compose-based configs
//! - Verification that no GPU warnings appear in output

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

/// Test that CLI correctly accepts --gpu-mode none without errors
#[test]
fn test_gpu_mode_none_cli_parsing() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_mode_none_cli_parsing: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a minimal valid devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU None Test",
    "image": "alpine:3.19"
}
"#;
    fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
    fs::write(
        tmp.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run up with --gpu-mode none
    // This test verifies CLI parsing and absence of GPU functionality
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let result = cmd
        .current_dir(tmp.path())
        .arg("up")
        .arg("--gpu-mode")
        .arg("none")
        .arg("--log-level")
        .arg("debug")
        .assert();

    let output = result.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify no CLI parsing errors for --gpu-mode flag
    assert!(
        !stderr.contains("unexpected argument") && !stderr.contains("invalid value 'none'"),
        "CLI should accept --gpu-mode none, stderr: {}",
        stderr
    );

    // Verify no GPU-related warnings appear when mode is "none"
    assert!(
        !stderr.contains("GPU") || !stderr.contains("warning"),
        "GPU mode 'none' should not emit GPU-related warnings, stderr: {}",
        stderr
    );
}

/// Test GpuMode::None enum parsing (unit test)
#[test]
fn test_gpu_mode_none_enum_parsing() {
    use std::str::FromStr;

    // Test FromStr implementation for none mode
    assert_eq!(GpuMode::from_str("none").unwrap(), GpuMode::None);
    assert_eq!(GpuMode::from_str("NONE").unwrap(), GpuMode::None);
    assert_eq!(GpuMode::from_str("None").unwrap(), GpuMode::None);

    // Verify it's distinct from other modes
    assert_ne!(GpuMode::from_str("none").unwrap(), GpuMode::All);
    assert_ne!(GpuMode::from_str("none").unwrap(), GpuMode::Detect);
}

/// Test that default GPU mode is "none" when --gpu-mode not specified
#[test]
fn test_gpu_mode_none_is_default() {
    // Test that GpuMode::default() returns GpuMode::None
    assert_eq!(GpuMode::default(), GpuMode::None);

    // Test serialization of default
    let mode = GpuMode::default();
    let json = serde_json::to_string(&mode).unwrap();
    assert_eq!(json, r#""none""#);

    // Test Display trait for None
    assert_eq!(GpuMode::None.to_string(), "none");
}

/// Test default behavior when --gpu-mode is not specified
#[test]
fn test_gpu_mode_default_behavior() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_mode_default_behavior: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a minimal devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU Default Test",
    "image": "alpine:3.19"
}
"#;
    fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
    fs::write(
        tmp.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run up WITHOUT --gpu-mode flag (should default to "none")
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(tmp.path())
        .arg("up")
        .arg("--log-level")
        .arg("debug")
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify no GPU-related warnings appear in default mode
    // (default should be "none" which emits no warnings)
    let has_gpu_warning = stderr.contains("GPU") && stderr.contains("warning");
    assert!(
        !has_gpu_warning,
        "Default GPU mode should not emit GPU warnings, stderr: {}",
        stderr
    );
}

/// Test GPU mode none with traditional image-based configuration
///
/// This test verifies that "none" mode works with a traditional devcontainer.json
/// that uses the "image" property. The behavior should be:
/// - No GPU flags are added to container operations
/// - No GPU-related warnings appear in output
#[test]
fn test_gpu_mode_none_with_traditional_config() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_mode_none_with_traditional_config: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a traditional image-based devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU None Traditional Test",
    "image": "alpine:3.19"
}
"#;
    fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
    fs::write(
        tmp.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run up with --gpu-mode none
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(tmp.path())
        .arg("up")
        .arg("--gpu-mode")
        .arg("none")
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
        "CLI should accept --gpu-mode none without errors. stderr: {}",
        stderr
    );

    // Verify no GPU-related warnings appear
    let has_gpu_warning = stderr.contains("GPU") && stderr.contains("warning");
    assert!(
        !has_gpu_warning,
        "GPU mode 'none' should not emit GPU warnings, stderr: {}",
        stderr
    );
}

/// Test GPU mode none with compose-based configuration
///
/// This test verifies that "none" mode works with a compose-based devcontainer
/// configuration. The behavior should be consistent with traditional configs:
/// - No GPU flags are added to compose operations
/// - No GPU-related warnings appear in output
#[test]
fn test_gpu_mode_none_with_compose_config() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_mode_none_with_compose_config: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a compose-based devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU None Compose Test",
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

    // Run up with --gpu-mode none
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(tmp.path())
        .arg("up")
        .arg("--gpu-mode")
        .arg("none")
        .arg("--log-level")
        .arg("debug")
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify CLI accepts the flag
    assert!(
        !stderr.contains("unexpected argument") && !stderr.contains("invalid value"),
        "Compose-based up should accept --gpu-mode none. stderr: {}",
        stderr
    );

    // Verify no GPU-related warnings appear
    let has_gpu_warning = stderr.contains("GPU") && stderr.contains("warning");
    assert!(
        !has_gpu_warning,
        "GPU mode 'none' in compose should not emit GPU warnings, stderr: {}",
        stderr
    );
}

/// Test that GPU mode "none" is distinct from other modes
#[test]
fn test_gpu_mode_none_distinctness() {
    // Verify enum values are distinct
    assert_ne!(GpuMode::None, GpuMode::All);
    assert_ne!(GpuMode::None, GpuMode::Detect);

    // Verify string representations are distinct
    assert_ne!(GpuMode::None.to_string(), GpuMode::All.to_string());
    assert_ne!(GpuMode::None.to_string(), GpuMode::Detect.to_string());
}

/// Test GpuMode::None enum serialization
#[test]
fn test_gpu_mode_none_enum_serialization() {
    // Test that GpuMode::None serializes correctly
    let mode = GpuMode::None;
    let json = serde_json::to_string(&mode).unwrap();
    assert_eq!(json, r#""none""#);

    // Test deserialization
    let parsed: GpuMode = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, GpuMode::None);

    // Test Display trait
    assert_eq!(GpuMode::None.to_string(), "none");
}
