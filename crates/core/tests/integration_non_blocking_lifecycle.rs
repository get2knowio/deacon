//! Test to demonstrate non-blocking lifecycle phases

use deacon_core::container_lifecycle::{
    execute_container_lifecycle_with_docker, ContainerLifecycleCommands, ContainerLifecycleConfig,
};
use deacon_core::docker::mock::{MockDocker, MockDockerConfig, MockExecResponse};
use deacon_core::variable::SubstitutionContext;
use std::collections::HashMap;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn test_non_blocking_phases_are_deferred() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path();
    let substitution_context = SubstitutionContext::new(workspace_path).unwrap();

    // Create a mock Docker runtime with successful responses
    let mut config = MockDockerConfig::default();
    config.default_exec_response = MockExecResponse {
        exit_code: 0,
        success: true,
        delay: None,
        stdout: None,
        stderr: None,
    };
    let docker = MockDocker::with_config(config);

    // Create lifecycle configuration with non-blocking commands enabled
    let lifecycle_config = ContainerLifecycleConfig {
        container_id: "test-container".to_string(),
        user: Some("root".to_string()),
        container_workspace_folder: "/workspace".to_string(),
        container_env: HashMap::new(),
        skip_post_create: false,
        skip_non_blocking_commands: false, // Enable non-blocking commands
        non_blocking_timeout: Duration::from_secs(30),
    };

    // Create lifecycle commands with all phases
    let commands = ContainerLifecycleCommands::new()
        .with_on_create(vec!["echo 'onCreate'".to_string()])
        .with_post_create(vec!["echo 'postCreate'".to_string()])
        .with_post_start(vec!["echo 'postStart'".to_string()])
        .with_post_attach(vec!["echo 'postAttach'".to_string()]);

    // Execute lifecycle commands
    let result = execute_container_lifecycle_with_docker(
        &lifecycle_config,
        &commands,
        &substitution_context,
        &docker,
    )
    .await
    .unwrap();

    // Verify that blocking phases were executed immediately
    assert_eq!(result.phases.len(), 2); // onCreate and postCreate
    assert_eq!(result.phases[0].phase.as_str(), "onCreate");
    assert_eq!(result.phases[1].phase.as_str(), "postCreate");
    
    // Verify that non-blocking phases are marked for later execution
    assert_eq!(result.non_blocking_phases.len(), 2); // postStart and postAttach
    assert_eq!(result.non_blocking_phases[0].phase.as_str(), "postStart");
    assert_eq!(result.non_blocking_phases[1].phase.as_str(), "postAttach");
    
    // Verify non-blocking phase specifications
    let post_start_spec = &result.non_blocking_phases[0];
    assert_eq!(post_start_spec.commands, vec!["echo 'postStart'"]);
    assert_eq!(post_start_spec.timeout, Duration::from_secs(30));
    
    let post_attach_spec = &result.non_blocking_phases[1];
    assert_eq!(post_attach_spec.commands, vec!["echo 'postAttach'"]);
    assert_eq!(post_attach_spec.timeout, Duration::from_secs(30));

    // Verify that the MockDocker received exec calls for blocking phases only
    let exec_history = docker.get_exec_history();
    assert_eq!(exec_history.len(), 2); // Only onCreate and postCreate should have been executed
    
    println!("✓ Non-blocking phases are properly deferred for later execution");
    println!("✓ Blocking phases (onCreate, postCreate) execute immediately"); 
    println!("✓ Non-blocking phases (postStart, postAttach) are marked for background execution");
    println!("✓ MockDocker confirms only {} exec calls were made for blocking phases", exec_history.len());
}

#[tokio::test] 
async fn test_skip_non_blocking_commands_behavior() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path();
    let substitution_context = SubstitutionContext::new(workspace_path).unwrap();

    // Create a mock Docker runtime with successful responses
    let mut config = MockDockerConfig::default();
    config.default_exec_response = MockExecResponse {
        exit_code: 0,
        success: true,
        delay: None,
        stdout: None,
        stderr: None,
    };
    let docker = MockDocker::with_config(config);

    // Create lifecycle configuration with non-blocking commands DISABLED
    let lifecycle_config = ContainerLifecycleConfig {
        container_id: "test-container".to_string(),
        user: Some("root".to_string()),
        container_workspace_folder: "/workspace".to_string(),
        container_env: HashMap::new(),
        skip_post_create: false,
        skip_non_blocking_commands: true, // Disable non-blocking commands
        non_blocking_timeout: Duration::from_secs(30),
    };

    // Create lifecycle commands with all phases
    let commands = ContainerLifecycleCommands::new()
        .with_on_create(vec!["echo 'onCreate'".to_string()])
        .with_post_create(vec!["echo 'postCreate'".to_string()])
        .with_post_start(vec!["echo 'postStart'".to_string()])
        .with_post_attach(vec!["echo 'postAttach'".to_string()]);

    // Execute lifecycle commands
    let result = execute_container_lifecycle_with_docker(
        &lifecycle_config,
        &commands,
        &substitution_context,
        &docker,
    )
    .await
    .unwrap();

    // Verify that only blocking phases were executed
    assert_eq!(result.phases.len(), 2); // onCreate and postCreate only
    assert_eq!(result.phases[0].phase.as_str(), "onCreate");
    assert_eq!(result.phases[1].phase.as_str(), "postCreate");
    
    // Verify that no non-blocking phases are scheduled
    assert_eq!(result.non_blocking_phases.len(), 0); // postStart and postAttach should be skipped

    // Verify that the MockDocker received exec calls for blocking phases only
    let exec_history = docker.get_exec_history();
    assert_eq!(exec_history.len(), 2); // Only onCreate and postCreate should have been executed

    println!("✓ --skip-non-blocking-commands properly excludes postStart and postAttach");
    println!("✓ Blocking phases still execute normally when non-blocking commands are skipped");
    println!("✓ MockDocker confirms only {} exec calls were made", exec_history.len());
}

#[tokio::test]
async fn test_non_blocking_phases_sync_execution() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path();
    let substitution_context = SubstitutionContext::new(workspace_path).unwrap();

    // Create a mock Docker runtime with successful responses
    let mut config = MockDockerConfig::default();
    config.default_exec_response = MockExecResponse {
        exit_code: 0,
        success: true,
        delay: None,
        stdout: None,
        stderr: None,
    };
    let docker = MockDocker::with_config(config);

    // Create lifecycle configuration with non-blocking commands enabled
    let lifecycle_config = ContainerLifecycleConfig {
        container_id: "test-container".to_string(),
        user: Some("root".to_string()),
        container_workspace_folder: "/workspace".to_string(),
        container_env: HashMap::new(),
        skip_post_create: false,
        skip_non_blocking_commands: false, // Enable non-blocking commands
        non_blocking_timeout: Duration::from_secs(30),
    };

    // Create lifecycle commands with all phases
    let commands = ContainerLifecycleCommands::new()
        .with_on_create(vec!["echo 'onCreate'".to_string()])
        .with_post_create(vec!["echo 'postCreate'".to_string()])
        .with_post_start(vec!["echo 'postStart'".to_string()])
        .with_post_attach(vec!["echo 'postAttach'".to_string()]);

    // Execute lifecycle commands
    let result = execute_container_lifecycle_with_docker(
        &lifecycle_config,
        &commands,
        &substitution_context,
        &docker,
    )
    .await
    .unwrap();

    // Verify initial state: only blocking phases executed
    assert_eq!(result.phases.len(), 2); // onCreate and postCreate
    assert_eq!(result.non_blocking_phases.len(), 2); // postStart and postAttach deferred

    // Now execute the non-blocking phases synchronously for testing
    let final_result = result.execute_non_blocking_phases_sync(&docker).await.unwrap();
    
    // Verify that now all phases have been executed
    assert_eq!(final_result.phases.len(), 4); // All phases should now be complete
    assert_eq!(final_result.non_blocking_phases.len(), 0); // Should be empty after sync execution
    
    // Check that all phases are in the correct order
    assert_eq!(final_result.phases[0].phase.as_str(), "onCreate");
    assert_eq!(final_result.phases[1].phase.as_str(), "postCreate");
    assert_eq!(final_result.phases[2].phase.as_str(), "postStart");
    assert_eq!(final_result.phases[3].phase.as_str(), "postAttach");

    // Verify that the MockDocker received exec calls for all phases
    let exec_history = docker.get_exec_history();
    assert_eq!(exec_history.len(), 4); // All phases should have been executed

    println!("✓ Non-blocking phases can be executed synchronously for testing");
    println!("✓ All phases execute in correct order when sync execution is used");
    println!("✓ MockDocker confirms all {} phases were executed", exec_history.len());
}