//! Image observer (`chan-image`, T053, FR-011): the image configuration and metadata —
//! labels, env, entrypoint, cmd — of either the image a container was created FROM, or
//! the image a `build` operation PRODUCED.
//!
//! Captures the RAW `.Config` slice from `docker inspect`; the shared normalizer applies
//! `label_semantic` (labels → canonical key/value) and `null_preserving`
//! (`normalize::normalize_image`). Nothing is blanket-removed (FR-029).
//!
//! **Two sources, one channel, and a `source` discriminator saying which.** A `build`
//! leaves an image and no container, so its evidence comes from
//! [`RunContext::image_inspect`] rather than [`RunContext::container_inspect`]. The shapes
//! are otherwise identical because both read `.Config`, which is exactly why the
//! discriminator is recorded: without it, `"image": null` on built-image evidence would be
//! indistinguishable from a container whose config carried no image ref, and a reader
//! could not tell which question the evidence answers.
//!
//! **A built image's identity is deliberately NOT captured** — no `Id`, no `RepoDigests`,
//! no `RepoTags`. deacon and the reference build SEPARATELY into per-side tags, so every
//! one of those differs by construction and comparing them would report a divergence on
//! every run while saying nothing about behavior. What is comparable is what the build
//! PUT in the image: its labels, environment, entrypoint, command and working directory.

use crate::model::{CHAN_IMAGE, Operation};
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
        // Read the runner's pre-fetched inspects (finding #4) — no subprocess here.
        // A built image wins when both exist: an operation sequence that both builds and
        // brings up is asking about what it built.
        let (source, inspect) = match (&ctx.image_inspect, &ctx.container_inspect) {
            (Some(image), _) => ("image", image),
            (None, Some(container)) => ("container", container),
            (None, None) => return Ok(not_captured(CHAN_IMAGE, &op.id)),
        };
        let config = &inspect["Config"];
        let get = |k: &str| config.get(k).cloned().unwrap_or(serde_json::Value::Null);
        Ok(RawChannelEvidence {
            channel: CHAN_IMAGE.to_string(),
            operation: op.id.clone(),
            present: true,
            value: json!({
                "source": source,
                // The image ref a CONTAINER was created from is comparable; a built
                // image's own id is not (see the module docs), so it stays null there.
                "image": if source == "container" { get("Image") } else { serde_json::Value::Null },
                "labels": get("Labels"),
                "env": get("Env"),
                "entrypoint": get("Entrypoint"),
                "cmd": get("Cmd"),
                "workingDir": get("WorkingDir"),
                "user": get("User"),
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(subcommand: &str) -> Operation {
        Operation {
            id: "op".to_string(),
            subcommand: subcommand.to_string(),
            ..Operation::default()
        }
    }

    #[test]
    fn no_container_and_no_image_is_not_captured() {
        let ctx = RunContext::new(std::path::PathBuf::from("/tmp"));
        let ev = ImageObserver.capture(&ctx, &op("up")).unwrap();
        assert!(!ev.present, "nothing to inspect → not captured (FR-018)");
    }

    #[test]
    fn a_built_image_is_captured_without_its_identity() {
        let mut ctx = RunContext::new(std::path::PathBuf::from("/tmp"));
        ctx.image_inspect = Some(json!({
            "Id": "sha256:deadbeef",
            "RepoTags": ["dcr-1-0-img:latest"],
            "Config": { "Labels": {"parity.token": "t"}, "Env": ["A=b"], "WorkingDir": "/" },
        }));
        let ev = ImageObserver.capture(&ctx, &op("build")).unwrap();
        assert!(ev.present);
        assert_eq!(ev.value["source"], "image");
        assert_eq!(ev.value["labels"]["parity.token"], "t");
        // The two sides build separately into per-side tags, so identity is uncomparable
        // by construction and must not appear at all.
        assert!(
            ev.value["image"].is_null(),
            "a built image's id is not captured"
        );
        let rendered = ev.value.to_string();
        for absent in ["deadbeef", "RepoTags", "dcr-1-0-img"] {
            assert!(
                !rendered.contains(absent),
                "built-image evidence must not carry {absent}: {rendered}"
            );
        }
    }

    #[test]
    fn a_built_image_wins_over_a_container() {
        let mut ctx = RunContext::new(std::path::PathBuf::from("/tmp"));
        ctx.container_inspect = Some(json!({ "Config": { "Image": "alpine:3.19" } }));
        ctx.image_inspect = Some(json!({ "Config": { "Labels": {"built": "yes"} } }));
        let ev = ImageObserver.capture(&ctx, &op("build")).unwrap();
        assert_eq!(ev.value["source"], "image");
        assert_eq!(ev.value["labels"]["built"], "yes");
    }

    #[test]
    fn a_container_still_reports_the_image_it_was_created_from() {
        let mut ctx = RunContext::new(std::path::PathBuf::from("/tmp"));
        ctx.container_inspect = Some(json!({ "Config": { "Image": "alpine:3.19" } }));
        let ev = ImageObserver.capture(&ctx, &op("up")).unwrap();
        assert_eq!(ev.value["source"], "container");
        assert_eq!(ev.value["image"], "alpine:3.19");
    }
}
