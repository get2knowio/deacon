//! Docker and OCI container runtime integration
//!
//! This module handles Docker client abstraction, container lifecycle management,
//! image building, and container execution.

#[cfg(feature = "docker")]
use crate::config::DevContainerConfig;
#[cfg(feature = "docker")]
use crate::container::{ContainerIdentity, ContainerOps, ContainerResult};
#[cfg(feature = "docker")]
use crate::errors::{DockerError, Result};
#[cfg(feature = "docker")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "docker")]
use std::collections::HashMap;
#[cfg(feature = "docker")]
use std::path::Path;
#[cfg(feature = "docker")]
use std::process::Command;
#[cfg(feature = "docker")]
use tracing::{debug, instrument};

/// Container information returned by Docker operations
#[cfg(feature = "docker")]
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
#[cfg(feature = "docker")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExposedPort {
    /// Port number
    pub port: u16,
    /// Protocol (tcp/udp)
    pub protocol: String,
}

/// Represents a port mapping from host to container
#[cfg(feature = "docker")]
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
#[cfg(feature = "docker")]
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
#[cfg(feature = "docker")]
#[derive(Debug)]
pub struct ExecResult {
    /// Exit code of the command
    pub exit_code: i32,
    /// Whether the command completed successfully (exit code 0)
    pub success: bool,
}

/// Docker client abstraction trait
#[cfg(feature = "docker")]
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
}

/// Docker client abstraction trait extended with container lifecycle operations
#[cfg(feature = "docker")]
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

/// CLI-based Docker implementation using docker command
#[cfg(feature = "docker")]
#[derive(Debug, Default)]
pub struct CliDocker {
    /// Docker CLI binary path
    docker_path: String,
}

#[cfg(feature = "docker")]
impl CliDocker {
    /// Create a new CliDocker instance
    pub fn new() -> Self {
        Self {
            docker_path: "docker".to_string(),
        }
    }

    /// Create a new CliDocker instance with custom docker binary path
    pub fn with_path(docker_path: String) -> Self {
        Self { docker_path }
    }

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

    /// Check if docker binary is available
    #[instrument(skip(self))]
    pub fn check_docker_installed(&self) -> Result<()> {
        debug!(
            "Checking if Docker binary is installed at: {}",
            self.docker_path
        );

        let output = Command::new(&self.docker_path).arg("--version").output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    debug!("Docker binary found and working");
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(
                        DockerError::CLIError(format!("Docker version check failed: {}", stderr))
                            .into(),
                    )
                }
            }
            Err(e) => {
                debug!("Docker binary not found: {}", e);
                Err(DockerError::NotInstalled.into())
            }
        }
    }

    /// Execute docker command and return stdout
    #[instrument(skip(self))]
    #[allow(dead_code)] // Used by future features
    fn execute_docker(&self, args: &[&str]) -> Result<String> {
        debug!(
            "Executing docker command: {} {}",
            self.docker_path,
            args.join(" ")
        );

        let output = Command::new(&self.docker_path)
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
    #[cfg(feature = "docker")]
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

#[cfg(feature = "docker")]
impl Docker for CliDocker {
    #[instrument(skip(self))]
    async fn ping(&self) -> Result<()> {
        debug!("Pinging Docker daemon");

        // Use blocking call as sync is acceptable per issue requirements
        tokio::task::spawn_blocking({
            let docker_path = self.docker_path.clone();
            move || {
                let output = Command::new(&docker_path)
                    .args(["version", "--format", "json"])
                    .output();

                match output {
                    Ok(output) => {
                        if output.status.success() {
                            debug!("Docker daemon is available");
                            Ok(())
                        } else {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            Err(
                                DockerError::CLIError(format!("Docker ping failed: {}", stderr))
                                    .into(),
                            )
                        }
                    }
                    Err(e) => {
                        debug!("Docker ping failed: {}", e);
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

    let docker_path = self.docker_path.clone();
    let label_selector = label_selector.map(|s| s.to_string());

        tokio::task::spawn_blocking(move || {
            let mut args: Vec<String> = vec!["ps", "--all", "--format", "json"]
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

            let output = Command::new(&docker_path)
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

        let docker_path = self.docker_path.clone();
        let container_id = id.to_string();

        tokio::task::spawn_blocking(move || {
            let output = Command::new(&docker_path)
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

        let docker_path = self.docker_path.clone();
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

            debug!("Docker exec args: {:?}", args);

            let mut child = std::process::Command::new(&docker_path)
                .args(&args)
                .spawn()
                .map_err(|e| {
                    DockerError::CLIError(format!("Failed to spawn docker exec: {}", e))
                })?;

            let exit_status = child.wait().map_err(|e| {
                DockerError::CLIError(format!("Failed to wait for docker exec: {}", e))
            })?;

            let exit_code = exit_status.code().unwrap_or(-1);
            let success = exit_status.success();

            Ok(ExecResult { exit_code, success })
        })
        .await
        .map_err(|e| DockerError::CLIError(format!("Task join error: {}", e)))?
    }
}

#[cfg(feature = "docker")]
impl ContainerOps for CliDocker {
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
            // Use default workspace mount
            let workspace_mount = format!(
                "type=bind,source={},target=/workspaces/{}",
                workspace_path.display(),
                workspace_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("workspace")
            );
            args.push("--mount".to_string());
            args.push(workspace_mount);
        }

        // Add additional mounts from configuration
        let mounts = crate::mount::MountParser::parse_mounts_from_json(&config.mounts);
        for mount in mounts {
            args.extend(mount.to_docker_args());
        }

        // Add runArgs if present
        args.extend(config.run_args.iter().cloned());

        // Add image
        let image = config.image.as_ref().ok_or_else(|| {
            DockerError::CLIError("No image specified in configuration".to_string())
        })?;
        args.push(image.clone());

        // Execute docker create command
        let docker_path = self.docker_path.clone();
        let container_id = tokio::task::spawn_blocking(move || {
            let output = Command::new(&docker_path)
                .args(&args)
                .output()
                .map_err(|e| DockerError::CLIError(format!("Failed to create container: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(DockerError::CLIError(format!(
                    "Docker create failed: {}",
                    stderr
                )));
            }

            let stdout = String::from_utf8(output.stdout).map_err(|e| {
                DockerError::CLIError(format!("Invalid UTF-8 in docker output: {}", e))
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

        let docker_path = self.docker_path.clone();
        let container_id = container_id.to_string();

        tokio::task::spawn_blocking(move || -> std::result::Result<(), DockerError> {
            let output = Command::new(&docker_path)
                .args(["start", &container_id])
                .output()
                .map_err(|e| DockerError::CLIError(format!("Failed to start container: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(DockerError::CLIError(format!(
                    "Docker start failed: {}",
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

        let docker_path = self.docker_path.clone();
        let container_id = container_id.to_string();

        tokio::task::spawn_blocking(move || -> std::result::Result<(), DockerError> {
            let output = Command::new(&docker_path)
                .args(["rm", "-f", &container_id])
                .output()
                .map_err(|e| DockerError::CLIError(format!("Failed to remove container: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(DockerError::CLIError(format!(
                    "Docker rm failed: {}",
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

        let docker_path = self.docker_path.clone();
        let container_id = container_id.to_string();

        tokio::task::spawn_blocking(move || -> std::result::Result<String, DockerError> {
            let output = Command::new(&docker_path)
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

#[cfg(feature = "docker")]
impl DockerLifecycle for CliDocker {
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
// Non-docker feature fallback
#[cfg(not(feature = "docker"))]
pub struct DockerClient;

#[cfg(not(feature = "docker"))]
impl DockerClient {
    pub fn new() -> anyhow::Result<Self> {
        Ok(DockerClient)
    }
}

#[cfg(not(feature = "docker"))]
impl Default for DockerClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default Docker client")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "docker")]
    #[test]
    fn test_cli_docker_new() {
        let docker = CliDocker::new();
        assert_eq!(docker.docker_path, "docker");
    }

    #[cfg(feature = "docker")]
    #[test]
    fn test_cli_docker_with_path() {
        let custom_path = "/usr/local/bin/docker";
        let docker = CliDocker::with_path(custom_path.to_string());
        assert_eq!(docker.docker_path, custom_path);
    }

    #[cfg(feature = "docker")]
    #[test]
    fn test_parse_container_list_empty() {
        let docker = CliDocker::new();
        let result = docker.parse_container_list("").unwrap();
        assert!(result.is_empty());
    }

    #[cfg(feature = "docker")]
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

    #[cfg(feature = "docker")]
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

    #[cfg(feature = "docker")]
    #[test]
    fn test_parse_container_inspect_empty() {
        let docker = CliDocker::new();
        let result = docker.parse_container_inspect("").unwrap();
        assert!(result.is_none());
    }

    #[cfg(feature = "docker")]
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

    #[cfg(feature = "docker")]
    #[test]
    fn test_parse_container_inspect_empty_array() {
        let docker = CliDocker::new();
        let result = docker.parse_container_inspect("[]").unwrap();
        assert!(result.is_none());
    }

    #[cfg(feature = "docker")]
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

    #[cfg(feature = "docker")]
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

    #[cfg(not(feature = "docker"))]
    #[test]
    fn test_docker_client_without_feature() {
        let _client = DockerClient::new().unwrap();
        let _default_client = DockerClient::default();
    }
}
