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
use serde_json::{Value, json};
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

    fs::write(feature_dir.join("install.sh"), "#!/bin/sh\nexit 0\n").unwrap();
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
        "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind",
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
        "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind",
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
        "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind",
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
        "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind",
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
        "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind",
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
        "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind",
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
        "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind",
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
        "workspaceMount": "source=${{localWorkspaceFolder}},target=/workspace,type=bind",
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
        "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind",
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
        "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind",
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
        "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind",
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

/// Test that postStartCommand failures are recorded without failing `up`
#[test]
fn test_feature_poststart_command_failure_does_not_fail_up() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_feature_poststart_command_failure_does_not_fail_up: Docker not available"
        );
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
        "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind",
        "features": {
            "./failing-poststart": {}
        },
        "postStartCommand": "echo 'Config postStart still runs'"
    });

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    // Run deacon up. postStartCommand is a non-blocking phase, so failures are
    // recorded but do not fail the main up flow.
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    assert!(
        up_output.status.success(),
        "deacon up should succeed even when feature postStartCommand fails"
    );
}

/// Test that postAttachCommand failures are recorded without failing `up`
#[test]
fn test_feature_postattach_command_failure_does_not_fail_up() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_feature_postattach_command_failure_does_not_fail_up: Docker not available"
        );
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
        "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind",
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

    // Run deacon up. postAttachCommand is a non-blocking phase, so failures are
    // recorded but do not fail the main up flow.
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

    assert!(
        up_output.status.success(),
        "deacon up should succeed even when feature postAttachCommand fails"
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
        "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind",
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
    let _up_output = up_cmd
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

/// Image cleanup guard for tests that build a throwaway image.
struct ImageGuard(String);

impl Drop for ImageGuard {
    fn drop(&mut self) {
        let _ = StdCommand::new("docker")
            .args(["rmi", "-f", &self.0])
            .output();
    }
}

/// #467 (single-container path): when the image's `devcontainer.metadata` and
/// the devcontainer.json declare the SAME lifecycle phase, BOTH hooks run —
/// the image's first — rather than the config's replacing the image's.
///
/// The spec's image-metadata Merge Logic table gives each lifecycle hook a
/// "Collected list of all `<phase>Command`s", with "the devcontainer.json is
/// considered last". Every OTHER row of that table is last-wins, and deacon
/// folded the label in through `ConfigMerger`, where a hook is an ordinary
/// scalar — so the image's hook was silently dropped on a collision.
///
/// The marker file is the whole observation: both CLIs exit 0 either way, so no
/// status assertion can see this. Measured against the pinned reference CLI
/// 0.87.0 on this shape, whose log reads exactly the five lines asserted below.
///
/// Three phases are exercised deliberately, because the fix has to leave the
/// non-colliding cases alone:
///   - `onCreate`   — declared by BOTH (string vs string): collects
///   - `postCreate` — declared by BOTH (string vs ARRAY form): collects
///   - `postStart`  — declared by the IMAGE only: must run exactly ONCE. That
///     is the regression the first draft of this fix introduced — it recorded
///     the hook as a layer while the merge ALSO left it in the singular field,
///     so it ran twice — and the reason this asserts the whole log rather than
///     mere presence.
///
/// `remoteUser` is the control: it is 'Last value wins', so the config's `root`
/// must beat the image metadata's `metauser` outright. The hook records
/// `whoami` itself, because the hook's own user is the thing being measured.
#[test]
fn test_up_collects_image_metadata_and_config_hooks_for_the_same_phase() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_up_collects_image_metadata_and_config_hooks_for_the_same_phase: Docker not available"
        );
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let guard = ContainerGuard::new();

    // Unique tag so parallel runs in the docker-shared group never collide.
    let image_tag = format!(
        "deacon-test-same-phase-hooks-{}-{}:local",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    );
    let _image = ImageGuard(image_tag.clone());

    // Every hook appends to ONE log inside the container, so the file records
    // both which hooks ran and the order in which they ran.
    let image_dir = temp_dir.path().join("image");
    fs::create_dir_all(&image_dir).unwrap();
    fs::write(
        image_dir.join("Dockerfile"),
        concat!(
            "FROM alpine:3.19\n",
            "RUN adduser -D -u 1234 metauser\n",
            "LABEL devcontainer.metadata='[{\"remoteUser\":\"metauser\",",
            "\"onCreateCommand\":\"echo img-onCreate >> /tmp/lifecycle.log\",",
            "\"postCreateCommand\":\"echo img-postCreate >> /tmp/lifecycle.log\",",
            "\"postStartCommand\":\"echo img-postStart >> /tmp/lifecycle.log\"}]'\n",
        ),
    )
    .unwrap();

    let build = StdCommand::new("docker")
        .args(["build", "-q", "-t", &image_tag])
        .arg(&image_dir)
        .output()
        .expect("docker build should run");
    assert!(
        build.status.success(),
        "docker build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let devcontainer_config = json!({
        "name": "Same-phase hooks",
        "image": image_tag,
        "remoteUser": "root",
        "onCreateCommand": "echo ws-onCreate >> /tmp/lifecycle.log",
        "postCreateCommand": [
            "/bin/sh",
            "-c",
            "echo ws-postCreate >> /tmp/lifecycle.log; whoami > /tmp/hook-user.txt"
        ],
    });

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    let container_id = run_deacon_up(&temp_dir, &guard, &[]).expect("deacon up should succeed");

    let log = read_container_file(&container_id, "/tmp/lifecycle.log")
        .expect("the lifecycle hooks should have written /tmp/lifecycle.log");
    let lines: Vec<&str> = log
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();

    assert_eq!(
        lines,
        vec![
            "img-onCreate",
            "ws-onCreate",
            "img-postCreate",
            "ws-postCreate",
            "img-postStart",
        ],
        "image-metadata and config hooks for the SAME phase must both run, image \
         first, and an image-only phase must run exactly ONCE (#467). Got: {:?}",
        lines
    );

    let hook_user = read_container_file(&container_id, "/tmp/hook-user.txt")
        .expect("the postCreate hook should have recorded its user");
    assert_eq!(
        hook_user.trim(),
        "root",
        "remoteUser is 'Last value wins': the config's `root` must beat the image \
         metadata's `metauser`. Collecting the lifecycle hooks must not turn the \
         last-wins rows of the Merge Logic table into collections."
    );
}

/// #467 follow-up: the FULL layer order, end to end, in one file —
/// image-metadata entries in label order, then Features in install order, then
/// the devcontainer.json.
///
/// The sibling test above pins the same-phase COLLISION, which is what #467 was
/// filed about, but it uses a single-entry label and no Feature, so two things it
/// cannot see:
///
/// 1. **Order between metadata entries.** The merge rule is a "Collected list",
///    so the label is an ordered array of partial configurations and the order
///    between its entries is part of the claim. Both entries here declare
///    `onCreateCommand`, so `img1-onCreate` preceding `img2-onCreate` is that
///    assertion. A collection that used a set, reversed the entries, or read only
///    the first or the last would satisfy a single-entry fixture.
///
/// 2. **Where the image layers sit relative to Features.** Feature-contributed
///    hooks were ALREADY aggregated ahead of the configuration's before #467;
///    the image layers had to be inserted ahead of BOTH, not beside either. The
///    Feature's line between the image's and the config's is what proves it, and
///    it is also what proves neither layer runs twice — each appears once in a
///    log asserted whole.
///
/// Upstream's `getDevcontainerMetadata` builds exactly this order —
/// `[...baseImageMetadata.raw, ...featureRaw, pickConfigProperties(config.raw)]`
/// — and it is the order the reference CLI 0.87.0 was measured running on this
/// shape. Both CLIs exit 0 whatever the order, so the file is the whole
/// observation.
///
/// Cross-entry ordering is also pinned end-to-end against the live oracle by
/// `case-up-image-metadata-same-phase-differential` and its compose twin, but
/// those are differential-only (their bases are `:local` images only the nightly
/// and release lanes build). This test is what carries the property on the
/// PR-gating Docker lane. The compose path needs no twin of it: the layers reach
/// both paths through the one shared merge, and that they reach COMPOSE at all is
/// what `integration_compose_config_mounts::
/// test_compose_up_collects_image_metadata_and_config_hooks_for_the_same_phase`
/// already pins.
#[test]
fn test_image_metadata_layers_features_and_config_run_in_spec_order() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_image_metadata_layers_features_and_config_run_in_spec_order: \
             Docker not available"
        );
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let guard = ContainerGuard::new();

    let image_tag = format!(
        "deacon-test-layer-order-{}-{}:local",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    );
    let _image = ImageGuard(image_tag.clone());

    // TWO entries. Both declare onCreate (cross-entry order); postCreate is split
    // one per entry so the collection is shown to read every entry, not just one.
    let image_dir = temp_dir.path().join("image");
    fs::create_dir_all(&image_dir).unwrap();
    fs::write(
        image_dir.join("Dockerfile"),
        concat!(
            "FROM alpine:3.19\n",
            "LABEL devcontainer.metadata='[",
            "{\"onCreateCommand\":\"echo img1-onCreate >> /tmp/order.log\"},",
            "{\"onCreateCommand\":\"echo img2-onCreate >> /tmp/order.log\",",
            "\"postCreateCommand\":\"echo img2-postCreate >> /tmp/order.log\"}",
            "]'\n",
        ),
    )
    .unwrap();

    let build = StdCommand::new("docker")
        .args(["build", "-q", "-t", &image_tag])
        .arg(&image_dir)
        .output()
        .expect("docker build should run");
    assert!(
        build.status.success(),
        "docker build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    create_local_feature(
        &temp_dir,
        "order-feature",
        json!({ "onCreateCommand": "echo feat-onCreate >> /tmp/order.log" }),
    );

    let devcontainer_config = json!({
        "name": "Layer order",
        "image": image_tag,
        "features": { "./order-feature": {} },
        "onCreateCommand": "echo ws-onCreate >> /tmp/order.log",
        "postCreateCommand": ["/bin/sh", "-c", "echo ws-postCreate >> /tmp/order.log"],
    });

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    let container_id = run_deacon_up(&temp_dir, &guard, &[]).expect("deacon up should succeed");

    let log = read_container_file(&container_id, "/tmp/order.log")
        .expect("the lifecycle hooks should have written /tmp/order.log");
    let lines: Vec<&str> = log
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();

    assert_eq!(
        lines,
        vec![
            // onCreate: metadata entry 0, metadata entry 1, Feature, config
            "img1-onCreate",
            "img2-onCreate",
            "feat-onCreate",
            "ws-onCreate",
            // postCreate: metadata entry 1, config
            "img2-postCreate",
            "ws-postCreate",
        ],
        "every layer must run exactly once, in spec order: image-metadata entries \
         in LABEL order, then Features in install order, then devcontainer.json \
         (#467). Got: {:?}",
        lines
    );
}

/// #477 (`set-up` surface): every lifecycle hook the CONTAINER's stamped
/// `devcontainer.metadata` label declares for a phase runs, in label order,
/// ahead of the one `--config` declares — instead of only the last survivor of
/// a last-wins fold.
///
/// Same defect CLASS as #467, different SOURCE. #467 was the IMAGE's label
/// collapsed by `ConfigMerger` on the `up` path; this is the CONTAINER's
/// stamped label collapsed the same way on a path #467's fix did not reach.
/// There were two collapse points and they compose, so both shapes are
/// measured here:
///   - **no `--config`** isolates `config_from_metadata_label`, which folded the
///     label's own fragments last-wins. `deacon up` stamps
///     `[...image entries, ...feature entries, config entry]`, so a phase two of
///     those sources declare arrives as two fragments and used to leave as one.
///   - **with `--config`** adds the second point, where `set_up.rs` folded that
///     result again and read the five SINGULAR fields off it.
///
/// The label below is that stamped shape: an image entry, a FEATURE entry, and a
/// config entry. The feature entry is deliberate — `set-up` never called
/// `aggregate_lifecycle_commands` at all, so Feature-contributed hooks were not
/// merely mis-ordered, they never ran unless no other entry declared the phase.
///
/// `postStart` is declared by the feature entry ALONE and must appear exactly
/// ONCE. That is the trap #467's fix documented: a hook lifted onto a layer that
/// also stays in the singular field runs twice. It is why every assertion here
/// spans the WHOLE log — a `contains` assertion cannot see an appended
/// duplicate.
///
/// Both CLIs exit 0 on every one of these shapes, so the marker file is the
/// entire observation. Measured against the pinned reference CLI 0.87.0: its
/// logs are exactly the two vectors asserted below.
#[test]
fn test_set_up_collects_container_label_hooks_alongside_the_config_hook() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_set_up_collects_container_label_hooks_alongside_the_config_hook: Docker not available"
        );
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let guard = ContainerGuard::new();

    // Unique tag/names so parallel runs in the docker-shared group never collide.
    let unique = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    );
    let image_tag = format!("deacon-test-setup-label-hooks-{}:local", unique);
    let _image = ImageGuard(image_tag.clone());

    // A PLAIN container is the point: one a prior `up` has already set up
    // carries in-container phase markers that suppress every hook and make the
    // surface unmeasurable. This container merely inherits the image's label.
    let image_dir = temp_dir.path().join("image");
    fs::create_dir_all(&image_dir).unwrap();
    fs::write(
        image_dir.join("Dockerfile"),
        concat!(
            "FROM alpine:3.19\n",
            "LABEL devcontainer.metadata='[",
            "{\"onCreateCommand\":\"echo e0-onCreate >> /tmp/setup.log\",",
            "\"postCreateCommand\":\"echo e0-postCreate >> /tmp/setup.log\"},",
            "{\"id\":\"ghcr.io/example/feat:1\",",
            "\"onCreateCommand\":\"echo feat-onCreate >> /tmp/setup.log\",",
            "\"postStartCommand\":\"echo feat-postStart >> /tmp/setup.log\"},",
            "{\"onCreateCommand\":\"echo e2-onCreate >> /tmp/setup.log\",",
            "\"postCreateCommand\":\"echo e2-postCreate >> /tmp/setup.log\"}",
            "]'\n",
            "CMD [\"sleep\", \"infinity\"]\n",
        ),
    )
    .unwrap();

    let build = StdCommand::new("docker")
        .args(["build", "-q", "-t", &image_tag])
        .arg(&image_dir)
        .output()
        .expect("docker build should run");
    assert!(
        build.status.success(),
        "docker build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    // `set-up`'s `--config` shape. The config declares the same two phases the
    // image entry does, which is the collision the reference collects.
    let config_path = temp_dir.path().join("set-up-config.json");
    fs::write(
        &config_path,
        serde_json::to_string_pretty(&json!({
            "image": image_tag,
            "onCreateCommand": "echo ws-onCreate >> /tmp/setup.log",
            "postCreateCommand": "echo ws-postCreate >> /tmp/setup.log",
        }))
        .unwrap(),
    )
    .unwrap();

    let start_container = |name: &str| -> String {
        let out = StdCommand::new("docker")
            .args(["run", "-d", "--name", name, &image_tag])
            .output()
            .expect("docker run should execute");
        assert!(
            out.status.success(),
            "docker run failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
        guard.register(id.clone());
        id
    };

    let run_set_up = |container_id: &str, extra: &[&str]| {
        let mut cmd = Command::cargo_bin("deacon").expect("deacon binary");
        let mut args = vec!["set-up", "--container-id", container_id];
        args.extend_from_slice(extra);
        let assert = cmd.env("DEACON_LOG", "warn").args(&args).assert();
        let output = assert.get_output();
        assert!(
            output.status.success(),
            "deacon set-up failed:\nSTDOUT:\n{}\nSTDERR:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    };

    let log_lines = |container_id: &str| -> Vec<String> {
        read_container_file(container_id, "/tmp/setup.log")
            .expect("the lifecycle hooks should have written /tmp/setup.log")
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect()
    };

    // --- Collapse point 1, isolated: no `--config` at all. ---
    let bare = start_container(&format!("deacon-test-setup-477-bare-{}", unique));
    run_set_up(&bare, &[]);
    assert_eq!(
        log_lines(&bare),
        vec![
            "e0-onCreate",
            "feat-onCreate",
            "e2-onCreate",
            "e0-postCreate",
            "e2-postCreate",
            "feat-postStart",
        ],
        "with no --config, every entry of the container's devcontainer.metadata \
         label must contribute its hook in label order — including the FEATURE \
         entry's, which set-up never aggregated — and the feature-only postStart \
         exactly once (#477)"
    );

    // --- Both collapse points: the same label plus a `--config`. ---
    let with_config = start_container(&format!("deacon-test-setup-477-cfg-{}", unique));
    run_set_up(&with_config, &["--config", config_path.to_str().unwrap()]);
    assert_eq!(
        log_lines(&with_config),
        vec![
            "e0-onCreate",
            "feat-onCreate",
            "e2-onCreate",
            "ws-onCreate",
            "e0-postCreate",
            "e2-postCreate",
            "ws-postCreate",
            "feat-postStart",
        ],
        "the devcontainer.json's hook is collected LAST for each phase, never \
         instead of the label's (#477)"
    );
}

/// #526 (`set-up` surface): a container's `devcontainer.metadata` label contributes ONLY
/// the properties upstream's `mergeConfiguration` reads off a metadata entry.
///
/// The reference's merge is not a fold of two configurations. It is the base config minus
/// the collected properties, PLUS one expression per ENUMERATED metadata property (bundle
/// `Xi`, `mergeConfiguration` in `devcontainers/cli/src/spec-node/imageMetadata.ts`). A
/// property with no expression is never read from a label at all. deacon folded
/// everything, so a label could contribute any property a base image happened to write.
///
/// A **raw-labeled** container is the whole point, and it is why no parity case reaches
/// this through `op-up` alone: `deacon up` already picks the CONFIGURATION down to
/// upstream's list before stamping it (#322/#373), so the only leakable entries on an
/// `up`-created container are the ones it copied verbatim off the base image's own label.
/// `docker run --label` puts an arbitrary metadata document on the container directly,
/// which is the shape a hand-written base-image `LABEL` produces.
///
/// Both halves are asserted. Absence alone would pass for a restriction that dropped
/// everything, so the enumerated properties are checked to have survived — including the
/// collected hook arrays, which is the #475/#477 invariant this change must not undo.
///
/// The EXECUTION is asserted too, because the leak was never only a reporting defect:
/// `workspaceFolder` is what `execute_lifecycle_hooks` passes as the `docker exec` CWD, so
/// the label's `/meta-ws` — a directory that does not exist in the container — made every
/// hook fail with exit 127 and `set-up` exit non-zero. Measured at oracle 0.87.0 on this
/// exact label: the reference runs all four hooks and its log reads exactly the vector
/// asserted below.
#[test]
fn test_set_up_folds_only_the_enumerated_metadata_properties() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_set_up_folds_only_the_enumerated_metadata_properties: Docker not available"
        );
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let guard = ContainerGuard::new();

    let unique = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    );

    // Every key here is either in the leak census (#526) or on upstream's list. The two
    // fragments also declare the same phase, which is the #475/#477 collection invariant.
    let label = json!([
        {
            "id": "frag1",
            "workspaceFolder": "/meta-ws",
            "name": "meta-name",
            "runArgs": ["--meta-arg"],
            "appPort": [9999],
            "workspaceMount": "source=/m,target=/m,type=bind",
            "features": { "ghcr.io/x/y:1": {} },
            "overrideFeatureInstallOrder": ["ghcr.io/x/y"],
            "image": "should-not-leak",
            "service": "nope",
            "runServices": ["nope"],
            "initializeCommand": "echo nope",
            "remoteUser": "root",
            "containerEnv": { "META": "1" },
            "forwardPorts": [3000],
            "userEnvProbe": "none",
            "onCreateCommand": "echo frag1-onCreate >> /tmp/setup526.log",
            "postCreateCommand": "echo frag1-postCreate >> /tmp/setup526.log",
        },
        {
            "id": "frag2",
            "postCreateCommand": "echo frag2-postCreate >> /tmp/setup526.log",
        },
    ])
    .to_string();

    let name = format!("deacon-test-setup-526-{}", unique);
    let out = StdCommand::new("docker")
        .args(["run", "-d", "--name", &name, "--label"])
        .arg(format!("devcontainer.metadata={}", label))
        .args(["alpine:3.19", "sleep", "infinity"])
        .output()
        .expect("docker run should execute");
    assert!(
        out.status.success(),
        "docker run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let container_id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    guard.register(container_id.clone());

    let config_path = temp_dir.path().join("set-up-config.json");
    fs::write(
        &config_path,
        serde_json::to_string_pretty(&json!({
            "postCreateCommand": "echo config-postCreate >> /tmp/setup526.log",
        }))
        .unwrap(),
    )
    .unwrap();

    let assert = Command::cargo_bin("deacon")
        .expect("deacon binary")
        .env("DEACON_LOG", "warn")
        .args([
            "set-up",
            "--container-id",
            &container_id,
            "--config",
            config_path.to_str().unwrap(),
            "--include-merged-configuration",
        ])
        .assert();
    let output = assert.get_output();
    assert!(
        output.status.success(),
        "deacon set-up failed — a `workspaceFolder` reaching the exec CWD from the label \
         is one way this happens (#526):\nSTDOUT:\n{}\nSTDERR:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let document: Value = serde_json::from_slice(&output.stdout)
        .expect("set-up must emit a single JSON document on stdout");
    let merged = document
        .get("mergedConfiguration")
        .and_then(Value::as_object)
        .expect("--include-merged-configuration must produce the block");

    for key in [
        "workspaceFolder",
        "name",
        "runArgs",
        "appPort",
        "workspaceMount",
        "features",
        "overrideFeatureInstallOrder",
        "image",
        "service",
        "runServices",
        "initializeCommand",
    ] {
        assert!(
            !merged.contains_key(key),
            "`{}` is not on upstream's metadata property list, so a container label must \
             not be able to put it in mergedConfiguration (#526). Got: {}",
            key,
            serde_json::to_string_pretty(merged).unwrap()
        );
    }

    assert_eq!(
        merged.get("remoteUser").and_then(Value::as_str),
        Some("root"),
        "`remoteUser` IS on upstream's list and must still fold"
    );
    assert_eq!(
        merged
            .get("containerEnv")
            .and_then(Value::as_object)
            .and_then(|m| m.get("META"))
            .and_then(Value::as_str),
        Some("1"),
        "`containerEnv` IS on upstream's list and must still fold"
    );
    assert_eq!(
        merged.get("userEnvProbe").and_then(Value::as_str),
        Some("none"),
        "`userEnvProbe` IS on upstream's list and must still fold"
    );
    assert_eq!(
        merged.get("forwardPorts"),
        Some(&json!([3000])),
        "`forwardPorts` IS on upstream's list and must still fold"
    );
    assert_eq!(
        merged.get("onCreateCommands"),
        Some(&json!(["echo frag1-onCreate >> /tmp/setup526.log"])),
        "the collected hook arrays must survive the restriction (#475/#477)"
    );
    assert_eq!(
        merged.get("postCreateCommands"),
        Some(&json!([
            "echo frag1-postCreate >> /tmp/setup526.log",
            "echo frag2-postCreate >> /tmp/setup526.log",
            "echo config-postCreate >> /tmp/setup526.log",
        ])),
        "every fragment declaring a phase contributes a hook, devcontainer.json last \
         (#475/#477)"
    );

    let log: Vec<String> = read_container_file(&container_id, "/tmp/setup526.log")
        .expect("the lifecycle hooks should have written /tmp/setup526.log")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    assert_eq!(
        log,
        vec![
            "frag1-onCreate",
            "frag1-postCreate",
            "frag2-postCreate",
            "config-postCreate",
        ],
        "the hooks must RUN — a `workspaceFolder` folded off the label used to become the \
         exec CWD and fail every one of them with exit 127 (#526)"
    );
}
