//! Docker Compose integration
//!
//! This module handles Docker Compose-based development containers,
//! including service management, project detection, and container lifecycle.

use crate::config::DevContainerConfig;
use crate::errors::{ConfigError, DockerError, Result};
use crate::security::SecurityOptions;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    /// Environment files to pass to docker compose
    pub env_files: Vec<PathBuf>,
    /// Additional mounts to apply to the primary service (Task T020)
    /// These are converted from CLI --mount flags for compose workflows
    pub additional_mounts: Vec<ComposeMount>,
    /// Profiles to activate for this project (Task T020)
    /// Automatically derived from runServices profiles
    pub profiles: Vec<String>,
    /// Additional environment variables to inject into primary service
    pub additional_env: HashMap<String, String>,
}

/// Mount specification for Docker Compose volumes (Task T020)
#[derive(Debug, Clone)]
pub struct ComposeMount {
    /// Mount type (bind or volume)
    pub mount_type: String,
    /// Source path or volume name
    pub source: String,
    /// Target path in container
    pub target: String,
    /// Whether the volume is external (for named volumes)
    pub external: bool,
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
    /// Environment files
    env_files: Vec<PathBuf>,
}

impl ComposeCommand {
    /// Create a new compose command builder
    pub fn new(base_path: PathBuf, compose_files: Vec<PathBuf>) -> Self {
        Self {
            docker_path: "docker".to_string(),
            compose_files,
            project_name: None,
            base_path,
            env_files: Vec::new(),
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

    /// Set environment files
    pub fn with_env_files(mut self, env_files: Vec<PathBuf>) -> Self {
        self.env_files = env_files;
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

        // Add environment files
        for file in &self.env_files {
            command.arg("--env-file").arg(file);
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

    /// Warn about security options that cannot be applied dynamically in Docker Compose
    pub fn warn_security_options_for_compose(config: &DevContainerConfig) {
        // TODO: In the future, this should accept features parameter to check feature-derived options too

        // For now, only check config options. Features would require access to resolved features.
        let security = SecurityOptions {
            privileged: config.privileged.unwrap_or(false),
            cap_add: SecurityOptions::normalize_capabilities(&config.cap_add),
            security_opt: SecurityOptions::normalize_security_opts(&config.security_opt),
            conflicts: Vec::new(),
        };

        if security.has_security_options() {
            warn!("Security options detected in configuration for Docker Compose:");

            if security.privileged {
                warn!("  - privileged mode must be defined in docker-compose.yml file");
            }

            if !security.cap_add.is_empty() {
                warn!(
                    "  - capabilities ({:?}) must be defined in docker-compose.yml file",
                    security.cap_add
                );
            }

            if !security.security_opt.is_empty() {
                warn!(
                    "  - security options ({:?}) must be defined in docker-compose.yml file",
                    security.security_opt
                );
            }

            warn!("Security options from devcontainer.json cannot be applied dynamically to Docker Compose services.");
            warn!("Please add these options to your docker-compose.yml service definition.");
        }
    }

    /// Stop and remove containers
    #[instrument(skip(self))]
    pub fn down(&self) -> Result<String> {
        self.execute(&["down"])
    }

    /// Stop and remove containers with additional flags
    #[instrument(skip(self))]
    pub fn down_with_flags(&self, flags: &[&str]) -> Result<String> {
        let mut args = vec!["down"];
        args.extend(flags);
        self.execute(&args)
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

        // Generate project name from directory name, ensuring it meets Docker Compose requirements
        let project_name = derive_project_name(base_path);

        Ok(ComposeProject {
            name: project_name,
            base_path: base_path.to_path_buf(),
            compose_files: resolved_files,
            service: service.clone(),
            run_services: config.run_services.clone(),
            env_files: Vec::new(),
            additional_mounts: Vec::new(), // Will be populated from CLI --mount flags
            profiles: Vec::new(),          // Will be populated from service profiles
            additional_env: std::collections::HashMap::new(),
        })
    }

    /// Get compose command for a project
    pub fn get_command(&self, project: &ComposeProject) -> ComposeCommand {
        ComposeCommand::new(project.base_path.clone(), project.compose_files.clone())
            .with_docker_path(self.docker_path.clone())
            .with_project_name(project.name.clone())
            .with_env_files(project.env_files.clone())
    }

    /// Check if project containers are running
    #[instrument(skip(self))]
    pub fn is_project_running(&self, project: &ComposeProject) -> Result<bool> {
        let command = self.get_command(project);
        let services = command.ps()?;

        // Get all services that should be running (primary + run_services)
        let all_services = project.get_all_services();

        // Check if all required services are running
        let running_services: Vec<String> = services
            .iter()
            .filter(|s| s.state == "running")
            .map(|s| s.name.clone())
            .collect();

        let all_running = all_services
            .iter()
            .all(|service| running_services.contains(service));

        debug!(
            "Project {} all services {:?} running: {} (running services: {:?})",
            project.name, all_services, all_running, running_services
        );

        Ok(all_running)
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

    /// Stop and remove compose project containers
    #[instrument(skip(self))]
    pub fn down_project(&self, project: &ComposeProject) -> Result<()> {
        let command = self.get_command(project);

        debug!("Stopping and removing compose project {}", project.name);

        // Use down without --volumes to preserve volumes
        command.down_with_flags(&[])?;

        debug!(
            "Compose project {} stopped and removed successfully",
            project.name
        );
        Ok(())
    }

    /// Stop and remove compose project containers including volumes
    #[instrument(skip(self))]
    pub fn down_project_with_volumes(&self, project: &ComposeProject) -> Result<()> {
        let command = self.get_command(project);

        debug!(
            "Stopping and removing compose project {} with volumes",
            project.name
        );

        // Use down with --volumes to remove named volumes as well
        command.down_with_flags(&["--volumes"])?;

        debug!(
            "Compose project {} stopped and removed with volumes successfully",
            project.name
        );
        Ok(())
    }

    /// Build a specific service in a Docker Compose project.
    ///
    /// This method executes `docker compose build <service>` to build the specified
    /// service defined in the project's compose configuration.
    ///
    /// # Arguments
    ///
    /// * `project` - The compose project containing the service
    /// * `service` - Name of the service to build
    ///
    /// # Returns
    ///
    /// Returns the command output on success.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The docker compose command fails to execute
    /// - The service does not exist in the project
    /// - The build process fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use deacon_core::compose::{ComposeManager, ComposeProject};
    /// # use std::path::PathBuf;
    /// # fn example() -> anyhow::Result<()> {
    /// let manager = ComposeManager::new();
    /// let project = ComposeProject {
    ///     name: "my-project".to_string(),
    ///     base_path: PathBuf::from("/path/to/project"),
    ///     compose_files: vec![PathBuf::from("docker-compose.yml")],
    ///     service: "web".to_string(),
    ///     run_services: Vec::new(),
    ///     env_files: Vec::new(),
    ///     additional_mounts: Vec::new(),
    ///     profiles: Vec::new(),
    /// };
    ///
    /// let output = manager.build_service(&project, "web")?;
    /// println!("Build output: {}", output);
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self))]
    pub fn build_service(&self, project: &ComposeProject, service: &str) -> Result<String> {
        let command = self.get_command(project);

        debug!(
            "Building compose project {} service {}",
            project.name, service
        );

        let output = command.execute(&["build", service])?;

        debug!(
            "Compose project {} service {} built successfully",
            project.name, service
        );
        Ok(output)
    }

    /// Validate that a service exists in a Docker Compose project configuration.
    ///
    /// This method queries the compose configuration to determine if a service
    /// with the given name is defined in the project.
    ///
    /// # Arguments
    ///
    /// * `project` - The compose project to check
    /// * `service` - Name of the service to validate
    ///
    /// # Returns
    ///
    /// Returns `Ok(true)` if the service exists, `Ok(false)` if it doesn't.
    ///
    /// # Errors
    ///
    /// Returns an error if the docker compose command fails to execute.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use deacon_core::compose::{ComposeManager, ComposeProject};
    /// # use std::path::PathBuf;
    /// # fn example() -> anyhow::Result<()> {
    /// let manager = ComposeManager::new();
    /// let project = ComposeProject {
    ///     name: "my-project".to_string(),
    ///     base_path: PathBuf::from("/path/to/project"),
    ///     compose_files: vec![PathBuf::from("docker-compose.yml")],
    ///     service: "web".to_string(),
    ///     run_services: Vec::new(),
    ///     env_files: Vec::new(),
    ///     additional_mounts: Vec::new(),
    ///     profiles: Vec::new(),
    /// };
    ///
    /// if manager.validate_service_exists(&project, "web")? {
    ///     println!("Service 'web' exists in the project");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self))]
    pub fn validate_service_exists(&self, project: &ComposeProject, service: &str) -> Result<bool> {
        let command = self.get_command(project);

        // Use docker compose config --services to list all available services
        let output = command.execute(&["config", "--services"])?;

        let services: Vec<String> = output
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        debug!(
            "Found services in compose project {}: {:?}",
            project.name, services
        );

        Ok(services.contains(&service.to_string()))
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

    /// Get container IDs for all services in the project
    #[instrument(skip(self))]
    pub fn get_all_container_ids(
        &self,
        project: &ComposeProject,
    ) -> Result<std::collections::HashMap<String, String>> {
        let command = self.get_command(project);
        let services = command.ps()?;

        let mut container_ids = std::collections::HashMap::new();

        for service in services.iter() {
            if let Some(ref container_id) = service.container_id {
                container_ids.insert(service.name.clone(), container_id.clone());
                debug!(
                    "Found service container: {} -> {}",
                    service.name, container_id
                );
            }
        }

        debug!("Found {} service containers total", container_ids.len());
        Ok(container_ids)
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

/// Parse .env file and extract COMPOSE_PROJECT_NAME if present.
///
/// Reads a .env file line by line, looking for COMPOSE_PROJECT_NAME=value.
/// Returns the value if found, otherwise None.
///
/// Per Task T020: Support .env project name propagation for compose workflows.
fn parse_env_file_for_project_name(env_file_path: &Path) -> Option<String> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    if !env_file_path.exists() {
        return None;
    }

    let file = File::open(env_file_path).ok()?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line.ok()?;
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Look for COMPOSE_PROJECT_NAME=value
        if let Some(value) = line.strip_prefix("COMPOSE_PROJECT_NAME=") {
            let value = value.trim();
            // Remove quotes if present
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(value);
            let value = value
                .strip_prefix('\'')
                .and_then(|v| v.strip_suffix('\''))
                .unwrap_or(value);

            if !value.is_empty() {
                debug!("Found COMPOSE_PROJECT_NAME in .env: {}", value);
                return Some(value.to_string());
            }
        }
    }

    None
}

fn derive_project_name(base_path: &Path) -> String {
    // Task T020: Check for .env file and extract COMPOSE_PROJECT_NAME
    let env_file_path = base_path.join(".env");
    if let Some(project_name) = parse_env_file_for_project_name(&env_file_path) {
        debug!("Using project name from .env file: {}", project_name);
        return project_name;
    }

    // Docker Compose project name rules:
    // - must start with a lowercase letter or number
    // - may contain only lowercase alphanumeric characters, hyphens, and underscores
    // - we also collapse runs of invalid characters into a single '-'
    const FALLBACK: &str = "deacon-compose";

    let original = base_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(FALLBACK);

    // Fallback when directory name is empty or only dots (e.g., "...")
    if original.is_empty() || original.chars().all(|c| c == '.') {
        return FALLBACK.to_string();
    }

    // Sanitize: lowercase, keep [a-z0-9_-], convert other chars to '-'
    let mut sanitized = String::with_capacity(original.len());
    let mut last_was_dash = false;
    for ch in original.chars() {
        let lc = ch.to_ascii_lowercase();
        if lc.is_ascii_alphanumeric() {
            sanitized.push(lc);
            last_was_dash = false;
        } else if lc == '-' || lc == '_' {
            // Preserve hyphen/underscore but avoid leading repetitions
            if !(sanitized.is_empty() && (lc == '-' || lc == '_')) {
                sanitized.push(lc);
            }
            last_was_dash = lc == '-';
        } else {
            // Replace invalid characters with a single '-'
            if !last_was_dash {
                sanitized.push('-');
                last_was_dash = true;
            }
        }
    }

    // Trim leading/trailing dashes/underscores
    let sanitized = sanitized
        .trim_matches(|c: char| c == '-' || c == '_')
        .to_string();

    // Ensure first character is a lowercase letter or number
    let final_name = match sanitized.chars().next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => sanitized,
        Some(_) => format!("d{}", sanitized),
        None => FALLBACK.to_string(),
    };

    final_name
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
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
    fn test_compose_command_with_env_file() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().to_path_buf();
        let compose_files = vec![base_path.join("docker-compose.yml")];
        let env_files = vec![base_path.join(".env"), base_path.join(".env.local")];

        let cmd = ComposeCommand::new(base_path.clone(), compose_files.clone())
            .with_project_name("test-project".to_string())
            .with_env_files(env_files.clone());

        let command = cmd.build_command(&["up", "-d"]);

        let args: Vec<String> = command
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();

        assert!(args.contains(&"compose".to_string()));
        assert!(args.contains(&"--env-file".to_string()));

        // Verify that both env files are included
        let env_file_count = args.iter().filter(|&arg| arg == "--env-file").count();
        assert_eq!(env_file_count, 2, "Should have two --env-file flags");
    }

    #[test]
    fn test_compose_project_get_all_services() {
        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: vec!["db".to_string(), "redis".to_string()],
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env: HashMap::new(),
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

    #[test]
    fn test_security_options_warning_for_compose() {
        // Test config with security options
        let config = DevContainerConfig {
            privileged: Some(true),
            cap_add: vec!["SYS_PTRACE".to_string(), "NET_ADMIN".to_string()],
            security_opt: vec!["seccomp=unconfined".to_string()],
            ..Default::default()
        };

        // This should log warnings - in a real test we'd capture logs
        ComposeCommand::warn_security_options_for_compose(&config);

        // Test config without security options
        let empty_config = DevContainerConfig::default();

        // This should not log any warnings
        ComposeCommand::warn_security_options_for_compose(&empty_config);
    }

    #[test]
    fn test_compose_project_all_services_coverage() {
        // Test that get_all_services includes primary service and run_services
        let project = ComposeProject {
            name: "multi-service".to_string(),
            base_path: PathBuf::from("/workspace"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "web".to_string(),
            run_services: vec![
                "database".to_string(),
                "cache".to_string(),
                "queue".to_string(),
            ],
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env: HashMap::new(),
        };

        let all_services = project.get_all_services();
        assert_eq!(all_services.len(), 4);
        assert_eq!(all_services[0], "web"); // Primary service first
        assert!(all_services.contains(&"database".to_string()));
        assert!(all_services.contains(&"cache".to_string()));
        assert!(all_services.contains(&"queue".to_string()));
    }

    #[test]
    fn test_compose_project_single_service_only() {
        // Test project with only primary service, no run_services
        let project = ComposeProject {
            name: "single-service".to_string(),
            base_path: PathBuf::from("/workspace"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: vec![],
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env: HashMap::new(),
        };

        let all_services = project.get_all_services();
        assert_eq!(all_services, vec!["app"]);
    }

    #[test]
    fn test_derive_project_name_from_hidden_directory() {
        let path = Path::new("/tmp/.tmpAbC123");
        // Leading dot should be removed and name sanitized to valid compose project name
        assert_eq!(derive_project_name(path), "tmpabc123");
    }

    #[test]
    fn test_derive_project_name_replaces_invalid_characters() {
        let path = Path::new("/tmp/My Project!");
        // Sanitize spaces and punctuation, lowercase and replace with hyphen
        assert_eq!(derive_project_name(path), "my-project");
    }

    #[test]
    fn test_derive_project_name_fallback_for_all_invalid() {
        let path = Path::new("/tmp/...");
        assert_eq!(derive_project_name(path), "deacon-compose");
    }
}
