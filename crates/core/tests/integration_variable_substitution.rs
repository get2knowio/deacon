//! Integration test for configuration discovery and variable substitution
//!
//! This test validates the complete workflow of configuration discovery
//! and variable substitution using fixture configurations.

use deacon_core::config::ConfigLoader;
use deacon_core::variable::{SubstitutionContext, VariableSubstitution};
use std::env;
use std::path::Path;
use tempfile::TempDir;

/// Test configuration discovery with fixture
#[test]
fn test_discover_and_load_fixture_config() -> anyhow::Result<()> {
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures")
        .join("config")
        .join("with-variables");

    // Create a temporary workspace directory
    let temp_workspace = TempDir::new()?;
    let workspace = temp_workspace.path();
    // Use a canonicalized workspace path for assertions to avoid macOS /var vs /private/var issues
    let workspace_canon = std::fs::canonicalize(workspace)?;
    let workspace_canon_str = workspace_canon
        .to_str()
        .expect("canonicalized workspace path should be valid UTF-8");

    // Copy the fixture to a .devcontainer directory in the temp workspace
    let devcontainer_dir = workspace.join(".devcontainer");
    std::fs::create_dir_all(&devcontainer_dir)?;
    let target_config = devcontainer_dir.join("devcontainer.json");

    let fixture_config = fixture_path.join("devcontainer.jsonc");
    std::fs::copy(&fixture_config, &target_config)?;

    // Test configuration discovery
    let location = ConfigLoader::discover_config(workspace)?;
    assert!(location.exists());
    assert_eq!(location.path(), &target_config);

    // Test loading with variable substitution
    let (config, report) = ConfigLoader::load_with_substitution(location.path(), workspace)?;

    // Verify basic config was loaded
    assert_eq!(
        config.name,
        Some("Variable Substitution Test Container".to_string())
    );
    assert_eq!(config.image, Some("ubuntu:20.04".to_string()));

    // Verify substitutions were applied
    assert!(report.has_substitutions());
    assert!(!report.replacements.is_empty());

    // Check workspace folder substitution
    if let Some(workspace_folder) = &config.workspace_folder {
        assert!(workspace_folder.starts_with(workspace_canon_str));
        assert!(workspace_folder.ends_with("/src"));
    }

    // Check container environment variable substitution
    let workspace_root = config.container_env.get("WORKSPACE_ROOT").unwrap();
    assert!(workspace_root.starts_with(workspace_canon_str));

    let container_id = config.container_env.get("CONTAINER_ID").unwrap();
    assert_eq!(container_id.len(), 12); // Should be 12-character deterministic ID

    // Check host USER environment variable (if set)
    if let Ok(host_user) = env::var("USER") {
        assert_eq!(config.container_env.get("HOST_USER").unwrap(), &host_user);
    }

    // Check missing environment variable becomes empty string
    assert_eq!(config.container_env.get("MISSING_VAR").unwrap(), "");

    // Check mounts substitution
    assert!(!config.mounts.is_empty());
    for mount in &config.mounts {
        if let serde_json::Value::String(mount_str) = mount {
            if mount_str.contains("source=") || mount_str.contains(":") {
                assert!(mount_str.contains(workspace_canon_str));
            }
        }
    }

    // Check run args substitution
    assert!(!config.run_args.is_empty());
    let devcontainer_name = config
        .run_args
        .iter()
        .find(|arg| arg.starts_with("devcontainer-"))
        .unwrap();
    assert!(devcontainer_name.len() > "devcontainer-".len());

    // Check lifecycle commands substitution
    if let Some(serde_json::Value::String(on_create)) = &config.on_create_command {
        assert!(on_create.contains(workspace_canon_str));
    }

    if let Some(serde_json::Value::Array(post_create)) = &config.post_create_command {
        // Check that at least one array element contains the substituted workspace path
        let has_workspace_path = post_create.iter().any(|cmd| {
            if let serde_json::Value::String(s) = cmd {
                s.contains(workspace_canon_str)
            } else {
                false
            }
        });
        assert!(has_workspace_path);
    }

    println!("✅ Configuration discovery and variable substitution test passed");
    println!("   Workspace: {}", workspace.display());
    println!("   Substitutions: {}", report.replacements.len());
    println!("   Unknown variables: {}", report.unknown_variables.len());

    Ok(())
}

/// Test deterministic devcontainer ID generation
#[test]
fn test_deterministic_devcontainer_id() -> anyhow::Result<()> {
    let temp_workspace = TempDir::new()?;
    let workspace = temp_workspace.path();

    // Create multiple substitution contexts with the same workspace
    let context1 = SubstitutionContext::new(workspace)?;
    let context2 = SubstitutionContext::new(workspace)?;

    // IDs should be identical for the same workspace
    assert_eq!(context1.devcontainer_id, context2.devcontainer_id);

    // Test with the same workspace path but created differently
    let workspace_clone = workspace.to_path_buf();
    let context3 = SubstitutionContext::new(&workspace_clone)?;
    assert_eq!(context1.devcontainer_id, context3.devcontainer_id);

    println!("✅ Deterministic devcontainer ID test passed");
    println!("   ID: {}", context1.devcontainer_id);

    Ok(())
}

/// Test configuration discovery order and fallback
#[test]
fn test_config_discovery_order() -> anyhow::Result<()> {
    let temp_workspace = TempDir::new()?;
    let workspace = temp_workspace.path();

    // Test 1: No config files exist
    let location = ConfigLoader::discover_config(workspace)?;
    assert!(!location.exists());
    assert_eq!(
        location.path(),
        &workspace.join(".devcontainer").join("devcontainer.json")
    );

    // Test 2: Only root .devcontainer.json exists
    let root_config = workspace.join(".devcontainer.json");
    std::fs::write(&root_config, r#"{"name": "Root Config"}"#)?;

    let location = ConfigLoader::discover_config(workspace)?;
    assert!(location.exists());
    assert_eq!(location.path(), &root_config);

    // Test 3: Both configs exist - should prefer .devcontainer/devcontainer.json
    let devcontainer_dir = workspace.join(".devcontainer");
    std::fs::create_dir_all(&devcontainer_dir)?;
    let dir_config = devcontainer_dir.join("devcontainer.json");
    std::fs::write(&dir_config, r#"{"name": "Dir Config"}"#)?;

    let location = ConfigLoader::discover_config(workspace)?;
    assert!(location.exists());
    assert_eq!(location.path(), &dir_config);

    // Verify we can load the correct config
    let config = ConfigLoader::load_from_path(location.path())?;
    assert_eq!(config.name, Some("Dir Config".to_string()));

    println!("✅ Configuration discovery order test passed");

    Ok(())
}

/// Test variable substitution with environment variables
#[test]
fn test_env_variable_substitution() -> anyhow::Result<()> {
    // Set a test environment variable
    env::set_var("DEACON_TEST_VAR", "test_value_123");

    let temp_workspace = TempDir::new()?;
    let workspace = temp_workspace.path();
    let context = SubstitutionContext::new(workspace)?;

    // Test localEnv substitution
    let input = "Value: ${localEnv:DEACON_TEST_VAR}";
    let mut report = deacon_core::variable::SubstitutionReport::new();
    let result = VariableSubstitution::substitute_string(input, &context, &mut report);

    assert_eq!(result, "Value: test_value_123");
    assert!(report.replacements.contains_key("localEnv:DEACON_TEST_VAR"));

    // Test missing environment variable
    let input = "Missing: ${localEnv:NONEXISTENT_VAR}";
    let mut report = deacon_core::variable::SubstitutionReport::new();
    let result = VariableSubstitution::substitute_string(input, &context, &mut report);

    assert_eq!(result, "Missing: ");
    assert!(report.replacements.contains_key("localEnv:NONEXISTENT_VAR"));

    // Clean up
    env::remove_var("DEACON_TEST_VAR");

    println!("✅ Environment variable substitution test passed");

    Ok(())
}
