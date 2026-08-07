//! Temporal observer (`chan-temporal`, T056, FR-014): lifecycle-transition state of the
//! case's container — current status, first-create vs restart, and cleanup.
//!
//! Captures DETERMINISTIC transition markers from `docker inspect`: `.State.Status`
//! (`created` / `running` / `exited` / `paused`), `.State.Running`, and `.RestartCount`
//! (docker's restart-POLICY auto-restart count — a manual restart does NOT change it).
//! Wall-clock timestamps (`StartedAt` / `FinishedAt`) are DELIBERATELY excluded — they
//! are non-deterministic and would break byte-stable snapshots (contract
//! observer-channel.md: ordering-preserving plus `null_preserving`, no timestamps). A
//! removed container (cleanup) is `present:false`, which is exactly how the cleanup
//! transition is observed (FR-018).
//!
//! The `first-create vs restart` DISTINCTION is a metamorphic RELATIONSHIP across two
//! `up` operations (US6) — it compares two temporal captures — not a property derivable
//! from a single inspect (deacon's up-reuse does not bump docker's `RestartCount`).
//! Likewise fine-grained lifecycle-COMMAND ordering (onCreate then postCreate then more)
//! is surfaced via the fixture's marker files on the filesystem channel; this channel
//! captures the container-STATE transitions.

use crate::model::{CHAN_TEMPORAL, Operation};
use serde_json::json;

use crate::HarnessError;
use crate::evidence::RawChannelEvidence;
use crate::observe::{ChannelObserver, RunContext, not_captured};

/// Captures `chan-temporal` from the case's container.
#[derive(Debug, Clone, Copy)]
pub struct TemporalObserver;

impl ChannelObserver for TemporalObserver {
    fn channel(&self) -> &'static str {
        CHAN_TEMPORAL
    }

    fn capture(
        &self,
        ctx: &RunContext,
        op: &Operation,
    ) -> Result<RawChannelEvidence, HarnessError> {
        // No container OR it was cleaned up: the cleanup transition is exactly a
        // not-captured temporal channel (FR-018). Reads the runner's pre-fetched inspect
        // (finding #4) — no subprocess here.
        let Some(inspect) = &ctx.container_inspect else {
            return Ok(not_captured(CHAN_TEMPORAL, &op.id));
        };
        let mut value = temporal_from_inspect(inspect);
        // The per-container transition markers above say nothing about how many
        // containers the run LEFT BEHIND, which is its own lifecycle-transition fact and
        // the one #371 is about: deacon recreates on a changed configuration, so a
        // workspace can hold several generations, and only one of them should be live.
        // Added here rather than in `temporal_from_inspect` because that function is also
        // the metamorphic oracle's per-op snapshot, which compares container IDENTITY
        // across two operations and has no use for a census.
        if let (Some(obj), Some(census)) = (value.as_object_mut(), ctx.workspace_containers) {
            obj.insert(
                "workspaceContainers".to_string(),
                json!({ "total": census.total, "running": census.running }),
            );
        }
        Ok(RawChannelEvidence {
            channel: CHAN_TEMPORAL.to_string(),
            operation: op.id.clone(),
            present: true,
            value,
        })
    }
}

/// Build the deterministic `chan-temporal` value (status / running / restartCount, no
/// timestamps) from a `docker inspect` object. Shared by the observer and the runner's
/// per-op snapshot capture (US6 metamorphic evaluation).
pub(crate) fn temporal_from_inspect(inspect: &serde_json::Value) -> serde_json::Value {
    json!({
        "status": inspect["State"]["Status"].clone(),
        "running": inspect["State"]["Running"].clone(),
        "restartCount": inspect["RestartCount"].as_i64().unwrap_or(0),
    })
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
        assert!(!TemporalObserver.capture(&ctx, &op).unwrap().present);
    }
}
