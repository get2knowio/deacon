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
    /// Resolve the runtime from the (already env-resolved) CLI flag, or default.
    ///
    /// Precedence: CLI flag > default (docker). The `DEACON_CONTAINER_RUNTIME`
    /// environment variable is resolved at the CLI layer by clap's `env=`
    /// attribute on `--runtime`, so by the time a value reaches here it already
    /// reflects flag-over-env-over-default. See `deacon::cli::Cli::runtime`.
    pub fn detect_runtime(cli_runtime: Option<RuntimeKind>) -> RuntimeKind {
        cli_runtime.unwrap_or(RuntimeKind::Docker)
    }

    /// Resolve the runtime for a specific CLI binary.
    ///
    /// Precedence, and clap owns the first two: explicit `--runtime` (or
    /// `DEACON_CONTAINER_RUNTIME`) wins outright; otherwise the BINARY decides.
    ///
    /// Asking the binary is what the reference does — it has no runtime flag at
    /// all and detects podman by running `<dockerPath> -v` and looking for
    /// `podman` in the output (`isPodman`, `src/spec-shutdown/dockerUtils.ts` at
    /// v0.87.0). Two shapes need it and neither is exotic: `--docker-path podman`,
    /// which is how the reference's own podman suite selects podman; and the
    /// `podman-docker` package, which installs a `docker` shim, so even the
    /// DEFAULT path can be podman (#692).
    ///
    /// A probe that fails — missing binary, non-zero exit, unreadable output —
    /// yields `Docker`. That is the reference's behavior too (`isPodman` catches
    /// and returns false), and it is the right fallback here: an absent binary
    /// must surface as the command's own clear error when it runs the thing, not
    /// as a confusing misdetection at selection time.
    pub async fn detect_runtime_for_path(
        cli_runtime: Option<RuntimeKind>,
        runtime_path: &str,
    ) -> RuntimeKind {
        if let Some(kind) = cli_runtime {
            return kind;
        }
        if Self::binary_reports_podman(runtime_path).await {
            RuntimeKind::Podman
        } else {
            RuntimeKind::Docker
        }
    }

    async fn binary_reports_podman(runtime_path: &str) -> bool {
        match tokio::process::Command::new(runtime_path)
            .arg("-v")
            .output()
            .await
        {
            Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
                .to_ascii_lowercase()
                .contains("podman"),
            _ => false,
        }
    }

    /// Create a runtime instance for `kind`, running `runtime_path` as its CLI.
    pub fn create_runtime_with_path(kind: RuntimeKind, runtime_path: &str) -> ContainerRuntimeImpl {
        match kind {
            RuntimeKind::Docker => {
                ContainerRuntimeImpl::Docker(DockerRuntime::with_path(runtime_path.to_string()))
            }
            RuntimeKind::Podman => {
                ContainerRuntimeImpl::Podman(PodmanRuntime::with_path(runtime_path.to_string()))
            }
        }
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

    async fn ensure_image_available(&self, image_ref: &str) -> Result<Option<ImageInfo>> {
        match self {
            Self::Docker(runtime) => runtime.ensure_image_available(image_ref).await,
            Self::Podman(runtime) => runtime.ensure_image_available(image_ref).await,
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

    async fn exec_with_line_prefix(
        &self,
        container_id: &str,
        command: &[String],
        config: ExecConfig,
        line_prefix: &str,
    ) -> Result<ExecResult> {
        match self {
            Self::Docker(runtime) => {
                runtime
                    .exec_with_line_prefix(container_id, command, config, line_prefix)
                    .await
            }
            Self::Podman(runtime) => {
                runtime
                    .exec_with_line_prefix(container_id, command, config, line_prefix)
                    .await
            }
        }
    }

    async fn exec_with_stdin(
        &self,
        container_id: &str,
        command: &[String],
        stdin: &[u8],
        config: &ExecConfig,
    ) -> Result<ExecResult> {
        match self {
            Self::Docker(runtime) => {
                runtime
                    .exec_with_stdin(container_id, command, stdin, config)
                    .await
            }
            Self::Podman(runtime) => {
                runtime
                    .exec_with_stdin(container_id, command, stdin, config)
                    .await
            }
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
        entrypoint_chain: &crate::features::EntrypointChain,
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
                        entrypoint_chain,
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
                        entrypoint_chain,
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
        entrypoint_chain: &crate::features::EntrypointChain,
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
                        entrypoint_chain,
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
                        entrypoint_chain,
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

    async fn ensure_image_available(&self, image_ref: &str) -> Result<Option<ImageInfo>> {
        self.docker.ensure_image_available(image_ref).await
    }

    async fn exec(
        &self,
        container_id: &str,
        command: &[String],
        config: ExecConfig,
    ) -> Result<ExecResult> {
        self.docker.exec(container_id, command, config).await
    }

    async fn exec_with_line_prefix(
        &self,
        container_id: &str,
        command: &[String],
        config: ExecConfig,
        line_prefix: &str,
    ) -> Result<ExecResult> {
        self.docker
            .exec_with_line_prefix(container_id, command, config, line_prefix)
            .await
    }

    async fn exec_with_stdin(
        &self,
        container_id: &str,
        command: &[String],
        stdin: &[u8],
        config: &ExecConfig,
    ) -> Result<ExecResult> {
        self.docker
            .exec_with_stdin(container_id, command, stdin, config)
            .await
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
        entrypoint_chain: &crate::features::EntrypointChain,
    ) -> Result<String> {
        self.docker
            .create_container(
                identity,
                config,
                workspace_path,
                gpu_mode,
                merged_security,
                merged_mounts,
                entrypoint_chain,
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
        entrypoint_chain: &crate::features::EntrypointChain,
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
                entrypoint_chain,
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
            runtime: CliRuntime::with_runtime_path_and_flavor(
                podman_path,
                crate::docker::RuntimeFlavor::Podman,
            ),
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

    async fn ensure_image_available(&self, image_ref: &str) -> Result<Option<ImageInfo>> {
        self.runtime.ensure_image_available(image_ref).await
    }

    async fn exec(
        &self,
        container_id: &str,
        command: &[String],
        config: ExecConfig,
    ) -> Result<ExecResult> {
        self.runtime.exec(container_id, command, config).await
    }

    async fn exec_with_line_prefix(
        &self,
        container_id: &str,
        command: &[String],
        config: ExecConfig,
        line_prefix: &str,
    ) -> Result<ExecResult> {
        self.runtime
            .exec_with_line_prefix(container_id, command, config, line_prefix)
            .await
    }

    async fn exec_with_stdin(
        &self,
        container_id: &str,
        command: &[String],
        stdin: &[u8],
        config: &ExecConfig,
    ) -> Result<ExecResult> {
        self.runtime
            .exec_with_stdin(container_id, command, stdin, config)
            .await
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
        entrypoint_chain: &crate::features::EntrypointChain,
    ) -> Result<String> {
        self.runtime
            .create_container(
                identity,
                config,
                workspace_path,
                gpu_mode,
                merged_security,
                merged_mounts,
                entrypoint_chain,
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
        entrypoint_chain: &crate::features::EntrypointChain,
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
                entrypoint_chain,
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
        // `None` (no CLI flag / env resolved) falls back to Docker. The
        // `DEACON_CONTAINER_RUNTIME` env var is now resolved at the CLI layer by
        // clap's `env=` on `--runtime`, so `detect_runtime` no longer reads the
        // environment itself — the flag-vs-env precedence is covered by the clap
        // parse tests in `deacon::cli`.
        assert_eq!(RuntimeFactory::detect_runtime(None), RuntimeKind::Docker);
    }

    #[test]
    fn test_detect_runtime_cli_precedence() {
        assert_eq!(
            RuntimeFactory::detect_runtime(Some(RuntimeKind::Docker)),
            RuntimeKind::Docker
        );
        assert_eq!(
            RuntimeFactory::detect_runtime(Some(RuntimeKind::Podman)),
            RuntimeKind::Podman
        );
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

    /// #692: with no explicit `--runtime`, the BINARY decides — which is how the
    /// reference selects podman (it has no runtime flag at all) and how the
    /// `podman-docker` shim gets noticed even at the default `docker` path.
    ///
    /// Unix-only because it needs an executable shim; the logic it covers is
    /// platform-agnostic.
    #[cfg(unix)]
    #[tokio::test]
    async fn runtime_flavor_is_detected_from_the_binary_when_not_given() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("deacon-rt-detect-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let make = |name: &str, body: &str| {
            let path = dir.join(name);
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "#!/bin/sh\n{body}").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path.to_string_lossy().to_string()
        };

        let podmanish = make("podmanish", "echo 'podman version 4.9.3'");
        let dockerish = make("dockerish", "echo 'Docker version 27.0.0, build abc'");
        let failing = make("failing", "exit 1");

        // The binary is asked, and its answer is taken.
        assert_eq!(
            RuntimeFactory::detect_runtime_for_path(None, &podmanish).await,
            RuntimeKind::Podman
        );
        assert_eq!(
            RuntimeFactory::detect_runtime_for_path(None, &dockerish).await,
            RuntimeKind::Docker
        );

        // An explicit selection wins over whatever the binary says — clap owns
        // flag > env, and neither may be overridden by a probe.
        assert_eq!(
            RuntimeFactory::detect_runtime_for_path(Some(RuntimeKind::Docker), &podmanish).await,
            RuntimeKind::Docker
        );
        assert_eq!(
            RuntimeFactory::detect_runtime_for_path(Some(RuntimeKind::Podman), &dockerish).await,
            RuntimeKind::Podman
        );

        // A probe that cannot answer falls back to docker rather than guessing:
        // a missing or broken binary must surface as the command's own error when
        // it runs the thing, not as a misdetection here.
        assert_eq!(
            RuntimeFactory::detect_runtime_for_path(None, &failing).await,
            RuntimeKind::Docker
        );
        assert_eq!(
            RuntimeFactory::detect_runtime_for_path(None, "/nonexistent/definitely-not-a-runtime")
                .await,
            RuntimeKind::Docker
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The path reaches the constructed runtime — dropping it is what made
    /// `--docker-path` a no-op on `up` (#692).
    #[test]
    fn create_runtime_with_path_carries_the_binary() {
        let docker = RuntimeFactory::create_runtime_with_path(RuntimeKind::Docker, "/opt/mydocker");
        assert_eq!(docker.cli_docker().runtime_path(), "/opt/mydocker");
        assert_eq!(docker.runtime_name(), "docker");

        let podman = RuntimeFactory::create_runtime_with_path(RuntimeKind::Podman, "/opt/mypodman");
        assert_eq!(podman.cli_docker().runtime_path(), "/opt/mypodman");
        assert_eq!(podman.runtime_name(), "podman");
        assert!(podman.cli_docker().is_podman());
    }
}
