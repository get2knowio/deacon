//! Shared helpers for command implementations.

use deacon_core::runtime::{
    ContainerRuntimeImpl, DockerRuntime, PodmanRuntime, RuntimeFactory, RuntimeKind,
};

/// Select the container runtime for a consumer command, honoring
/// `--runtime`/`DEACON_CONTAINER_RUNTIME` (via [`RuntimeFactory::detect_runtime`])
/// and the `--docker-path` override for the docker runtime.
///
/// Every command that talks to a container (`up`, `exec`, `down`,
/// `run-user-commands`, `set-up`) MUST select its runtime this way. Hardcoding
/// `CliDocker::new()` silently ignores a podman selection, so the command talks
/// to docker while the container lives in podman → "No such container" /
/// "No running container found".
pub(crate) fn resolve_runtime(
    cli_runtime: Option<RuntimeKind>,
    docker_path: &str,
) -> ContainerRuntimeImpl {
    match RuntimeFactory::detect_runtime(cli_runtime) {
        RuntimeKind::Podman => ContainerRuntimeImpl::Podman(PodmanRuntime::new()),
        RuntimeKind::Docker => {
            ContainerRuntimeImpl::Docker(DockerRuntime::with_path(docker_path.to_string()))
        }
    }
}

pub(crate) mod build_resolution;
pub mod config_loader;
pub mod env_user;
pub mod feature_resolver;
pub mod host_ca;
pub mod identity;
pub mod profile;
pub mod progress;
pub mod remote_env;
pub mod terminal;
pub mod workspace;

pub use config_loader::{ConfigLoadArgs, ConfigLoadResult, load_config};
pub use env_user::resolve_env_and_user;
pub use identity::canonical_reconnect_identity;
pub use remote_env::NormalizedRemoteEnv;
pub use terminal::TerminalDimensions;
pub use workspace::{derive_container_workspace_folder, resolve_container_cwd};
