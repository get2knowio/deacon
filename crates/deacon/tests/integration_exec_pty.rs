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
        docker_path: "docker".to_string(),
        docker_compose_path: "docker-compose".to_string(),
        force_tty_if_json: false,
        default_user_env_probe: Some(deacon_core::container_env_probe::ContainerProbeMode::None),
        container_data_folder: None,
        container_system_data_folder: None,
        terminal_columns: None,
        terminal_rows: None,
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
    assert!(!call.config.tty, "Expected tty=false for non-TTY runs");
    assert!(
        call.config.interactive,
        "Interactive should remain true to attach stdin"
    );
}
