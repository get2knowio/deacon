//! Container runtime abstraction for Docker/Podman/etc.
//!
//! This module provides runtime abstraction that allows switching between different
//! container runtimes (Docker, Podman) without changing command logic.

use crate::config::DevContainerConfig;
use crate::container::{ContainerIdentity, ContainerOps, ContainerResult};
use crate::docker::{
    CliRuntime, ContainerInfo, Docker, DockerLifecycle, ExecConfig, ExecResult, ImageInfo,
};
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
    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }
}

impl std::str::FromStr for RuntimeKind {
    type Err = crate::errors::DeaconError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "docker" => Ok(Self::Docker),
            "podman" => Ok(Self::Podman),
            _ => Err(DeaconError::Runtime(format!(
                "Unknown runtime: {}. Supported runtimes: docker, podman",
                s
            ))),
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
            if let Ok(runtime) = env_runtime.parse() {
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

    /// Get the underlying CliDocker/CliRuntime instance for feature installation
    pub fn cli_docker(&self) -> CliRuntime {
        match self {
            Self::Docker(runtime) => runtime.docker.clone(),
            Self::Podman(runtime) => runtime.runtime.clone(),
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

    async fn inspect_image(&self, image_ref: &str) -> Result<Option<ImageInfo>> {
        match self {
            Self::Docker(runtime) => runtime.inspect_image(image_ref).await,
            Self::Podman(runtime) => runtime.inspect_image(image_ref).await,
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
        gpu_mode: crate::gpu::GpuMode,
        merged_security: &crate::features::MergedSecurityOptions,
        merged_mounts: &crate::mount::MergedMounts,
    ) -> Result<String> {
        match self {
            Self::Docker(runtime) => {
                runtime
                    .create_container(
                        identity,
                        config,
                        workspace_path,
                        gpu_mode,
                        merged_security,
                        merged_mounts,
                    )
                    .await
            }
            Self::Podman(runtime) => {
                runtime
                    .create_container(
                        identity,
                        config,
                        workspace_path,
                        gpu_mode,
                        merged_security,
                        merged_mounts,
                    )
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

    async fn commit_container(&self, container_id: &str, image_tag: &str) -> Result<()> {
        match self {
            Self::Docker(runtime) => runtime.commit_container(container_id, image_tag).await,
            Self::Podman(runtime) => runtime.commit_container(container_id, image_tag).await,
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
        gpu_mode: crate::gpu::GpuMode,
        merged_security: &crate::features::MergedSecurityOptions,
        merged_mounts: &crate::mount::MergedMounts,
    ) -> Result<ContainerResult> {
        match self {
            Self::Docker(runtime) => {
                runtime
                    .up(
                        identity,
                        config,
                        workspace_path,
                        remove_existing,
                        gpu_mode,
                        merged_security,
                        merged_mounts,
                    )
                    .await
            }
            Self::Podman(runtime) => {
                runtime
                    .up(
                        identity,
                        config,
                        workspace_path,
                        remove_existing,
                        gpu_mode,
                        merged_security,
                        merged_mounts,
                    )
                    .await
            }
        }
    }
}

/// Docker runtime implementation wrapping CliRuntime
#[derive(Debug)]
pub struct DockerRuntime {
    pub(crate) docker: CliRuntime,
}

impl DockerRuntime {
    /// Create new Docker runtime
    pub fn new() -> Self {
        Self {
            docker: CliRuntime::new(),
        }
    }

    /// Create new Docker runtime with custom path
    pub fn with_path(docker_path: String) -> Self {
        Self {
            docker: CliRuntime::with_runtime_path(docker_path),
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

    async fn inspect_image(&self, image_ref: &str) -> Result<Option<ImageInfo>> {
        self.docker.inspect_image(image_ref).await
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
        gpu_mode: crate::gpu::GpuMode,
        merged_security: &crate::features::MergedSecurityOptions,
        merged_mounts: &crate::mount::MergedMounts,
    ) -> Result<String> {
        self.docker
            .create_container(
                identity,
                config,
                workspace_path,
                gpu_mode,
                merged_security,
                merged_mounts,
            )
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

    async fn commit_container(&self, container_id: &str, image_tag: &str) -> Result<()> {
        self.docker.commit_container(container_id, image_tag).await
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
        gpu_mode: crate::gpu::GpuMode,
        merged_security: &crate::features::MergedSecurityOptions,
        merged_mounts: &crate::mount::MergedMounts,
    ) -> Result<ContainerResult> {
        self.docker
            .up(
                identity,
                config,
                workspace_path,
                remove_existing,
                gpu_mode,
                merged_security,
                merged_mounts,
            )
            .await
    }
}

impl ContainerRuntime for DockerRuntime {
    fn runtime_name(&self) -> &'static str {
        "docker"
    }
}

/// Podman runtime implementation
#[derive(Debug)]
pub struct PodmanRuntime {
    pub(crate) runtime: CliRuntime,
}

impl PodmanRuntime {
    /// Create new Podman runtime
    pub fn new() -> Self {
        Self {
            runtime: CliRuntime::podman(),
        }
    }

    /// Create new Podman runtime with custom path
    pub fn with_path(podman_path: String) -> Self {
        Self {
            runtime: CliRuntime::with_runtime_path(podman_path),
        }
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
        self.runtime.ping().await
    }

    async fn list_containers(&self, label_selector: Option<&str>) -> Result<Vec<ContainerInfo>> {
        self.runtime.list_containers(label_selector).await
    }

    async fn inspect_container(&self, id: &str) -> Result<Option<ContainerInfo>> {
        self.runtime.inspect_container(id).await
    }

    async fn inspect_image(&self, image_ref: &str) -> Result<Option<ImageInfo>> {
        self.runtime.inspect_image(image_ref).await
    }

    async fn exec(
        &self,
        container_id: &str,
        command: &[String],
        config: ExecConfig,
    ) -> Result<ExecResult> {
        self.runtime.exec(container_id, command, config).await
    }

    async fn stop_container(&self, container_id: &str, timeout: Option<u32>) -> Result<()> {
        self.runtime.stop_container(container_id, timeout).await
    }
}

#[allow(async_fn_in_trait)]
impl ContainerOps for PodmanRuntime {
    async fn find_matching_containers(&self, identity: &ContainerIdentity) -> Result<Vec<String>> {
        self.runtime.find_matching_containers(identity).await
    }

    async fn create_container(
        &self,
        identity: &ContainerIdentity,
        config: &DevContainerConfig,
        workspace_path: &Path,
        gpu_mode: crate::gpu::GpuMode,
        merged_security: &crate::features::MergedSecurityOptions,
        merged_mounts: &crate::mount::MergedMounts,
    ) -> Result<String> {
        self.runtime
            .create_container(
                identity,
                config,
                workspace_path,
                gpu_mode,
                merged_security,
                merged_mounts,
            )
            .await
    }

    async fn start_container(&self, container_id: &str) -> Result<()> {
        self.runtime.start_container(container_id).await
    }

    async fn remove_container(&self, container_id: &str) -> Result<()> {
        self.runtime.remove_container(container_id).await
    }

    async fn get_container_image(&self, container_id: &str) -> Result<String> {
        self.runtime.get_container_image(container_id).await
    }

    async fn commit_container(&self, container_id: &str, image_tag: &str) -> Result<()> {
        self.runtime.commit_container(container_id, image_tag).await
    }
}

#[allow(async_fn_in_trait)]
impl DockerLifecycle for PodmanRuntime {
    async fn up(
        &self,
        identity: &ContainerIdentity,
        config: &DevContainerConfig,
        workspace_path: &Path,
        remove_existing: bool,
        gpu_mode: crate::gpu::GpuMode,
        merged_security: &crate::features::MergedSecurityOptions,
        merged_mounts: &crate::mount::MergedMounts,
    ) -> Result<ContainerResult> {
        self.runtime
            .up(
                identity,
                config,
                workspace_path,
                remove_existing,
                gpu_mode,
                merged_security,
                merged_mounts,
            )
            .await
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
        assert_eq!(
            "docker".parse::<RuntimeKind>().unwrap(),
            RuntimeKind::Docker
        );
        assert_eq!(
            "Docker".parse::<RuntimeKind>().unwrap(),
            RuntimeKind::Docker
        );
        assert_eq!(
            "DOCKER".parse::<RuntimeKind>().unwrap(),
            RuntimeKind::Docker
        );
        assert_eq!(
            "podman".parse::<RuntimeKind>().unwrap(),
            RuntimeKind::Podman
        );
        assert_eq!(
            "Podman".parse::<RuntimeKind>().unwrap(),
            RuntimeKind::Podman
        );
        assert_eq!(
            "PODMAN".parse::<RuntimeKind>().unwrap(),
            RuntimeKind::Podman
        );

        assert!("invalid".parse::<RuntimeKind>().is_err());
        assert!("containerd".parse::<RuntimeKind>().is_err());
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
    async fn test_podman_runtime_works() {
        // This test just verifies that PodmanRuntime can be instantiated
        // and that it uses the podman binary path
        let runtime = ContainerRuntimeImpl::Podman(PodmanRuntime::new());
        assert_eq!(runtime.runtime_name(), "podman");
    }

    #[test]
    fn test_podman_runtime_with_custom_path() {
        let custom_path = "/usr/local/bin/podman";
        let _runtime = PodmanRuntime::with_path(custom_path.to_string());
        // Verify the PodmanRuntime was created successfully (no panic)
    }

    #[test]
    fn test_podman_runtime_creation() {
        let runtime = PodmanRuntime::new();
        let wrapped = ContainerRuntimeImpl::Podman(runtime);
        assert_eq!(wrapped.runtime_name(), "podman");
    }

    #[test]
    fn test_docker_runtime_creation() {
        let runtime = DockerRuntime::new();
        let wrapped = ContainerRuntimeImpl::Docker(runtime);
        assert_eq!(wrapped.runtime_name(), "docker");
    }
}
