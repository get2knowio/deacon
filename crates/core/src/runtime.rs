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
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tracing::info;

/// `--docker-path`'s default.
///
/// It belongs to the DOCKER flavor and is not a choice the user made, which is what lets
/// [`RuntimeFactory::resolve`] treat it as "nothing chosen". Telling an explicit
/// `--docker-path docker` apart from the default would need clap's `ValueSource`; it is not
/// worth it, because the two mean the same thing everywhere except the podman flavor, which
/// is exactly the case [`binary_for`] repairs.
pub const DEFAULT_DOCKER_PATH: &str = "docker";

/// The binary to run for `kind`, given whatever `--docker-path` holds.
///
/// Handing the docker default to a podman-flavored runtime would run the `docker` binary
/// with podman's argument construction, which is how a first cut of #692 broke
/// `--runtime podman` on a host that has podman: it stopped running podman at all. So an
/// explicit path always wins, and an untouched default resolves to the flavor's own name.
pub fn binary_for(kind: RuntimeKind, docker_path: &str) -> &str {
    match kind {
        RuntimeKind::Podman if docker_path == DEFAULT_DOCKER_PATH => "podman",
        _ => docker_path,
    }
}

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

    /// Choose the runtime flavor AND the binary to run, in one decision.
    ///
    /// This is the selection every container-touching command and `doctor` make. On top of
    /// [`detect_runtime_for_path`](Self::detect_runtime_for_path) it adds one rule the
    /// reference does not have: **when the user chose nothing** (no `--runtime`, no
    /// `DEACON_CONTAINER_RUNTIME`, `--docker-path` left at its default) **and there is no
    /// `docker` on `PATH` but there is a `podman`, run podman.** The reference defaults to
    /// `docker` and fails on such a host; a podman-only machine is common enough (Fedora,
    /// RHEL, a Windows box with `podman machine` and no Docker Desktop) that failing there
    /// helps nobody.
    ///
    /// Deliberately narrow:
    /// - it reacts to the docker BINARY being absent, never to a docker that is present but
    ///   unusable (daemon down, socket denied). Switching runtimes because Docker Desktop is
    ///   not running would hide the problem the user needs to hear about.
    /// - the lookup is a `PATH` walk on the filesystem, not a probe, so the case it applies
    ///   to costs no process spawn at all.
    /// - it is announced at `info` and shown by `doctor`; `--runtime` makes it explicit.
    pub async fn resolve(
        cli_runtime: Option<RuntimeKind>,
        docker_path: &str,
    ) -> (RuntimeKind, String) {
        Self::resolve_with_search_path(
            cli_runtime,
            docker_path,
            std::env::var_os("PATH").as_deref(),
        )
        .await
    }

    /// [`resolve`](Self::resolve) against an explicit search path, so the fallback is
    /// testable without mutating the process environment.
    async fn resolve_with_search_path(
        cli_runtime: Option<RuntimeKind>,
        docker_path: &str,
        search_path: Option<&OsStr>,
    ) -> (RuntimeKind, String) {
        if let Some(podman) = podman_fallback(cli_runtime, docker_path, search_path).await {
            info!(
                "No `docker` on PATH; using podman ({}). Pass --runtime (or set \
                 DEACON_CONTAINER_RUNTIME) to choose the runtime explicitly.",
                podman.display()
            );
            return (RuntimeKind::Podman, "podman".to_string());
        }
        let kind = Self::detect_runtime_for_path(cli_runtime, docker_path).await;
        (kind, binary_for(kind, docker_path).to_string())
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

/// The podman binary [`RuntimeFactory::resolve`] falls back to, if the fallback applies.
///
/// Applies only when the user chose nothing and `docker` is absent from `search_path`
/// while `podman` is present. Separate from `resolve` so every arm is testable without a
/// probe spawning whatever `docker` the test host happens to have.
async fn podman_fallback(
    cli_runtime: Option<RuntimeKind>,
    docker_path: &str,
    search_path: Option<&OsStr>,
) -> Option<PathBuf> {
    if cli_runtime.is_some() || docker_path != DEFAULT_DOCKER_PATH {
        return None;
    }
    if find_executable("docker", search_path).await.is_some() {
        return None;
    }
    find_executable("podman", search_path).await
}

/// Find `name` the way spawning it by bare name would: the first match on `search_path`.
///
/// On Windows only `<name>.exe` is tried, because that is the only extension
/// `std::process::Command` appends; finding a `docker.cmd` the spawn would never run would
/// make the lookup disagree with what actually runs.
async fn find_executable(name: &str, search_path: Option<&OsStr>) -> Option<PathBuf> {
    for dir in std::env::split_paths(search_path?) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        #[cfg(windows)]
        let candidate = dir.join(format!("{name}.exe"));
        #[cfg(not(windows))]
        let candidate = dir.join(name);
        if is_executable_file(&candidate).await {
            return Some(candidate);
        }
    }
    None
}

async fn is_executable_file(path: &Path) -> bool {
    let Ok(meta) = tokio::fs::metadata(path).await else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
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

    /// `--docker-path` defaults to the literal `"docker"`, and that default is the DOCKER
    /// flavor's — not a choice the user made. A first cut of #692 handed it to
    /// podman-flavored runtimes, so `--runtime podman` on a host WITH podman stopped
    /// running podman at all and ran the docker binary with podman's argument
    /// construction. CI caught it; this keeps it caught.
    #[test]
    fn podman_defaults_to_the_podman_binary_but_an_explicit_path_wins() {
        assert_eq!(binary_for(RuntimeKind::Podman, "docker"), "podman");
        assert_eq!(
            binary_for(RuntimeKind::Podman, "/usr/local/bin/podman4"),
            "/usr/local/bin/podman4"
        );
        // An explicit non-default path is honored even when it names docker.
        assert_eq!(
            binary_for(RuntimeKind::Podman, "/opt/docker"),
            "/opt/docker"
        );

        // The docker flavor keeps whatever it was given, default included.
        assert_eq!(binary_for(RuntimeKind::Docker, "docker"), "docker");
        assert_eq!(
            binary_for(RuntimeKind::Docker, "/opt/mydocker"),
            "/opt/mydocker"
        );
    }

    /// Put an executable named `name` (as a bare-name spawn would look for it) in `dir`.
    fn place_executable(dir: &Path, name: &str) -> PathBuf {
        #[cfg(windows)]
        let path = dir.join(format!("{name}.exe"));
        #[cfg(not(windows))]
        let path = dir.join(name);
        std::fs::write(&path, b"").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    fn search_path(dirs: &[&Path]) -> std::ffi::OsString {
        std::env::join_paths(dirs).unwrap()
    }

    /// The lookup returns the FIRST match in PATH order, as a spawn would run it, and
    /// passes over directories that do not have it.
    #[tokio::test]
    async fn find_executable_takes_the_first_match_in_path_order() {
        let empty = tempfile::tempdir().unwrap();
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let expected = place_executable(first.path(), "podman");
        place_executable(second.path(), "podman");

        let path = search_path(&[empty.path(), first.path(), second.path()]);
        assert_eq!(find_executable("podman", Some(&path)).await, Some(expected));
        assert_eq!(find_executable("docker", Some(&path)).await, None);
        assert_eq!(find_executable("podman", None).await, None);
    }

    /// A same-named DIRECTORY or a non-executable file is not something a spawn could
    /// run, so it must not count as `docker` being present.
    #[cfg(unix)]
    #[tokio::test]
    async fn find_executable_ignores_what_a_spawn_could_not_run() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("docker")).unwrap();
        std::fs::write(dir.path().join("podman"), b"").unwrap(); // mode 0644

        let path = search_path(&[dir.path()]);
        assert_eq!(find_executable("docker", Some(&path)).await, None);
        assert_eq!(find_executable("podman", Some(&path)).await, None);
    }

    /// The fallback fires on exactly one shape: nothing chosen, no docker, podman present.
    #[tokio::test]
    async fn podman_fallback_applies_only_when_nothing_was_chosen_and_docker_is_absent() {
        let podman_only = tempfile::tempdir().unwrap();
        let podman = place_executable(podman_only.path(), "podman");
        let both = tempfile::tempdir().unwrap();
        place_executable(both.path(), "docker");
        place_executable(both.path(), "podman");
        let neither = tempfile::tempdir().unwrap();

        let podman_only = search_path(&[podman_only.path()]);
        let both = search_path(&[both.path()]);
        let neither = search_path(&[neither.path()]);

        // The one shape it is for.
        assert_eq!(
            podman_fallback(None, DEFAULT_DOCKER_PATH, Some(&podman_only)).await,
            Some(podman)
        );

        // docker present: the existing selection stands, shim detection included.
        assert_eq!(
            podman_fallback(None, DEFAULT_DOCKER_PATH, Some(&both)).await,
            None
        );
        // Neither present: nothing to fall back to; the command reports docker missing.
        assert_eq!(
            podman_fallback(None, DEFAULT_DOCKER_PATH, Some(&neither)).await,
            None
        );
        // An explicit flavor, either one, is never second-guessed.
        assert_eq!(
            podman_fallback(
                Some(RuntimeKind::Docker),
                DEFAULT_DOCKER_PATH,
                Some(&podman_only)
            )
            .await,
            None
        );
        assert_eq!(
            podman_fallback(
                Some(RuntimeKind::Podman),
                DEFAULT_DOCKER_PATH,
                Some(&podman_only)
            )
            .await,
            None
        );
        // An explicit --docker-path is a choice too, even one that does not exist.
        assert_eq!(
            podman_fallback(None, "/opt/docker", Some(&podman_only)).await,
            None
        );
    }

    /// When the fallback applies, `resolve` hands back the podman FLAVOR and binary
    /// together — a podman binary under the docker flavor's argument construction is the
    /// #692 regression in another shape.
    #[tokio::test]
    async fn resolve_selects_podman_flavor_and_binary_together_on_a_podman_only_path() {
        let dir = tempfile::tempdir().unwrap();
        place_executable(dir.path(), "podman");
        let path = search_path(&[dir.path()]);

        assert_eq!(
            RuntimeFactory::resolve_with_search_path(None, DEFAULT_DOCKER_PATH, Some(&path)).await,
            (RuntimeKind::Podman, "podman".to_string())
        );
        // An explicit flavor goes straight through, with no probe and no lookup.
        assert_eq!(
            RuntimeFactory::resolve_with_search_path(
                Some(RuntimeKind::Docker),
                DEFAULT_DOCKER_PATH,
                Some(&path)
            )
            .await,
            (RuntimeKind::Docker, "docker".to_string())
        );
    }

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
