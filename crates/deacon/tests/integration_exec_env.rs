use deacon::commands::exec::{execute_exec_with_docker, ExecArgs};
use deacon_core::docker::mock::{MockContainer, MockDocker};

use std::collections::HashMap;

#[tokio::test]
async fn integration_exec_applies_merged_env() {
    // Setup mock docker with a container that has labels and env
    let mock = MockDocker::new();

    let mut labels = HashMap::new();
    labels.insert("deacon.remoteEnv.A".to_string(), "label_a".to_string());

    let mut env = HashMap::new();
    env.insert("FROM_CONTAINER".to_string(), "container_val".to_string());

    let container = MockContainer::new(
        "test-merge-1".to_string(),
        "test-merge-1".to_string(),
        "myimage:latest".to_string(),
    )
    .with_labels(labels)
    .with_env(env);

    mock.add_container(container);

    // Prepare args to target this container by id
    let args = ExecArgs {
        user: None,
        no_tty: true,
        env: vec!["CLI_B=cli_b".to_string(), "A=cli_override".to_string()],
        workdir: Some("/".to_string()),
        container_id: Some("test-merge-1".to_string()),
        id_label: vec![],
        service: None,
        command: vec!["echo".to_string(), "hello".to_string()],
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

    // Execute
    let res = execute_exec_with_docker(args, &mock).await;

    // Expect Ok (exec will call mock.exec and return; mock exec default success)
    assert!(res.is_ok());

    // Verify exec history
    let history = mock.get_exec_history();
    assert_eq!(history.len(), 1);
    let call = &history[0];

    // Build env map from recorded ExecConfig
    let recorded_env = &call.config.env;

    // CLI A should override label A
    assert_eq!(
        recorded_env.get("A").map(String::as_str),
        Some("cli_override")
    );
    // CLI_B should be present
    assert_eq!(recorded_env.get("CLI_B").map(String::as_str), Some("cli_b"));
    // FROM_CONTAINER should NOT be present when targeting by direct container ID:
    // probe is disabled and container env is not merged, so only CLI env applies.
    assert!(recorded_env.get("FROM_CONTAINER").is_none());
}
