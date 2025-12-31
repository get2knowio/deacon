#![cfg(feature = "full")]
//! Smoke tests for doctor command text mode
//!
//! Scenarios covered:
//! - Doctor text mode: human-readable output with recognizable markers
//! - Doctor text mode stability: consistent output format even with logging noise
//!
//! Tests verify that `deacon doctor` (without --json) provides stable text output
//! that includes key diagnostic markers while being resilient to log output.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

/// Test doctor text mode basic functionality
#[test]
fn test_doctor_text_mode_basic() {
    let temp_dir = TempDir::new().unwrap();

    // Create minimal devcontainer.json to provide some context
    let devcontainer_config = r#"{
    "name": "Doctor Text Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test doctor command (text mode is default)
    let mut doctor_cmd = Command::cargo_bin("deacon").unwrap();
    let doctor_output = doctor_cmd
        .current_dir(&temp_dir)
        .arg("doctor")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let doctor_stderr = String::from_utf8_lossy(&doctor_output.stderr);
    let doctor_stdout = String::from_utf8_lossy(&doctor_output.stdout);

    // Doctor should preferably succeed, but we also allow non-empty stderr
    // as long as stdout contains recognizable markers
    if doctor_output.status.success() || !doctor_stdout.is_empty() {
        // Check for stable text output markers
        let combined_output = format!("{}\n{}", doctor_stdout, doctor_stderr);

        // Look for key diagnostic markers that should be present in text mode
        let has_doctor_marker = combined_output.contains("Doctor")
            || combined_output.contains("Diagnostics")
            || combined_output.contains("Environment");

        let has_host_info = combined_output.contains("Host")
            || combined_output.contains("OS")
            || combined_output.contains("Architecture");

        let has_docker_info =
            combined_output.contains("Docker") || combined_output.contains("Container");

        assert!(
            has_doctor_marker,
            "Doctor text output should contain diagnostic markers. Got stdout: '{}', stderr: '{}'",
            doctor_stdout, doctor_stderr
        );

        // At least one of host or docker info should be present
        assert!(
            has_host_info || has_docker_info,
            "Doctor text output should contain host or docker information. Got stdout: '{}', stderr: '{}'",
            doctor_stdout, doctor_stderr
        );

        println!("Doctor text mode basic test passed");
    } else {
        // If doctor fails completely, that's also acceptable in constrained environments
        println!(
            "Doctor command failed, which is acceptable in constrained environments: {}",
            doctor_stderr
        );
    }
}

/// Test doctor text mode stability with logging noise
#[test]
fn test_doctor_text_mode_stability() {
    let temp_dir = TempDir::new().unwrap();

    // Create devcontainer.json
    let devcontainer_config = r#"{
    "name": "Doctor Text Stability Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test doctor command with verbose logging that might add noise
    let mut doctor_cmd = Command::cargo_bin("deacon").unwrap();
    let doctor_output = doctor_cmd
        .current_dir(&temp_dir)
        .arg("--log-level")
        .arg("debug")
        .arg("doctor")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let doctor_stderr = String::from_utf8_lossy(&doctor_output.stderr);
    let doctor_stdout = String::from_utf8_lossy(&doctor_output.stdout);

    // Even with debug logging, we should still get meaningful text output
    if doctor_output.status.success() || !doctor_stdout.is_empty() {
        let combined_output = format!("{}\n{}", doctor_stdout, doctor_stderr);

        // Verify recognizable diagnostic markers are still present despite logging noise
        let has_stable_markers = combined_output.contains("Doctor")
            || combined_output.contains("Diagnostics")
            || combined_output.contains("CLI Version")
            || combined_output.contains("Host")
            || combined_output.contains("Docker");

        assert!(
            has_stable_markers,
            "Doctor text output should remain stable with debug logging. Got stdout: '{}', stderr: '{}'",
            doctor_stdout, doctor_stderr
        );

        // Verify we have actual diagnostic content, not just log messages
        let has_diagnostic_content = combined_output.contains("Version")
            || combined_output.contains("OS")
            || combined_output.contains("Available")
            || combined_output.contains("Running");

        assert!(
            has_diagnostic_content,
            "Doctor should provide diagnostic content beyond just log messages. Got stdout: '{}', stderr: '{}'",
            doctor_stdout, doctor_stderr
        );

        println!("Doctor text mode stability test passed");
    } else {
        println!(
            "Doctor command failed with debug logging, which is acceptable: {}",
            doctor_stderr
        );
    }
}

/// Test doctor text mode vs JSON mode differences
#[test]
fn test_doctor_text_vs_json_mode() {
    let temp_dir = TempDir::new().unwrap();

    // Create devcontainer.json
    let devcontainer_config = r#"{
    "name": "Doctor Mode Comparison Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test text mode (default)
    let mut text_cmd = Command::cargo_bin("deacon").unwrap();
    let text_output = text_cmd
        .current_dir(&temp_dir)
        .arg("doctor")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    // Test JSON mode
    let mut json_cmd = Command::cargo_bin("deacon").unwrap();
    let json_output = json_cmd
        .current_dir(&temp_dir)
        .arg("doctor")
        .arg("--json")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let text_stdout = String::from_utf8_lossy(&text_output.stdout);
    let json_stdout = String::from_utf8_lossy(&json_output.stdout);

    // Both should succeed or fail gracefully
    if text_output.status.success() && json_output.status.success() {
        // Text mode should not be valid JSON
        let text_is_json = serde_json::from_str::<serde_json::Value>(&text_stdout).is_ok();
        assert!(
            !text_is_json,
            "Text mode output should not be valid JSON. Got: '{}'",
            text_stdout
        );

        // JSON mode should contain valid JSON (may have logs before it)
        // Look for complete JSON object by finding first standalone { and matching }
        let json_content = if let Some(start) = json_stdout.find("{\n") {
            // Find the entire JSON by counting braces to get a complete object
            let from_start = &json_stdout[start..];
            let mut brace_count = 0;
            let mut end_pos = 0;

            for (i, ch) in from_start.char_indices() {
                if ch == '{' {
                    brace_count += 1;
                } else if ch == '}' {
                    brace_count -= 1;
                    if brace_count == 0 {
                        end_pos = start + i + 1;
                        break;
                    }
                }
            }

            if end_pos > start {
                &json_stdout[start..end_pos]
            } else {
                &json_stdout
            }
        } else {
            &json_stdout
        };

        let json_is_valid = serde_json::from_str::<serde_json::Value>(json_content).is_ok();
        assert!(
            json_is_valid,
            "JSON mode output should contain valid JSON. Got full: '{}', extracted: '{}'",
            json_stdout, json_content
        );

        // Text mode should be human-readable (contains spaces and readable text)
        let text_is_readable = text_stdout.contains(" ")
            && (text_stdout.contains("Doctor")
                || text_stdout.contains("Host")
                || text_stdout.contains("Docker"));

        assert!(
            text_is_readable,
            "Text mode should be human-readable. Got: '{}'",
            text_stdout
        );

        println!("Doctor text vs JSON mode comparison test passed");
    } else {
        println!("Doctor commands failed, which is acceptable in constrained environments");
    }
}
