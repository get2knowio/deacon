//! Shared recovery of a container's `devcontainer.metadata` label into config.
//!
//! deacon stamps the merged `devcontainer.metadata` array on containers it
//! creates (#322), and reads it back — without the workspace — from
//! `set-up`, `read-configuration --container-id`, and `exec --container-id`.
//! This is the single parser those callers share (CLAUDE.md principle 6),
//! rather than each re-implementing the fold.

use anyhow::{Context, Result};

use deacon_core::config::{ConfigMerger, DevContainerConfig};
use deacon_core::docker::ContainerInfo;

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
