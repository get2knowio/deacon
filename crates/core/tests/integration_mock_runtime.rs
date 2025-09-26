//! Integration tests for exec and lifecycle flows without Docker
//!
//! These tests use the MockDocker runtime to validate exec and lifecycle command execution
//! without requiring a real Docker daemon, enabling comprehensive testing in CI environments.

use anyhow::Result;
use deacon_core::config::DevContainerConfig;
use deacon_core::container::ContainerIdentity;
use deacon_core::container_lifecycle::{
    execute_container_lifecycle_with_docker, ContainerLifecycleCommands, ContainerLifecycleConfig,
};
use deacon_core::docker::mock::{MockContainer, MockDocker, MockDockerConfig, MockExecResponse};
use deacon_core::docker::{Docker, ExecConfig};
use deacon_core::variable::SubstitutionContext;
use std::collections::HashMap;
use std::time::Duration;
use tempfile::TempDir;

/// Test helper to create a basic dev container config
fn create_test_config() -> DevContainerConfig {
    DevContainerConfig {
        image: Some("ubuntu:20.04".to_string()),
        name: Some("test-dev".to_string()),
        ..Default::default()
    }
}

/// Test helper to create a mock container with devcontainer labels
fn create_labeled_container(workspace_hash: &str, config_hash: &str) -> MockContainer {
    let mut labels = HashMap::new();
    labels.insert("devcontainer.source".to_string(), "deacon".to_string());
    labels.insert(
        "devcontainer.workspaceHash".to_string(),
        workspace_hash.to_string(),
    );
    labels.insert(
        "devcontainer.configHash".to_string(),
        config_hash.to_string(),
    );
    labels.insert("devcontainer.name".to_string(), "test-dev".to_string());

    MockContainer::new(
        "test-container-123".to_string(),
        "test-dev-container".to_string(),
        "ubuntu:20.04".to_string(),
    )
    .with_labels(labels)
}

#[tokio::test]
async fn test_exec_with_mock_docker_success() -> Result<()> {
    let mock_docker = MockDocker::new();

    // Create test workspace
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    let config = create_test_config();

    // Create container identity and add matching container
    let identity = ContainerIdentity::new(workspace_path, &config);
    let container = create_labeled_container(&identity.workspace_hash, &identity.config_hash);
    mock_docker.add_container(container);

    // Execute a simple command
    let exec_config = ExecConfig {
        user: Some("root".to_string()),
        working_dir: Some("/workspace".to_string()),
        env: HashMap::new(),
        tty: true,
        interactive: true,
        detach: false,
    };

    let result = mock_docker
        .exec(
            "test-container-123",
            &["echo".to_string(), "hello".to_string()],
            exec_config,
        )
        .await?;

    assert_eq!(result.exit_code, 0);
    assert!(result.success);

    // Verify exec history
    let history = mock_docker.get_exec_history();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].container_id, "test-container-123");
    assert_eq!(history[0].command, vec!["echo", "hello"]);
    assert!(history[0].config.tty);
    assert!(history[0].config.interactive);

    Ok(())
}

#[tokio::test]
async fn test_exec_with_mock_docker_failure() -> Result<()> {
    let mock_docker = MockDocker::new();

    // Configure failing command response
    let failing_response = MockExecResponse {
        exit_code: 1,
        success: false,
        delay: Some(Duration::from_millis(50)),
        stdout: None,
        stderr: None,
    };
    mock_docker.set_exec_response("failing command".to_string(), failing_response);

    // Create container
    let container = MockContainer::new(
        "test-container-456".to_string(),
        "failing-container".to_string(),
        "ubuntu:20.04".to_string(),
    );
    mock_docker.add_container(container);

    // Execute failing command
    let exec_config = ExecConfig {
        user: None,
        working_dir: None,
        env: HashMap::new(),
        tty: false,
        interactive: false,
        detach: false,
    };

    let start_time = std::time::Instant::now();
    let result = mock_docker
        .exec(
            "test-container-456",
            &["failing".to_string(), "command".to_string()],
            exec_config,
        )
        .await?;
    let elapsed = start_time.elapsed();

    assert_eq!(result.exit_code, 1);
    assert!(!result.success);
    assert!(elapsed >= Duration::from_millis(50)); // Verify delay was applied

    Ok(())
}

#[tokio::test]
async fn test_exec_with_tty_flag_capture() -> Result<()> {
    let mock_docker = MockDocker::new();

    // Add container
    let container = MockContainer::new(
        "tty-test-container".to_string(),
        "tty-test".to_string(),
        "ubuntu:20.04".to_string(),
    );
    mock_docker.add_container(container);

    // Test TTY enabled
    let exec_config_tty = ExecConfig {
        user: Some("testuser".to_string()),
        working_dir: Some("/app".to_string()),
        env: {
            let mut env = HashMap::new();
            env.insert("TEST_VAR".to_string(), "test_value".to_string());
            env
        },
        tty: true,
        interactive: true,
        detach: false,
    };

    let _result = mock_docker
        .exec(
            "tty-test-container",
            &[
                "bash".to_string(),
                "-c".to_string(),
                "echo test".to_string(),
            ],
            exec_config_tty,
        )
        .await?;

    // Test TTY disabled
    let exec_config_no_tty = ExecConfig {
        user: Some("testuser".to_string()),
        working_dir: Some("/app".to_string()),
        env: HashMap::new(),
        tty: false,
        interactive: false,
        detach: false,
    };

    let _result = mock_docker
        .exec(
            "tty-test-container",
            &["ls".to_string(), "-la".to_string()],
            exec_config_no_tty,
        )
        .await?;

    // Verify both exec calls captured TTY flags correctly
    let history = mock_docker.get_exec_history();
    assert_eq!(history.len(), 2);

    // First call with TTY
    assert!(history[0].config.tty);
    assert!(history[0].config.interactive);
    assert_eq!(history[0].config.user, Some("testuser".to_string()));
    assert_eq!(history[0].config.working_dir, Some("/app".to_string()));
    assert_eq!(
        history[0].config.env.get("TEST_VAR"),
        Some(&"test_value".to_string())
    );

    // Second call without TTY
    assert!(!history[1].config.tty);
    assert!(!history[1].config.interactive);

    Ok(())
}

#[tokio::test]
async fn test_container_resolution_no_running_containers() -> Result<()> {
    let mock_docker = MockDocker::new();

    // Create test workspace
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    let config = create_test_config();

    // Don't add any containers - should result in error
    let identity = ContainerIdentity::new(workspace_path, &config);
    let label_selector = identity.label_selector();
    let containers = mock_docker.list_containers(Some(&label_selector)).await?;

    assert!(containers.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_container_resolution_multiple_containers_error() -> Result<()> {
    let mock_docker = MockDocker::new();

    // Create test workspace
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    let config = create_test_config();

    // Create identity and add multiple matching containers
    let identity = ContainerIdentity::new(workspace_path, &config);
    let container1 = create_labeled_container(&identity.workspace_hash, &identity.config_hash);
    let mut container2 = create_labeled_container(&identity.workspace_hash, &identity.config_hash);
    container2.id = "test-container-456".to_string();

    mock_docker.add_container(container1);
    mock_docker.add_container(container2);

    // Should find both containers
    let label_selector = identity.label_selector();
    let containers = mock_docker.list_containers(Some(&label_selector)).await?;

    assert_eq!(containers.len(), 2);

    Ok(())
}

#[tokio::test]
async fn test_lifecycle_execution_with_mock_docker() -> Result<()> {
    let mock_docker = MockDocker::new();

    // Configure different responses for lifecycle commands
    mock_docker.set_exec_response(
        "sh -c npm install".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            delay: Some(Duration::from_millis(100)),
            stdout: None,
            stderr: None,
        },
    );

    mock_docker.set_exec_response(
        "sh -c npm run build".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            delay: Some(Duration::from_millis(200)),
            stdout: None,
            stderr: None,
        },
    );

    // Add minimal delays for other commands so timing test works
    mock_docker.set_exec_response(
        "sh -c echo 'container started'".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            delay: Some(Duration::from_millis(1)),
            stdout: None,
            stderr: None,
        },
    );

    mock_docker.set_exec_response(
        "sh -c echo 'container attached'".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            delay: Some(Duration::from_millis(1)),
            stdout: None,
            stderr: None,
        },
    );

    // Create lifecycle configuration
    let config = ContainerLifecycleConfig {
        container_id: "lifecycle-test-container".to_string(),
        user: Some("node".to_string()),
        container_workspace_folder: "/workspace/app".to_string(),
        container_env: {
            let mut env = HashMap::new();
            env.insert("NODE_ENV".to_string(), "development".to_string());
            env
        },
        skip_post_create: false,
        skip_non_blocking_commands: false,
        non_blocking_timeout: Duration::from_secs(300),
    };

    // Create lifecycle commands
    let commands = ContainerLifecycleCommands {
        on_create: Some(vec!["npm install".to_string()]),
        post_create: Some(vec!["npm run build".to_string()]),
        post_start: Some(vec!["echo 'container started'".to_string()]),
        post_attach: Some(vec!["echo 'container attached'".to_string()]),
    };

    // Create substitution context
    let temp_dir = TempDir::new()?;
    let substitution_context = SubstitutionContext::new(temp_dir.path())?;

    // Execute lifecycle
    let start_time = std::time::Instant::now();
    let result = execute_container_lifecycle_with_docker(
        &config,
        &commands,
        &substitution_context,
        &mock_docker,
    )
    .await?;
    let elapsed = start_time.elapsed();

    // Verify result
    assert_eq!(result.phases.len(), 4); // onCreate, postCreate, postStart, postAttach
    assert!(result.success());

    // Verify timing - should have at least the configured delays
    assert!(elapsed >= Duration::from_millis(300)); // 100ms + 200ms + 1ms + 1ms + processing time

    // Verify exec history
    let history = mock_docker.get_exec_history();
    assert_eq!(history.len(), 4);

    // Check specific commands were executed
    let command_strings: Vec<String> = history.iter().map(|h| h.command.join(" ")).collect();
    assert!(command_strings.contains(&"sh -c npm install".to_string()));
    assert!(command_strings.contains(&"sh -c npm run build".to_string()));
    assert!(command_strings.contains(&"sh -c echo 'container started'".to_string()));
    assert!(command_strings.contains(&"sh -c echo 'container attached'".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_lifecycle_execution_with_skip_flags() -> Result<()> {
    let mock_docker = MockDocker::new();

    // Create lifecycle configuration with skip flags
    let config = ContainerLifecycleConfig {
        container_id: "skip-test-container".to_string(),
        user: Some("root".to_string()),
        container_workspace_folder: "/workspace".to_string(),
        container_env: HashMap::new(),
        skip_post_create: true,           // Skip postCreate
        skip_non_blocking_commands: true, // Skip postStart and postAttach
        non_blocking_timeout: Duration::from_secs(300),
    };

    // Create lifecycle commands
    let commands = ContainerLifecycleCommands {
        on_create: Some(vec!["echo 'on create'".to_string()]),
        post_create: Some(vec!["echo 'post create'".to_string()]),
        post_start: Some(vec!["echo 'post start'".to_string()]),
        post_attach: Some(vec!["echo 'post attach'".to_string()]),
    };

    // Create substitution context
    let temp_dir = TempDir::new()?;
    let substitution_context = SubstitutionContext::new(temp_dir.path())?;

    // Execute lifecycle
    let result = execute_container_lifecycle_with_docker(
        &config,
        &commands,
        &substitution_context,
        &mock_docker,
    )
    .await?;

    // Verify result - only onCreate should have executed
    assert_eq!(result.phases.len(), 1);
    assert!(result.success());

    // Verify exec history - only onCreate command should be present
    let history = mock_docker.get_exec_history();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].command, vec!["sh", "-c", "echo 'on create'"]);

    Ok(())
}

#[tokio::test]
async fn test_lifecycle_execution_with_command_failure() -> Result<()> {
    let mock_docker = MockDocker::new();

    // Configure failing command in postCreate
    mock_docker.set_exec_response(
        "sh -c failing-command".to_string(),
        MockExecResponse {
            exit_code: 1,
            success: false,
            delay: None,
            stdout: None,
            stderr: None,
        },
    );

    // Create lifecycle configuration
    let config = ContainerLifecycleConfig {
        container_id: "failure-test-container".to_string(),
        user: Some("root".to_string()),
        container_workspace_folder: "/workspace".to_string(),
        container_env: HashMap::new(),
        skip_post_create: false,
        skip_non_blocking_commands: true, // Skip postStart/postAttach to focus on failure
        non_blocking_timeout: Duration::from_secs(300),
    };

    // Create lifecycle commands with a failing command
    let commands = ContainerLifecycleCommands {
        on_create: Some(vec!["echo 'success'".to_string()]),
        post_create: Some(vec!["failing-command".to_string()]),
        post_start: None,
        post_attach: None,
    };

    // Create substitution context
    let temp_dir = TempDir::new()?;
    let substitution_context = SubstitutionContext::new(temp_dir.path())?;

    // Execute lifecycle - should not fail at the top level but report failure in results
    let result = execute_container_lifecycle_with_docker(
        &config,
        &commands,
        &substitution_context,
        &mock_docker,
    )
    .await?;

    // Verify result
    assert_eq!(result.phases.len(), 2); // onCreate (success) + postCreate (failure)
    assert!(!result.success()); // Overall failure due to postCreate failure

    // Check individual phase results
    assert!(result.phases[0].success); // onCreate succeeded
    assert!(!result.phases[1].success); // postCreate failed

    // Verify exec history
    let history = mock_docker.get_exec_history();
    assert_eq!(history.len(), 2);

    Ok(())
}

#[tokio::test]
async fn test_docker_daemon_unavailable_error() -> Result<()> {
    // Configure mock to simulate daemon unavailable
    let config = MockDockerConfig {
        daemon_unavailable: true,
        ..Default::default()
    };
    let mock_docker = MockDocker::with_config(config);

    // Test ping failure
    let ping_result = mock_docker.ping().await;
    assert!(ping_result.is_err());

    // Test list_containers failure
    let list_result = mock_docker.list_containers(None).await;
    assert!(list_result.is_err());

    // Test exec failure
    let exec_config = ExecConfig {
        user: None,
        working_dir: None,
        env: HashMap::new(),
        tty: false,
        interactive: false,
        detach: false,
    };

    let exec_result = mock_docker
        .exec("any-container", &["echo".to_string()], exec_config)
        .await;
    assert!(exec_result.is_err());

    Ok(())
}

#[tokio::test]
async fn test_non_blocking_command_skip_behavior() -> Result<()> {
    let mock_docker = MockDocker::new();

    // Create lifecycle configuration with non-blocking commands skipped
    let config = ContainerLifecycleConfig {
        container_id: "non-blocking-test".to_string(),
        user: Some("root".to_string()),
        container_workspace_folder: "/workspace".to_string(),
        container_env: HashMap::new(),
        skip_post_create: false,
        skip_non_blocking_commands: true, // This should skip postStart and postAttach
        non_blocking_timeout: Duration::from_secs(300),
    };

    // Create lifecycle commands
    let commands = ContainerLifecycleCommands {
        on_create: Some(vec!["echo 'onCreate'".to_string()]),
        post_create: Some(vec!["echo 'postCreate'".to_string()]),
        post_start: Some(vec!["echo 'postStart'".to_string()]),
        post_attach: Some(vec!["echo 'postAttach'".to_string()]),
    };

    // Create substitution context
    let temp_dir = TempDir::new()?;
    let substitution_context = SubstitutionContext::new(temp_dir.path())?;

    // Execute lifecycle
    let result = execute_container_lifecycle_with_docker(
        &config,
        &commands,
        &substitution_context,
        &mock_docker,
    )
    .await?;

    // Verify result - only onCreate and postCreate should execute
    assert_eq!(result.phases.len(), 2);
    assert!(result.success());

    // Verify exec history - should only contain onCreate and postCreate
    let history = mock_docker.get_exec_history();
    assert_eq!(history.len(), 2);

    let command_strings: Vec<String> = history.iter().map(|h| h.command.join(" ")).collect();
    assert!(command_strings.contains(&"sh -c echo 'onCreate'".to_string()));
    assert!(command_strings.contains(&"sh -c echo 'postCreate'".to_string()));
    assert!(!command_strings.contains(&"sh -c echo 'postStart'".to_string()));
    assert!(!command_strings.contains(&"sh -c echo 'postAttach'".to_string()));

    Ok(())
}
