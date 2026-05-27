//! Integration tests for security option handling
//!
//! Tests container creation with security options applied from configuration.

use deacon_core::config::{ConfigLoader, DevContainerConfig};
use deacon_core::security::SecurityOptions;
use serde_json::json;
use std::collections::HashMap;
use std::io::Write;
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_security_options_in_config_parsing() -> anyhow::Result<()> {
    // Create a test configuration with security options
    let config_content = json!({
        "image": "ubuntu:22.04",
        "privileged": true,
        "capAdd": ["SYS_PTRACE", "NET_ADMIN"],
        "securityOpt": ["seccomp=unconfined", "apparmor=unconfined"]
    });

    let mut temp_file = NamedTempFile::new()?;
    temp_file.write_all(config_content.to_string().as_bytes())?;

    // Load and parse the configuration
    let config = ConfigLoader::load_from_path(temp_file.path()).await?;

    // Verify security options are parsed correctly
    assert_eq!(config.privileged, Some(true));
    assert_eq!(config.cap_add, vec!["SYS_PTRACE", "NET_ADMIN"]);
    assert_eq!(
        config.security_opt,
        vec!["seccomp=unconfined", "apparmor=unconfined"]
    );

    Ok(())
}

#[test]
fn test_security_options_merge_with_features() -> anyhow::Result<()> {
    // Create config with some security options
    let config = DevContainerConfig {
        privileged: Some(true),
        cap_add: vec!["SYS_PTRACE".to_string()],
        ..Default::default()
    };

    // Create test features with additional security options
    use deacon_core::features::{FeatureMetadata, ResolvedFeature};

    let feature = ResolvedFeature {
        id: "test-feature".to_string(),
        source: "test://features/test-feature".to_string(),
        options: HashMap::new(),
        metadata: FeatureMetadata {
            id: "test-feature".to_string(),
            version: Some("1.0.0".to_string()),
            name: Some("Test Feature".to_string()),
            description: None,
            documentation_url: None,
            license_url: None,
            options: HashMap::new(),
            container_env: HashMap::new(),
            customizations: None,
            mounts: Vec::new(),
            init: None,
            privileged: None,
            cap_add: vec!["NET_ADMIN".to_string()],
            security_opt: vec!["seccomp=unconfined".to_string()],
            entrypoint: None,
            installs_after: Vec::new(),
            depends_on: HashMap::new(),
            on_create_command: None,
            update_content_command: None,
            post_create_command: None,
            post_start_command: None,
            post_attach_command: None,
        },
    };

    // Merge security options
    let security = SecurityOptions::merge_from_config_and_features(&config, &[feature]);

    // Verify merged options
    assert!(security.privileged);
    assert_eq!(security.cap_add, vec!["NET_ADMIN", "SYS_PTRACE"]); // Sorted and deduped
    assert_eq!(security.security_opt, vec!["seccomp=unconfined"]);
    assert!(security.has_security_options());

    // Verify Docker args generation
    let docker_args = security.to_docker_args();
    assert!(docker_args.contains(&"--privileged".to_string()));
    assert!(docker_args.contains(&"--cap-add".to_string()));
    assert!(docker_args.contains(&"SYS_PTRACE".to_string()));
    assert!(docker_args.contains(&"NET_ADMIN".to_string()));
    assert!(docker_args.contains(&"--security-opt".to_string()));
    assert!(docker_args.contains(&"seccomp=unconfined".to_string()));

    Ok(())
}

#[test]
fn test_security_options_docker_args_format() -> anyhow::Result<()> {
    // Create security options
    let mut security = SecurityOptions::new();
    security.privileged = true;
    security.cap_add = vec!["SYS_PTRACE".to_string(), "NET_ADMIN".to_string()];
    security.security_opt = vec!["seccomp=unconfined".to_string()];

    let args = security.to_docker_args();

    // Expected format: --privileged --cap-add SYS_PTRACE --cap-add NET_ADMIN --security-opt seccomp=unconfined
    let expected = vec![
        "--privileged",
        "--cap-add",
        "SYS_PTRACE",
        "--cap-add",
        "NET_ADMIN",
        "--security-opt",
        "seccomp=unconfined",
    ];

    assert_eq!(args, expected);

    Ok(())
}

#[test]
fn test_security_options_warning_for_existing_container() -> anyhow::Result<()> {
    let security = SecurityOptions {
        privileged: true,
        cap_add: vec!["SYS_PTRACE".to_string()],
        security_opt: vec!["seccomp=unconfined".to_string()],
        conflicts: Vec::new(),
    };

    // This should generate warning logs
    security.warn_if_post_create_application("test-container-id");

    // Note: In a real test, we would capture and verify the warning logs
    // For now, this test ensures the function doesn't panic

    Ok(())
}

#[test]
fn test_config_merge_security_options() -> anyhow::Result<()> {
    use deacon_core::config::ConfigMerger;

    // Base config with some security options
    let base_config = DevContainerConfig {
        privileged: Some(false),
        cap_add: vec!["SYS_PTRACE".to_string()],
        ..Default::default()
    };

    // Overlay config with additional security options
    let overlay_config = DevContainerConfig {
        privileged: Some(true),                 // This should override
        cap_add: vec!["NET_ADMIN".to_string()], // Set-union with base
        security_opt: vec!["seccomp=unconfined".to_string()],
        ..Default::default()
    };

    // Merge configs
    let merged = ConfigMerger::merge_configs(&[base_config, overlay_config]);

    // Verify merged security options
    assert_eq!(merged.privileged, Some(true)); // Last writer wins
    assert_eq!(merged.cap_add, vec!["SYS_PTRACE", "NET_ADMIN"]); // Union, base order preserved
    assert_eq!(merged.security_opt, vec!["seccomp=unconfined"]);

    Ok(())
}

/// capAdd / securityOpt MUST be merged as a set-union across configs (no duplicates),
/// matching upstream `unionOrUndefined` in `devcontainers/cli`
/// (`src/spec-node/imageMetadata.ts::mergeConfiguration`). First-seen order is preserved.
#[test]
fn test_config_merge_security_options_dedupes_overlapping_entries() -> anyhow::Result<()> {
    use deacon_core::config::ConfigMerger;

    let base = DevContainerConfig {
        cap_add: vec!["SYS_PTRACE".to_string(), "NET_ADMIN".to_string()],
        security_opt: vec!["seccomp=unconfined".to_string()],
        ..Default::default()
    };

    // Overlay overlaps with base on SYS_PTRACE and seccomp=unconfined and adds new entries.
    let overlay = DevContainerConfig {
        cap_add: vec![
            "SYS_PTRACE".to_string(), // already in base — must dedupe
            "SYS_ADMIN".to_string(),
        ],
        security_opt: vec![
            "seccomp=unconfined".to_string(), // already in base — must dedupe
            "apparmor=unconfined".to_string(),
        ],
        ..Default::default()
    };

    let merged = ConfigMerger::merge_configs(&[base, overlay]);

    // Base order preserved; overlay-only entries appended in order; duplicates dropped.
    assert_eq!(merged.cap_add, vec!["SYS_PTRACE", "NET_ADMIN", "SYS_ADMIN"]);
    assert_eq!(
        merged.security_opt,
        vec!["seccomp=unconfined", "apparmor=unconfined"]
    );

    Ok(())
}

/// Folding across more than two configs (e.g. extends chain or container metadata array) must
/// still deduplicate across every layer, not just adjacent pairs.
#[test]
fn test_config_merge_security_options_dedupes_across_multi_layer_chain() -> anyhow::Result<()> {
    use deacon_core::config::ConfigMerger;

    let layer_a = DevContainerConfig {
        cap_add: vec!["NET_ADMIN".to_string()],
        ..Default::default()
    };
    let layer_b = DevContainerConfig {
        cap_add: vec!["SYS_PTRACE".to_string(), "NET_ADMIN".to_string()],
        security_opt: vec!["label=disable".to_string()],
        ..Default::default()
    };
    let layer_c = DevContainerConfig {
        cap_add: vec!["SYS_ADMIN".to_string(), "SYS_PTRACE".to_string()],
        security_opt: vec!["label=disable".to_string()],
        ..Default::default()
    };

    let merged = ConfigMerger::merge_configs(&[layer_a, layer_b, layer_c]);

    assert_eq!(merged.cap_add, vec!["NET_ADMIN", "SYS_PTRACE", "SYS_ADMIN"]);
    assert_eq!(merged.security_opt, vec!["label=disable"]);

    Ok(())
}
