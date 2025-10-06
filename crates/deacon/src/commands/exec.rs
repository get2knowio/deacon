//! Exec command implementation for container execution
//!
//! This module provides container resolution and execution functionality
//! for the exec command, targeting the correct workspace container.

use anyhow::Result;
use deacon_core::compose::ComposeManager;
use deacon_core::config::{ConfigLoader, DevContainerConfig};
use deacon_core::container::ContainerIdentity;
use deacon_core::docker::{CliDocker, Docker, ExecConfig};
use deacon_core::errors::{ConfigError, DeaconError};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, instrument};

/// Arguments for the exec command
#[derive(Debug, Clone)]
pub struct ExecArgs {
    /// User to run the command as
    pub user: Option<String>,
    /// Disable TTY allocation
    pub no_tty: bool,
    /// Environment variables to set (KEY=VALUE format)
    pub env: Vec<String>,
    /// Working directory for command execution
    pub workdir: Option<String>,
    /// Identify container by labels (KEY=VALUE format)
    pub id_label: Vec<String>,
    /// Command to execute
    pub command: Vec<String>,
    /// Workspace folder path
    pub workspace_folder: Option<std::path::PathBuf>,
    /// Configuration file path
    pub config_path: Option<std::path::PathBuf>,
}

/// Resolve the target container for the current workspace/config
#[instrument(skip(docker_client))]
pub async fn resolve_target_container<D>(
    docker_client: &D,
    workspace_folder: &Path,
    config: &DevContainerConfig,
) -> Result<String>
where
    D: Docker,
{
    debug!("Resolving target container for workspace");

    // Check if this is a Docker Compose configuration
    if config.uses_compose() {
        debug!("Configuration uses Docker Compose, resolving via compose manager");
        return resolve_compose_target_container(workspace_folder, config).await;
    }

    // For single container configurations, use existing logic
    debug!("Configuration uses single container, resolving via container identity");

    // Create container identity for this workspace/config
    let identity = ContainerIdentity::new(workspace_folder, config);
    debug!("Created container identity: {:?}", identity);

    // Find matching containers and only keep running ones
    let label_selector = identity.label_selector();
    let containers = docker_client.list_containers(Some(&label_selector)).await?;
    let matching_containers: Vec<String> = containers
        .into_iter()
        .filter(|c| c.state == "running")
        .map(|c| c.id)
        .collect();

    match matching_containers.len() {
        0 => {
            let workspace_path = workspace_folder.display();
            let config_name = config.name.as_deref().unwrap_or("unnamed");
            Err(anyhow::anyhow!(
                "No running container found for workspace '{}' with config '{}'. \
                 Run 'deacon up' first to create the container.",
                workspace_path,
                config_name
            ))
        }
        1 => {
            let container_id = matching_containers[0].clone();
            debug!("Found unique matching container: {}", container_id);
            Ok(container_id)
        }
        multiple => {
            let workspace_path = workspace_folder.display();
            let config_name = config.name.as_deref().unwrap_or("unnamed");
            Err(anyhow::anyhow!(
                "Found {} running containers for workspace '{}' with config '{}'. \
                 This should not happen. Container IDs: {:?}",
                multiple,
                workspace_path,
                config_name,
                matching_containers
            ))
        }
    }
}

/// Resolve target container by custom id-labels
#[instrument(skip(docker_client))]
pub async fn resolve_target_container_by_labels<D>(
    docker_client: &D,
    id_labels: &[String],
) -> Result<String>
where
    D: Docker,
{
    debug!("Resolving target container by id-labels: {:?}", id_labels);

    if id_labels.is_empty() {
        return Err(anyhow::anyhow!(
            "No id-labels provided for container resolution"
        ));
    }

    // Validate label format (KEY=VALUE)
    for label in id_labels {
        if !label.contains('=') {
            return Err(anyhow::anyhow!(
                "Invalid id-label format: '{}'. Expected KEY=VALUE",
                label
            ));
        }
    }

    // Build label selector string (comma-separated)
    let label_selector = id_labels.join(",");
    debug!("Label selector: {}", label_selector);

    // List containers with the specified labels
    let containers = docker_client.list_containers(Some(&label_selector)).await?;

    // Filter to only running containers
    let matching_containers: Vec<String> = containers
        .into_iter()
        .filter(|c| c.state == "running")
        .map(|c| c.id)
        .collect();

    let match_count = matching_containers.len();
    tracing::Span::current().record("match_count", match_count);

    match matching_containers.len() {
        0 => Err(anyhow::anyhow!(
            "No running container found matching labels: {}",
            id_labels.join(", ")
        )),
        1 => {
            let container_id = matching_containers[0].clone();
            debug!("Found unique matching container: {}", container_id);
            Ok(container_id)
        }
        multiple => Err(anyhow::anyhow!(
            "Found {} running containers matching labels: {}. \
             Please refine your label selector to uniquely identify a single container. \
             Matching container IDs: {:?}",
            multiple,
            id_labels.join(", "),
            matching_containers
        )),
    }
}

/// Resolve the target container for Docker Compose configurations
#[instrument]
async fn resolve_compose_target_container(
    workspace_folder: &Path,
    config: &DevContainerConfig,
) -> Result<String> {
    debug!("Resolving compose target container");

    let compose_manager = ComposeManager::new();
    let project = compose_manager.create_project(config, workspace_folder)?;

    debug!("Created compose project: {:?}", project.name);

    // Get the primary service container ID
    match compose_manager.get_primary_container_id(&project)? {
        Some(container_id) => {
            debug!("Found primary service container: {}", container_id);
            Ok(container_id)
        }
        None => {
            let workspace_path = workspace_folder.display();
            let config_name = config.name.as_deref().unwrap_or("unnamed");
            let service_name = &project.service;
            Err(anyhow::anyhow!(
                "No running container found for service '{}' in compose project for workspace '{}' with config '{}'. \
                 Run 'deacon up' first to start the compose project.",
                service_name,
                workspace_path,
                config_name
            ))
        }
    }
}

/// Determine the working directory inside the container
#[instrument(skip(config))]
pub fn determine_container_working_dir(
    config: &DevContainerConfig,
    workspace_folder: &Path,
) -> String {
    // Use containerWorkspaceFolder if specified in config
    if let Some(ref container_workspace_folder) = config.workspace_folder {
        debug!(
            "Using containerWorkspaceFolder from config: {}",
            container_workspace_folder
        );
        container_workspace_folder.clone()
    } else {
        // Default to /workspaces/{workspace_name}
        let workspace_name = workspace_folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace");
        let default_path = format!("/workspaces/{}", workspace_name);
        debug!(
            "Using default container working directory: {}",
            default_path
        );
        default_path
    }
}

/// Execute the exec command
#[instrument]
pub async fn execute_exec(_args: ExecArgs) -> Result<()> {
    execute_exec_with_docker(_args, &CliDocker::new()).await
}

/// Execute the exec command with a custom Docker implementation
#[instrument(skip(docker_client), fields(workdir, user, labels_used, match_count))]
pub async fn execute_exec_with_docker<D>(args: ExecArgs, docker_client: &D) -> Result<()>
where
    D: Docker,
{
    if args.command.is_empty() {
        return Err(anyhow::anyhow!("No command specified for exec"));
    }

    {
        tracing::info!("Executing command in container: {:?}", args.command);

        // Parse environment variables early to catch format errors
        let mut env_map = HashMap::new();
        for env_var in &args.env {
            if let Some((key, value)) = env_var.split_once('=') {
                env_map.insert(key.to_string(), value.to_string());
            } else {
                return Err(anyhow::anyhow!(
                    "Invalid environment variable format: '{}'. Expected KEY=VALUE",
                    env_var
                ));
            }
        }

        // Check Docker availability
        docker_client.ping().await?;

        // Resolve target container using id-labels if provided, otherwise use workspace/config
        let container_id = if !args.id_label.is_empty() {
            // Add labels to tracing span
            let labels_str = args.id_label.join(",");
            tracing::Span::current().record("labels_used", &labels_str);
            debug!("Using id-label for container resolution: {}", labels_str);

            resolve_target_container_by_labels(docker_client, &args.id_label).await?
        } else {
            // Load configuration for workspace-based resolution
            let workspace_folder = args.workspace_folder.as_deref().unwrap_or(Path::new("."));

            let config = if let Some(config_path) = args.config_path.as_ref() {
                ConfigLoader::load_from_path(config_path)?
            } else {
                let config_location = ConfigLoader::discover_config(workspace_folder)?;
                if !config_location.exists() {
                    return Err(DeaconError::Config(ConfigError::NotFound {
                        path: config_location.path().to_string_lossy().to_string(),
                    })
                    .into());
                }
                ConfigLoader::load_from_path(config_location.path())?
            };

            debug!("Loaded configuration: {:?}", config.name);
            resolve_target_container(docker_client, workspace_folder, &config).await?
        };

        // Determine TTY allocation
        let should_use_tty = !args.no_tty && CliDocker::is_tty();

        // Determine working directory - prioritize CLI argument over config
        let working_dir = if let Some(ref cli_workdir) = args.workdir {
            debug!("Using working directory from CLI: {}", cli_workdir);
            cli_workdir.clone()
        } else if !args.id_label.is_empty() {
            // For id-label based exec, default to current directory in container
            debug!("Using default working directory for id-label based exec");
            String::from("/")
        } else {
            // For workspace-based exec, load config to get workspace folder
            let workspace_folder = args.workspace_folder.as_deref().unwrap_or(Path::new("."));
            let config = if let Some(config_path) = args.config_path.as_ref() {
                ConfigLoader::load_from_path(config_path)?
            } else {
                let config_location = ConfigLoader::discover_config(workspace_folder)?;
                ConfigLoader::load_from_path(config_location.path())?
            };
            determine_container_working_dir(&config, workspace_folder)
        };

        // Add workdir to the current tracing span
        tracing::Span::current().record("workdir", &working_dir);

        // Add user to the current tracing span if specified
        if let Some(ref user) = args.user {
            tracing::Span::current().record("user", user.as_str());
        }

        // Create exec config
        // Always attach stdin (interactive) so piped/stdin data flows into the container,
        // independent of TTY allocation. TTY only controls pseudo‑terminal behavior.
        let exec_config = ExecConfig {
            user: args.user.clone(),
            working_dir: Some(working_dir.clone()),
            env: env_map,
            tty: should_use_tty,
            interactive: true,
            detach: false,
        };

        match docker_client
            .exec(&container_id, &args.command, exec_config)
            .await
        {
            Ok(result) => {
                tracing::info!("Command completed with exit code: {}", result.exit_code);
                std::process::exit(result.exit_code);
            }
            Err(e) => {
                tracing::error!("Failed to execute command: {}", e);
                Err(e.into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::config::DevContainerConfig;
    use tempfile::TempDir;

    #[test]
    fn test_determine_container_working_dir_with_config() {
        let config = DevContainerConfig {
            workspace_folder: Some("/custom/workspace".to_string()),
            ..Default::default()
        };

        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let working_dir = determine_container_working_dir(&config, workspace_path);
        assert_eq!(working_dir, "/custom/workspace");
    }

    #[test]
    fn test_determine_container_working_dir_default() {
        let config = DevContainerConfig {
            workspace_folder: None,
            ..Default::default()
        };

        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();
        let workspace_name = workspace_path.file_name().unwrap().to_str().unwrap();

        let working_dir = determine_container_working_dir(&config, workspace_path);
        assert_eq!(working_dir, format!("/workspaces/{}", workspace_name));
    }

    #[test]
    fn test_determine_container_working_dir_fallback() {
        let config = DevContainerConfig {
            workspace_folder: None,
            ..Default::default()
        };

        // Use a path that might not have a proper file name
        let working_dir = determine_container_working_dir(&config, Path::new("/"));
        assert_eq!(working_dir, "/workspaces/workspace");
    }

    #[test]
    fn test_compose_config_detection() {
        use serde_json::json;

        // Test compose configuration detection
        let compose_config = DevContainerConfig {
            docker_compose_file: Some(json!("docker-compose.yml")),
            service: Some("app".to_string()),
            ..Default::default()
        };
        assert!(compose_config.uses_compose());

        // Test single container configuration
        let container_config = DevContainerConfig {
            image: Some("alpine:latest".to_string()),
            ..Default::default()
        };
        assert!(!container_config.uses_compose());

        // Test invalid compose configuration (missing service)
        let invalid_config = DevContainerConfig {
            docker_compose_file: Some(json!("docker-compose.yml")),
            service: None,
            ..Default::default()
        };
        assert!(!invalid_config.uses_compose());
    }

    #[test]
    fn test_compose_config_with_run_services() {
        use serde_json::json;

        // Test compose configuration with run services
        let compose_config = DevContainerConfig {
            docker_compose_file: Some(json!("docker-compose.yml")),
            service: Some("web".to_string()),
            run_services: vec!["db".to_string(), "redis".to_string()],
            ..Default::default()
        };

        assert!(compose_config.uses_compose());
        let all_services = compose_config.get_all_services();
        assert_eq!(all_services, vec!["web", "db", "redis"]);
    }

    #[test]
    fn test_exec_args_with_workdir() {
        // Test that ExecArgs correctly stores workdir field
        let args = ExecArgs {
            user: Some("testuser".to_string()),
            no_tty: false,
            env: vec!["KEY=value".to_string()],
            workdir: Some("/custom/path".to_string()),
            id_label: vec![],
            command: vec!["ls".to_string(), "-la".to_string()],
            workspace_folder: None,
            config_path: None,
        };

        assert_eq!(args.workdir, Some("/custom/path".to_string()));
        assert_eq!(args.command, vec!["ls", "-la"]);
    }

    #[test]
    fn test_exec_args_without_workdir() {
        // Test that ExecArgs works without workdir (should fall back to config)
        let args = ExecArgs {
            user: None,
            no_tty: true,
            env: vec![],
            workdir: None,
            id_label: vec![],
            command: vec!["pwd".to_string()],
            workspace_folder: None,
            config_path: None,
        };

        assert_eq!(args.workdir, None);
        assert_eq!(args.command, vec!["pwd"]);
    }
}
