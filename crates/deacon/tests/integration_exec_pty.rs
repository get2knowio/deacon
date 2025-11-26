use deacon::commands::exec::{execute_exec_with_docker, ExecArgs};
use deacon_core::docker::mock::{MockContainer, MockDocker, MockExecResponse};

#[tokio::test]
async fn integration_exec_non_tty_preserves_streams_and_tty_flag() {
    // Setup mock docker and a container
    let mock = MockDocker::new();

    // Ensure default exec response contains stdout/stderr and non-zero exit
    mock.update_config(|cfg| {
        cfg.default_exec_response = MockExecResponse {
            exit_code: 0,
            success: true,
            delay: None,
            stdout: Some("mock-stdout".to_string()),
            stderr: Some("mock-stderr".to_string()),
        };
    });

    let container = MockContainer::new(
        "test-pty-1".to_string(),
        "test-pty-1".to_string(),
        "myimage:latest".to_string(),
    );

    mock.add_container(container);

    // Build args for non-TTY run (disable TTY)
    let args = ExecArgs {
        user: None,
        no_tty: true, // non-TTY
        env: vec![],
        workdir: Some("/".to_string()),
        container_id: Some("test-pty-1".to_string()),
        id_label: vec![],
        service: None,
        command: vec!["sh".to_string(), "-c".to_string(), "echo hello".to_string()],
        workspace_folder: None,
        config_path: None,
        override_config_path: None,
        secrets_files: Vec::new(),
        docker_path: "docker".to_string(),
        docker_compose_path: "docker-compose".to_string(),
        env_file: Vec::new(),
        force_tty_if_json: false,
        default_user_env_probe: Some(deacon_core::container_env_probe::ContainerProbeMode::None),
        container_data_folder: None,
        container_system_data_folder: None,
        terminal_dimensions: None,
    };

    // Execute - should complete and be recorded by the mock
    let res = execute_exec_with_docker(args, &mock).await;

    assert!(res.is_ok(), "exec call failed: {:?}", res.err());

    // Verify exec history and that tty was not requested
    let history = mock.get_exec_history();
    assert_eq!(history.len(), 1);
    let call = &history[0];

    assert_eq!(call.container_id, "test-pty-1");
    // Non-TTY run should have tty == false but interactive true (stdin attached)
    assert!(!call.config.tty, "Expected tty=false for non-PTY runs");
    assert!(
        call.config.interactive,
        "Interactive should remain true to attach stdin"
    );
}

/// Regression test for FR-006: Exec command PTY behavior is unaffected by up lifecycle PTY toggle
///
/// This test verifies that the exec command:
/// 1. Does NOT use resolve_force_pty() from up.rs
/// 2. Continues to use its own compute_should_use_tty() function
/// 3. The force_tty_if_json field is handled independently through build_exec_config
/// 4. PTY allocation follows exec's existing logic, not up's lifecycle logic
///
/// Per spec FR-006: "Exec entry points outside the `up` lifecycle MUST retain their existing TTY
/// behavior and log separation; introducing the PTY toggle MUST NOT change their defaults or
/// exit-code handling."
#[test]
fn integration_exec_pty_behavior_unaffected_by_force_tty_if_json() {
    use std::collections::HashMap;

    use deacon::commands::exec::build_exec_config;

    // Test 1: force_tty_if_json=false (default) with no_tty=false should use standard logic
    let args_without_force = ExecArgs {
        user: None,
        no_tty: false,
        env: vec![],
        workdir: Some("/".to_string()),
        container_id: Some("test-pty-regression".to_string()),
        id_label: vec![],
        service: None,
        command: vec!["test".to_string()],
        workspace_folder: None,
        config_path: None,
        override_config_path: None,
        secrets_files: Vec::new(),
        docker_path: "docker".to_string(),
        docker_compose_path: "docker-compose".to_string(),
        env_file: Vec::new(),
        force_tty_if_json: false, // Default: no forced PTY
        default_user_env_probe: Some(deacon_core::container_env_probe::ContainerProbeMode::None),
        container_data_folder: None,
        container_system_data_folder: None,
        terminal_dimensions: None,
    };

    let exec_config = build_exec_config(
        &args_without_force,
        "/".to_string(),
        HashMap::new(),
        false, // stdin_is_tty=false (simulating non-TTY test environment)
        false, // stdout_is_tty=false
    );

    // In non-TTY environment with force_tty_if_json=false, should not allocate PTY
    assert!(
        !exec_config.tty,
        "Default exec should not allocate PTY in non-TTY environment"
    );

    // Test 2: force_tty_if_json=true should force PTY allocation
    let args_with_force = ExecArgs {
        user: None,
        no_tty: false,
        env: vec![],
        workdir: Some("/".to_string()),
        container_id: Some("test-pty-regression".to_string()),
        id_label: vec![],
        service: None,
        command: vec!["test".to_string()],
        workspace_folder: None,
        config_path: None,
        override_config_path: None,
        secrets_files: Vec::new(),
        docker_path: "docker".to_string(),
        docker_compose_path: "docker-compose".to_string(),
        env_file: Vec::new(),
        force_tty_if_json: true, // Force PTY
        default_user_env_probe: Some(deacon_core::container_env_probe::ContainerProbeMode::None),
        container_data_folder: None,
        container_system_data_folder: None,
        terminal_dimensions: None,
    };

    let exec_config = build_exec_config(
        &args_with_force,
        "/".to_string(),
        HashMap::new(),
        false, // stdin_is_tty=false
        false, // stdout_is_tty=false
    );

    // With force_tty_if_json=true, should force PTY allocation
    assert!(
        exec_config.tty,
        "force_tty_if_json=true should allocate PTY"
    );

    // Test 3: force_tty_if_json=true takes precedence over no_tty=true
    // This verifies that exec uses compute_should_use_tty which checks force_tty first
    let args_no_tty_override = ExecArgs {
        user: None,
        no_tty: true, // Explicit no-TTY
        env: vec![],
        workdir: Some("/".to_string()),
        container_id: Some("test-pty-regression".to_string()),
        id_label: vec![],
        service: None,
        command: vec!["test".to_string()],
        workspace_folder: None,
        config_path: None,
        override_config_path: None,
        secrets_files: Vec::new(),
        docker_path: "docker".to_string(),
        docker_compose_path: "docker-compose".to_string(),
        env_file: Vec::new(),
        force_tty_if_json: true, // Force takes precedence
        default_user_env_probe: Some(deacon_core::container_env_probe::ContainerProbeMode::None),
        container_data_folder: None,
        container_system_data_folder: None,
        terminal_dimensions: None,
    };

    let exec_config = build_exec_config(
        &args_no_tty_override,
        "/".to_string(),
        HashMap::new(),
        false,
        false,
    );

    // Per compute_should_use_tty: force_tty takes precedence over no_tty
    assert!(
        exec_config.tty,
        "force_tty takes precedence over no_tty in exec"
    );

    // Test 4: no_tty=true without force should disable PTY even with TTY environment
    let args_no_tty = ExecArgs {
        user: None,
        no_tty: true,
        env: vec![],
        workdir: Some("/".to_string()),
        container_id: Some("test-pty-regression".to_string()),
        id_label: vec![],
        service: None,
        command: vec!["test".to_string()],
        workspace_folder: None,
        config_path: None,
        override_config_path: None,
        secrets_files: Vec::new(),
        docker_path: "docker".to_string(),
        docker_compose_path: "docker-compose".to_string(),
        env_file: Vec::new(),
        force_tty_if_json: false,
        default_user_env_probe: Some(deacon_core::container_env_probe::ContainerProbeMode::None),
        container_data_folder: None,
        container_system_data_folder: None,
        terminal_dimensions: None,
    };

    let exec_config = build_exec_config(
        &args_no_tty,
        "/".to_string(),
        HashMap::new(),
        true, // stdin_is_tty=true (simulating TTY environment)
        true, // stdout_is_tty=true
    );

    // no_tty should prevent PTY allocation even in TTY environment
    assert!(!exec_config.tty, "no_tty should prevent PTY allocation");
}
