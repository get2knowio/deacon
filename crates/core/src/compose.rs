//! Docker Compose integration
//!
//! This module handles Docker Compose-based development containers,
//! including service management, project detection, and container lifecycle.

use crate::config::DevContainerConfig;
use crate::errors::{ConfigError, DockerError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{debug, instrument, warn};

/// Docker Compose project information
#[derive(Debug, Clone)]
pub struct ComposeProject {
    /// Project name (derived from directory name or compose project name)
    pub name: String,
    /// Base directory containing compose files
    pub base_path: PathBuf,
    /// Compose files in order
    pub compose_files: Vec<PathBuf>,
    /// Primary service name
    pub service: String,
    /// Additional services to run
    pub run_services: Vec<String>,
}

/// Docker Compose service information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeService {
    /// Service name
    pub name: String,
    /// Container ID (if running)
    pub container_id: Option<String>,
    /// Service image
    pub image: Option<String>,
    /// Service state
    pub state: String,
    /// Service status
    pub status: String,
}

/// Docker Compose command builder
#[derive(Debug)]
pub struct ComposeCommand {
    /// Docker binary path
    docker_path: String,
    /// Compose files
    compose_files: Vec<PathBuf>,
    /// Project name
    project_name: Option<String>,
    /// Base directory
    base_path: PathBuf,
}

impl ComposeCommand {
    /// Create a new compose command builder
    pub fn new(base_path: PathBuf, compose_files: Vec<PathBuf>) -> Self {
        Self {
            docker_path: "docker".to_string(),
            compose_files,
            project_name: None,
            base_path,
        }
    }

    /// Set custom docker binary path
    pub fn with_docker_path(mut self, docker_path: String) -> Self {
        self.docker_path = docker_path;
        self
    }

    /// Set project name
    pub fn with_project_name(mut self, project_name: String) -> Self {
        self.project_name = Some(project_name);
        self
    }

    /// Build docker compose command with given arguments
    pub fn build_command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(&self.docker_path);
        command.arg("compose");

        // Add compose files
        for file in &self.compose_files {
            command.arg("-f").arg(file);
        }

        // Add project name if specified
        if let Some(ref project_name) = self.project_name {
            command.arg("-p").arg(project_name);
        }

        // Add arguments
        command.args(args);

        // Set working directory
        command.current_dir(&self.base_path);

        command
    }

    /// Execute compose command and return output
    #[instrument(skip(self))]
    pub fn execute(&self, args: &[&str]) -> Result<String> {
        let mut command = self.build_command(args);

        debug!(
            "Executing docker compose command: {} compose {} {}",
            self.docker_path,
            self.compose_files
                .iter()
                .map(|f| format!("-f {}", f.display()))
                .collect::<Vec<_>>()
                .join(" "),
            args.join(" ")
        );

        let output = command.output().map_err(|e| {
            DockerError::CLIError(format!("Failed to execute docker compose command: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DockerError::CLIError(format!(
                "Docker compose command failed: {}",
                stderr
            ))
            .into());
        }

        let stdout = String::from_utf8(output.stdout).map_err(|e| {
            DockerError::CLIError(format!("Invalid UTF-8 in docker compose output: {}", e))
        })?;

        Ok(stdout)
    }

    /// Start services
    #[instrument(skip(self))]
    pub fn up(&self, services: &[String], detached: bool) -> Result<String> {
        let mut args = vec!["up"];
        if detached {
            args.push("-d");
        }
        args.extend(services.iter().map(|s| s.as_str()));
        self.execute(&args)
    }

    /// Stop and remove containers
    #[instrument(skip(self))]
    pub fn down(&self) -> Result<String> {
        self.execute(&["down"])
    }

    /// List services with their status
    #[instrument(skip(self))]
    pub fn ps(&self) -> Result<Vec<ComposeService>> {
        let output = self.execute(&["ps", "--format", "json"])?;
        self.parse_ps_output(&output)
    }

    /// Parse docker compose ps JSON output
    fn parse_ps_output(&self, json_output: &str) -> Result<Vec<ComposeService>> {
        if json_output.trim().is_empty() {
            return Ok(Vec::new());
        }

        let services: Vec<serde_json::Value> = json_output
            .trim()
            .lines()
            .map(serde_json::from_str)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| {
                DockerError::CLIError(format!("Failed to parse compose ps JSON: {}", e))
            })?;

        let mut result = Vec::new();
        for service in services {
            let compose_service = ComposeService {
                name: service
                    .get("Service")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                container_id: service
                    .get("ID")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                image: service
                    .get("Image")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                state: service
                    .get("State")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                status: service
                    .get("Status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
            };
            result.push(compose_service);
        }

        Ok(result)
    }
}

/// Docker Compose manager
pub struct ComposeManager {
    /// Docker binary path
    docker_path: String,
}

impl ComposeManager {
    /// Create a new compose manager
    pub fn new() -> Self {
        Self {
            docker_path: "docker".to_string(),
        }
    }

    /// Create a new compose manager with custom docker path
    pub fn with_docker_path(docker_path: String) -> Self {
        Self { docker_path }
    }

    /// Create a compose project from configuration
    #[instrument(skip(self))]
    pub fn create_project(
        &self,
        config: &DevContainerConfig,
        base_path: &Path,
    ) -> Result<ComposeProject> {
        // Check if docker_compose_file is specified
        if config.docker_compose_file.is_none() {
            return Err(ConfigError::Validation {
                message: "Configuration does not specify Docker Compose setup".to_string(),
            }
            .into());
        }

        let compose_files = config.get_compose_files();
        if compose_files.is_empty() {
            return Err(ConfigError::Validation {
                message: "No Docker Compose files specified".to_string(),
            }
            .into());
        }

        let service = config
            .service
            .as_ref()
            .ok_or_else(|| ConfigError::Validation {
                message: "No service specified for compose project".to_string(),
            })?;

        // Resolve compose file paths relative to base_path
        let mut resolved_files = Vec::new();
        for file in &compose_files {
            let file_path = if Path::new(file).is_absolute() {
                PathBuf::from(file)
            } else {
                base_path.join(file)
            };

            if !file_path.exists() {
                warn!("Compose file does not exist: {}", file_path.display());
            }

            resolved_files.push(file_path);
        }

        // Generate project name from directory name
        let project_name = base_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("deacon-compose")
            .to_string();

        Ok(ComposeProject {
            name: project_name,
            base_path: base_path.to_path_buf(),
            compose_files: resolved_files,
            service: service.clone(),
            run_services: config.run_services.clone(),
        })
    }

    /// Get compose command for a project
    pub fn get_command(&self, project: &ComposeProject) -> ComposeCommand {
        ComposeCommand::new(project.base_path.clone(), project.compose_files.clone())
            .with_docker_path(self.docker_path.clone())
            .with_project_name(project.name.clone())
    }

    /// Check if project containers are running
    #[instrument(skip(self))]
    pub fn is_project_running(&self, project: &ComposeProject) -> Result<bool> {
        let command = self.get_command(project);
        let services = command.ps()?;

        // Check if the primary service is running
        let primary_running = services
            .iter()
            .any(|s| s.name == project.service && s.state == "running");

        debug!(
            "Project {} primary service {} running: {}",
            project.name, project.service, primary_running
        );

        Ok(primary_running)
    }

    /// Start compose project
    #[instrument(skip(self))]
    pub fn start_project(&self, project: &ComposeProject) -> Result<()> {
        let command = self.get_command(project);
        let services = project.get_all_services();

        debug!(
            "Starting compose project {} with services: {:?}",
            project.name, services
        );

        command.up(&services, true)?;

        debug!("Compose project {} started successfully", project.name);
        Ok(())
    }

    /// Stop compose project
    #[instrument(skip(self))]
    pub fn stop_project(&self, project: &ComposeProject) -> Result<()> {
        let command = self.get_command(project);

        debug!("Stopping compose project {}", project.name);

        command.down()?;

        debug!("Compose project {} stopped successfully", project.name);
        Ok(())
    }

    /// Get primary service container ID
    #[instrument(skip(self))]
    pub fn get_primary_container_id(&self, project: &ComposeProject) -> Result<Option<String>> {
        let command = self.get_command(project);
        let services = command.ps()?;

        let primary_service = services.iter().find(|s| s.name == project.service);

        match primary_service {
            Some(service) => {
                debug!(
                    "Found primary service container: {} -> {:?}",
                    service.name, service.container_id
                );
                Ok(service.container_id.clone())
            }
            None => {
                debug!(
                    "Primary service {} not found in running services",
                    project.service
                );
                Ok(None)
            }
        }
    }
}

impl Default for ComposeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ComposeProject {
    /// Get all services to start (primary + run services)
    pub fn get_all_services(&self) -> Vec<String> {
        let mut services = vec![self.service.clone()];
        services.extend(self.run_services.clone());
        services
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_compose_command_build() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().to_path_buf();
        let compose_files = vec![
            base_path.join("docker-compose.yml"),
            base_path.join("docker-compose.override.yml"),
        ];

        let cmd = ComposeCommand::new(base_path.clone(), compose_files.clone())
            .with_project_name("test-project".to_string());

        let command = cmd.build_command(&["up", "-d"]);

        let args: Vec<String> = command
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();

        assert!(args.contains(&"compose".to_string()));
        assert!(args.contains(&"-f".to_string()));
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"test-project".to_string()));
        assert!(args.contains(&"up".to_string()));
        assert!(args.contains(&"-d".to_string()));
    }

    #[test]
    fn test_compose_project_get_all_services() {
        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: vec!["db".to_string(), "redis".to_string()],
        };

        let services = project.get_all_services();
        assert_eq!(services, vec!["app", "db", "redis"]);
    }

    #[test]
    fn test_config_compose_methods() {
        use serde_json::json;

        // Test single compose file
        let mut config = DevContainerConfig {
            docker_compose_file: Some(json!("docker-compose.yml")),
            service: Some("app".to_string()),
            ..Default::default()
        };

        assert!(config.uses_compose());
        assert_eq!(config.get_compose_files(), vec!["docker-compose.yml"]);
        assert_eq!(config.get_all_services(), vec!["app"]);

        // Test multiple compose files
        config.docker_compose_file =
            Some(json!(["docker-compose.yml", "docker-compose.override.yml"]));
        config.run_services = vec!["db".to_string(), "redis".to_string()];

        assert_eq!(
            config.get_compose_files(),
            vec!["docker-compose.yml", "docker-compose.override.yml"]
        );
        assert_eq!(config.get_all_services(), vec!["app", "db", "redis"]);

        // Test stopCompose shutdown action
        config.shutdown_action = Some("stopCompose".to_string());
        assert!(config.has_stop_compose_shutdown());
    }
}
