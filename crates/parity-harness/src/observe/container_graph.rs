//! Container-graph observer (`chan-process-graph`, T054, FR-012): the container's mount
//! / network / volume graph.
//!
//! Captures `.Mounts` (normalized to `{ source, target, type, ro }`), the network names
//! from `.NetworkSettings.Networks`, and the volume names (volume-type mounts). The
//! shared normalizer applies `mount_source_canonical` to each mount `source` and
//! `path_token` elsewhere (`normalize::normalize_process_graph`); nothing is removed
//! (FR-029).

use deacon_conformance::model::{CHAN_PROCESS_GRAPH, Operation};
use serde_json::{Value, json};

use crate::HarnessError;
use crate::evidence::RawChannelEvidence;
use crate::observe::{ChannelObserver, RunContext, docker_inspect, not_captured};

/// Captures `chan-process-graph` from the case's container.
#[derive(Debug, Clone, Copy)]
pub struct ContainerGraphObserver;

impl ChannelObserver for ContainerGraphObserver {
    fn channel(&self) -> &'static str {
        CHAN_PROCESS_GRAPH
    }

    fn capture(
        &self,
        ctx: &RunContext,
        op: &Operation,
    ) -> Result<RawChannelEvidence, HarnessError> {
        let Some(id) = &ctx.container_id else {
            return Ok(not_captured(CHAN_PROCESS_GRAPH, &op.id));
        };
        let Some(inspect) = docker_inspect(id)? else {
            return Ok(not_captured(CHAN_PROCESS_GRAPH, &op.id));
        };

        // Mounts → { source, target, type, ro }; volume mounts also contribute a volume.
        let mut mounts = Vec::new();
        let mut volumes = Vec::new();
        if let Some(arr) = inspect["Mounts"].as_array() {
            for m in arr {
                let mount_type = m["Type"].as_str().unwrap_or("").to_string();
                let source = m["Source"].as_str().unwrap_or("").to_string();
                mounts.push(json!({
                    "source": source,
                    "target": m["Destination"].as_str().unwrap_or(""),
                    "type": mount_type,
                    "ro": !m["RW"].as_bool().unwrap_or(true),
                }));
                if mount_type == "volume" {
                    if let Some(name) = m["Name"].as_str() {
                        volumes.push(Value::String(name.to_string()));
                    }
                }
            }
        }

        let networks: Vec<Value> = inspect["NetworkSettings"]["Networks"]
            .as_object()
            .map(|o| o.keys().map(|k| Value::String(k.clone())).collect())
            .unwrap_or_default();

        Ok(RawChannelEvidence {
            channel: CHAN_PROCESS_GRAPH.to_string(),
            operation: op.id.clone(),
            present: true,
            value: json!({
                "mounts": mounts,
                "networks": networks,
                "volumes": volumes,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_container_is_not_captured() {
        let ctx = RunContext::new(std::path::PathBuf::from("/tmp"));
        let op = Operation {
            id: "op".to_string(),
            subcommand: "up".to_string(),
            ..Operation::default()
        };
        assert!(!ContainerGraphObserver.capture(&ctx, &op).unwrap().present);
    }
}
