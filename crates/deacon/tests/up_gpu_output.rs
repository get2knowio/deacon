//! Integration tests for GPU mode output contracts in the up command
//!
//! These tests verify the output contracts specified in specs/001-gpu-modes/spec.md FR-007:
//! - Warnings and logs appear on stderr in text mode
//! - JSON mode preserves stdout for JSON and routes warnings to stderr
//! - GPU mode "none" produces no GPU-related output
//! - GPU mode "detect" emits warnings on stderr when no GPU is found
//! - GPU mode "all" logs info about GPU usage to appropriate channel
//!
//! Tests cover:
//! - Stderr/stdout separation for warnings in text mode
//! - JSON output integrity when GPU modes are used
//! - Silent behavior of "none" mode
//! - Warning emission in "detect" mode on non-GPU hosts

use assert_cmd::Command;
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

/// Test that GPU mode "detect" warnings appear on stderr, not stdout
///
/// This test verifies FR-007: "warnings/logs appear on stderr in text mode"
#[test]
fn test_gpu_warning_goes_to_stderr() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_warning_goes_to_stderr: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a minimal devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU Warning Test",
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
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // On most CI/test environments without GPUs, detect mode should emit a warning
    // The warning should be on stderr, not stdout
    if stderr.contains("GPU") {
        // If there's GPU-related output, it must be on stderr
        assert!(
            stderr.contains("GPU mode 'detect'") || stderr.contains("GPU runtime"),
            "GPU detection output should be on stderr, stderr: {}",
            stderr
        );
    }

    // Stdout should NOT contain warning text (it may contain JSON or be empty)
    assert!(
        !stdout.contains("GPU mode 'detect' specified but no GPU runtime found"),
        "GPU warnings should be on stderr, not stdout. stdout: {}",
        stdout
    );
}

/// Test that GPU mode "none" produces no GPU-related output
///
/// This test verifies FR-006: "system MUST avoid emitting GPU-related warnings"
#[test]
fn test_gpu_none_silent_output() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_none_silent_output: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a minimal devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU None Silent Test",
    "image": "alpine:3.19"
}
"#;
    fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
    fs::write(
        tmp.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run up with --gpu-mode none (explicit)
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(tmp.path())
        .arg("up")
        .arg("--gpu-mode")
        .arg("none")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify no GPU-related warnings or messages in either stream
    assert!(
        !stdout.contains("GPU") && !stderr.contains("GPU"),
        "GPU mode 'none' should produce no GPU-related output. stdout: {}, stderr: {}",
        stdout,
        stderr
    );
}

/// Test that GPU mode "none" (default) produces no GPU-related output
///
/// This test verifies FR-009: default mode is "none" and FR-006: no warnings
#[test]
fn test_gpu_default_silent_output() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_default_silent_output: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a minimal devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU Default Silent Test",
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
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify no GPU-related warnings or messages in either stream
    assert!(
        !stdout.contains("GPU") && !stderr.contains("GPU"),
        "Default GPU mode (none) should produce no GPU-related output. stdout: {}, stderr: {}",
        stdout,
        stderr
    );
}

/// Test that JSON output mode preserves stdout structure with GPU modes
///
/// This test verifies FR-007: "JSON mode preserves stdout for JSON and routes warnings to stderr"
#[test]
fn test_gpu_json_output_preserved() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_json_output_preserved: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a minimal devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU JSON Output Test",
    "image": "alpine:3.19"
}
"#;
    fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
    fs::write(
        tmp.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run up with --gpu-mode detect (most likely to produce warnings on CI)
    // The up command outputs JSON to stdout by default
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(tmp.path())
        .arg("up")
        .arg("--gpu-mode")
        .arg("detect")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // If the command succeeded, stdout should contain valid JSON
    if output.status.success() {
        // Stdout should be parseable as JSON
        let parse_result: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
        assert!(
            parse_result.is_ok(),
            "Stdout should contain valid JSON even with GPU mode detect. stdout: {}, parse error: {:?}",
            stdout,
            parse_result.err()
        );

        // Any GPU warnings should be on stderr, not in the JSON output
        if stderr.contains("GPU") {
            assert!(
                !stdout.contains("GPU mode 'detect' specified but no GPU runtime found"),
                "GPU warnings should not appear in JSON stdout. stdout: {}",
                stdout
            );
        }
    }
}

/// Test that GPU mode "detect" with compose config emits warnings on stderr
///
/// This test verifies FR-007 and FR-008: warnings on stderr, consistent across compose
#[test]
fn test_gpu_detect_compose_stderr() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_detect_compose_stderr: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a compose-based devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU Detect Compose Stderr Test",
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
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // GPU-related output should be on stderr, not stdout
    if stderr.contains("GPU") {
        assert!(
            stderr.contains("GPU mode 'detect'") || stderr.contains("GPU runtime"),
            "GPU detection output should be on stderr for compose configs, stderr: {}",
            stderr
        );
    }

    // Stdout should NOT contain warning text
    assert!(
        !stdout.contains("GPU mode 'detect' specified but no GPU runtime found"),
        "GPU warnings should be on stderr for compose configs, not stdout. stdout: {}",
        stdout
    );
}

/// Test that GPU mode "all" info logging goes to stderr, not stdout
///
/// This test verifies FR-007: info logging about GPU usage goes to stderr
#[test]
fn test_gpu_all_info_to_stderr() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_all_info_to_stderr: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a minimal devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU All Info Test",
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
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(tmp.path())
        .arg("up")
        .arg("--gpu-mode")
        .arg("all")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Stdout should not contain GPU mode configuration details
    // (any GPU info should be on stderr as logs)
    assert!(
        !stdout.contains("GPU mode 'all'") && !stdout.contains("--gpus all"),
        "GPU mode info should be logged to stderr, not stdout. stdout: {}",
        stdout
    );
}

/// Test stderr/stdout separation with all three GPU modes
///
/// This comprehensive test verifies that all GPU modes respect the output contract
#[test]
fn test_gpu_modes_output_separation() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_modes_output_separation: Docker not available");
        return;
    }

    for mode in &["all", "detect", "none"] {
        let tmp = TempDir::new().unwrap();
        let _guard = DeaconGuard::new(tmp.path());

        let devcontainer_config = format!(
            r#"{{
    "name": "GPU Mode {} Test",
    "image": "alpine:3.19"
}}
"#,
            mode
        );
        fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
        fs::write(
            tmp.path().join(".devcontainer/devcontainer.json"),
            devcontainer_config,
        )
        .unwrap();

        let mut cmd = Command::cargo_bin("deacon").unwrap();
        let output = cmd
            .current_dir(tmp.path())
            .arg("up")
            .arg("--gpu-mode")
            .arg(mode)
            .output()
            .expect("Failed to execute command");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // If the command succeeded, stdout should be either empty or valid JSON
        if output.status.success() && !stdout.trim().is_empty() {
            let parse_result: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
            assert!(
                parse_result.is_ok(),
                "Stdout should be valid JSON for mode '{}'. stdout: {}, error: {:?}",
                mode,
                stdout,
                parse_result.err()
            );
        }

        // For "none" mode, there should be no GPU mentions anywhere
        if *mode == "none" {
            assert!(
                !stdout.contains("GPU") && !stderr.contains("GPU"),
                "Mode 'none' should have no GPU-related output. stdout: {}, stderr: {}",
                stdout,
                stderr
            );
        }

        // Any GPU-related warnings or logs should be on stderr only
        if stdout.contains("GPU") {
            // This should not happen - warnings/logs belong on stderr
            panic!(
                "GPU-related output found on stdout for mode '{}'. stdout: {}",
                mode, stdout
            );
        }
    }
}
