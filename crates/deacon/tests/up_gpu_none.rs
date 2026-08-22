//! Integration tests for GPU mode "none" propagation in the up command
//!
//! These tests verify that when `--gpu-mode none` is specified (or when no mode
//! is specified and the default is used), the GPU mode flows through the up
//! command without GPU flags or GPU-related warnings.
//!
//! Tests cover:
//! - CLI parsing of --gpu-mode none
//! - GPU mode enum default value (`GpuMode::default()` is `None`; the CLI FLAG
//!   defaults to `detect`, which is a different thing — see
//!   `test_gpu_mode_default_behavior`)
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

/// Does stderr carry one of deacon's own GPU diagnostics?
///
/// Deliberately NOT `stderr.contains("GPU") && stderr.contains("warning")`, which is
/// what these tests used to ask. Every fixture in this file names its configuration
/// `"GPU …"`, and at `--log-level debug` deacon echoes that name back
/// (`Loaded configuration: Some("GPU None Compose Test")`) — so the first half is
/// satisfied by the fixture itself, and the second by any unrelated line anywhere in a
/// debug-level log (Compose's obsolete-`version:` notice, for one). The pair was a
/// false positive waiting for a log line.
///
/// It never fired because, until #610, none of these tests passed
/// `--workspace-folder` — they set `current_dir` and `up` rejected the invocation at
/// argument validation, so no `up` ever ran and no log was ever produced. With `up`
/// defaulting to the current directory they run for real, and the co-occurrence
/// tripped on Windows.
///
/// These are the three sentences `up` actually emits about GPUs
/// (`commands::up::execute_up`). None of them may appear for mode `none`, which does
/// not probe at all. The CLI DEFAULT is `detect`, not `none`, and does probe — so
/// `test_gpu_mode_default_behavior` deliberately does not use this helper and asserts
/// the narrower claim instead; see its own comment.
fn has_gpu_diagnostic(stderr: &str) -> bool {
    stderr.contains("GPU detection failed")
        || stderr.contains("Proceeding without GPU acceleration")
        || stderr.contains("proceeding with GPU acceleration")
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
        !has_gpu_diagnostic(&stderr),
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

/// Test that the `GpuMode` ENUM defaults to `None`.
///
/// This is the library default, for callers constructing `UpArgs` directly. The CLI
/// flag defaults to `detect` instead (`default_value = "detect"`); the two are
/// deliberately different, and `test_gpu_mode_default_behavior` below covers the CLI
/// half.
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

/// Test default behavior when `--gpu-mode` is not specified.
///
/// The CLI default is `detect`, NOT `none` — `#[arg(default_value = "detect")]` on
/// `--gpu-availability`/`--gpu-mode`. That is distinct from `GpuMode::default()`,
/// which IS `None` and is what `test_gpu_mode_none_is_default` above asserts; the
/// enum default serves library callers building `UpArgs` directly, and the two are
/// deliberately different. This test used to claim the CLI defaulted to `none` and
/// assert that no GPU diagnostic was emitted — a premise it could never check,
/// because until #610 it passed no `--workspace-folder` and `up` rejected the
/// invocation at argument validation before reaching the GPU probe.
///
/// What the default actually does on a GPU-less host, and what is asserted here:
/// probe, find nothing, say so ONCE at info level, and continue without GPU
/// support rather than failing. A detection *failure* — the probe itself
/// malfunctioning — is a different, warn-level sentence and must not appear.
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

    // Run up WITHOUT --gpu-mode flag (the CLI default is "detect")
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(tmp.path())
        .arg("up")
        .arg("--log-level")
        .arg("debug")
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // The default probes. On a host without a GPU that is a clean negative result,
    // reported once and informationally, not a malfunction.
    assert!(
        !stderr.contains("GPU detection failed"),
        "The default GPU mode must not report a detection FAILURE on a GPU-less host \
         (a clean negative is not an error), stderr: {}",
        stderr
    );

    // And it must never silently attach GPU flags it could not verify.
    assert!(
        !stderr.contains("proceeding with GPU acceleration"),
        "The default GPU mode must not claim GPU acceleration when no runtime was \
         found, stderr: {}",
        stderr
    );

    // Emitted exactly once when the probe comes back empty, so a GPU-less host sees a
    // single line rather than one per container operation.
    let announcements = stderr
        .matches("Proceeding without GPU acceleration")
        .count();
    assert!(
        announcements <= 1,
        "The default GPU mode must announce the absent GPU at most once, saw {}. \
         stderr: {}",
        announcements,
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
    let has_gpu_warning = has_gpu_diagnostic(&stderr);
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
    let has_gpu_warning = has_gpu_diagnostic(&stderr);
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
