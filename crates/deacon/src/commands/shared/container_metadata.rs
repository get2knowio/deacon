//! Shared recovery of a container's `devcontainer.metadata` label into config.
//!
//! deacon stamps the merged `devcontainer.metadata` array on containers it
//! creates (#322), and reads it back — without the workspace — from
//! `set-up`, `read-configuration --container-id`, and `exec --container-id`.
//! This is the single parser those callers share (CLAUDE.md principle 6),
//! rather than each re-implementing the fold.
//!
//! The module also owns the other direction — resolving a config the caller
//! already loaded from the workspace AGAINST a running container, which folds in
//! the container's IMAGE metadata. `exec` and `run-user-commands` share it (#405).

use std::path::Path;

use anyhow::{Context, Result};

use deacon_core::config::{ConfigMerger, DevContainerConfig, LifecycleHookLayer};
use deacon_core::docker::{ContainerInfo, Docker};

/// Extract a merged [`DevContainerConfig`] from a container's
/// `devcontainer.metadata` label.
///
/// The label is a JSON array of metadata fragments (tolerating the legacy
/// single-object form) that [`ConfigMerger`] folds together. A missing label is
/// NOT an error — many containers aren't built by `deacon up` — so this returns
/// `Ok(None)` and the caller falls back to whatever config it already has.
///
/// # Lifecycle hooks do not fold (#477)
///
/// The fold is last-wins, which is right for every row of the spec's
/// image-metadata Merge Logic table EXCEPT the five lifecycle hooks — each of
/// those is a "Collected list of all `<phase>Command`s", so every fragment that
/// declares a phase contributes a hook that RUNS, in fragment order. deacon's
/// own `up` stamps `[...image entries, ...feature entries, config entry]`
/// (`build_container_metadata_label`), so a phase two of those sources declare
/// arrives here as two fragments — and left this function as one.
///
/// The hooks are therefore lifted OFF the merged configuration and onto
/// [`DevContainerConfig::metadata_lifecycle_layers`], where
/// `container_lifecycle::aggregate_lifecycle_commands` replays them in order —
/// the same carrier and the same aggregation #467 introduced for the IMAGE's
/// label, given one more source rather than a second mechanism.
///
/// Every fragment of this label sits BELOW anything the caller layers on top
/// (`set-up`'s `--config`), so ALL of them become layers and the five singular
/// fields are left empty. That is the invariant callers depend on: each hook has
/// exactly one home, so a caller that both merges this config and aggregates its
/// layers cannot run the same hook twice.
pub fn config_from_metadata_label(container: &ContainerInfo) -> Result<Option<DevContainerConfig>> {
    let Some(label) = container.labels.get("devcontainer.metadata") else {
        return Ok(None);
    };

    let value: serde_json::Value = serde_json::from_str(label).with_context(|| {
        format!(
            "Failed to parse devcontainer.metadata label as JSON for container '{}'",
            container.id
        )
    })?;

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

    // Collected BEFORE the fold, because after it only the last fragment's hook
    // per phase survives. A fragment declaring no hook at all (the common
    // `{"remoteUser": …}` base-image shape) contributes no layer.
    let hook_layers: Vec<LifecycleHookLayer> = configs
        .iter()
        .filter_map(LifecycleHookLayer::from_config)
        .collect();

    let mut merged = ConfigMerger::merge_configs(&configs);

    // One home per hook: clear the five singular fields the fold just last-won,
    // since every one of their values is now carried as a layer. Leaving a copy
    // behind would make `aggregate_lifecycle_commands` queue it twice.
    LifecycleHookLayer::default().apply_to(&mut merged);
    // Assignment, not append: the fragments were freshly deserialized from the
    // label and `metadata_lifecycle_layers` is `#[serde(skip)]`, so the fold
    // concatenated nothing and `merged`'s vec is empty by construction.
    merged.metadata_lifecycle_layers = hook_layers;

    Ok(Some(merged))
}

/// Resolve a workspace-loaded config against a RUNNING container: fold the
/// container's image `devcontainer.metadata` label in at lower precedence, then
/// resolve the effective configuration against the container's own labels.
///
/// Why this is shared (#405): `up` merges image metadata while creating the
/// container, so any subcommand that later attaches to that container has to
/// re-apply the same merge or it will disagree with `up` about `remoteUser`,
/// `remoteEnv` and the lifecycle hooks the image contributes. `exec` did this
/// inline (#223); `run-user-commands` did not, and silently ran hooks as `root`
/// with none of the image's environment. Measured against the pinned reference
/// CLI 0.87.0, whose `run-user-commands` honors all three. This is the single
/// implementation both share (CLAUDE.md principle 6).
///
/// Best-effort by construction: an image that cannot be inspected, an absent
/// label, or a label that fails to parse all leave the caller's config
/// unchanged (see `merge_image_metadata_after_image_ready`), and a failed
/// effective-config resolution warns and returns the merged base.
pub async fn resolve_config_against_container<D: Docker>(
    docker: &D,
    container: &ContainerInfo,
    config: DevContainerConfig,
    workspace_folder: &Path,
) -> DevContainerConfig {
    let base = crate::commands::up::merged_config::merge_image_metadata_after_image_ready(
        docker,
        &container.image,
        config,
    )
    .await;

    match ConfigMerger::resolve_effective_config(&base, Some(&container.labels), workspace_folder) {
        Ok((resolved, _report)) => resolved,
        Err(e) => {
            tracing::warn!("Failed to resolve effective config with labels: {}", e);
            base
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::docker::ContainerInfo;
    use std::collections::HashMap;

    fn container_with_metadata(label: &str) -> ContainerInfo {
        let mut labels = HashMap::new();
        labels.insert("devcontainer.metadata".to_string(), label.to_string());
        ContainerInfo {
            id: "c0ffee".to_string(),
            names: vec![],
            image: "alpine:3.18".to_string(),
            status: "running".to_string(),
            state: "running".to_string(),
            exposed_ports: vec![],
            port_mappings: vec![],
            env: HashMap::new(),
            labels,
            mounts: vec![],
        }
    }

    /// #477: the label's own fragments are a COLLECTION for the five lifecycle
    /// hooks, not a last-wins fold. `deacon up` stamps
    /// `[...image entries, ...feature entries, config entry]`, so a phase two of
    /// those sources declare arrives as two fragments — and used to leave as one.
    #[test]
    fn label_fragments_contribute_one_hook_layer_each_in_order() {
        let container = container_with_metadata(
            r#"[
                {"onCreateCommand": "e0-onCreate", "postCreateCommand": "e0-postCreate"},
                {"id": "ghcr.io/example/feat:1", "onCreateCommand": "feat-onCreate"},
                {"onCreateCommand": "e2-onCreate", "postCreateCommand": "e2-postCreate"}
            ]"#,
        );

        let cfg = config_from_metadata_label(&container).unwrap().unwrap();

        let layers = &cfg.metadata_lifecycle_layers;
        assert_eq!(
            layers.len(),
            3,
            "every fragment declaring a hook must contribute a layer, in label order"
        );
        assert_eq!(
            layers[0].on_create_command,
            Some(serde_json::json!("e0-onCreate"))
        );
        assert_eq!(
            layers[1].on_create_command,
            Some(serde_json::json!("feat-onCreate")),
            "a FEATURE entry's hook is a layer like any other — this is the \
             contribution set-up never aggregated at all"
        );
        assert_eq!(
            layers[2].on_create_command,
            Some(serde_json::json!("e2-onCreate"))
        );
        // The hookless phase of the feature entry stays empty rather than
        // inheriting a neighbour's.
        assert_eq!(layers[1].post_create_command, None);
    }

    /// The other half, and the trap #467's fix documented: a hook lifted onto a
    /// layer must NOT also remain in the singular field, or a caller that both
    /// merges this config and aggregates its layers runs it twice.
    #[test]
    fn lifted_hooks_leave_the_singular_fields_empty() {
        let container = container_with_metadata(
            r#"[
                {"onCreateCommand": "e0-onCreate"},
                {"postStartCommand": "e1-postStart"}
            ]"#,
        );

        let cfg = config_from_metadata_label(&container).unwrap().unwrap();

        assert_eq!(cfg.metadata_lifecycle_layers.len(), 2);
        for (field, value) in [
            ("onCreateCommand", &cfg.on_create_command),
            ("updateContentCommand", &cfg.update_content_command),
            ("postCreateCommand", &cfg.post_create_command),
            ("postStartCommand", &cfg.post_start_command),
            ("postAttachCommand", &cfg.post_attach_command),
        ] {
            assert_eq!(
                *value, None,
                "{field} must have exactly one home — the layer — or aggregation queues it twice"
            );
        }
    }

    /// Collecting the hooks must not turn the last-wins rows of the Merge Logic
    /// table into collections. `exec --container-id` reads exactly these two
    /// scalars off this function (#322) and nothing else, which is why the
    /// change is invisible there.
    #[test]
    fn scalar_properties_still_last_win_across_fragments() {
        let container = container_with_metadata(
            r#"[
                {"remoteUser": "first", "onCreateCommand": "e0-onCreate"},
                {"remoteUser": "second", "remoteEnv": {"FROM_LABEL": "yes"}}
            ]"#,
        );

        let cfg = config_from_metadata_label(&container).unwrap().unwrap();

        assert_eq!(
            cfg.remote_user.as_deref(),
            Some("second"),
            "remoteUser is 'Last value wins' — the later fragment must win outright"
        );
        assert_eq!(
            cfg.remote_env().get("FROM_LABEL"),
            Some(&Some("yes".to_string()))
        );
        assert_eq!(cfg.metadata_lifecycle_layers.len(), 1);
    }

    /// A fragment carrying no hook at all (the common
    /// `mcr.microsoft.com/devcontainers/*` `{"remoteUser": …}` shape)
    /// contributes no layer rather than an empty one.
    #[test]
    fn hookless_label_contributes_no_layers() {
        let container = container_with_metadata(r#"[{"remoteUser": "vscode"}]"#);
        let cfg = config_from_metadata_label(&container).unwrap().unwrap();
        assert!(cfg.metadata_lifecycle_layers.is_empty());
        assert_eq!(cfg.remote_user.as_deref(), Some("vscode"));
    }
}
