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
use deacon_core::variable::SubstitutionContext;

/// Extract a merged [`DevContainerConfig`] from a container's
/// `devcontainer.metadata` label.
///
/// The label is a JSON array of metadata fragments (tolerating the legacy
/// single-object form) that [`ConfigMerger`] folds together. A missing label is
/// NOT an error — many containers aren't built by `deacon up` — so this returns
/// `Ok(None)` and the caller falls back to whatever config it already has.
///
/// ## Substitution
///
/// The label stores the **authored** configuration, templates intact (T115), so the
/// recovered fragments still contain `${...}` tokens. The callers of this function
/// are the `--container-id` paths, which by definition have no workspace folder, so
/// a [`SubstitutionContext::host_env_only`] pass runs here: `${localEnv:VAR}` /
/// `${env:VAR}` resolve against the host, while `${localWorkspaceFolder}` and
/// `${containerWorkspaceFolder}` stay literal (there is nothing to resolve them
/// against) and `${containerEnv:VAR}` stays literal for the later env-probe pass.
///
/// This is the reference CLI's measured behavior, not a guess. With pinned oracle
/// 0.87.0, `devcontainer exec --container-id <id>` over a container labelled
/// `remoteEnv: { LWF: "${localWorkspaceFolder}", LENV: "${localEnv:HOME}" }` prints
/// `LWF=[${localWorkspaceFolder}] LENV=[/home/vscode]`.
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

    let merged = ConfigMerger::merge_configs(&configs);
    let context = SubstitutionContext::host_env_only();
    let (substituted, _report) = merged.apply_variable_substitution(&context);
    Ok(Some(substituted))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn container(labels: HashMap<String, String>) -> ContainerInfo {
        ContainerInfo {
            id: "cid".to_string(),
            names: vec![],
            image: "debian:bookworm-slim".to_string(),
            status: "running".to_string(),
            state: "running".to_string(),
            exposed_ports: vec![],
            port_mappings: vec![],
            env: HashMap::new(),
            labels,
            mounts: vec![],
        }
    }

    fn container_with_label(label: &str) -> ContainerInfo {
        container(HashMap::from([(
            "devcontainer.metadata".to_string(),
            label.to_string(),
        )]))
    }

    /// T115: the label now carries authored templates, so the read-back applies a
    /// host-env-only pass. Pins the three outcomes measured on pinned oracle
    /// 0.87.0's `exec --container-id`: host-env token resolved, workspace tokens
    /// literal (nothing to resolve them against), container-env token deferred to
    /// the later env-probe pass.
    #[test]
    fn read_back_resolves_host_env_and_leaves_workspace_tokens_literal() {
        // `PATH` rather than a var this test sets: `unsafe_code` is denied
        // workspace-wide, so `set_var` is unavailable, and PATH is always present.
        let want = std::env::var("PATH").expect("PATH is set");

        let cfg = config_from_metadata_label(&container_with_label(
            r#"[{"remoteEnv":{
                 "HOSTENV":"${localEnv:PATH}",
                 "LWF":"${localWorkspaceFolder}",
                 "CWF":"${containerWorkspaceFolder}",
                 "CENV":"${containerEnv:PATH}"
               }}]"#,
        ))
        .unwrap()
        .expect("label present");

        let get = |k: &str| cfg.remote_env.get(k).cloned().flatten().unwrap_or_default();
        assert_eq!(get("HOSTENV"), want, "${{localEnv:…}} must resolve");
        assert_eq!(
            get("LWF"),
            "${localWorkspaceFolder}",
            "no workspace exists on the --container-id path, so the token stays literal \
             rather than collapsing to the empty string"
        );
        assert_eq!(
            get("CWF"),
            "${containerWorkspaceFolder}",
            "container workspace folder is not known here either"
        );
        assert_eq!(
            get("CENV"),
            "${containerEnv:PATH}",
            "deferred to the env-probe pass, which is the only place the container \
             environment is known"
        );
    }

    #[test]
    fn missing_label_is_not_an_error() {
        assert!(
            config_from_metadata_label(&container(HashMap::new()))
                .unwrap()
                .is_none()
        );
    }
}
