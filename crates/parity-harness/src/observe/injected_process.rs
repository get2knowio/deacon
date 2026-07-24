//! Injected-process observer (`chan-injected-process`, T055, FR-013): the process
//! context a command runs under inside the case's container — environment, user, cwd,
//! PATH, and TTY.
//!
//! Captures the container's `.Config` process context (`User`, `WorkingDir`, `Env`,
//! `Tty`); `env` is parsed into an object and `path` is the `PATH` env value. The shared
//! normalizer applies `path_env_segmented` (PATH → segments, with the executable-probe
//! resolution seam left from US3), `null_preserving`, and `path_token`
//! (`normalize::normalize_injected_process`). Signal forwarding and exit propagation are
//! exec-semantic and are surfaced through the CLI-process channels (exit-code) — the
//! configured process context (env/user/cwd/PATH/TTY) is what this channel captures.

use deacon_conformance::model::{CHAN_INJECTED_PROCESS, Operation};
use serde_json::{Value, json};

use crate::HarnessError;
use crate::evidence::RawChannelEvidence;
use crate::observe::{
    ChannelObserver, RunContext, docker_inspect, env_array_to_object, not_captured,
};

/// Captures `chan-injected-process` from the case's container.
#[derive(Debug, Clone, Copy)]
pub struct InjectedProcessObserver;

impl ChannelObserver for InjectedProcessObserver {
    fn channel(&self) -> &'static str {
        CHAN_INJECTED_PROCESS
    }

    fn capture(
        &self,
        ctx: &RunContext,
        op: &Operation,
    ) -> Result<RawChannelEvidence, HarnessError> {
        let Some(id) = &ctx.container_id else {
            return Ok(not_captured(CHAN_INJECTED_PROCESS, &op.id));
        };
        let Some(inspect) = docker_inspect(id)? else {
            return Ok(not_captured(CHAN_INJECTED_PROCESS, &op.id));
        };
        let config = &inspect["Config"];
        let env = env_array_to_object(config.get("Env").unwrap_or(&Value::Null));
        // PATH is captured as the raw string; the normalizer segments it.
        let path = env.get("PATH").cloned().unwrap_or(Value::Null);
        Ok(RawChannelEvidence {
            channel: CHAN_INJECTED_PROCESS.to_string(),
            operation: op.id.clone(),
            present: true,
            value: json!({
                "env": env,
                "user": config.get("User").cloned().unwrap_or(Value::Null),
                "cwd": config.get("WorkingDir").cloned().unwrap_or(Value::Null),
                "path": path,
                "tty": config.get("Tty").cloned().unwrap_or(Value::Null),
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
            subcommand: "exec".to_string(),
            ..Operation::default()
        };
        assert!(!InjectedProcessObserver.capture(&ctx, &op).unwrap().present);
    }
}
