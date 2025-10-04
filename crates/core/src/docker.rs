//! Docker and OCI container runtime integration
//!
//! This module handles Docker client abstraction, container lifecycle management,
//! image building, and container execution.

use crate::config::DevContainerConfig;
use crate::container::{ContainerIdentity, ContainerOps, ContainerResult};
use crate::errors::{DockerError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use tracing::{debug, instrument};

/// Container information returned by Docker operations

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    /// Container ID
    pub id: String,
    /// Container names
    pub names: Vec<String>,
    /// Container image
    pub image: String,
    /// Container status
    pub status: String,
    /// Container state
    pub state: String,
    /// Exposed ports from the container
    pub exposed_ports: Vec<ExposedPort>,
    /// Port mappings from host to container
    pub port_mappings: Vec<PortMapping>,
}

/// Represents an exposed port from a container

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExposedPort {
    /// Port number
    pub port: u16,
    /// Protocol (tcp/udp)
    pub protocol: String,
}

/// Represents a port mapping from host to container

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    /// Host port
    pub host_port: u16,
    /// Container port
    pub container_port: u16,
    /// Protocol (tcp/udp)
    pub protocol: String,
    /// Host IP
    pub host_ip: String,
}

/// Configuration for executing commands in containers

#[derive(Debug, Clone)]
pub struct ExecConfig {
    /// User to run command as
    pub user: Option<String>,
    /// Working directory
    pub working_dir: Option<String>,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Whether to allocate a TTY
    pub tty: bool,
    /// Whether to attach stdin
    pub interactive: bool,
    /// Whether to detach the command
    pub detach: bool,
}

/// Result of executing a command in a container

#[derive(Debug)]
pub struct ExecResult {
    /// Exit code of the command
    pub exit_code: i32,
    /// Whether the command completed successfully (exit code 0)
    pub success: bool,
}

/// Docker client abstraction trait
#[allow(async_fn_in_trait)]
pub trait Docker {
    /// Health check for Docker daemon availability
    async fn ping(&self) -> Result<()>;

    /// List containers with optional label selector
    async fn list_containers(&self, label_selector: Option<&str>) -> Result<Vec<ContainerInfo>>;

    /// Inspect a specific container by ID
    async fn inspect_container(&self, id: &str) -> Result<Option<ContainerInfo>>;

    /// Execute a command in a running container
    async fn exec(
        &self,
        container_id: &str,
        command: &[String],
        config: ExecConfig,
    ) -> Result<ExecResult>;

    /// Stop a container with optional timeout
    async fn stop_container(&self, container_id: &str, timeout: Option<u32>) -> Result<()>;
}

/// Docker client abstraction trait extended with container lifecycle operations
#[allow(async_fn_in_trait)]
pub trait DockerLifecycle: Docker + ContainerOps {
    /// Execute the complete `up` workflow: find existing containers, reuse or create new
    async fn up(
        &self,
        identity: &ContainerIdentity,
        config: &DevContainerConfig,
        workspace_path: &Path,
        remove_existing: bool,
    ) -> Result<ContainerResult>;
}

// Implement Docker trait for references to types that implement Docker

impl<T: Docker> Docker for &T {
    async fn ping(&self) -> Result<()> {
        (*self).ping().await
    }

    async fn list_containers(&self, label_selector: Option<&str>) -> Result<Vec<ContainerInfo>> {
        (*self).list_containers(label_selector).await
    }

    async fn inspect_container(&self, id: &str) -> Result<Option<ContainerInfo>> {
        (*self).inspect_container(id).await
    }

    async fn exec(
        &self,
        container_id: &str,
        command: &[String],
        config: ExecConfig,
    ) -> Result<ExecResult> {
        (*self).exec(container_id, command, config).await
    }

    async fn stop_container(&self, container_id: &str, timeout: Option<u32>) -> Result<()> {
        (*self).stop_container(container_id, timeout).await
    }
}

/// Generic CLI-based container runtime implementation
///
/// This can be used for both Docker and Podman runtimes since they share
/// a compatible CLI interface.
#[derive(Debug, Clone)]
pub struct CliRuntime {
    /// Container runtime CLI binary path (e.g., "docker" or "podman")
    runtime_path: String,
}

impl CliRuntime {
    /// Create a new CliRuntime for Docker
    pub fn docker() -> Self {
        Self {
            runtime_path: "docker".to_string(),
        }
    }

    /// Create a new CliRuntime for Podman
    pub fn podman() -> Self {
        Self {
            runtime_path: "podman".to_string(),
        }
    }

    /// Create a new CliRuntime with custom runtime binary path
    pub fn with_runtime_path(runtime_path: String) -> Self {
        Self { runtime_path }
    }
}

impl Default for CliRuntime {
    fn default() -> Self {
        Self::docker()
    }
}

/// CLI-based Docker implementation using docker command
///
/// This is a type alias for CliRuntime configured for Docker.
pub type CliDocker = CliRuntime;

// Provide backward-compatible constructors for CliDocker
impl CliDocker {
    /// Create a new CliDocker instance
    pub fn new() -> Self {
        Self::docker()
    }

    /// Create a new CliDocker instance with custom docker binary path
    pub fn with_path(docker_path: String) -> Self {
        Self {
            runtime_path: docker_path,
        }
    }
}

impl CliRuntime {
    /// Parse exposed ports from container Config.ExposedPorts
    fn parse_exposed_ports(config: &serde_json::Value) -> Vec<ExposedPort> {
        let mut exposed_ports = Vec::new();

        if let Some(exposed_ports_obj) = config
            .get("Config")
            .and_then(|c| c.get("ExposedPorts"))
            .and_then(|ep| ep.as_object())
        {
            for port_spec in exposed_ports_obj.keys() {
                if let Some((port_str, protocol)) = port_spec.split_once('/') {
                    if let Ok(port) = port_str.parse::<u16>() {
                        exposed_ports.push(ExposedPort {
                            port,
                            protocol: protocol.to_string(),
                        });
                    }
                }
            }
        }

        exposed_ports
    }

    /// Parse port mappings from container NetworkSettings.Ports
    fn parse_port_mappings(container: &serde_json::Value) -> Vec<PortMapping> {
        let mut port_mappings = Vec::new();

        if let Some(ports_obj) = container
            .get("NetworkSettings")
            .and_then(|ns| ns.get("Ports"))
            .and_then(|p| p.as_object())
        {
            for (port_spec, bindings) in ports_obj.iter() {
                if let Some((port_str, protocol)) = port_spec.split_once('/') {
                    if let Ok(container_port) = port_str.parse::<u16>() {
                        if let Some(bindings_array) = bindings.as_array() {
                            for binding in bindings_array {
                                if let (Some(host_port_str), Some(host_ip)) = (
                                    binding.get("HostPort").and_then(|hp| hp.as_str()),
                                    binding.get("HostIp").and_then(|hi| hi.as_str()),
                                ) {
                                    if let Ok(host_port) = host_port_str.parse::<u16>() {
                                        port_mappings.push(PortMapping {
                                            host_port,
                                            container_port,
                                            protocol: protocol.to_string(),
                                            host_ip: host_ip.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        port_mappings
    }

    /// Check if container runtime binary is available
    #[instrument(skip(self))]
    pub fn check_runtime_installed(&self) -> Result<()> {
        debug!(
            "Checking if container runtime binary is installed at: {}",
            self.runtime_path
        );

        let output = Command::new(&self.runtime_path).arg("--version").output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    debug!("Container runtime binary found and working");
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(
                        DockerError::CLIError(format!("Runtime version check failed: {}", stderr))
                            .into(),
                    )
                }
            }
            Err(e) => {
                debug!("Container runtime binary not found: {}", e);
                Err(DockerError::NotInstalled.into())
            }
        }
    }

    /// Alias for backward compatibility - check if docker is installed
    pub fn check_docker_installed(&self) -> Result<()> {
        self.check_runtime_installed()
    }

    /// Execute container runtime command and return stdout
    #[instrument(skip(self))]
    #[allow(dead_code)] // Used by future features
    fn execute_docker(&self, args: &[&str]) -> Result<String> {
        debug!(
            "Executing runtime command: {} {}",
            self.runtime_path,
            args.join(" ")
        );

        let output = Command::new(&self.runtime_path)
            .args(args)
            .output()
            .map_err(|e| {
                DockerError::CLIError(format!("Failed to execute docker command: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DockerError::CLIError(format!("Docker command failed: {}", stderr)).into());
        }

        let stdout = String::from_utf8(output.stdout)
            .map_err(|e| DockerError::CLIError(format!("Invalid UTF-8 in docker output: {}", e)))?;

        Ok(stdout)
    }

    /// Parse docker ps JSON output into ContainerInfo
    #[allow(dead_code)] // Used by future features
    fn parse_container_list(&self, json_output: &str) -> Result<Vec<ContainerInfo>> {
        if json_output.trim().is_empty() {
            return Ok(Vec::new());
        }

        let containers: Vec<serde_json::Value> = json_output
            .trim()
            .lines()
            .map(serde_json::from_str)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| {
                DockerError::CLIError(format!("Failed to parse container list JSON: {}", e))
            })?;

        let mut result = Vec::new();
        for container in containers {
            let container_info = ContainerInfo {
                id: container
                    .get("ID")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                names: container
                    .get("Names")
                    .and_then(|v| v.as_str())
                    .map(|s| s.split(',').map(|name| name.trim().to_string()).collect())
                    .unwrap_or_default(),
                image: container
                    .get("Image")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                status: container
                    .get("Status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                state: container
                    .get("State")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                exposed_ports: vec![], // Not available in list format
                port_mappings: vec![], // Not available in list format
            };
            result.push(container_info);
        }

        Ok(result)
    }

    /// Check if we're running in a TTY
    pub fn is_tty() -> bool {
        use std::io::IsTerminal;
        std::io::stdin().is_terminal()
    }

    /// Parse environment variables from KEY=VALUE format
    #[allow(dead_code)] // Used by exec command
    fn parse_env_vars(env_vars: &[String]) -> Result<HashMap<String, String>> {
        let mut env_map = HashMap::new();
        for env_var in env_vars {
            if let Some((key, value)) = env_var.split_once('=') {
                env_map.insert(key.to_string(), value.to_string());
            } else {
                return Err(DockerError::CLIError(format!(
                    "Invalid environment variable format: '{}'. Expected KEY=VALUE",
                    env_var
                ))
                .into());
            }
        }
        Ok(env_map)
    }

    /// Parse docker inspect JSON output into ContainerInfo
    #[allow(dead_code)] // Used by future features
    fn parse_container_inspect(&self, json_output: &str) -> Result<Option<ContainerInfo>> {
        if json_output.trim().is_empty() {
            return Ok(None);
        }

        let containers: Vec<serde_json::Value> = serde_json::from_str(json_output)
            .map_err(|e| DockerError::CLIError(format!("Failed to parse inspect JSON: {}", e)))?;

        if containers.is_empty() {
            return Ok(None);
        }

        let container = &containers[0];
        let exposed_ports = Self::parse_exposed_ports(container);
        let port_mappings = Self::parse_port_mappings(container);

        let container_info = ContainerInfo {
            id: container
                .get("Id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            names: container
                .get("Name")
                .and_then(|v| v.as_str())
                .map(|name| vec![name.trim_start_matches('/').to_string()])
                .unwrap_or_default(),
            image: container
                .get("Config")
                .and_then(|config| config.get("Image"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            status: container
                .get("State")
                .and_then(|state| state.get("Status"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            state: container
                .get("State")
                .and_then(|state| state.get("Status"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            exposed_ports,
            port_mappings,
        };

        Ok(Some(container_info))
    }
}

impl Docker for CliRuntime {
    #[instrument(skip(self))]
    async fn ping(&self) -> Result<()> {
        debug!("Pinging container runtime daemon");

        // Use blocking call as sync is acceptable per issue requirements
        tokio::task::spawn_blocking({
            let runtime_path = self.runtime_path.clone();
            move || {
                let output = Command::new(&runtime_path)
                    .args(["version", "--format", "json"])
                    .output();

                match output {
                    Ok(output) => {
                        if output.status.success() {
                            debug!("Container runtime daemon is available");
                            Ok(())
                        } else {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            Err(
                                DockerError::CLIError(format!("Runtime ping failed: {}", stderr))
                                    .into(),
                            )
                        }
                    }
                    Err(e) => {
                        debug!("Runtime ping failed: {}", e);
                        Err(DockerError::NotInstalled.into())
                    }
                }
            }
        })
        .await
        .map_err(|e| DockerError::CLIError(format!("Task join error: {}", e)))?
    }

    #[instrument(skip(self))]
    async fn list_containers(&self, label_selector: Option<&str>) -> Result<Vec<ContainerInfo>> {
        debug!(
            "Listing containers with label selector: {:?}",
            label_selector
        );

        let runtime_path = self.runtime_path.clone();
        let label_selector = label_selector.map(|s| s.to_string());

        tokio::task::spawn_blocking(move || {
            let mut args: Vec<String> = vec!["ps", "--all", "--format", "{{json .}}"]
                .into_iter()
                .map(|s| s.to_string())
                .collect();

            // Support multiple label filters; Docker expects one --filter per label
            if let Some(label) = &label_selector {
                for part in label.split(',') {
                    let trimmed = part.trim();
                    if !trimmed.is_empty() {
                        args.push("--filter".to_string());
                        // Each part is expected to be key or key=value
                        args.push(format!("label={}", trimmed));
                    }
                }
            }

            let output = Command::new(&runtime_path)
                .args(&args)
                .output()
                .map_err(|e| DockerError::CLIError(format!("Failed to list containers: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(DockerError::CLIError(format!("Docker ps failed: {}", stderr)).into());
            }

            let stdout = String::from_utf8(output.stdout).map_err(|e| {
                DockerError::CLIError(format!("Invalid UTF-8 in docker output: {}", e))
            })?;

            // Parse the JSON output
            let mut containers = Vec::new();
            for line in stdout.trim().lines() {
                if line.trim().is_empty() {
                    continue;
                }

                let container: serde_json::Value = serde_json::from_str(line).map_err(|e| {
                    DockerError::CLIError(format!("Failed to parse container JSON: {}", e))
                })?;

                let container_info = ContainerInfo {
                    id: container
                        .get("ID")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    names: container
                        .get("Names")
                        .and_then(|v| v.as_str())
                        .map(|s| vec![s.to_string()])
                        .unwrap_or_default(),
                    image: container
                        .get("Image")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    status: container
                        .get("Status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    state: container
                        .get("State")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    exposed_ports: vec![], // Not available in list format
                    port_mappings: vec![], // Not available in list format
                };
                containers.push(container_info);
            }

            Ok(containers)
        })
        .await
        .map_err(|e| DockerError::CLIError(format!("Task join error: {}", e)))?
    }

    #[instrument(skip(self))]
    async fn inspect_container(&self, id: &str) -> Result<Option<ContainerInfo>> {
        debug!("Inspecting container: {}", id);

        let runtime_path = self.runtime_path.clone();
        let container_id = id.to_string();

        tokio::task::spawn_blocking(move || {
            let output = Command::new(&runtime_path)
                .args(["inspect", &container_id])
                .output()
                .map_err(|e| {
                    DockerError::CLIError(format!("Failed to inspect container: {}", e))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("No such object") || stderr.contains("No such container") {
                    return Ok(None);
                }
                return Err(
                    DockerError::CLIError(format!("Docker inspect failed: {}", stderr)).into(),
                );
            }

            let stdout = String::from_utf8(output.stdout).map_err(|e| {
                DockerError::CLIError(format!("Invalid UTF-8 in docker output: {}", e))
            })?;

            let containers: Vec<serde_json::Value> =
                serde_json::from_str(&stdout).map_err(|e| {
                    DockerError::CLIError(format!("Failed to parse inspect JSON: {}", e))
                })?;

            if containers.is_empty() {
                return Ok(None);
            }

            let container = &containers[0];
            let exposed_ports = Self::parse_exposed_ports(container);
            let port_mappings = Self::parse_port_mappings(container);

            let container_info = ContainerInfo {
                id: container
                    .get("Id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                names: container
                    .get("Name")
                    .and_then(|v| v.as_str())
                    .map(|name| vec![name.trim_start_matches('/').to_string()])
                    .unwrap_or_default(),
                image: container
                    .get("Config")
                    .and_then(|config| config.get("Image"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                status: container
                    .get("State")
                    .and_then(|state| state.get("Status"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                state: container
                    .get("State")
                    .and_then(|state| state.get("Status"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                exposed_ports,
                port_mappings,
            };

            Ok(Some(container_info))
        })
        .await
        .map_err(|e| DockerError::CLIError(format!("Task join error: {}", e)))?
    }

    #[instrument(skip(self))]
    async fn exec(
        &self,
        container_id: &str,
        command: &[String],
        config: ExecConfig,
    ) -> Result<ExecResult> {
        debug!("Executing command in container: {}", container_id);

        let runtime_path = self.runtime_path.clone();
        let container_id = container_id.to_string();
        let command = command.to_vec();

        tokio::task::spawn_blocking(move || {
            let mut args = vec!["exec"];

            // Add TTY and interactive flags
            if config.tty {
                args.push("-t");
            }
            if config.interactive {
                args.push("-i");
            }

            // Add detach flag
            if config.detach {
                args.push("-d");
            }

            // Add user if specified
            if let Some(ref user) = config.user {
                args.push("-u");
                args.push(user);
            }

            // Add working directory if specified
            if let Some(ref workdir) = config.working_dir {
                args.push("-w");
                args.push(workdir);
            }

            // Add environment variables
            let env_args: Vec<String> = config
                .env
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            for env_arg in &env_args {
                args.push("-e");
                args.push(env_arg);
            }

            // Add signal proxy for TTY sessions
            if config.tty {
                args.push("--sig-proxy=true");
            }

            // Add container ID
            args.push(&container_id);

            // Add the command and arguments
            for cmd_part in &command {
                args.push(cmd_part);
            }

            debug!("Runtime exec args: {:?}", args);

            let mut child = std::process::Command::new(&runtime_path)
                .args(&args)
                .spawn()
                .map_err(|e| {
                    DockerError::CLIError(format!("Failed to spawn runtime exec: {}", e))
                })?;

            let exit_status = child.wait().map_err(|e| {
                DockerError::CLIError(format!("Failed to wait for runtime exec: {}", e))
            })?;

            let exit_code = exit_status.code().unwrap_or(-1);
            let success = exit_status.success();

            Ok(ExecResult { exit_code, success })
        })
        .await
        .map_err(|e| DockerError::CLIError(format!("Task join error: {}", e)))?
    }

    #[instrument(skip(self))]
    async fn stop_container(&self, container_id: &str, timeout: Option<u32>) -> Result<()> {
        debug!("Stopping container: {}", container_id);

        let runtime_path = self.runtime_path.clone();
        let container_id = container_id.to_string();

        tokio::task::spawn_blocking(move || {
            let mut args = vec!["stop"];

            let timeout_str = timeout.map(|t| t.to_string());
            if let Some(ref timeout_str) = timeout_str {
                args.push("-t");
                args.push(timeout_str);
            }

            args.push(&container_id);

            let output = std::process::Command::new(&runtime_path)
                .args(&args)
                .output()
                .map_err(|e| DockerError::CLIError(format!("Failed to run stop command: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(DockerError::CLIError(format!(
                    "stop command failed: {}",
                    stderr
                )));
            }

            debug!("Container {} stopped successfully", container_id);
            Ok(())
        })
        .await
        .map_err(|e| DockerError::CLIError(format!("Task join error: {}", e)))?
        .map_err(Into::into)
    }
}

impl ContainerOps for CliRuntime {
    #[instrument(skip(self))]
    async fn find_matching_containers(&self, identity: &ContainerIdentity) -> Result<Vec<String>> {
        debug!("Finding containers with identity: {:?}", identity);

        let label_selector = identity.label_selector();
        let containers = self.list_containers(Some(&label_selector)).await?;

        let container_ids: Vec<String> = containers.into_iter().map(|c| c.id).collect();
        debug!("Found {} matching containers", container_ids.len());

        Ok(container_ids)
    }

    #[instrument(skip(self, config))]
    async fn create_container(
        &self,
        identity: &ContainerIdentity,
        config: &DevContainerConfig,
        workspace_path: &Path,
    ) -> Result<String> {
        debug!("Creating container with identity: {:?}", identity);

        let container_name = identity.container_name();
        let labels = identity.labels();

        // Build docker run command
        let mut args = vec!["create".to_string()];

        // Add container name
        args.push("--name".to_string());
        args.push(container_name.clone());

        // Add labels
        for (key, value) in labels {
            args.push("--label".to_string());
            args.push(format!("{}={}", key, value));
        }

        // Add workspace mount (either from workspaceMount config or default)
        if let Some(ref workspace_mount) = config.workspace_mount {
            // Use custom workspace mount from config
            let mount = crate::mount::MountParser::parse_mount(workspace_mount)
                .map_err(|e| DockerError::CLIError(format!("Invalid workspaceMount: {}", e)))?;
            args.extend(mount.to_docker_args());
        } else {
            // Use default workspace mount - respect workspaceFolder from config if specified
            let target_path = if let Some(ref workspace_folder) = config.workspace_folder {
                workspace_folder.clone()
            } else {
                format!(
                    "/workspaces/{}",
                    workspace_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("workspace")
                )
            };
            let workspace_mount = {
                // Use platform-aware path conversion for Docker Desktop compatibility
                let platform = crate::platform::Platform::detect();
                let source_path = if platform.needs_docker_desktop_path_conversion() {
                    crate::platform::convert_path_for_docker_desktop(workspace_path)
                } else {
                    workspace_path.display().to_string()
                };
                format!("type=bind,source={},target={}", source_path, target_path)
            };
            args.push("--mount".to_string());
            args.push(workspace_mount);
        }

        // Add additional mounts from configuration
        let mounts = crate::mount::MountParser::parse_mounts_from_json(&config.mounts);
        for mount in mounts {
            args.extend(mount.to_docker_args());
        }

        // Apply containerEnv variables from configuration
        for (key, value) in &config.container_env {
            args.push("--env".to_string());
            args.push(format!("{}={}", key, value));
        }

        // Apply remoteEnv variables (Dev Container spec: remote env should be available during shell/exec sessions)
        // `None` values indicate removal; emulate by setting an empty value to override defaults.
        for (key, value) in &config.remote_env {
            args.push("--env".to_string());
            match value {
                Some(val) => args.push(format!("{}={}", key, val)),
                None => args.push(format!("{}=", key)),
            }
        }

        // Add security options from configuration (centralized)
        {
            use crate::security::SecurityOptions;
            let security = SecurityOptions {
                privileged: config.privileged.unwrap_or(false),
                cap_add: config.cap_add.clone(),
                security_opt: config.security_opt.clone(),
                conflicts: Vec::new(),
            };
            args.extend(security.to_docker_args());
        }

        // Add runArgs if present
        args.extend(config.run_args.iter().cloned());

        // Add image
        let image = config.image.as_ref().ok_or_else(|| {
            DockerError::CLIError("No image specified in configuration".to_string())
        })?;
        args.push(image.clone());

        // Respect overrideCommand semantics (default: true)
        // When enabled, ensure the container stays running so lifecycle commands can execute.
        // Use a minimal keep-alive command that is broadly available.
        // Reference: DevContainer spec "overrideCommand".
        let override_cmd = config.override_command.unwrap_or(true);
        if override_cmd {
            // Use a portable keep-alive: prefer 'sleep infinity' (GNU coreutils),
            // fall back to 'tail -f /dev/null' for BusyBox/Alpine images.
            args.push("/bin/sh".to_string());
            args.push("-c".to_string());
            args.push("sleep infinity || tail -f /dev/null".to_string());
        }

        // Execute container create command
        let runtime_path = self.runtime_path.clone();
        let container_id = tokio::task::spawn_blocking(move || {
            let output = Command::new(&runtime_path)
                .args(&args)
                .output()
                .map_err(|e| DockerError::CLIError(format!("Failed to create container: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(DockerError::CLIError(format!(
                    "Container create failed: {}",
                    stderr
                )));
            }

            let stdout = String::from_utf8(output.stdout).map_err(|e| {
                DockerError::CLIError(format!("Invalid UTF-8 in runtime output: {}", e))
            })?;

            Ok(stdout.trim().to_string())
        })
        .await
        .map_err(|e| DockerError::CLIError(format!("Task join error: {}", e)))??;

        debug!("Created container with ID: {}", container_id);
        Ok(container_id)
    }

    #[instrument(skip(self))]
    async fn start_container(&self, container_id: &str) -> Result<()> {
        debug!("Starting container: {}", container_id);

        let runtime_path = self.runtime_path.clone();
        let container_id = container_id.to_string();

        tokio::task::spawn_blocking(move || -> std::result::Result<(), DockerError> {
            let output = Command::new(&runtime_path)
                .args(["start", &container_id])
                .output()
                .map_err(|e| DockerError::CLIError(format!("Failed to start container: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(DockerError::CLIError(format!(
                    "Start command failed: {}",
                    stderr
                )));
            }

            Ok(())
        })
        .await
        .map_err(|e| DockerError::CLIError(format!("Task join error: {}", e)))?
        .map_err(Into::into)
    }

    #[instrument(skip(self))]
    async fn remove_container(&self, container_id: &str) -> Result<()> {
        debug!("Removing container: {}", container_id);

        let runtime_path = self.runtime_path.clone();
        let container_id = container_id.to_string();

        tokio::task::spawn_blocking(move || -> std::result::Result<(), DockerError> {
            let output = Command::new(&runtime_path)
                .args(["rm", "-f", &container_id])
                .output()
                .map_err(|e| DockerError::CLIError(format!("Failed to remove container: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(DockerError::CLIError(format!(
                    "Remove command failed: {}",
                    stderr
                )));
            }

            Ok(())
        })
        .await
        .map_err(|e| DockerError::CLIError(format!("Task join error: {}", e)))?
        .map_err(Into::into)
    }

    #[instrument(skip(self))]
    async fn get_container_image(&self, container_id: &str) -> Result<String> {
        debug!("Getting image for container: {}", container_id);

        let runtime_path = self.runtime_path.clone();
        let container_id = container_id.to_string();

        tokio::task::spawn_blocking(move || -> std::result::Result<String, DockerError> {
            let output = Command::new(&runtime_path)
                .args(["inspect", "--format", "{{.Image}}", &container_id])
                .output()
                .map_err(|e| {
                    DockerError::CLIError(format!("Failed to inspect container: {}", e))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(DockerError::CLIError(format!(
                    "Docker inspect failed: {}",
                    stderr
                )));
            }

            let stdout = String::from_utf8(output.stdout).map_err(|e| {
                DockerError::CLIError(format!("Invalid UTF-8 in docker output: {}", e))
            })?;

            Ok(stdout.trim().to_string())
        })
        .await
        .map_err(|e| DockerError::CLIError(format!("Task join error: {}", e)))?
        .map_err(Into::into)
    }
}

impl DockerLifecycle for CliRuntime {
    #[instrument(skip(self, config))]
    async fn up(
        &self,
        identity: &ContainerIdentity,
        config: &DevContainerConfig,
        workspace_path: &Path,
        remove_existing: bool,
    ) -> Result<ContainerResult> {
        debug!("Starting up container workflow");

        // Find existing containers
        let existing_containers = self.find_matching_containers(identity).await?;

        if !existing_containers.is_empty() && !remove_existing {
            // Reuse existing container
            let container_id = existing_containers[0].clone();
            debug!("Reusing existing container: {}", container_id);

            // Start the container if it's not running
            self.start_container(&container_id).await?;

            // Get the image ID
            let image_id = self.get_container_image(&container_id).await?;

            return Ok(ContainerResult {
                container_id,
                reused: true,
                image_id,
            });
        }

        // Remove existing containers if requested
        if remove_existing {
            for container_id in existing_containers {
                debug!("Removing existing container: {}", container_id);
                self.remove_container(&container_id).await?;
            }
        }

        // Create new container
        let container_id = self
            .create_container(identity, config, workspace_path)
            .await?;
        self.start_container(&container_id).await?;

        // Get the image ID
        let image_id = self.get_container_image(&container_id).await?;

        debug!(
            "Successfully created and started new container: {}",
            container_id
        );

        Ok(ContainerResult {
            container_id,
            reused: false,
            image_id,
        })
    }
}

pub mod mock {
    //! Mock Docker runtime for testing exec and lifecycle flows
    //!
    //! This module provides a mock implementation of the Docker trait that can be used
    //! for testing without requiring a real Docker daemon. It supports configurable
    //! responses for container operations, exec commands, and timing simulation.

    use crate::config::DevContainerConfig;
    use crate::container::{ContainerIdentity, ContainerOps, ContainerResult};
    use crate::docker::{ContainerInfo, Docker, DockerLifecycle, ExecConfig, ExecResult};
    use crate::errors::{DockerError, Result};
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tracing::{debug, instrument};

    /// Configuration for exec command responses
    #[derive(Debug, Clone)]
    pub struct MockExecResponse {
        /// Exit code to return
        pub exit_code: i32,
        /// Whether to simulate success
        pub success: bool,
        /// Optional delay to simulate command execution time
        pub delay: Option<Duration>,
        /// Optional stdout content (for future use)
        pub stdout: Option<String>,
        /// Optional stderr content (for future use)
        pub stderr: Option<String>,
    }

    impl Default for MockExecResponse {
        fn default() -> Self {
            Self {
                exit_code: 0,
                success: true,
                delay: None,
                stdout: None,
                stderr: None,
            }
        }
    }

    /// Mock container state for simulation
    #[derive(Debug, Clone)]
    pub struct MockContainer {
        /// Container ID
        pub id: String,
        /// Container names
        pub names: Vec<String>,
        /// Container image
        pub image: String,
        /// Container status
        pub status: String,
        /// Container state
        pub state: String,
        /// Labels on the container
        pub labels: HashMap<String, String>,
    }

    impl MockContainer {
        /// Create a new mock container
        pub fn new(id: String, name: String, image: String) -> Self {
            Self {
                id,
                names: vec![name],
                image,
                status: "Up 5 minutes".to_string(),
                state: "running".to_string(),
                labels: HashMap::new(),
            }
        }

        /// Set container labels
        pub fn with_labels(mut self, labels: HashMap<String, String>) -> Self {
            self.labels = labels;
            self
        }

        /// Set container state
        pub fn with_state(mut self, state: String, status: String) -> Self {
            self.state = state;
            self.status = status;
            self
        }
    }

    /// Configuration for the MockDocker runtime
    #[derive(Debug, Clone)]
    pub struct MockDockerConfig {
        /// Whether ping should succeed
        pub ping_success: bool,
        /// Default exec response for commands
        pub default_exec_response: MockExecResponse,
        /// Command-specific exec responses (command string -> response)
        pub exec_responses: HashMap<String, MockExecResponse>,
        /// Whether to capture TTY flags in exec calls
        pub capture_tty_flags: bool,
        /// Simulate Docker daemon unavailable
        pub daemon_unavailable: bool,
    }

    impl Default for MockDockerConfig {
        fn default() -> Self {
            Self {
                ping_success: true,
                default_exec_response: MockExecResponse::default(),
                exec_responses: HashMap::new(),
                capture_tty_flags: true,
                daemon_unavailable: false,
            }
        }
    }

    /// Mock Docker runtime implementation
    #[derive(Debug)]
    pub struct MockDocker {
        /// Configuration for mock behavior
        config: Arc<Mutex<MockDockerConfig>>,
        /// Mock containers in the "system"
        containers: Arc<Mutex<Vec<MockContainer>>>,
        /// History of exec calls made (for testing verification)
        exec_history: Arc<Mutex<Vec<MockExecCall>>>,
    }

    /// Record of an exec call for verification in tests
    #[derive(Debug, Clone)]
    pub struct MockExecCall {
        /// Container ID where exec was called
        pub container_id: String,
        /// Command that was executed
        pub command: Vec<String>,
        /// Exec configuration used
        pub config: ExecConfig,
        /// Timestamp when the call was made
        pub timestamp: Instant,
    }

    impl MockDocker {
        /// Create a new MockDocker instance with default configuration
        pub fn new() -> Self {
            Self::with_config(MockDockerConfig::default())
        }

        /// Create a new MockDocker instance with custom configuration
        pub fn with_config(config: MockDockerConfig) -> Self {
            Self {
                config: Arc::new(Mutex::new(config)),
                containers: Arc::new(Mutex::new(Vec::new())),
                exec_history: Arc::new(Mutex::new(Vec::new())),
            }
        }

        /// Add a mock container to the system
        pub fn add_container(&self, container: MockContainer) {
            let mut containers = self.containers.lock().unwrap();
            containers.push(container);
        }

        /// Clear all mock containers
        pub fn clear_containers(&self) {
            let mut containers = self.containers.lock().unwrap();
            containers.clear();
        }

        /// Get history of exec calls made
        pub fn get_exec_history(&self) -> Vec<MockExecCall> {
            let history = self.exec_history.lock().unwrap();
            history.clone()
        }

        /// Clear exec call history
        pub fn clear_exec_history(&self) {
            let mut history = self.exec_history.lock().unwrap();
            history.clear();
        }

        /// Update mock configuration
        pub fn update_config<F>(&self, f: F)
        where
            F: FnOnce(&mut MockDockerConfig),
        {
            let mut config = self.config.lock().unwrap();
            f(&mut config);
        }

        /// Set specific exec response for a command
        pub fn set_exec_response(&self, command: String, response: MockExecResponse) {
            let mut config = self.config.lock().unwrap();
            config.exec_responses.insert(command, response);
        }

        /// Convert MockContainer to ContainerInfo for trait compatibility
        fn container_to_info(&self, container: &MockContainer) -> ContainerInfo {
            ContainerInfo {
                id: container.id.clone(),
                names: container.names.clone(),
                image: container.image.clone(),
                status: container.status.clone(),
                state: container.state.clone(),
                exposed_ports: vec![], // Not implemented for mock
                port_mappings: vec![], // Not implemented for mock
            }
        }

        /// Check if container matches label selector
        fn matches_label_selector(&self, container: &MockContainer, selector: &str) -> bool {
            // Parse label selector (simplified implementation)
            // Supports: "key=value" or "key" format, comma-separated
            for part in selector.split(',') {
                let trimmed = part.trim();
                if trimmed.is_empty() {
                    continue;
                }

                if let Some((key, value)) = trimmed.split_once('=') {
                    // key=value format
                    if let Some(container_value) = container.labels.get(key) {
                        if container_value != value {
                            return false;
                        }
                    } else {
                        return false;
                    }
                } else {
                    // key format - just check if key exists
                    if !container.labels.contains_key(trimmed) {
                        return false;
                    }
                }
            }
            true
        }
    }

    impl Default for MockDocker {
        fn default() -> Self {
            Self::new()
        }
    }

    #[allow(async_fn_in_trait)]
    impl Docker for MockDocker {
        #[instrument(skip(self))]
        async fn ping(&self) -> Result<()> {
            debug!("MockDocker ping called");

            let config = self.config.lock().unwrap();
            if config.daemon_unavailable {
                return Err(DockerError::NotInstalled.into());
            }

            if config.ping_success {
                debug!("MockDocker ping successful");
                Ok(())
            } else {
                debug!("MockDocker ping failed (configured)");
                Err(DockerError::CLIError("Mock ping failure".to_string()).into())
            }
        }

        #[instrument(skip(self))]
        async fn list_containers(
            &self,
            label_selector: Option<&str>,
        ) -> Result<Vec<ContainerInfo>> {
            debug!(
                "MockDocker list_containers called with selector: {:?}",
                label_selector
            );

            let config = self.config.lock().unwrap();
            if config.daemon_unavailable {
                return Err(DockerError::NotInstalled.into());
            }

            let containers = self.containers.lock().unwrap();
            let mut result = Vec::new();

            for container in containers.iter() {
                let matches = if let Some(selector) = label_selector {
                    self.matches_label_selector(container, selector)
                } else {
                    true
                };

                if matches {
                    result.push(self.container_to_info(container));
                }
            }

            debug!("MockDocker returning {} containers", result.len());
            Ok(result)
        }

        #[instrument(skip(self))]
        async fn inspect_container(&self, id: &str) -> Result<Option<ContainerInfo>> {
            debug!("MockDocker inspect_container called for ID: {}", id);

            let config = self.config.lock().unwrap();
            if config.daemon_unavailable {
                return Err(DockerError::NotInstalled.into());
            }

            let containers = self.containers.lock().unwrap();
            for container in containers.iter() {
                if container.id == id || container.names.contains(&id.to_string()) {
                    debug!("MockDocker found container for inspection");
                    return Ok(Some(self.container_to_info(container)));
                }
            }

            debug!("MockDocker container not found for inspection");
            Ok(None)
        }

        #[instrument(skip(self, config))]
        async fn exec(
            &self,
            container_id: &str,
            command: &[String],
            config: ExecConfig,
        ) -> Result<ExecResult> {
            debug!(
                "MockDocker exec called on container {} with command: {:?}",
                container_id, command
            );

            let response = {
                let mock_config = self.config.lock().unwrap();
                if mock_config.daemon_unavailable {
                    return Err(DockerError::NotInstalled.into());
                }

                // Find appropriate response
                let command_str = command.join(" ");
                mock_config
                    .exec_responses
                    .get(&command_str)
                    .cloned()
                    .unwrap_or_else(|| mock_config.default_exec_response.clone())
            };

            // Record the exec call
            let exec_call = MockExecCall {
                container_id: container_id.to_string(),
                command: command.to_vec(),
                config: config.clone(),
                timestamp: Instant::now(),
            };

            {
                let mut history = self.exec_history.lock().unwrap();
                history.push(exec_call);
            }

            // Simulate execution delay if configured
            if let Some(delay) = response.delay {
                debug!("MockDocker simulating exec delay: {:?}", delay);
                tokio::time::sleep(delay).await;
            }

            debug!(
                "MockDocker exec returning exit_code: {}, success: {}",
                response.exit_code, response.success
            );

            Ok(ExecResult {
                exit_code: response.exit_code,
                success: response.success,
            })
        }

        #[instrument(skip(self))]
        async fn stop_container(&self, container_id: &str, _timeout: Option<u32>) -> Result<()> {
            debug!("MockDocker stop_container called for ID: {}", container_id);

            let config = self.config.lock().unwrap();
            if config.daemon_unavailable {
                return Err(DockerError::NotInstalled.into());
            }

            // Update container state to stopped
            let mut containers = self.containers.lock().unwrap();
            for container in containers.iter_mut() {
                if container.id == container_id {
                    container.state = "exited".to_string();
                    container.status = "Exited (0) 1 second ago".to_string();
                    debug!("MockDocker container stopped");
                    return Ok(());
                }
            }

            debug!("MockDocker container not found for stopping");
            Err(DockerError::CLIError(format!("Container {} not found", container_id)).into())
        }
    }

    #[allow(async_fn_in_trait)]
    impl ContainerOps for MockDocker {
        #[instrument(skip(self))]
        async fn find_matching_containers(
            &self,
            identity: &ContainerIdentity,
        ) -> Result<Vec<String>> {
            debug!("MockDocker find_matching_containers called");

            let label_selector = identity.label_selector();
            let containers = self.list_containers(Some(&label_selector)).await?;
            let container_ids: Vec<String> = containers.into_iter().map(|c| c.id).collect();

            debug!(
                "MockDocker found {} matching containers",
                container_ids.len()
            );
            Ok(container_ids)
        }

        #[instrument(skip(self, config))]
        async fn create_container(
            &self,
            identity: &ContainerIdentity,
            config: &DevContainerConfig,
            workspace_path: &Path,
        ) -> Result<String> {
            debug!("MockDocker create_container called");

            let mock_config = self.config.lock().unwrap();
            if mock_config.daemon_unavailable {
                return Err(DockerError::NotInstalled.into());
            }

            // Generate a mock container ID
            let container_id = format!("mock-container-{}", fastrand::u64(..));
            let container_name = identity.container_name();
            let image = config
                .image
                .as_deref()
                .unwrap_or("mock-image:latest")
                .to_string();

            // Create mock container with identity labels
            let mut container = MockContainer::new(container_id.clone(), container_name, image);
            container.labels = identity.labels();

            // Add to mock container list
            {
                let mut containers = self.containers.lock().unwrap();
                containers.push(container);
            }

            debug!("MockDocker created container: {}", container_id);
            Ok(container_id)
        }

        #[instrument(skip(self))]
        async fn start_container(&self, container_id: &str) -> Result<()> {
            debug!("MockDocker start_container called for ID: {}", container_id);

            let config = self.config.lock().unwrap();
            if config.daemon_unavailable {
                return Err(DockerError::NotInstalled.into());
            }

            // Update container state to running
            let mut containers = self.containers.lock().unwrap();
            for container in containers.iter_mut() {
                if container.id == container_id {
                    container.state = "running".to_string();
                    container.status = "Up 1 second".to_string();
                    debug!("MockDocker container started");
                    return Ok(());
                }
            }

            debug!("MockDocker container not found for starting");
            Err(DockerError::CLIError(format!("Container {} not found", container_id)).into())
        }

        #[instrument(skip(self))]
        async fn remove_container(&self, container_id: &str) -> Result<()> {
            debug!(
                "MockDocker remove_container called for ID: {}",
                container_id
            );

            let config = self.config.lock().unwrap();
            if config.daemon_unavailable {
                return Err(DockerError::NotInstalled.into());
            }

            // Remove container from mock list
            let mut containers = self.containers.lock().unwrap();
            let initial_len = containers.len();
            containers.retain(|c| c.id != container_id);

            if containers.len() < initial_len {
                debug!("MockDocker container removed");
                Ok(())
            } else {
                debug!("MockDocker container not found for removal");
                Err(DockerError::CLIError(format!("Container {} not found", container_id)).into())
            }
        }

        #[instrument(skip(self))]
        async fn get_container_image(&self, container_id: &str) -> Result<String> {
            debug!(
                "MockDocker get_container_image called for ID: {}",
                container_id
            );

            let config = self.config.lock().unwrap();
            if config.daemon_unavailable {
                return Err(DockerError::NotInstalled.into());
            }

            let containers = self.containers.lock().unwrap();
            for container in containers.iter() {
                if container.id == container_id {
                    debug!("MockDocker returning image: {}", container.image);
                    return Ok(container.image.clone());
                }
            }

            debug!("MockDocker container not found for image query");
            Err(DockerError::CLIError(format!("Container {} not found", container_id)).into())
        }
    }

    #[allow(async_fn_in_trait)]
    impl DockerLifecycle for MockDocker {
        #[instrument(skip(self, config))]
        async fn up(
            &self,
            identity: &ContainerIdentity,
            config: &DevContainerConfig,
            workspace_path: &Path,
            remove_existing: bool,
        ) -> Result<ContainerResult> {
            debug!("MockDocker up called");

            // Find existing containers
            let existing_containers = self.find_matching_containers(identity).await?;

            if !existing_containers.is_empty() && !remove_existing {
                // Reuse existing container
                let container_id = existing_containers[0].clone();
                debug!("MockDocker reusing existing container: {}", container_id);

                // Start the container if it's not running
                self.start_container(&container_id).await?;

                // Get the image ID
                let image_id = self.get_container_image(&container_id).await?;

                return Ok(ContainerResult {
                    container_id,
                    reused: true,
                    image_id,
                });
            }

            // Remove existing containers if requested
            if remove_existing {
                for container_id in existing_containers {
                    debug!("MockDocker removing existing container: {}", container_id);
                    self.remove_container(&container_id).await?;
                }
            }

            // Create new container
            let container_id = self
                .create_container(identity, config, workspace_path)
                .await?;
            self.start_container(&container_id).await?;

            // Get the image ID
            let image_id = self.get_container_image(&container_id).await?;

            debug!(
                "MockDocker successfully created and started new container: {}",
                container_id
            );

            Ok(ContainerResult {
                container_id,
                reused: false,
                image_id,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_docker_new() {
        let docker = CliDocker::new();
        assert_eq!(docker.runtime_path, "docker");
    }

    #[test]
    fn test_cli_docker_with_path() {
        let custom_path = "/usr/local/bin/docker";
        let docker = CliDocker::with_path(custom_path.to_string());
        assert_eq!(docker.runtime_path, custom_path);
    }

    #[test]
    fn test_parse_container_list_empty() {
        let docker = CliDocker::new();
        let result = docker.parse_container_list("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_container_list_single_container() {
        let docker = CliDocker::new();
        let json_input = r#"{"ID":"abc123","Names":"test-container","Image":"ubuntu:20.04","Status":"Up 5 minutes","State":"running"}"#;
        let result = docker.parse_container_list(json_input).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "abc123");
        assert_eq!(result[0].names, vec!["test-container"]);
        assert_eq!(result[0].image, "ubuntu:20.04");
        assert_eq!(result[0].status, "Up 5 minutes");
        assert_eq!(result[0].state, "running");
    }

    #[test]
    fn test_parse_container_list_multiple_containers() {
        let docker = CliDocker::new();
        let json_input = r#"{"ID":"abc123","Names":"test-container-1","Image":"ubuntu:20.04","Status":"Up 5 minutes","State":"running"}
{"ID":"def456","Names":"test-container-2","Image":"nginx:latest","Status":"Exited (0) 2 hours ago","State":"exited"}"#;
        let result = docker.parse_container_list(json_input).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, "abc123");
        assert_eq!(result[1].id, "def456");
    }

    #[test]
    fn test_parse_container_inspect_empty() {
        let docker = CliDocker::new();
        let result = docker.parse_container_inspect("").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_container_inspect_single_container() {
        let docker = CliDocker::new();
        let json_input = r#"[{"Id":"abc123def456","Name":"/test-container","Config":{"Image":"ubuntu:20.04"},"State":{"Status":"running"}}]"#;
        let result = docker.parse_container_inspect(json_input).unwrap();

        assert!(result.is_some());
        let container = result.unwrap();
        assert_eq!(container.id, "abc123def456");
        assert_eq!(container.names, vec!["test-container"]); // trimmed /
        assert_eq!(container.image, "ubuntu:20.04");
        assert_eq!(container.status, "running");
        assert_eq!(container.state, "running");
        assert_eq!(container.exposed_ports.len(), 0); // No exposed ports in this test data
        assert_eq!(container.port_mappings.len(), 0); // No port mappings in this test data
    }

    #[test]
    fn test_parse_container_inspect_empty_array() {
        let docker = CliDocker::new();
        let result = docker.parse_container_inspect("[]").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_container_info_serialization() {
        let container = ContainerInfo {
            id: "abc123".to_string(),
            names: vec!["test".to_string()],
            image: "ubuntu:20.04".to_string(),
            status: "running".to_string(),
            state: "running".to_string(),
            exposed_ports: vec![],
            port_mappings: vec![],
        };

        let serialized = serde_json::to_string(&container).unwrap();
        let deserialized: ContainerInfo = serde_json::from_str(&serialized).unwrap();

        assert_eq!(container.id, deserialized.id);
        assert_eq!(container.names, deserialized.names);
        assert_eq!(container.image, deserialized.image);
        assert_eq!(container.status, deserialized.status);
        assert_eq!(container.state, deserialized.state);
        assert_eq!(
            container.exposed_ports.len(),
            deserialized.exposed_ports.len()
        );
        assert_eq!(
            container.port_mappings.len(),
            deserialized.port_mappings.len()
        );
    }

    #[test]
    fn test_parse_container_with_ports() {
        let docker = CliDocker::new();
        let json_input = r#"[{
            "Id":"abc123def456",
            "Name":"/test-container",
            "Config":{
                "Image":"ubuntu:20.04",
                "ExposedPorts":{
                    "3000/tcp":{},
                    "8080/tcp":{}
                }
            },
            "State":{"Status":"running"},
            "NetworkSettings":{
                "Ports":{
                    "3000/tcp":[{"HostIp":"0.0.0.0","HostPort":"3000"}],
                    "8080/tcp":[{"HostIp":"127.0.0.1","HostPort":"8080"}]
                }
            }
        }]"#;
        let result = docker.parse_container_inspect(json_input).unwrap();

        assert!(result.is_some());
        let container = result.unwrap();
        assert_eq!(container.id, "abc123def456");
        assert_eq!(container.names, vec!["test-container"]);
        assert_eq!(container.image, "ubuntu:20.04");
        assert_eq!(container.status, "running");
        assert_eq!(container.state, "running");

        // Check exposed ports
        assert_eq!(container.exposed_ports.len(), 2);
        assert!(container
            .exposed_ports
            .iter()
            .any(|p| p.port == 3000 && p.protocol == "tcp"));
        assert!(container
            .exposed_ports
            .iter()
            .any(|p| p.port == 8080 && p.protocol == "tcp"));

        // Check port mappings
        assert_eq!(container.port_mappings.len(), 2);
        assert!(container.port_mappings.iter().any(|p| p.host_port == 3000
            && p.container_port == 3000
            && p.protocol == "tcp"
            && p.host_ip == "0.0.0.0"));
        assert!(container.port_mappings.iter().any(|p| p.host_port == 8080
            && p.container_port == 8080
            && p.protocol == "tcp"
            && p.host_ip == "127.0.0.1"));
    }

    // Mock Docker tests
    use mock::*;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn test_mock_docker_ping_success() {
        let mock_docker = MockDocker::new();
        let result = mock_docker.ping().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_docker_ping_failure() {
        let config = MockDockerConfig {
            ping_success: false,
            ..Default::default()
        };
        let mock_docker = MockDocker::with_config(config);

        let result = mock_docker.ping().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_docker_daemon_unavailable() {
        let config = MockDockerConfig {
            daemon_unavailable: true,
            ..Default::default()
        };
        let mock_docker = MockDocker::with_config(config);

        let result = mock_docker.ping().await;
        assert!(result.is_err());

        let result = mock_docker.list_containers(None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_docker_list_containers_empty() {
        let mock_docker = MockDocker::new();
        let containers = mock_docker.list_containers(None).await.unwrap();
        assert!(containers.is_empty());
    }

    #[tokio::test]
    async fn test_mock_docker_list_containers_with_mock_data() {
        let mock_docker = MockDocker::new();

        let container = MockContainer::new(
            "test-123".to_string(),
            "test-container".to_string(),
            "ubuntu:20.04".to_string(),
        );
        mock_docker.add_container(container);

        let containers = mock_docker.list_containers(None).await.unwrap();
        assert_eq!(containers.len(), 1);
        assert_eq!(containers[0].id, "test-123");
        assert_eq!(containers[0].names, vec!["test-container"]);
        assert_eq!(containers[0].image, "ubuntu:20.04");
    }

    #[tokio::test]
    async fn test_mock_docker_list_containers_with_label_selector() {
        let mock_docker = MockDocker::new();

        let mut labels = HashMap::new();
        labels.insert("app".to_string(), "web".to_string());
        labels.insert("env".to_string(), "test".to_string());

        let container = MockContainer::new(
            "test-123".to_string(),
            "test-container".to_string(),
            "ubuntu:20.04".to_string(),
        )
        .with_labels(labels);

        mock_docker.add_container(container);

        // Test exact match
        let containers = mock_docker.list_containers(Some("app=web")).await.unwrap();
        assert_eq!(containers.len(), 1);

        // Test key existence
        let containers = mock_docker.list_containers(Some("app")).await.unwrap();
        assert_eq!(containers.len(), 1);

        // Test no match
        let containers = mock_docker.list_containers(Some("app=api")).await.unwrap();
        assert_eq!(containers.len(), 0);

        // Test multiple labels
        let containers = mock_docker
            .list_containers(Some("app=web,env=test"))
            .await
            .unwrap();
        assert_eq!(containers.len(), 1);
    }

    #[tokio::test]
    async fn test_mock_docker_inspect_container() {
        let mock_docker = MockDocker::new();

        let container = MockContainer::new(
            "test-123".to_string(),
            "test-container".to_string(),
            "ubuntu:20.04".to_string(),
        );
        mock_docker.add_container(container);

        // Test by ID
        let result = mock_docker.inspect_container("test-123").await.unwrap();
        assert!(result.is_some());
        let container_info = result.unwrap();
        assert_eq!(container_info.id, "test-123");

        // Test by name
        let result = mock_docker
            .inspect_container("test-container")
            .await
            .unwrap();
        assert!(result.is_some());

        // Test not found
        let result = mock_docker.inspect_container("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_mock_docker_exec_default_response() {
        let mock_docker = MockDocker::new();

        let container = MockContainer::new(
            "test-123".to_string(),
            "test-container".to_string(),
            "ubuntu:20.04".to_string(),
        );
        mock_docker.add_container(container);

        let exec_config = ExecConfig {
            user: Some("root".to_string()),
            working_dir: Some("/workspace".to_string()),
            env: HashMap::new(),
            tty: true,
            interactive: true,
            detach: false,
        };

        let result = mock_docker
            .exec(
                "test-123",
                &["echo".to_string(), "hello".to_string()],
                exec_config,
            )
            .await
            .unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.success);

        // Check exec history
        let history = mock_docker.get_exec_history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].container_id, "test-123");
        assert_eq!(history[0].command, vec!["echo", "hello"]);
        assert!(history[0].config.tty);
        assert!(history[0].config.interactive);
    }

    #[tokio::test]
    async fn test_mock_docker_exec_custom_response() {
        let mock_docker = MockDocker::new();

        let container = MockContainer::new(
            "test-123".to_string(),
            "test-container".to_string(),
            "ubuntu:20.04".to_string(),
        );
        mock_docker.add_container(container);

        // Set custom response for specific command
        let response = MockExecResponse {
            exit_code: 1,
            success: false,
            delay: Some(Duration::from_millis(100)),
            stdout: None,
            stderr: None,
        };
        mock_docker.set_exec_response("failing command".to_string(), response);

        let exec_config = ExecConfig {
            user: None,
            working_dir: None,
            env: HashMap::new(),
            tty: false,
            interactive: false,
            detach: false,
        };

        let start_time = Instant::now();
        let result = mock_docker
            .exec(
                "test-123",
                &["failing".to_string(), "command".to_string()],
                exec_config,
            )
            .await
            .unwrap();
        let elapsed = start_time.elapsed();

        assert_eq!(result.exit_code, 1);
        assert!(!result.success);
        assert!(elapsed >= Duration::from_millis(100)); // Check delay was applied
    }

    #[tokio::test]
    async fn test_mock_docker_container_lifecycle() {
        let mock_docker = MockDocker::new();

        // Test create_container
        let identity = ContainerIdentity {
            workspace_hash: "workspace123".to_string(),
            config_hash: "config456".to_string(),
            name: Some("test-dev".to_string()),
        };

        let config = DevContainerConfig {
            image: Some("ubuntu:20.04".to_string()),
            ..Default::default()
        };

        let container_id = mock_docker
            .create_container(&identity, &config, Path::new("/workspace"))
            .await
            .unwrap();
        assert!(container_id.starts_with("mock-container-"));

        // Test start_container
        let result = mock_docker.start_container(&container_id).await;
        assert!(result.is_ok());

        // Test get_container_image
        let image = mock_docker
            .get_container_image(&container_id)
            .await
            .unwrap();
        assert_eq!(image, "ubuntu:20.04");

        // Test remove_container
        let result = mock_docker.remove_container(&container_id).await;
        assert!(result.is_ok());

        // Verify container is gone
        let result = mock_docker.get_container_image(&container_id).await;
        assert!(result.is_err());
    }
}
