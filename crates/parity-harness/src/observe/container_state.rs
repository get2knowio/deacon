//! Container-state observer (`chan-container-state`, 024 Phase 4): the whole observable
//! state of the container a case brought up, as ONE channel.
//!
//! This observer **delegates** to [`crate::normalize::container_state`], which is already
//! pure over a `docker inspect` object — exactly what the runner pre-fetches into
//! [`RunContext::container_inspect`]. There is no second derivation of container state
//! here (Constitution VIII): the legacy field-by-field comparison and this declarative
//! channel read the same snapshot, so they cannot drift.
//!
//! Emitted: `{mounts, env, labels, user, workingDir, exposedPorts, publishedPorts,
//! entrypoint, cmd, networks}` (the serialized [`StateSnapshot`]) plus the DERIVED
//! `workspaceBindTargets`, `envMap`, `pathSegments`, `mountSources`, `labelNamespaces`,
//! `userSpec` and `composeProjectResources` (024 US5, see [`crate::observe::derived`]).
//! The shared normalizer then applies `workspace_basename_token`,
//! `path_token` and `null_preserving`; nothing is removed (FR-029). Labels, entrypoint,
//! cmd and networks are emitted verbatim, and any characterized difference is declared on
//! the case as a scoped, backed `allowedDifference`.
//!
//! # Why `workspaceBindTargets` is derived HERE
//!
//! The claim it exists for is "there is NO mount at `/workspace` and there IS a bind
//! under `/workspaces/*` whose source is the workspace root". Expressed as an assertion
//! that is a quantified search over a map — a query language embedded in JSON, and once
//! `∃` exists, `∀`, negation and composition follow. Computed once by the observer it is
//! a plain list, and the cross-CLI claim becomes ordinary equality.
//!
//! **The rule this establishes for the whole feature: when an assertion needs a search,
//! add a derived field to the observer, not a search engine to the assertion language.**

use std::path::Path;

use crate::model::{CHAN_CONTAINER_STATE, Operation};
use serde_json::Value;

use crate::HarnessError;
use crate::evidence::RawChannelEvidence;
use crate::observe::{ChannelObserver, RunContext, derived, not_captured};

/// Captures `chan-container-state` from the case's container.
#[derive(Debug, Clone, Copy)]
pub struct ContainerStateObserver;

impl ChannelObserver for ContainerStateObserver {
    fn channel(&self) -> &'static str {
        CHAN_CONTAINER_STATE
    }

    fn capture(
        &self,
        ctx: &RunContext,
        op: &Operation,
    ) -> Result<RawChannelEvidence, HarnessError> {
        // Read the runner's pre-fetched inspect (finding #4) — no subprocess here, which
        // is also what lets the differential release deacon's side before the reference
        // runs (024 Phase 3, D-3).
        let Some(inspect) = &ctx.container_inspect else {
            return Ok(not_captured(CHAN_CONTAINER_STATE, &op.id));
        };

        // DELEGATE: one definition of container state, shared with the legacy comparison.
        // A malformed inspect (no `Config`) is a fail-loud normalization error, never a
        // silently empty snapshot.
        let snapshot = crate::normalize::container_state(&op.id, inspect)?;
        let mut value =
            serde_json::to_value(&snapshot).map_err(|e| HarnessError::NormalizationFailed {
                channel: CHAN_CONTAINER_STATE.to_string(),
                cause: format!("container state snapshot did not serialize: {e}"),
            })?;

        let targets = workspace_bind_targets(inspect, &ctx.workspace);
        match value.as_object_mut() {
            Some(obj) => {
                obj.insert("workspaceBindTargets".to_string(), targets);
                // The US5 derived fields (T122): each turns a comparison that would
                // otherwise need a search, a grouping or a type test into ordinary
                // equality. All are ADDITIVE — the raw `env` / `mounts` / `labels` /
                // `user` / `networks` fields they summarize stay compared alongside them.
                let env_map = derived::env_map(obj.get("env").unwrap_or(&Value::Null));
                obj.insert("pathSegments".to_string(), derived::path_segments(&env_map));
                obj.insert("envMap".to_string(), env_map);
                obj.insert("mountSources".to_string(), derived::mount_sources(inspect));
                obj.insert(
                    "labelNamespaces".to_string(),
                    derived::label_namespaces(obj.get("labels").unwrap_or(&Value::Null)),
                );
                obj.insert(
                    "userSpec".to_string(),
                    derived::user_spec(obj.get("user").and_then(Value::as_str).unwrap_or("")),
                );
                let project = inspect["Config"]["Labels"]["com.docker.compose.project"]
                    .as_str()
                    .unwrap_or("");
                obj.insert(
                    "composeProjectResources".to_string(),
                    derived::compose_project_resources(inspect, project),
                );
            }
            None => {
                return Err(HarnessError::NormalizationFailed {
                    channel: CHAN_CONTAINER_STATE.to_string(),
                    cause: "serialized container state is not a JSON object".to_string(),
                });
            }
        }

        Ok(RawChannelEvidence {
            channel: CHAN_CONTAINER_STATE.to_string(),
            operation: op.id.clone(),
            present: true,
            value,
        })
    }
}

/// The DERIVED `workspaceBindTargets`: the destinations of every **bind** mount whose
/// source is the workspace root, sorted and deduplicated for determinism.
///
/// "Is the workspace root" is matched against both the path as given and its
/// canonicalized form, because the daemon reports the resolved source (`/tmp` is a
/// symlink to `/private/tmp` on macOS) while the runner holds the path it created. A
/// canonicalization failure is not an error — the workspace may be a path the current
/// process cannot resolve — it simply falls back to the literal comparison.
fn workspace_bind_targets(inspect: &Value, workspace: &Path) -> Value {
    let literal = workspace.to_string_lossy().into_owned();
    let canonical = std::fs::canonicalize(workspace)
        .ok()
        .map(|p| p.to_string_lossy().into_owned());

    let mut targets: Vec<String> = Vec::new();
    if let Some(mounts) = inspect["Mounts"].as_array() {
        for m in mounts {
            if m["Type"].as_str() != Some("bind") {
                continue;
            }
            let source = m["Source"].as_str().unwrap_or("");
            let is_workspace =
                source == literal || canonical.as_deref().is_some_and(|c| source == c);
            if !is_workspace {
                continue;
            }
            if let Some(dest) = m["Destination"].as_str() {
                if !dest.is_empty() {
                    targets.push(dest.to_string());
                }
            }
        }
    }
    targets.sort();
    targets.dedup();
    Value::Array(targets.into_iter().map(Value::String).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    fn op() -> Operation {
        Operation {
            id: "op-up".to_string(),
            subcommand: "up".to_string(),
            ..Operation::default()
        }
    }

    #[test]
    fn no_container_is_not_captured() {
        let ctx = RunContext::new(std::path::PathBuf::from("/tmp/ws"));
        let ev = ContainerStateObserver
            .capture(&ctx, &op())
            .expect("capture");
        assert!(!ev.present, "no inspect → not-captured (FR-018)");
        assert_eq!(ev.channel, CHAN_CONTAINER_STATE);
    }

    #[test]
    fn a_malformed_inspect_fails_loud() {
        // No `Config` object → the delegated normalizer rejects it. A silently empty
        // snapshot would compare equal to another silently empty one (the 024 D-2 shape).
        let mut ctx = RunContext::new(std::path::PathBuf::from("/tmp/ws"));
        ctx.container_inspect = Some(json!({ "Mounts": [] }));
        assert!(ContainerStateObserver.capture(&ctx, &op()).is_err());
    }

    #[test]
    fn workspace_bind_targets_selects_only_workspace_rooted_binds() {
        let inspect = json!({
            "Mounts": [
                { "Type": "bind", "Source": "/tmp/ws", "Destination": "/workspaces/ws", "RW": true },
                { "Type": "bind", "Source": "/tmp/elsewhere", "Destination": "/other", "RW": true },
                { "Type": "volume", "Name": "v", "Source": "/tmp/ws", "Destination": "/vol", "RW": true },
            ]
        });
        assert_eq!(
            workspace_bind_targets(&inspect, Path::new("/tmp/ws")),
            json!(["/workspaces/ws"]),
            "only BIND mounts rooted at the workspace count"
        );
    }
}
