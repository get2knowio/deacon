//! Integration tests for mount parsing and application
//!
//! These tests verify the end-to-end mount functionality including:
//! - Mount parsing from JSON configuration
//! - Variable substitution in mount specifications
//! - Docker CLI argument generation
//! - Mount validation and error handling

use deacon_core::config::{ConfigLoader, DevContainerConfig};
use deacon_core::mount::{Mount, MountConsistency, MountMode, MountParser, MountType};
use deacon_core::variable::SubstitutionContext;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_mount_parsing_from_config() {
    // Test parsing mounts from configuration with various formats
    let config_json = json!({
        "name": "Test Container",
        "image": "ubuntu:20.04",
        "mounts": [
            "type=bind,source=/host/path,target=/container/path,ro,consistency=cached",
            "/host/volume:/container/volume:rw",
            "myvolume:/data",
            "source=${localWorkspaceFolder}/src,target=/workspaces/src,type=bind"
        ]
    });

    let config: DevContainerConfig = serde_json::from_value(config_json).unwrap();
    let mounts = MountParser::parse_mounts_from_json(&config.mounts);

    assert_eq!(mounts.len(), 4);

    // Verify first mount (Docker mount syntax)
    let mount1 = &mounts[0];
    assert_eq!(mount1.mount_type, MountType::Bind);
    assert_eq!(mount1.source, Some("/host/path".to_string()));
    assert_eq!(mount1.target, "/container/path");
    assert_eq!(mount1.mode, MountMode::ReadOnly);
    assert_eq!(mount1.consistency, Some(MountConsistency::Cached));

    // Verify second mount (volume syntax with options)
    let mount2 = &mounts[1];
    assert_eq!(mount2.mount_type, MountType::Bind);
    assert_eq!(mount2.source, Some("/host/volume".to_string()));
    assert_eq!(mount2.target, "/container/volume");
    assert_eq!(mount2.mode, MountMode::ReadWrite);

    // Verify third mount (named volume)
    let mount3 = &mounts[2];
    assert_eq!(mount3.mount_type, MountType::Volume);
    assert_eq!(mount3.source, Some("myvolume".to_string()));
    assert_eq!(mount3.target, "/data");

    // Verify fourth mount (with variable substitution placeholder)
    let mount4 = &mounts[3];
    assert_eq!(mount4.mount_type, MountType::Bind);
    assert!(mount4
        .source
        .as_ref()
        .unwrap()
        .contains("${localWorkspaceFolder}"));
    assert_eq!(mount4.target, "/workspaces/src");
}

#[test]
fn test_mount_variable_substitution() {
    // Test that mount variable substitution works correctly
    let workspace_dir = TempDir::new().unwrap();
    let workspace_path = workspace_dir.path();

    let config_json = json!({
        "name": "Test Container",
        "image": "ubuntu:20.04",
        "mounts": [
            "source=${localWorkspaceFolder}/src,target=/workspaces/src,type=bind",
            "${localWorkspaceFolder}/cache:/cache:cached"
        ]
    });

    let config: DevContainerConfig = serde_json::from_value(config_json).unwrap();

    // Apply variable substitution
    let context = SubstitutionContext::new(workspace_path).unwrap();
    let (substituted_config, _) = config.apply_variable_substitution(&context);

    let mounts = MountParser::parse_mounts_from_json(&substituted_config.mounts);
    assert_eq!(mounts.len(), 2);

    // Verify first mount has variable substituted
    let mount1 = &mounts[0];
    assert_eq!(mount1.mount_type, MountType::Bind);
    // Use canonicalized path from substitution context to be robust across platforms (e.g., macOS /var -> /private/var)
    let expected_source = format!("{}/src", context.local_workspace_folder);
    assert_eq!(mount1.source, Some(expected_source));
    assert_eq!(mount1.target, "/workspaces/src");

    // Verify second mount has variable substituted
    let mount2 = &mounts[1];
    assert_eq!(mount2.mount_type, MountType::Bind);
    // Use canonicalized path from substitution context
    let expected_source = format!("{}/cache", context.local_workspace_folder);
    assert_eq!(mount2.source, Some(expected_source));
    assert_eq!(mount2.target, "/cache");
}

#[test]
fn test_workspace_mount_configuration() {
    // Test workspaceMount field parsing and variable substitution
    let workspace_dir = TempDir::new().unwrap();
    let workspace_path = workspace_dir.path();

    let config_json = json!({
        "name": "Test Container",
        "image": "ubuntu:20.04",
        "workspaceMount": "type=bind,source=${localWorkspaceFolder},target=/workspace,consistency=delegated"
    });

    let config: DevContainerConfig = serde_json::from_value(config_json).unwrap();
    assert!(config.workspace_mount.is_some());

    // Apply variable substitution
    let context = SubstitutionContext::new(workspace_path).unwrap();
    let (substituted_config, _) = config.apply_variable_substitution(&context);

    let workspace_mount_str = substituted_config.workspace_mount.unwrap();
    let workspace_mount = MountParser::parse_mount(&workspace_mount_str).unwrap();

    assert_eq!(workspace_mount.mount_type, MountType::Bind);
    // Compare against the canonicalized workspace path from the substitution
    // context
    assert_eq!(
        workspace_mount.source,
        Some(context.local_workspace_folder.clone())
    );
    assert_eq!(workspace_mount.target, "/workspace");
    assert_eq!(
        workspace_mount.consistency,
        Some(MountConsistency::Delegated)
    );
}

#[test]
fn test_mount_docker_args_generation() {
    // Test that mounts generate correct Docker CLI arguments
    let mount = Mount {
        mount_type: MountType::Bind,
        source: Some("/host/path".to_string()),
        target: "/container/path".to_string(),
        mode: MountMode::ReadOnly,
        consistency: Some(MountConsistency::Cached),
        options: {
            let mut opts = HashMap::new();
            opts.insert("bind-propagation".to_string(), "shared".to_string());
            opts
        },
    };

    let args = mount.to_docker_args();
    assert_eq!(args.len(), 2);
    assert_eq!(args[0], "--mount");

    let mount_str = &args[1];
    assert!(mount_str.contains("type=bind"));
    assert!(mount_str.contains("source=/host/path"));
    assert!(mount_str.contains("target=/container/path"));
    assert!(mount_str.contains("ro"));
    assert!(mount_str.contains("consistency=cached"));
    assert!(mount_str.contains("bind-propagation=shared"));
}

#[test]
fn test_mount_validation_errors() {
    // Test that invalid mount specifications produce validation errors

    // Missing source for bind mount
    let result = MountParser::parse_mount("type=bind,target=/container/path");
    assert!(result.is_err());

    // Missing target
    let result = MountParser::parse_mount("type=bind,source=/host/path");
    assert!(result.is_err());

    // Missing type
    let result = MountParser::parse_mount("source=/host/path,target=/container/path");
    assert!(result.is_err());

    // Relative target path
    let result = MountParser::parse_mount("type=bind,source=/host/path,target=relative/path");
    assert!(result.is_err());

    // Invalid volume syntax
    let result = MountParser::parse_mount("incomplete");
    assert!(result.is_err());
}

#[test]
fn test_mount_parsing_edge_cases() {
    // Test various edge cases in mount parsing

    // Empty source (for tmpfs)
    let mount = MountParser::parse_mount("type=tmpfs,target=/tmp").unwrap();
    assert_eq!(mount.mount_type, MountType::Tmpfs);
    assert_eq!(mount.source, None);
    assert_eq!(mount.target, "/tmp");

    // Volume with dots in name (should be treated as volume, not bind)
    let mount = MountParser::parse_mount("my.volume:/data").unwrap();
    assert_eq!(mount.mount_type, MountType::Volume);
    assert_eq!(mount.source, Some("my.volume".to_string()));

    // Relative path (should be treated as bind)
    let mount = MountParser::parse_mount("./local:/container").unwrap();
    assert_eq!(mount.mount_type, MountType::Bind);
    assert_eq!(mount.source, Some("./local".to_string()));

    // Path with special characters (but valid volume syntax)
    let mount = MountParser::parse_mount("/host-path_with.chars:/container:ro").unwrap();
    assert_eq!(mount.mount_type, MountType::Bind);
    assert_eq!(mount.source, Some("/host-path_with.chars".to_string()));
    assert_eq!(mount.target, "/container");
    assert_eq!(mount.mode, MountMode::ReadOnly);
}

#[test]
fn test_mount_types_and_consistency() {
    // Test different mount types
    let bind_mount = MountParser::parse_mount("type=bind,source=/host,target=/container").unwrap();
    assert_eq!(bind_mount.mount_type, MountType::Bind);

    let volume_mount =
        MountParser::parse_mount("type=volume,source=myvolume,target=/container").unwrap();
    assert_eq!(volume_mount.mount_type, MountType::Volume);

    let tmpfs_mount = MountParser::parse_mount("type=tmpfs,target=/tmp").unwrap();
    assert_eq!(tmpfs_mount.mount_type, MountType::Tmpfs);

    // Test consistency options
    let cached_mount =
        MountParser::parse_mount("type=bind,source=/host,target=/container,consistency=cached")
            .unwrap();
    assert_eq!(cached_mount.consistency, Some(MountConsistency::Cached));

    let consistent_mount =
        MountParser::parse_mount("type=bind,source=/host,target=/container,consistency=consistent")
            .unwrap();
    assert_eq!(
        consistent_mount.consistency,
        Some(MountConsistency::Consistent)
    );

    let delegated_mount =
        MountParser::parse_mount("type=bind,source=/host,target=/container,consistency=delegated")
            .unwrap();
    assert_eq!(
        delegated_mount.consistency,
        Some(MountConsistency::Delegated)
    );
}

#[test]
fn test_mount_from_fixture_config() {
    // Test loading and parsing mounts from actual fixture files
    let fixture_path = Path::new("../../fixtures/config/basic/devcontainer.jsonc");

    // Skip test if fixture doesn't exist (for CI environments)
    if !fixture_path.exists() {
        eprintln!(
            "Skipping test: fixture file not found at {:?}",
            fixture_path
        );
        return;
    }

    let config = ConfigLoader::load_from_path(fixture_path).unwrap();

    // Verify mount from fixture is parsed correctly
    assert_eq!(config.mounts.len(), 1);
    let mounts = MountParser::parse_mounts_from_json(&config.mounts);
    assert_eq!(mounts.len(), 1);

    let mount = &mounts[0];
    assert_eq!(mount.mount_type, MountType::Bind);
    assert!(mount
        .source
        .as_ref()
        .unwrap()
        .contains("${localWorkspaceFolder}"));
    assert_eq!(mount.target, "/usr/local/cargo");
    assert_eq!(mount.consistency, Some(MountConsistency::Cached));
}

#[test]
fn test_mount_with_variables_fixture() {
    // Test mount parsing with variable substitution from fixtures
    let fixture_path = Path::new("../../fixtures/config/with-variables/devcontainer.jsonc");

    // Skip test if fixture doesn't exist (for CI environments)
    if !fixture_path.exists() {
        eprintln!(
            "Skipping test: fixture file not found at {:?}",
            fixture_path
        );
        return;
    }

    let workspace_dir = TempDir::new().unwrap();
    let workspace_path = workspace_dir.path();

    let config = ConfigLoader::load_from_path(fixture_path).unwrap();

    // Apply variable substitution
    let context = SubstitutionContext::new(workspace_path).unwrap();
    let (substituted_config, _) = config.apply_variable_substitution(&context);

    // Parse mounts with variable substitution applied
    let mounts = MountParser::parse_mounts_from_json(&substituted_config.mounts);
    assert_eq!(mounts.len(), 3);

    // Verify that variables were substituted
    for mount in &mounts {
        if let Some(ref source) = mount.source {
            // Variables should be substituted
            assert!(!source.contains("${localWorkspaceFolder}"));
            // Should contain actual workspace path
            assert!(source.contains(&context.local_workspace_folder));
        }
    }
}
