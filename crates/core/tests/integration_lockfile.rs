//! Integration tests for lockfile I/O operations
//!
//! These tests verify end-to-end lockfile functionality including
//! reading, writing, and merging lockfiles in real filesystem scenarios.

use deacon_core::lockfile::{
    get_lockfile_path, merge_lockfile_features, read_lockfile, write_lockfile, Lockfile,
    LockfileFeature,
};
use std::collections::HashMap;
use tempfile::TempDir;

#[test]
fn test_read_write_integration() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let lockfile_path = temp_dir.path().join("devcontainer-lock.json");

    // Create a lockfile with multiple features
    let mut lockfile = Lockfile {
        features: HashMap::new(),
    };

    lockfile.features.insert(
        "ghcr.io/devcontainers/features/node".to_string(),
        LockfileFeature {
            version: "1.2.3".to_string(),
            resolved: "ghcr.io/devcontainers/features/node@sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            integrity: "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            depends_on: None,
        },
    );

    lockfile.features.insert(
        "ghcr.io/devcontainers/features/docker".to_string(),
        LockfileFeature {
            version: "2.0.0".to_string(),
            resolved: "ghcr.io/devcontainers/features/docker@sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
            integrity: "sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
            depends_on: Some(vec!["ghcr.io/devcontainers/features/node".to_string()]),
        },
    );

    // Write lockfile
    write_lockfile(&lockfile_path, &lockfile, false).expect("Failed to write lockfile");

    // Verify file exists
    assert!(lockfile_path.exists());

    // Read back and verify
    let read_lockfile = read_lockfile(&lockfile_path)
        .expect("Failed to read lockfile")
        .expect("Lockfile should exist");

    assert_eq!(lockfile.features.len(), read_lockfile.features.len());
    assert_eq!(
        lockfile.features.get("ghcr.io/devcontainers/features/node"),
        read_lockfile
            .features
            .get("ghcr.io/devcontainers/features/node")
    );
    assert_eq!(
        lockfile
            .features
            .get("ghcr.io/devcontainers/features/docker"),
        read_lockfile
            .features
            .get("ghcr.io/devcontainers/features/docker")
    );
}

#[test]
fn test_lockfile_path_derivation() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Test with normal config
    let config_path = temp_dir.path().join("devcontainer.json");
    let lockfile_path = get_lockfile_path(&config_path);
    assert_eq!(lockfile_path.file_name().unwrap(), "devcontainer-lock.json");

    // Test with hidden config
    let config_path = temp_dir.path().join(".devcontainer.json");
    let lockfile_path = get_lockfile_path(&config_path);
    assert_eq!(
        lockfile_path.file_name().unwrap(),
        ".devcontainer-lock.json"
    );

    // Test with subdirectory
    let config_dir = temp_dir.path().join(".devcontainer");
    let config_path = config_dir.join("devcontainer.json");
    let lockfile_path = get_lockfile_path(&config_path);
    assert_eq!(lockfile_path.parent().unwrap(), config_dir);
}

#[test]
fn test_merge_workflow() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let lockfile_path = temp_dir.path().join("devcontainer-lock.json");

    // Create initial lockfile
    let mut existing = Lockfile {
        features: HashMap::new(),
    };
    existing.features.insert(
        "feature-a".to_string(),
        LockfileFeature {
            version: "1.0.0".to_string(),
            resolved: "registry/feature-a@sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            integrity: "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            depends_on: None,
        },
    );

    existing.features.insert(
        "feature-b".to_string(),
        LockfileFeature {
            version: "1.5.0".to_string(),
            resolved: "registry/feature-b@sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
            integrity: "sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
            depends_on: None,
        },
    );

    write_lockfile(&lockfile_path, &existing, false).expect("Failed to write initial lockfile");

    // Create update with new version of feature-a and new feature-c
    let mut update = Lockfile {
        features: HashMap::new(),
    };
    update.features.insert(
        "feature-a".to_string(),
        LockfileFeature {
            version: "2.0.0".to_string(),
            resolved: "registry/feature-a@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            integrity: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            depends_on: None,
        },
    );

    update.features.insert(
        "feature-c".to_string(),
        LockfileFeature {
            version: "3.0.0".to_string(),
            resolved: "registry/feature-c@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string(),
            integrity: "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string(),
            depends_on: None,
        },
    );

    // Read existing and merge
    let existing_read = read_lockfile(&lockfile_path)
        .expect("Failed to read lockfile")
        .expect("Lockfile should exist");

    let merged = merge_lockfile_features(&existing_read, &update);

    // Verify merge results
    assert_eq!(merged.features.len(), 3);

    // feature-a should be updated to new version
    assert_eq!(merged.features.get("feature-a").unwrap().version, "2.0.0");

    // feature-b should be preserved
    assert_eq!(merged.features.get("feature-b").unwrap().version, "1.5.0");

    // feature-c should be added
    assert_eq!(merged.features.get("feature-c").unwrap().version, "3.0.0");

    // Write merged lockfile (force_init=true to overwrite existing)
    write_lockfile(&lockfile_path, &merged, true).expect("Failed to write merged lockfile");

    // Verify final state
    let final_lockfile = read_lockfile(&lockfile_path)
        .expect("Failed to read final lockfile")
        .expect("Lockfile should exist");

    assert_eq!(final_lockfile, merged);
}

#[test]
fn test_json_formatting() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let lockfile_path = temp_dir.path().join("formatted-lock.json");

    let mut lockfile = Lockfile {
        features: HashMap::new(),
    };

    lockfile.features.insert(
        "test-feature".to_string(),
        LockfileFeature {
            version: "1.0.0".to_string(),
            resolved: "registry/test@sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            integrity: "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            depends_on: None,
        },
    );

    write_lockfile(&lockfile_path, &lockfile, false).expect("Failed to write lockfile");

    // Read the raw JSON to verify formatting
    let json_content =
        std::fs::read_to_string(&lockfile_path).expect("Failed to read lockfile content");

    // Verify it's properly formatted JSON (not minified)
    assert!(json_content.contains('\n'));
    assert!(json_content.contains("  ")); // Should have indentation

    // Verify it can be parsed
    let _parsed: serde_json::Value =
        serde_json::from_str(&json_content).expect("Failed to parse JSON");
}

#[test]
fn test_nonexistent_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let nonexistent_path = temp_dir.path().join("does-not-exist.json");

    // Reading nonexistent file should return None, not error
    let result = read_lockfile(&nonexistent_path).expect("Should not error on nonexistent file");
    assert!(result.is_none());
}

#[test]
fn test_invalid_json() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let invalid_path = temp_dir.path().join("invalid.json");

    // Write invalid JSON
    std::fs::write(&invalid_path, "not valid json").expect("Failed to write invalid JSON");

    // Reading should error
    let result = read_lockfile(&invalid_path);
    assert!(result.is_err());
}

#[test]
fn test_nested_directory_creation() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let nested_path = temp_dir
        .path()
        .join("a")
        .join("b")
        .join("c")
        .join("devcontainer-lock.json");

    let lockfile = Lockfile {
        features: HashMap::new(),
    };

    // Should create parent directories automatically
    write_lockfile(&nested_path, &lockfile, false)
        .expect("Failed to write lockfile with nested path");

    assert!(nested_path.exists());
}

#[test]
fn test_overwrite_existing_lockfile() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let lockfile_path = temp_dir.path().join("overwrite-test.json");

    // Write initial lockfile
    let mut lockfile1 = Lockfile {
        features: HashMap::new(),
    };
    lockfile1.features.insert(
        "feature-a".to_string(),
        LockfileFeature {
            version: "1.0.0".to_string(),
            resolved: "registry/feature-a@sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            integrity: "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            depends_on: None,
        },
    );

    write_lockfile(&lockfile_path, &lockfile1, false).expect("Failed to write first lockfile");

    // Overwrite with new lockfile
    let mut lockfile2 = Lockfile {
        features: HashMap::new(),
    };
    lockfile2.features.insert(
        "feature-b".to_string(),
        LockfileFeature {
            version: "2.0.0".to_string(),
            resolved: "registry/feature-b@sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
            integrity: "sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
            depends_on: None,
        },
    );

    write_lockfile(&lockfile_path, &lockfile2, true).expect("Failed to overwrite lockfile");

    // Verify the new content
    let read_lockfile = read_lockfile(&lockfile_path)
        .expect("Failed to read lockfile")
        .expect("Lockfile should exist");

    assert_eq!(read_lockfile.features.len(), 1);
    assert!(read_lockfile.features.contains_key("feature-b"));
    assert!(!read_lockfile.features.contains_key("feature-a"));
}

#[test]
fn test_no_overwrite_when_force_init_false() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let lockfile_path = temp_dir.path().join("no-overwrite-test.json");

    // Write initial lockfile
    let mut lockfile1 = Lockfile {
        features: HashMap::new(),
    };
    lockfile1.features.insert(
        "original-feature".to_string(),
        LockfileFeature {
            version: "1.0.0".to_string(),
            resolved: "registry/original@sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            integrity: "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            depends_on: None,
        },
    );

    write_lockfile(&lockfile_path, &lockfile1, false).expect("Failed to write first lockfile");

    // Read original content
    let original_content = std::fs::read_to_string(&lockfile_path).expect("Failed to read file");

    // Try to overwrite with force_init=false (should fail)
    let mut lockfile2 = Lockfile {
        features: HashMap::new(),
    };
    lockfile2.features.insert(
        "new-feature".to_string(),
        LockfileFeature {
            version: "2.0.0".to_string(),
            resolved: "registry/new@sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
            integrity: "sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
            depends_on: None,
        },
    );

    let result = write_lockfile(&lockfile_path, &lockfile2, false);
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("already exists"));
    assert!(error_msg.contains("force_init=true"));

    // Verify original file is unchanged
    let current_content = std::fs::read_to_string(&lockfile_path).expect("Failed to read file");
    assert_eq!(original_content, current_content);

    // Verify original content is still there
    let read_lockfile = read_lockfile(&lockfile_path)
        .expect("Failed to read lockfile")
        .expect("Lockfile should exist");
    assert_eq!(read_lockfile.features.len(), 1);
    assert!(read_lockfile.features.contains_key("original-feature"));
    assert!(!read_lockfile.features.contains_key("new-feature"));
}

#[test]
fn test_real_world_scenario() {
    // Simulate a real-world scenario with config file and adjacent lockfile
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let devcontainer_dir = temp_dir.path().join(".devcontainer");
    std::fs::create_dir(&devcontainer_dir).expect("Failed to create .devcontainer dir");

    let config_path = devcontainer_dir.join("devcontainer.json");
    let lockfile_path = get_lockfile_path(&config_path);

    // Create lockfile with features matching typical DevContainer usage
    let mut lockfile = Lockfile {
        features: HashMap::new(),
    };

    lockfile.features.insert(
        "ghcr.io/devcontainers/features/node".to_string(),
        LockfileFeature {
            version: "1.5.0".to_string(),
            resolved: "ghcr.io/devcontainers/features/node@sha256:abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string(),
            integrity: "sha256:abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string(),
            depends_on: None,
        },
    );

    lockfile.features.insert(
        "ghcr.io/devcontainers/features/docker-in-docker".to_string(),
        LockfileFeature {
            version: "2.10.0".to_string(),
            resolved: "ghcr.io/devcontainers/features/docker-in-docker@sha256:fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321".to_string(),
            integrity: "sha256:fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321".to_string(),
            depends_on: None,
        },
    );

    lockfile.features.insert(
        "ghcr.io/devcontainers/features/git".to_string(),
        LockfileFeature {
            version: "1.0.0".to_string(),
            resolved: "ghcr.io/devcontainers/features/git@sha256:1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
            integrity: "sha256:1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
            depends_on: None,
        },
    );

    // Write lockfile
    write_lockfile(&lockfile_path, &lockfile, false).expect("Failed to write lockfile");

    // Verify lockfile is in correct location
    assert_eq!(
        lockfile_path,
        devcontainer_dir.join("devcontainer-lock.json")
    );
    assert!(lockfile_path.exists());

    // Read and verify
    let read_lockfile = read_lockfile(&lockfile_path)
        .expect("Failed to read lockfile")
        .expect("Lockfile should exist");

    assert_eq!(read_lockfile.features.len(), 3);
    assert!(read_lockfile
        .features
        .contains_key("ghcr.io/devcontainers/features/node"));
    assert!(read_lockfile
        .features
        .contains_key("ghcr.io/devcontainers/features/docker-in-docker"));
    assert!(read_lockfile
        .features
        .contains_key("ghcr.io/devcontainers/features/git"));
}
