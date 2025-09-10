//! Up command implementation
//!
//! Implements the `deacon up` subcommand for starting development containers.
//! Supports both traditional container workflows and Docker Compose workflows.

use anyhow::Result;
use deacon_core::compose::{ComposeManager, ComposeProject};
use deacon_core::config::{ConfigLoader, DevContainerConfig};
use deacon_core::docker::{CliDocker, Docker, ExecConfig};
use deacon_core::errors::{DeaconError, DockerError};
use std::path::{Path, PathBuf};
use tracing::{debug, info, instrument, warn};

/// Up command arguments
#[derive(Debug, Clone)]
pub struct UpArgs {
    pub remove_existing_container: bool,
    pub skip_post_create: bool,
    pub workspace_folder: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
}

/// Execute the up command
#[instrument(skip(args))]
pub async fn execute_up(args: UpArgs) -> Result<()> {
    info!("Starting up command execution");
    debug!("Up args: {:?}", args);

    // Load configuration
    let workspace_folder = args.workspace_folder.as_deref().unwrap_or(Path::new("."));

    let config = if let Some(config_path) = args.config_path.as_ref() {
        ConfigLoader::load_from_path(config_path)?
    } else {
        let config_location = ConfigLoader::discover_config(workspace_folder)?;
        if !config_location.exists() {
            return Err(
                DeaconError::Config(deacon_core::errors::ConfigError::NotFound {
                    path: config_location.path().to_string_lossy().to_string(),
                })
                .into(),
            );
        }
        ConfigLoader::load_from_path(config_location.path())?
    };

    debug!("Loaded configuration: {:?}", config.name);

    // Check if this is a compose-based configuration
    if config.uses_compose() {
        execute_compose_up(&config, workspace_folder, &args).await
    } else {
        execute_container_up(&config, workspace_folder, &args).await
    }
}

/// Execute up for Docker Compose configurations
#[instrument(skip(config, workspace_folder, args))]
async fn execute_compose_up(
    config: &DevContainerConfig,
    workspace_folder: &Path,
    args: &UpArgs,
) -> Result<()> {
    info!("Starting Docker Compose project");

    let compose_manager = ComposeManager::new();
    let project = compose_manager.create_project(config, workspace_folder)?;

    debug!("Created compose project: {:?}", project.name);

    // Check if project is already running
    if !args.remove_existing_container && compose_manager.is_project_running(&project)? {
        info!("Compose project {} is already running", project.name);

        // Get the primary container ID for potential exec operations
        if let Some(container_id) = compose_manager.get_primary_container_id(&project)? {
            info!("Primary service container ID: {}", container_id);
        }

        return Ok(());
    }

    // Stop existing containers if requested
    if args.remove_existing_container {
        info!("Stopping existing compose project");
        if let Err(e) = compose_manager.stop_project(&project) {
            warn!("Failed to stop existing project: {}", e);
        }
    }

    // Start the compose project
    compose_manager.start_project(&project)?;

    info!("Compose project {} started successfully", project.name);

    // Execute post-create lifecycle if not skipped
    if !args.skip_post_create {
        execute_compose_post_create(&project, config).await?;
    }

    Ok(())
}

/// Execute up for traditional container configurations
#[instrument(skip_all)]
async fn execute_container_up(
    _config: &DevContainerConfig,
    _workspace_folder: &Path,
    _args: &UpArgs,
) -> Result<()> {
    info!("Starting traditional development container");

    // For now, return an error indicating this needs implementation
    // The existing CLI implementation in cli.rs handles this case
    Err(DeaconError::Docker(DockerError::CLIError(
        "Traditional container up is not yet fully implemented in this command. Use existing CLI workflow.".to_string()
    )).into())
}

/// Execute post-create lifecycle for compose projects
#[instrument(skip(project, config))]
async fn execute_compose_post_create(
    project: &ComposeProject,
    config: &DevContainerConfig,
) -> Result<()> {
    info!("Executing post-create lifecycle for compose project");

    // Get the primary container ID
    let compose_manager = ComposeManager::new();
    let container_id = match compose_manager.get_primary_container_id(project)? {
        Some(id) => id,
        None => {
            warn!("Primary service container not found, skipping post-create");
            return Ok(());
        }
    };

    debug!(
        "Running post-create commands in container: {}",
        container_id
    );

    // Execute postCreateCommand if specified
    if let Some(post_create_cmd) = &config.post_create_command {
        if let Some(cmd_str) = post_create_cmd.as_str() {
            info!("Executing postCreateCommand: {}", cmd_str);

            let docker = CliDocker::new();
            let result = docker
                .exec(
                    &container_id,
                    &["sh".to_string(), "-c".to_string(), cmd_str.to_string()],
                    ExecConfig {
                        user: None,
                        working_dir: None,
                        env: std::collections::HashMap::new(),
                        tty: false,
                        interactive: false,
                        detach: false,
                    },
                )
                .await;

            match result {
                Ok(_) => info!("postCreateCommand completed successfully"),
                Err(e) => warn!("postCreateCommand failed: {}", e),
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::config::DevContainerConfig;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn test_up_args_creation() {
        let args = UpArgs {
            remove_existing_container: true,
            skip_post_create: false,
            workspace_folder: Some(PathBuf::from("/test")),
            config_path: None,
        };

        assert!(args.remove_existing_container);
        assert!(!args.skip_post_create);
        assert_eq!(args.workspace_folder, Some(PathBuf::from("/test")));
        assert!(args.config_path.is_none());
    }

    #[test]
    fn test_compose_config_detection() {
        let compose_config = DevContainerConfig {
            extends: None,
            name: Some("Test Compose".to_string()),
            image: None,
            dockerfile: None,
            build: None,
            docker_compose_file: Some(json!("docker-compose.yml")),
            service: Some("app".to_string()),
            run_services: vec!["db".to_string()],
            features: serde_json::Value::Object(Default::default()),
            customizations: serde_json::Value::Object(Default::default()),
            workspace_folder: None,
            mounts: vec![],
            container_env: HashMap::new(),
            remote_env: HashMap::new(),
            forward_ports: vec![],
            app_port: None,
            ports_attributes: HashMap::new(),
            other_ports_attributes: None,
            run_args: vec![],
            shutdown_action: Some("stopCompose".to_string()),
            override_command: None,
            on_create_command: None,
            post_start_command: None,
            post_create_command: Some(json!("echo 'Container ready'")),
            post_attach_command: None,
            initialize_command: None,
            update_content_command: None,
        };

        assert!(compose_config.uses_compose());
        assert!(compose_config.has_stop_compose_shutdown());
    }

    #[test]
    fn test_traditional_config_detection() {
        let traditional_config = DevContainerConfig {
            extends: None,
            name: Some("Test Traditional".to_string()),
            image: Some("node:18".to_string()),
            dockerfile: None,
            build: None,
            docker_compose_file: None,
            service: None,
            run_services: vec![],
            features: serde_json::Value::Object(Default::default()),
            customizations: serde_json::Value::Object(Default::default()),
            workspace_folder: None,
            mounts: vec![],
            container_env: HashMap::new(),
            remote_env: HashMap::new(),
            forward_ports: vec![],
            app_port: None,
            ports_attributes: HashMap::new(),
            other_ports_attributes: None,
            run_args: vec![],
            shutdown_action: None,
            override_command: None,
            on_create_command: None,
            post_start_command: None,
            post_create_command: None,
            post_attach_command: None,
            initialize_command: None,
            update_content_command: None,
        };

        assert!(!traditional_config.uses_compose());
        assert!(!traditional_config.has_stop_compose_shutdown());
    }
}
