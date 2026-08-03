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

use deacon_core::config::{ConfigMerger, DevContainerConfig};
use deacon_core::docker::{ContainerInfo, Docker};

/// Extract a merged [`DevContainerConfig`] from a container's
/// `devcontainer.metadata` label.
///
/// The label is a JSON array of metadata fragments (tolerating the legacy
/// single-object form) that [`ConfigMerger`] folds together. A missing label is
/// NOT an error — many containers aren't built by `deacon up` — so this returns
/// `Ok(None)` and the caller falls back to whatever config it already has.
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

    Ok(Some(ConfigMerger::merge_configs(&configs)))
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
