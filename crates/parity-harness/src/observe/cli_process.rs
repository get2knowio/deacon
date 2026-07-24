//! CLI-process observer (`chan-exit-code` / `chan-stdout` / `chan-stderr` /
//! `chan-structured-output`): exit code, stdout, stderr, and the parsed structured
//! result document, plus the [`FailurePhase`] on failure (T019/T020,
//! 022-conformance-runner).
//!
//! The runner runs each operation once (via [`crate::exec`]) and records a
//! [`ProcessOutcome`] on the [`RunContext`]; this observer reads that outcome and maps
//! it to per-channel [`RawChannelEvidence`]. It never re-runs the command.

use deacon_conformance::model::{
    CHAN_EXIT_CODE, CHAN_STDERR, CHAN_STDOUT, CHAN_STRUCTURED_OUTPUT, FailurePhase, Operation,
};

use crate::HarnessError;
use crate::evidence::RawChannelEvidence;
use crate::observe::{ChannelObserver, ProcessOutcome, RunContext};

/// An observer for one of the four CLI-process channels. One instance per channel (the
/// channel id it captures is fixed at construction), so the runner can invoke exactly
/// the channels a case declares.
#[derive(Debug, Clone, Copy)]
pub struct CliProcessObserver {
    channel: &'static str,
}

impl CliProcessObserver {
    /// Construct the observer for a CLI-process channel, or `None` when `channel` is not
    /// one of the four this module owns.
    pub fn for_channel(channel: &str) -> Option<CliProcessObserver> {
        let channel = match channel {
            CHAN_EXIT_CODE => CHAN_EXIT_CODE,
            CHAN_STDOUT => CHAN_STDOUT,
            CHAN_STDERR => CHAN_STDERR,
            CHAN_STRUCTURED_OUTPUT => CHAN_STRUCTURED_OUTPUT,
            _ => return None,
        };
        Some(CliProcessObserver { channel })
    }

    /// Build this channel's evidence from an already-captured [`ProcessOutcome`]. Kept
    /// separate from the trait so the runner can also call it directly (and unit tests
    /// can exercise the mapping without a `RunContext`).
    pub fn evidence_from(&self, op_id: &str, outcome: &ProcessOutcome) -> RawChannelEvidence {
        match self.channel {
            CHAN_EXIT_CODE => RawChannelEvidence {
                channel: CHAN_EXIT_CODE.to_string(),
                operation: op_id.to_string(),
                present: true,
                // A signal-terminated process has no exit code (`null`), which stays
                // distinct from a captured `0` (FR-018).
                value: match outcome.exit_code {
                    Some(code) => serde_json::json!(code),
                    None => serde_json::Value::Null,
                },
            },
            CHAN_STDOUT => RawChannelEvidence {
                channel: CHAN_STDOUT.to_string(),
                operation: op_id.to_string(),
                present: true,
                value: serde_json::Value::String(String::from_utf8_lossy(&outcome.stdout).into()),
            },
            CHAN_STDERR => RawChannelEvidence {
                channel: CHAN_STDERR.to_string(),
                operation: op_id.to_string(),
                present: true,
                value: serde_json::Value::String(String::from_utf8_lossy(&outcome.stderr).into()),
            },
            CHAN_STRUCTURED_OUTPUT => structured_output_evidence(op_id, outcome),
            // `for_channel` only ever constructs one of the four above.
            other => RawChannelEvidence {
                channel: other.to_string(),
                operation: op_id.to_string(),
                present: false,
                value: serde_json::Value::Null,
            },
        }
    }
}

impl ChannelObserver for CliProcessObserver {
    fn channel(&self) -> &'static str {
        self.channel
    }

    fn capture(
        &self,
        ctx: &RunContext,
        op: &Operation,
    ) -> Result<RawChannelEvidence, HarnessError> {
        match ctx.outcome(&op.id) {
            Some(outcome) => Ok(self.evidence_from(&op.id, outcome)),
            // The operation did not run (or its outcome was not recorded): the channel
            // was not captured for this op (FR-018), NOT a captured-empty value.
            None => Ok(RawChannelEvidence {
                channel: self.channel.to_string(),
                operation: op.id.clone(),
                present: false,
                value: serde_json::Value::Null,
            }),
        }
    }
}

/// The parsed-JSON structured-output evidence (T020). Reuses the same "is this valid
/// JSON?" decision as [`crate::exec::Invocation::stdout_json`]: parseable stdout →
/// present with the parsed value; non-JSON stdout → `present:false` (the structured
/// channel was not observable for this op), which stays distinct from a captured empty
/// document (FR-018). There is no fallback to comparing raw bytes here — a case that
/// declares `chan-structured-output` against non-JSON stdout verdicts as a divergence
/// in `compare`, not a silent pass.
fn structured_output_evidence(op_id: &str, outcome: &ProcessOutcome) -> RawChannelEvidence {
    let text = String::from_utf8_lossy(&outcome.stdout);
    match serde_json::from_str::<serde_json::Value>(text.trim()) {
        Ok(value) => RawChannelEvidence {
            channel: CHAN_STRUCTURED_OUTPUT.to_string(),
            operation: op_id.to_string(),
            present: true,
            value,
        },
        Err(_) => RawChannelEvidence {
            channel: CHAN_STRUCTURED_OUTPUT.to_string(),
            operation: op_id.to_string(),
            present: false,
            value: serde_json::Value::Null,
        },
    }
}

/// Infer the [`FailurePhase`] (closed set §8) for a FAILED operation from its
/// subcommand (FR-009). This is the coarse, subcommand-anchored inference US1 needs for
/// config-only cases: `read-configuration`/`doctor` can only fail during
/// config-resolution. The finer, lifecycle-aware inference (which lifecycle hook a
/// container op failed in) lands with the temporal observer (US5); the mapping below is
/// the deacon phase each subcommand fails in when it fails as a whole.
pub fn infer_failure_phase(subcommand: &str) -> FailurePhase {
    match subcommand {
        "read-configuration" | "doctor" => FailurePhase::ConfigResolution,
        "build" => FailurePhase::Build,
        "up" => FailurePhase::ContainerCreate,
        "exec" | "run-user-commands" | "templates-apply" => FailurePhase::Exec,
        // Any other (already validated to be in the consumer surface) op that fails
        // before it does anything meaningful is a config-resolution failure.
        _ => FailurePhase::ConfigResolution,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(exit: Option<i32>, stdout: &str, phase: Option<FailurePhase>) -> ProcessOutcome {
        ProcessOutcome {
            exit_code: exit,
            success: exit == Some(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
            failure_phase: phase,
        }
    }

    #[test]
    fn exit_code_evidence_preserves_null_for_signal() {
        let obs = CliProcessObserver::for_channel(CHAN_EXIT_CODE).unwrap();
        assert_eq!(
            obs.evidence_from("op", &outcome(Some(0), "", None)).value,
            serde_json::json!(0)
        );
        assert_eq!(
            obs.evidence_from("op", &outcome(None, "", None)).value,
            serde_json::Value::Null,
            "signal termination → null exit code, distinct from 0"
        );
    }

    #[test]
    fn structured_output_present_only_when_json() {
        let obs = CliProcessObserver::for_channel(CHAN_STRUCTURED_OUTPUT).unwrap();
        let json = obs.evidence_from("op", &outcome(Some(0), r#"{"a":1}"#, None));
        assert!(json.present);
        assert_eq!(json.value, serde_json::json!({"a":1}));

        let not_json = obs.evidence_from("op", &outcome(Some(0), "not json", None));
        assert!(
            !not_json.present,
            "non-JSON stdout → structured channel not captured (present:false)"
        );
    }

    #[test]
    fn stdout_is_a_string() {
        let obs = CliProcessObserver::for_channel(CHAN_STDOUT).unwrap();
        assert_eq!(
            obs.evidence_from("op", &outcome(Some(0), "hello", None))
                .value,
            serde_json::json!("hello")
        );
    }

    #[test]
    fn failure_phase_inference_matches_subcommand() {
        assert_eq!(
            infer_failure_phase("read-configuration"),
            FailurePhase::ConfigResolution
        );
        assert_eq!(infer_failure_phase("build"), FailurePhase::Build);
        assert_eq!(infer_failure_phase("up"), FailurePhase::ContainerCreate);
        assert_eq!(infer_failure_phase("exec"), FailurePhase::Exec);
    }

    #[test]
    fn for_channel_rejects_non_cli_channels() {
        assert!(CliProcessObserver::for_channel("chan-image").is_none());
        assert!(CliProcessObserver::for_channel(CHAN_STDERR).is_some());
    }
}
