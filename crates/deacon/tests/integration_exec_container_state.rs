//! Integration tests for BEAD-12: container running-state validation in exec.
//!
//! Both `--container-id` and `--id-label` reach `resolve_container` → `inspect`,
//! which returns containers regardless of state. Exec must fail fast with
//! "Dev container is not running." if the resolved container is stopped/exited.

use deacon::commands::exec::{execute_exec_with_docker, ExecArgs};
use deacon_core::docker::mock::{MockContainer, MockDocker};

fn make_args(container_id: Option<String>, id_label: Vec<String>) -> ExecArgs {
    ExecArgs {
        user: None,
        no_tty: true,
        remote_env: vec![],
        workdir: Some("/".to_string()),
        container_id,
        id_label,
        mount_workspace_git_root: true,
        service: None,
        command: vec!["echo".to_string(), "hi".to_string()],
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
    }
}

/// BEAD-12-T01: --container-id pointing at a stopped container must fail with
/// the spec error message, not attempt any exec call.
#[tokio::test]
async fn exec_container_id_stopped_container_errors_not_running() {
    let mock = MockDocker::new();
    let stopped = MockContainer::new(
        "stopped-1".to_string(),
        "stopped-1".to_string(),
        "alpine:3.18".to_string(),
    )
    .with_state("exited".to_string(), "Exited (0) 1 minute ago".to_string());
    mock.add_container(stopped);

    let res =
        execute_exec_with_docker(make_args(Some("stopped-1".to_string()), vec![]), &mock).await;

    let err = res.expect_err("exec should fail when container is not running");
    assert!(
        err.to_string().contains("Dev container is not running"),
        "expected 'Dev container is not running' message, got: {}",
        err
    );

    // No exec calls should have been made
    assert!(
        mock.get_exec_history().is_empty(),
        "should bail before any exec call"
    );
}

/// BEAD-12-T02: --container-id pointing at a running container proceeds normally.
#[tokio::test]
async fn exec_container_id_running_container_succeeds() {
    let mock = MockDocker::new();
    let running = MockContainer::new(
        "running-1".to_string(),
        "running-1".to_string(),
        "alpine:3.18".to_string(),
    ); // MockContainer::new defaults to state="running"
    mock.add_container(running);

    let res =
        execute_exec_with_docker(make_args(Some("running-1".to_string()), vec![]), &mock).await;

    assert!(
        res.is_ok(),
        "exec should succeed against running container: {:?}",
        res.err()
    );
    assert!(
        !mock.get_exec_history().is_empty(),
        "should have made at least one exec call"
    );
}

/// BEAD-12-T03: --id-label resolving to a stopped container must also fail
/// (the brief flagged that the label path did not filter by state previously).
#[tokio::test]
async fn exec_id_label_stopped_container_errors_not_running() {
    let mock = MockDocker::new();
    let mut labels = std::collections::HashMap::new();
    labels.insert(
        "devcontainer.local_folder".to_string(),
        "/abs/path".to_string(),
    );
    let stopped = MockContainer::new(
        "label-stopped-1".to_string(),
        "label-stopped-1".to_string(),
        "alpine:3.18".to_string(),
    )
    .with_labels(labels)
    .with_state(
        "exited".to_string(),
        "Exited (0) 30 seconds ago".to_string(),
    );
    mock.add_container(stopped);

    let res = execute_exec_with_docker(
        make_args(
            None,
            vec!["devcontainer.local_folder=/abs/path".to_string()],
        ),
        &mock,
    )
    .await;

    let err = res.expect_err("label-resolved stopped container should fail");
    assert!(
        err.to_string().contains("Dev container is not running"),
        "expected 'Dev container is not running' message, got: {}",
        err
    );
    assert!(
        mock.get_exec_history().is_empty(),
        "should bail before any exec call"
    );
}
