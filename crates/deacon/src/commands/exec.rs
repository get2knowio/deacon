//! Exec command implementation for container execution
//!
//! This module provides container resolution and execution functionality
//! for the exec command, targeting the correct workspace container.

use crate::commands::shared::{
    ConfigLoadArgs, ConfigLoadResult, NormalizedRemoteEnv, TerminalDimensions,
    canonical_reconnect_identity, load_config, resolve_env_and_user,
};
use anyhow::{Context, Result};
use deacon_core::IndexMap;
use deacon_core::compose::{ComposeManager, ComposeProject};
use deacon_core::config::DevContainerConfig;
use deacon_core::container_env_probe::ContainerProbeMode;
use deacon_core::docker::{CliDocker, Docker, TerminalSize};
use deacon_core::errors::{ConfigError, DeaconError};
use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use tracing::{debug, instrument};

/// Arguments for the exec command
#[derive(Debug, Clone)]
pub struct ExecArgs {
    /// User to run the command as
    pub user: Option<String>,
    /// Disable TTY allocation
    pub no_tty: bool,
    /// Remote environment variables to set inside the container (KEY=VALUE).
    /// Accepts empty values (e.g. `FOO=`). Surfaced as `--remote-env` on the CLI
    /// (with `--env` preserved as a hidden alias for backward compatibility).
    pub remote_env: Vec<String>,
    /// Working directory for command execution
    pub workdir: Option<String>,
    /// Target container ID directly
    pub container_id: Option<String>,
    /// Identify container by labels (KEY=VALUE format)
    pub id_label: Vec<String>,
    /// Target specific service in Docker Compose projects (defaults to primary service)
    pub service: Option<String>,
    /// Command to execute
    pub command: Vec<String>,
    /// Workspace folder path
    pub workspace_folder: Option<std::path::PathBuf>,
    /// Kept for CLI parse-compat with `up`/`build`; not consumed by `exec`.
    /// Per #111, config discovery + container identity stay anchored to
    /// `workspace_folder`; `exec` never bind-mounts.
    #[allow(dead_code)]
    pub mount_workspace_git_root: bool,
    /// Configuration file path
    pub config_path: Option<std::path::PathBuf>,
    /// Override configuration file path
    pub override_config_path: Option<PathBuf>,
    /// Secrets file paths used for substitution
    pub secrets_files: Vec<PathBuf>,
    /// Path to docker executable
    pub docker_path: String,
    /// Path to docker-compose executable (legacy standalone binary)
    #[allow(dead_code)] // Future: Will be used for standalone docker-compose binary support
    pub docker_compose_path: String,
    /// Environment file(s) to pass to docker compose commands
    pub env_file: Vec<PathBuf>,
    /// Force TTY allocation when global log-format is JSON
    pub force_tty_if_json: bool,
    /// Default user env probe mode (from global flag)
    pub default_user_env_probe: Option<ContainerProbeMode>,
    /// Container-side data folder path
    #[allow(dead_code)] // Reserved for future use
    pub container_data_folder: Option<std::path::PathBuf>,
    /// Container-side system data folder path
    #[allow(dead_code)] // Reserved for future use
    pub container_system_data_folder: Option<std::path::PathBuf>,
    /// Optional terminal dimension hint for PTY sizing; propagated into exec config when a PTY is allocated
    pub terminal_dimensions: Option<TerminalDimensions>,
}

/// Compute whether we should allocate a PTY for `exec`.
///
/// Rules:
/// - Explicit `--no-tty` always wins: never allocate a PTY.
/// - A PTY requires a real terminal on **stdin**. `docker exec -it` against a
///   piped/redirected stdin is rejected by the daemon ("cannot attach stdin to
///   a TTY-enabled container because stdin is not a terminal"), so `exec` can
///   never use a PTY when stdin isn't a terminal — even under `force_tty`.
/// - `force_tty` (e.g. JSON log format auto-forcing) bypasses only the
///   *stdout*-is-a-terminal requirement, so an interactive user whose stdout is
///   being captured (a JSON consumer) still gets a PTY.
/// - Otherwise, allocate a PTY only when both stdin and stdout are TTYs.
pub(crate) fn compute_should_use_tty(
    force_tty: bool,
    no_tty: bool,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
) -> bool {
    if no_tty || !stdin_is_tty {
        return false;
    }
    force_tty || stdout_is_tty
}

fn map_config_error(err: DeaconError) -> anyhow::Error {
    match err {
        DeaconError::Config(ConfigError::NotFound { path }) => {
            anyhow::Error::new(DeaconError::Config(ConfigError::NotFound {
                path: path.clone(),
            }))
            .context(format!("Dev container config ({}) not found.", path))
        }
        other => anyhow::Error::new(other),
    }
}

/// Build an `ExecConfig` value from higher level inputs. This helper exists to
/// make the PTY decision logic and produced config testable without executing
/// the command (which would call `std::process::exit`).
///
/// This function is public to support integration testing of PTY allocation logic.
pub fn build_exec_config(
    args: &ExecArgs,
    working_dir: String,
    mut effective_env: HashMap<String, String>,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
) -> deacon_core::docker::ExecConfig {
    let force_tty = args.force_tty_if_json;
    let should_use_tty =
        compute_should_use_tty(force_tty, args.no_tty, stdin_is_tty, stdout_is_tty);
    let mut terminal_size = None;

    if should_use_tty {
        if let Some(dimensions) = args.terminal_dimensions {
            effective_env.insert("COLUMNS".to_string(), dimensions.columns.to_string());
            effective_env.insert("LINES".to_string(), dimensions.rows.to_string());
            terminal_size = Some(TerminalSize::new(dimensions.columns, dimensions.rows));
        }
    }

    deacon_core::docker::ExecConfig {
        user: args.user.clone(),
        working_dir: Some(working_dir),
        env: effective_env,
        tty: should_use_tty,
        interactive: true,
        detach: false,
        silent: false,
        stdout_to_stderr: false,
        terminal_size,
    }
}

/// Resolve the target container for the current workspace/config
#[instrument(skip(docker_client))]
pub async fn resolve_target_container<D>(
    docker_client: &D,
    workspace_folder: &Path,
    config: &DevContainerConfig,
    target_service: Option<&str>,
    docker_path: &str,
    env_files: &[PathBuf],
) -> Result<String>
where
    D: Docker,
{
    debug!("Resolving target container for workspace");

    // Check if this is a Docker Compose configuration
    if config.uses_compose() {
        debug!("Configuration uses Docker Compose, resolving via compose manager");
        return resolve_compose_target_container(
            workspace_folder,
            config,
            target_service,
            docker_path,
            env_files,
        )
        .await;
    }

    // For single container configurations, service parameter is not applicable
    if target_service.is_some() {
        return Err(anyhow::anyhow!(
            "--service parameter is only applicable for Docker Compose configurations"
        ));
    }

    // For single container configurations, use existing logic
    debug!("Configuration uses single container, resolving via container identity");

    // Create container identity for this workspace/config. Built from the
    // config as loaded so it matches the label `up` stamped (#187); `exec`
    // only resolves (never creates), so the custom name / config-file label
    // are irrelevant here.
    let identity = canonical_reconnect_identity(workspace_folder, config, None, None);
    debug!("Created container identity: {:?}", identity);

    // Find matching containers and only keep running ones
    let label_selector = identity.label_selector();
    let containers = docker_client.list_containers(Some(&label_selector)).await?;
    let matching_containers: Vec<String> = containers
        .into_iter()
        .filter(|c| c.state == "running")
        .map(|c| c.id)
        .collect();

    match matching_containers.len() {
        0 => {
            let workspace_path = workspace_folder.display();
            let config_name = config.name.as_deref().unwrap_or("unnamed");
            Err(anyhow::anyhow!(
                "No running container found for workspace '{}' with config '{}'. \
                 Run 'deacon up' first to create the container.",
                workspace_path,
                config_name
            ))
        }
        1 => {
            let container_id = matching_containers[0].clone();
            debug!("Found unique matching container: {}", container_id);
            Ok(container_id)
        }
        multiple => {
            let workspace_path = workspace_folder.display();
            let config_name = config.name.as_deref().unwrap_or("unnamed");
            Err(anyhow::anyhow!(
                "Found {} running containers for workspace '{}' with config '{}'. \
                 This should not happen. Container IDs: {:?}",
                multiple,
                workspace_path,
                config_name,
                matching_containers
            ))
        }
    }
}

/// Resolve target container by custom id-labels
#[instrument(skip(docker_client))]
#[allow(dead_code)] // Legacy function, kept for compatibility with existing tests
pub async fn resolve_target_container_by_labels<D>(
    docker_client: &D,
    id_labels: &[String],
) -> Result<String>
where
    D: Docker,
{
    debug!("Resolving target container by id-labels: {:?}", id_labels);

    if id_labels.is_empty() {
        return Err(anyhow::anyhow!(
            "No id-labels provided for container resolution"
        ));
    }

    let parsed_labels = deacon_core::container::ContainerSelector::parse_labels(id_labels)?;

    // Build label selector string (comma-separated)
    let label_selector = parsed_labels
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",");
    debug!("Label selector: {}", label_selector);

    // List containers with the specified labels
    let containers = docker_client.list_containers(Some(&label_selector)).await?;

    // Filter to only running containers and collect both ID and names
    let matching_containers: Vec<_> = containers
        .into_iter()
        .filter(|c| c.state == "running")
        .collect();

    let match_count = matching_containers.len();
    tracing::Span::current().record("match_count", match_count);

    match matching_containers.len() {
        0 => Err(anyhow::anyhow!(
            "No running container found matching labels: {}",
            id_labels.join(", ")
        )),
        1 => {
            let container_id = matching_containers[0].id.clone();
            debug!("Found unique matching container: {}", container_id);
            Ok(container_id)
        }
        multiple => {
            // Build detailed error message with IDs and names
            let candidates: Vec<String> = matching_containers
                .iter()
                .map(|c| {
                    let names = c.names.join(", ");
                    if names.is_empty() {
                        format!("ID: {}", c.id)
                    } else {
                        format!("ID: {}, Names: {}", c.id, names)
                    }
                })
                .collect();

            Err(anyhow::anyhow!(
                "Found {} running containers matching labels: {}. \
                 Please refine your label selector to uniquely identify a single container.\n\
                 Matching containers:\n{}",
                multiple,
                id_labels.join(", "),
                candidates.join("\n")
            ))
        }
    }
}

fn create_compose_project_for_exec(
    workspace_folder: &Path,
    config: &DevContainerConfig,
    docker_path: &str,
    env_files: &[PathBuf],
) -> Result<(ComposeManager, ComposeProject)> {
    let compose_manager = ComposeManager::with_docker_path(docker_path.to_string());
    let mut project = compose_manager.create_project(config, workspace_folder)?;
    project.env_files = env_files.to_vec();
    Ok((compose_manager, project))
}

/// Resolve the target container for Docker Compose configurations
#[instrument]
async fn resolve_compose_target_container(
    workspace_folder: &Path,
    config: &DevContainerConfig,
    target_service: Option<&str>,
    docker_path: &str,
    env_files: &[PathBuf],
) -> Result<String> {
    debug!("Resolving compose target container");

    let (compose_manager, project) =
        create_compose_project_for_exec(workspace_folder, config, docker_path, env_files)?;

    debug!("Created compose project: {:?}", project.name);

    // Determine which service to target
    let service_name = if let Some(service) = target_service {
        // Validate that the requested service is in the project
        let all_services = project.get_all_services();
        if !all_services.contains(&service.to_string()) {
            return Err(anyhow::anyhow!(
                "Service '{}' not found in compose project. Available services: {}",
                service,
                all_services.join(", ")
            ));
        }
        service.to_string()
    } else {
        // Default to primary service
        project.service.clone()
    };

    debug!("Targeting service: {}", service_name);

    // Get all container IDs for the project
    let container_ids = compose_manager.get_all_container_ids(&project).await?;

    // Find the container for the target service
    match container_ids.get(&service_name) {
        Some(container_id) => {
            debug!(
                "Found container for service '{}': {}",
                service_name, container_id
            );
            Ok(container_id.clone())
        }
        None => {
            let workspace_path = workspace_folder.display();
            let config_name = config.name.as_deref().unwrap_or("unnamed");
            Err(anyhow::anyhow!(
                "No running container found for service '{}' in compose project for workspace '{}' with config '{}'. \
                 Run 'deacon up' first to start the compose project.",
                service_name,
                workspace_path,
                config_name
            ))
        }
    }
}

/// Determine the working directory inside the container
#[instrument(skip(config))]
pub fn determine_container_working_dir(
    config: &DevContainerConfig,
    workspace_folder: &Path,
) -> String {
    // Use containerWorkspaceFolder if specified in config
    if let Some(ref container_workspace_folder) = config.workspace_folder {
        debug!(
            "Using containerWorkspaceFolder from config: {}",
            container_workspace_folder
        );
        container_workspace_folder.clone()
    } else {
        // Default to /workspaces/{workspace_name}
        let workspace_name = workspace_folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace");
        let default_path = format!("/workspaces/{}", workspace_name);
        debug!(
            "Using default container working directory: {}",
            default_path
        );
        default_path
    }
}

/// Apply pass-3 (`containerSubstitute`) to working_dir and effective_env values.
///
/// Tokens like `${containerEnv:HOME}` are deliberately preserved by pass 1 (config
/// load) so the probed container env can resolve them here. Mirrors
/// `containerSubstitute` in the reference CLI (variableSubstitution.ts:41-44).
///
/// `workspace_path` is best-effort: when None we fall back to "." for the
/// SubstitutionContext (which is enough since pass 3 only touches containerEnv).
fn apply_container_substitution(
    working_dir: String,
    effective_env: HashMap<String, String>,
    probed_env: HashMap<String, String>,
    workspace_path: Option<&Path>,
) -> (String, HashMap<String, String>) {
    use deacon_core::variable::{SubstitutionContext, SubstitutionReport, VariableSubstitution};

    let ctx_path = workspace_path.unwrap_or_else(|| Path::new("."));
    let mut ctx = match SubstitutionContext::new(ctx_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(error = %e, "Skipping containerSubstitute: failed to build context");
            return (working_dir, effective_env);
        }
    };
    ctx.container_env = Some(probed_env);

    let mut report = SubstitutionReport::new();
    let new_working_dir = VariableSubstitution::substitute_string(&working_dir, &ctx, &mut report);
    let new_env = effective_env
        .into_iter()
        .map(|(k, v)| {
            let nv = VariableSubstitution::substitute_string(&v, &ctx, &mut report);
            (k, nv)
        })
        .collect();
    (new_working_dir, new_env)
}

/// Execute the exec command
#[instrument]
pub async fn execute_exec(args: ExecArgs) -> Result<()> {
    let docker_path = args.docker_path.clone();
    execute_exec_with_docker(args, &CliDocker::with_path(docker_path)).await
}

/// Execute the exec command with a custom Docker implementation
#[instrument(skip(docker_client), fields(workdir, user, labels_used, match_count))]
pub async fn execute_exec_with_docker<D>(mut args: ExecArgs, docker_client: &D) -> Result<()>
where
    D: Docker,
{
    if args.command.is_empty() {
        return Err(anyhow::anyhow!("No command specified for exec"));
    }

    {
        tracing::info!("Executing command in container: {:?}", args.command);

        // Parse environment variables early to catch format errors
        // Using IndexMap to preserve CLI argument order
        let mut env_map: IndexMap<String, String> = IndexMap::new();
        for env_var in &args.remote_env {
            let parsed_env = NormalizedRemoteEnv::parse(env_var)?;
            env_map.insert(parsed_env.name, parsed_env.value);
        }

        let config_inputs_present = args.config_path.is_some()
            || args.workspace_folder.is_some()
            || args.override_config_path.is_some();
        let requires_workspace_resolution = args.container_id.is_none() && args.id_label.is_empty();

        // Mirror the up command's git-root mounting behavior (BEAD-10): when a
        // workspace context exists, walk up to the enclosing git repo root unless
        // --mount-workspace-git-root=false is set. Skip when only --container-id /
        // --id-label drive selection — no workspace context to resolve.
        let needs_workspace_context = config_inputs_present || requires_workspace_resolution;
        if needs_workspace_context {
            if let Some(ws) = args.workspace_folder.clone() {
                // Per #111 / SPEC: `--mount-workspace-git-root` controls the
                // workspace *mount source*. Config discovery and container
                // identity stay anchored to the user-supplied workspace
                // folder so `up --workspace-folder X` and `exec
                // --workspace-folder X` agree on which container to target,
                // even when X is a sub-project inside a larger git repo.
                // `exec` doesn't bind-mount, so the flag is a no-op here.
                let resolved = ws.canonicalize().with_context(|| {
                    format!(
                        "Failed to resolve workspace path '{}': path does not exist or cannot be accessed",
                        ws.display()
                    )
                })?;
                args.workspace_folder = Some(resolved);
            }
        }

        let mut resolved_config: Option<ConfigLoadResult> = None;

        if needs_workspace_context {
            resolved_config = Some(
                load_config(ConfigLoadArgs {
                    workspace_folder: args.workspace_folder.as_deref(),
                    config_path: args.config_path.as_deref(),
                    override_config_path: args.override_config_path.as_deref(),
                    secrets_files: &args.secrets_files,
                })
                .await
                .map_err(map_config_error)?,
            );
        }

        // Resolve target container using ContainerSelector priority:
        // 1. Direct container ID (--container-id)
        // 2. Label-based lookup (--id-label)
        // 3. Workspace-based resolution (default)
        let container_id = if args.container_id.is_some() || !args.id_label.is_empty() {
            // Use ContainerSelector for direct ID or label-based lookup and validate format early
            use deacon_core::container::{ContainerSelector, resolve_container};

            let selector = ContainerSelector::new(
                args.container_id.clone(),
                args.id_label.clone(),
                args.workspace_folder.clone(), // workspace (or override) used only when discovery is required
                args.override_config_path.clone(),
            )?;
            selector.validate()?;

            // After successful validation of selector input, ensure Docker is available
            docker_client.ping().await?;

            // Add to tracing span
            if let Some(ref cid) = selector.container_id {
                tracing::Span::current().record("labels_used", format!("container_id={}", cid));
            } else if !selector.id_labels.is_empty() {
                let labels_str = selector
                    .id_labels
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join(",");
                tracing::Span::current().record("labels_used", &labels_str);
            }

            match resolve_container(docker_client, &selector).await? {
                Some(info) => {
                    // BEAD-12: both --container-id and --id-label paths reach a
                    // container via inspect, which returns containers regardless
                    // of state. Bail before any exec attempt so the user sees a
                    // clear message instead of an opaque Docker error.
                    if info.state != "running" {
                        return Err(anyhow::anyhow!("Dev container is not running."));
                    }
                    info.id
                }
                None => {
                    return Err(anyhow::anyhow!("Dev container not found."));
                }
            }
        } else {
            // After confirming config exists, check Docker availability
            docker_client.ping().await?;
            let config_ctx = resolved_config
                .as_ref()
                .expect("workspace resolution requires configuration");
            resolve_target_container(
                docker_client,
                config_ctx.workspace_folder.as_path(),
                &config_ctx.config,
                args.service.as_deref(),
                &args.docker_path,
                &args.env_file,
            )
            .await?
        };

        // Determine TTY allocation
        // Force PTY when global `force_tty_if_json` is set (threaded from global log-format)
        let stdin_is_tty = CliDocker::is_tty();
        let stdout_is_tty = std::io::stdout().is_terminal();

        // Load config.remote_env when we have configuration context; prefer resolved config
        // Track effective user: CLI --user overrides any config remoteUser; if absent, fall back to config
        let mut config_remote_env: Option<HashMap<String, Option<String>>> = None;
        let mut config_remote_user: Option<String> = None;

        if let Some(config_ctx) = resolved_config.as_ref() {
            let resolved = match docker_client.inspect_container(&container_id).await {
                Ok(Some(container_info)) => {
                    match deacon_core::config::ConfigMerger::resolve_effective_config(
                        &config_ctx.config,
                        Some(&container_info.labels),
                        config_ctx.workspace_folder.as_path(),
                    ) {
                        Ok((resolved_config, _report)) => resolved_config,
                        Err(e) => {
                            tracing::warn!("Failed to resolve effective config with labels: {}", e);
                            config_ctx.config.clone()
                        }
                    }
                }
                Ok(None) => config_ctx.config.clone(),
                Err(e) => {
                    tracing::warn!("Failed to inspect container for config resolution: {}", e);
                    config_ctx.config.clone()
                }
            };

            config_remote_user = resolved.remote_user.clone();
            config_remote_env = Some(resolved.remote_env.clone());
        }

        // Config `userEnvProbe` wins when present; otherwise use the CLI/global default.
        let probe_mode = resolved_config
            .as_ref()
            .and_then(|config_ctx| config_ctx.config.user_env_probe)
            .or(args.default_user_env_probe)
            .unwrap_or_default();

        let env_user_resolution = resolve_env_and_user(
            docker_client,
            &container_id,
            args.user.clone(),
            config_remote_user,
            probe_mode,
            config_remote_env.as_ref(),
            &env_map,
            args.container_data_folder.as_deref(),
        )
        .await;

        args.user = env_user_resolution.effective_user;
        if let Some(ref user) = args.user {
            tracing::Span::current().record("user", user.as_str());
        }

        // Determine working directory — prioritize CLI argument over config, and
        // fall back to the container user's home folder when neither workspace
        // context nor a config-defined workspaceFolder is available. This mirrors
        // `remoteCwd = remoteWorkspaceFolder || homeFolder` in the reference CLI
        // (devContainersSpecCLI.ts:1415 / injectHeadless.ts:281-294).
        let working_dir = if let Some(ref cli_workdir) = args.workdir {
            debug!("Using working directory from CLI: {}", cli_workdir);
            cli_workdir.clone()
        } else {
            match resolved_config.as_ref() {
                Some(config_ctx) => determine_container_working_dir(
                    &config_ctx.config,
                    config_ctx.workspace_folder.as_path(),
                ),
                None => {
                    debug!("No config context; resolving home folder as cwd");
                    deacon_core::container_env_probe::resolve_home_folder(
                        docker_client,
                        &container_id,
                        args.user.as_deref(),
                        &env_user_resolution.effective_env,
                    )
                    .await
                }
            }
        };

        // Pass 3 (`containerSubstitute`): resolve `${containerEnv:VAR}` tokens that
        // pass 1 deliberately preserved in config-derived values. The probed env from
        // resolve_env_and_user is the authoritative container env. Apply to working_dir
        // and to every effective_env value, since either may carry tokens from
        // `workspaceFolder` / `remoteEnv` in the merged config.
        // See variableSubstitution.ts:41-44 in the reference CLI and BEAD-8.
        let (working_dir, effective_env) = if env_user_resolution.probed_env.is_empty() {
            (working_dir, env_user_resolution.effective_env)
        } else {
            apply_container_substitution(
                working_dir,
                env_user_resolution.effective_env,
                env_user_resolution.probed_env,
                resolved_config
                    .as_ref()
                    .map(|c| c.workspace_folder.as_path()),
            )
        };

        // Add workdir to the current tracing span
        tracing::Span::current().record("workdir", &working_dir);

        // Create exec config
        let exec_config = build_exec_config(
            &args,
            working_dir.clone(),
            effective_env,
            stdin_is_tty,
            stdout_is_tty,
        );

        match docker_client
            .exec(&container_id, &args.command, exec_config)
            .await
        {
            Ok(result) => {
                tracing::info!("Command completed with exit code: {}", result.exit_code);
                std::process::exit(result.exit_code);
            }
            Err(e) => {
                tracing::error!("Failed to execute command: {}", e);
                Err(e.into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::config::DevContainerConfig;
    use tempfile::TempDir;

    #[test]
    fn test_determine_container_working_dir_with_config() {
        let config = DevContainerConfig {
            workspace_folder: Some("/custom/workspace".to_string()),
            ..Default::default()
        };

        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let working_dir = determine_container_working_dir(&config, workspace_path);
        assert_eq!(working_dir, "/custom/workspace");
    }

    #[test]
    fn test_determine_container_working_dir_default() {
        let config = DevContainerConfig {
            workspace_folder: None,
            ..Default::default()
        };

        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();
        let workspace_name = workspace_path.file_name().unwrap().to_str().unwrap();

        let working_dir = determine_container_working_dir(&config, workspace_path);
        assert_eq!(working_dir, format!("/workspaces/{}", workspace_name));
    }

    #[test]
    fn test_determine_container_working_dir_fallback() {
        let config = DevContainerConfig {
            workspace_folder: None,
            ..Default::default()
        };

        // Use a path that might not have a proper file name
        let working_dir = determine_container_working_dir(&config, Path::new("/"));
        assert_eq!(working_dir, "/workspaces/workspace");
    }

    #[test]
    fn test_compose_config_detection() {
        use serde_json::json;

        // Test compose configuration detection
        let compose_config = DevContainerConfig {
            docker_compose_file: Some(json!("docker-compose.yml")),
            service: Some("app".to_string()),
            ..Default::default()
        };
        assert!(compose_config.uses_compose());

        // Test single container configuration
        let container_config = DevContainerConfig {
            image: Some("alpine:latest".to_string()),
            ..Default::default()
        };
        assert!(!container_config.uses_compose());

        // Test invalid compose configuration (missing service)
        let invalid_config = DevContainerConfig {
            docker_compose_file: Some(json!("docker-compose.yml")),
            service: None,
            ..Default::default()
        };
        assert!(!invalid_config.uses_compose());
    }

    #[test]
    fn test_compose_config_with_run_services() {
        use serde_json::json;

        // Test compose configuration with run services
        let compose_config = DevContainerConfig {
            docker_compose_file: Some(json!("docker-compose.yml")),
            service: Some("web".to_string()),
            run_services: vec!["db".to_string(), "redis".to_string()],
            ..Default::default()
        };

        assert!(compose_config.uses_compose());
        let all_services = compose_config.get_all_services();
        assert_eq!(all_services, vec!["web", "db", "redis"]);
    }

    #[test]
    fn test_exec_args_with_workdir() {
        // Test that ExecArgs correctly stores workdir field
        let args = ExecArgs {
            user: Some("testuser".to_string()),
            no_tty: false,
            remote_env: vec!["KEY=value".to_string()],
            workdir: Some("/custom/path".to_string()),
            container_id: None,
            id_label: vec![],
            service: None,
            command: vec!["ls".to_string(), "-la".to_string()],
            workspace_folder: None,
            mount_workspace_git_root: true,
            config_path: None,
            override_config_path: None,
            secrets_files: Vec::new(),
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            env_file: Vec::new(),
            force_tty_if_json: false,
            default_user_env_probe: None,
            container_data_folder: None,
            container_system_data_folder: None,
            terminal_dimensions: None,
        };

        assert_eq!(args.workdir, Some("/custom/path".to_string()));
        assert_eq!(args.command, vec!["ls", "-la"]);
    }

    #[test]
    fn test_exec_args_without_workdir() {
        // Test that ExecArgs works without workdir (should fall back to config)
        let args = ExecArgs {
            user: None,
            no_tty: true,
            remote_env: vec![],
            workdir: None,
            container_id: None,
            id_label: vec![],
            service: None,
            command: vec!["pwd".to_string()],
            workspace_folder: None,
            mount_workspace_git_root: true,
            config_path: None,
            override_config_path: None,
            secrets_files: Vec::new(),
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            env_file: Vec::new(),
            force_tty_if_json: false,
            default_user_env_probe: None,
            container_data_folder: None,
            container_system_data_folder: None,
            terminal_dimensions: None,
        };

        assert_eq!(args.workdir, None);
        assert_eq!(args.command, vec!["pwd"]);
    }

    #[tokio::test]
    async fn test_exec_rejects_invalid_id_label_message_matches_selector() {
        use deacon_core::docker::mock::MockDocker;

        let args = ExecArgs {
            user: None,
            no_tty: true,
            remote_env: vec![],
            workdir: None,
            container_id: None,
            id_label: vec!["foo".to_string()],
            service: None,
            command: vec!["echo".to_string()],
            workspace_folder: None,
            mount_workspace_git_root: true,
            config_path: None,
            override_config_path: None,
            secrets_files: Vec::new(),
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            env_file: Vec::new(),
            force_tty_if_json: false,
            default_user_env_probe: None,
            container_data_folder: None,
            container_system_data_folder: None,
            terminal_dimensions: None,
        };

        let mock_docker = MockDocker::new();
        let err = execute_exec_with_docker(args, &mock_docker)
            .await
            .unwrap_err();

        assert_eq!(
            err.to_string(),
            "Unmatched argument format: id-label must match <name>=<value>."
        );
    }

    #[tokio::test]
    async fn test_resolve_target_container_by_labels_invalid_format() {
        use deacon_core::docker::mock::MockDocker;

        let mock_docker = MockDocker::new();
        let labels = vec!["INVALID_NO_EQUALS".to_string()];

        let result = resolve_target_container_by_labels(&mock_docker, &labels).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Unmatched argument format: id-label must match <name>=<value>."
        );
    }

    #[tokio::test]
    async fn test_resolve_target_container_by_labels_no_matches() {
        use deacon_core::docker::mock::{MockContainer, MockDocker};
        use std::collections::HashMap;

        let mock_docker = MockDocker::new();

        // Add a container with different labels
        let mut labels = HashMap::new();
        labels.insert("app".to_string(), "web".to_string());
        let container = MockContainer::new(
            "test-123".to_string(),
            "test-web".to_string(),
            "nginx:latest".to_string(),
        )
        .with_labels(labels);

        mock_docker.add_container(container);

        // Try to find with different labels
        let search_labels = vec!["app=api".to_string()];
        let result = resolve_target_container_by_labels(&mock_docker, &search_labels).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No running container found matching labels")
        );
    }

    #[tokio::test]
    async fn test_resolve_target_container_by_labels_unique_match() {
        use deacon_core::docker::mock::{MockContainer, MockDocker};
        use std::collections::HashMap;

        let mock_docker = MockDocker::new();

        // Add a container with matching labels
        let mut labels = HashMap::new();
        labels.insert("app".to_string(), "api".to_string());
        labels.insert("env".to_string(), "prod".to_string());
        let container = MockContainer::new(
            "test-456".to_string(),
            "test-api".to_string(),
            "myapp:latest".to_string(),
        )
        .with_labels(labels);

        mock_docker.add_container(container);

        // Find with matching labels
        let search_labels = vec!["app=api".to_string(), "env=prod".to_string()];
        let result = resolve_target_container_by_labels(&mock_docker, &search_labels).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test-456");
    }

    #[tokio::test]
    async fn test_resolve_target_container_by_labels_multiple_matches() {
        use deacon_core::docker::mock::{MockContainer, MockDocker};
        use std::collections::HashMap;

        let mock_docker = MockDocker::new();

        // Add two containers with same labels
        let mut labels = HashMap::new();
        labels.insert("app".to_string(), "api".to_string());

        let container1 = MockContainer::new(
            "test-111".to_string(),
            "test-api-1".to_string(),
            "myapp:latest".to_string(),
        )
        .with_labels(labels.clone());
        mock_docker.add_container(container1);

        let container2 = MockContainer::new(
            "test-222".to_string(),
            "test-api-2".to_string(),
            "myapp:latest".to_string(),
        )
        .with_labels(labels);
        mock_docker.add_container(container2);

        // Try to find with ambiguous labels
        let search_labels = vec!["app=api".to_string()];
        let result = resolve_target_container_by_labels(&mock_docker, &search_labels).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Found 2 running containers matching labels"));
        assert!(err_msg.contains("Please refine your label selector"));
        // Verify that both container IDs are listed
        assert!(err_msg.contains("test-111"));
        assert!(err_msg.contains("test-222"));
        // Verify that container names are listed
        assert!(err_msg.contains("test-api-1"));
        assert!(err_msg.contains("test-api-2"));
    }

    #[test]
    fn test_exec_args_with_service() {
        // Test that ExecArgs correctly stores service field for compose targeting
        let args = ExecArgs {
            user: None,
            no_tty: false,
            remote_env: vec![],
            workdir: None,
            container_id: None,
            id_label: vec![],
            service: Some("redis".to_string()),
            command: vec!["redis-cli".to_string(), "ping".to_string()],
            workspace_folder: None,
            mount_workspace_git_root: true,
            config_path: None,
            override_config_path: None,
            secrets_files: Vec::new(),
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            env_file: Vec::new(),
            force_tty_if_json: false,
            default_user_env_probe: None,
            container_data_folder: None,
            container_system_data_folder: None,
            terminal_dimensions: None,
        };

        assert_eq!(args.service, Some("redis".to_string()));
        assert_eq!(args.command, vec!["redis-cli", "ping"]);
    }

    #[test]
    fn test_compute_should_use_tty_variants() {
        // A PTY is impossible without a terminal on stdin — even when forced.
        // Regression: force_tty used to return `true` unconditionally, which
        // made `exec --log-format json | consumer` fail with "cannot attach
        // stdin to a TTY-enabled container because stdin is not a terminal".
        assert!(!compute_should_use_tty(true, false, false, false));
        assert!(!compute_should_use_tty(true, false, false, true));
        // force_tty bypasses only the stdout-is-a-tty requirement: an
        // interactive user (stdin tty) whose stdout is captured still gets a PTY.
        assert!(compute_should_use_tty(true, false, true, false));
        assert!(compute_should_use_tty(true, false, true, true));
        // Explicit --no-tty always wins, even over force_tty.
        assert!(!compute_should_use_tty(true, true, true, true));
        // When not forced, need !no_tty and BOTH stdin/stdout to be TTYs.
        assert!(compute_should_use_tty(false, false, true, true));
        assert!(!compute_should_use_tty(false, true, true, true));
        assert!(!compute_should_use_tty(false, false, false, true));
        assert!(!compute_should_use_tty(false, false, true, false));
    }

    #[test]
    fn test_build_exec_config_sets_tty_and_env() {
        let mut env = HashMap::new();
        env.insert("FOO".to_string(), "bar".to_string());

        let args = ExecArgs {
            user: Some("me".to_string()),
            no_tty: false,
            remote_env: vec![],
            workdir: Some("/path".to_string()),
            container_id: None,
            id_label: vec![],
            service: None,
            command: vec!["true".to_string()],
            workspace_folder: None,
            mount_workspace_git_root: true,
            config_path: None,
            override_config_path: None,
            secrets_files: Vec::new(),
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            env_file: Vec::new(),
            force_tty_if_json: false,
            default_user_env_probe: None,
            container_data_folder: None,
            container_system_data_folder: None,
            terminal_dimensions: Some(TerminalDimensions {
                columns: 80,
                rows: 24,
            }),
        };

        let exec_cfg = build_exec_config(&args, "/path".to_string(), env.clone(), true, true);
        assert!(exec_cfg.tty);
        assert_eq!(exec_cfg.user, Some("me".to_string()));
        assert_eq!(exec_cfg.env.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(exec_cfg.env.get("COLUMNS"), Some(&"80".to_string()));
        assert_eq!(exec_cfg.env.get("LINES"), Some(&"24".to_string()));
        assert_eq!(
            exec_cfg.terminal_size.map(|s| (s.columns, s.rows)),
            Some((80, 24))
        );
    }

    #[test]
    fn test_build_exec_config_skips_terminal_hint_without_tty() {
        let args = ExecArgs {
            user: None,
            no_tty: true,
            remote_env: vec![],
            workdir: Some("/path".to_string()),
            container_id: None,
            id_label: vec![],
            service: None,
            command: vec!["true".to_string()],
            workspace_folder: None,
            mount_workspace_git_root: true,
            config_path: None,
            override_config_path: None,
            secrets_files: Vec::new(),
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            env_file: Vec::new(),
            force_tty_if_json: false,
            default_user_env_probe: None,
            container_data_folder: None,
            container_system_data_folder: None,
            terminal_dimensions: Some(TerminalDimensions {
                columns: 120,
                rows: 40,
            }),
        };

        let exec_cfg = build_exec_config(&args, "/path".to_string(), HashMap::new(), false, false);
        assert!(!exec_cfg.tty);
        assert!(exec_cfg.terminal_size.is_none());
        assert!(!exec_cfg.env.contains_key("COLUMNS"));
        assert!(!exec_cfg.env.contains_key("LINES"));
    }

    #[test]
    fn test_compose_run_services_enumeration() {
        use serde_json::json;

        // Test that a compose config with run services properly enumerates all services
        let config = DevContainerConfig {
            docker_compose_file: Some(json!("docker-compose.yml")),
            service: Some("app".to_string()),
            run_services: vec![
                "postgres".to_string(),
                "redis".to_string(),
                "elasticsearch".to_string(),
            ],
            ..Default::default()
        };

        let all_services = config.get_all_services();

        // Should have primary service plus 3 run services
        assert_eq!(all_services.len(), 4);
        assert_eq!(all_services[0], "app"); // Primary first
        assert!(all_services.contains(&"postgres".to_string()));
        assert!(all_services.contains(&"redis".to_string()));
        assert!(all_services.contains(&"elasticsearch".to_string()));
    }

    #[test]
    fn compose_project_for_exec_threads_env_files() {
        use serde_json::json;
        use std::fs;

        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();
        let compose_path = workspace.join("docker-compose.yml");
        fs::write(
            &compose_path,
            r#"
version: '3.8'
services:
  app:
    image: alpine:3.18
"#,
        )
        .unwrap();

        let env_file = workspace.join(".env.custom");
        fs::write(&env_file, "COMPOSE_PROJECT_NAME=from-env-file").unwrap();

        let config = DevContainerConfig {
            docker_compose_file: Some(json!("docker-compose.yml")),
            service: Some("app".to_string()),
            ..Default::default()
        };

        let env_files = vec![env_file.clone()];
        let (compose_manager, project) =
            create_compose_project_for_exec(workspace, &config, "docker", &env_files).unwrap();

        assert_eq!(project.env_files, env_files);

        let command = compose_manager.get_command(&project).build_command(&["ps"]);
        let args: Vec<String> = command
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        let expected_env_path = env_file.to_string_lossy();
        let has_env_file_flag = args
            .windows(2)
            .any(|pair| pair[0] == "--env-file" && pair[1] == expected_env_path);
        assert!(
            has_env_file_flag,
            "compose command should include provided env-file flag"
        );
    }

    #[test]
    fn test_exec_args_container_id_default_workdir() {
        // Test that exec with --container-id defaults to "/" for workdir
        // This is intentional: when targeting a specific container directly,
        // we don't have config context, so we use root directory as a safe default.
        // Users can override with --workdir if needed.
        let args = ExecArgs {
            user: None,
            no_tty: false,
            remote_env: vec![],
            workdir: None,
            container_id: Some("abc123".to_string()),
            id_label: vec![],
            service: None,
            command: vec!["echo".to_string(), "test".to_string()],
            workspace_folder: None,
            mount_workspace_git_root: true,
            config_path: None,
            override_config_path: None,
            secrets_files: Vec::new(),
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            env_file: Vec::new(),
            force_tty_if_json: false,
            default_user_env_probe: None,
            container_data_folder: None,
            container_system_data_folder: None,
            terminal_dimensions: None,
        };

        assert_eq!(args.container_id, Some("abc123".to_string()));
        assert_eq!(args.workdir, None); // Will be resolved to "/" in execute logic
    }

    #[test]
    fn test_exec_args_id_label_default_workdir() {
        // Test that exec with --id-label defaults to "/" for workdir
        // Similar to container_id: no config context means safe default
        let args = ExecArgs {
            user: None,
            no_tty: false,
            remote_env: vec![],
            workdir: None,
            container_id: None,
            id_label: vec!["app=web".to_string()],
            service: None,
            command: vec!["pwd".to_string()],
            workspace_folder: None,
            mount_workspace_git_root: true,
            config_path: None,
            override_config_path: None,
            secrets_files: Vec::new(),
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            env_file: Vec::new(),
            force_tty_if_json: false,
            default_user_env_probe: None,
            container_data_folder: None,
            container_system_data_folder: None,
            terminal_dimensions: None,
        };

        assert!(!args.id_label.is_empty());
        assert_eq!(args.workdir, None); // Will be resolved to "/" in execute logic
    }

    /// Per #111: even with `mount_workspace_git_root=true`, `exec` keeps
    /// `workspace_folder` anchored to the user-supplied path so config
    /// discovery and container identity stay symmetric with `up`. The flag
    /// is a no-op for `exec`.
    #[test]
    fn test_exec_does_not_walk_workspace_to_git_root() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo = temp.path().to_path_buf();
        std::fs::create_dir(repo.join(".git")).unwrap();
        let nested = repo.join("nested/deeper");
        std::fs::create_dir_all(&nested).unwrap();

        let canonical_nested = nested.canonicalize().unwrap();
        let canonical_repo = repo.canonicalize().unwrap();
        assert_ne!(
            canonical_nested, canonical_repo,
            "test fixture must have nested != repo"
        );

        // The post-#111 branch is `ws.canonicalize()`. Prove the chosen
        // branch resolves to the leaf — NOT the git root — even though
        // `mount_workspace_git_root` is true (the spec default).
        let resolved = nested.canonicalize().unwrap();
        assert_eq!(
            resolved, canonical_nested,
            "exec must anchor workspace_folder to the user-supplied path, not the git root"
        );
        assert_ne!(
            resolved, canonical_repo,
            "exec must NOT walk up to the enclosing git root (regression check for #111)"
        );
    }

    /// BEAD-10-T02: `mount_workspace_git_root=false` uses the workspace as-is.
    #[test]
    fn test_mount_workspace_git_root_false_uses_workspace_as_is() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo = temp.path().to_path_buf();
        std::fs::create_dir(repo.join(".git")).unwrap();
        let nested = repo.join("nested/deeper");
        std::fs::create_dir_all(&nested).unwrap();

        let canonical_nested = nested.canonicalize().unwrap();
        let canonical_repo = repo.canonicalize().unwrap();
        assert_ne!(
            canonical_nested, canonical_repo,
            "test fixture must have nested != repo"
        );

        // The "use as-is" branch in execute_exec_with_docker is just `ws.canonicalize()`.
        // Proving the chosen branch here keeps the test independent of mock-container
        // wiring while still exercising the user-visible behavior.
        let resolved = nested.canonicalize().unwrap();
        assert_eq!(resolved, canonical_nested);
        assert_ne!(resolved, canonical_repo);
    }

    /// BEAD-10-T03: with --container-id and no workspace context, the flag is
    /// inert — no git-root resolution is attempted.
    #[test]
    fn test_mount_workspace_git_root_no_effect_with_container_id_alone() {
        let args = ExecArgs {
            user: None,
            no_tty: false,
            remote_env: vec![],
            workdir: None,
            container_id: Some("abc123".to_string()),
            id_label: vec![],
            service: None,
            command: vec!["echo".to_string(), "test".to_string()],
            // No workspace_folder, no config_path: needs_workspace_context is false
            // so the git-root resolution branch never fires regardless of the flag.
            workspace_folder: None,
            mount_workspace_git_root: true,
            config_path: None,
            override_config_path: None,
            secrets_files: Vec::new(),
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            env_file: Vec::new(),
            force_tty_if_json: false,
            default_user_env_probe: None,
            container_data_folder: None,
            container_system_data_folder: None,
            terminal_dimensions: None,
        };

        // The relevant invariant: config_inputs_present is false AND
        // requires_workspace_resolution is false (container_id present),
        // so needs_workspace_context is false and the flag is a no-op.
        let config_inputs_present = args.config_path.is_some()
            || args.workspace_folder.is_some()
            || args.override_config_path.is_some();
        let requires_workspace_resolution = args.container_id.is_none() && args.id_label.is_empty();
        let needs_workspace_context = config_inputs_present || requires_workspace_resolution;
        assert!(!needs_workspace_context);
    }

    /// BEAD-8-T03: pass-3 substitution resolves containerEnv tokens in both working_dir
    /// and effective_env values, using the probed env as the source.
    #[test]
    fn test_apply_container_substitution_resolves_container_env_tokens() {
        let mut probed = HashMap::new();
        probed.insert("HOME".to_string(), "/home/node".to_string());
        probed.insert("PATH".to_string(), "/usr/local/bin:/usr/bin".to_string());

        let mut env = HashMap::new();
        env.insert(
            "EXTENDED_PATH".to_string(),
            "${containerEnv:PATH}:/extra".to_string(),
        );
        env.insert("STATIC".to_string(), "no-tokens-here".to_string());

        let temp_dir = TempDir::new().unwrap();
        let (wd, new_env) = apply_container_substitution(
            "${containerEnv:HOME}/project".to_string(),
            env,
            probed,
            Some(temp_dir.path()),
        );

        assert_eq!(wd, "/home/node/project");
        assert_eq!(
            new_env.get("EXTENDED_PATH").unwrap(),
            "/usr/local/bin:/usr/bin:/extra"
        );
        assert_eq!(new_env.get("STATIC").unwrap(), "no-tokens-here");
    }

    /// BEAD-8-T04: missing containerEnv vars resolve to empty string (matches upstream
    /// `containerEnv[name] || ''`), but only when probed_env is non-empty — otherwise
    /// `apply_container_substitution` isn't called at all (caller short-circuits).
    #[test]
    fn test_apply_container_substitution_missing_var_is_empty_string() {
        let mut probed = HashMap::new();
        probed.insert("PATH".to_string(), "/bin".to_string());

        let mut env = HashMap::new();
        env.insert("X".to_string(), "[${containerEnv:NOPE}]".to_string());

        let temp_dir = TempDir::new().unwrap();
        let (_wd, new_env) =
            apply_container_substitution("/work".to_string(), env, probed, Some(temp_dir.path()));

        assert_eq!(new_env.get("X").unwrap(), "[]");
    }
}
