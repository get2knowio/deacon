//! Per-channel observers for the declarative conformance runner (research D7, contract
//! observer-channel.md, 022-conformance-runner).
//!
//! One module per observable channel, each implementing the small [`ChannelObserver`]
//! contract. The runner invokes ONLY the observers a case's `expected`/`fsAllowlist`
//! declares, so a pure `read-configuration` CLI case pays nothing for Docker
//! inspection (research D7). Filesystem capture is allowlist-scoped, never a full-tree
//! diff (clarify Q1).
//!
//! MODULE STATUS: the contract ([`ChannelObserver`] + [`RunContext`]) is defined here,
//! and the CLI-process observer (`cli_process`, covering exit-code / stdout / stderr /
//! structured-output) is implemented for User Story 1. The remaining observers land per
//! user story: `filesystem` in US3 (T044), and `image` / `container_graph` /
//! `injected_process` / `temporal` in US5 (T053–T056) — those submodules are still
//! scaffolding.

use std::collections::HashMap;
use std::path::PathBuf;

use deacon_conformance::model::{
    CHAN_EXIT_CODE, CHAN_FILE_CONTENT, CHAN_FILESYSTEM, CHAN_STDERR, CHAN_STDOUT,
    CHAN_STRUCTURED_OUTPUT, FailurePhase, Operation,
};

use crate::HarnessError;
use crate::evidence::RawChannelEvidence;

pub mod cli_process;
pub mod container_graph;
pub mod filesystem;
pub mod image;
pub mod injected_process;
pub mod temporal;

/// The process-level result of running one operation's CLI invocation, captured once by
/// the runner and read by the CLI-process observer(s) for that operation.
///
/// Holding the outcome on the [`RunContext`] (rather than having each observer re-run
/// the command) means the operation runs exactly once and every declared channel
/// observes the SAME invocation — the model the Docker channels also follow (they
/// inspect state left behind by the single run).
#[derive(Debug, Clone)]
pub struct ProcessOutcome {
    /// Exit code, or `None` when terminated by signal.
    pub exit_code: Option<i32>,
    /// Whether the process exited successfully (status 0).
    pub success: bool,
    /// Verbatim stdout bytes.
    pub stdout: Vec<u8>,
    /// Verbatim stderr bytes.
    pub stderr: Vec<u8>,
    /// The inferred failure phase, present ONLY when the operation failed (FR-009,
    /// closed set §8). `None` on success.
    pub failure_phase: Option<FailurePhase>,
}

/// What the runner hands each observer for a capture: the workspace the case ran in,
/// (once a container exists) its id for the Docker-backed channels, and the per-op
/// process outcomes captured by the runner.
///
/// This grows as the Docker observers land (a container runtime handle, probed env) —
/// always via additive fields so existing observers keep compiling.
#[derive(Debug, Clone)]
pub struct RunContext {
    /// The workspace the case's operations ran against (US5 makes this an isolated
    /// external temp dir; US1 config-only cases run against the committed fixture dir).
    pub workspace: PathBuf,
    /// The container id, once the case has brought one up. `None` for pure CLI-process
    /// cases that never create a container.
    pub container_id: Option<String>,
    /// The full `docker inspect` object for [`RunContext::container_id`], captured ONCE by
    /// the runner (off the async executor via `spawn_blocking`) after the container exists.
    /// The Docker channel observers read THIS instead of each spawning their own
    /// `docker inspect` — so a case pays a single inspect, and no observer blocks the async
    /// executor (finding #4). `None` when no container exists / it was removed.
    pub container_inspect: Option<serde_json::Value>,
    /// The case's `fsAllowlist` — the path/glob allowlist the filesystem observer is
    /// scoped to (clarify Q1: allowlist-scoped, never a full-tree diff). Empty for cases
    /// with no filesystem expectation.
    pub fs_allowlist: Vec<String>,
    /// Per-operation process outcomes, keyed by `Operation::id`, populated by the runner
    /// before observers run.
    outcomes: HashMap<String, ProcessOutcome>,
    /// Per-operation container snapshots (the container id + temporal state AT that op's
    /// boundary), keyed by `Operation::id`. Populated by the runner after each Docker op
    /// so the invariant/metamorphic oracle can compare state ACROSS operations (US6).
    op_snapshots: HashMap<String, OpSnapshot>,
}

/// The container state captured at one operation's boundary (US6 metamorphic evaluation).
#[derive(Debug, Clone, PartialEq)]
pub struct OpSnapshot {
    /// The primary container id observed after this op (`None` = no container / removed).
    /// This is the first of [`OpSnapshot::container_ids`] — kept for callers that need a
    /// single id.
    pub container_id: Option<String>,
    /// EVERY container id matching this op's workspace label at its boundary, sorted. The
    /// metamorphic oracle needs the full SET, not just one: a non-idempotent op that left a
    /// SECOND container behind has `len > 1`, and observing a single id would mask exactly
    /// the failure the idempotence relationship exists to catch (finding #3).
    pub container_ids: Vec<String>,
    /// The `chan-temporal` evidence value at this op's boundary (status/running/restart).
    pub temporal: serde_json::Value,
}

impl RunContext {
    /// A context for a container-less run rooted at `workspace`.
    pub fn new(workspace: PathBuf) -> RunContext {
        RunContext {
            workspace,
            container_id: None,
            container_inspect: None,
            fs_allowlist: Vec::new(),
            outcomes: HashMap::new(),
            op_snapshots: HashMap::new(),
        }
    }

    /// Record an operation's process outcome (runner-side, before observers run).
    pub fn record_outcome(&mut self, op_id: impl Into<String>, outcome: ProcessOutcome) {
        self.outcomes.insert(op_id.into(), outcome);
    }

    /// The process outcome captured for `op_id`, if the operation ran.
    pub fn outcome(&self, op_id: &str) -> Option<&ProcessOutcome> {
        self.outcomes.get(op_id)
    }

    /// Record the container snapshot at an operation's boundary (runner-side, US6).
    pub fn record_op_snapshot(&mut self, op_id: impl Into<String>, snapshot: OpSnapshot) {
        self.op_snapshots.insert(op_id.into(), snapshot);
    }

    /// The container snapshot captured at `op_id`'s boundary, if any (US6).
    pub fn op_snapshot(&self, op_id: &str) -> Option<&OpSnapshot> {
        self.op_snapshots.get(op_id)
    }
}

/// The contract every channel observer implements (contract observer-channel.md).
///
/// An observer captures its one channel's evidence for a single operation. It returns
/// [`RawChannelEvidence`] with `present:false` when the channel could not be observed
/// for this op (distinct from a captured-but-empty value, FR-018), or a cause-specific
/// [`HarnessError`] when capture itself faults (never a silent skip, constitution IV).
pub trait ChannelObserver {
    /// The channel id this observer captures (`chan-…`), matching `channels.json`.
    fn channel(&self) -> &'static str;

    /// Capture this channel's evidence for `op` under `ctx`.
    fn capture(&self, ctx: &RunContext, op: &Operation)
    -> Result<RawChannelEvidence, HarnessError>;
}

/// Resolve the observer for a declared channel, or `None` when no observer for that
/// channel exists yet (the runner turns `None` into a fail-loud error — a case must
/// never declare a channel the harness cannot observe, constitution IV).
///
/// US1 wires the four CLI-process channels; US3 the filesystem channels; US5 the four
/// Docker channels.
pub fn observer_for(channel: &str) -> Option<Box<dyn ChannelObserver>> {
    use deacon_conformance::model::{
        CHAN_IMAGE, CHAN_INJECTED_PROCESS, CHAN_PROCESS_GRAPH, CHAN_TEMPORAL,
    };
    match channel {
        CHAN_EXIT_CODE | CHAN_STDOUT | CHAN_STDERR | CHAN_STRUCTURED_OUTPUT => {
            cli_process::CliProcessObserver::for_channel(channel)
                .map(|o| Box::new(o) as Box<dyn ChannelObserver>)
        }
        CHAN_FILESYSTEM | CHAN_FILE_CONTENT => filesystem::FilesystemObserver::for_channel(channel)
            .map(|o| Box::new(o) as Box<dyn ChannelObserver>),
        CHAN_IMAGE => Some(Box::new(image::ImageObserver)),
        CHAN_PROCESS_GRAPH => Some(Box::new(container_graph::ContainerGraphObserver)),
        CHAN_INJECTED_PROCESS => Some(Box::new(injected_process::InjectedProcessObserver)),
        CHAN_TEMPORAL => Some(Box::new(temporal::TemporalObserver)),
        _ => None,
    }
}

/// Evidence for a channel that could NOT be observed for this op (no container, or the
/// container is gone) — `present:false`, distinct from a captured-empty value (FR-018).
pub(crate) fn not_captured(channel: &'static str, op_id: &str) -> RawChannelEvidence {
    RawChannelEvidence {
        channel: channel.to_string(),
        operation: op_id.to_string(),
        present: false,
        value: serde_json::Value::Null,
    }
}

/// Run `docker inspect <id>` and return the first result object, `None` when the object
/// does not exist (container/image removed — a legitimate not-captured state), or a
/// fail-loud [`HarnessError::DockerUnavailable`] when `docker` itself cannot run.
///
/// This is a BLOCKING call. Async callers (the runner) must invoke it via
/// `tokio::task::spawn_blocking` so it never blocks the executor (finding #4); the
/// one-shot refresh / `snapshot check` CLIs and the `Drop` cleanup guard call it directly
/// (non-concurrent, like `Drop`). Observers never call it — they read the pre-fetched
/// [`RunContext::container_inspect`].
pub fn docker_inspect(id: &str) -> Result<Option<serde_json::Value>, HarnessError> {
    let output = std::process::Command::new("docker")
        .args(["inspect", id])
        .output()
        .map_err(|e| HarnessError::DockerUnavailable {
            cause: format!("could not run `docker inspect {id}`: {e}"),
        })?;
    if !output.status.success() {
        // Non-zero for "No such object" — treat as not-present, not a harness fault.
        return Ok(None);
    }
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| HarnessError::DockerUnavailable {
            cause: format!("`docker inspect {id}` returned non-JSON: {e}"),
        })?;
    Ok(parsed.as_array().and_then(|a| a.first()).cloned())
}

/// Parse a Docker `Env` array of `"KEY=VALUE"` strings into a `{ key: value }` object
/// (shared by the image + injected-process observers).
pub(crate) fn env_array_to_object(env: &serde_json::Value) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    if let Some(items) = env.as_array() {
        for item in items {
            if let Some(s) = item.as_str() {
                let (k, v) = s.split_once('=').unwrap_or((s, ""));
                map.insert(k.to_string(), serde_json::Value::String(v.to_string()));
            }
        }
    }
    serde_json::Value::Object(map)
}
