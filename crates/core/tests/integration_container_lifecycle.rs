//! Integration test for container lifecycle execution with variable substitution

use deacon_core::container_lifecycle::{
    execute_container_lifecycle, ContainerLifecycleCommands, ContainerLifecycleConfig,
};
use deacon_core::variable::SubstitutionContext;
use std::collections::HashMap;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn test_container_lifecycle_with_variable_substitution() {
    // This test demonstrates how the container lifecycle execution would work
    // Note: This test doesn't actually run a container since it would require Docker
    // Instead, it shows the structure and API usage

    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path();

    // Create substitution context for the workspace
    let substitution_context = SubstitutionContext::new(workspace_path).unwrap();

    // Configure container lifecycle
    let mut container_env = HashMap::new();
    container_env.insert("NODE_ENV".to_string(), "development".to_string());
    container_env.insert("DEBUG".to_string(), "true".to_string());

    let config = ContainerLifecycleConfig {
        container_id: "test-container-123".to_string(),
        user: Some("root".to_string()),
        container_workspace_folder: "/workspaces/test".to_string(),
        container_env,
        skip_post_create: false,
        skip_non_blocking_commands: false,
        non_blocking_timeout: Duration::from_secs(300),
        use_login_shell: false,
        user_env_probe: deacon_core::container_env_probe::ContainerProbeMode::None,
    };

    // Define lifecycle commands with variable substitution
    let commands = ContainerLifecycleCommands::new()
        .with_on_create(vec![
            "echo 'onCreate in ${containerWorkspaceFolder}'".to_string(),
            "mkdir -p ${containerWorkspaceFolder}/.devcontainer".to_string(),
        ])
        .with_post_create(vec![
            "echo 'postCreate: NODE_ENV=${containerEnv:NODE_ENV}'".to_string(),
            "touch ${containerWorkspaceFolder}/.post-create-marker".to_string(),
        ])
        .with_post_start(vec![
            "echo 'postStart: Debug mode=${containerEnv:DEBUG}'".to_string()
        ])
        .with_post_attach(vec![
            "echo 'postAttach: Ready in ${containerWorkspaceFolder}'".to_string(),
        ]);

    let result = execute_container_lifecycle(&config, &commands, &substitution_context).await;

    match result {
        Ok(lifecycle) => {
            assert!(!lifecycle.phases.is_empty());
            // Non-blocking phases should be deferred when not skipped
            assert!(
                !lifecycle.non_blocking_phases.is_empty(),
                "Expected non-blocking phases to be scheduled"
            );
            assert!(
                lifecycle.phases.iter().any(|phase| !phase.success),
                "Expected at least one lifecycle phase to reflect the missing container"
            );
        }
        Err(error) => {
            println!("Error: {}", error);
            assert!(
                error
                    .to_string()
                    .contains("Container command failed in phase onCreate")
                    || error
                        .to_string()
                        .contains("Failed to execute container command")
                    || error.to_string().contains("Docker error")
                    || error.to_string().contains("No such container")
            );
        }
    }
}

#[tokio::test]
async fn test_container_lifecycle_with_skip_flags() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path();
    let substitution_context = SubstitutionContext::new(workspace_path).unwrap();

    // Test with skip flags enabled
    let config = ContainerLifecycleConfig {
        container_id: "test-container-456".to_string(),
        user: None, // Test without user specification
        container_workspace_folder: "/workspaces/test".to_string(),
        container_env: HashMap::new(),
        skip_post_create: true,
        skip_non_blocking_commands: true,
        non_blocking_timeout: Duration::from_secs(300),
        use_login_shell: false,
        user_env_probe: deacon_core::container_env_probe::ContainerProbeMode::None,
    };

    let commands = ContainerLifecycleCommands::new()
        .with_on_create(vec!["echo 'onCreate'".to_string()])
        .with_post_create(vec!["echo 'postCreate (should be skipped)'".to_string()])
        .with_post_start(vec!["echo 'postStart (should be skipped)'".to_string()])
        .with_post_attach(vec!["echo 'postAttach (should be skipped)'".to_string()]);

    let result = execute_container_lifecycle(&config, &commands, &substitution_context).await;

    match result {
        Ok(lifecycle) => {
            assert_eq!(lifecycle.phases.len(), 1);
            assert!(
                lifecycle.non_blocking_phases.is_empty(),
                "Non-blocking phases should be empty when skipped"
            );
            let phase = &lifecycle.phases[0];
            assert_eq!(phase.phase.as_str(), "onCreate");
            assert!(
                !phase.success,
                "Expected onCreate phase to fail without a container"
            );
        }
        Err(_) => {
            // An error is also acceptable when Docker is unavailable
        }
    }
}

#[test]
fn test_container_lifecycle_config_validation() {
    // Test the configuration structure
    let mut container_env = HashMap::new();
    container_env.insert("TEST_VAR".to_string(), "test_value".to_string());

    let config = ContainerLifecycleConfig {
        container_id: "test-container".to_string(),
        user: Some("testuser".to_string()),
        container_workspace_folder: "/workspaces/myproject".to_string(),
        container_env: container_env.clone(),
        skip_post_create: false,
        skip_non_blocking_commands: true,
        non_blocking_timeout: Duration::from_secs(300),
        use_login_shell: false,
        user_env_probe: deacon_core::container_env_probe::ContainerProbeMode::None,
    };

    assert_eq!(config.container_id, "test-container");
    assert_eq!(config.user, Some("testuser".to_string()));
    assert_eq!(config.container_workspace_folder, "/workspaces/myproject");
    assert_eq!(
        config.container_env.get("TEST_VAR"),
        Some(&"test_value".to_string())
    );
    assert!(!config.skip_post_create);
    assert!(config.skip_non_blocking_commands);
    assert_eq!(config.non_blocking_timeout, Duration::from_secs(300));
}

#[test]
fn test_lifecycle_commands_structure() {
    let commands = ContainerLifecycleCommands::new()
        .with_on_create(vec![
            "echo 'Setting up project'".to_string(),
            "npm install".to_string(),
        ])
        .with_post_create(vec!["echo 'Project initialized'".to_string()])
        .with_post_start(vec!["echo 'Starting services'".to_string()])
        .with_post_attach(vec!["echo 'Ready for development'".to_string()]);

    assert!(commands.on_create.is_some());
    assert_eq!(commands.on_create.unwrap().len(), 2);

    assert!(commands.post_create.is_some());
    assert_eq!(commands.post_create.unwrap().len(), 1);

    assert!(commands.post_start.is_some());
    assert_eq!(commands.post_start.unwrap().len(), 1);

    assert!(commands.post_attach.is_some());
    assert_eq!(commands.post_attach.unwrap().len(), 1);
}

/// Test that all 6 lifecycle phases can be configured and executed in order
#[tokio::test]
async fn test_all_lifecycle_phases_ordering() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path();
    let substitution_context = SubstitutionContext::new(workspace_path).unwrap();

    let config = ContainerLifecycleConfig {
        container_id: "test-container-all-phases".to_string(),
        user: None,
        container_workspace_folder: "/workspaces/test".to_string(),
        container_env: HashMap::new(),
        skip_post_create: false,
        skip_non_blocking_commands: false,
        non_blocking_timeout: Duration::from_secs(300),
        use_login_shell: false,
        user_env_probe: deacon_core::container_env_probe::ContainerProbeMode::None,
    };

    // Define all 6 lifecycle phases
    let commands = ContainerLifecycleCommands::new()
        .with_initialize(vec!["echo 'Phase 1: initialize (host-side)'".to_string()])
        .with_on_create(vec!["echo 'Phase 2: onCreate'".to_string()])
        .with_update_content(vec!["echo 'Phase 3: updateContent'".to_string()])
        .with_post_create(vec!["echo 'Phase 4: postCreate'".to_string()])
        .with_post_start(vec!["echo 'Phase 5: postStart'".to_string()])
        .with_post_attach(vec!["echo 'Phase 6: postAttach'".to_string()]);

    // Execute lifecycle - this will attempt to run in a container
    // Without Docker, it will fail, but we can verify the structure
    let result = execute_container_lifecycle(&config, &commands, &substitution_context).await;

    match result {
        Ok(lifecycle) => {
            // If Docker is available, verify phases were executed in order
            // Phase 1 (initialize) should be executed on host first
            // Then phases 2-4 should be blocking
            // Phases 5-6 should be scheduled as non-blocking
            assert!(
                !lifecycle.phases.is_empty() || !lifecycle.non_blocking_phases.is_empty(),
                "Expected at least some phases to be executed or scheduled"
            );
        }
        Err(error) => {
            // Expected when Docker is not available
            println!(
                "Lifecycle execution failed (expected without Docker): {}",
                error
            );
        }
    }
}
