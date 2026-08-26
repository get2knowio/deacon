//! Shared helpers for command implementations.

use deacon_core::runtime::{ContainerRuntimeImpl, RuntimeFactory, RuntimeKind};

/// Select the container runtime for a consumer command.
///
/// This is the ONE place a runtime is chosen. It honors both knobs, which used to
/// be honored by different subsets of the commands (#692): `--runtime` /
/// `DEACON_CONTAINER_RUNTIME` picks the flavor, `--docker-path` /
/// `DEACON_DOCKER_PATH` picks the binary, and with no explicit flavor the BINARY
/// decides — the reference has no runtime flag and detects podman exactly that
/// way.
///
/// Every command that talks to a container MUST select its runtime here.
/// Hardcoding `CliDocker::new()` ignores BOTH knobs, so the command talks to
/// docker while the container lives in podman → "No such container" / "No
/// running container found"; `RuntimeFactory::create_runtime` honors the flavor
/// but drops the path, so `--docker-path` silently does nothing. `build` did the
/// first and `up` the second.
pub(crate) async fn resolve_runtime(
    cli_runtime: Option<RuntimeKind>,
    docker_path: &str,
) -> ContainerRuntimeImpl {
    let kind = RuntimeFactory::detect_runtime_for_path(cli_runtime, docker_path).await;
    RuntimeFactory::create_runtime_with_path(kind, docker_path)
}

pub(crate) mod build_resolution;
pub mod config_loader;
pub mod container_metadata;
pub(crate) mod container_substitution;
pub(crate) mod disallowed_features;
pub mod env_user;
pub mod feature_resolver;
pub mod host_ca;
pub mod identity;
pub(crate) mod lockfile;
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
