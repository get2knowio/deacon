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
//! - A container-aware substitution pass over the REPORTED blocks
//!   (`${containerEnv:VAR}` against the adopted container's `Config.Env`) — the
//!   reference's `containerSubstitute`, shared with `up` (#616)
//! - JSON output on stdout: `{outcome, configuration?, mergedConfiguration?}`

use crate::commands::shared::resolve_runtime;
use anyhow::{Context, Result};
use deacon_core::config::DevContainerConfig;
use deacon_core::container_lifecycle::{
    ContainerLifecycleCommands, ContainerLifecycleConfig, DotfilesConfig, LifecycleCommandList,
    aggregate_lifecycle_commands, execute_container_lifecycle_with_progress_callback_and_docker,
};
use deacon_core::docker::{CliRuntime, ContainerInfo, Docker, ExecConfig};
use deacon_core::lifecycle::LifecyclePhase;
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
    /// Host user-data folder (`--user-data-folder`); `None` → `~/.deacon`.
    /// Roots lifecycle markers outside the project (#280).
    pub user_data_folder: Option<PathBuf>,
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
        #[serde(
            rename = "mergedConfiguration",
            skip_serializing_if = "Option::is_none"
        )]
        merged_configuration: Option<serde_json::Value>,
    },
}

/// Shape the `configuration` block: the `--config` document the caller supplied, AS
/// AUTHORED (after variable substitution), plus its `configFilePath` (#502).
///
/// The authored-vs-resolved split is the whole point. `configuration` answers "what did
/// the caller hand me?" and `mergedConfiguration` answers "what did I end up running?" —
/// the same contract `up`'s and `read-configuration`'s blocks keep (#490/#407). deacon used
/// to pass the metadata-merged config here, so a `remoteUser` or `containerEnv` that lived
/// only on the container's `devcontainer.metadata` label appeared in the block as though
/// the caller had written it, and the `configFilePath` the reference emits was missing
/// entirely.
///
/// `configFilePath`'s scheme is `file`, and that falls out of the existing rule rather than
/// being a special case: `read_configuration::config_file_path_value` keys the scheme off
/// whether the CALLER NAMED the file, and a `set-up` config is only ever one it was given.
/// With no `--config` there is no config file, so no `configFilePath` — and the block is the
/// empty document, which is exactly what the reference emits there (`"configuration": {}`).
fn configuration_document(
    config: &DevContainerConfig,
    config_path: Option<&std::path::Path>,
) -> Result<serde_json::Value> {
    let mut document =
        serde_json::to_value(config).context("Failed to serialize set-up configuration")?;
    if let Some(path) = config_path {
        crate::commands::read_configuration::insert_config_file_path(&mut document, path, true);
    }
    Ok(document)
}

/// Shape the merged configuration into upstream's `mergeConfiguration` OUTPUT form —
/// the same document `read-configuration --include-merged-configuration` emits (#483).
///
/// deacon used to serialize the flat [`DevContainerConfig`] here, which got three things
/// wrong at once: the key was snake_case, the content was the flat config rather than the
/// merged shape, and the five lifecycle hooks appeared as SINGULAR fields instead of the
/// collected plural arrays the reference reports. All three are fixed by routing this
/// block through `read_configuration`'s existing pipeline rather than a second serializer.
///
/// The collected entries are ordered exactly as
/// [`aggregate_lifecycle_commands`] replays them, which is what makes the reported arrays
/// agree with what set-up actually RAN: the container label's fragments (carried on
/// [`DevContainerConfig::metadata_lifecycle_layers`] since #477) first, in label order,
/// then the `--config` file's own hooks last — "the devcontainer.json is considered last"
/// (spec image-metadata Merge Logic).
///
/// `configFilePath` rides on the block for the same reason it rides on
/// `read-configuration`'s (#376): the reference emits it and a consumer already knows how to
/// unmarshal the VS Code URI shape. Its `scheme` is `file` here rather than
/// `vscode-fileHost`, and that is not a special case — `config_file_path_value` already
/// keys the scheme off whether the CALLER NAMED the file, and `set-up` only ever has a
/// `--config` it was given. A `set-up` with no `--config` has no config file, and the
/// reference emits no `configFilePath` for it either.
///
/// `customizations` is the one property whose reported shape the merge cannot produce
/// (#532). Upstream's `mergeConfiguration` DELETES it from the base config and rebuilds it
/// as one array slot per contributing metadata entry, keyed by tool —
/// `{"vscode": [{…entry0}, {…entry1}]}` — leaving each tool to reconcile its own slots.
/// deacon reported the deep-merged object instead, which reads as a single contributor and
/// silently resolves conflicts (two entries setting the same VS Code setting collapsed to
/// the later one) that the reference hands to the tool intact. The entries are ordered
/// `[…label fragments in label order, --config]`, which is upstream's `Tt`:
/// `[...labelEntries, pick(config, pickConfigProperties)]` — and `base_config` is therefore
/// the `--config` document ALONE, not the merged one, whose `customizations` already
/// contains the label's.
///
/// Reuses `read-configuration`'s `apply_customizations_shape` rather than growing a second
/// one; the reason set-up never reached it is that it routed through
/// `apply_upstream_merge_shape` only, and `ConfigMerger` had already collapsed the
/// fragments by then. The per-fragment objects now ride down on
/// [`DevContainerConfig::metadata_customizations_layers`], the same carrier
/// [`DevContainerConfig::metadata_lifecycle_layers`] gave the hooks in #477.
fn merged_configuration_document(
    merged: &DevContainerConfig,
    base_config: &DevContainerConfig,
    config_path: Option<&std::path::Path>,
) -> Result<serde_json::Value> {
    use crate::commands::read_configuration::{
        apply_customizations_shape, apply_upstream_merge_shape, collect_entry_from_config_json,
        insert_config_file_path, normalize_merged_configuration_shape,
    };

    let merged_json =
        serde_json::to_value(merged).context("Failed to serialize merged configuration")?;

    let mut entries: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();
    for layer in &merged.metadata_lifecycle_layers {
        // Round-trip the layer through a bare config so the entry is built by the SAME
        // collector the base config uses — one definition of "which fields are collected".
        let mut as_config = DevContainerConfig::default();
        layer.clone().apply_to(&mut as_config);
        let entry = collect_entry_from_config_json(
            &serde_json::to_value(&as_config)
                .context("Failed to serialize metadata lifecycle layer")?,
        );
        if !entry.is_empty() {
            entries.push(entry);
        }
    }
    let base_entry = collect_entry_from_config_json(&merged_json);
    if !base_entry.is_empty() {
        entries.push(base_entry);
    }

    // `[…label fragments in label order, --config]`. A contributor that authored an
    // empty object is not a contributor: upstream's `for (let u in c.customizations)`
    // adds no key for one, so an authored `"customizations": {}` must not create a slot.
    let mut customizations_entries: Vec<serde_json::Value> =
        merged.metadata_customizations_layers.clone();
    if let Some(value) = &base_config.customizations {
        if value.as_object().is_some_and(|map| !map.is_empty()) {
            customizations_entries.push(value.clone());
        }
    }

    let mut shaped = apply_upstream_merge_shape(merged_json, &entries);
    shaped = apply_customizations_shape(shaped, &customizations_entries);
    normalize_merged_configuration_shape(&mut shaped);
    if let Some(path) = config_path {
        insert_config_file_path(&mut shaped, path, true);
    }
    Ok(shaped)
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
    // wins over image metadata on scalar fields, lists are concatenated. The
    // metadata side contributes only `METADATA_MERGE_PROPERTIES`, the enumerated
    // list upstream's `mergeConfiguration` reads off a metadata entry (#526).
    let merged_config = merge_configs(&base_config, metadata_config.as_ref())?;

    // Phase 5: Variable substitution. `set-up` adopts a running container and takes no
    // `--workspace-folder`, so the workspace-derived variables have no answer:
    // `${localWorkspaceFolder}`, `${localWorkspaceFolderBasename}` and
    // `${devcontainerId}` stay LITERAL, which is what the reference CLI emits here —
    // measured at oracle 0.87.0 on both the reported blocks and the lifecycle commands
    // it exec's (#510). `${localEnv:*}` still resolves; both sides agree on that.
    //
    // The cwd is passed as a MECHANICAL anchor only (lifecycle phase markers and the
    // host-side working directory need a path); `without_workspace` is what makes the
    // absence of a workspace explicit, so no variable can observe it. Anchoring with
    // `SubstitutionContext::new` instead silently substituted the cwd for a workspace
    // the caller never named.
    //
    // `${containerWorkspaceFolder}` is the one workspace-shaped variable that CAN
    // have an answer here, and it comes from the `--config` document's own
    // `workspaceFolder` — the reference builds its context with exactly that
    // (`containerWorkspaceFolder: <the config's raw workspaceFolder>`, undefined
    // when the document omits it), and leaves the token literal otherwise.
    // Measured at oracle 0.87.0 on all four cells (#513), including the negative:
    // a `workspaceFolder` reaching only through the container's image metadata
    // does NOT resolve it, so this reads `base_config`, not the merged config.
    let cwd = std::env::current_dir().context("Failed to get current working directory")?;
    let mut substitution_context = SubstitutionContext::without_workspace(&cwd)?;
    substitution_context.container_workspace_folder = base_config.workspace_folder.clone();

    // The `configuration` block reports what the CALLER SUPPLIED — the `--config`
    // document alone, substituted, never folded with the container's image metadata.
    // The merge belongs to `mergedConfiguration` (#483/#501), and this is the same
    // authored-vs-resolved contract `up`'s and `read-configuration`'s blocks keep
    // (#490/#407). Measured at oracle 0.87.0 (#502).
    let (substituted_config, _) = base_config.apply_variable_substitution(&substitution_context);
    let (substituted_merged, _) = merged_config.apply_variable_substitution(&substitution_context);

    // Phase 6: System patches (spec §5 phase 3a). Best-effort per spec §9
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

    // Phase 7: Lifecycle hook execution. Skipped entirely when
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

    // Phase 8: Container substitution over the REPORTED blocks only — the reference's
    // third pass (`containerSubstitute`), the same one `up` got in #608/#613 and
    // `set-up` was left without (#616). Pass 5 ran before this function knew anything
    // about the container's environment, so every `${containerEnv:*}` in it is still a
    // template; measured at oracle 0.87.0, the reference resolves them in BOTH blocks
    // (and answers a key the container does not define with the empty string).
    //
    // The container environment is already in hand: Phase 2 inspected the container and
    // `ContainerInfo::env` IS `Config.Env`, the canonical source for `${containerEnv:*}`
    // — so there is no second inspect, and no probe. Fail-safe per #613: an empty map
    // means the inspect gave us nothing, and passing `None` leaves the templates intact
    // rather than collapsing every one of them to an empty string.
    //
    // REPORTED ONLY, deliberately. What set-up EXECS keeps the pass-5 configuration,
    // because the reference leaves `${containerEnv:*}` literal in the lifecycle commands
    // it runs — measured on the same container: its `postCreateCommand` wrote
    // `cenv=[${containerEnv:SETUP_PROBE_VAR}]`, and so does deacon's. That agreement is
    // pinned by the `chan-file-content` channel of `case-set-up-container-env-substitution`,
    // so folding this pass into `substituted_merged` would go red.
    //
    // The context is the one pass 5 built, not a fresh workspace-anchored one: `set-up`
    // takes no `--workspace-folder`, so `without_workspace` keeps `${localWorkspaceFolder}`,
    // `${localWorkspaceFolderBasename}` and `${devcontainerId}` literal (#510) and
    // `${containerWorkspaceFolder}` comes from the `--config` document's own
    // `workspaceFolder` (#513). This pass adds the container environment and nothing else.
    let reported_env = Some(&container.env).filter(|env| !env.is_empty());
    let reported_config = if args.include_configuration || args.include_merged_configuration {
        crate::commands::shared::container_substitution::container_substituted_with_context(
            &substituted_config,
            &substitution_context,
            reported_env,
        )
    } else {
        substituted_config.clone()
    };
    let reported_merged = if args.include_merged_configuration {
        crate::commands::shared::container_substitution::container_substituted_with_context(
            &substituted_merged,
            &substitution_context,
            reported_env,
        )
    } else {
        substituted_merged.clone()
    };

    // Phase 9: Emit JSON result on stdout (spec §10).
    let result = SetUpResult::Success {
        outcome: "success",
        configuration: args
            .include_configuration
            .then(|| configuration_document(&reported_config, args.config_path.as_deref()))
            .transpose()?,
        merged_configuration: args
            .include_merged_configuration
            .then(|| {
                merged_configuration_document(
                    &reported_merged,
                    &reported_config,
                    args.config_path.as_deref(),
                )
            })
            .transpose()?,
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
/// label. Thin wrapper over the shared
/// [`container_metadata::config_from_metadata_label`] so set-up, exec, and
/// read-configuration all fold the label identically (#322).
fn extract_image_metadata_config(container: &ContainerInfo) -> Result<Option<DevContainerConfig>> {
    crate::commands::shared::container_metadata::config_from_metadata_label(container)
}

/// The properties a `devcontainer.metadata` entry may contribute to the merged
/// configuration — upstream's `mergeConfiguration` property list, transcribed (#526).
///
/// The reference's merge is not "fold two configurations". It is `{...config minus the
/// collected properties}` PLUS an explicitly ENUMERATED set of properties read off the
/// metadata entries, one expression per property (bundle `Xi` at
/// `dist/spec-node/devContainersSpecCLI.js`, `mergeConfiguration` in
/// `devcontainers/cli/src/spec-node/imageMetadata.ts`). A property with no expression is
/// simply never read from metadata, so it cannot reach the merged configuration from a
/// label no matter what the label says — the base config is its only source.
///
/// deacon folded EVERYTHING instead, so a container label could contribute
/// `workspaceFolder`, `name`, `runArgs`, `appPort`, `workspaceMount`, `features`,
/// `overrideFeatureInstallOrder` — and, since the label is arbitrary JSON, any other
/// property a base image happened to write. Measured at oracle 0.87.0 on a raw-labeled
/// container carrying all of them, deacon's `mergedConfiguration` reported eleven keys the
/// reference omitted. It is not only a reporting defect: `workspaceFolder` is what
/// `execute_lifecycle_hooks` uses for the exec CWD, so a label naming a directory that does
/// not exist in the container made every hook fail with exit 127 where the reference ran
/// them.
///
/// The list is exactly the union of the properties `Xi` reads from an entry. That is
/// upstream's `pickConfigProperties` list (bundle `SV`) plus `entrypoint`, which is a
/// FEATURE property (bundle `RV`) and therefore never picked off a configuration, but which
/// `Xi` does read off metadata entries and report as the collected `entrypoints` array.
///
/// Names are the ON-THE-WIRE (camelCase) spellings, because the restriction is applied to
/// the serialized document rather than field-by-field: a property added to
/// [`DevContainerConfig`] tomorrow is absent from this list and is therefore NOT folded,
/// which is the safe default and matches the reference (whose list is likewise explicit).
/// `entrypoint` is unmodeled on `DevContainerConfig` and rides in
/// [`DevContainerConfig::extra`]; retaining the key here is what keeps it reaching the
/// collected `entrypoints` array.
const METADATA_MERGE_PROPERTIES: &[&str] = &[
    "onCreateCommand",
    "updateContentCommand",
    "postCreateCommand",
    "postStartCommand",
    "postAttachCommand",
    "waitFor",
    "customizations",
    "mounts",
    "containerEnv",
    "containerUser",
    "init",
    "privileged",
    "capAdd",
    "securityOpt",
    "remoteUser",
    "userEnvProbe",
    "remoteEnv",
    "overrideCommand",
    "portsAttributes",
    "otherPortsAttributes",
    "forwardPorts",
    "shutdownAction",
    "updateRemoteUserUID",
    "hostRequirements",
    "entrypoint",
];

/// Project a container-metadata configuration down to [`METADATA_MERGE_PROPERTIES`].
///
/// Applied to the already-folded label config rather than to each fragment, which is
/// equivalent: [`deacon_core::config::ConfigMerger`] folds field-wise, so projecting after
/// the fold drops exactly the fields projecting before it would have.
///
/// The round-trip is through JSON on purpose. The property list is upstream's, written in
/// upstream's spelling, and checking it against the serialized document is the only form in
/// which the two can be compared by eye. It also disposes of
/// [`DevContainerConfig::extra`] for free: an unmodeled key a base image wrote into its
/// label is not on the list, so it does not survive — where a field-by-field copy would
/// have carried `extra` through untouched.
///
/// [`DevContainerConfig::metadata_lifecycle_layers`] is re-attached after the round-trip
/// because it is `#[serde(skip)]` merge provenance, not an authored property: the five
/// singular hook fields were already lifted onto it by
/// [`container_metadata::config_from_metadata_label`], and losing it here would silently
/// stop every label-contributed hook from running (#475/#477).
/// [`DevContainerConfig::metadata_customizations_layers`] is re-attached for the same
/// reason (#532) — losing it would silently return the reported `customizations` to the
/// deep-merged single-contributor shape, and the fold's own `customizations` key survives
/// the restriction, so nothing would look broken.
fn restrict_to_metadata_properties(metadata: &DevContainerConfig) -> Result<DevContainerConfig> {
    let mut value = serde_json::to_value(metadata)
        .context("Failed to serialize container metadata configuration")?;
    if let Some(object) = value.as_object_mut() {
        object.retain(|key, _| METADATA_MERGE_PROPERTIES.contains(&key.as_str()));
    }
    let mut restricted: DevContainerConfig = serde_json::from_value(value)
        .context("Failed to re-read the restricted container metadata configuration")?;
    restricted
        .metadata_lifecycle_layers
        .clone_from(&metadata.metadata_lifecycle_layers);
    restricted
        .metadata_customizations_layers
        .clone_from(&metadata.metadata_customizations_layers);
    Ok(restricted)
}

/// Merge `--config` (if any) on top of image metadata config (if any).
///
/// Order matters: per spec §4 `mergeConfiguration(config.config, imageMetadata)`
/// puts the file config FIRST so its scalar values win and its lists go
/// before image-metadata lists. `ConfigMerger::merge_configs` folds left to
/// right with later entries winning, so we pass `[metadata, file_config]` to
/// preserve that semantics.
///
/// The metadata side is first projected through [`restrict_to_metadata_properties`], so
/// only the properties upstream's `mergeConfiguration` reads off a metadata entry can
/// reach the result. The file config is NOT restricted — it is the base of the merge and
/// every property it authors belongs in the output (#526).
fn merge_configs(
    file_config: &DevContainerConfig,
    metadata_config: Option<&DevContainerConfig>,
) -> Result<DevContainerConfig> {
    match metadata_config {
        Some(meta) => {
            let restricted = restrict_to_metadata_properties(meta)?;
            Ok(deacon_core::config::ConfigMerger::merge_configs(&[
                restricted,
                file_config.clone(),
            ]))
        }
        None => Ok(file_config.clone()),
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
    // Collected into a `HashMap` because that is where the authored-order map
    // (#394) stops: this becomes `docker exec -e K=V` flags, where order carries
    // no meaning, and nothing downstream re-serializes it.
    let mut container_env: HashMap<String, String> =
        merged_config.container_env().clone().into_iter().collect();
    for (k, v) in &remote_env_pairs {
        container_env.insert(k.clone(), v.clone());
    }

    // Two different uses of one path, split by #513.
    //
    // The exec CWD must always be a path — `docker exec -w` needs one — so an
    // unauthored `workspaceFolder` still falls back to `/`, unchanged.
    //
    // The SUBSTITUTION value must be able to be absent, and is whatever the
    // caller's context resolved (the `--config`'s own `workspaceFolder`, or
    // `None`). Reading it from the context rather than recomputing it is what
    // makes the reported blocks and the commands set-up actually runs agree by
    // construction: before, the report left `${containerWorkspaceFolder}` literal
    // while the exec substituted an invented `/`.
    let container_workspace_folder = merged_config
        .workspace_folder
        .clone()
        .unwrap_or_else(|| "/".to_string());
    let substitution_workspace_folder = substitution_context.container_workspace_folder.clone();

    // #372: every lifecycle-marker writer stamps the hash of the config it actually
    // ran, so `up`'s config-drift detection stays meaningful. Markers key on the
    // workspace hash of the marker anchor, and `set-up`'s anchor is the process cwd
    // — which walks to the same git root as an `up --workspace-folder <same repo>`,
    // so these markers DO land in `up`'s directory and a `None` here would stamp
    // them "legacy, compatible with any config" and make a later `up` skip hooks it
    // must re-run.
    //
    // This hash deliberately will NOT equal `up`'s: `set-up` adopts a container it
    // did not create, folds in the container's image metadata, and substitutes
    // without a workspace (`${localWorkspaceFolder}` stays literal). The honest
    // consequence is that a following `up` sees drift and RE-RUNS — the fail-safe
    // direction, and strictly better than silently skipping.
    let marker_anchor = std::path::Path::new(&substitution_context.local_workspace_folder);
    let config_hash =
        deacon_core::container::ContainerIdentity::new(marker_anchor, merged_config).config_hash;

    let lifecycle_config = ContainerLifecycleConfig {
        capture_output: false,
        container_id: container.id.clone(),
        user: merged_config
            .remote_user
            .clone()
            .or_else(|| merged_config.container_user.clone()),
        container_workspace_folder,
        substitution_workspace_folder,
        container_env,
        // We've already gated all-skip in the caller; pass false here so the
        // lifecycle helper runs the individual phases it would normally run.
        skip_post_create: false,
        skip_non_blocking_commands: args.skip_non_blocking_commands,
        non_blocking_timeout: Duration::from_secs(300),
        use_login_shell: true,
        user_env_probe: deacon_core::container_env_probe::ContainerProbeMode::LoginShell,
        cache_folder: args.container_data_folder.clone(),
        user_data_folder: args.user_data_folder.clone(),
        force_pty: false,
        dotfiles: build_dotfiles_config(args),
        is_prebuild: false,
        config_hash: Some(config_hash),
    };

    let mut commands = ContainerLifecycleCommands::new();

    // Every hook the container's `devcontainer.metadata` label declares for a
    // phase runs, in label order, ahead of the one `--config` declares — the
    // spec's "Collected list of all `<phase>Command`s … the devcontainer.json is
    // considered last" (#477). Reading the five SINGULAR fields ran only the
    // last survivor of the fold; `aggregate_lifecycle_commands` is the
    // collection `up` and `run-user-commands` already use, so set-up joins it
    // rather than growing a second one.
    //
    // No separately-resolved features: a stamped label already carries each
    // Feature's contribution as its own entry, so those arrive as layers too —
    // which is how set-up gains Feature-hook execution it never had.
    const NO_FEATURES: &[deacon_core::features::ResolvedFeature] = &[];
    let collect = |phase: LifecyclePhase| -> Result<Option<LifecycleCommandList>> {
        let list = aggregate_lifecycle_commands(phase, NO_FEATURES, merged_config)
            .with_context(|| format!("Failed to parse {:?} commands", phase))?;
        Ok((!list.commands.is_empty()).then_some(list))
    };

    if let Some(list) = collect(LifecyclePhase::OnCreate)? {
        commands = commands.with_on_create(list);
    }
    if let Some(list) = collect(LifecyclePhase::UpdateContent)? {
        commands = commands.with_update_content(list);
    }
    if let Some(list) = collect(LifecyclePhase::PostCreate)? {
        commands = commands.with_post_create(list);
    }
    if !args.skip_non_blocking_commands {
        if let Some(list) = collect(LifecyclePhase::PostStart)? {
            commands = commands.with_post_start(list);
        }
        if let Some(list) = collect(LifecyclePhase::PostAttach)? {
            commands = commands.with_post_attach(list);
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
    for (k, v) in merged_config.container_env() {
        env.insert(k.clone(), v.clone());
    }
    for (k, v) in merged_config.remote_env() {
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
    use std::path::Path;

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
            user_data_folder: None,
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
            cfg.container_env().get("FOO").map(|s| s.as_str()),
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
            .get_or_insert_default()
            .insert("META_VAR".to_string(), "meta".to_string());

        // Per spec §4: file config wins over metadata on scalar fields.
        let merged = merge_configs(&file_cfg, Some(&meta_cfg)).unwrap();
        assert_eq!(merged.remote_user.as_deref(), Some("file-user"));
        // Metadata env still flows through via the merger's map overlay.
        assert_eq!(
            merged.container_env().get("META_VAR").map(|s| s.as_str()),
            Some("meta")
        );
    }

    #[test]
    fn merge_configs_returns_file_config_when_no_metadata() {
        let file_cfg = DevContainerConfig {
            name: Some("only-file".to_string()),
            ..DevContainerConfig::default()
        };
        let merged = merge_configs(&file_cfg, None).unwrap();
        assert_eq!(merged.name.as_deref(), Some("only-file"));
    }

    /// #526: a container's `devcontainer.metadata` label contributes ONLY the
    /// properties upstream's `mergeConfiguration` reads off a metadata entry.
    ///
    /// The label below authors every property in the leak census plus a handful the
    /// census did not name (`image`, `service`, `runServices`, `initializeCommand`) and an
    /// unmodeled key, alongside enumerated properties that MUST survive. Measured at
    /// oracle 0.87.0 on a raw-labeled container carrying exactly this document: the
    /// reference's `mergedConfiguration` reports the enumerated half and none of the rest.
    ///
    /// Asserting both halves is the point. A restriction that dropped too much would be
    /// just as wrong as the fold that dropped nothing, and only the positive half can see
    /// it — which is why `remoteUser`, `containerEnv`, `forwardPorts` and `userEnvProbe`
    /// are checked rather than assumed.
    #[test]
    fn merge_configs_folds_only_the_enumerated_metadata_properties() {
        let metadata: DevContainerConfig = serde_json::from_value(serde_json::json!({
            // NOT on upstream's list — none of these may reach the merged config.
            "workspaceFolder": "/meta-ws",
            "name": "meta-name",
            "runArgs": ["--meta-arg"],
            "appPort": [9999],
            "workspaceMount": "source=/m,target=/m,type=bind",
            "features": { "ghcr.io/x/y:1": {} },
            "overrideFeatureInstallOrder": ["ghcr.io/x/y"],
            "image": "should-not-leak",
            "service": "nope",
            "runServices": ["nope"],
            "initializeCommand": "echo nope",
            "someFutureProperty": "unmodeled, rides in `extra`",
            // ON upstream's list — all of these must survive.
            "remoteUser": "meta-user",
            "containerEnv": { "META": "1" },
            "forwardPorts": [3000],
            "userEnvProbe": "none",
        }))
        .expect("the metadata document should deserialize");

        let merged = merge_configs(&DevContainerConfig::default(), Some(&metadata)).unwrap();

        assert_eq!(
            merged.workspace_folder, None,
            "workspaceFolder must not fold"
        );
        assert_eq!(merged.name, None, "name must not fold");
        assert_eq!(merged.run_args, None, "runArgs must not fold");
        assert_eq!(merged.app_port, None, "appPort must not fold");
        assert_eq!(merged.workspace_mount, None, "workspaceMount must not fold");
        assert_eq!(merged.features, None, "features must not fold");
        assert_eq!(
            merged.override_feature_install_order, None,
            "overrideFeatureInstallOrder must not fold"
        );
        assert_eq!(merged.image, None, "image must not fold");
        assert_eq!(merged.service, None, "service must not fold");
        assert_eq!(merged.run_services, None, "runServices must not fold");
        assert_eq!(
            merged.initialize_command, None,
            "initializeCommand must not fold"
        );
        assert!(
            merged.extra.is_empty(),
            "an unmodeled label key is not on upstream's list either, so it must not \
             survive in `extra`: {:?}",
            merged.extra
        );

        assert_eq!(merged.remote_user.as_deref(), Some("meta-user"));
        assert_eq!(
            merged.container_env().get("META").map(String::as_str),
            Some("1")
        );
        assert!(merged.forward_ports.is_some(), "forwardPorts must fold");
        assert!(merged.user_env_probe.is_some(), "userEnvProbe must fold");
    }

    /// #526 must not undo #475/#477: the hook layers lifted off the label survive the
    /// property restriction.
    ///
    /// `metadata_lifecycle_layers` is `#[serde(skip)]`, so the JSON round-trip the
    /// restriction performs erases it unless it is re-attached — and the failure would be
    /// silent, because the five singular hook fields are already empty by then. Every
    /// label-contributed hook would simply stop running.
    #[test]
    fn restricting_metadata_properties_preserves_the_lifecycle_layers() {
        let mut labels = HashMap::new();
        labels.insert(
            "devcontainer.metadata".to_string(),
            serde_json::json!([
                { "workspaceFolder": "/meta-ws", "onCreateCommand": "echo one" },
                { "onCreateCommand": "echo two" },
            ])
            .to_string(),
        );
        let container = make_container("abc", "alpine:3.18", labels);
        let metadata = extract_image_metadata_config(&container).unwrap().unwrap();

        let restricted = restrict_to_metadata_properties(&metadata).unwrap();

        assert_eq!(
            restricted.metadata_lifecycle_layers.len(),
            2,
            "both label fragments' hooks must still be carried as layers"
        );
        assert_eq!(restricted.workspace_folder, None);
    }

    /// #477, hermetically: the whole set-up path from the container's stamped
    /// label through `merge_configs` to the aggregation that feeds
    /// `ContainerLifecycleCommands`.
    ///
    /// Both collapse points are covered at once, because they compose. The
    /// label's own fragments used to fold last-wins (so `e0-onCreate` and
    /// `feat-onCreate` vanished), and then reading the five SINGULAR fields off
    /// the `--config` merge dropped whatever had survived (so only `ws-onCreate`
    /// ran). The reference runs all four, devcontainer.json last — measured at
    /// oracle 0.87.0, whose log on this exact shape reads the lines asserted
    /// below.
    ///
    /// `postStart` is the control the fix must not break twice over: only the
    /// FEATURE entry declares it, so it must appear exactly ONCE. A hook
    /// recorded as a layer while the merge also left it in the singular field
    /// would run twice, and an equality assertion over the whole list is the
    /// only kind that can see that.
    #[test]
    fn set_up_collects_every_label_entry_hook_ahead_of_the_config_hook() {
        let mut labels = HashMap::new();
        labels.insert(
            "devcontainer.metadata".to_string(),
            r#"[
                {"onCreateCommand": "e0-onCreate", "postCreateCommand": "e0-postCreate"},
                {"id": "ghcr.io/example/feat:1", "onCreateCommand": "feat-onCreate",
                 "postStartCommand": "feat-postStart"},
                {"onCreateCommand": "e2-onCreate", "postCreateCommand": "e2-postCreate"}
            ]"#
            .to_string(),
        );
        let container = make_container("abc", "alpine:3.18", labels);
        let metadata = extract_image_metadata_config(&container).unwrap();

        let file_cfg = DevContainerConfig {
            on_create_command: Some(serde_json::json!("ws-onCreate")),
            post_create_command: Some(serde_json::json!("ws-postCreate")),
            ..DevContainerConfig::default()
        };
        let merged = merge_configs(&file_cfg, metadata.as_ref()).unwrap();

        let phase_commands = |phase: LifecyclePhase| {
            aggregate_lifecycle_commands(phase, &[], &merged)
                .unwrap()
                .commands
                .into_iter()
                .map(|c| c.command)
                .collect::<Vec<_>>()
        };

        assert_eq!(
            phase_commands(LifecyclePhase::OnCreate),
            vec![
                serde_json::json!("e0-onCreate"),
                serde_json::json!("feat-onCreate"),
                serde_json::json!("e2-onCreate"),
                serde_json::json!("ws-onCreate"),
            ],
            "every label entry's onCreateCommand runs in label order, the \
             devcontainer.json's last"
        );
        assert_eq!(
            phase_commands(LifecyclePhase::PostCreate),
            vec![
                serde_json::json!("e0-postCreate"),
                serde_json::json!("e2-postCreate"),
                serde_json::json!("ws-postCreate"),
            ],
            "the hookless-for-this-phase feature entry contributes nothing"
        );
        assert_eq!(
            phase_commands(LifecyclePhase::PostStart),
            vec![serde_json::json!("feat-postStart")],
            "a phase only ONE entry declares must run exactly once — a layer that \
             also stayed in the singular field would run twice"
        );
        assert!(phase_commands(LifecyclePhase::UpdateContent).is_empty());
        assert!(phase_commands(LifecyclePhase::PostAttach).is_empty());
    }

    /// The merged config of the test above — the container's three-entry label plus a
    /// `--config` — so what `--include-merged-configuration` REPORTS is pinned against
    /// what set-up actually RUNS. A report that disagreed with the execution would be the
    /// worse defect of the two, and only building both from one input can see it.
    ///
    /// Returns the `--config` document alongside the merge, because the reported
    /// `customizations` entries need the config's OWN contribution rather than the merged
    /// one (#532) — exactly what `execute_set_up` hands the reporting layer.
    fn merged_for_report() -> (DevContainerConfig, DevContainerConfig) {
        let mut labels = HashMap::new();
        labels.insert(
            "devcontainer.metadata".to_string(),
            r#"[
                {"onCreateCommand": "e0-onCreate", "postCreateCommand": "e0-postCreate"},
                {"id": "ghcr.io/example/feat:1", "onCreateCommand": "feat-onCreate",
                 "postStartCommand": "feat-postStart"},
                {"onCreateCommand": "e2-onCreate", "postCreateCommand": "e2-postCreate"}
            ]"#
            .to_string(),
        );
        let container = make_container("abc", "alpine:3.18", labels);
        let metadata = extract_image_metadata_config(&container).unwrap();
        let file_cfg = DevContainerConfig {
            on_create_command: Some(serde_json::json!("ws-onCreate")),
            post_create_command: Some(serde_json::json!("ws-postCreate")),
            ..DevContainerConfig::default()
        };
        let merged = merge_configs(&file_cfg, metadata.as_ref()).unwrap();
        (merged, file_cfg)
    }

    /// #483: the block carries the COLLECTED plural arrays, in the order the hooks run,
    /// and no singular hook key survives.
    ///
    /// Equality over each whole array, never `contains`: the defect this replaced reported
    /// one command per phase, and a containment assertion would have called that a pass.
    #[test]
    fn merged_configuration_reports_the_collected_plural_arrays() {
        let (merged, file_cfg) = merged_for_report();
        let doc = merged_configuration_document(&merged, &file_cfg, None).unwrap();

        assert_eq!(
            doc["onCreateCommands"],
            serde_json::json!(["e0-onCreate", "feat-onCreate", "e2-onCreate", "ws-onCreate"]),
            "every label entry contributes, the devcontainer.json last — the same order \
             `aggregate_lifecycle_commands` replays"
        );
        assert_eq!(
            doc["postCreateCommands"],
            serde_json::json!(["e0-postCreate", "e2-postCreate", "ws-postCreate"])
        );
        assert_eq!(
            doc["postStartCommands"],
            serde_json::json!(["feat-postStart"]),
            "a phase only one entry declares appears exactly once"
        );
        // The reference always reports the five hook slots, empty ones as `[]`.
        assert_eq!(doc["updateContentCommands"], serde_json::json!([]));
        assert_eq!(doc["postAttachCommands"], serde_json::json!([]));
        // …and materializes these two as booleans rather than omitting them.
        assert_eq!(doc["init"], serde_json::json!(false));
        assert_eq!(doc["privileged"], serde_json::json!(false));

        for singular in [
            "onCreateCommand",
            "updateContentCommand",
            "postCreateCommand",
            "postStartCommand",
            "postAttachCommand",
        ] {
            assert!(
                doc.get(singular).is_none(),
                "the merged shape strips `{singular}`; leaving it beside the plural array \
                 would report the same hook twice"
            );
        }
    }

    /// The other half of #483: the key itself. deacon emitted serde's default snake_case
    /// `merged_configuration`, which no consumer of the reference's output looks for.
    #[test]
    fn merged_configuration_is_reported_under_the_camel_case_key() {
        let result = SetUpResult::Success {
            outcome: "success",
            configuration: None,
            merged_configuration: Some(serde_json::json!({ "name": "x" })),
        };
        let json: serde_json::Value = serde_json::to_value(&result).unwrap();

        assert!(json.get("mergedConfiguration").is_some());
        assert!(
            json.get("merged_configuration").is_none(),
            "the snake_case key is the pre-#483 shape and must not survive alongside it"
        );
    }

    /// `configFilePath` rides on the block whenever a `--config` named a file, with the
    /// `file` scheme the reference uses for a caller-named path — and is absent when
    /// set-up ran against the container's label alone, which is what the reference does
    /// too (measured at oracle 0.87.0 on both shapes).
    #[test]
    fn merged_configuration_reports_config_file_path_only_when_config_was_named() {
        let (merged, file_cfg) = merged_for_report();

        let named =
            merged_configuration_document(&merged, &file_cfg, Some(Path::new("/ws/overlay.json")))
                .expect("shaping a named config succeeds");
        assert_eq!(named["configFilePath"]["scheme"], serde_json::json!("file"));
        assert!(
            named["configFilePath"]["fsPath"]
                .as_str()
                .is_some_and(|p| p.ends_with("overlay.json")),
            "the reported path names the file the caller passed: {named:#}"
        );

        let unnamed = merged_configuration_document(&merged, &file_cfg, None).unwrap();
        assert!(unnamed.get("configFilePath").is_none());
    }

    /// Build the merged + `--config` pair set-up reports from, for a container label and a
    /// `--config` document given as raw JSON. The label goes through the real
    /// `config_from_metadata_label` / `merge_configs` path, so the per-fragment
    /// `customizations` reach the reporting layer the same way they do in production.
    fn report_inputs(
        label: serde_json::Value,
        config: serde_json::Value,
    ) -> (DevContainerConfig, DevContainerConfig) {
        let mut labels = HashMap::new();
        labels.insert("devcontainer.metadata".to_string(), label.to_string());
        let container = make_container("abc", "alpine:3.18", labels);
        let metadata = extract_image_metadata_config(&container).unwrap();
        let file_cfg: DevContainerConfig =
            serde_json::from_value(config).expect("the --config document should deserialize");
        let merged = merge_configs(&file_cfg, metadata.as_ref()).unwrap();
        (merged, file_cfg)
    }

    /// #532: `mergedConfiguration.customizations` is one array SLOT PER CONTRIBUTOR, keyed
    /// by tool — not a deep merge.
    ///
    /// Upstream's `mergeConfiguration` deletes `customizations` from the base config (it is
    /// on the `kV` delete list) and rebuilds it as
    /// `entries.reduce((acc, e) => { for (let tool in e.customizations) …push… })`, leaving
    /// each consuming tool to reconcile its own slots. deacon reported whatever
    /// `ConfigMerger` had deep-merged, which reads as a single contributor and silently
    /// resolves conflicts the reference hands over intact.
    ///
    /// Both label fragments below set `vscode.settings`, and one sets a key the other also
    /// sets. That is the assertion that can tell the shapes apart: a deep merge produces
    /// ONE `vscode` object with `{"a": 1, "b": 2}`, and only the array form preserves the
    /// boundary. Measured at oracle 0.87.0 on a raw-labeled container carrying exactly this
    /// label plus this `--config`.
    #[test]
    fn merged_configuration_reports_customizations_as_per_tool_arrays() {
        let (merged, file_cfg) = report_inputs(
            serde_json::json!([
                {"id": "frag1", "customizations": {"vscode": {"settings": {"a": 1},
                                                              "extensions": ["ext.one"]}}},
                {"id": "frag2", "customizations": {"vscode": {"settings": {"b": 2}},
                                                   "jetbrains": {"backend": "IU"}}},
                {"remoteUser": "root"},
            ]),
            serde_json::json!({"customizations": {"vscode": {"settings": {"fromConfig": true}}}}),
        );

        let doc = merged_configuration_document(&merged, &file_cfg, None).unwrap();

        assert_eq!(
            doc["customizations"],
            serde_json::json!({
                "vscode": [
                    {"settings": {"a": 1}, "extensions": ["ext.one"]},
                    {"settings": {"b": 2}},
                    {"settings": {"fromConfig": true}},
                ],
                "jetbrains": [{"backend": "IU"}],
            }),
            "one slot per contributing entry, in `[…label fragments in label order, \
             --config]` order — a fragment that authored no customizations contributes \
             none. Got: {doc:#}"
        );
    }

    /// The negative half, and the reason the field is omitted rather than emitted empty:
    /// upstream ends with `customizations: Object.keys(t).length ? t : void 0`. A label and
    /// a `--config` that author none — or author an empty object, which the `for (let tool
    /// in …)` loop adds no key for — leave the property out entirely. Measured at oracle
    /// 0.87.0, which reports no `customizations` key on this shape.
    #[test]
    fn merged_configuration_omits_customizations_when_nobody_contributed() {
        let (merged, file_cfg) = report_inputs(
            serde_json::json!([{"remoteUser": "root"}, {"customizations": {}}]),
            serde_json::json!({"customizations": {}}),
        );

        let doc = merged_configuration_document(&merged, &file_cfg, None).unwrap();

        assert!(
            doc.get("customizations").is_none(),
            "an empty contribution is not a contribution: {doc:#}"
        );
    }

    /// The `--config`'s slot carries what the CALLER authored, never the merge.
    ///
    /// Upstream's final entry is `pick(config, pickConfigProperties)` — the configuration
    /// document itself — so a tool the label also customized must NOT find the label's
    /// values folded into the config's slot. Reading `merged.customizations` here (the
    /// deep-merged object) instead of the `--config`'s own would produce exactly that, and
    /// the single-contributor case would still pass, which is why this asserts the
    /// two-contributor one.
    #[test]
    fn the_config_slot_reports_the_authored_document_not_the_merge() {
        let (merged, file_cfg) = report_inputs(
            serde_json::json!([{"customizations": {"vscode": {"settings": {"fromLabel": 1}}}}]),
            serde_json::json!({"customizations": {"vscode": {"settings": {"fromConfig": 2}}}}),
        );

        let doc = merged_configuration_document(&merged, &file_cfg, None).unwrap();

        assert_eq!(
            doc["customizations"]["vscode"],
            serde_json::json!([
                {"settings": {"fromLabel": 1}},
                {"settings": {"fromConfig": 2}},
            ]),
            "the config's slot is its own document — a merged one would carry \
             `fromLabel` too: {doc:#}"
        );
    }

    /// The label's per-fragment `customizations` are variable-substituted, with the very
    /// substitution `set-up` applies to the configuration — the reference maps its
    /// `substitute` over every metadata entry before merging (`Tr` → `IG`: `i.map(e)`).
    ///
    /// Under `set-up` that means `${localEnv:*}` RESOLVES while `${localWorkspaceFolder}`
    /// stays literal (there is no `--workspace-folder`, #510). Measured at oracle 0.87.0 on
    /// a label carrying both tokens: the reported slot holds the env value and the literal
    /// workspace token, side by side.
    ///
    /// Without this the layers would report the raw label text while the deep-merged
    /// `customizations` — which deacon already substituted — reported the resolved one, so
    /// the fix would have traded one divergence for another.
    #[test]
    fn label_customizations_layers_are_substituted_like_the_configuration() {
        let (merged, file_cfg) = report_inputs(
            serde_json::json!([{"customizations": {"vscode": {"settings": {
                "le": "${localEnv:DEACON_TEST_532_PROBE}",
                "lwf": "${localWorkspaceFolder}",
            }}}}]),
            serde_json::json!({}),
        );

        // Exactly the context `execute_set_up` builds, with the probe seeded on it
        // rather than on the process environment (env mutation is a global side effect
        // in a parallel test binary).
        let cwd = std::env::current_dir().unwrap();
        let mut context = SubstitutionContext::without_workspace(&cwd).unwrap();
        context.local_env.insert(
            "DEACON_TEST_532_PROBE".to_string(),
            "probe-value".to_string(),
        );
        let (substituted_merged, _) = merged.apply_variable_substitution(&context);
        let (substituted_config, _) = file_cfg.apply_variable_substitution(&context);

        let doc =
            merged_configuration_document(&substituted_merged, &substituted_config, None).unwrap();

        assert_eq!(
            doc["customizations"]["vscode"],
            serde_json::json!([{"settings": {
                "le": "probe-value",
                "lwf": "${localWorkspaceFolder}",
            }}]),
            "`${{localEnv:*}}` resolves and `${{localWorkspaceFolder}}` stays literal, \
             exactly as in the rest of set-up's reported blocks: {doc:#}"
        );
    }

    /// `set-up` has no `--workspace-folder`, so the workspace-derived variables must
    /// survive substitution as LITERALS — in the reported blocks AND in the lifecycle
    /// command strings set-up goes on to exec, which is the surface a test of the JSON
    /// alone cannot see. deacon used to anchor at the process cwd and substitute an
    /// invented workspace into both (#510); the reference leaves all three literal and
    /// resolves `${localEnv:*}` only. Measured at oracle 0.87.0.
    #[test]
    fn set_up_substitution_leaves_workspace_derived_variables_literal() {
        let probe = "ws=${localWorkspaceFolder} base=${localWorkspaceFolderBasename} \
                     id=${devcontainerId} env=${localEnv:DEACON_TEST_SETUP_SCOPE}";
        let config = DevContainerConfig {
            remote_env: Some(
                [("PROBE".to_string(), Some(probe.to_string()))]
                    .into_iter()
                    .collect(),
            ),
            post_create_command: Some(serde_json::json!(probe)),
            ..Default::default()
        };

        // Exactly what `execute_set_up` builds, anchored at a path that would be a
        // plausible-looking (and wrong) answer if any variable could observe it.
        let anchor = std::env::current_dir().expect("cwd is readable");
        let mut context =
            SubstitutionContext::without_workspace(&anchor).expect("context builds from the cwd");
        context.local_env.insert(
            "DEACON_TEST_SETUP_SCOPE".to_string(),
            "resolved".to_string(),
        );

        let (substituted, _) = config.apply_variable_substitution(&context);

        let expected = "ws=${localWorkspaceFolder} base=${localWorkspaceFolderBasename} \
                        id=${devcontainerId} env=resolved";
        assert_eq!(
            substituted.remote_env.as_ref().unwrap()["PROBE"].as_deref(),
            Some(expected),
            "the reported remoteEnv must keep the workspace tokens literal"
        );
        assert_eq!(
            substituted.post_create_command.as_ref().unwrap(),
            &serde_json::json!(expected),
            "the command set-up EXECS must keep the workspace tokens literal too"
        );

        let anchor = anchor.to_string_lossy();
        assert!(
            !substituted
                .post_create_command
                .as_ref()
                .unwrap()
                .to_string()
                .contains(anchor.as_ref()),
            "the mechanical cwd anchor must never leak into a substituted value"
        );
    }

    /// `${containerWorkspaceFolder}` is the one workspace-shaped variable set-up CAN
    /// answer, and its source is the `--config` document's own `workspaceFolder`:
    /// resolved when authored, literal when not — on the reported blocks AND on the
    /// command strings set-up execs, which are built from this one context. Measured
    /// at oracle 0.87.0 on all four cells (#513); deacon used to leave the token
    /// literal in the report while substituting an invented `/` into the command.
    #[test]
    fn set_up_substitution_answers_container_workspace_folder_only_when_authored() {
        let probe = "cwf=${containerWorkspaceFolder} base=${containerWorkspaceFolderBasename}";
        let config_with = |workspace_folder: Option<&str>| DevContainerConfig {
            workspace_folder: workspace_folder.map(str::to_string),
            remote_env: Some(
                [("PROBE".to_string(), Some(probe.to_string()))]
                    .into_iter()
                    .collect(),
            ),
            post_create_command: Some(serde_json::json!(probe)),
            ..Default::default()
        };
        // Exactly what `execute_set_up` builds: `without_workspace` anchored at the cwd,
        // with the token's value taken from the AUTHORED `--config` document.
        let anchor = std::env::current_dir().expect("cwd is readable");
        let substitute = |config: &DevContainerConfig| {
            let mut context = SubstitutionContext::without_workspace(&anchor)
                .expect("context builds from the cwd");
            context.container_workspace_folder = config.workspace_folder.clone();
            config.apply_variable_substitution(&context).0
        };

        let authored = substitute(&config_with(Some("/work")));
        assert_eq!(
            authored.remote_env.as_ref().unwrap()["PROBE"].as_deref(),
            Some("cwf=/work base=work"),
            "an authored workspaceFolder answers both tokens in the reported block"
        );
        assert_eq!(
            authored.post_create_command.as_ref().unwrap(),
            &serde_json::json!("cwf=/work base=work"),
            "and in the command set-up EXECS, from the same context"
        );

        let unauthored = substitute(&config_with(None));
        assert_eq!(
            unauthored.remote_env.as_ref().unwrap()["PROBE"].as_deref(),
            Some(probe),
            "with no authored workspaceFolder the tokens stay literal in the report"
        );
        assert_eq!(
            unauthored.post_create_command.as_ref().unwrap(),
            &serde_json::json!(probe),
            "and in the command, where an invented `/` used to appear instead"
        );
    }

    /// #616: the container pass resolves `${containerEnv:*}` in the blocks set-up
    /// REPORTS and leaves the commands it EXECS alone — two surfaces that must end up
    /// with DIFFERENT values from one authored string, which is exactly what the
    /// reference does (measured at oracle 0.87.0: its reported `postCreateCommand` reads
    /// `'from-container-env' ''` while the artifact its hook wrote reads
    /// `cenv=[${containerEnv:SETUP_PROBE_VAR}]`).
    ///
    /// The two #510/#513 characterizations are asserted in the same test rather than
    /// trusted: the container pass reuses the context pass 1 built, so a future version
    /// that rebuilt it with the workspace-anchored constructor would resolve
    /// `${localWorkspaceFolder}` / `${devcontainerId}` here and this is what catches it.
    #[test]
    fn set_up_container_pass_resolves_container_env_only_in_the_reported_blocks() {
        use crate::commands::shared::container_substitution::container_substituted_with_context;

        let probe = "cenv=${containerEnv:SETUP_PROBE_VAR} \
                     missing=${containerEnv:NO_SUCH_VAR} \
                     ws=${localWorkspaceFolder} id=${devcontainerId}";
        let config = DevContainerConfig {
            remote_env: Some(
                [("PROBE".to_string(), Some(probe.to_string()))]
                    .into_iter()
                    .collect(),
            ),
            post_create_command: Some(serde_json::json!(probe)),
            ..Default::default()
        };

        // Exactly what `execute_set_up` builds.
        let anchor = std::env::current_dir().expect("cwd is readable");
        let context =
            SubstitutionContext::without_workspace(&anchor).expect("context builds from the cwd");
        let executed = config.apply_variable_substitution(&context).0;

        let container_env = HashMap::from([(
            "SETUP_PROBE_VAR".to_string(),
            "from-container-env".to_string(),
        )]);
        let reported =
            container_substituted_with_context(&executed, &context, Some(&container_env));

        assert_eq!(
            reported.remote_env.as_ref().unwrap()["PROBE"].as_deref(),
            Some(
                "cenv=from-container-env missing= \
                 ws=${localWorkspaceFolder} id=${devcontainerId}"
            ),
            "the reported block resolves the container variables — a key the container \
             does not define becomes the empty string, as the reference does — while the \
             workspace variables stay literal (#510)"
        );
        assert_eq!(
            executed.post_create_command.as_ref().unwrap(),
            &serde_json::json!(probe),
            "and what set-up EXECS is untouched: the reference hands the raw token to the \
             container shell, so folding the pass into the executed config would diverge"
        );
    }

    /// #616 fail-safe, the same rule #613 set for `up`: with no container environment in
    /// hand the pass is SKIPPED so the template survives. Running it against an absent
    /// environment would be worse than not running it — `resolve_variable` answers
    /// `Some("")` for a missing key once `container_env` is `Some`, so every reference
    /// would collapse to an empty string and the caller could not tell "the container has
    /// no such variable" from "deacon could not read the container".
    #[test]
    fn set_up_container_pass_preserves_templates_without_a_container_env() {
        use crate::commands::shared::container_substitution::container_substituted_with_context;

        let config = DevContainerConfig {
            remote_env: Some(
                [(
                    "PROBE".to_string(),
                    Some("${containerEnv:SETUP_PROBE_VAR}".to_string()),
                )]
                .into_iter()
                .collect(),
            ),
            ..Default::default()
        };
        let anchor = std::env::current_dir().expect("cwd is readable");
        let context =
            SubstitutionContext::without_workspace(&anchor).expect("context builds from the cwd");

        let reported = container_substituted_with_context(&config, &context, None);
        assert_eq!(
            reported.remote_env.as_ref().unwrap()["PROBE"].as_deref(),
            Some("${containerEnv:SETUP_PROBE_VAR}"),
            "an unreadable container environment leaves the template intact"
        );
    }

    /// A `--config` document and the metadata-merged config that set-up RUNS, so the two
    /// candidate inputs to the `configuration` block are distinguishable. #502's defect
    /// was passing the second where the reference reports the first.
    fn authored_and_merged() -> (DevContainerConfig, DevContainerConfig) {
        let mut labels = HashMap::new();
        labels.insert(
            "devcontainer.metadata".to_string(),
            r#"[{"remoteUser": "root", "containerEnv": {"FROM_LABEL": "1"}}]"#.to_string(),
        );
        let container = make_container("abc", "alpine:3.18", labels);
        let metadata = extract_image_metadata_config(&container).unwrap();
        let authored = DevContainerConfig {
            name: Some("Overlay".to_string()),
            post_create_command: Some(serde_json::json!("echo overlay-postCreate")),
            ..DevContainerConfig::default()
        };
        let merged = merge_configs(&authored, metadata.as_ref()).unwrap();
        (authored, merged)
    }

    /// #502: the `configuration` block echoes the `--config` document AS AUTHORED. A
    /// property that lives only on the container's `devcontainer.metadata` label belongs
    /// to `mergedConfiguration`, never here — reporting it in this block claims the caller
    /// wrote something they did not.
    ///
    /// The merged config is asserted to CARRY those properties in the same test: without
    /// that half, a `configuration_document` that dropped them for some unrelated reason
    /// would look like a pass.
    #[test]
    fn configuration_reports_the_authored_document_not_the_metadata_merge() {
        let (authored, merged) = authored_and_merged();

        let block = configuration_document(&authored, None).expect("shaping the authored config");
        assert_eq!(block["name"], serde_json::json!("Overlay"));
        assert_eq!(
            block["postCreateCommand"],
            serde_json::json!("echo overlay-postCreate")
        );
        for folded in ["remoteUser", "containerEnv"] {
            assert!(
                block.get(folded).is_none(),
                "`{folded}` came from the container label, not the caller's --config: {block:#}"
            );
        }

        let merged_block = configuration_document(&merged, None).expect("shaping the merged one");
        assert_eq!(
            merged_block["remoteUser"],
            serde_json::json!("root"),
            "the merge genuinely folds the label in — which is why passing it here was the bug"
        );
        assert_eq!(
            merged_block["containerEnv"]["FROM_LABEL"],
            serde_json::json!("1")
        );
    }

    /// `configFilePath` rides on the `configuration` block under the same caller-named
    /// rule as the merged one, and is absent with no `--config` — where the reference
    /// emits a bare `"configuration": {}` (measured at oracle 0.87.0 on both shapes).
    #[test]
    fn configuration_reports_config_file_path_only_when_config_was_named() {
        let (authored, _) = authored_and_merged();

        let named = configuration_document(&authored, Some(Path::new("/ws/overlay.json")))
            .expect("shaping a named config succeeds");
        assert_eq!(named["configFilePath"]["scheme"], serde_json::json!("file"));
        assert!(
            named["configFilePath"]["fsPath"]
                .as_str()
                .is_some_and(|p| p.ends_with("overlay.json")),
            "the reported path names the file the caller passed: {named:#}"
        );

        let empty = configuration_document(&DevContainerConfig::default(), None)
            .expect("shaping an empty config succeeds");
        assert!(empty.get("configFilePath").is_none());
        assert_eq!(
            empty,
            serde_json::json!({}),
            "no --config means the empty document the reference emits, not a skeleton"
        );
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
                let mut m = deacon_core::IndexMap::new();
                m.insert("FROM_CONTAINER".to_string(), "c".to_string());
                m.insert("OVERRIDDEN".to_string(), "from-config".to_string());
                Some(m)
            },
            remote_env: {
                let mut m = deacon_core::IndexMap::new();
                m.insert("FROM_REMOTE".to_string(), Some("r".to_string()));
                m.insert("DROPPED".to_string(), None); // None-valued keys are skipped
                Some(m)
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
                let mut m = deacon_core::IndexMap::new();
                m.insert("ZED".to_string(), "z".to_string());
                m.insert("ALPHA".to_string(), "a".to_string());
                m.insert("MID".to_string(), "m".to_string());
                Some(m)
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
