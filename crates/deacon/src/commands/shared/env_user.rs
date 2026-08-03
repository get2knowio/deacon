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
    /// Probed container environment, before merging with config remoteEnv / CLI overrides.
    /// Callers may use this for runtime behavior that depends on user shell startup.
    ///
    /// `PATH` is the one entry that is not verbatim from the probe: it carries the
    /// container's own `PATH` entries merged back in (#370), because a login shell on many
    /// bases assigns `PATH` instead of extending it and would otherwise drop everything the
    /// image contributed. See [`ContainerEnvironmentProber::merge_container_path`].
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
    config_remote_env: Option<&IndexMap<String, Option<String>>>,
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

        // Restore the container's own `PATH` entries that the probe's login shell dropped
        // (#370). Only when BOTH sides have a `PATH` — with nothing to merge from, or
        // nothing to merge into, the probe's answer stands unchanged.
        //
        // `effective_user` is the user the probe ran as and the user `exec` will run as, so
        // it is the right input for the non-root `/sbin` rule. When no user was resolved,
        // deacon passes no `-u` and the image's own `USER` applies — unknown to us here, and
        // treated as non-root, exactly as the reference treats an unresolved user.
        if let (Some(probed_path), Some(container_path)) =
            (probed_env.get("PATH"), container_env.get("PATH"))
        {
            let is_root = matches!(effective_user.as_deref(), Some("root") | Some("0"));
            let merged = ContainerEnvironmentProber::merge_container_path(
                probed_path,
                container_path,
                is_root,
            );
            if &merged != probed_path {
                tracing::debug!(
                    probe = %probed_path,
                    container = %container_path,
                    merged = %merged,
                    "restored container PATH entries dropped by the probe's login shell"
                );
                probed_env.insert("PATH".to_string(), merged);
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
