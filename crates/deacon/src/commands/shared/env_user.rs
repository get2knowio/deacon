use deacon_core::IndexMap;
use deacon_core::container_env_probe::{ContainerEnvironmentProber, ContainerProbeMode};
use deacon_core::docker::Docker;
use std::collections::HashMap;
use tracing::warn;

/// Result of resolving effective environment variables and user for in-container execution.
#[derive(Debug, Clone)]
pub struct EnvUserResolution {
    pub effective_env: HashMap<String, String>,
    pub effective_user: Option<String>,
    /// Raw probed container environment (before merging with config remoteEnv / CLI overrides).
    /// Callers may use this for runtime behavior that depends on user shell startup.
    pub probed_env: HashMap<String, String>,
    /// Raw container environment from container inspect (`Config.Env`), before userEnvProbe.
    /// This is the canonical source for `${containerEnv:VAR}` substitutions.
    pub container_env: HashMap<String, String>,
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
    let mut container_env = HashMap::new();

    match docker_client.inspect_container(container_id).await {
        Ok(Some(info)) => {
            container_env = info.env;
        }
        Ok(None) => {
            warn!(
                "Container '{}' not found during env resolution",
                container_id
            );
        }
        Err(error) => {
            warn!(
                "Container inspect failed while reading base container env: {}",
                error
            );
        }
    }

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
    let substitution_source = if container_env.is_empty() {
        &probed_env
    } else {
        &container_env
    };
    let effective_env =
        prober.build_effective_env(&probed_env, substitution_source, config_remote_env, cli_env);

    EnvUserResolution {
        effective_env,
        effective_user,
        probed_env,
        container_env,
    }
}
