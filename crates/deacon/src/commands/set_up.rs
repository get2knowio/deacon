//! Set-up subcommand implementation.
//!
//! `set-up` converts an already-running container into a DevContainer by
//! applying configuration + image metadata, executing lifecycle hooks, and
//! returning a JSON snapshot of the (optionally merged) configuration.
//!
//! See the containers.dev spec / reference CLI for the authoritative behavior.
//!
//! ## Scope (PR-6a + PR-6b + PR-6c)
//!
//! - `--container-id` resolution + inspect validation
//! - Optional `--config` load via the shared `ConfigLoader` (extends-aware)
//! - Image-metadata extraction from the container's `devcontainer.metadata`
//!   label and merge with the parsed config
//! - Variable substitution (config + merged config)
//! - **`/etc/environment` + `/etc/profile` root patches** (PR-6c) — guarded
//!   by markers under `--container-system-data-folder` (default
//!   `/var/devcontainer/`). Best-effort: a non-zero exit from the
//!   patch shell emits a WARN and proceeds (spec §9 — system-level patches
//!   "do not abort set-up unless critical")
//! - Lifecycle hook execution (`onCreate` → `updateContent` → `postCreate` →
//!   `postStart` → `postAttach`) via the shared `ContainerLifecycle` helper,
//!   gated by `--skip-post-create` and `--skip-non-blocking-commands`
//! - **Dotfiles installer** (`--dotfiles-repository` / `--dotfiles-install-command`
//!   / `--dotfiles-target-path`) via `ContainerLifecycle`'s built-in clone +
//!   auto-detect installer + target-path marker (PR-6b)
//! - JSON output on stdout: `{outcome, configuration?, mergedConfiguration?}`
//!
//! ## Deferred (post-PR-6c)
//!
//! - A second substitution pass against the live container environment
//!   (`${containerEnv:VAR}`) — current pass uses only the configured
//!   `container_env`, not the live `docker exec` env probe

use crate::commands::shared::resolve_runtime;
use anyhow::{Context, Result};
use deacon_core::config::DevContainerConfig;
use deacon_core::container_lifecycle::{
    AggregatedLifecycleCommand, ContainerLifecycleCommands, ContainerLifecycleConfig,
    DotfilesConfig, LifecycleCommandList, LifecycleCommandSource, LifecycleCommandValue,
    execute_container_lifecycle_with_progress_callback_and_docker,
};
use deacon_core::docker::{CliRuntime, ContainerInfo, Docker, ExecConfig};
use deacon_core::runtime::RuntimeKind;
use deacon_core::variable::SubstitutionContext;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

/// Arguments for the `set-up` command. Mirrors the spec's CLI surface
/// (the containers.dev spec / reference CLI).
#[derive(Debug, Clone)]
pub struct SetUpArgs {
    /// Required: container id of the already-running container to set up.
    pub container_id: String,
    /// Optional path to a devcontainer.json to layer on top of the
    /// container's embedded image metadata.
    pub config_path: Option<PathBuf>,
    /// Skip all lifecycle hooks (onCreate, updateContent, postCreate, postStart,
    /// postAttach) and dotfiles installation. Spec §2 (`--skip-post-create`).
    pub skip_post_create: bool,
    /// Stop after the configured `waitFor` hook (default `updateContent`).
    /// Spec §2 (`--skip-non-blocking-commands`).
    pub skip_non_blocking_commands: bool,
    /// Extra remote-env entries to inject when running hooks
    /// (CLI `--remote-env name=value`, repeatable).
    pub remote_env: Vec<String>,
    /// Dotfiles git repository URL or `owner/repo` shorthand.
    /// Spec §2 (`--dotfiles-repository`).
    pub dotfiles_repository: Option<String>,
    /// Custom dotfiles install command. When `None`, the lifecycle helper
    /// auto-detects `install.sh` / `bootstrap` / `setup` / `script/*`.
    /// Spec §2 (`--dotfiles-install-command`).
    pub dotfiles_install_command: Option<String>,
    /// Override for the dotfiles clone target. Defaults are computed by the
    /// lifecycle helper based on the remote user (`~/dotfiles`).
    /// Spec §2 (`--dotfiles-target-path`).
    pub dotfiles_target_path: Option<String>,
    /// Include the (substituted) configuration in the JSON result.
    pub include_configuration: bool,
    /// Include the (substituted) merged configuration in the JSON result.
    pub include_merged_configuration: bool,
    /// Inside-container user data root (default `~/.devcontainer`); reserved
    /// for marker-file storage, currently only forwarded to the lifecycle
    /// helper as `cache_folder`.
    pub container_data_folder: Option<PathBuf>,
    /// Inside-container system data root for root-owned marker files; default
    /// `/var/devcontainer`. Spec §6 — `.patchEtcEnvironmentMarker` and
    /// `.patchEtcProfileMarker` live here.
    pub container_system_data_folder: Option<PathBuf>,
    /// Docker CLI path; defaults to `"docker"`. Forwarded to
    /// [`resolve_runtime`] so a `--docker-path` override reaches the selected
    /// runtime (and, under docker, the binary it shells out to).
    pub docker_path: String,
    /// Progress tracker shared with the CLI shell.
    pub progress_tracker: Arc<Mutex<Option<deacon_core::progress::ProgressTracker>>>,
}

/// JSON result emitted on stdout. Per spec §10:
///
/// - Success: `{outcome: "success", configuration?, mergedConfiguration?}`
/// - Error:   `{outcome: "error", message, description}`
///
/// `containerId` is intentionally NOT included — the caller already knows it
/// (spec §16, "Result schema excludes container id" design decision).
#[derive(Debug, serde::Serialize)]
#[serde(untagged)]
enum SetUpResult {
    Success {
        outcome: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        configuration: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        merged_configuration: Option<serde_json::Value>,
    },
}

/// Execute the `set-up` command end-to-end.
///
/// On success: prints a single-line JSON document to stdout and returns
/// `Ok(())`. On error: propagates the error so the binary boundary maps it
/// to the spec's `{outcome: "error", ...}` JSON shape + exit code 1
/// (handled in `crates/deacon/src/main.rs` via the existing error path).
#[instrument(skip(args, runtime), fields(container_id = %args.container_id))]
pub async fn execute_set_up(args: SetUpArgs, runtime: Option<RuntimeKind>) -> Result<()> {
    info!("Starting set-up execution");

    // Phase 1: Validate --remote-env early (fail-fast per spec §9).
    parse_remote_env(&args.remote_env)?;

    // Select the runtime (docker/podman) honoring --runtime/DEACON_CONTAINER_RUNTIME.
    // Hardcoding CliDocker::new() here would inspect/exec via docker while the
    // container lives in podman → "Dev container not found" (mirrors up/exec/down).
    let docker = resolve_runtime(runtime, &args.docker_path).cli_docker();

    // Phase 2: Inspect the target container. Per spec §9, a missing container
    // produces the upstream-aligned summary "Dev container not found."
    let container = docker
        .inspect_container(&args.container_id)
        .await
        .with_context(|| format!("Failed to inspect container '{}'", args.container_id))?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Dev container not found.\nContainer id '{}' did not match a running container.",
                args.container_id
            )
        })?;

    info!(
        container_id = %container.id,
        image = %container.image,
        "Resolved target container for set-up"
    );

    // Phase 3: Load the optional --config and the image-metadata config.
    let base_config = load_optional_config(args.config_path.as_deref()).await?;
    let metadata_config = extract_image_metadata_config(&container)?;

    // Phase 4: Build the merged configuration. Per spec §4 the merge order is
    // `mergeConfiguration(config, imageMetadata)` — the in-file `--config`
    // wins over image metadata on scalar fields, lists are concatenated.
    let merged_config = merge_configs(&base_config, metadata_config.as_ref());

    // Phase 5: Pick the effective config for lifecycle execution. If
    // `--config` was provided, layer image-metadata on top; otherwise just
    // use the image-metadata config (or a default).
    let effective_config = merged_config.clone();

    // Phase 6: Variable substitution. Without a workspace we still need a
    // substitution context — use the current working directory as a
    // best-effort stand-in (spec §4 notes workspace placeholders are
    // typically not applicable to set-up).
    let cwd = std::env::current_dir().context("Failed to get current working directory")?;
    let substitution_context = SubstitutionContext::new(&cwd)?;

    let (substituted_config, _) =
        effective_config.apply_variable_substitution(&substitution_context);
    let (substituted_merged, _) = merged_config.apply_variable_substitution(&substitution_context);

    // Phase 7: System patches (spec §5 phase 3a). Best-effort per spec §9
    // — failure to write either /etc patch logs a WARN but does NOT abort
    // set-up. The shell scripts are guarded by per-file markers under
    // `--container-system-data-folder` (default `/var/devcontainer`) so
    // repeated set-up runs against the same container are no-ops.
    //
    // Skipped when --skip-post-create is set, mirroring upstream: post-create
    // is the conceptual umbrella for "user-customization work", which
    // includes the env patches.
    if !args.skip_post_create {
        apply_etc_patches(&args, &docker, &container, &substituted_merged).await;
    }

    // Phase 8: Lifecycle hook execution. Skipped entirely when
    // `--skip-post-create` is set (spec §2: "Skip all lifecycle hooks").
    if !args.skip_post_create {
        execute_lifecycle_hooks(
            &args,
            &container,
            &substituted_merged,
            &substitution_context,
            &docker,
        )
        .await?;
    } else {
        info!("--skip-post-create set; skipping /etc patches, all lifecycle hooks, and dotfiles");
    }

    // Phase 8: Emit JSON result on stdout (spec §10).
    let result = SetUpResult::Success {
        outcome: "success",
        configuration: args
            .include_configuration
            .then(|| serde_json::to_value(&substituted_config).unwrap_or(serde_json::Value::Null)),
        merged_configuration: args
            .include_merged_configuration
            .then(|| serde_json::to_value(&substituted_merged).unwrap_or(serde_json::Value::Null)),
    };
    let json = serde_json::to_string(&result).context("Failed to serialize set-up result")?;
    println!("{}", json);

    info!("set-up completed successfully");
    Ok(())
}

/// Parse `--remote-env name=value` entries with the upstream-aligned format
/// check (spec §9: "Invalid `--remote-env` format → argument validation error").
fn parse_remote_env(entries: &[String]) -> Result<Vec<(String, String)>> {
    let mut parsed = Vec::with_capacity(entries.len());
    for entry in entries {
        let (name, value) = entry.split_once('=').ok_or_else(|| {
            anyhow::anyhow!(
                "Invalid --remote-env format: '{}'. Expected '<name>=<value>'.",
                entry
            )
        })?;
        if name.is_empty() {
            return Err(anyhow::anyhow!(
                "Invalid --remote-env format: '{}'. Variable name must not be empty.",
                entry
            ));
        }
        parsed.push((name.to_string(), value.to_string()));
    }
    Ok(parsed)
}

/// Load an optional `--config` file via the shared `ConfigLoader` so the
/// extends chain is honored (per CLAUDE.md "use `ConfigLoader::load_with_extends`").
///
/// Returns a default `DevContainerConfig` when no path is provided.
async fn load_optional_config(path: Option<&std::path::Path>) -> Result<DevContainerConfig> {
    let Some(path) = path else {
        debug!("No --config provided; using empty base configuration");
        return Ok(DevContainerConfig::default());
    };

    if !path.exists() {
        return Err(anyhow::anyhow!(
            "Dev container config ({}) not found.",
            path.display()
        ));
    }

    use deacon_core::config::ConfigLoader;
    let resolved = ConfigLoader::load_with_extends(path)
        .await
        .with_context(|| {
            format!(
                "Failed to load devcontainer config from '{}'",
                path.display()
            )
        })?;
    Ok(resolved)
}

/// Extract a `DevContainerConfig` from the container's `devcontainer.metadata`
/// label. This label is a JSON array of metadata fragments that
/// `ConfigMerger` already knows how to fold together.
///
/// Per spec §4 the label's contents are the authoritative image-metadata
/// source. Missing label is NOT an error: many containers are not built by
/// `deacon up` and still benefit from set-up running lifecycle hooks against
/// the user's `--config`.
fn extract_image_metadata_config(container: &ContainerInfo) -> Result<Option<DevContainerConfig>> {
    let Some(label) = container.labels.get("devcontainer.metadata") else {
        debug!(
            container_id = %container.id,
            "Container has no devcontainer.metadata label; proceeding without image metadata"
        );
        return Ok(None);
    };

    let value: serde_json::Value = serde_json::from_str(label).with_context(|| {
        format!(
            "Failed to parse devcontainer.metadata label as JSON for container '{}'",
            container.id
        )
    })?;

    // PR-2 (#27) emits the label as a JSON array; tolerate both array and
    // single-object forms for older images built before that bump.
    let fragments: Vec<serde_json::Value> = match value {
        serde_json::Value::Array(arr) => arr,
        other => vec![other],
    };

    if fragments.is_empty() {
        return Ok(None);
    }

    let mut configs = Vec::with_capacity(fragments.len());
    for (idx, fragment) in fragments.into_iter().enumerate() {
        let cfg: DevContainerConfig = serde_json::from_value(fragment).with_context(|| {
            format!(
                "Failed to deserialize devcontainer.metadata fragment {} for container '{}'",
                idx, container.id
            )
        })?;
        configs.push(cfg);
    }

    let merged = deacon_core::config::ConfigMerger::merge_configs(&configs);
    Ok(Some(merged))
}

/// Merge `--config` (if any) on top of image metadata config (if any).
///
/// Order matters: per spec §4 `mergeConfiguration(config.config, imageMetadata)`
/// puts the file config FIRST so its scalar values win and its lists go
/// before image-metadata lists. `ConfigMerger::merge_configs` folds left to
/// right with later entries winning, so we pass `[metadata, file_config]` to
/// preserve that semantics.
fn merge_configs(
    file_config: &DevContainerConfig,
    metadata_config: Option<&DevContainerConfig>,
) -> DevContainerConfig {
    match metadata_config {
        Some(meta) => {
            deacon_core::config::ConfigMerger::merge_configs(&[meta.clone(), file_config.clone()])
        }
        None => file_config.clone(),
    }
}

/// Execute the lifecycle hooks against the target container. Mirrors
/// `run_user_commands::execute_lifecycle_commands` but:
///
/// - reads the container id from `args.container_id` (no workspace lookup);
/// - treats `--skip-post-create` as "skip everything" (the caller has
///   already short-circuited this function when that flag is set);
/// - takes the already-merged + substituted config so we don't redo the
///   substitution pass.
async fn execute_lifecycle_hooks(
    args: &SetUpArgs,
    container: &ContainerInfo,
    merged_config: &DevContainerConfig,
    substitution_context: &SubstitutionContext,
    cli: &CliRuntime,
) -> Result<()> {
    let remote_env_pairs = parse_remote_env(&args.remote_env)?;

    // CLI --remote-env overlays the config's remoteEnv map (CLI wins per
    // spec §3 normalization). The lifecycle helper consumes container_env
    // separately, so we fold the CLI env into the config's container_env
    // for the duration of this exec.
    let mut container_env = merged_config.container_env.clone();
    for (k, v) in &remote_env_pairs {
        container_env.insert(k.clone(), v.clone());
    }

    let container_workspace_folder = merged_config
        .workspace_folder
        .clone()
        .unwrap_or_else(|| "/".to_string());

    let lifecycle_config = ContainerLifecycleConfig {
        capture_output: false,
        container_id: container.id.clone(),
        user: merged_config
            .remote_user
            .clone()
            .or_else(|| merged_config.container_user.clone()),
        container_workspace_folder,
        container_env,
        // We've already gated all-skip in the caller; pass false here so the
        // lifecycle helper runs the individual phases it would normally run.
        skip_post_create: false,
        skip_non_blocking_commands: args.skip_non_blocking_commands,
        non_blocking_timeout: Duration::from_secs(300),
        use_login_shell: true,
        user_env_probe: deacon_core::container_env_probe::ContainerProbeMode::LoginShell,
        cache_folder: args.container_data_folder.clone(),
        force_pty: false,
        dotfiles: build_dotfiles_config(args),
        is_prebuild: false,
        config_hash: None,
    };

    let mut commands = ContainerLifecycleCommands::new();

    let parse_phase_command =
        |json_val: &serde_json::Value, phase: &str| -> Result<Option<LifecycleCommandList>> {
            let parsed = LifecycleCommandValue::from_json_value(json_val)
                .with_context(|| format!("Failed to parse {} command", phase))?;
            Ok(match parsed {
                Some(cmd) if !cmd.is_empty() => Some(LifecycleCommandList {
                    commands: vec![AggregatedLifecycleCommand {
                        command: cmd,
                        source: LifecycleCommandSource::Config,
                    }],
                }),
                _ => None,
            })
        };

    if let Some(ref on_create) = merged_config.on_create_command {
        if let Some(cmd) = parse_phase_command(on_create, "onCreateCommand")? {
            commands = commands.with_on_create(cmd);
        }
    }
    if let Some(ref update_content) = merged_config.update_content_command {
        if let Some(cmd) = parse_phase_command(update_content, "updateContentCommand")? {
            commands = commands.with_update_content(cmd);
        }
    }
    if let Some(ref post_create) = merged_config.post_create_command {
        if let Some(cmd) = parse_phase_command(post_create, "postCreateCommand")? {
            commands = commands.with_post_create(cmd);
        }
    }
    if !args.skip_non_blocking_commands {
        if let Some(ref post_start) = merged_config.post_start_command {
            if let Some(cmd) = parse_phase_command(post_start, "postStartCommand")? {
                commands = commands.with_post_start(cmd);
            }
        }
        if let Some(ref post_attach) = merged_config.post_attach_command {
            if let Some(cmd) = parse_phase_command(post_attach, "postAttachCommand")? {
                commands = commands.with_post_attach(cmd);
            }
        }
    }

    debug!("Executing lifecycle hooks in container {}", container.id);
    let result = execute_container_lifecycle_with_progress_callback_and_docker(
        &lifecycle_config,
        &commands,
        substitution_context,
        cli,
        Some(crate::commands::shared::progress::make_progress_callback(
            &args.progress_tracker,
        )),
    )
    .await
    .with_context(|| format!("Lifecycle execution failed in container '{}'", container.id))?;

    debug!(
        "Lifecycle execution completed: {} blocking phases, {} non-blocking phases queued",
        result.phases.len(),
        result.non_blocking_phases.len()
    );

    // Warn about anything that ran in best-effort fallback so the operator
    // can spot silently-skipped work in CI logs. Read this *before* moving
    // `result` into `execute_non_blocking_phases_sync_with_callback` below.
    if let Some(skipped) = result.phases.iter().find(|p| !p.success) {
        warn!(
            phase = ?skipped.phase,
            "Lifecycle phase did not complete successfully; further phases were aborted"
        );
    }

    // #73: actually execute the non-blocking phases (postStart, postAttach)
    // inside the container — not just log that we "would". The upstream
    // reference CLI runs them in the background before returning; deacon's
    // set-up previously stopped at the log line, so file side effects
    // (e.g. `/tmp/postStart.flag`) were never observable to callers.
    if !result.non_blocking_phases.is_empty() {
        debug!(
            "Executing {} non-blocking phase(s) synchronously",
            result.non_blocking_phases.len()
        );
        result
            .execute_non_blocking_phases_sync_with_callback(
                cli,
                Some(crate::commands::shared::progress::make_progress_callback(
                    &args.progress_tracker,
                )),
            )
            .await
            .with_context(|| {
                format!(
                    "Non-blocking lifecycle phase execution failed in container '{}'",
                    container.id
                )
            })?;
    }

    Ok(())
}

/// Default location for root-owned marker files inside the container.
/// Matches the upstream `devcontainers/cli` convention and the spec's §6
/// default for `--container-system-data-folder`.
const DEFAULT_CONTAINER_SYSTEM_DATA_FOLDER: &str = "/var/devcontainer";

/// Delimiter marking the start of deacon's appended block in `/etc/*` files.
/// MUST appear on its own line so re-running set-up can detect it cheaply.
const ETC_BLOCK_BEGIN: &str = "# >>> deacon set-up >>>";
/// Delimiter marking the end of deacon's appended block.
const ETC_BLOCK_END: &str = "# <<< deacon set-up <<<";

/// Apply the spec-§5 phase 3a system patches against the live container.
///
/// Both patches are guarded by marker files under
/// `--container-system-data-folder` (default `/var/devcontainer`); a second
/// invocation against the same container is a no-op. Per spec §9 the patches
/// are best-effort — any failure (no root, read-only `/etc`, etc.) emits a
/// WARN and proceeds so that set-up still runs lifecycle hooks against
/// containers we can't fully personalize.
async fn apply_etc_patches<D: Docker>(
    args: &SetUpArgs,
    docker: &D,
    container: &ContainerInfo,
    merged_config: &DevContainerConfig,
) {
    let env_pairs = collect_env_pairs(args, merged_config);
    let system_data_folder = args
        .container_system_data_folder
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONTAINER_SYSTEM_DATA_FOLDER));
    let system_data_folder_str = system_data_folder.to_string_lossy().to_string();

    let environment_script = build_etc_environment_patch_script(
        &env_pairs,
        &format!("{}/.patchEtcEnvironmentMarker", system_data_folder_str),
        &system_data_folder_str,
    );
    if let Err(err) = run_root_shell(docker, &container.id, &environment_script).await {
        warn!(
            container_id = %container.id,
            error = %err,
            "Best-effort patch of /etc/environment failed; continuing without it"
        );
    }

    let profile_script = build_etc_profile_patch_script(
        &format!("{}/.patchEtcProfileMarker", system_data_folder_str),
        &system_data_folder_str,
    );
    if let Err(err) = run_root_shell(docker, &container.id, &profile_script).await {
        warn!(
            container_id = %container.id,
            error = %err,
            "Best-effort patch of /etc/profile failed; continuing without it"
        );
    }
}

/// Collect the env pairs that should be appended to `/etc/environment`.
///
/// Merges (in this order):
/// 1. The merged config's `containerEnv` map.
/// 2. The merged config's `remoteEnv` map (where the value is `Some`).
/// 3. The CLI `--remote-env` overlays (CLI wins).
///
/// Returned as a vector sorted by key so the appended block is deterministic
/// across runs — important for the marker-driven idempotency check.
fn collect_env_pairs(
    args: &SetUpArgs,
    merged_config: &DevContainerConfig,
) -> Vec<(String, String)> {
    let mut env: HashMap<String, String> = HashMap::new();
    for (k, v) in &merged_config.container_env {
        env.insert(k.clone(), v.clone());
    }
    for (k, v) in &merged_config.remote_env {
        if let Some(value) = v {
            env.insert(k.clone(), value.clone());
        }
    }
    if let Ok(cli_pairs) = parse_remote_env(&args.remote_env) {
        for (k, v) in cli_pairs {
            env.insert(k, v);
        }
    }
    let mut pairs: Vec<(String, String)> = env.into_iter().collect();
    pairs.sort();
    pairs
}

/// Build the shell script that patches `/etc/environment`.
///
/// The script is idempotent: it short-circuits when the marker file already
/// exists. When it runs it writes a delimited block of `KEY="VALUE"` lines,
/// preceded by the literal lines `ETC_BLOCK_BEGIN` and followed by
/// `ETC_BLOCK_END`, then touches the marker file. Empty env-pair lists
/// result in a no-op (no block written, no marker touched).
fn build_etc_environment_patch_script(
    env_pairs: &[(String, String)],
    marker_path: &str,
    system_data_folder: &str,
) -> String {
    if env_pairs.is_empty() {
        // Nothing to patch — skip cleanly so an empty config doesn't even
        // touch the marker. Re-running with a populated config will still
        // perform the patch on the next invocation.
        return "exit 0".to_string();
    }

    let mut lines = String::new();
    lines.push_str(ETC_BLOCK_BEGIN);
    lines.push('\n');
    for (k, v) in env_pairs {
        // Escape backslash and double-quote so the value parses as a
        // standard `KEY="VALUE"` line that `/etc/environment` consumers
        // (PAM, systemd-environd) understand.
        let escaped = v.replace('\\', r"\\").replace('"', r#"\""#);
        lines.push_str(&format!("{}=\"{}\"\n", k, escaped));
    }
    lines.push_str(ETC_BLOCK_END);
    lines.push('\n');

    // The outer shell wrapper:
    // - Bails out if the marker is present (idempotency).
    // - Creates the system data folder so the touch on the marker succeeds
    //   on fresh containers that don't ship with it.
    // - Uses a heredoc to append the block atomically — no intermediate
    //   temp file required.
    format!(
        "#!/bin/sh\nset -e\nif [ -f '{marker}' ]; then exit 0; fi\nmkdir -p '{sysdir}'\ncat >> /etc/environment <<'DEACON_ETC_ENV_EOF'\n{lines}DEACON_ETC_ENV_EOF\ntouch '{marker}'\n",
        marker = marker_path,
        sysdir = system_data_folder,
        lines = lines,
    )
}

/// Build the shell script that patches `/etc/profile`.
///
/// Appends a one-time block that re-exports the PATH from
/// `/etc/environment` so login shells inherit any PATH segments that
/// `/etc/environment` adds. The marker guards against repeated execution.
fn build_etc_profile_patch_script(marker_path: &str, system_data_folder: &str) -> String {
    let block = format!(
        "{begin}\n# Re-export PATH from /etc/environment so login shells inherit deacon-managed PATH segments.\nif [ -f /etc/environment ]; then\n  while IFS='=' read -r key value; do\n    case \"$key\" in\n      PATH) export PATH=\"$(printf '%s' \"$value\" | sed -e 's/^\"//' -e 's/\"$//')\" ;;\n    esac\n  done < /etc/environment\nfi\n{end}\n",
        begin = ETC_BLOCK_BEGIN,
        end = ETC_BLOCK_END,
    );

    format!(
        "#!/bin/sh\nset -e\nif [ -f '{marker}' ]; then exit 0; fi\nmkdir -p '{sysdir}'\ncat >> /etc/profile <<'DEACON_ETC_PROFILE_EOF'\n{block}DEACON_ETC_PROFILE_EOF\ntouch '{marker}'\n",
        marker = marker_path,
        sysdir = system_data_folder,
        block = block,
    )
}

/// Run a script in the container as root via `sh -c`. Returns an error when
/// the exec command itself fails OR when the script exits non-zero — the
/// caller decides whether that's fatal or best-effort.
async fn run_root_shell<D: Docker>(docker: &D, container_id: &str, script: &str) -> Result<()> {
    let exec_config = ExecConfig {
        user: Some("root".to_string()),
        working_dir: None,
        env: HashMap::new(),
        tty: false,
        interactive: false,
        detach: false,
        // Patches are noisy on first run (mkdir, touch, cat); suppress stdout
        // so set-up's JSON output stays clean. The lifecycle helper handles
        // its own streaming separately.
        silent: true,
        stdout_to_stderr: false,
        terminal_size: None,
    };
    let result = docker
        .exec(
            container_id,
            &["sh".to_string(), "-c".to_string(), script.to_string()],
            exec_config,
        )
        .await
        .with_context(|| format!("docker exec failed against container '{}'", container_id))?;

    if !result.success {
        return Err(anyhow::anyhow!(
            "Patch script exited {} (stderr: {})",
            result.exit_code,
            result.stderr.trim()
        ));
    }
    Ok(())
}

/// Build a `DotfilesConfig` from set-up CLI args.
///
/// Returns `None` (which short-circuits the lifecycle helper's dotfiles step)
/// when no repository is supplied — set-up should never clone without an
/// explicit user opt-in. `target_path` and `install_command` are forwarded
/// as-is; the lifecycle helper computes sensible defaults when they're `None`.
///
/// Per spec §6, idempotency is enforced by a marker file at the target path
/// (handled inside `container_lifecycle::execute_dotfiles_in_container`); we
/// do not need to track that here.
fn build_dotfiles_config(args: &SetUpArgs) -> Option<DotfilesConfig> {
    args.dotfiles_repository
        .as_ref()
        .map(|repo| DotfilesConfig {
            repository: Some(repo.clone()),
            target_path: args.dotfiles_target_path.clone(),
            install_command: args.dotfiles_install_command.clone(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn empty_progress_tracker() -> Arc<Mutex<Option<deacon_core::progress::ProgressTracker>>> {
        Arc::new(Mutex::new(None))
    }

    /// Build a `ContainerInfo` fixture with sensible defaults. Keeps the
    /// individual tests focused on the field they actually care about
    /// (labels for metadata-extraction tests).
    fn make_container(id: &str, image: &str, labels: HashMap<String, String>) -> ContainerInfo {
        ContainerInfo {
            id: id.to_string(),
            names: vec![],
            image: image.to_string(),
            status: "running".to_string(),
            state: "running".to_string(),
            exposed_ports: vec![],
            port_mappings: vec![],
            env: HashMap::new(),
            labels,
            mounts: vec![],
        }
    }

    fn make_args(container_id: &str) -> SetUpArgs {
        SetUpArgs {
            container_id: container_id.to_string(),
            config_path: None,
            skip_post_create: false,
            skip_non_blocking_commands: false,
            remote_env: vec![],
            dotfiles_repository: None,
            dotfiles_install_command: None,
            dotfiles_target_path: None,
            include_configuration: false,
            include_merged_configuration: false,
            container_data_folder: None,
            container_system_data_folder: None,
            docker_path: "docker".to_string(),
            progress_tracker: empty_progress_tracker(),
        }
    }

    #[test]
    fn parse_remote_env_accepts_name_equals_value() {
        let parsed = parse_remote_env(&["FOO=bar".to_string(), "BAZ=qux=1".to_string()]).unwrap();
        assert_eq!(
            parsed,
            vec![
                ("FOO".to_string(), "bar".to_string()),
                ("BAZ".to_string(), "qux=1".to_string()),
            ]
        );
    }

    #[test]
    fn parse_remote_env_rejects_missing_equals() {
        let err = parse_remote_env(&["NO_EQUALS".to_string()]).unwrap_err();
        assert!(
            err.to_string().contains("Invalid --remote-env format"),
            "got: {err}"
        );
    }

    #[test]
    fn parse_remote_env_rejects_empty_name() {
        let err = parse_remote_env(&["=value".to_string()]).unwrap_err();
        assert!(
            err.to_string().contains("Variable name must not be empty"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn load_optional_config_returns_default_when_none() {
        let cfg = load_optional_config(None).await.unwrap();
        assert!(cfg.name.is_none());
        assert!(cfg.image.is_none());
    }

    #[tokio::test]
    async fn load_optional_config_errors_on_missing_path() {
        let bogus = std::path::Path::new("/tmp/definitely-does-not-exist/devcontainer.json");
        let err = load_optional_config(Some(bogus)).await.unwrap_err();
        // Spec §9: "Dev container config (<path>) not found."
        assert!(err.to_string().contains("Dev container config"));
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn extract_image_metadata_tolerates_missing_label() {
        let container = make_container("abc", "alpine:3.18", HashMap::new());
        let result = extract_image_metadata_config(&container).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn extract_image_metadata_parses_array_form() {
        // PR-2 (#27) emits the label as a JSON array of metadata fragments.
        let mut labels = HashMap::new();
        labels.insert(
            "devcontainer.metadata".to_string(),
            r#"[{"remoteUser":"vscode"},{"containerEnv":{"FOO":"bar"}}]"#.to_string(),
        );
        let container = make_container("abc", "alpine:3.18", labels);
        let cfg = extract_image_metadata_config(&container).unwrap().unwrap();
        assert_eq!(cfg.remote_user.as_deref(), Some("vscode"));
        assert_eq!(
            cfg.container_env.get("FOO").map(|s| s.as_str()),
            Some("bar")
        );
    }

    #[test]
    fn extract_image_metadata_tolerates_single_object_form() {
        // Older images may write a single object (pre-PR-2 reader-tolerance).
        let mut labels = HashMap::new();
        labels.insert(
            "devcontainer.metadata".to_string(),
            r#"{"remoteUser":"node"}"#.to_string(),
        );
        let container = make_container("abc", "node:20", labels);
        let cfg = extract_image_metadata_config(&container).unwrap().unwrap();
        assert_eq!(cfg.remote_user.as_deref(), Some("node"));
    }

    #[test]
    fn extract_image_metadata_rejects_invalid_json() {
        let mut labels = HashMap::new();
        labels.insert(
            "devcontainer.metadata".to_string(),
            "this is not json".to_string(),
        );
        let container = make_container("abc", "alpine:3.18", labels);
        let err = extract_image_metadata_config(&container).unwrap_err();
        assert!(err.to_string().contains("devcontainer.metadata"));
    }

    #[test]
    fn merge_configs_with_metadata_overlays_correctly() {
        let file_cfg = DevContainerConfig {
            remote_user: Some("file-user".to_string()),
            ..DevContainerConfig::default()
        };

        let mut meta_cfg = DevContainerConfig {
            remote_user: Some("meta-user".to_string()),
            ..DevContainerConfig::default()
        };
        meta_cfg
            .container_env
            .insert("META_VAR".to_string(), "meta".to_string());

        // Per spec §4: file config wins over metadata on scalar fields.
        let merged = merge_configs(&file_cfg, Some(&meta_cfg));
        assert_eq!(merged.remote_user.as_deref(), Some("file-user"));
        // Metadata env still flows through via the merger's map overlay.
        assert_eq!(
            merged.container_env.get("META_VAR").map(|s| s.as_str()),
            Some("meta")
        );
    }

    #[test]
    fn merge_configs_returns_file_config_when_no_metadata() {
        let file_cfg = DevContainerConfig {
            name: Some("only-file".to_string()),
            ..DevContainerConfig::default()
        };
        let merged = merge_configs(&file_cfg, None);
        assert_eq!(merged.name.as_deref(), Some("only-file"));
    }

    #[test]
    fn args_have_sensible_defaults() {
        let args = make_args("abc123");
        assert_eq!(args.container_id, "abc123");
        assert!(!args.skip_post_create);
        assert!(!args.skip_non_blocking_commands);
        assert!(args.config_path.is_none());
        assert!(args.remote_env.is_empty());
    }

    // =========================================================================
    // /etc patch builders (PR-6c)
    // =========================================================================

    #[test]
    fn build_etc_environment_patch_returns_noop_when_env_empty() {
        // Empty env → no block to write. Returning `exit 0` keeps the
        // exec a no-op without touching the marker, so a later run with a
        // populated env still performs the patch.
        let script = build_etc_environment_patch_script(
            &[],
            "/var/devcontainer/.patchEtcEnvironmentMarker",
            "/var/devcontainer",
        );
        assert_eq!(script, "exit 0");
    }

    #[test]
    fn build_etc_environment_patch_short_circuits_when_marker_present() {
        // The script's outer `if -f marker` guard is the idempotency anchor
        // — without it, re-running set-up would duplicate the env block.
        let env = vec![("FOO".to_string(), "bar".to_string())];
        let script = build_etc_environment_patch_script(
            &env,
            "/var/devcontainer/.patchEtcEnvironmentMarker",
            "/var/devcontainer",
        );
        assert!(
            script
                .contains("if [ -f '/var/devcontainer/.patchEtcEnvironmentMarker' ]; then exit 0"),
            "expected marker guard in script, got: {}",
            script
        );
    }

    #[test]
    fn build_etc_environment_patch_writes_sorted_env_block() {
        // Sorted-by-key output is what makes the block byte-stable across
        // runs — a prerequisite for any future "did we already patch this?"
        // content check. The caller passes a pre-sorted slice; we just
        // verify the script preserves order.
        let env = vec![
            ("ALPHA".to_string(), "1".to_string()),
            ("BETA".to_string(), "2".to_string()),
        ];
        let script = build_etc_environment_patch_script(
            &env,
            "/var/devcontainer/.patchEtcEnvironmentMarker",
            "/var/devcontainer",
        );
        let alpha_pos = script.find("ALPHA=\"1\"").expect("ALPHA missing");
        let beta_pos = script.find("BETA=\"2\"").expect("BETA missing");
        assert!(alpha_pos < beta_pos, "env entries must appear in order");
    }

    #[test]
    fn build_etc_environment_patch_wraps_block_in_delimiters() {
        // Future tooling needs to find/replace deacon's block without
        // touching user-managed lines; the delimiters are the seam.
        let env = vec![("X".to_string(), "y".to_string())];
        let script = build_etc_environment_patch_script(
            &env,
            "/var/devcontainer/.patchEtcEnvironmentMarker",
            "/var/devcontainer",
        );
        assert!(script.contains(ETC_BLOCK_BEGIN));
        assert!(script.contains(ETC_BLOCK_END));
    }

    #[test]
    fn build_etc_environment_patch_escapes_special_chars_in_values() {
        // `/etc/environment` is a PAM-style `KEY="VALUE"` file; embedded
        // double-quotes and backslashes break the parser when not escaped.
        let env = vec![(
            "MIX".to_string(),
            r#"quoted "literal" with \backslash"#.to_string(),
        )];
        let script = build_etc_environment_patch_script(
            &env,
            "/var/devcontainer/.patchEtcEnvironmentMarker",
            "/var/devcontainer",
        );
        // The literal `\` and `"` characters should be escaped in the
        // emitted line. We check for the escaped form rather than asserting
        // the exact post-substitution string so the test stays robust to
        // formatter changes.
        assert!(
            script.contains(r#"MIX="quoted \"literal\" with \\backslash""#),
            "expected escaped value in script, got: {}",
            script
        );
    }

    #[test]
    fn build_etc_profile_patch_short_circuits_on_marker() {
        let script = build_etc_profile_patch_script(
            "/var/devcontainer/.patchEtcProfileMarker",
            "/var/devcontainer",
        );
        assert!(
            script.contains("if [ -f '/var/devcontainer/.patchEtcProfileMarker' ]; then exit 0")
        );
    }

    #[test]
    fn build_etc_profile_patch_reexports_path_from_environment() {
        // The whole point of patching /etc/profile is to make login shells
        // inherit `/etc/environment`'s PATH; if we don't re-export PATH, the
        // patch is useless.
        let script = build_etc_profile_patch_script(
            "/var/devcontainer/.patchEtcProfileMarker",
            "/var/devcontainer",
        );
        assert!(script.contains("export PATH="));
        assert!(script.contains("/etc/environment"));
    }

    #[test]
    fn build_etc_profile_patch_wraps_in_delimiters() {
        let script = build_etc_profile_patch_script(
            "/var/devcontainer/.patchEtcProfileMarker",
            "/var/devcontainer",
        );
        assert!(script.contains(ETC_BLOCK_BEGIN));
        assert!(script.contains(ETC_BLOCK_END));
    }

    #[test]
    fn collect_env_pairs_merges_config_remote_and_cli() {
        // Spec §3 + §5 expect set-up to overlay container_env, remote_env,
        // and CLI --remote-env (CLI last so it wins). Verify all three
        // sources surface, with CLI overriding any conflicting key.
        let merged = DevContainerConfig {
            container_env: {
                let mut m = std::collections::HashMap::new();
                m.insert("FROM_CONTAINER".to_string(), "c".to_string());
                m.insert("OVERRIDDEN".to_string(), "from-config".to_string());
                m
            },
            remote_env: {
                let mut m = std::collections::HashMap::new();
                m.insert("FROM_REMOTE".to_string(), Some("r".to_string()));
                m.insert("DROPPED".to_string(), None); // None-valued keys are skipped
                m
            },
            ..DevContainerConfig::default()
        };
        let mut args = make_args("abc");
        args.remote_env = vec![
            "FROM_CLI=cli".to_string(),
            "OVERRIDDEN=from-cli".to_string(),
        ];
        let pairs = collect_env_pairs(&args, &merged);
        let map: std::collections::HashMap<_, _> = pairs.iter().cloned().collect();

        assert_eq!(map.get("FROM_CONTAINER").map(|s| s.as_str()), Some("c"));
        assert_eq!(map.get("FROM_REMOTE").map(|s| s.as_str()), Some("r"));
        assert_eq!(map.get("FROM_CLI").map(|s| s.as_str()), Some("cli"));
        assert_eq!(
            map.get("OVERRIDDEN").map(|s| s.as_str()),
            Some("from-cli"),
            "CLI --remote-env must win over config containerEnv on key conflicts"
        );
        assert!(
            !map.contains_key("DROPPED"),
            "remote_env entries with None values must be excluded"
        );
    }

    #[test]
    fn collect_env_pairs_returns_sorted_output() {
        // Sorted output is what gives the appended block its byte-stable
        // form; the order matters for any future "did we already patch this
        // exact env?" check.
        let merged = DevContainerConfig {
            container_env: {
                let mut m = std::collections::HashMap::new();
                m.insert("ZED".to_string(), "z".to_string());
                m.insert("ALPHA".to_string(), "a".to_string());
                m.insert("MID".to_string(), "m".to_string());
                m
            },
            ..DevContainerConfig::default()
        };
        let args = make_args("abc");
        let pairs = collect_env_pairs(&args, &merged);
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["ALPHA", "MID", "ZED"]);
    }

    #[test]
    fn success_result_serializes_outcome_field() {
        // Spec §10: stdout JSON must carry an `outcome` field.
        let result = SetUpResult::Success {
            outcome: "success",
            configuration: None,
            merged_configuration: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"outcome\""));
        assert!(json.contains("\"success\""));
        // Optional fields are omitted when None.
        assert!(!json.contains("\"configuration\""));
        assert!(!json.contains("\"mergedConfiguration\""));
    }

    #[test]
    fn build_dotfiles_config_returns_none_when_no_repository() {
        // No --dotfiles-repository means no opt-in: the lifecycle helper
        // must NOT clone anything, even if the other dotfiles flags are set.
        let mut args = make_args("abc");
        args.dotfiles_install_command = Some("./install.sh".to_string());
        args.dotfiles_target_path = Some("/tmp/dotfiles".to_string());
        assert!(build_dotfiles_config(&args).is_none());
    }

    #[test]
    fn build_dotfiles_config_forwards_all_three_fields() {
        let mut args = make_args("abc");
        args.dotfiles_repository = Some("octocat/dotfiles".to_string());
        args.dotfiles_install_command = Some("./bootstrap.sh".to_string());
        args.dotfiles_target_path = Some("/workspaces/dotfiles".to_string());

        let cfg = build_dotfiles_config(&args).expect("repository set; config must be Some");
        assert_eq!(cfg.repository.as_deref(), Some("octocat/dotfiles"));
        assert_eq!(cfg.install_command.as_deref(), Some("./bootstrap.sh"));
        assert_eq!(cfg.target_path.as_deref(), Some("/workspaces/dotfiles"));
    }

    #[test]
    fn build_dotfiles_config_leaves_defaults_to_lifecycle_helper() {
        // When only --dotfiles-repository is set, target_path and
        // install_command must stay None so the lifecycle helper computes
        // its standard defaults (target = ~/dotfiles, install auto-detected).
        let mut args = make_args("abc");
        args.dotfiles_repository = Some("https://github.com/octocat/dotfiles.git".to_string());

        let cfg = build_dotfiles_config(&args).unwrap();
        assert!(cfg.target_path.is_none());
        assert!(cfg.install_command.is_none());
        assert!(cfg.is_configured());
    }

    #[test]
    fn success_result_includes_optional_fields_when_set() {
        let result = SetUpResult::Success {
            outcome: "success",
            configuration: Some(serde_json::json!({"name": "test"})),
            merged_configuration: Some(serde_json::json!({"name": "test"})),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"configuration\""));
        assert!(
            json.contains("\"merged_configuration\"") || json.contains("\"mergedConfiguration\"")
        );
    }
}
