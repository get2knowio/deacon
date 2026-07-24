//! Image observer (`chan-image`, T053, FR-011): the built-image configuration and
//! metadata a container was created from — image ref, labels, env, entrypoint, cmd.
//!
//! Captures the RAW `.Config` slice from `docker inspect <container>`; the shared
//! normalizer applies `label_semantic` (labels → canonical key/value) and
//! `null_preserving` (`normalize::normalize_image`). Nothing is blanket-removed (FR-029).

use deacon_conformance::model::{CHAN_IMAGE, Operation};
use serde_json::json;

use crate::HarnessError;
use crate::evidence::RawChannelEvidence;
use crate::observe::{ChannelObserver, RunContext, not_captured};

/// Captures `chan-image` from the case's container.
#[derive(Debug, Clone, Copy)]
pub struct ImageObserver;

impl ChannelObserver for ImageObserver {
    fn channel(&self) -> &'static str {
        CHAN_IMAGE
    }

    fn capture(
        &self,
        ctx: &RunContext,
        op: &Operation,
    ) -> Result<RawChannelEvidence, HarnessError> {
        // Read the runner's pre-fetched inspect (finding #4) — no subprocess here.
        let Some(inspect) = &ctx.container_inspect else {
            return Ok(not_captured(CHAN_IMAGE, &op.id));
        };
        let config = &inspect["Config"];
        let get = |k: &str| config.get(k).cloned().unwrap_or(serde_json::Value::Null);
        Ok(RawChannelEvidence {
            channel: CHAN_IMAGE.to_string(),
            operation: op.id.clone(),
            present: true,
            value: json!({
                "image": get("Image"),
                "labels": get("Labels"),
                "env": get("Env"),
                "entrypoint": get("Entrypoint"),
                "cmd": get("Cmd"),
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
        let ev = ImageObserver.capture(&ctx, &op).unwrap();
        assert!(!ev.present, "no container id → not captured (FR-018)");
    }
}
