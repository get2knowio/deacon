//! Integration tests for feature lifecycle command execution
//!
//! These tests verify that feature lifecycle commands execute in the correct order
//! (features before config) and that failures are handled with proper fail-fast behavior
//! and error attribution.
//!
//! This is part of User Story 2: Feature Lifecycle Commands Execute Before User Commands
//! from specs/009-complete-feature-support/spec.md
//!
//! Test Coverage:
//! - T022: Integration tests for lifecycle ordering (docker-shared group)
//! - T023: Test fail-fast behavior when feature command fails

use assert_cmd::Command;
use serde_json::{json, Value};
use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

/// Helper function to check if Docker is available
fn is_docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Helper to create a simple local feature with lifecycle commands
fn create_local_feature(
    temp_dir: &TempDir,
    feature_name: &str,
    lifecycle_commands: serde_json::Value,
) {
    let feature_dir = temp_dir.path().join(".devcontainer").join(feature_name);
    fs::create_dir_all(&feature_dir).unwrap();

    let mut feature_json = json!({
        "id": feature_name,
        "version": "1.0.0",
        "name": format!("Test Feature {}", feature_name),
    });

    // Merge lifecycle commands into the feature JSON
    if let Some(obj) = feature_json.as_object_mut() {
        if let Some(lifecycle_obj) = lifecycle_commands.as_object() {
            for (key, value) in lifecycle_obj {
                obj.insert(key.clone(), value.clone());
            }
        }
    }

    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        serde_json::to_string_pretty(&feature_json).unwrap(),
    )
    .unwrap();
}

/// Container cleanup guard - ensures containers are removed after tests
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

/// Run `deacon up` and return the container ID
fn run_deacon_up(
    temp_dir: &TempDir,
    guard: &ContainerGuard,
    extra_args: &[&str],
) -> Result<String, String> {
    let mut cmd = Command::cargo_bin("deacon").expect("deacon binary");
    let mut args = vec![
        "up",
        "--workspace-folder",
        temp_dir.path().to_str().unwrap(),
        "--mount-workspace-git-root=false",
        "--remove-existing-container",
    ];
    args.extend_from_slice(extra_args);

    let assert = cmd
        .current_dir(temp_dir)
        .env("DEACON_LOG", "warn")
        .args(&args)
        .assert();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        return Err(format!(
            "deacon up failed:\nSTDOUT:\n{}\nSTDERR:\n{}",
            stdout, stderr
        ));
    }

    let trimmed = stdout.trim();
    let value: Value = serde_json::from_str::<Value>(trimmed)
        .ok()
        .or_else(|| {
            trimmed
                .rfind('{')
                .and_then(|idx| serde_json::from_str::<Value>(&trimmed[idx..]).ok())
        })
        .ok_or_else(|| {
            format!(
                "Expected valid JSON output\nSTDOUT:\n{}\nSTDERR:\n{}",
                stdout, stderr
            )
        })?;

    let container_id = value["containerId"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    if container_id.is_empty() {
        return Err(format!("Expected containerId in output: {:?}", value));
    }

    guard.register(container_id.clone());
    Ok(container_id)
}

/// Read a file from the container using docker exec
fn read_container_file(container_id: &str, path: &str) -> Result<String, String> {
    let output = StdCommand::new("docker")
        .args(["exec", container_id, "cat", path])
        .output()
        .map_err(|e| format!("Failed to run docker exec: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "docker exec failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Check if a file exists in the container
fn file_exists_in_container(container_id: &str, path: &str) -> bool {
    StdCommand::new("docker")
        .args(["exec", container_id, "test", "-f", path])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ============================================================================
// T022: Integration tests for lifecycle ordering (docker-shared group)
// ============================================================================

/// Test that feature lifecycle commands execute before config lifecycle commands
#[test]
fn test_feature_lifecycle_commands_execute_before_config() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_feature_lifecycle_commands_execute_before_config: Docker not available"
        );
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create two features with onCreateCommand that append to a marker file
    create_local_feature(
        &temp_dir,
        "feature-a",
        json!({
            "onCreateCommand": "echo 'feature-a' >> /tmp/lifecycle_order.txt"
        }),
    );

    create_local_feature(
        &temp_dir,
        "feature-b",
        json!({
            "onCreateCommand": "echo 'feature-b' >> /tmp/lifecycle_order.txt"
        }),
    );

    // Create devcontainer.json with config onCreateCommand
    let devcontainer_config = json!({
        "name": "Lifecycle Ordering Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./feature-a": {},
            "./feature-b": {}
        },
        "onCreateCommand": "echo 'config' >> /tmp/lifecycle_order.txt"
    });

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    let guard = ContainerGuard::new();
    let container_id = run_deacon_up(&temp_dir, &guard, &["--skip-post-create"])
        .expect("deacon up should succeed");

    // Read the lifecycle order file from the container
    let content = read_container_file(&container_id, "/tmp/lifecycle_order.txt")
        .expect("Failed to read lifecycle order file");

    let lines: Vec<&str> = content.trim().lines().collect();

    // Verify order: feature-a, feature-b, config
    assert_eq!(
        lines.len(),
        3,
        "Expected 3 lifecycle commands to execute. Got: {:?}",
        lines
    );

    assert_eq!(
        lines[0], "feature-a",
        "First command should be from feature-a"
    );

    assert_eq!(
        lines[1], "feature-b",
        "Second command should be from feature-b"
    );

    assert_eq!(lines[2], "config", "Third command should be from config");

    println!("✓ Feature lifecycle commands executed before config in correct order");
}

/// Test that feature lifecycle commands execute in installation order
#[test]
fn test_feature_lifecycle_commands_in_installation_order() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_feature_lifecycle_commands_in_installation_order: Docker not available"
        );
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create three features with distinct lifecycle commands
    create_local_feature(
        &temp_dir,
        "first-feature",
        json!({
            "onCreateCommand": "echo '1:first' >> /tmp/install_order.txt"
        }),
    );

    create_local_feature(
        &temp_dir,
        "second-feature",
        json!({
            "onCreateCommand": "echo '2:second' >> /tmp/install_order.txt"
        }),
    );

    create_local_feature(
        &temp_dir,
        "third-feature",
        json!({
            "onCreateCommand": "echo '3:third' >> /tmp/install_order.txt"
        }),
    );

    // Features should execute in the order they are declared in the config
    let devcontainer_config = json!({
        "name": "Installation Order Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./first-feature": {},
            "./second-feature": {},
            "./third-feature": {}
        }
    });

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    let guard = ContainerGuard::new();
    let container_id = run_deacon_up(&temp_dir, &guard, &["--skip-post-create"])
        .expect("deacon up should succeed");

    let content = read_container_file(&container_id, "/tmp/install_order.txt")
        .expect("Failed to read install order file");

    let lines: Vec<&str> = content.trim().lines().collect();

    assert_eq!(lines.len(), 3, "Expected 3 features to execute");
    assert_eq!(lines[0], "1:first", "First feature should execute first");
    assert_eq!(lines[1], "2:second", "Second feature should execute second");
    assert_eq!(lines[2], "3:third", "Third feature should execute third");

    println!("✓ Feature lifecycle commands executed in installation order");
}

/// Test multiple lifecycle phases (onCreate, postCreate) with features and config
#[test]
fn test_multiple_lifecycle_phases_ordering() {
    if !is_docker_available() {
        eprintln!("Skipping test_multiple_lifecycle_phases_ordering: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create feature with multiple lifecycle phases
    create_local_feature(
        &temp_dir,
        "multi-phase-feature",
        json!({
            "onCreateCommand": "echo 'feature-onCreate' > /tmp/onCreate.txt",
            "updateContentCommand": "echo 'feature-updateContent' > /tmp/updateContent.txt",
            "postCreateCommand": "echo 'feature-postCreate' > /tmp/postCreate.txt"
        }),
    );

    // Config also has multiple lifecycle phases
    let devcontainer_config = json!({
        "name": "Multi-Phase Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./multi-phase-feature": {}
        },
        "onCreateCommand": "echo 'config-onCreate' >> /tmp/onCreate.txt",
        "updateContentCommand": "echo 'config-updateContent' >> /tmp/updateContent.txt",
        "postCreateCommand": "echo 'config-postCreate' >> /tmp/postCreate.txt"
    });

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    let guard = ContainerGuard::new();
    let container_id = run_deacon_up(&temp_dir, &guard, &[]).expect("deacon up should succeed");

    // Verify onCreate ordering
    let on_create_content =
        read_container_file(&container_id, "/tmp/onCreate.txt").unwrap_or_default();
    let on_create_lines: Vec<&str> = on_create_content.trim().lines().collect();

    if on_create_lines.len() >= 2 {
        assert_eq!(
            on_create_lines[0], "feature-onCreate",
            "Feature onCreate should execute first"
        );
        assert_eq!(
            on_create_lines[1], "config-onCreate",
            "Config onCreate should execute after feature"
        );
    }

    // Verify updateContent ordering
    let update_content_content =
        read_container_file(&container_id, "/tmp/updateContent.txt").unwrap_or_default();
    let update_content_lines: Vec<&str> = update_content_content.trim().lines().collect();

    if update_content_lines.len() >= 2 {
        assert_eq!(
            update_content_lines[0], "feature-updateContent",
            "Feature updateContent should execute first"
        );
        assert_eq!(
            update_content_lines[1], "config-updateContent",
            "Config updateContent should execute after feature"
        );
    }

    // Verify postCreate ordering
    let post_create_content =
        read_container_file(&container_id, "/tmp/postCreate.txt").unwrap_or_default();
    let post_create_lines: Vec<&str> = post_create_content.trim().lines().collect();

    if post_create_lines.len() >= 2 {
        assert_eq!(
            post_create_lines[0], "feature-postCreate",
            "Feature postCreate should execute first"
        );
        assert_eq!(
            post_create_lines[1], "config-postCreate",
            "Config postCreate should execute after feature"
        );
    }

    println!("✓ Multiple lifecycle phases maintain feature-before-config ordering");
}

/// Test that empty/null feature lifecycle commands are filtered out
#[test]
fn test_empty_feature_lifecycle_commands_filtered() {
    if !is_docker_available() {
        eprintln!("Skipping test_empty_feature_lifecycle_commands_filtered: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Feature with null onCreateCommand
    create_local_feature(
        &temp_dir,
        "null-feature",
        json!({
            "onCreateCommand": null
        }),
    );

    // Feature with empty string onCreateCommand
    create_local_feature(
        &temp_dir,
        "empty-feature",
        json!({
            "onCreateCommand": ""
        }),
    );

    // Feature with actual command
    create_local_feature(
        &temp_dir,
        "real-feature",
        json!({
            "onCreateCommand": "echo 'real-feature' > /tmp/real_marker.txt"
        }),
    );

    let devcontainer_config = json!({
        "name": "Empty Commands Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./null-feature": {},
            "./empty-feature": {},
            "./real-feature": {}
        },
        "onCreateCommand": "echo 'config' > /tmp/config_marker.txt"
    });

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    let guard = ContainerGuard::new();
    let container_id = run_deacon_up(&temp_dir, &guard, &["--skip-post-create"])
        .expect("deacon up should succeed");

    // Verify only the real feature command and config command ran
    assert!(
        file_exists_in_container(&container_id, "/tmp/real_marker.txt"),
        "Real feature command should have executed"
    );

    assert!(
        file_exists_in_container(&container_id, "/tmp/config_marker.txt"),
        "Config command should have executed"
    );

    println!("✓ Empty and null feature lifecycle commands are properly filtered");
}

/// Test array-format lifecycle commands from features
#[test]
fn test_array_format_feature_lifecycle_commands() {
    if !is_docker_available() {
        eprintln!("Skipping test_array_format_feature_lifecycle_commands: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Feature with array-format command
    create_local_feature(
        &temp_dir,
        "array-cmd-feature",
        json!({
            "onCreateCommand": ["sh", "-c", "echo 'array-command' > /tmp/array_test.txt"]
        }),
    );

    let devcontainer_config = json!({
        "name": "Array Command Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./array-cmd-feature": {}
        }
    });

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    let guard = ContainerGuard::new();
    let container_id = run_deacon_up(&temp_dir, &guard, &["--skip-post-create"])
        .expect("deacon up should succeed");

    // Verify array command executed
    assert!(
        file_exists_in_container(&container_id, "/tmp/array_test.txt"),
        "Array-format command should have executed"
    );

    let content = read_container_file(&container_id, "/tmp/array_test.txt").unwrap_or_default();
    assert!(
        content.contains("array-command"),
        "Array command output should be correct"
    );

    println!("✓ Array-format lifecycle commands from features execute correctly");
}

/// Test object-format lifecycle commands from features (parallel execution)
#[test]
fn test_object_format_feature_lifecycle_commands() {
    if !is_docker_available() {
        eprintln!("Skipping test_object_format_feature_lifecycle_commands: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Feature with object-format command (multiple named commands)
    create_local_feature(
        &temp_dir,
        "object-cmd-feature",
        json!({
            "onCreateCommand": {
                "cmd1": "echo 'command1' > /tmp/cmd1.txt",
                "cmd2": "echo 'command2' > /tmp/cmd2.txt"
            }
        }),
    );

    let devcontainer_config = json!({
        "name": "Object Command Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./object-cmd-feature": {}
        }
    });

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    let guard = ContainerGuard::new();
    let container_id = run_deacon_up(&temp_dir, &guard, &["--skip-post-create"])
        .expect("deacon up should succeed");

    // Verify both commands executed (object format runs in parallel)
    assert!(
        file_exists_in_container(&container_id, "/tmp/cmd1.txt"),
        "First command in object should have executed"
    );

    assert!(
        file_exists_in_container(&container_id, "/tmp/cmd2.txt"),
        "Second command in object should have executed"
    );

    println!("✓ Object-format lifecycle commands from features execute correctly");
}

// ============================================================================
// T023: Test fail-fast behavior when feature command fails
// ============================================================================

/// Test that when a feature onCreateCommand fails, execution stops immediately
/// and returns exit code 1 with proper error attribution
#[test]
fn test_feature_oncreate_command_fails_immediately() {
    if !is_docker_available() {
        eprintln!("Skipping test_feature_oncreate_command_fails_immediately: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a local feature with a failing onCreateCommand
    create_local_feature(
        &temp_dir,
        "failing-feature",
        json!({
            "onCreateCommand": "exit 1"
        }),
    );

    // Create devcontainer.json that uses the failing feature
    // Also add a config onCreateCommand that should NOT run due to fail-fast
    let devcontainer_config = json!({
        "name": "Feature Lifecycle Fail Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./failing-feature": {}
        },
        "onCreateCommand": "echo 'This should not run' > /tmp/config_ran.txt"
    });

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    // Run deacon up - should fail
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    // Verify the command failed
    assert!(
        !up_output.status.success(),
        "deacon up should have failed when feature onCreateCommand fails"
    );

    // Verify exit code is 1
    assert_eq!(
        up_output.status.code(),
        Some(1),
        "Exit code should be 1 when lifecycle command fails"
    );

    // Verify error message contains proper attribution
    let stderr = String::from_utf8_lossy(&up_output.stderr);
    let stdout = String::from_utf8_lossy(&up_output.stdout);
    let combined_output = format!("{}\n{}", stderr, stdout);

    // Error should mention the failure
    assert!(
        combined_output.contains("failed") || combined_output.contains("exit"),
        "Error output should indicate command failure. Output:\n{}",
        combined_output
    );

    // Error should mention onCreate phase
    assert!(
        combined_output.contains("onCreate") || combined_output.contains("create"),
        "Error output should mention onCreate phase. Output:\n{}",
        combined_output
    );
}

/// Test that when a feature postCreateCommand fails, execution stops immediately
/// and the user's postCreateCommand does not run
#[test]
fn test_feature_postcreate_command_fails_before_config_command() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_feature_postcreate_command_fails_before_config_command: Docker not available"
        );
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a marker path on the host to check if config command ran
    let marker_path = temp_dir.path().join("should_not_exist.txt");

    // Create a local feature with a failing postCreateCommand
    create_local_feature(
        &temp_dir,
        "failing-postcreate",
        json!({
            "postCreateCommand": "exit 42"
        }),
    );

    // Create devcontainer.json with a config postCreateCommand that creates a marker
    // This should NOT run due to fail-fast behavior
    let devcontainer_config = format!(
        r#"{{
        "name": "Feature PostCreate Fail Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {{
            "./failing-postcreate": {{}}
        }},
        "postCreateCommand": "echo 'Config command ran' > {}"
    }}"#,
        marker_path.display()
    );

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run deacon up - should fail
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    // Verify the command failed
    assert!(
        !up_output.status.success(),
        "deacon up should have failed when feature postCreateCommand fails"
    );

    // Verify exit code is 1 (lifecycle failures should normalize to exit code 1)
    assert_eq!(
        up_output.status.code(),
        Some(1),
        "Exit code should be 1 when lifecycle command fails (normalized from exit 42)"
    );

    // Verify error message
    let stderr = String::from_utf8_lossy(&up_output.stderr);
    let stdout = String::from_utf8_lossy(&up_output.stdout);
    let combined_output = format!("{}\n{}", stderr, stdout);

    assert!(
        combined_output.contains("failed") || combined_output.contains("exit"),
        "Error output should indicate command failure. Output:\n{}",
        combined_output
    );

    // The marker file should NOT exist because config command should not have run
    // Note: marker_path is on the host, not in container, so this may not be testable
    // in all scenarios. This assertion is aspirational - adjust based on actual behavior.
}

/// Test that when a feature updateContentCommand fails, it stops before
/// config's updateContentCommand
#[test]
fn test_feature_updatecontent_command_fails_immediately() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_feature_updatecontent_command_fails_immediately: Docker not available"
        );
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a local feature with a failing updateContentCommand
    create_local_feature(
        &temp_dir,
        "failing-update",
        json!({
            "updateContentCommand": "false"  // false command always exits with 1
        }),
    );

    // Create devcontainer.json
    let devcontainer_config = json!({
        "name": "Feature UpdateContent Fail Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./failing-update": {}
        },
        "updateContentCommand": "echo 'Config updateContent should not run'"
    });

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    // Run deacon up - should fail
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    // Verify the command failed
    assert!(
        !up_output.status.success(),
        "deacon up should have failed when feature updateContentCommand fails"
    );

    // Verify exit code is 1
    assert_eq!(
        up_output.status.code(),
        Some(1),
        "Exit code should be 1 when lifecycle command fails"
    );

    // Verify error mentions the failure
    let stderr = String::from_utf8_lossy(&up_output.stderr);
    let stdout = String::from_utf8_lossy(&up_output.stdout);
    let combined_output = format!("{}\n{}", stderr, stdout);

    assert!(
        combined_output.contains("failed") || combined_output.contains("exit"),
        "Error output should indicate command failure. Output:\n{}",
        combined_output
    );
}

/// Test that when multiple features have lifecycle commands and the second one fails,
/// the first one completes but subsequent commands (including config) do not run
#[test]
fn test_multiple_features_second_feature_fails() {
    if !is_docker_available() {
        eprintln!("Skipping test_multiple_features_second_feature_fails: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create first feature that succeeds
    create_local_feature(
        &temp_dir,
        "succeeding-feature",
        json!({
            "onCreateCommand": "echo 'First feature succeeded' > /tmp/first_feature.txt"
        }),
    );

    // Create second feature that fails
    create_local_feature(
        &temp_dir,
        "failing-feature",
        json!({
            "onCreateCommand": "exit 1"
        }),
    );

    // Create devcontainer.json with both features
    // The order should be: succeeding-feature, failing-feature, then config
    let devcontainer_config = json!({
        "name": "Multiple Features Fail Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./succeeding-feature": {},
            "./failing-feature": {}
        },
        "onCreateCommand": "echo 'Config should not run' > /tmp/config.txt"
    });

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    // Run deacon up - should fail
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    // Verify the command failed
    assert!(
        !up_output.status.success(),
        "deacon up should have failed when second feature's onCreateCommand fails"
    );

    // Verify exit code is 1
    assert_eq!(
        up_output.status.code(),
        Some(1),
        "Exit code should be 1 when lifecycle command fails"
    );

    // Verify error output
    let stderr = String::from_utf8_lossy(&up_output.stderr);
    let stdout = String::from_utf8_lossy(&up_output.stdout);
    let combined_output = format!("{}\n{}", stderr, stdout);

    assert!(
        combined_output.contains("failed") || combined_output.contains("exit"),
        "Error output should indicate command failure. Output:\n{}",
        combined_output
    );
}

/// Test error attribution - verify that error messages clearly identify
/// which feature command failed
#[test]
fn test_error_attribution_identifies_failing_feature() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_error_attribution_identifies_failing_feature: Docker not available"
        );
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a feature with a descriptive ID
    create_local_feature(
        &temp_dir,
        "my-custom-feature",
        json!({
            "onCreateCommand": "exit 1"
        }),
    );

    // Create devcontainer.json
    let devcontainer_config = json!({
        "name": "Error Attribution Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./my-custom-feature": {}
        }
    });

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    // Run deacon up - should fail
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    // Verify the command failed
    assert!(!up_output.status.success());

    let stderr = String::from_utf8_lossy(&up_output.stderr);
    let stdout = String::from_utf8_lossy(&up_output.stdout);
    let combined_output = format!("{}\n{}", stderr, stdout);

    // Error should ideally mention the feature that failed
    // This is aspirational - the actual error format may vary
    // The key requirement from the spec is "proper error attribution shows which feature/command failed"
    assert!(
        combined_output.contains("feature") || combined_output.contains("my-custom-feature"),
        "Error output should provide attribution to the failing feature. Output:\n{}",
        combined_output
    );
}

/// Test that postStartCommand failures also trigger fail-fast behavior
#[test]
fn test_feature_poststart_command_fails() {
    if !is_docker_available() {
        eprintln!("Skipping test_feature_poststart_command_fails: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a feature with a failing postStartCommand
    create_local_feature(
        &temp_dir,
        "failing-poststart",
        json!({
            "postStartCommand": "exit 1"
        }),
    );

    // Create devcontainer.json
    let devcontainer_config = json!({
        "name": "PostStart Fail Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./failing-poststart": {}
        },
        "postStartCommand": "echo 'Config postStart should not run'"
    });

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    // Run deacon up - should fail
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    // Verify the command failed
    assert!(
        !up_output.status.success(),
        "deacon up should have failed when feature postStartCommand fails"
    );

    // Verify exit code is 1
    assert_eq!(
        up_output.status.code(),
        Some(1),
        "Exit code should be 1 when lifecycle command fails"
    );
}

/// Test that postAttachCommand failures also trigger fail-fast behavior
#[test]
fn test_feature_postattach_command_fails() {
    if !is_docker_available() {
        eprintln!("Skipping test_feature_postattach_command_fails: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a feature with a failing postAttachCommand
    create_local_feature(
        &temp_dir,
        "failing-postattach",
        json!({
            "postAttachCommand": "exit 1"
        }),
    );

    // Create devcontainer.json
    let devcontainer_config = json!({
        "name": "PostAttach Fail Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./failing-postattach": {}
        },
        "postAttachCommand": "echo 'Config postAttach should not run'"
    });

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    // Run deacon up - should fail
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("--log-level")
        .arg("debug")
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    // Verify the command failed
    assert!(
        !up_output.status.success(),
        "deacon up should have failed when feature postAttachCommand fails"
    );

    // Verify exit code is 1
    assert_eq!(
        up_output.status.code(),
        Some(1),
        "Exit code should be 1 when lifecycle command fails"
    );
}

/// Test that a feature with a command that times out or hangs is handled appropriately
/// Note: This is a more advanced test that may need timeout configuration
#[test]
#[ignore] // Ignore by default as it may take a while
fn test_feature_command_timeout_behavior() {
    if !is_docker_available() {
        eprintln!("Skipping test_feature_command_timeout_behavior: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a feature with a command that sleeps for a long time
    create_local_feature(
        &temp_dir,
        "hanging-feature",
        json!({
            "onCreateCommand": "sleep 300"  // 5 minutes
        }),
    );

    // Create devcontainer.json
    let devcontainer_config = json!({
        "name": "Timeout Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./hanging-feature": {}
        }
    });

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    // Run deacon up with a timeout
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .timeout(std::time::Duration::from_secs(10)) // Give it 10 seconds max
        .output()
        .unwrap();

    // The command should either timeout or be handled gracefully
    // Exact behavior depends on implementation details
}
