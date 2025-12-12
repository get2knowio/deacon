//! Docker Compose integration
//!
//! This module handles Docker Compose-based development containers,
//! including service management, project detection, and container lifecycle.

use crate::config::DevContainerConfig;
use crate::errors::{ConfigError, DockerError, Result};
use crate::security::SecurityOptions;
use indexmap::IndexMap;
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
    /// Environment files to pass to docker compose
    pub env_files: Vec<PathBuf>,
    /// Additional mounts to apply to the primary service
    /// Includes workspace mounts (with optional consistency) and CLI --mount flags
    pub additional_mounts: Vec<ComposeMount>,
    /// Profiles to activate for this project
    /// Automatically derived from runServices profiles
    pub profiles: Vec<String>,
    /// Additional environment variables to inject into primary service
    pub additional_env: IndexMap<String, String>,
    /// External volume names that must remain referenced (not replaced by injection)
    /// Per spec: these volumes should surface compose errors if missing, not bind fallback
    pub external_volumes: Vec<String>,
}

/// Mount specification for Docker Compose volumes
///
/// Used to inject additional volume mounts into Compose services during
/// container startup. Supports workspace mounts with consistency options.
#[derive(Debug, Clone)]
pub struct ComposeMount {
    /// Mount type (bind or volume)
    pub mount_type: String,
    /// Source path or volume name
    pub source: String,
    /// Target path in container
    pub target: String,
    /// Whether the mount is read-only (adds `:ro` suffix to the volume)
    pub read_only: bool,
    /// Mount consistency option (cached, consistent, delegated)
    /// Only applicable to bind mounts on macOS for performance tuning
    pub consistency: Option<String>,
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
    /// Profiles to activate
    profiles: Vec<String>,
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
            profiles: Vec::new(),
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

    /// Set profiles to activate
    ///
    /// Per FR-005: The up workflow must respect compose profiles
    pub fn with_profiles(mut self, profiles: Vec<String>) -> Self {
        self.profiles = profiles;
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

        // Add profiles if specified (per FR-005)
        for profile in &self.profiles {
            command.arg("--profile").arg(profile);
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
        self.execute_with_stdin(args, None)
    }

    /// Execute compose command with optional stdin input (e.g., for inline override YAML)
    ///
    /// When `stdin_input` is Some, the command will:
    /// 1. Add `-f -` to read an additional compose file from stdin
    /// 2. Pipe the stdin_input content to the command
    ///
    /// This allows injecting mounts/env without creating temporary override files.
    #[instrument(skip(self, stdin_input))]
    pub fn execute_with_stdin(&self, args: &[&str], stdin_input: Option<&str>) -> Result<String> {
        use std::io::Write;
        use std::process::Stdio;

        let mut command = self.build_command(args);

        // Add stdin file source if we have input
        if stdin_input.is_some() {
            // Insert -f - before the subcommand args to read from stdin
            // Note: We need to rebuild command to insert at the right position
            let mut new_command = Command::new(&self.docker_path);
            new_command.arg("compose");

            // Add compose files
            for file in &self.compose_files {
                new_command.arg("-f").arg(file);
            }

            // Add stdin as additional compose file
            new_command.arg("-f").arg("-");

            // Add environment files
            for file in &self.env_files {
                new_command.arg("--env-file").arg(file);
            }

            // Add project name if specified
            if let Some(ref project_name) = self.project_name {
                new_command.arg("-p").arg(project_name);
            }

            // Add profiles if specified
            for profile in &self.profiles {
                new_command.arg("--profile").arg(profile);
            }

            // Add arguments
            new_command.args(args);

            // Set working directory
            new_command.current_dir(&self.base_path);

            command = new_command;
        }

        debug!(
            "Executing docker compose command: {} compose {} {} {}",
            self.docker_path,
            self.compose_files
                .iter()
                .map(|f| format!("-f {}", f.display()))
                .collect::<Vec<_>>()
                .join(" "),
            if stdin_input.is_some() {
                "-f - (stdin)"
            } else {
                ""
            },
            args.join(" ")
        );

        // Set up stdin/stdout/stderr
        if stdin_input.is_some() {
            command.stdin(Stdio::piped());
        }
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|e| {
            DockerError::CLIError(format!("Failed to execute docker compose command: {}", e))
        })?;

        // Write stdin input if provided
        if let Some(input) = stdin_input {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(input.as_bytes()).map_err(|e| {
                    DockerError::CLIError(format!("Failed to write stdin to docker compose: {}", e))
                })?;
                // Drop stdin to signal EOF
            }
        }

        let output = child.wait_with_output().map_err(|e| {
            DockerError::CLIError(format!("Failed to wait for docker compose command: {}", e))
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
    pub fn up(
        &self,
        services: &[String],
        detached: bool,
        gpu_mode: crate::gpu::GpuMode,
    ) -> Result<String> {
        self.up_with_injection(services, detached, gpu_mode, None)
    }

    /// Start services with optional inline injection override.
    ///
    /// Per FR-001/FR-002: This method allows injecting mounts and environment
    /// variables into the primary service without creating temporary override files.
    ///
    /// The injection_override YAML is piped to docker compose via stdin using `-f -`.
    #[instrument(skip(self, injection_override))]
    pub fn up_with_injection(
        &self,
        services: &[String],
        detached: bool,
        gpu_mode: crate::gpu::GpuMode,
        injection_override: Option<&str>,
    ) -> Result<String> {
        let mut args = vec!["up"];
        if detached {
            args.push("-d");
        }

        // Add GPU flags based on GPU mode
        // Note: GpuMode::Detect is resolved to All or None by the caller (e.g., in up.rs)
        match gpu_mode {
            crate::gpu::GpuMode::All => {
                args.push("--gpus");
                args.push("all");
                debug!("Added --gpus all flag for compose up (GpuMode::All)");
            }
            crate::gpu::GpuMode::None => {
                // Silent no-op per FR-006: no GPU requests, no GPU-related logs
            }
            crate::gpu::GpuMode::Detect => {
                // This should never happen - Detect mode should be resolved upstream
                warn!("GpuMode::Detect passed to compose.rs - this indicates a bug. Skipping GPU flags.");
            }
        }

        args.extend(services.iter().map(|s| s.as_str()));
        self.execute_with_stdin(&args, injection_override)
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

    /// Extract external volumes from compose configuration.
    ///
    /// Uses `docker compose config --format json` to get the merged configuration
    /// and extracts volume names that are marked as external.
    ///
    /// Per the spec (data-model.md): External volumes are those with `external: true`
    /// or `external: { name: "..." }` in the compose configuration. These must remain
    /// intact and not be replaced by injection logic.
    ///
    /// # Returns
    ///
    /// A list of external volume names. Returns an empty list if no volumes are defined
    /// or if none are marked as external.
    ///
    /// # Errors
    ///
    /// Returns an error if the docker compose config command fails to execute or
    /// produces invalid JSON output.
    #[instrument(skip(self))]
    pub fn extract_external_volumes(&self) -> Result<Vec<String>> {
        let output = self.execute(&["config", "--format", "json"])?;
        parse_external_volumes_from_config(&output)
    }
}

/// Parse external volumes from docker compose config JSON output.
///
/// This function extracts volume names that are marked as external from the
/// compose configuration. It handles both formats:
/// - `external: true` - Simple boolean form
/// - `external: { name: "actual-volume-name" }` - Object form with explicit name
///
/// # Arguments
///
/// * `json_output` - The JSON output from `docker compose config --format json`
///
/// # Returns
///
/// A list of external volume names. For the object form with `name`, the actual
/// external volume name is used. For the simple boolean form, the key name from
/// the volumes section is used.
fn parse_external_volumes_from_config(json_output: &str) -> Result<Vec<String>> {
    if json_output.trim().is_empty() {
        return Ok(Vec::new());
    }

    let config: serde_json::Value = serde_json::from_str(json_output).map_err(|e| {
        DockerError::CLIError(format!("Failed to parse compose config JSON: {}", e))
    })?;

    let mut external_volumes = Vec::new();

    if let Some(volumes) = config.get("volumes").and_then(|v| v.as_object()) {
        for (volume_name, volume_config) in volumes {
            // Check if the volume is marked as external
            if let Some(external) = volume_config.get("external") {
                if external.as_bool() == Some(true) {
                    // Simple form: external: true
                    external_volumes.push(volume_name.clone());
                    debug!("Found external volume (simple form): {}", volume_name);
                } else if external.is_object() {
                    // Object form: external: { name: "..." }
                    // In this case, the external volume name might differ from the key
                    if let Some(external_name) = external.get("name").and_then(|n| n.as_str()) {
                        external_volumes.push(external_name.to_string());
                        debug!(
                            "Found external volume (object form): {} -> {}",
                            volume_name, external_name
                        );
                    } else {
                        // external is an object but no name specified, use the key name
                        external_volumes.push(volume_name.clone());
                        debug!(
                            "Found external volume (object form, no name): {}",
                            volume_name
                        );
                    }
                }
            }
        }
    }

    debug!(
        "Extracted {} external volumes from compose config",
        external_volumes.len()
    );
    Ok(external_volumes)
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
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(), // Will be populated via populate_external_volumes()
        })
    }

    /// Populate external volumes for a compose project.
    ///
    /// This method uses `docker compose config --format json` to extract external
    /// volume declarations from the compose configuration files. The extracted
    /// volume names are stored in the project's `external_volumes` field.
    ///
    /// Per the spec (data-model.md): External volumes must remain intact and
    /// not be replaced or mutated by injection logic. This method enables
    /// tracking which volumes are external for validation and preservation.
    ///
    /// # Arguments
    ///
    /// * `project` - The compose project to populate external volumes for
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success. The project's `external_volumes` field
    /// is modified in-place.
    ///
    /// # Errors
    ///
    /// Returns an error if the docker compose config command fails to execute.
    /// This may happen if:
    /// - Docker is not available
    /// - The compose files are invalid
    /// - Variable substitution fails
    ///
    /// # Note
    ///
    /// This operation requires Docker to be available and may fail in
    /// environments without Docker. Callers should handle errors gracefully
    /// and potentially continue without external volume information if
    /// Docker is unavailable.
    #[instrument(skip(self))]
    pub fn populate_external_volumes(&self, project: &mut ComposeProject) -> Result<()> {
        let command = self.get_command(project);
        let external_volumes = command.extract_external_volumes()?;
        project.external_volumes = external_volumes;
        debug!(
            "Populated {} external volumes for project {}",
            project.external_volumes.len(),
            project.name
        );
        Ok(())
    }

    /// Get compose command for a project
    ///
    /// Per T005: Threads profiles, env-files, and project naming through all compose invocations
    pub fn get_command(&self, project: &ComposeProject) -> ComposeCommand {
        ComposeCommand::new(project.base_path.clone(), project.compose_files.clone())
            .with_docker_path(self.docker_path.clone())
            .with_project_name(project.name.clone())
            .with_env_files(project.env_files.clone())
            .with_profiles(project.profiles.clone())
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
    ///
    /// Per FR-001/FR-002: Injects mounts and env into the primary service
    /// using inline YAML via stdin (no temp files).
    #[instrument(skip(self))]
    pub fn start_project(
        &self,
        project: &ComposeProject,
        gpu_mode: crate::gpu::GpuMode,
    ) -> Result<()> {
        let command = self.get_command(project);
        let services = project.get_all_services();

        debug!(
            "Starting compose project {} with services: {:?}, gpu_mode: {:?}",
            project.name, services, gpu_mode
        );

        // Generate injection override if we have mounts or env to inject
        let injection_override = project.generate_injection_override();

        command.up_with_injection(&services, true, gpu_mode, injection_override.as_deref())?;

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
    /// # use indexmap::IndexMap;
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
    ///     additional_env: IndexMap::new(),
    ///     external_volumes: Vec::new(),
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
    /// # use indexmap::IndexMap;
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
    ///     additional_env: IndexMap::new(),
    ///     external_volumes: Vec::new(),
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

    /// Generate inline compose override YAML for mount/env injection.
    ///
    /// Per FR-001/FR-002: Apply CLI mounts and remote env to the primary service
    /// without creating temporary override files that users need to manage.
    ///
    /// **External Volume Preservation (FR-004, T010)**:
    /// This override only adds volumes to the service definition - it does NOT
    /// define or modify top-level volume declarations. External volumes declared
    /// in the original compose files remain intact, and missing external volumes
    /// will surface as compose errors (not silently replaced with bind mounts).
    ///
    /// Returns None if no mounts or env need to be injected.
    #[must_use = "injection override should be passed to compose up"]
    pub fn generate_injection_override(&self) -> Option<String> {
        if self.additional_mounts.is_empty() && self.additional_env.is_empty() {
            return None;
        }

        let mut yaml = String::from("services:\n");
        yaml.push_str(&format!("  {}:\n", self.service));

        if !self.additional_env.is_empty() {
            yaml.push_str("    environment:\n");
            // IndexMap preserves insertion order - no sorting needed
            for (key, value) in &self.additional_env {
                let escaped = escape_yaml_value(value);
                yaml.push_str(&format!("      {}: {}\n", key, escaped));
            }
        }

        if !self.additional_mounts.is_empty() {
            yaml.push_str("    volumes:\n");
            for mount in &self.additional_mounts {
                let mut mount_str = format!("{}:{}", mount.source, mount.target);
                // Build options suffix: ro and/or consistency
                // Docker Compose short-form: source:target:options
                // Options can be comma-separated: :ro,cached or just :cached
                let mut options = Vec::new();
                if mount.read_only {
                    options.push("ro");
                }
                if let Some(ref consistency) = mount.consistency {
                    options.push(consistency);
                }
                if !options.is_empty() {
                    mount_str.push(':');
                    mount_str.push_str(&options.join(","));
                }
                yaml.push_str(&format!("      - {}\n", mount_str));
            }
        }

        debug!(
            "Generated compose injection override for service '{}': {} env vars, {} mounts",
            self.service,
            self.additional_env.len(),
            self.additional_mounts.len()
        );

        Some(yaml)
    }

    /// Merge CLI remote environment with existing environment entries.
    ///
    /// Per the spec (FR-002, research.md Decision 3):
    /// - CLI/remote env entries override duplicate keys from env-files/service defaults
    /// - Non-conflicting keys remain untouched
    /// - Returns merged IndexMap with CLI values taking precedence
    ///
    /// # Arguments
    /// * `service_env` - Environment variables from compose service definition
    /// * `env_file_env` - Environment variables from env-files
    /// * `cli_env` - CLI-provided remote environment entries (highest precedence)
    ///
    /// # Returns
    /// Merged IndexMap with CLI precedence: CLI > env-files > service defaults
    pub fn merge_env_with_cli_precedence(
        service_env: &IndexMap<String, String>,
        env_file_env: &IndexMap<String, String>,
        cli_env: &IndexMap<String, String>,
    ) -> IndexMap<String, String> {
        let mut merged = IndexMap::new();

        // Layer 1: Service defaults (lowest precedence)
        for (key, value) in service_env {
            merged.insert(key.clone(), value.clone());
        }

        // Layer 2: Env-file values (override service defaults)
        for (key, value) in env_file_env {
            merged.insert(key.clone(), value.clone());
        }

        // Layer 3: CLI/remote env (highest precedence)
        for (key, value) in cli_env {
            merged.insert(key.clone(), value.clone());
        }

        debug!(
            "Merged env: {} service defaults + {} env-file + {} CLI = {} total",
            service_env.len(),
            env_file_env.len(),
            cli_env.len(),
            merged.len()
        );

        merged
    }

    /// Apply additional mounts and environment to this project.
    ///
    /// This method prepares the project for compose up by:
    /// 1. Setting additional mounts for the primary service
    /// 2. Merging CLI environment with precedence over defaults
    ///
    /// Per the spec, injection targets only the primary service.
    pub fn with_injection(
        mut self,
        additional_mounts: Vec<ComposeMount>,
        cli_env: IndexMap<String, String>,
    ) -> Self {
        self.additional_mounts = additional_mounts;
        self.additional_env = cli_env;
        self
    }
}

/// Escape a value for YAML output.
///
/// YAML requires special handling for values containing:
/// - Newlines (must be quoted and escaped)
/// - Colons (especially at start)
/// - Quotes (must be escaped)
/// - Leading/trailing whitespace (must be quoted)
/// - Hash characters (could be interpreted as comments)
fn escape_yaml_value(value: &str) -> String {
    // Check if value needs quoting
    let needs_quoting = value.contains('\n')
        || value.contains(':')
        || value.contains('#')
        || value.contains('"')
        || value.contains('\'')
        || value.starts_with(' ')
        || value.ends_with(' ')
        || value.starts_with('!')
        || value.starts_with('&')
        || value.starts_with('*')
        || value.is_empty();

    if needs_quoting {
        // Use double quotes and escape special characters
        let escaped = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t");
        format!("\"{}\"", escaped)
    } else {
        // Simple values can be unquoted or double-quoted for consistency
        format!("\"{}\"", value)
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
    fn test_compose_command_with_profiles() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().to_path_buf();
        let compose_files = vec![base_path.join("docker-compose.yml")];
        let profiles = vec!["dev".to_string(), "debug".to_string()];

        let cmd = ComposeCommand::new(base_path.clone(), compose_files.clone())
            .with_project_name("test-project".to_string())
            .with_profiles(profiles);

        let command = cmd.build_command(&["up", "-d"]);

        let args: Vec<String> = command
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();

        assert!(args.contains(&"compose".to_string()));
        assert!(args.contains(&"--profile".to_string()));
        assert!(args.contains(&"dev".to_string()));
        assert!(args.contains(&"debug".to_string()));

        // Verify that both profiles are included
        let profile_count = args.iter().filter(|&arg| arg == "--profile").count();
        assert_eq!(profile_count, 2, "Should have two --profile flags");
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
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(),
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
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(),
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
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(),
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

    #[test]
    fn test_merge_env_with_cli_precedence() {
        let mut service_env: IndexMap<String, String> = IndexMap::new();
        service_env.insert("DB_HOST".to_string(), "localhost".to_string());
        service_env.insert("DB_PORT".to_string(), "5432".to_string());
        service_env.insert("SERVICE_ONLY".to_string(), "from_service".to_string());

        let mut env_file_env: IndexMap<String, String> = IndexMap::new();
        env_file_env.insert("DB_HOST".to_string(), "db.example.com".to_string());
        env_file_env.insert("ENV_FILE_ONLY".to_string(), "from_env_file".to_string());

        let mut cli_env: IndexMap<String, String> = IndexMap::new();
        cli_env.insert(
            "DB_HOST".to_string(),
            "cli-override.example.com".to_string(),
        );
        cli_env.insert("CLI_ONLY".to_string(), "from_cli".to_string());

        let merged =
            ComposeProject::merge_env_with_cli_precedence(&service_env, &env_file_env, &cli_env);

        // CLI takes precedence over both env-file and service defaults
        assert_eq!(
            merged.get("DB_HOST"),
            Some(&"cli-override.example.com".to_string())
        );

        // Service default preserved when not overridden
        assert_eq!(merged.get("DB_PORT"), Some(&"5432".to_string()));
        assert_eq!(
            merged.get("SERVICE_ONLY"),
            Some(&"from_service".to_string())
        );

        // Env-file value preserved when not overridden by CLI
        assert_eq!(
            merged.get("ENV_FILE_ONLY"),
            Some(&"from_env_file".to_string())
        );

        // CLI-only value present
        assert_eq!(merged.get("CLI_ONLY"), Some(&"from_cli".to_string()));

        // Total should be 5 unique keys
        assert_eq!(merged.len(), 5);
    }

    #[test]
    fn test_merge_env_empty_inputs() {
        let service_env: IndexMap<String, String> = IndexMap::new();
        let env_file_env: IndexMap<String, String> = IndexMap::new();
        let cli_env: IndexMap<String, String> = IndexMap::new();

        let merged =
            ComposeProject::merge_env_with_cli_precedence(&service_env, &env_file_env, &cli_env);

        assert!(merged.is_empty());
    }

    #[test]
    fn test_merge_env_cli_only() {
        let service_env: IndexMap<String, String> = IndexMap::new();
        let env_file_env: IndexMap<String, String> = IndexMap::new();
        let mut cli_env: IndexMap<String, String> = IndexMap::new();
        cli_env.insert("MY_VAR".to_string(), "my_value".to_string());

        let merged =
            ComposeProject::merge_env_with_cli_precedence(&service_env, &env_file_env, &cli_env);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged.get("MY_VAR"), Some(&"my_value".to_string()));
    }

    #[test]
    fn test_generate_injection_override_empty() {
        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(),
        };

        // No mounts or env, should return None
        assert!(project.generate_injection_override().is_none());
    }

    #[test]
    fn test_generate_injection_override_with_env() {
        let mut additional_env: IndexMap<String, String> = IndexMap::new();
        additional_env.insert("FOO".to_string(), "bar".to_string());
        additional_env.insert("BAZ".to_string(), "qux".to_string());

        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "myservice".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env,
            external_volumes: Vec::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();
        assert!(override_yaml.contains("services:"));
        assert!(override_yaml.contains("myservice:"));
        assert!(override_yaml.contains("environment:"));
        assert!(override_yaml.contains("FOO:"));
        assert!(override_yaml.contains("BAZ:"));
    }

    #[test]
    fn test_generate_injection_override_with_mounts() {
        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "myservice".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![
                ComposeMount {
                    mount_type: "bind".to_string(),
                    source: "/host/path".to_string(),
                    target: "/container/path".to_string(),
                    read_only: false,
                    consistency: None,
                },
                ComposeMount {
                    mount_type: "bind".to_string(),
                    source: "/another/host".to_string(),
                    target: "/another/container".to_string(),
                    read_only: true,
                    consistency: None,
                },
            ],
            profiles: Vec::new(),
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();
        assert!(override_yaml.contains("services:"));
        assert!(override_yaml.contains("myservice:"));
        assert!(override_yaml.contains("volumes:"));
        assert!(override_yaml.contains("/host/path:/container/path"));
        assert!(override_yaml.contains("/another/host:/another/container:ro"));
    }

    #[test]
    fn test_generate_injection_override_with_both() {
        let mut additional_env: IndexMap<String, String> = IndexMap::new();
        additional_env.insert("MY_VAR".to_string(), "my_value".to_string());

        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![ComposeMount {
                mount_type: "bind".to_string(),
                source: "/src".to_string(),
                target: "/dst".to_string(),
                read_only: false,
                consistency: None,
            }],
            profiles: Vec::new(),
            additional_env,
            external_volumes: Vec::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();

        // Should have both environment and volumes sections
        assert!(override_yaml.contains("environment:"));
        assert!(override_yaml.contains("volumes:"));
        assert!(override_yaml.contains("MY_VAR:"));
        assert!(override_yaml.contains("/src:/dst"));
    }

    #[test]
    fn test_generate_injection_override_with_special_chars() {
        let mut additional_env: IndexMap<String, String> = IndexMap::new();
        additional_env.insert("MULTILINE".to_string(), "line1\nline2".to_string());
        additional_env.insert("QUOTED".to_string(), "value with \"quotes\"".to_string());
        additional_env.insert("COLON".to_string(), "key:value".to_string());
        additional_env.insert("HASH".to_string(), "before#after".to_string());

        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env,
            external_volumes: Vec::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();

        // Verify proper escaping
        assert!(override_yaml.contains("MULTILINE: \"line1\\nline2\""));
        assert!(override_yaml.contains("QUOTED: \"value with \\\"quotes\\\"\""));
        assert!(override_yaml.contains("COLON: \"key:value\""));
        assert!(override_yaml.contains("HASH: \"before#after\""));
    }

    #[test]
    fn test_generate_injection_override_preserves_insertion_order() {
        let mut additional_env: IndexMap<String, String> = IndexMap::new();
        // Insert in this specific order: ZZZ, AAA, MMM
        additional_env.insert("ZZZ".to_string(), "last".to_string());
        additional_env.insert("AAA".to_string(), "first".to_string());
        additional_env.insert("MMM".to_string(), "middle".to_string());

        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env,
            external_volumes: Vec::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();

        // IndexMap preserves insertion order: ZZZ, AAA, MMM (not sorted alphabetically)
        let zzz_pos = override_yaml.find("ZZZ:").unwrap();
        let aaa_pos = override_yaml.find("AAA:").unwrap();
        let mmm_pos = override_yaml.find("MMM:").unwrap();

        assert!(
            zzz_pos < aaa_pos && aaa_pos < mmm_pos,
            "Keys should be in insertion order: ZZZ < AAA < MMM, but got ZZZ={}, AAA={}, MMM={}",
            zzz_pos,
            aaa_pos,
            mmm_pos
        );
    }

    #[test]
    fn test_escape_yaml_value() {
        // Simple value
        assert_eq!(escape_yaml_value("hello"), "\"hello\"");

        // Value with newline
        assert_eq!(escape_yaml_value("line1\nline2"), "\"line1\\nline2\"");

        // Value with quotes
        assert_eq!(escape_yaml_value("say \"hi\""), "\"say \\\"hi\\\"\"");

        // Value with colon
        assert_eq!(escape_yaml_value("key:value"), "\"key:value\"");

        // Value with backslash - backslash doesn't need special escaping in YAML
        // when double-quoted, unless combined with other special chars
        assert_eq!(escape_yaml_value(r"path\to\file"), r#""path\to\file""#);

        // Empty value
        assert_eq!(escape_yaml_value(""), "\"\"");

        // Value with leading space
        assert_eq!(escape_yaml_value(" leading"), "\" leading\"");
    }

    // Tests for parse_external_volumes_from_config

    #[test]
    fn test_parse_external_volumes_empty_config() {
        // Empty JSON
        let result = parse_external_volumes_from_config("").unwrap();
        assert!(result.is_empty());

        // JSON with no volumes section
        let config = r#"{"services": {"app": {"image": "nginx"}}}"#;
        let result = parse_external_volumes_from_config(config).unwrap();
        assert!(result.is_empty());

        // JSON with empty volumes section
        let config = r#"{"volumes": {}}"#;
        let result = parse_external_volumes_from_config(config).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_external_volumes_simple_form() {
        // external: true (simple boolean form)
        let config = r#"{
            "volumes": {
                "my_data": {
                    "external": true
                }
            }
        }"#;
        let result = parse_external_volumes_from_config(config).unwrap();
        assert_eq!(result, vec!["my_data"]);
    }

    #[test]
    fn test_parse_external_volumes_object_form_with_name() {
        // external: { name: "actual-name" } (object form with explicit name)
        let config = r#"{
            "volumes": {
                "local_name": {
                    "external": {
                        "name": "actual-external-volume"
                    }
                }
            }
        }"#;
        let result = parse_external_volumes_from_config(config).unwrap();
        assert_eq!(result, vec!["actual-external-volume"]);
    }

    #[test]
    fn test_parse_external_volumes_object_form_without_name() {
        // external: {} (object form without name, uses key name)
        let config = r#"{
            "volumes": {
                "my_volume": {
                    "external": {}
                }
            }
        }"#;
        let result = parse_external_volumes_from_config(config).unwrap();
        assert_eq!(result, vec!["my_volume"]);
    }

    #[test]
    fn test_parse_external_volumes_multiple_volumes() {
        // Mix of external and non-external volumes
        let config = r#"{
            "volumes": {
                "external_vol1": {
                    "external": true
                },
                "local_vol": {
                    "driver": "local"
                },
                "external_vol2": {
                    "external": {
                        "name": "shared-data"
                    }
                },
                "another_local": {}
            }
        }"#;
        let result = parse_external_volumes_from_config(config).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"external_vol1".to_string()));
        assert!(result.contains(&"shared-data".to_string()));
    }

    #[test]
    fn test_parse_external_volumes_external_false() {
        // external: false should not be included
        let config = r#"{
            "volumes": {
                "not_external": {
                    "external": false
                }
            }
        }"#;
        let result = parse_external_volumes_from_config(config).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_external_volumes_invalid_json() {
        // Invalid JSON should return an error
        let result = parse_external_volumes_from_config("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_external_volumes_realistic_config() {
        // A more realistic compose config output
        let config = r#"{
            "name": "myproject",
            "services": {
                "app": {
                    "image": "myapp:latest",
                    "volumes": [
                        {
                            "type": "volume",
                            "source": "app_data",
                            "target": "/data"
                        },
                        {
                            "type": "volume",
                            "source": "shared_cache",
                            "target": "/cache"
                        }
                    ]
                }
            },
            "volumes": {
                "app_data": {
                    "driver": "local"
                },
                "shared_cache": {
                    "external": true
                }
            }
        }"#;
        let result = parse_external_volumes_from_config(config).unwrap();
        assert_eq!(result, vec!["shared_cache"]);
    }
}
