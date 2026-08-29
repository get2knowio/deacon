//! Wait for the container runtime's own `start` event.
//!
//! **Why this exists.** The reference CLI does not touch a container it has just started
//! until the runtime has told it the container started. It opens a
//! `docker`/`podman events --event start` subscription *before* the start, matches the
//! event against the container's labels, and blocks on it — `startEventSeen` in
//! `src/spec-node/utils.ts:172`, awaited on both paths that bring a devcontainer up:
//! `src/spec-node/dockerCompose.ts:359` (before `compose up`, resolved before it will even
//! look up a container id) and `src/spec-node/singleContainer.ts:421` (before `docker
//! run`), both at the pinned oracle tag `v0.87.0`.
//!
//! deacon had no equivalent. Its compose path retried `compose ps` until an id merely
//! *appeared*, and `start_container` returned on `docker start`'s exit code. Both are
//! weaker signals than the event: a runtime can publish a container as `running`, with a
//! live pid, before it has finished the work the `start` event announces — which is what
//! [`ensure_container_running`](crate::docker) checks and therefore cannot rule out.
//!
//! **Why an event subscription rather than a poll.** [#688] established that polling with
//! the *same guarantee* is an acceptable substitute for a subscription, and made that trade
//! for the destroy path. It does not transfer here: the only thing a state poll can ask is
//! whether the container reports `running`, and reporting `running` early is precisely the
//! condition this gate exists to outlast. There is no second observable to poll — the event
//! is the signal, so the subscription is the mechanism.
//!
//! **Where this deliberately differs from the reference**, each bounded and each documented
//! at the point it bites:
//!
//! 1. **It is bounded** by [`START_EVENT_TIMEOUT`], and a timeout warns and proceeds rather
//!    than failing `up`. The reference is unbounded: its `started` promise is settled only
//!    by the event or by the start command dying, so a missed event hangs it forever. A
//!    devcontainer that came up correctly must not fail to come up because an events stream
//!    was unavailable, and this gate is strictly additive — degrading to what deacon did
//!    before it existed is the honest floor. The degradation is logged at WARN, never
//!    swallowed.
//! 2. **The subscription asks for a second of history** (`--since`), because spawning it
//!    is not the same as its being attached. See the comment at the flag.
//! 3. **Podman's attributes are read where podman puts them.** The reference reads
//!    `info.Actor.Attributes` and, when absent, falls back to inspecting the container by
//!    id. Measured here, that fallback fires on every podman event: podman 4.9.3 emits
//!    `{"ID":…,"Status":"start","Attributes":{…}}` with no `Actor` wrapper at all, while
//!    docker 29.7.2 emits `{"Action":"start","Actor":{"ID":…,"Attributes":{…}}}`. Reading
//!    both shapes reaches the same decision the inspect fallback would, without a
//!    round-trip per event against a container that may already be gone.
//!
//! [#688]: https://github.com/get2knowio/deacon/issues/688

use std::process::Stdio;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::oneshot;
use tracing::{debug, instrument, warn};

/// How long [`StartEventWatch::wait`] will wait for the event before giving up.
///
/// The clock starts when `wait` is called — after the start command has returned — so by
/// then the runtime has already emitted the event in every shape this gate is for, and the
/// budget is spent only when something is wrong. It is generous rather than tight because
/// the cost of expiring early (proceeding without the guarantee, silently) is worse than
/// the cost of waiting.
pub const START_EVENT_TIMEOUT: Duration = Duration::from_secs(30);

/// How far back the subscription asks the runtime to replay.
///
/// Guaranteed rather than incidental: see the `--since` comment in
/// [`StartEventWatch::open`] for why the window has to be a whole second wide, and why
/// nothing older than it can satisfy either filter.
pub const LOOKBACK: Duration = Duration::from_secs(1);

/// What the event must say before the caller may touch the container.
///
/// The two constructors are the reference's own two idioms: it filters by container when it
/// has an id (`dockerUtils.ts:188`, the destroy path) and matches labels when it does not
/// (`utils.ts:172`, which is called before the container exists).
#[derive(Debug, Clone, Default)]
pub struct StartEventFilter {
    /// Container id or name. Passed to the runtime as `--filter container=…` AND
    /// re-checked against the event, so a runtime with looser filter semantics than
    /// expected cannot satisfy the gate with somebody else's container.
    container: Option<String>,
    /// Labels every one of which the event's container must carry, with these values.
    labels: Vec<(String, String)>,
}

impl StartEventFilter {
    /// Match the start of one known container.
    pub fn for_container(container_id: impl Into<String>) -> Self {
        Self {
            container: Some(container_id.into()),
            labels: Vec::new(),
        }
    }

    /// Match the start of whichever container carries all of these labels.
    ///
    /// Used where the id cannot be known yet: the compose gate is opened before
    /// `compose up` creates the container, exactly as the reference's is.
    pub fn for_labels(labels: Vec<(String, String)>) -> Self {
        Self {
            container: None,
            labels,
        }
    }
}

/// The outcome of waiting. Every variant is non-fatal by design — see the module doc on
/// why a gate that is strictly additive must not be able to fail an `up` that would
/// otherwise have succeeded.
#[derive(Debug, PartialEq, Eq)]
pub enum StartEventOutcome {
    /// The runtime emitted a matching `start` event. The caller has the guarantee.
    Seen,
    /// No matching event arrived within [`START_EVENT_TIMEOUT`].
    TimedOut,
    /// The events stream could not be opened, or ended before a matching event.
    Unavailable(String),
}

impl StartEventOutcome {
    /// Log the outcome at the level it deserves and return whether the guarantee was had.
    ///
    /// Callers proceed either way; this is what keeps a degradation from being silent.
    pub fn log(&self, what: &str) -> bool {
        match self {
            Self::Seen => {
                debug!("Runtime reported the {} container started", what);
                true
            }
            Self::TimedOut => {
                warn!(
                    "No `start` event for the {} container within {:?}; proceeding without \
                     the runtime's start confirmation. Lifecycle commands may race a \
                     container the runtime has not finished starting.",
                    what, START_EVENT_TIMEOUT
                );
                false
            }
            Self::Unavailable(reason) => {
                warn!(
                    "Could not observe the runtime's `start` event for the {} container \
                     ({}); proceeding without the start confirmation.",
                    what, reason
                );
                false
            }
        }
    }
}

/// A live `events` subscription, opened before the start and awaited after it.
///
/// **The ordering is the whole mechanism.** Open this BEFORE issuing the start, or the
/// event is emitted into a stream nobody is reading and the wait can only expire.
#[derive(Debug)]
pub struct StartEventWatch {
    /// Resolves once a matching event has been read. Dropped by the reader task without
    /// sending when the stream ends first.
    seen: oneshot::Receiver<()>,
    /// Held so it is killed on drop; `kill_on_drop` then ends the reader task's read.
    _child: Child,
    /// Whatever the runtime wrote to stderr, so a stream that dies can say WHY.
    ///
    /// Without this a degradation is announced as "the events stream ended", which names
    /// the symptom and nothing else — and a fail-open gate whose failure explains nothing
    /// is the silent fallback this repository does not allow. A runtime that rejects the
    /// invocation (an unsupported flag, a daemon it cannot reach) says so here.
    stderr: std::sync::Arc<tokio::sync::Mutex<String>>,
    /// Names the container in log messages.
    what: String,
}

impl StartEventWatch {
    /// Subscribe to the runtime's `start` events.
    ///
    /// Never fails the caller: a runtime that cannot serve `events` yields an
    /// [`Err`] the caller turns into a warning, exactly as a stream that dies later does.
    #[instrument(skip(filter))]
    pub async fn open(
        runtime_path: &str,
        is_podman: bool,
        filter: StartEventFilter,
        what: impl Into<String> + std::fmt::Debug,
    ) -> Result<Self> {
        // `--format json` for podman, `{{json .}}` for docker: the reference makes exactly
        // this split (`dockerUtils.ts:218`), citing containers/libpod#5981.
        let format = if is_podman { "json" } else { "{{json .}}" };

        let mut args = vec![
            "events".to_string(),
            "--format".to_string(),
            format.to_string(),
            "--filter".to_string(),
            "event=start".to_string(),
        ];

        // `--since` closes the attach race, and this is NOT a refinement for its own sake
        // — it was measured. Spawning the subscription returns as soon as the process
        // exists, not as soon as it has connected to the daemon and begun streaming, so a
        // container started inside that window emits its event into a stream nobody is
        // reading yet and the wait can only expire. Observed once here, as a 30-second
        // stall on a busy daemon, in the very test written to prove the gate blocks.
        //
        // Measured on both runtimes in this dev container (docker 29.7.2, podman 4.9.3):
        // `events --since <unix seconds>` REPLAYS matching events from that point and then
        // continues streaming live ones. Truncating to whole seconds gives up to a second
        // of lookback, which is the point; nothing older can match either filter, because
        // the id filter names a container that did not exist a second ago and the label
        // filter is only reached for a project that was not running a second ago.
        //
        // The reference carries the same race unguarded (`utils.ts:173`). This is one of
        // the two places this gate is deliberately not a transcription; see the module doc.
        if let Ok(since) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            args.push("--since".to_string());
            // One whole second BACK, not the current second. Truncating to `as_secs()`
            // alone gives a lookback of anywhere between zero and one second depending on
            // where in the second the call lands, which is a race with a small probability
            // rather than no race. Subtracting one makes the window at least a full second
            // in every case, and `LOOKBACK` is what the test for this asserts against.
            args.push(
                since
                    .as_secs()
                    .saturating_sub(LOOKBACK.as_secs())
                    .to_string(),
            );
        }

        if let Some(container) = filter.container.as_deref() {
            args.push("--filter".to_string());
            args.push(format!("container={}", container));
        }

        debug!(
            "Opening start-event subscription: {} {:?}",
            runtime_path, args
        );

        let mut child = Command::new(runtime_path)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("events subscription produced no stdout"))?;

        let stderr = std::sync::Arc::new(tokio::sync::Mutex::new(String::new()));
        if let Some(pipe) = child.stderr.take() {
            let sink = std::sync::Arc::clone(&stderr);
            tokio::spawn(async move {
                let mut lines = BufReader::new(pipe).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let mut sink = sink.lock().await;
                    // Bounded: this exists to name a cause in one warning, not to mirror
                    // a stream that may run for the length of a compose build.
                    if sink.len() < 1024 {
                        sink.push_str(line.trim());
                        sink.push('\n');
                    }
                }
            });
        }

        let (tx, seen) = oneshot::channel();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            // Read continuously from the moment we subscribe rather than only once the
            // caller waits. The reference attaches its handler the same way, and a reader
            // that starts late can stall the runtime's own writes once the pipe fills.
            while let Ok(Some(line)) = lines.next_line().await {
                if event_matches(&line, &filter) {
                    let _ = tx.send(());
                    return;
                }
            }
            // Dropping `tx` here is the signal that the stream ended without a match.
        });

        Ok(Self {
            seen,
            _child: child,
            stderr,
            what: what.into(),
        })
    }

    /// [`open`](Self::open) with a failure to subscribe turned into a warning.
    ///
    /// This is what callers want: a gate that cannot even be opened must not fail an `up`
    /// that would otherwise succeed. `None` means the same thing to a caller as an expired
    /// wait — proceed, having said so — so the two are handled identically at both sites.
    pub async fn open_or_warn(
        runtime_path: &str,
        is_podman: bool,
        filter: StartEventFilter,
        what: impl Into<String> + std::fmt::Debug + Clone,
    ) -> Option<Self> {
        match Self::open(runtime_path, is_podman, filter, what.clone()).await {
            Ok(watch) => Some(watch),
            Err(e) => {
                warn!(
                    "Could not subscribe to the runtime's `start` events for the {:?} \
                     container ({}); proceeding without the start confirmation.",
                    what, e
                );
                None
            }
        }
    }

    /// Block until the runtime confirms the start, or [`START_EVENT_TIMEOUT`] expires.
    ///
    /// Call this AFTER the start command has returned.
    pub async fn wait(self) -> StartEventOutcome {
        self.wait_with_timeout(START_EVENT_TIMEOUT).await
    }

    /// [`wait`](Self::wait) with an explicit budget.
    ///
    /// Exists so a test can assert the NEGATIVE — that a start this gate is not waiting
    /// for leaves it waiting — in seconds rather than in [`START_EVENT_TIMEOUT`]. A gate
    /// whose only tested outcome is the one that fires is a gate nothing distinguishes
    /// from a no-op.
    pub async fn wait_with_timeout(self, budget: Duration) -> StartEventOutcome {
        // Destructured rather than field-accessed so `_child` stays bound for the whole
        // wait: it is what keeps the subscription alive, and `kill_on_drop` ends it here.
        let Self {
            seen,
            _child,
            stderr,
            what: _,
        } = self;

        match tokio::time::timeout(budget, seen).await {
            Ok(Ok(())) => StartEventOutcome::Seen,
            Ok(Err(_)) => {
                let reason = stderr.lock().await.trim().to_string();
                StartEventOutcome::Unavailable(if reason.is_empty() {
                    "the events stream ended".to_string()
                } else {
                    format!("the events stream ended: {reason}")
                })
            }
            Err(_) => StartEventOutcome::TimedOut,
        }
    }

    /// Wait, log the outcome, and report whether the guarantee was had.
    pub async fn wait_and_log(self) -> bool {
        let what = self.what.clone();
        self.wait().await.log(&what)
    }
}

/// Does this event line announce the start of a container the filter names?
///
/// Pure, so the decision this gate turns on is testable without a daemon — and it is where
/// every runtime spelling difference is absorbed. Invalid JSON is ignored rather than
/// reported, as the reference does: an events stream may carry lines this build has never
/// seen, and none of them are the one being waited for.
pub fn event_matches(line: &str, filter: &StartEventFilter) -> bool {
    let line = line.trim();
    if line.is_empty() {
        return false;
    }
    let Ok(event) = serde_json::from_str::<Value>(line) else {
        return false;
    };

    if event_action(&event) != Some("start") {
        return false;
    }

    if let Some(expected) = filter.container.as_deref() {
        match event_container_id(&event) {
            // Ids come in full and short form depending on who printed them, so agreement
            // on the shorter of the two is agreement.
            Some(actual) => {
                if !(actual.starts_with(expected) || expected.starts_with(actual)) {
                    return false;
                }
            }
            None => return false,
        }
    }

    if filter.labels.is_empty() {
        return true;
    }
    let Some(attributes) = event_attributes(&event) else {
        return false;
    };
    filter
        .labels
        .iter()
        .all(|(name, value)| attributes.get(name).and_then(Value::as_str) == Some(value.as_str()))
}

/// `status` is docker's historical spelling, `Status` podman's, and `Action` what docker
/// emits from v29.0.0 onward now that `status` is deprecated. The reference reads all three
/// (`utils.ts:195`) and so does this.
fn event_action(event: &Value) -> Option<&str> {
    ["status", "Status", "Action"]
        .iter()
        .find_map(|key| event.get(key).and_then(Value::as_str))
}

/// docker nests the id under `Actor`; podman puts it at the top level as `ID`.
fn event_container_id(event: &Value) -> Option<&str> {
    event
        .get("Actor")
        .and_then(|actor| {
            actor
                .get("ID")
                .or_else(|| actor.get("Id"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            ["id", "ID"]
                .iter()
                .find_map(|key| event.get(key).and_then(Value::as_str))
        })
}

/// The container's labels as the event carries them — `Actor.Attributes` on docker, a
/// top-level `Attributes` on podman. See the module doc for why reading both replaces the
/// reference's inspect fallback rather than merely skipping it.
fn event_attributes(event: &Value) -> Option<&serde_json::Map<String, Value>> {
    event
        .get("Actor")
        .and_then(|actor| actor.get("Attributes"))
        .or_else(|| event.get("Attributes"))
        .and_then(Value::as_object)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured verbatim from `docker events --format '{{json .}}' --filter event=start`
    /// against docker 29.7.2 in this dev container. Do not hand-edit an expectation here:
    /// regenerating means re-measuring against a real runtime.
    const DOCKER_START: &str = r#"{"Type":"container","Action":"start","Actor":{"ID":"5e98887f8ad4292afd2fa876b410ba62a187a8f5371f1e211c19a87cf1580e7b","Attributes":{"com.docker.compose.project":"deacon-probe","devcontainer.local_folder":"/tmp/probe","image":"alpine:3.18","name":"condescending_rubin"}},"scope":"local","time":1788021191,"timeNano":1788021191583407241}"#;

    /// Captured verbatim from `podman events --format json --filter event=start` against
    /// podman 4.9.3 in this dev container. Note the absence of any `Actor`: this shape is
    /// why the reference's `info.Actor?.Attributes` read falls through to an inspect on
    /// every podman event.
    const PODMAN_START: &str = r#"{"ID":"1ab54423b35289d1826e74abfc400a14babc38a97dedddafa7dcccce9e608e3e","Image":"docker.io/library/alpine:3.18","Name":"gallant_buck","Status":"start","Time":"2026-08-29T16:33:29.869831505Z","Type":"container","Attributes":{"com.docker.compose.project":"deacon-probe","devcontainer.local_folder":"/tmp/probe"}}"#;

    fn labels(pairs: &[(&str, &str)]) -> StartEventFilter {
        StartEventFilter::for_labels(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }

    #[test]
    fn docker_start_event_matches_its_labels() {
        assert!(event_matches(
            DOCKER_START,
            &labels(&[("com.docker.compose.project", "deacon-probe")])
        ));
    }

    #[test]
    fn podman_start_event_matches_its_labels_without_an_actor_wrapper() {
        assert!(event_matches(
            PODMAN_START,
            &labels(&[("com.docker.compose.project", "deacon-probe")])
        ));
    }

    /// Every expected label must be present, not merely one of them — the compose gate
    /// matches on project AND service, and a project-only match is somebody else's service.
    #[test]
    fn one_matching_label_is_not_enough() {
        for line in [DOCKER_START, PODMAN_START] {
            assert!(!event_matches(
                line,
                &labels(&[
                    ("com.docker.compose.project", "deacon-probe"),
                    ("com.docker.compose.service", "app"),
                ])
            ));
        }
    }

    #[test]
    fn a_different_label_value_does_not_match() {
        for line in [DOCKER_START, PODMAN_START] {
            assert!(!event_matches(
                line,
                &labels(&[("com.docker.compose.project", "someone-elses-project")])
            ));
        }
    }

    #[test]
    fn matches_by_container_id_in_both_shapes() {
        assert!(event_matches(
            DOCKER_START,
            &StartEventFilter::for_container(
                "5e98887f8ad4292afd2fa876b410ba62a187a8f5371f1e211c19a87cf1580e7b"
            )
        ));
        assert!(event_matches(
            PODMAN_START,
            &StartEventFilter::for_container(
                "1ab54423b35289d1826e74abfc400a14babc38a97dedddafa7dcccce9e608e3e"
            )
        ));
    }

    /// `docker ps` and `compose ps` print short ids while the events stream prints full
    /// ones; agreement on the shorter is agreement.
    #[test]
    fn a_short_id_matches_the_full_one() {
        assert!(event_matches(
            DOCKER_START,
            &StartEventFilter::for_container("5e98887f8ad4")
        ));
    }

    #[test]
    fn another_containers_start_does_not_match() {
        assert!(!event_matches(
            DOCKER_START,
            &StartEventFilter::for_container("0123456789ab")
        ));
    }

    /// The gate is for `start` and nothing else. `--filter event=start` already asks the
    /// runtime for that, but the check is deacon's too: a filter that silently stopped
    /// working would otherwise let `die` satisfy a wait for `start`.
    #[test]
    fn a_non_start_event_never_matches() {
        let die = DOCKER_START.replace(r#""Action":"start""#, r#""Action":"die""#);
        assert!(!event_matches(&die, &StartEventFilter::default()));

        let podman_die = PODMAN_START.replace(r#""Status":"start""#, r#""Status":"die""#);
        assert!(!event_matches(&podman_die, &StartEventFilter::default()));
    }

    /// docker before v29 spells it `status`, which the reference still reads.
    #[test]
    fn the_legacy_lowercase_status_spelling_is_read() {
        let legacy =
            r#"{"status":"start","id":"abc123","Actor":{"ID":"abc123","Attributes":{"k":"v"}}}"#;
        assert!(event_matches(legacy, &labels(&[("k", "v")])));
    }

    /// An events stream carries lines this build has never seen. None of them is the one
    /// being waited for, and none of them may crash the reader task.
    #[test]
    fn unparseable_and_empty_lines_are_ignored() {
        for line in ["", "   ", "not json at all", "{", "[]", "null"] {
            assert!(!event_matches(line, &StartEventFilter::default()));
        }
    }

    /// A runtime binary that does not exist is a failure to open, not a wait that hangs.
    #[tokio::test]
    async fn opening_against_a_missing_binary_fails_rather_than_waiting() {
        let opened = StartEventWatch::open(
            "/nonexistent/deacon-no-such-runtime",
            false,
            StartEventFilter::default(),
            "test",
        )
        .await;
        assert!(opened.is_err());
    }

    /// **A degradation must name its cause.** `/bin/sh` spawns fine and then rejects
    /// `events` as a script it cannot open, which is the shape of every real failure here:
    /// the process starts, the stream ends immediately, and the only account of why is on
    /// stderr. Drop the stderr capture and this reports a bare "the events stream ended".
    ///
    /// `#[cfg(unix)]` because `/bin/sh` is the VEHICLE, not the subject: the capture itself
    /// is platform-agnostic and this gate costs no coverage of deacon's own behavior. There
    /// is no Windows binary that spawns and then fails on these argv in the same shape.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_stream_that_dies_reports_what_the_runtime_said() {
        let watch = StartEventWatch::open("/bin/sh", false, StartEventFilter::default(), "test")
            .await
            .expect("/bin/sh spawns");

        match watch.wait_with_timeout(Duration::from_secs(10)).await {
            StartEventOutcome::Unavailable(reason) => {
                assert!(
                    reason.len() > "the events stream ended".len(),
                    "the reason carried no cause: {reason}"
                );
                assert!(
                    reason.contains("events"),
                    "the cause does not name what failed: {reason}"
                );
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    /// A start event whose container carries no labels cannot satisfy a label filter —
    /// the absence of attributes is a non-match, never a free pass.
    #[test]
    fn missing_attributes_never_satisfy_a_label_filter() {
        let bare = r#"{"Action":"start","Actor":{"ID":"abc123"}}"#;
        assert!(!event_matches(bare, &labels(&[("k", "v")])));
        // …while the same event satisfies an id filter, which asks nothing of labels.
        assert!(event_matches(
            bare,
            &StartEventFilter::for_container("abc123")
        ));
    }
}
