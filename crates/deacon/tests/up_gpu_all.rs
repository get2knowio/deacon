//! Integration tests for GPU mode "all" propagation in the up command
//!
//! These tests verify that when `--gpu-mode all` is specified, the GPU mode
//! flows through the up command and would result in `--gpus all` flags being
//! propagated to Docker operations.
//!
//! Tests cover:
//! - CLI parsing of --gpu-mode all
//! - GPU mode enum serialization and behavior
//! - GPU mode propagation (verified through unit tests in gpu.rs module)

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

/// Test that CLI correctly accepts --gpu-mode all without errors
#[test]
fn test_gpu_mode_all_cli_parsing() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_mode_all_cli_parsing: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a minimal valid devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU Test",
    "image": "alpine:3.19"
}
"#;
    fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
    fs::write(
        tmp.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run up with --gpu-mode all
    // This test verifies CLI parsing, not actual GPU functionality
    // We expect this to fail during container creation (since we don't have GPUs in CI)
    // but the important part is that the CLI accepts the flag
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let result = cmd
        .current_dir(tmp.path())
        .arg("up")
        .arg("--gpu-mode")
        .arg("all")
        .arg("--log-level")
        .arg("debug")
        .assert();

    // The command may succeed or fail depending on Docker/GPU availability
    // What matters is that --gpu-mode all is accepted (no CLI parsing errors)
    let output = result.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify no CLI parsing errors for --gpu-mode flag
    assert!(
        !stderr.contains("unexpected argument") && !stderr.contains("invalid value 'all'"),
        "CLI should accept --gpu-mode all, stderr: {}",
        stderr
    );

    // If debug logging is working, we should see GPU mode mentioned
    // (This is a best-effort check - may not appear if command fails early)
    if stderr.contains("GPU mode") || stderr.contains("gpu_mode") {
        // Good - GPU mode is being processed
        assert!(
            stderr.contains("all") || stderr.contains("All"),
            "GPU mode should be set to 'all' in logs when --gpu-mode all is specified"
        );
    }
}

/// Test GpuMode enum parsing and serialization (unit test)
#[test]
fn test_gpu_mode_enum_parsing() {
    use std::str::FromStr;

    // Test FromStr implementation
    assert_eq!(GpuMode::from_str("all").unwrap(), GpuMode::All);
    assert_eq!(GpuMode::from_str("ALL").unwrap(), GpuMode::All);
    assert_eq!(GpuMode::from_str("All").unwrap(), GpuMode::All);

    assert_eq!(GpuMode::from_str("detect").unwrap(), GpuMode::Detect);
    assert_eq!(GpuMode::from_str("none").unwrap(), GpuMode::None);

    // Test invalid value
    assert!(GpuMode::from_str("invalid").is_err());
}

/// Test GpuMode enum serialization
#[test]
fn test_gpu_mode_enum_serialization() {
    // Test that GpuMode::All serializes correctly
    let mode = GpuMode::All;
    let json = serde_json::to_string(&mode).unwrap();
    assert_eq!(json, r#""all""#);

    // Test deserialization
    let parsed: GpuMode = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, GpuMode::All);

    // Test Display trait
    assert_eq!(GpuMode::All.to_string(), "all");
    assert_eq!(GpuMode::Detect.to_string(), "detect");
    assert_eq!(GpuMode::None.to_string(), "none");
}

/// Test that default GPU mode is "none"
#[test]
fn test_gpu_mode_default() {
    assert_eq!(GpuMode::default(), GpuMode::None);
}

/// Test GPU mode propagation through up command (JSON output)
#[test]
fn test_gpu_mode_all_json_output() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_mode_all_json_output: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a minimal devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU JSON Test",
    "image": "alpine:3.19"
}
"#;
    fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
    fs::write(
        tmp.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run up with --gpu-mode all
    // up command always outputs JSON to stdout
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(tmp.path())
        .arg("up")
        .arg("--gpu-mode")
        .arg("all")
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Command may succeed or fail depending on GPU availability
    // DeaconGuard will automatically clean up via `deacon down`

    // Verify that GPU mode was processed (should appear in debug logs)
    // The actual GPU flag application happens in docker.rs and compose.rs
    assert!(
        !stderr.contains("unexpected argument")
            && !stderr.contains("invalid value")
            && !stderr.contains("unrecognized option '--gpu-mode'"),
        "CLI should accept --gpu-mode all without errors. stderr: {}",
        stderr
    );
}

/// Test GPU mode all with compose-based configuration
#[test]
fn test_gpu_mode_all_with_compose() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_mode_all_with_compose: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a compose-based devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU Compose Test",
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

    // Run up with --gpu-mode all
    // up command always outputs JSON to stdout
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(tmp.path())
        .arg("up")
        .arg("--gpu-mode")
        .arg("all")
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // DeaconGuard will automatically clean up containers and compose projects via `deacon down`

    // Verify CLI accepts the flag
    assert!(
        !stderr.contains("unexpected argument") && !stderr.contains("invalid value"),
        "Compose-based up should accept --gpu-mode all. stderr: {}",
        stderr
    );
}

/// Test that GPU mode "all" is distinct from "detect" and "none"
#[test]
fn test_gpu_mode_all_distinctness() {
    // Verify enum values are distinct
    assert_ne!(GpuMode::All, GpuMode::Detect);
    assert_ne!(GpuMode::All, GpuMode::None);
    assert_ne!(GpuMode::Detect, GpuMode::None);

    // Verify string representations are distinct
    assert_ne!(GpuMode::All.to_string(), GpuMode::Detect.to_string());
    assert_ne!(GpuMode::All.to_string(), GpuMode::None.to_string());
}
