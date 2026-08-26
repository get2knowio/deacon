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
    RuntimeFactory::create_runtime_with_path(kind, binary_for(kind, docker_path))
}

/// The binary to run for `kind`, given whatever `--docker-path` holds.
///
/// `--docker-path` defaults to the literal `"docker"`, and that default belongs
/// to the DOCKER flavor — it is not a choice the user made. Handing it to a
/// podman-flavored runtime would run the `docker` binary with podman's argument
/// construction, which is how a first cut of #692 broke
/// `--runtime podman` on a host that has podman: it stopped running podman at all.
///
/// So an explicit path always wins, and an untouched default resolves to the
/// flavor's own name. Distinguishing "explicitly `--docker-path docker`" from
/// "left alone" would need clap's `ValueSource`; it is not worth it, because the
/// two mean the same thing for the docker flavor and the podman flavor is exactly
/// the case this repairs.
fn binary_for(kind: RuntimeKind, docker_path: &str) -> &str {
    const DOCKER_PATH_DEFAULT: &str = "docker";
    match kind {
        RuntimeKind::Podman if docker_path == DOCKER_PATH_DEFAULT => "podman",
        _ => docker_path,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `--docker-path` defaults to the literal `"docker"`, and that default is the
    /// DOCKER flavor's — not a choice the user made. A first cut of #692 handed it
    /// to podman-flavored runtimes, so `--runtime podman` on a host WITH podman
    /// stopped running podman at all and ran the docker binary with podman's
    /// argument construction. CI caught it; this keeps it caught.
    ///
    /// It went unnoticed locally for a revealing reason: podman had just been
    /// installed here, so the broken path still "worked" against docker and the
    /// existing `integration_runtime_selection` test passed. An environment that
    /// differs from CI can mask a regression as easily as it can reveal one.
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
}
