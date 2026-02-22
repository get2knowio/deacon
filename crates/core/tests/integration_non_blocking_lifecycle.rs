//! Test to demonstrate non-blocking lifecycle phases

use deacon_core::container_lifecycle::{
    execute_container_lifecycle_with_docker, AggregatedLifecycleCommand,
    ContainerLifecycleCommands, ContainerLifecycleConfig, LifecycleCommandList,
    LifecycleCommandSource, LifecycleCommandValue,
};
use deacon_core::docker::mock::{MockDocker, MockDockerConfig, MockExecResponse};
use deacon_core::variable::SubstitutionContext;
use std::collections::HashMap;
use std::time::Duration;
use tempfile::TempDir;

/// Helper to create a LifecycleCommandList from shell command strings
fn make_shell_command_list(cmds: &[&str]) -> LifecycleCommandList {
    LifecycleCommandList {
        commands: cmds
            .iter()
            .map(|cmd| AggregatedLifecycleCommand {
                command: LifecycleCommandValue::Shell(cmd.to_string()),
                source: LifecycleCommandSource::Config,
            })
            .collect(),
    }
}

#[tokio::test]
async fn test_non_blocking_phases_are_deferred() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path();
    let substitution_context = SubstitutionContext::new(workspace_path).unwrap();

    // Create a mock Docker runtime with successful responses
    let config = MockDockerConfig {
        default_exec_response: MockExecResponse {
            exit_code: 0,
            success: true,
            delay: None,
            stdout: None,
            stderr: None,
        },
        ..Default::default()
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
        use_login_shell: false,
        user_env_probe: deacon_core::container_env_probe::ContainerProbeMode::None,
        cache_folder: None,
        force_pty: false,
        dotfiles: None,
        is_prebuild: false,
    };

    // Create lifecycle commands with all phases
    let commands = ContainerLifecycleCommands::new()
        .with_on_create(make_shell_command_list(&["echo 'onCreate'"]))
        .with_post_create(make_shell_command_list(&["echo 'postCreate'"]))
        .with_post_start(make_shell_command_list(&["echo 'postStart'"]))
        .with_post_attach(make_shell_command_list(&["echo 'postAttach'"]));

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
    let mut deferred: Vec<_> = result
        .non_blocking_phases
        .iter()
        .map(|p| p.phase.as_str())
        .collect();
    deferred.sort_unstable();
    assert_eq!(deferred, vec!["postAttach", "postStart"]);

    // Verify non-blocking phase specifications (find the phases)
    let post_start_spec = result
        .non_blocking_phases
        .iter()
        .find(|spec| spec.phase.as_str() == "postStart")
        .unwrap();
    assert_eq!(post_start_spec.commands.len(), 1);
    assert_eq!(
        post_start_spec.commands[0].command,
        LifecycleCommandValue::Shell("echo 'postStart'".to_string())
    );
    assert_eq!(post_start_spec.timeout, Duration::from_secs(30));

    let post_attach_spec = result
        .non_blocking_phases
        .iter()
        .find(|spec| spec.phase.as_str() == "postAttach")
        .unwrap();
    assert_eq!(post_attach_spec.commands.len(), 1);
    assert_eq!(
        post_attach_spec.commands[0].command,
        LifecycleCommandValue::Shell("echo 'postAttach'".to_string())
    );
    assert_eq!(post_attach_spec.timeout, Duration::from_secs(30));

    // Verify that the MockDocker received exec calls for blocking phases only
    let exec_history = docker.get_exec_history();
    assert_eq!(exec_history.len(), 2); // Only onCreate and postCreate should have been executed

    println!("✓ Non-blocking phases are properly deferred for later execution");
    println!("✓ Blocking phases (onCreate, postCreate) execute immediately");
    println!("✓ Non-blocking phases (postStart, postAttach) are marked for background execution");
    println!(
        "✓ MockDocker confirms only {} exec calls were made for blocking phases",
        exec_history.len()
    );
}

#[tokio::test]
async fn test_skip_non_blocking_commands_behavior() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path();
    let substitution_context = SubstitutionContext::new(workspace_path).unwrap();

    // Create a mock Docker runtime with successful responses
    let config = MockDockerConfig {
        default_exec_response: MockExecResponse {
            exit_code: 0,
            success: true,
            delay: None,
            stdout: None,
            stderr: None,
        },
        ..Default::default()
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
        use_login_shell: false,
        user_env_probe: deacon_core::container_env_probe::ContainerProbeMode::None,
        cache_folder: None,
        force_pty: false,
        dotfiles: None,
        is_prebuild: false,
    };

    // Create lifecycle commands with all phases
    let commands = ContainerLifecycleCommands::new()
        .with_on_create(make_shell_command_list(&["echo 'onCreate'"]))
        .with_post_create(make_shell_command_list(&["echo 'postCreate'"]))
        .with_post_start(make_shell_command_list(&["echo 'postStart'"]))
        .with_post_attach(make_shell_command_list(&["echo 'postAttach'"]));

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
    println!(
        "✓ MockDocker confirms only {} exec calls were made",
        exec_history.len()
    );
}

#[tokio::test]
async fn test_non_blocking_phases_sync_execution() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path();
    let substitution_context = SubstitutionContext::new(workspace_path).unwrap();

    // Create a mock Docker runtime with successful responses
    let config = MockDockerConfig {
        default_exec_response: MockExecResponse {
            exit_code: 0,
            success: true,
            delay: None,
            stdout: None,
            stderr: None,
        },
        ..Default::default()
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
        use_login_shell: false,
        user_env_probe: deacon_core::container_env_probe::ContainerProbeMode::None,
        cache_folder: None,
        force_pty: false,
        dotfiles: None,
        is_prebuild: false,
    };

    // Create lifecycle commands with all phases
    let commands = ContainerLifecycleCommands::new()
        .with_on_create(make_shell_command_list(&["echo 'onCreate'"]))
        .with_post_create(make_shell_command_list(&["echo 'postCreate'"]))
        .with_post_start(make_shell_command_list(&["echo 'postStart'"]))
        .with_post_attach(make_shell_command_list(&["echo 'postAttach'"]));

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
    let final_result = result
        .execute_non_blocking_phases_sync(&docker)
        .await
        .unwrap();

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
    println!(
        "✓ MockDocker confirms all {} phases were executed",
        exec_history.len()
    );
}

#[tokio::test]
async fn test_non_blocking_phase_command_failures_are_handled() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path();
    let substitution_context = SubstitutionContext::new(workspace_path).unwrap();

    // Create a mock Docker runtime with specific command responses
    let mut exec_responses = HashMap::new();
    // Success responses for blocking phases
    exec_responses.insert(
        "sh -c echo 'onCreate'".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            delay: None,
            stdout: None,
            stderr: None,
        },
    );
    exec_responses.insert(
        "sh -c echo 'postCreate'".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            delay: None,
            stdout: None,
            stderr: None,
        },
    );
    // Failure responses for non-blocking phases - but still return Ok with success: false
    exec_responses.insert(
        "sh -c echo 'postStart'".to_string(),
        MockExecResponse {
            exit_code: 1,
            success: false,
            delay: None,
            stdout: None,
            stderr: None,
        },
    );
    exec_responses.insert(
        "sh -c echo 'postAttach'".to_string(),
        MockExecResponse {
            exit_code: 1,
            success: false,
            delay: None,
            stdout: None,
            stderr: None,
        },
    );

    let config = MockDockerConfig {
        exec_responses,
        ..Default::default()
    };
    let docker = MockDocker::with_config(config);

    // Create lifecycle configuration with non-blocking commands enabled
    let lifecycle_config = ContainerLifecycleConfig {
        container_id: "test-container".to_string(),
        user: Some("root".to_string()),
        container_workspace_folder: "/workspace".to_string(),
        container_env: HashMap::new(),
        skip_post_create: false,
        skip_non_blocking_commands: false,
        non_blocking_timeout: Duration::from_secs(30),
        use_login_shell: false,
        user_env_probe: deacon_core::container_env_probe::ContainerProbeMode::None,
        cache_folder: None,
        force_pty: false,
        dotfiles: None,
        is_prebuild: false,
    };

    // Create lifecycle commands with all phases
    let commands = ContainerLifecycleCommands::new()
        .with_on_create(make_shell_command_list(&["echo 'onCreate'"]))
        .with_post_create(make_shell_command_list(&["echo 'postCreate'"]))
        .with_post_start(make_shell_command_list(&["echo 'postStart'"]))
        .with_post_attach(make_shell_command_list(&["echo 'postAttach'"]));

    // Execute lifecycle commands
    let result = execute_container_lifecycle_with_docker(
        &lifecycle_config,
        &commands,
        &substitution_context,
        &docker,
    )
    .await
    .unwrap();

    // Verify that non-blocking phases are scheduled
    assert_eq!(result.non_blocking_phases.len(), 2);

    // Execute non-blocking phases synchronously which should have failed commands
    let final_result = result
        .execute_non_blocking_phases_sync(&docker)
        .await
        .unwrap();

    // The non-blocking phases should complete but with failed commands
    assert_eq!(final_result.phases.len(), 4); // All phases should be present
    assert_eq!(final_result.non_blocking_phases.len(), 0); // Should be empty after execution

    // Check that postStart and postAttach phases are marked as failed
    let post_start_phase = final_result
        .phases
        .iter()
        .find(|p| p.phase.as_str() == "postStart")
        .unwrap();
    let post_attach_phase = final_result
        .phases
        .iter()
        .find(|p| p.phase.as_str() == "postAttach")
        .unwrap();

    assert!(
        !post_start_phase.success,
        "postStart phase should be marked as failed"
    );
    assert!(
        !post_attach_phase.success,
        "postAttach phase should be marked as failed"
    );

    // When the lifecycle engine returns Err for a non-blocking phase (fail-fast),
    // the PhaseResult may have an empty commands vector. When it returns Ok with
    // failure status, commands will be populated. Check either case.
    if !post_start_phase.commands.is_empty() {
        assert!(
            !post_start_phase.commands[0].success,
            "postStart command should have failed"
        );
        assert_eq!(post_start_phase.commands[0].exit_code, 1);
    }
    if !post_attach_phase.commands.is_empty() {
        assert!(
            !post_attach_phase.commands[0].success,
            "postAttach command should have failed"
        );
        assert_eq!(post_attach_phase.commands[0].exit_code, 1);
    }

    // Background errors should be empty since we don't have actual exceptions
    assert_eq!(final_result.background_errors.len(), 0);

    // Non-blocking phases should still not block the main flow (no panic/error)
    assert_eq!(final_result.non_blocking_phases.len(), 0); // Should be empty after execution

    println!("Non-blocking phase command failures are properly handled");
    println!(
        "{} phases completed with proper success status",
        final_result.phases.len()
    );
}

#[tokio::test]
async fn test_non_blocking_phase_timeout_handling() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path();
    let substitution_context = SubstitutionContext::new(workspace_path).unwrap();

    // Create a mock Docker runtime with long delays that will cause timeout
    let config = MockDockerConfig {
        default_exec_response: MockExecResponse {
            exit_code: 0,
            success: true,
            delay: Some(Duration::from_secs(5)), // Long delay
            stdout: None,
            stderr: None,
        },
        ..Default::default()
    };
    let docker = MockDocker::with_config(config);

    // Create lifecycle configuration with very short timeout
    let lifecycle_config = ContainerLifecycleConfig {
        container_id: "test-container".to_string(),
        user: Some("root".to_string()),
        container_workspace_folder: "/workspace".to_string(),
        container_env: HashMap::new(),
        skip_post_create: false,
        skip_non_blocking_commands: false,
        non_blocking_timeout: Duration::from_millis(100), // Very short timeout
        use_login_shell: false,
        user_env_probe: deacon_core::container_env_probe::ContainerProbeMode::None,
        cache_folder: None,
        force_pty: false,
        dotfiles: None,
        is_prebuild: false,
    };

    // Create lifecycle commands
    let commands = ContainerLifecycleCommands::new()
        .with_on_create(make_shell_command_list(&["echo 'onCreate'"]))
        .with_post_create(make_shell_command_list(&["echo 'postCreate'"]))
        .with_post_start(make_shell_command_list(&["echo 'postStart'"]));

    // Execute lifecycle commands
    let result = execute_container_lifecycle_with_docker(
        &lifecycle_config,
        &commands,
        &substitution_context,
        &docker,
    )
    .await
    .unwrap();

    // Execute non-blocking phases synchronously which should timeout
    let final_result = result
        .execute_non_blocking_phases_sync(&docker)
        .await
        .unwrap();

    // Verify that timeout errors are properly aggregated
    assert_eq!(final_result.background_errors.len(), 1); // postStart should timeout
    assert!(final_result
        .background_errors
        .iter()
        .any(|err| err.contains("timed out")));
    assert!(final_result
        .background_errors
        .iter()
        .any(|err| err.contains("postStart")));

    println!("✓ Non-blocking phase timeouts are properly handled");
    println!(
        "✓ Timeout error properly aggregated: {}",
        final_result.background_errors[0]
    );
}

#[tokio::test]
async fn test_non_blocking_phases_with_progress_streaming() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path();
    let substitution_context = SubstitutionContext::new(workspace_path).unwrap();

    // Create a mock Docker runtime with successful responses
    let config = MockDockerConfig {
        default_exec_response: MockExecResponse {
            exit_code: 0,
            success: true,
            delay: None,
            stdout: None,
            stderr: None,
        },
        ..Default::default()
    };
    let docker = MockDocker::with_config(config);

    // Create lifecycle configuration with non-blocking commands enabled
    let lifecycle_config = ContainerLifecycleConfig {
        container_id: "test-container".to_string(),
        user: Some("root".to_string()),
        container_workspace_folder: "/workspace".to_string(),
        container_env: HashMap::new(),
        skip_post_create: false,
        skip_non_blocking_commands: false,
        non_blocking_timeout: Duration::from_secs(30),
        use_login_shell: false,
        user_env_probe: deacon_core::container_env_probe::ContainerProbeMode::None,
        cache_folder: None,
        force_pty: false,
        dotfiles: None,
        is_prebuild: false,
    };

    // Create lifecycle commands with non-blocking phases
    let commands = ContainerLifecycleCommands::new()
        .with_on_create(make_shell_command_list(&["echo 'onCreate'"]))
        .with_post_create(make_shell_command_list(&["echo 'postCreate'"]))
        .with_post_start(make_shell_command_list(&["echo 'postStart'"]))
        .with_post_attach(make_shell_command_list(&["echo 'postAttach'"]));

    // Execute lifecycle commands
    let result = execute_container_lifecycle_with_docker(
        &lifecycle_config,
        &commands,
        &substitution_context,
        &docker,
    )
    .await
    .unwrap();

    // Verify that non-blocking phases are scheduled
    assert_eq!(result.non_blocking_phases.len(), 2);

    // Set up progress event tracking
    use std::sync::{Arc, Mutex};
    let captured_events = Arc::new(Mutex::new(Vec::new()));
    let captured_events_clone = captured_events.clone();

    let progress_callback = move |event: deacon_core::progress::ProgressEvent| {
        captured_events_clone.lock().unwrap().push(event);
        Ok(())
    };

    // Execute non-blocking phases with progress streaming
    let final_result = result
        .execute_non_blocking_phases_sync_with_callback(&docker, Some(progress_callback))
        .await
        .unwrap();

    // Verify that all phases completed
    assert_eq!(final_result.phases.len(), 4); // All phases should now be complete
    assert_eq!(final_result.non_blocking_phases.len(), 0);

    // Verify that progress events were emitted
    let events = captured_events.lock().unwrap();
    assert!(
        !events.is_empty(),
        "Progress events should have been captured"
    );

    // Count phase begin/end events for non-blocking phases
    let phase_begin_count = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                deacon_core::progress::ProgressEvent::LifecyclePhaseBegin { .. }
            )
        })
        .count();
    let phase_end_count = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                deacon_core::progress::ProgressEvent::LifecyclePhaseEnd { .. }
            )
        })
        .count();

    // Should have begin/end for postStart and postAttach
    assert_eq!(phase_begin_count, 2, "Should have 2 phase begin events");
    assert_eq!(phase_end_count, 2, "Should have 2 phase end events");

    // Count command begin/end events (one command per phase)
    let command_begin_count = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                deacon_core::progress::ProgressEvent::LifecycleCommandBegin { .. }
            )
        })
        .count();
    let command_end_count = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                deacon_core::progress::ProgressEvent::LifecycleCommandEnd { .. }
            )
        })
        .count();

    assert_eq!(command_begin_count, 2, "Should have 2 command begin events");
    assert_eq!(command_end_count, 2, "Should have 2 command end events");

    println!("✓ Non-blocking phases emit progress events during execution");
    println!("✓ Captured {} total progress events", events.len());
    println!(
        "✓ Phase events: {} begin, {} end",
        phase_begin_count, phase_end_count
    );
    println!(
        "✓ Command events: {} begin, {} end",
        command_begin_count, command_end_count
    );
}
