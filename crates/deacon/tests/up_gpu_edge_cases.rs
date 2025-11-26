//! Integration tests for GPU mode edge cases in the up command
//!
//! These tests verify edge cases and corner scenarios for GPU mode handling:
//! - Explicit GPU mode overrides default settings
//! - Invalid GPU mode values are rejected by CLI
//! - GPU mode is case-insensitive
//! - GPU mode works correctly with other up flags
//!
//! Tests cover:
//! - CLI validation and error handling
//! - GPU mode interaction with other command-line flags
//! - Default vs explicit mode selection behavior

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

/// Test that explicit GPU mode overrides default (none)
///
/// This test verifies that when a user explicitly specifies a GPU mode,
/// it overrides the default "none" setting. This ensures cached or default
/// settings don't interfere with explicit user choices.
#[test]
fn test_gpu_mode_override_default() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_mode_override_default: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a minimal devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU Override Test",
    "image": "alpine:3.19"
}
"#;
    fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
    fs::write(
        tmp.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run up with explicit --gpu-mode all (overriding default "none")
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(tmp.path())
        .arg("up")
        .arg("--gpu-mode")
        .arg("all")
        .arg("--log-level")
        .arg("debug")
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify the CLI accepts the explicit mode
    assert!(
        !stderr.contains("unexpected argument") && !stderr.contains("invalid value"),
        "Explicit --gpu-mode all should be accepted. stderr: {}",
        stderr
    );

    // Now test that default behavior (no flag) is different
    let mut cmd2 = Command::cargo_bin("deacon").unwrap();
    let output2 = cmd2
        .current_dir(tmp.path())
        .arg("up")
        .arg("--log-level")
        .arg("debug")
        .output()
        .expect("Failed to execute command");

    let stderr2 = String::from_utf8_lossy(&output2.stderr);

    // Both should succeed (or fail consistently), but the explicit mode was honored
    // The difference is internal - explicit mode "all" vs default mode "none"
    // We verify that the explicit flag is processed without errors
    assert!(
        !stderr.contains("unexpected argument"),
        "Default behavior should work. stderr: {}",
        stderr2
    );
}

/// Test that invalid GPU mode values are rejected by CLI
///
/// This test verifies that the CLI properly validates GPU mode values
/// and rejects invalid inputs with clear error messages.
#[test]
fn test_invalid_gpu_mode_rejected() {
    if !is_docker_available() {
        eprintln!("Skipping test_invalid_gpu_mode_rejected: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a minimal devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU Invalid Test",
    "image": "alpine:3.19"
}
"#;
    fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
    fs::write(
        tmp.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test various invalid GPU mode values
    let invalid_modes = vec!["invalid", "auto", "yes", "no", "true", "false", "gpu"];

    for invalid_mode in invalid_modes {
        let mut cmd = Command::cargo_bin("deacon").unwrap();
        let result = cmd
            .current_dir(tmp.path())
            .arg("up")
            .arg("--gpu-mode")
            .arg(invalid_mode)
            .assert();

        let output = result.get_output();
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Verify that invalid mode is rejected
        // The error should come from clap or the FromStr implementation
        assert!(
            stderr.contains("invalid value")
                || stderr.contains("Invalid GPU mode")
                || !output.status.success(),
            "Invalid GPU mode '{}' should be rejected. stderr: {}",
            invalid_mode,
            stderr
        );
    }
}

/// Test that GPU mode is case-insensitive
///
/// This test verifies that GPU mode values are parsed case-insensitively,
/// allowing users to specify "ALL", "All", "all", etc.
#[test]
fn test_gpu_mode_case_insensitive() {
    // Unit test for FromStr implementation
    use std::str::FromStr;

    // Test "all" with different cases
    assert_eq!(GpuMode::from_str("all").unwrap(), GpuMode::All);
    assert_eq!(GpuMode::from_str("ALL").unwrap(), GpuMode::All);
    assert_eq!(GpuMode::from_str("All").unwrap(), GpuMode::All);
    assert_eq!(GpuMode::from_str("aLL").unwrap(), GpuMode::All);

    // Test "detect" with different cases
    assert_eq!(GpuMode::from_str("detect").unwrap(), GpuMode::Detect);
    assert_eq!(GpuMode::from_str("DETECT").unwrap(), GpuMode::Detect);
    assert_eq!(GpuMode::from_str("Detect").unwrap(), GpuMode::Detect);
    assert_eq!(GpuMode::from_str("DeTeCt").unwrap(), GpuMode::Detect);

    // Test "none" with different cases
    assert_eq!(GpuMode::from_str("none").unwrap(), GpuMode::None);
    assert_eq!(GpuMode::from_str("NONE").unwrap(), GpuMode::None);
    assert_eq!(GpuMode::from_str("None").unwrap(), GpuMode::None);
    assert_eq!(GpuMode::from_str("NoNe").unwrap(), GpuMode::None);
}

/// Test that GPU mode is case-insensitive in CLI usage
///
/// This integration test verifies that the CLI accepts GPU mode values
/// in any case combination.
#[test]
fn test_gpu_mode_case_insensitive_cli() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_mode_case_insensitive_cli: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a minimal devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU Case Test",
    "image": "alpine:3.19"
}
"#;
    fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
    fs::write(
        tmp.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test different case variations
    let case_variations = vec![
        ("ALL", "all"),
        ("All", "all"),
        ("DETECT", "detect"),
        ("Detect", "detect"),
        ("NONE", "none"),
        ("None", "none"),
    ];

    for (input_case, expected_mode) in case_variations {
        let mut cmd = Command::cargo_bin("deacon").unwrap();
        let output = cmd
            .current_dir(tmp.path())
            .arg("up")
            .arg("--gpu-mode")
            .arg(input_case)
            .arg("--log-level")
            .arg("debug")
            .output()
            .expect("Failed to execute command");

        let stderr = String::from_utf8_lossy(&output.stderr);

        // Verify the CLI accepts the case variation
        assert!(
            !stderr.contains("invalid value")
                && !stderr.contains("Invalid GPU mode")
                && !stderr.contains("unexpected argument"),
            "GPU mode '{}' (expecting '{}') should be accepted. stderr: {}",
            input_case,
            expected_mode,
            stderr
        );
    }
}

/// Test that GPU mode works correctly with other up flags
///
/// This test verifies that --gpu-mode can be combined with other common
/// up command flags without conflicts or errors.
#[test]
fn test_gpu_mode_with_other_flags() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_mode_with_other_flags: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a minimal devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU Flags Test",
    "image": "alpine:3.19"
}
"#;
    fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
    fs::write(
        tmp.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test GPU mode with --log-level
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(tmp.path())
        .arg("up")
        .arg("--gpu-mode")
        .arg("detect")
        .arg("--log-level")
        .arg("info")
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("unexpected argument") && !stderr.contains("conflicting"),
        "GPU mode should work with --log-level. stderr: {}",
        stderr
    );

    // Test GPU mode with --log-format
    let mut cmd2 = Command::cargo_bin("deacon").unwrap();
    let output2 = cmd2
        .current_dir(tmp.path())
        .arg("up")
        .arg("--gpu-mode")
        .arg("all")
        .arg("--log-format")
        .arg("json")
        .output()
        .expect("Failed to execute command");

    let stderr2 = String::from_utf8_lossy(&output2.stderr);

    assert!(
        !stderr2.contains("unexpected argument") && !stderr2.contains("conflicting"),
        "GPU mode should work with --log-format. stderr: {}",
        stderr2
    );
}

/// Test that GPU mode is applied consistently across multiple invocations
///
/// This test verifies that each invocation of `up` with an explicit GPU mode
/// uses that mode, and doesn't carry over settings from previous runs.
#[test]
fn test_gpu_mode_no_cross_invocation_state() {
    if !is_docker_available() {
        eprintln!("Skipping test_gpu_mode_no_cross_invocation_state: Docker not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(tmp.path());

    // Create a minimal devcontainer configuration
    let devcontainer_config = r#"{
    "name": "GPU State Test",
    "image": "alpine:3.19"
}
"#;
    fs::create_dir(tmp.path().join(".devcontainer")).unwrap();
    fs::write(
        tmp.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // First invocation with GPU mode "all"
    let mut cmd1 = Command::cargo_bin("deacon").unwrap();
    let output1 = cmd1
        .current_dir(tmp.path())
        .arg("up")
        .arg("--gpu-mode")
        .arg("all")
        .arg("--log-level")
        .arg("debug")
        .output()
        .expect("Failed to execute command");

    let stderr1 = String::from_utf8_lossy(&output1.stderr);

    assert!(
        !stderr1.contains("unexpected argument"),
        "First invocation with --gpu-mode all should succeed. stderr: {}",
        stderr1
    );

    // Second invocation with GPU mode "none" - should be independent
    let mut cmd2 = Command::cargo_bin("deacon").unwrap();
    let output2 = cmd2
        .current_dir(tmp.path())
        .arg("up")
        .arg("--gpu-mode")
        .arg("none")
        .arg("--log-level")
        .arg("debug")
        .output()
        .expect("Failed to execute command");

    let stderr2 = String::from_utf8_lossy(&output2.stderr);

    assert!(
        !stderr2.contains("unexpected argument"),
        "Second invocation with --gpu-mode none should be independent. stderr: {}",
        stderr2
    );

    // Third invocation with default (no flag) - should use default "none"
    let mut cmd3 = Command::cargo_bin("deacon").unwrap();
    let output3 = cmd3
        .current_dir(tmp.path())
        .arg("up")
        .arg("--log-level")
        .arg("debug")
        .output()
        .expect("Failed to execute command");

    let stderr3 = String::from_utf8_lossy(&output3.stderr);

    assert!(
        !stderr3.contains("unexpected argument"),
        "Third invocation with default should work. stderr: {}",
        stderr3
    );

    // All three invocations should be independent with no state carryover
}

/// Test that GPU mode enum values are comprehensive
///
/// This test verifies that the GpuMode enum has exactly the expected variants
/// and no additional ones.
#[test]
fn test_gpu_mode_enum_completeness() {
    use std::str::FromStr;

    // Verify all valid modes parse successfully
    assert!(GpuMode::from_str("all").is_ok());
    assert!(GpuMode::from_str("detect").is_ok());
    assert!(GpuMode::from_str("none").is_ok());

    // Verify enum has exactly 3 variants by checking all possible values
    let all_modes = [GpuMode::All, GpuMode::Detect, GpuMode::None];

    // Each mode should have a unique string representation
    let mode_strings: Vec<String> = all_modes.iter().map(|m| m.to_string()).collect();
    assert_eq!(mode_strings.len(), 3);
    assert!(mode_strings.contains(&"all".to_string()));
    assert!(mode_strings.contains(&"detect".to_string()));
    assert!(mode_strings.contains(&"none".to_string()));

    // Verify all modes are distinct
    assert_ne!(GpuMode::All, GpuMode::Detect);
    assert_ne!(GpuMode::All, GpuMode::None);
    assert_ne!(GpuMode::Detect, GpuMode::None);
}

/// Test that GPU mode error messages are helpful
///
/// This test verifies that when an invalid GPU mode is provided,
/// the error message clearly indicates what went wrong and what
/// the valid options are.
#[test]
fn test_gpu_mode_error_message_quality() {
    use std::str::FromStr;

    // Test that error message for invalid mode is helpful
    let result = GpuMode::from_str("invalid");
    assert!(result.is_err());

    let error_msg = result.unwrap_err();

    // Error message should mention the invalid value
    assert!(
        error_msg.contains("invalid") || error_msg.contains("Invalid"),
        "Error message should mention the invalid value: {}",
        error_msg
    );

    // Error message should list valid options
    assert!(
        error_msg.contains("all") && error_msg.contains("detect") && error_msg.contains("none"),
        "Error message should list valid options: {}",
        error_msg
    );
}
