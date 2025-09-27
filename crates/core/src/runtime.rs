//! Container runtime abstraction for Docker/Podman/etc.
//!
//! This module provides runtime abstraction that allows switching between different
//! container runtimes (Docker, Podman) without changing command logic.

use crate::config::DevContainerConfig;
use crate::container::{ContainerIdentity, ContainerOps, ContainerResult};
use crate::docker::{ContainerInfo, Docker, DockerLifecycle, ExecConfig, ExecResult};
use crate::errors::{DeaconError, Result};
use std::path::Path;

/// Container runtime abstraction that combines Docker and ContainerOps traits
#[allow(async_fn_in_trait)]
pub trait ContainerRuntime: Docker + ContainerOps + DockerLifecycle + Send + Sync {
    /// Get the name/type of this runtime (e.g., "docker", "podman")
    fn runtime_name(&self) -> &'static str;
}

/// Runtime selection options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    /// Docker runtime
    Docker,
    /// Podman runtime (placeholder)
    Podman,
}

impl RuntimeKind {
    /// Parse runtime kind from string
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "docker" => Ok(Self::Docker),
            "podman" => Ok(Self::Podman),
            _ => Err(DeaconError::Runtime(format!(
                "Unknown runtime: {}. Supported runtimes: docker, podman",
                s
            ))
            .into()),
        }
    }

    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }
}

impl std::fmt::Display for RuntimeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Runtime factory for creating container runtime instances
pub struct RuntimeFactory;

impl RuntimeFactory {
    /// Detect runtime from CLI flag, environment variable, or default
    /// 
    /// Precedence: CLI flag > DEACON_RUNTIME env var > default (docker)
    pub fn detect_runtime(cli_runtime: Option<RuntimeKind>) -> RuntimeKind {
        if let Some(runtime) = cli_runtime {
            return runtime;
        }

        if let Ok(env_runtime) = std::env::var("DEACON_RUNTIME") {
            if let Ok(runtime) = RuntimeKind::from_str(&env_runtime) {
                return runtime;
            }
        }

        // Default to Docker
        RuntimeKind::Docker
    }

    /// Create runtime instance based on RuntimeKind
    pub fn create_runtime(kind: RuntimeKind) -> Result<ContainerRuntimeImpl> {
        match kind {
            RuntimeKind::Docker => Ok(ContainerRuntimeImpl::Docker(DockerRuntime::new())),
            RuntimeKind::Podman => Ok(ContainerRuntimeImpl::Podman(PodmanRuntime::new())),
        }
    }
}

/// Concrete container runtime implementation enum
#[derive(Debug)]
pub enum ContainerRuntimeImpl {
    /// Docker runtime
    Docker(DockerRuntime),
    /// Podman runtime 
    Podman(PodmanRuntime),
}

impl ContainerRuntimeImpl {
    /// Get the name/type of this runtime (e.g., "docker", "podman")
    pub fn runtime_name(&self) -> &'static str {
        match self {
            Self::Docker(_) => "docker",
            Self::Podman(_) => "podman",
        }
    }
}

#[allow(async_fn_in_trait)]
impl Docker for ContainerRuntimeImpl {
    async fn ping(&self) -> Result<()> {
        match self {
            Self::Docker(runtime) => runtime.ping().await,
            Self::Podman(runtime) => runtime.ping().await,
        }
    }

    async fn list_containers(&self, label_selector: Option<&str>) -> Result<Vec<ContainerInfo>> {
        match self {
            Self::Docker(runtime) => runtime.list_containers(label_selector).await,
            Self::Podman(runtime) => runtime.list_containers(label_selector).await,
        }
    }

    async fn inspect_container(&self, id: &str) -> Result<Option<ContainerInfo>> {
        match self {
            Self::Docker(runtime) => runtime.inspect_container(id).await,
            Self::Podman(runtime) => runtime.inspect_container(id).await,
        }
    }

    async fn exec(
        &self,
        container_id: &str,
        command: &[String],
        config: ExecConfig,
    ) -> Result<ExecResult> {
        match self {
            Self::Docker(runtime) => runtime.exec(container_id, command, config).await,
            Self::Podman(runtime) => runtime.exec(container_id, command, config).await,
        }
    }

    async fn stop_container(&self, container_id: &str, timeout: Option<u32>) -> Result<()> {
        match self {
            Self::Docker(runtime) => runtime.stop_container(container_id, timeout).await,
            Self::Podman(runtime) => runtime.stop_container(container_id, timeout).await,
        }
    }
}

#[allow(async_fn_in_trait)]
impl ContainerOps for ContainerRuntimeImpl {
    async fn find_matching_containers(&self, identity: &ContainerIdentity) -> Result<Vec<String>> {
        match self {
            Self::Docker(runtime) => runtime.find_matching_containers(identity).await,
            Self::Podman(runtime) => runtime.find_matching_containers(identity).await,
        }
    }

    async fn create_container(
        &self,
        identity: &ContainerIdentity,
        config: &DevContainerConfig,
        workspace_path: &Path,
    ) -> Result<String> {
        match self {
            Self::Docker(runtime) => {
                runtime
                    .create_container(identity, config, workspace_path)
                    .await
            }
            Self::Podman(runtime) => {
                runtime
                    .create_container(identity, config, workspace_path)
                    .await
            }
        }
    }

    async fn start_container(&self, container_id: &str) -> Result<()> {
        match self {
            Self::Docker(runtime) => runtime.start_container(container_id).await,
            Self::Podman(runtime) => runtime.start_container(container_id).await,
        }
    }

    async fn remove_container(&self, container_id: &str) -> Result<()> {
        match self {
            Self::Docker(runtime) => runtime.remove_container(container_id).await,
            Self::Podman(runtime) => runtime.remove_container(container_id).await,
        }
    }

    async fn get_container_image(&self, container_id: &str) -> Result<String> {
        match self {
            Self::Docker(runtime) => runtime.get_container_image(container_id).await,
            Self::Podman(runtime) => runtime.get_container_image(container_id).await,
        }
    }
}

#[allow(async_fn_in_trait)]
impl DockerLifecycle for ContainerRuntimeImpl {
    async fn up(
        &self,
        identity: &ContainerIdentity,
        config: &DevContainerConfig,
        workspace_path: &Path,
        remove_existing: bool,
    ) -> Result<ContainerResult> {
        match self {
            Self::Docker(runtime) => {
                runtime
                    .up(identity, config, workspace_path, remove_existing)
                    .await
            }
            Self::Podman(runtime) => {
                runtime
                    .up(identity, config, workspace_path, remove_existing)
                    .await
            }
        }
    }
}

/// Docker runtime implementation wrapping CliDocker
#[derive(Debug)]
pub struct DockerRuntime {
    docker: crate::docker::CliDocker,
}

impl DockerRuntime {
    /// Create new Docker runtime
    pub fn new() -> Self {
        Self {
            docker: crate::docker::CliDocker::new(),
        }
    }

    /// Create new Docker runtime with custom path
    pub fn with_path(docker_path: String) -> Self {
        Self {
            docker: crate::docker::CliDocker::with_path(docker_path),
        }
    }
}

impl Default for DockerRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(async_fn_in_trait)]
impl Docker for DockerRuntime {
    async fn ping(&self) -> Result<()> {
        self.docker.ping().await
    }

    async fn list_containers(&self, label_selector: Option<&str>) -> Result<Vec<ContainerInfo>> {
        self.docker.list_containers(label_selector).await
    }

    async fn inspect_container(&self, id: &str) -> Result<Option<ContainerInfo>> {
        self.docker.inspect_container(id).await
    }

    async fn exec(
        &self,
        container_id: &str,
        command: &[String],
        config: ExecConfig,
    ) -> Result<ExecResult> {
        self.docker.exec(container_id, command, config).await
    }

    async fn stop_container(&self, container_id: &str, timeout: Option<u32>) -> Result<()> {
        self.docker.stop_container(container_id, timeout).await
    }
}

#[allow(async_fn_in_trait)]
impl ContainerOps for DockerRuntime {
    async fn find_matching_containers(&self, identity: &ContainerIdentity) -> Result<Vec<String>> {
        self.docker.find_matching_containers(identity).await
    }

    async fn create_container(
        &self,
        identity: &ContainerIdentity,
        config: &DevContainerConfig,
        workspace_path: &Path,
    ) -> Result<String> {
        self.docker
            .create_container(identity, config, workspace_path)
            .await
    }

    async fn start_container(&self, container_id: &str) -> Result<()> {
        self.docker.start_container(container_id).await
    }

    async fn remove_container(&self, container_id: &str) -> Result<()> {
        self.docker.remove_container(container_id).await
    }

    async fn get_container_image(&self, container_id: &str) -> Result<String> {
        self.docker.get_container_image(container_id).await
    }
}

#[allow(async_fn_in_trait)]
impl DockerLifecycle for DockerRuntime {
    async fn up(
        &self,
        identity: &ContainerIdentity,
        config: &DevContainerConfig,
        workspace_path: &Path,
        remove_existing: bool,
    ) -> Result<ContainerResult> {
        self.docker
            .up(identity, config, workspace_path, remove_existing)
            .await
    }
}

impl ContainerRuntime for DockerRuntime {
    fn runtime_name(&self) -> &'static str {
        "docker"
    }
}

/// Podman runtime implementation (placeholder stub)
#[derive(Debug)]
pub struct PodmanRuntime;

impl PodmanRuntime {
    /// Create new Podman runtime
    pub fn new() -> Self {
        Self
    }
}

impl Default for PodmanRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(async_fn_in_trait)]
impl Docker for PodmanRuntime {
    async fn ping(&self) -> Result<()> {
        Err(DeaconError::Runtime("Not implemented yet: Podman support".to_string()).into())
    }

    async fn list_containers(&self, _label_selector: Option<&str>) -> Result<Vec<ContainerInfo>> {
        Err(DeaconError::Runtime("Not implemented yet: Podman support".to_string()).into())
    }

    async fn inspect_container(&self, _id: &str) -> Result<Option<ContainerInfo>> {
        Err(DeaconError::Runtime("Not implemented yet: Podman support".to_string()).into())
    }

    async fn exec(
        &self,
        _container_id: &str,
        _command: &[String],
        _config: ExecConfig,
    ) -> Result<ExecResult> {
        Err(DeaconError::Runtime("Not implemented yet: Podman support".to_string()).into())
    }

    async fn stop_container(&self, _container_id: &str, _timeout: Option<u32>) -> Result<()> {
        Err(DeaconError::Runtime("Not implemented yet: Podman support".to_string()).into())
    }
}

#[allow(async_fn_in_trait)]
impl ContainerOps for PodmanRuntime {
    async fn find_matching_containers(&self, _identity: &ContainerIdentity) -> Result<Vec<String>> {
        Err(DeaconError::Runtime("Not implemented yet: Podman support".to_string()).into())
    }

    async fn create_container(
        &self,
        _identity: &ContainerIdentity,
        _config: &DevContainerConfig,
        _workspace_path: &Path,
    ) -> Result<String> {
        Err(DeaconError::Runtime("Not implemented yet: Podman support".to_string()).into())
    }

    async fn start_container(&self, _container_id: &str) -> Result<()> {
        Err(DeaconError::Runtime("Not implemented yet: Podman support".to_string()).into())
    }

    async fn remove_container(&self, _container_id: &str) -> Result<()> {
        Err(DeaconError::Runtime("Not implemented yet: Podman support".to_string()).into())
    }

    async fn get_container_image(&self, _container_id: &str) -> Result<String> {
        Err(DeaconError::Runtime("Not implemented yet: Podman support".to_string()).into())
    }
}

#[allow(async_fn_in_trait)]
impl DockerLifecycle for PodmanRuntime {
    async fn up(
        &self,
        _identity: &ContainerIdentity,
        _config: &DevContainerConfig,
        _workspace_path: &Path,
        _remove_existing: bool,
    ) -> Result<ContainerResult> {
        Err(DeaconError::Runtime("Not implemented yet: Podman support".to_string()).into())
    }
}

impl ContainerRuntime for PodmanRuntime {
    fn runtime_name(&self) -> &'static str {
        "podman"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_kind_from_str() {
        assert_eq!(RuntimeKind::from_str("docker").unwrap(), RuntimeKind::Docker);
        assert_eq!(RuntimeKind::from_str("Docker").unwrap(), RuntimeKind::Docker);
        assert_eq!(RuntimeKind::from_str("DOCKER").unwrap(), RuntimeKind::Docker);
        assert_eq!(RuntimeKind::from_str("podman").unwrap(), RuntimeKind::Podman);
        assert_eq!(RuntimeKind::from_str("Podman").unwrap(), RuntimeKind::Podman);
        assert_eq!(RuntimeKind::from_str("PODMAN").unwrap(), RuntimeKind::Podman);

        assert!(RuntimeKind::from_str("invalid").is_err());
        assert!(RuntimeKind::from_str("containerd").is_err());
    }

    #[test]
    fn test_runtime_kind_as_str() {
        assert_eq!(RuntimeKind::Docker.as_str(), "docker");
        assert_eq!(RuntimeKind::Podman.as_str(), "podman");
    }

    #[test]
    fn test_runtime_kind_display() {
        assert_eq!(RuntimeKind::Docker.to_string(), "docker");
        assert_eq!(RuntimeKind::Podman.to_string(), "podman");
    }

    #[test]
    fn test_detect_runtime_default() {
        // Clear environment variable for test
        std::env::remove_var("DEACON_RUNTIME");
        assert_eq!(RuntimeFactory::detect_runtime(None), RuntimeKind::Docker);
    }

    #[test]
    fn test_detect_runtime_cli_precedence() {
        std::env::set_var("DEACON_RUNTIME", "podman");
        assert_eq!(
            RuntimeFactory::detect_runtime(Some(RuntimeKind::Docker)),
            RuntimeKind::Docker
        );
        std::env::remove_var("DEACON_RUNTIME");
    }

    #[test]
    fn test_detect_runtime_env_var() {
        std::env::set_var("DEACON_RUNTIME", "podman");
        assert_eq!(RuntimeFactory::detect_runtime(None), RuntimeKind::Podman);
        
        std::env::set_var("DEACON_RUNTIME", "docker");
        assert_eq!(RuntimeFactory::detect_runtime(None), RuntimeKind::Docker);
        
        // Invalid env var should fall back to default
        std::env::set_var("DEACON_RUNTIME", "invalid");
        assert_eq!(RuntimeFactory::detect_runtime(None), RuntimeKind::Docker);
        
        std::env::remove_var("DEACON_RUNTIME");
    }

    #[test]
    fn test_create_runtime() {
        let docker_runtime = RuntimeFactory::create_runtime(RuntimeKind::Docker).unwrap();
        assert_eq!(docker_runtime.runtime_name(), "docker");

        let podman_runtime = RuntimeFactory::create_runtime(RuntimeKind::Podman).unwrap();
        assert_eq!(podman_runtime.runtime_name(), "podman");
    }

    #[tokio::test]
    async fn test_podman_runtime_returns_not_implemented() {
        let runtime = ContainerRuntimeImpl::Podman(PodmanRuntime::new());
        
        let result = runtime.ping().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not implemented yet: Podman support"));
    }
}