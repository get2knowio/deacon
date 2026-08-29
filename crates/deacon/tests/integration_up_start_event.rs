//! The `start`-event gate, against a real container runtime.
//!
//! `up` must not touch a container it has just started until the runtime has said the
//! container started — the reference CLI's `startEventSeen`
//! (`src/spec-node/utils.ts:172` at the pinned oracle `v0.87.0`), awaited on both the
//! compose path and the single-container path. The unit tests in
//! `deacon_core::start_event` cover the decision (which event, whose container); these
//! cover the mechanism end-to-end on a live daemon.
//!
//! **The load-bearing assertion is the negative one.** A gate that always returns
//! immediately passes every "did it see the start?" test there is. So one test starts a
//! container this gate is NOT waiting for and asserts the gate is still waiting, and one
//! delays the start and asserts the gate did not return before it.
//!
//! Named `integration_up_*` deliberately: every `.config/nextest.toml` profile already
//! carries a `binary(#integration_up_*)` rule, so this binary is grouped and filtered
//! correctly in all of them without touching ten filter lines.

mod support;

use std::process::Command;
use std::time::{Duration, Instant};

use deacon_core::start_event::{StartEventFilter, StartEventOutcome, StartEventWatch};
use support::{is_runtime_available, runtime_bin};

/// Pinned, and small enough that a pull is not the thing under test.
const IMAGE: &str = "alpine:3.18";

/// Short enough that the negative tests cost seconds, long enough that a slow daemon
/// under CI load does not report a start that happened as one that did not.
const NEGATIVE_BUDGET: Duration = Duration::from_secs(5);

fn is_podman() -> bool {
    runtime_bin().contains("podman")
}

/// Podman resolves short names through its own registry search, so the ref must be
/// qualified for it exactly as deacon's own runtime layer qualifies one.
fn image() -> String {
    if is_podman() {
        format!("docker.io/library/{IMAGE}")
    } else {
        IMAGE.to_string()
    }
}

fn ensure_image() {
    let _ = Command::new(runtime_bin())
        .args(["pull", &image()])
        .output();
}

/// `run -d` a throwaway container carrying `label`, returning its id.
fn run_labeled(label: &str) -> String {
    let out = Command::new(runtime_bin())
        .args(["run", "-d", "--rm", "-l", label, &image(), "sleep", "10"])
        .output()
        .expect("starting the probe container");
    assert!(
        out.status.success(),
        "probe container failed to start: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn remove(container_id: &str) {
    let _ = Command::new(runtime_bin())
        .args(["rm", "-f", container_id])
        .output();
}

/// A unique label value per test, so concurrent cases in the same group cannot satisfy
/// each other's gate.
fn unique(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

#[tokio::test]
async fn the_gate_sees_a_real_container_start() {
    if !is_runtime_available() {
        eprintln!("Skipping: {} is not available", runtime_bin());
        return;
    }
    ensure_image();

    let project = unique("deacon-start-event");
    let watch = StartEventWatch::open(
        &runtime_bin(),
        is_podman(),
        StartEventFilter::for_labels(vec![("deacon.test.project".to_string(), project.clone())]),
        "test",
    )
    .await
    .expect("opening the events subscription");

    let container_id = run_labeled(&format!("deacon.test.project={project}"));
    let outcome = watch.wait_with_timeout(NEGATIVE_BUDGET).await;
    remove(&container_id);

    assert_eq!(
        outcome,
        StartEventOutcome::Seen,
        "the runtime started a container carrying the gate's labels and the gate did not see it"
    );
}

/// **The negative.** Somebody else's container starting is not this container starting,
/// and a gate that cannot tell them apart is not a gate. Without the label check in
/// `event_matches` this returns `Seen`.
#[tokio::test]
async fn another_containers_start_leaves_the_gate_waiting() {
    if !is_runtime_available() {
        eprintln!("Skipping: {} is not available", runtime_bin());
        return;
    }
    ensure_image();

    let waited_for = unique("deacon-start-event-wanted");
    let watch = StartEventWatch::open(
        &runtime_bin(),
        is_podman(),
        StartEventFilter::for_labels(vec![(
            "deacon.test.project".to_string(),
            waited_for.clone(),
        )]),
        "test",
    )
    .await
    .expect("opening the events subscription");

    // A start the gate is not waiting for, from the same daemon, during the wait.
    let other = unique("deacon-start-event-other");
    let container_id = run_labeled(&format!("deacon.test.project={other}"));
    let outcome = watch.wait_with_timeout(NEGATIVE_BUDGET).await;
    remove(&container_id);

    assert_eq!(
        outcome,
        StartEventOutcome::TimedOut,
        "a container with different labels satisfied the gate"
    );
}

/// **The attach race, made deterministic.** Spawning `<runtime> events` returns before the
/// process has connected to the daemon, so a container started inside that window emits its
/// event into a stream nobody is reading yet. That is not hypothetical: it stalled
/// `the_gate_blocks_until_the_start_actually_happens` for the full timeout once, on a busy
/// daemon, while this gate was being built.
///
/// The guard is `--since`, which replays the last [`deacon_core::start_event::LOOKBACK`]
/// before streaming. This test inverts the ordering to assert it: the container starts
/// FIRST, and the subscription opens afterwards. Remove `--since` and this can only expire.
#[tokio::test]
async fn a_start_just_before_the_subscription_is_still_seen() {
    if !is_runtime_available() {
        eprintln!("Skipping: {} is not available", runtime_bin());
        return;
    }
    ensure_image();

    let project = unique("deacon-start-event-replay");
    let container_id = run_labeled(&format!("deacon.test.project={project}"));

    // Opened after the start it is waiting for — the shape the lookback exists to cover.
    let watch = StartEventWatch::open(
        &runtime_bin(),
        is_podman(),
        StartEventFilter::for_labels(vec![("deacon.test.project".to_string(), project.clone())]),
        "test",
    )
    .await
    .expect("opening the events subscription");

    let outcome = watch.wait_with_timeout(Duration::from_secs(10)).await;
    remove(&container_id);

    assert_eq!(
        outcome,
        StartEventOutcome::Seen,
        "the subscription did not replay a start from within its own lookback window"
    );
}

/// **The other negative: the gate actually blocks.** A no-op `wait` returns at once and
/// would satisfy `the_gate_sees_a_real_container_start` too, because that test starts the
/// container before it waits. Here the start is delayed, so returning early is the only
/// way to fail — which is what deacon did before this gate existed.
#[tokio::test]
async fn the_gate_blocks_until_the_start_actually_happens() {
    if !is_runtime_available() {
        eprintln!("Skipping: {} is not available", runtime_bin());
        return;
    }
    ensure_image();

    let project = unique("deacon-start-event-delayed");
    let watch = StartEventWatch::open(
        &runtime_bin(),
        is_podman(),
        StartEventFilter::for_labels(vec![("deacon.test.project".to_string(), project.clone())]),
        "test",
    )
    .await
    .expect("opening the events subscription");

    const DELAY: Duration = Duration::from_secs(2);
    let label = format!("deacon.test.project={project}");
    let starter = tokio::task::spawn_blocking(move || {
        std::thread::sleep(DELAY);
        run_labeled(&label)
    });

    let began = Instant::now();
    let outcome = watch.wait_with_timeout(Duration::from_secs(30)).await;
    let waited = began.elapsed();
    remove(&starter.await.expect("the delayed start"));

    assert_eq!(outcome, StartEventOutcome::Seen);
    assert!(
        waited >= DELAY,
        "the gate returned after {waited:?}, before the container was started at {DELAY:?} — \
         it is not waiting for anything"
    );
}
