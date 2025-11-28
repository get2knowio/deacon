use deacon_core::container_env_probe::{ContainerEnvironmentProber, ContainerProbeMode};
use deacon_core::docker::Docker;
use deacon_core::IndexMap;
use std::collections::HashMap;
use tracing::warn;

/// Result of resolving effective environment variables and user for in-container execution.
#[derive(Debug, Clone)]
pub struct EnvUserResolution {
    pub effective_env: HashMap<String, String>,
    pub effective_user: Option<String>,
}

/// Resolve the effective environment and user by probing the container (when enabled) and
/// merging configuration + CLI overrides.
///
/// Merge order matches the exec specification and is shared with the up lifecycle path:
/// probed shell environment → config `remoteEnv` → CLI `--remote-env` entries.
#[allow(clippy::too_many_arguments)]
pub async fn resolve_env_and_user<D: Docker>(
    docker_client: &D,
    container_id: &str,
    cli_user: Option<String>,
    config_remote_user: Option<String>,
    probe_mode: ContainerProbeMode,
    config_remote_env: Option<&HashMap<String, Option<String>>>,
    cli_env: &IndexMap<String, String>,
    cache_folder: Option<&std::path::Path>,
) -> EnvUserResolution {
    let effective_user = cli_user.or(config_remote_user);

    let mut probed_env = HashMap::new();
    if probe_mode != ContainerProbeMode::None {
        let prober = ContainerEnvironmentProber::new();
        match prober
            .probe_container_environment(
                docker_client,
                container_id,
                probe_mode,
                effective_user.as_deref(),
                cache_folder,
            )
            .await
        {
            Ok(result) => {
                probed_env = result.env_vars;
            }
            Err(error) => {
                warn!("Container environment probe failed: {}", error);
            }
        }
    }

    let prober = ContainerEnvironmentProber::new();
    let effective_env = prober.build_effective_env(&probed_env, config_remote_env, cli_env);

    EnvUserResolution {
        effective_env,
        effective_user,
    }
}
