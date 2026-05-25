//! Set-up subcommand implementation.
//!
//! `set-up` converts an already-running container into a DevContainer by
//! applying configuration + image metadata, executing lifecycle hooks, and
//! returning a JSON snapshot of the (optionally merged) configuration.
//!
//! See `docs/subcommand-specs/set-up/SPEC.md` for the authoritative behavior.
//!
//! ## Scope (PR-6a + PR-6b)
//!
//! - `--container-id` resolution + inspect validation
//! - Optional `--config` load via the shared `ConfigLoader` (extends-aware)
//! - Image-metadata extraction from the container's `devcontainer.metadata`
//!   label and merge with the parsed config
//! - Variable substitution (config + merged config)
//! - Lifecycle hook execution (`onCreate` → `updateContent` → `postCreate` →
//!   `postStart` → `postAttach`) via the shared `ContainerLifecycle` helper,
//!   gated by `--skip-post-create` and `--skip-non-blocking-commands`
//! - **Dotfiles installer** (`--dotfiles-repository` / `--dotfiles-install-command`
//!   / `--dotfiles-target-path`) via `ContainerLifecycle`'s built-in clone +
//!   auto-detect installer + target-path marker (PR-6b)
//! - JSON output on stdout: `{outcome, configuration?, mergedConfiguration?}`
//!
//! ## Deferred to PR-6c
//!
//! - `/etc/environment` + `/etc/profile` root-side patches with system markers
//!   under `/var/devcontainer/`
//! - A second substitution pass against the live container environment
//!   (`${containerEnv:VAR}`) — current pass uses only the configured
//!   `container_env`, not the live `docker exec` env probe

use anyhow::{Context, Result};
use deacon_core::config::DevContainerConfig;
use deacon_core::container_lifecycle::{
    execute_container_lifecycle_with_progress_callback, AggregatedLifecycleCommand,
    ContainerLifecycleCommands, ContainerLifecycleConfig, DotfilesConfig, LifecycleCommandList,
    LifecycleCommandSource, LifecycleCommandValue,
};
use deacon_core::docker::{CliDocker, ContainerInfo, Docker};
use deacon_core::variable::SubstitutionContext;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

/// Arguments for the `set-up` command.
///
/// Mirrors the spec's CLI surface (`docs/subcommand-specs/set-up/SPEC.md` §2)
/// minus the `--container-system-data-folder` flag, which is only consumed
/// by the `/etc` root-patch path (deferred to PR-6c).
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
    /// Docker CLI path; defaults to `"docker"`. Currently plumbed for shape
    /// parity with `run-user-commands`; `CliDocker::new()` uses the binary
    /// resolution baked into the runtime layer.
    #[allow(dead_code)]
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
#[instrument(skip(args), fields(container_id = %args.container_id))]
pub async fn execute_set_up(args: SetUpArgs) -> Result<()> {
    info!("Starting set-up execution");

    // Phase 1: Validate --remote-env early (fail-fast per spec §9).
    parse_remote_env(&args.remote_env)?;

    // Phase 2: Inspect the target container. Per spec §9, a missing container
    // produces the upstream-aligned summary "Dev container not found."
    let docker = CliDocker::new();
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
    let base_config = load_optional_config(args.config_path.as_deref())?;
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

    // Phase 7: Lifecycle hook execution. Skipped entirely when
    // `--skip-post-create` is set (spec §2: "Skip all lifecycle hooks").
    if !args.skip_post_create {
        execute_lifecycle_hooks(
            &args,
            &container,
            &substituted_merged,
            &substitution_context,
        )
        .await?;
    } else {
        info!("--skip-post-create set; skipping all lifecycle hooks and dotfiles");
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
fn load_optional_config(path: Option<&std::path::Path>) -> Result<DevContainerConfig> {
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
    let resolved = ConfigLoader::load_with_extends(path).with_context(|| {
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
    let result = execute_container_lifecycle_with_progress_callback(
        &lifecycle_config,
        &commands,
        substitution_context,
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
    result.log_non_blocking_phases();

    // Warn about anything that ran in best-effort fallback so the operator
    // can spot silently-skipped work in CI logs.
    if let Some(skipped) = result.phases.iter().find(|p| !p.success) {
        warn!(
            phase = ?skipped.phase,
            "Lifecycle phase did not complete successfully; further phases were aborted"
        );
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

    #[test]
    fn load_optional_config_returns_default_when_none() {
        let cfg = load_optional_config(None).unwrap();
        assert!(cfg.name.is_none());
        assert!(cfg.image.is_none());
    }

    #[test]
    fn load_optional_config_errors_on_missing_path() {
        let bogus = std::path::Path::new("/tmp/definitely-does-not-exist/devcontainer.json");
        let err = load_optional_config(Some(bogus)).unwrap_err();
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
