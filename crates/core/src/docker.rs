//! Docker and OCI container runtime integration
//!
//! This module handles Docker client abstraction, container lifecycle management,
//! image building, and container execution.

#[cfg(feature = "docker")]
use crate::errors::{DockerError, Result};
#[cfg(feature = "docker")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "docker")]
use std::collections::HashMap;
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
    async fn exec(&self, container_id: &str, command: &[String], config: ExecConfig) -> Result<ExecResult>;
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
        if json_output.trim().is_empty() {
            return Ok(None);
        }

        let containers: Vec<serde_json::Value> = serde_json::from_str(json_output)
            .map_err(|e| DockerError::CLIError(format!("Failed to parse inspect JSON: {}", e)))?;

        if containers.is_empty() {
            return Ok(None);
        }

        let container = &containers[0];
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
            let mut args = vec!["ps", "--all", "--format", "json"];
            let label_filter;

            if let Some(label) = &label_selector {
                label_filter = format!("label={}", label);
                args.push("--filter");
                args.push(&label_filter);
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
            };

            Ok(Some(container_info))
        })
        .await
        .map_err(|e| DockerError::CLIError(format!("Task join error: {}", e)))?
    }

    #[instrument(skip(self))]
    async fn exec(&self, container_id: &str, command: &[String], config: ExecConfig) -> Result<ExecResult> {
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
            let env_args: Vec<String> = config.env.iter()
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
                .map_err(|e| DockerError::CLIError(format!("Failed to spawn docker exec: {}", e)))?;

            let exit_status = child.wait()
                .map_err(|e| DockerError::CLIError(format!("Failed to wait for docker exec: {}", e)))?;

            let exit_code = exit_status.code().unwrap_or(-1);
            let success = exit_status.success();

            Ok(ExecResult { exit_code, success })
        })
        .await
        .map_err(|e| DockerError::CLIError(format!("Task join error: {}", e)))?
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
        };

        let serialized = serde_json::to_string(&container).unwrap();
        let deserialized: ContainerInfo = serde_json::from_str(&serialized).unwrap();

        assert_eq!(container.id, deserialized.id);
        assert_eq!(container.names, deserialized.names);
        assert_eq!(container.image, deserialized.image);
        assert_eq!(container.status, deserialized.status);
        assert_eq!(container.state, deserialized.state);
    }

    #[cfg(not(feature = "docker"))]
    #[test]
    fn test_docker_client_without_feature() {
        let _client = DockerClient::new().unwrap();
        let _default_client = DockerClient::default();
    }
}
