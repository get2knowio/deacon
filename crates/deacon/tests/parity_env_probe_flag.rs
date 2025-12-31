use deacon::commands::exec::{execute_exec_with_docker, ExecArgs};
use deacon::commands::shared::resolve_env_and_user;
use deacon_core::container_env_probe::ContainerProbeMode;
use deacon_core::docker::mock::{MockContainer, MockDocker};
use deacon_core::IndexMap;
use std::collections::HashMap;

#[tokio::test]
async fn exec_honors_default_user_env_probe_login_shell() {
    let mock = MockDocker::new();
    mock.add_container(MockContainer::new(
        "probe-exec-1".to_string(),
        "probe-exec-1".to_string(),
        "ubuntu:22.04".to_string(),
    ));

    let args = ExecArgs {
        user: None,
        no_tty: true,
        env: Vec::new(),
        workdir: Some("/".to_string()),
        container_id: Some("probe-exec-1".to_string()),
        id_label: Vec::new(),
        service: None,
        command: vec!["echo".to_string(), "hello".to_string()],
        workspace_folder: None,
        config_path: None,
        override_config_path: None,
        secrets_files: Vec::new(),
        docker_path: "docker".to_string(),
        docker_compose_path: "docker-compose".to_string(),
        env_file: Vec::new(),
        force_tty_if_json: false,
        default_user_env_probe: Some(ContainerProbeMode::LoginShell),
        container_data_folder: None,
        container_system_data_folder: None,
        terminal_dimensions: None,
    };

    let res = execute_exec_with_docker(args, &mock).await;
    assert!(res.is_ok());

    let history = mock.get_exec_history();
    let probe_command = history
        .iter()
        .map(|call| call.command.join(" "))
        .find(|cmd| cmd.contains("env 2>/dev/null"));

    assert!(
        probe_command
            .as_deref()
            .is_some_and(|cmd| cmd.contains("-lc 'env 2>/dev/null'")),
        "expected login shell probe command in history, got {:?}",
        history
    );
}

#[tokio::test]
async fn up_shared_probe_helper_uses_login_shell() {
    let mock = MockDocker::new();
    mock.add_container(MockContainer::new(
        "probe-up-1".to_string(),
        "probe-up-1".to_string(),
        "ubuntu:22.04".to_string(),
    ));

    let cli_env: IndexMap<String, String> = IndexMap::new();
    let config_remote_env: HashMap<String, Option<String>> = HashMap::new();

    let resolution = resolve_env_and_user(
        &mock,
        "probe-up-1",
        None,
        Some("root".to_string()),
        ContainerProbeMode::LoginShell,
        Some(&config_remote_env),
        &cli_env,
        None,
    )
    .await;

    let history = mock.get_exec_history();
    let probe_command = history
        .iter()
        .map(|call| call.command.join(" "))
        .find(|cmd| cmd.contains("env 2>/dev/null"));

    assert!(
        probe_command
            .as_deref()
            .is_some_and(|cmd| cmd.contains("-lc 'env 2>/dev/null'")),
        "expected login shell probe command in history, got {:?}",
        history
    );

    assert_eq!(resolution.effective_user.as_deref(), Some("root"));
}
