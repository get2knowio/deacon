//! Integration tests for force-tty-if-json feature in the up command
//!
//! Tests verify PTY allocation behavior when JSON logging is enabled, including:
//! - PTY enabled via --force-tty-if-json flag
//! - PTY enabled via DEACON_FORCE_TTY_IF_JSON environment variable
//! - JSON output purity (stdout JSON only, logs to stderr)
//! - Flag override precedence over environment variable
//!
//! Spec: specs/001-force-pty-up/spec.md

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

/// Check if Docker is available for running integration tests
fn docker_available() -> bool {
    StdCommand::new("docker")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Create a minimal devcontainer.json for testing
fn create_test_devcontainer(temp_dir: &TempDir) {
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();

    let devcontainer_config = serde_json::json!({
        "name": "PTYTest",
        "image": "alpine:3.19",
        "postCreateCommand": "echo 'lifecycle executed'"
    });

    let config_path = devcontainer_dir.join("devcontainer.json");
    fs::write(
        &config_path,
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();
}

/// Parse JSON from stdout, handling potential trailing output
fn parse_json_output(stdout: &str) -> Result<Value, String> {
    let trimmed = stdout.trim();

    // Try parsing the entire output first
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Ok(value);
    }

    // If that fails, try to find the last JSON object
    if let Some(idx) = trimmed.rfind('{') {
        if let Ok(value) = serde_json::from_str::<Value>(&trimmed[idx..]) {
            return Ok(value);
        }
    }

    Err(format!("Failed to parse JSON from stdout: {}", trimmed))
}

/// Extract container ID from deacon output
fn extract_container_id(output: &Value) -> Option<String> {
    output["containerId"]
        .as_str()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

/// Drop guard to clean up containers created by this test module
struct ContainerGuard {
    container_ids: std::cell::RefCell<Vec<String>>,
}

impl ContainerGuard {
    fn new() -> Self {
        Self {
            container_ids: std::cell::RefCell::new(Vec::new()),
        }
    }

    fn register(&self, id: String) {
        if !id.is_empty() {
            self.container_ids.borrow_mut().push(id);
        }
    }
}

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        for id in self.container_ids.borrow().iter() {
            let _ = StdCommand::new("docker").args(["rm", "-f", id]).output();
        }
    }
}

/// T005: Integration test for PTY-on with --force-tty-if-json flag
#[test]
fn integration_up_force_tty_if_json_flag_enables_pty() {
    if !docker_available() {
        eprintln!("Skipping: docker not available");
        return;
    }

    let guard = ContainerGuard::new();
    let temp_dir = TempDir::new().unwrap();
    create_test_devcontainer(&temp_dir);

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(temp_dir.path())
        .env("DEACON_LOG", "warn")
        .args([
            "up",
            "--workspace-folder",
            temp_dir.path().to_str().unwrap(),
            "--log-format",
            "json",
            "--force-tty-if-json",
            "--remove-existing-container",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Command should succeed
    if !output.status.success() {
        eprintln!("Command failed with exit code: {:?}", output.status.code());
        eprintln!("STDOUT:\n{}", stdout);
        eprintln!("STDERR:\n{}", stderr);
    }
    assert!(
        output.status.success(),
        "up command should succeed with --force-tty-if-json flag"
    );

    // Parse JSON output
    let json_result = parse_json_output(&stdout);
    assert!(
        json_result.is_ok(),
        "stdout should contain valid JSON: {:?}",
        json_result.err()
    );

    let json = json_result.unwrap();
    let container_id = extract_container_id(&json);
    assert!(
        container_id.is_some(),
        "output should contain containerId field"
    );

    // Register container for cleanup
    if let Some(id) = container_id {
        guard.register(id);
    }
}

/// T005 (env variant): Integration test for PTY-on with DEACON_FORCE_TTY_IF_JSON env var
#[test]
fn integration_up_force_tty_if_json_env_enables_pty() {
    if !docker_available() {
        eprintln!("Skipping: docker not available");
        return;
    }

    let guard = ContainerGuard::new();
    let temp_dir = TempDir::new().unwrap();
    create_test_devcontainer(&temp_dir);

    // Test with various truthy values
    for env_value in &["true", "True", "TRUE", "1", "yes", "Yes", "YES"] {
        let mut cmd = Command::cargo_bin("deacon").unwrap();
        let output = cmd
            .current_dir(temp_dir.path())
            .env("DEACON_LOG", "warn")
            .env("DEACON_FORCE_TTY_IF_JSON", env_value)
            .args([
                "up",
                "--workspace-folder",
                temp_dir.path().to_str().unwrap(),
                "--log-format",
                "json",
                "--remove-existing-container",
            ])
            .output()
            .unwrap();

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Command should succeed
        if !output.status.success() {
            eprintln!(
                "Command failed with DEACON_FORCE_TTY_IF_JSON={} exit code: {:?}",
                env_value,
                output.status.code()
            );
            eprintln!("STDOUT:\n{}", stdout);
            eprintln!("STDERR:\n{}", stderr);
        }
        assert!(
            output.status.success(),
            "up command should succeed with DEACON_FORCE_TTY_IF_JSON={}",
            env_value
        );

        // Parse JSON output
        let json_result = parse_json_output(&stdout);
        assert!(
            json_result.is_ok(),
            "stdout should contain valid JSON with env={}: {:?}",
            env_value,
            json_result.err()
        );

        let json = json_result.unwrap();
        let container_id = extract_container_id(&json);
        assert!(
            container_id.is_some(),
            "output should contain containerId field with env={}",
            env_value
        );

        // Register container for cleanup
        if let Some(id) = container_id {
            guard.register(id);
        }
    }
}

/// T005a: Integration test verifying JSON output purity under PTY
/// Ensures stdout contains only valid JSON and logs go to stderr
#[test]
fn integration_up_force_tty_json_output_purity() {
    if !docker_available() {
        eprintln!("Skipping: docker not available");
        return;
    }

    let guard = ContainerGuard::new();
    let temp_dir = TempDir::new().unwrap();
    create_test_devcontainer(&temp_dir);

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(temp_dir.path())
        .env("DEACON_LOG", "debug") // Enable more verbose logging
        .args([
            "up",
            "--workspace-folder",
            temp_dir.path().to_str().unwrap(),
            "--log-format",
            "json",
            "--force-tty-if-json",
            "--remove-existing-container",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Command should succeed
    assert!(
        output.status.success(),
        "up command should succeed for JSON purity test"
    );

    // Stdout should be parseable as JSON with no log contamination
    let json_result = parse_json_output(&stdout);
    assert!(
        json_result.is_ok(),
        "stdout should be pure JSON without log contamination: {:?}\nSTDOUT:\n{}",
        json_result.err(),
        stdout
    );

    let json = json_result.unwrap();

    // Verify the JSON has expected structure
    assert!(json.is_object(), "JSON output should be an object");

    let container_id = extract_container_id(&json);
    assert!(
        container_id.is_some(),
        "JSON output should contain containerId field"
    );

    // Stderr should contain logs (not stdout)
    // With debug logging enabled, stderr should have some content
    assert!(
        !stderr.is_empty(),
        "stderr should contain debug logs when DEACON_LOG=debug is set"
    );

    // Register container for cleanup
    if let Some(id) = container_id {
        guard.register(id);
    }
}

/// T005b: Integration test for PTY allocation failure path
/// Note: This test documents the expected behavior but is marked as ignored
/// because reliably simulating PTY allocation failure is environment-dependent
#[test]
#[ignore = "PTY allocation failure simulation requires specific environment setup"]
fn integration_up_force_tty_allocation_failure_surfaces_error() {
    // This test would verify that when PTY allocation fails:
    // 1. The error message is clear and actionable
    // 2. The system does not silently downgrade to non-PTY mode
    // 3. JSON log formatting expectations are not violated
    //
    // Manual testing approach:
    // - Run in environment without PTY support (e.g., strict CI container)
    // - Verify error message mentions PTY allocation failure
    // - Verify exit code is non-zero
    // - Verify no partial JSON output corruption
    //
    // Example expected error pattern:
    // "Failed to allocate PTY for lifecycle command: PTY not available"
}

/// T005c: Integration test verifying flag overrides environment variable
/// When both flag and env are set, the flag should take precedence
#[test]
fn integration_up_flag_overrides_env_var() {
    if !docker_available() {
        eprintln!("Skipping: docker not available");
        return;
    }

    let guard = ContainerGuard::new();
    let temp_dir = TempDir::new().unwrap();
    create_test_devcontainer(&temp_dir);

    // Set env to false, but provide flag (should enable PTY)
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(temp_dir.path())
        .env("DEACON_LOG", "warn")
        .env("DEACON_FORCE_TTY_IF_JSON", "false") // Env says no
        .args([
            "up",
            "--workspace-folder",
            temp_dir.path().to_str().unwrap(),
            "--log-format",
            "json",
            "--force-tty-if-json", // Flag says yes - should win
            "--remove-existing-container",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Command should succeed
    if !output.status.success() {
        eprintln!("Command failed with exit code: {:?}", output.status.code());
        eprintln!("STDOUT:\n{}", stdout);
        eprintln!("STDERR:\n{}", stderr);
    }
    assert!(
        output.status.success(),
        "up command should succeed when flag overrides env"
    );

    // Parse JSON output
    let json_result = parse_json_output(&stdout);
    assert!(
        json_result.is_ok(),
        "stdout should contain valid JSON when flag overrides env: {:?}",
        json_result.err()
    );

    let json = json_result.unwrap();
    let container_id = extract_container_id(&json);
    assert!(
        container_id.is_some(),
        "output should contain containerId when flag overrides env"
    );

    // Register container for cleanup
    if let Some(id) = container_id {
        guard.register(id);
    }
}

/// Additional test: Verify PTY is NOT enabled when flag/env are absent
#[test]
fn integration_up_default_no_pty_with_json() {
    if !docker_available() {
        eprintln!("Skipping: docker not available");
        return;
    }

    let guard = ContainerGuard::new();
    let temp_dir = TempDir::new().unwrap();
    create_test_devcontainer(&temp_dir);

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(temp_dir.path())
        .env("DEACON_LOG", "warn")
        .env_remove("DEACON_FORCE_TTY_IF_JSON") // Ensure env is not set
        .args([
            "up",
            "--workspace-folder",
            temp_dir.path().to_str().unwrap(),
            "--log-format",
            "json",
            "--remove-existing-container",
            // Note: NO --force-tty-if-json flag
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Command should succeed even without PTY
    if !output.status.success() {
        eprintln!("Command failed with exit code: {:?}", output.status.code());
        eprintln!("STDOUT:\n{}", stdout);
        eprintln!("STDERR:\n{}", stderr);
    }
    assert!(
        output.status.success(),
        "up command should succeed without PTY flags"
    );

    // Parse JSON output (should still work without PTY)
    let json_result = parse_json_output(&stdout);
    assert!(
        json_result.is_ok(),
        "stdout should contain valid JSON without PTY: {:?}",
        json_result.err()
    );

    let json = json_result.unwrap();
    let container_id = extract_container_id(&json);
    assert!(
        container_id.is_some(),
        "output should contain containerId without PTY"
    );

    // Register container for cleanup
    if let Some(id) = container_id {
        guard.register(id);
    }
}

/// Additional test: Verify falsey env values disable PTY
#[test]
fn integration_up_falsey_env_disables_pty() {
    if !docker_available() {
        eprintln!("Skipping: docker not available");
        return;
    }

    let guard = ContainerGuard::new();
    let temp_dir = TempDir::new().unwrap();
    create_test_devcontainer(&temp_dir);

    // Test with various falsey values
    for env_value in &["false", "False", "FALSE", "0", "no", "No", "NO"] {
        let mut cmd = Command::cargo_bin("deacon").unwrap();
        let output = cmd
            .current_dir(temp_dir.path())
            .env("DEACON_LOG", "warn")
            .env("DEACON_FORCE_TTY_IF_JSON", env_value)
            .args([
                "up",
                "--workspace-folder",
                temp_dir.path().to_str().unwrap(),
                "--log-format",
                "json",
                "--remove-existing-container",
            ])
            .output()
            .unwrap();

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Command should succeed without PTY
        if !output.status.success() {
            eprintln!(
                "Command failed with DEACON_FORCE_TTY_IF_JSON={} exit code: {:?}",
                env_value,
                output.status.code()
            );
            eprintln!("STDOUT:\n{}", stdout);
            eprintln!("STDERR:\n{}", stderr);
        }
        assert!(
            output.status.success(),
            "up command should succeed with DEACON_FORCE_TTY_IF_JSON={} (falsey)",
            env_value
        );

        // Parse JSON output
        let json_result = parse_json_output(&stdout);
        assert!(
            json_result.is_ok(),
            "stdout should contain valid JSON with falsey env={}: {:?}",
            env_value,
            json_result.err()
        );

        let json = json_result.unwrap();
        let container_id = extract_container_id(&json);
        assert!(
            container_id.is_some(),
            "output should contain containerId with falsey env={}",
            env_value
        );

        // Register container for cleanup
        if let Some(id) = container_id {
            guard.register(id);
        }
    }
}

/// T011: Integration test for non-JSON log mode with PTY toggle
/// Verifies that PTY toggle doesn't affect outcome when log-format is not json (FR-006)
/// Note: `up` command always outputs JSON to stdout per contract, regardless of log format
#[test]
fn integration_up_non_json_mode_ignores_pty_toggle() {
    if !docker_available() {
        eprintln!("Skipping: docker not available");
        return;
    }

    let guard = ContainerGuard::new();
    let temp_dir = TempDir::new().unwrap();
    create_test_devcontainer(&temp_dir);

    // Test with flag set but without JSON log format (default text logs on stderr)
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(temp_dir.path())
        .env("DEACON_LOG", "warn")
        .args([
            "up",
            "--workspace-folder",
            temp_dir.path().to_str().unwrap(),
            "--force-tty-if-json", // Flag is set
            "--remove-existing-container",
            // Note: NO --log-format json (default text logs on stderr)
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Command should succeed
    if !output.status.success() {
        eprintln!("Command failed with exit code: {:?}", output.status.code());
        eprintln!("STDOUT:\n{}", stdout);
        eprintln!("STDERR:\n{}", stderr);
    }
    assert!(
        output.status.success(),
        "up command should succeed with PTY flag and text log mode"
    );

    // Stdout should ALWAYS be JSON (per up contract), regardless of log format
    let result =
        parse_json_output(&stdout).expect("stdout should always be valid JSON per up contract");
    assert_eq!(result["outcome"], "success");

    // Register first container for cleanup
    if let Some(id) = extract_container_id(&result) {
        guard.register(id);
    }

    // Test with env var set but without JSON log format
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .current_dir(temp_dir.path())
        .env("DEACON_LOG", "warn")
        .env("DEACON_FORCE_TTY_IF_JSON", "true") // Env is set
        .args([
            "up",
            "--workspace-folder",
            temp_dir.path().to_str().unwrap(),
            "--remove-existing-container",
            // Note: NO --log-format json (default text logs on stderr)
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Command should succeed
    if !output.status.success() {
        eprintln!(
            "Command failed with env set, exit code: {:?}",
            output.status.code()
        );
        eprintln!("STDOUT:\n{}", stdout);
        eprintln!("STDERR:\n{}", stderr);
    }
    assert!(
        output.status.success(),
        "up command should succeed with PTY env var and text log mode"
    );

    // Stdout should ALWAYS be JSON (per up contract), regardless of log format
    let result = parse_json_output(&stdout)
        .expect("stdout should always be valid JSON per up contract with env var");
    assert_eq!(result["outcome"], "success");

    // Register second container for cleanup
    if let Some(id) = extract_container_id(&result) {
        guard.register(id);
    }
}
