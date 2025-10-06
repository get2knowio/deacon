//! Integration tests for in-container feature installation

use deacon_core::feature_installer::FeatureInstallationConfig;
use deacon_core::features::{FeatureMetadata, InstallationPlan, ResolvedFeature};
use deacon_core::oci::DownloadedFeature;
use std::collections::HashMap;
use tempfile::TempDir;

// Mock feature for testing
fn create_test_feature() -> (ResolvedFeature, DownloadedFeature) {
    let temp_dir = TempDir::new().unwrap();

    // Create a simple feature metadata
    let metadata = FeatureMetadata {
        id: "test-feature".to_string(),
        version: Some("1.0.0".to_string()),
        name: Some("Test Feature".to_string()),
        description: Some("A test feature for integration testing".to_string()),
        documentation_url: None,
        license_url: None,
        options: HashMap::new(),
        container_env: {
            let mut env = HashMap::new();
            env.insert("TEST_FEATURE_INSTALLED".to_string(), "true".to_string());
            env.insert("TEST_FEATURE_VERSION".to_string(), "1.0.0".to_string());
            env
        },
        mounts: vec![],
        init: None,
        privileged: None,
        cap_add: vec![],
        security_opt: vec![],
        entrypoint: None,
        installs_after: vec![],
        depends_on: HashMap::new(),
        on_create_command: None,
        update_content_command: None,
        post_create_command: None,
        post_start_command: None,
        post_attach_command: None,
    };

    // Create feature files in temp directory
    let feature_path = temp_dir.path();

    // Create devcontainer-feature.json
    let feature_json = serde_json::to_string_pretty(&metadata).unwrap();
    std::fs::write(feature_path.join("devcontainer-feature.json"), feature_json).unwrap();

    // Create install.sh script that creates a marker file and sets an env var
    let install_script = r#"#!/bin/bash
set -e

echo "Installing test feature..."
echo "FEATURE_ID: ${FEATURE_ID}"
echo "FEATURE_VERSION: ${FEATURE_VERSION}"
echo "PROVIDED_OPTIONS: ${PROVIDED_OPTIONS}"
echo "DEACON: ${DEACON}"

# Create a marker file to verify installation
mkdir -p /tmp/feature-test
echo "Test feature was installed successfully" > /tmp/feature-test/marker.txt
echo "Feature ID: ${FEATURE_ID}" >> /tmp/feature-test/marker.txt
echo "Feature Version: ${FEATURE_VERSION}" >> /tmp/feature-test/marker.txt

echo "Test feature installation completed successfully"
"#;

    std::fs::write(feature_path.join("install.sh"), install_script).unwrap();

    let resolved_feature = ResolvedFeature {
        id: "test-feature".to_string(),
        source: "test://test-feature".to_string(),
        options: HashMap::new(),
        metadata: metadata.clone(),
    };

    let downloaded_feature = DownloadedFeature {
        path: feature_path.to_path_buf(),
        metadata,
        digest: "test-digest".to_string(),
    };

    // Prevent temp_dir from being dropped
    std::mem::forget(temp_dir);

    (resolved_feature, downloaded_feature)
}

#[tokio::test]
#[ignore] // Ignore by default since it requires Docker
async fn test_feature_installation_integration() {
    // This test would require a running Docker container
    // For now, it's marked as ignored and serves as documentation

    let (feature, downloaded_feature) = create_test_feature();

    // Create installation plan
    let _plan = InstallationPlan::new(vec![feature]);

    // Create downloaded features map
    let mut _downloaded_features = HashMap::new();
    _downloaded_features.insert("test-feature".to_string(), downloaded_feature);

    // Create installation config (would need real container ID)
    let _config = FeatureInstallationConfig {
        container_id: "test-container".to_string(),
        apply_security_options: false,
        installation_base_dir: "/tmp/devcontainer-features".to_string(),
    };

    // This would require Docker to be available and a container to be running
    // let docker = CliDocker::new();
    // let installer = FeatureInstaller::new(docker);
    // let result = installer.install_features(&plan, &downloaded_features, &config).await;

    // Assertions would verify:
    // 1. Installation succeeded
    // 2. Marker file was created in container
    // 3. Environment variables were applied

    println!("Integration test framework ready - requires Docker container for full test");
}

#[test]
fn test_feature_creation() {
    let (feature, downloaded_feature) = create_test_feature();

    assert_eq!(feature.id, "test-feature");
    assert_eq!(feature.metadata.version, Some("1.0.0".to_string()));
    assert!(!feature.metadata.container_env.is_empty());
    assert_eq!(
        feature.metadata.container_env.get("TEST_FEATURE_INSTALLED"),
        Some(&"true".to_string())
    );

    assert_eq!(downloaded_feature.metadata.id, "test-feature");
    assert!(downloaded_feature.path.join("install.sh").exists());
    assert!(downloaded_feature
        .path
        .join("devcontainer-feature.json")
        .exists());
}

#[test]
fn test_installation_plan_creation() {
    let (feature, _) = create_test_feature();
    let plan = InstallationPlan::new(vec![feature]);

    assert_eq!(plan.len(), 1);
    assert!(!plan.is_empty());
    assert_eq!(plan.feature_ids(), vec!["test-feature"]);
}

#[test]
fn test_feature_installation_config() {
    let config = FeatureInstallationConfig {
        container_id: "test-container-123".to_string(),
        apply_security_options: true,
        installation_base_dir: "/custom/features".to_string(),
    };

    assert_eq!(config.container_id, "test-container-123");
    assert!(config.apply_security_options);
    assert_eq!(config.installation_base_dir, "/custom/features");
}

#[test]
fn test_installation_config_default() {
    let config = FeatureInstallationConfig::default();

    assert_eq!(config.container_id, "");
    assert!(!config.apply_security_options);
    assert_eq!(config.installation_base_dir, "/tmp/devcontainer-features");
}
