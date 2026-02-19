//! Integration tests for feature mount merging and application
//!
//! These tests verify that mounts declared in features are properly merged with
//! config mounts and applied to containers.
//!
//! This is part of User Story 3: Feature Mounts Applied to Container
//! from specs/009-complete-feature-support/spec.md
//!
//! Test Coverage:
//! - T031: Integration tests for mount merging (docker-shared group)
//!
//! ## Test Scenarios
//! 1. Feature mount is applied to container
//! 2. Config mount takes precedence over feature mount for same target
//! 3. Multiple features with different mounts are merged correctly
//! 4. Mount parsing errors are attributed to the correct feature

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

/// Helper to create a simple local feature with mounts
fn create_local_feature_with_mounts(
    temp_dir: &TempDir,
    feature_name: &str,
    mounts: serde_json::Value,
) {
    let feature_dir = temp_dir.path().join(".devcontainer").join(feature_name);
    fs::create_dir_all(&feature_dir).unwrap();

    let feature_json = json!({
        "id": feature_name,
        "version": "1.0.0",
        "name": format!("Test Feature {}", feature_name),
        "mounts": mounts
    });

    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        serde_json::to_string_pretty(&feature_json).unwrap(),
    )
    .unwrap();

    // Create a minimal install script
    let install_script = r#"#!/bin/sh
set -e
echo "Installing feature..."
"#;

    fs::write(feature_dir.join("install.sh"), install_script).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(feature_dir.join("install.sh"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(feature_dir.join("install.sh"), perms).unwrap();
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

/// Inspect Docker container and return the inspection JSON
fn inspect_container(container_id: &str) -> Result<Value, String> {
    let output = StdCommand::new("docker")
        .args(["inspect", container_id])
        .output()
        .map_err(|e| format!("Failed to run docker inspect: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "docker inspect failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let inspect_json = String::from_utf8_lossy(&output.stdout);
    let inspect_array: Vec<Value> = serde_json::from_str(&inspect_json)
        .map_err(|e| format!("Failed to parse inspect JSON: {}", e))?;

    inspect_array
        .into_iter()
        .next()
        .ok_or_else(|| "docker inspect returned empty array".to_string())
}

/// Check if a path is mounted in the container
fn check_mount_exists(container_id: &str, target: &str) -> bool {
    let inspect = match inspect_container(container_id) {
        Ok(json) => json,
        Err(_) => return false,
    };

    if let Some(mounts) = inspect["Mounts"].as_array() {
        for mount in mounts {
            if let Some(destination) = mount["Destination"].as_str() {
                if destination == target {
                    return true;
                }
            }
        }
    }

    false
}

/// Get mount details for a specific target path
fn get_mount_details(container_id: &str, target: &str) -> Option<Value> {
    let inspect = inspect_container(container_id).ok()?;

    if let Some(mounts) = inspect["Mounts"].as_array() {
        for mount in mounts {
            if let Some(destination) = mount["Destination"].as_str() {
                if destination == target {
                    return Some(mount.clone());
                }
            }
        }
    }

    None
}

// ============================================================================
// T031: Integration tests for mount merging (docker-shared group)
// ============================================================================

/// Test that a feature declaring a volume mount has the mount applied to the container
#[test]
fn test_feature_volume_mount_applied_to_container() {
    if !is_docker_available() {
        eprintln!("Skipping test_feature_volume_mount_applied_to_container: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a feature with a volume mount
    create_local_feature_with_mounts(
        &temp_dir,
        "volume-feature",
        json!(["type=volume,source=mydata,target=/data"]),
    );

    // Create devcontainer.json using the feature
    let devcontainer_config = json!({
        "name": "Volume Mount Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./volume-feature": {}
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

    // Verify the mount exists in the container
    assert!(
        check_mount_exists(&container_id, "/data"),
        "Volume mount from feature should be applied to container at /data"
    );

    // Verify mount details
    let mount_details = get_mount_details(&container_id, "/data")
        .expect("Mount details should be available for /data");

    assert_eq!(
        mount_details["Type"].as_str(),
        Some("volume"),
        "Mount type should be volume"
    );

    assert_eq!(
        mount_details["Name"].as_str(),
        Some("mydata"),
        "Volume name should be mydata"
    );

    println!("✓ Feature volume mount successfully applied to container");
}

/// Test that a feature declaring a bind mount has the mount applied to the container
#[test]
fn test_feature_bind_mount_applied_to_container() {
    if !is_docker_available() {
        eprintln!("Skipping test_feature_bind_mount_applied_to_container: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a host directory to bind mount
    let host_data_dir = temp_dir.path().join("host-data");
    fs::create_dir_all(&host_data_dir).unwrap();
    fs::write(host_data_dir.join("test.txt"), "test data").unwrap();

    // Create a feature with a bind mount
    let host_data_path = host_data_dir.to_str().unwrap();
    create_local_feature_with_mounts(
        &temp_dir,
        "bind-feature",
        json!([format!(
            "type=bind,source={},target=/mnt/data",
            host_data_path
        )]),
    );

    // Create devcontainer.json using the feature
    let devcontainer_config = json!({
        "name": "Bind Mount Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./bind-feature": {}
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

    // Verify the mount exists in the container
    assert!(
        check_mount_exists(&container_id, "/mnt/data"),
        "Bind mount from feature should be applied to container at /mnt/data"
    );

    // Verify mount details
    let mount_details = get_mount_details(&container_id, "/mnt/data")
        .expect("Mount details should be available for /mnt/data");

    assert_eq!(
        mount_details["Type"].as_str(),
        Some("bind"),
        "Mount type should be bind"
    );

    println!("✓ Feature bind mount successfully applied to container");
}

/// Test that config mount takes precedence over feature mount when both target the same path
#[test]
fn test_config_mount_precedence_over_feature_mount() {
    if !is_docker_available() {
        eprintln!("Skipping test_config_mount_precedence_over_feature_mount: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a feature with a volume mount at /shared
    create_local_feature_with_mounts(
        &temp_dir,
        "feature-mount",
        json!(["type=volume,source=feature-vol,target=/shared"]),
    );

    // Create devcontainer.json with BOTH feature mount AND config mount to same target
    // Config mount should take precedence
    let devcontainer_config = json!({
        "name": "Mount Precedence Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./feature-mount": {}
        },
        "mounts": [
            "type=volume,source=config-vol,target=/shared"
        ]
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

    // Verify the mount exists at /shared
    assert!(
        check_mount_exists(&container_id, "/shared"),
        "Mount should exist at /shared"
    );

    // Verify the mount uses the config volume (config-vol), not the feature volume (feature-vol)
    let mount_details =
        get_mount_details(&container_id, "/shared").expect("Mount details should be available");

    assert_eq!(
        mount_details["Type"].as_str(),
        Some("volume"),
        "Mount type should be volume"
    );

    assert_eq!(
        mount_details["Name"].as_str(),
        Some("config-vol"),
        "Config mount should take precedence - volume should be 'config-vol', not 'feature-vol'"
    );

    println!("✓ Config mount takes precedence over feature mount for same target path");
}

/// Test that multiple features with different mounts all get applied
#[test]
fn test_multiple_features_with_different_mounts() {
    if !is_docker_available() {
        eprintln!("Skipping test_multiple_features_with_different_mounts: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create first feature with a volume mount
    create_local_feature_with_mounts(
        &temp_dir,
        "feature-a",
        json!(["type=volume,source=vol-a,target=/data-a"]),
    );

    // Create second feature with a different volume mount
    create_local_feature_with_mounts(
        &temp_dir,
        "feature-b",
        json!(["type=volume,source=vol-b,target=/data-b"]),
    );

    // Create third feature with a tmpfs mount
    create_local_feature_with_mounts(
        &temp_dir,
        "feature-c",
        json!(["type=tmpfs,target=/tmp-data"]),
    );

    // Create devcontainer.json using all three features
    let devcontainer_config = json!({
        "name": "Multiple Feature Mounts Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./feature-a": {},
            "./feature-b": {},
            "./feature-c": {}
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

    // Verify all three mounts exist
    assert!(
        check_mount_exists(&container_id, "/data-a"),
        "Feature A mount should exist at /data-a"
    );

    assert!(
        check_mount_exists(&container_id, "/data-b"),
        "Feature B mount should exist at /data-b"
    );

    assert!(
        check_mount_exists(&container_id, "/tmp-data"),
        "Feature C mount should exist at /tmp-data"
    );

    // Verify mount types
    let mount_a = get_mount_details(&container_id, "/data-a").expect("Mount A should exist");
    assert_eq!(mount_a["Type"].as_str(), Some("volume"));
    assert_eq!(mount_a["Name"].as_str(), Some("vol-a"));

    let mount_b = get_mount_details(&container_id, "/data-b").expect("Mount B should exist");
    assert_eq!(mount_b["Type"].as_str(), Some("volume"));
    assert_eq!(mount_b["Name"].as_str(), Some("vol-b"));

    let mount_c = get_mount_details(&container_id, "/tmp-data").expect("Mount C should exist");
    assert_eq!(mount_c["Type"].as_str(), Some("tmpfs"));

    println!("✓ Multiple features with different mounts all applied successfully");
}

/// Test that feature mounts using volume syntax (shorthand) are parsed correctly
#[test]
fn test_feature_mount_volume_syntax() {
    if !is_docker_available() {
        eprintln!("Skipping test_feature_mount_volume_syntax: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a feature with volume syntax mount (shorthand: source:target)
    create_local_feature_with_mounts(
        &temp_dir,
        "volume-syntax-feature",
        json!(["myvolume:/container/path"]),
    );

    // Create devcontainer.json
    let devcontainer_config = json!({
        "name": "Volume Syntax Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./volume-syntax-feature": {}
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

    // Verify the mount exists
    assert!(
        check_mount_exists(&container_id, "/container/path"),
        "Mount should exist at /container/path"
    );

    // Verify mount details
    let mount_details = get_mount_details(&container_id, "/container/path")
        .expect("Mount details should be available");

    assert_eq!(
        mount_details["Type"].as_str(),
        Some("volume"),
        "Mount type should be volume"
    );

    assert_eq!(
        mount_details["Name"].as_str(),
        Some("myvolume"),
        "Volume name should be myvolume"
    );

    println!("✓ Feature mount with volume syntax parsed correctly");
}

/// Test that features and config mounts are both applied when targeting different paths
#[test]
fn test_feature_and_config_mounts_merged() {
    if !is_docker_available() {
        eprintln!("Skipping test_feature_and_config_mounts_merged: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a feature with a mount
    create_local_feature_with_mounts(
        &temp_dir,
        "feature-with-mount",
        json!(["type=volume,source=feature-volume,target=/feature-data"]),
    );

    // Create devcontainer.json with both feature and config mounts
    let devcontainer_config = json!({
        "name": "Merged Mounts Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./feature-with-mount": {}
        },
        "mounts": [
            "type=volume,source=config-volume,target=/config-data"
        ]
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

    // Verify both mounts exist
    assert!(
        check_mount_exists(&container_id, "/feature-data"),
        "Feature mount should exist at /feature-data"
    );

    assert!(
        check_mount_exists(&container_id, "/config-data"),
        "Config mount should exist at /config-data"
    );

    // Verify mount sources
    let feature_mount =
        get_mount_details(&container_id, "/feature-data").expect("Feature mount should exist");
    assert_eq!(feature_mount["Name"].as_str(), Some("feature-volume"));

    let config_mount =
        get_mount_details(&container_id, "/config-data").expect("Config mount should exist");
    assert_eq!(config_mount["Name"].as_str(), Some("config-volume"));

    println!("✓ Feature and config mounts both applied when targeting different paths");
}

/// Test that multiple mounts from a single feature are all applied
#[test]
fn test_feature_with_multiple_mounts() {
    if !is_docker_available() {
        eprintln!("Skipping test_feature_with_multiple_mounts: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a feature with multiple mounts
    create_local_feature_with_mounts(
        &temp_dir,
        "multi-mount-feature",
        json!([
            "type=volume,source=cache-vol,target=/cache",
            "type=volume,source=data-vol,target=/data",
            "type=tmpfs,target=/tmp-workspace"
        ]),
    );

    // Create devcontainer.json
    let devcontainer_config = json!({
        "name": "Multi-Mount Feature Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./multi-mount-feature": {}
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

    // Verify all three mounts from the single feature exist
    assert!(
        check_mount_exists(&container_id, "/cache"),
        "Cache mount should exist"
    );

    assert!(
        check_mount_exists(&container_id, "/data"),
        "Data mount should exist"
    );

    assert!(
        check_mount_exists(&container_id, "/tmp-workspace"),
        "Tmpfs mount should exist"
    );

    println!("✓ All mounts from a single feature applied successfully");
}

/// Test that mount read-only flag is respected from feature mounts
#[test]
fn test_feature_mount_readonly_flag() {
    if !is_docker_available() {
        eprintln!("Skipping test_feature_mount_readonly_flag: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a feature with a read-only mount
    create_local_feature_with_mounts(
        &temp_dir,
        "readonly-mount-feature",
        json!(["type=volume,source=readonly-vol,target=/readonly-data,ro"]),
    );

    // Create devcontainer.json
    let devcontainer_config = json!({
        "name": "Read-Only Mount Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./readonly-mount-feature": {}
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

    // Verify the mount exists and is read-only
    let mount_details = get_mount_details(&container_id, "/readonly-data")
        .expect("Mount should exist at /readonly-data");

    assert_eq!(
        mount_details["RW"].as_bool(),
        Some(false),
        "Mount should be read-only (RW=false)"
    );

    println!("✓ Feature mount read-only flag respected");
}

/// Test that an empty mounts array in a feature doesn't cause errors
#[test]
fn test_feature_with_empty_mounts_array() {
    if !is_docker_available() {
        eprintln!("Skipping test_feature_with_empty_mounts_array: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a feature with an empty mounts array
    create_local_feature_with_mounts(&temp_dir, "empty-mounts-feature", json!([]));

    // Create devcontainer.json
    let devcontainer_config = json!({
        "name": "Empty Mounts Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace",
        "features": {
            "./empty-mounts-feature": {}
        }
    });

    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_config).unwrap(),
    )
    .unwrap();

    let guard = ContainerGuard::new();
    let _container_id = run_deacon_up(&temp_dir, &guard, &["--skip-post-create"])
        .expect("deacon up should succeed even with empty mounts array");

    println!("✓ Feature with empty mounts array doesn't cause errors");
}
