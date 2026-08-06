//! Integration tests for lifecycle command format support
//!
//! Tests that all three formats (string, array, object) work correctly
//! for lifecycle commands in both container and host execution contexts.

use deacon_core::container_lifecycle::{
    AggregatedLifecycleCommand, LifecycleCommandList, LifecycleCommandSource, LifecycleCommandValue,
};
use indexmap::IndexMap;

// ================================================================
// Array (Exec-Style) Format Tests
// ================================================================

#[test]
fn test_exec_format_parsing() {
    let json = serde_json::json!(["npm", "install", "--save-dev"]);
    let result = LifecycleCommandValue::from_json_value(&json)
        .unwrap()
        .unwrap();
    assert_eq!(
        result,
        LifecycleCommandValue::Exec(vec![
            "npm".to_string(),
            "install".to_string(),
            "--save-dev".to_string(),
        ])
    );
}

#[test]
fn test_exec_format_preserves_spaces_in_args() {
    let json = serde_json::json!(["echo", "hello world", "foo bar"]);
    let result = LifecycleCommandValue::from_json_value(&json)
        .unwrap()
        .unwrap();
    match result {
        LifecycleCommandValue::Exec(args) => {
            assert_eq!(args[0], "echo");
            assert_eq!(args[1], "hello world"); // Preserved as single arg, no splitting
            assert_eq!(args[2], "foo bar");
        }
        _ => panic!("Expected Exec variant"),
    }
}

#[test]
fn test_exec_format_preserves_shell_metacharacters() {
    // Shell metacharacters should NOT be interpreted
    let json = serde_json::json!(["echo", "$HOME", "&&", "ls"]);
    let result = LifecycleCommandValue::from_json_value(&json)
        .unwrap()
        .unwrap();
    match result {
        LifecycleCommandValue::Exec(args) => {
            assert_eq!(args, vec!["echo", "$HOME", "&&", "ls"]);
        }
        _ => panic!("Expected Exec variant"),
    }
}

#[test]
fn test_exec_format_single_element() {
    let json = serde_json::json!(["ls"]);
    let result = LifecycleCommandValue::from_json_value(&json)
        .unwrap()
        .unwrap();
    assert_eq!(result, LifecycleCommandValue::Exec(vec!["ls".to_string()]));
}

#[test]
fn test_exec_format_empty_array_is_noop() {
    let json = serde_json::json!([]);
    let result = LifecycleCommandValue::from_json_value(&json)
        .unwrap()
        .unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_exec_format_rejects_non_string_elements() {
    let json = serde_json::json!(["echo", 42]);
    let result = LifecycleCommandValue::from_json_value(&json);
    assert!(result.is_err());
}

#[test]
fn test_exec_format_variable_substitution_element_wise() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let cmd = LifecycleCommandValue::Exec(vec![
        "echo".to_string(),
        "no-vars-here".to_string(),
        "also-plain".to_string(),
    ]);
    let context = deacon_core::variable::SubstitutionContext::new(temp_dir.path()).unwrap();
    let substituted = cmd.substitute_variables(&context);
    // Should still be Exec with same number of args
    match substituted {
        LifecycleCommandValue::Exec(args) => {
            assert_eq!(args.len(), 3);
        }
        _ => panic!("Expected Exec variant after substitution"),
    }
}

#[test]
fn test_exec_format_in_command_list() {
    let cmd_list = LifecycleCommandList {
        commands: vec![AggregatedLifecycleCommand {
            command: LifecycleCommandValue::Exec(vec!["npm".to_string(), "install".to_string()]),
            source: LifecycleCommandSource::Config,
        }],
    };
    assert_eq!(cmd_list.len(), 1);
    assert!(!cmd_list.is_empty());
}

// ================================================================
// Object (Parallel) Format Tests
// ================================================================

#[test]
fn test_parallel_format_parsing() {
    let json = serde_json::json!({
        "install": "npm install",
        "build": ["npm", "run", "build"]
    });
    let result = LifecycleCommandValue::from_json_value(&json)
        .unwrap()
        .unwrap();
    match result {
        LifecycleCommandValue::Parallel(map) => {
            assert_eq!(map.len(), 2);
            assert_eq!(
                map.get("install"),
                Some(&LifecycleCommandValue::Shell("npm install".to_string()))
            );
            assert_eq!(
                map.get("build"),
                Some(&LifecycleCommandValue::Exec(vec![
                    "npm".to_string(),
                    "run".to_string(),
                    "build".to_string(),
                ]))
            );
        }
        _ => panic!("Expected Parallel variant"),
    }
}

#[test]
fn test_parallel_format_preserves_declaration_order() {
    let json = serde_json::json!({
        "setup": "cp .env.example .env",
        "install": "npm install",
        "build": ["npm", "run", "build"]
    });
    let result = LifecycleCommandValue::from_json_value(&json)
        .unwrap()
        .unwrap();
    match result {
        LifecycleCommandValue::Parallel(map) => {
            let keys: Vec<&String> = map.keys().collect();
            assert_eq!(keys, vec!["setup", "install", "build"]);
        }
        _ => panic!("Expected Parallel variant"),
    }
}

#[test]
fn test_parallel_format_empty_object_is_noop() {
    let json = serde_json::json!({});
    let result = LifecycleCommandValue::from_json_value(&json)
        .unwrap()
        .unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_parallel_format_skips_invalid_values() {
    let json = serde_json::json!({
        "install": "npm install",
        "bad": 42,
        "build": ["npm", "run", "build"]
    });
    let result = LifecycleCommandValue::from_json_value(&json)
        .unwrap()
        .unwrap();
    match result {
        LifecycleCommandValue::Parallel(map) => {
            assert_eq!(map.len(), 2); // "bad" was skipped
            assert!(map.contains_key("install"));
            assert!(map.contains_key("build"));
            assert!(!map.contains_key("bad"));
        }
        _ => panic!("Expected Parallel variant"),
    }
}

#[test]
fn test_parallel_format_skips_null_values() {
    let json = serde_json::json!({
        "install": "npm install",
        "noop": null
    });
    let result = LifecycleCommandValue::from_json_value(&json)
        .unwrap()
        .unwrap();
    match result {
        LifecycleCommandValue::Parallel(map) => {
            assert_eq!(map.len(), 1);
            assert!(map.contains_key("install"));
        }
        _ => panic!("Expected Parallel variant"),
    }
}

#[test]
fn test_parallel_format_variable_substitution_recursive() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let mut map = IndexMap::new();
    map.insert(
        "shell".to_string(),
        LifecycleCommandValue::Shell("echo hello".to_string()),
    );
    map.insert(
        "exec".to_string(),
        LifecycleCommandValue::Exec(vec!["echo".to_string(), "world".to_string()]),
    );
    let cmd = LifecycleCommandValue::Parallel(map);
    let context = deacon_core::variable::SubstitutionContext::new(temp_dir.path()).unwrap();
    let substituted = cmd.substitute_variables(&context);
    match substituted {
        LifecycleCommandValue::Parallel(m) => {
            assert_eq!(m.len(), 2);
            assert!(m.contains_key("shell"));
            assert!(m.contains_key("exec"));
        }
        _ => panic!("Expected Parallel variant after substitution"),
    }
}

#[test]
fn test_parallel_format_in_command_list() {
    let mut map = IndexMap::new();
    map.insert(
        "install".to_string(),
        LifecycleCommandValue::Shell("npm install".to_string()),
    );
    let cmd_list = LifecycleCommandList {
        commands: vec![AggregatedLifecycleCommand {
            command: LifecycleCommandValue::Parallel(map),
            source: LifecycleCommandSource::Config,
        }],
    };
    assert_eq!(cmd_list.len(), 1);
}

// ================================================================
// Mixed Format Tests (all three formats together)
// ================================================================

#[test]
fn test_all_formats_in_aggregated_list() {
    // Simulates a phase with multiple commands from different sources
    let mut parallel_map = IndexMap::new();
    parallel_map.insert(
        "install".to_string(),
        LifecycleCommandValue::Shell("npm install".to_string()),
    );

    let cmd_list = LifecycleCommandList {
        commands: vec![
            AggregatedLifecycleCommand {
                command: LifecycleCommandValue::Shell("apt-get update".to_string()),
                source: LifecycleCommandSource::Feature {
                    id: "base".to_string(),
                },
            },
            AggregatedLifecycleCommand {
                command: LifecycleCommandValue::Exec(vec![
                    "pip".to_string(),
                    "install".to_string(),
                    "-r".to_string(),
                    "requirements.txt".to_string(),
                ]),
                source: LifecycleCommandSource::Feature {
                    id: "python".to_string(),
                },
            },
            AggregatedLifecycleCommand {
                command: LifecycleCommandValue::Parallel(parallel_map),
                source: LifecycleCommandSource::Config,
            },
        ],
    };

    assert_eq!(cmd_list.len(), 3);
    assert!(matches!(
        &cmd_list.commands[0].command,
        LifecycleCommandValue::Shell(_)
    ));
    assert!(matches!(
        &cmd_list.commands[1].command,
        LifecycleCommandValue::Exec(_)
    ));
    assert!(matches!(
        &cmd_list.commands[2].command,
        LifecycleCommandValue::Parallel(_)
    ));
}

// ================================================================
// Format Detection Tests
// ================================================================

#[test]
fn test_format_detection_number_rejected() {
    let json = serde_json::json!(42);
    assert!(LifecycleCommandValue::from_json_value(&json).is_err());
}

#[test]
fn test_format_detection_boolean_rejected() {
    let json = serde_json::json!(true);
    assert!(LifecycleCommandValue::from_json_value(&json).is_err());
}

#[test]
fn test_format_detection_null_returns_none() {
    let json = serde_json::json!(null);
    let result = LifecycleCommandValue::from_json_value(&json).unwrap();
    assert_eq!(result, None);
}

// ================================================================
// Parallel Execution Concurrency Tests (SC-003)
// ================================================================
// These tests verify that the parallel execution pattern used by the
// production code (JoinSet with spawn_blocking for host-side,
// futures::future::join_all for container-side) achieves actual
// concurrency rather than sequential execution.

/// Both concurrency tests below rendezvous instead of racing a stopwatch.
///
/// They used to sleep 500ms per task and assert the pair finished under 900ms,
/// inferring concurrency from the gap between ~500ms (concurrent) and ~1000ms
/// (sequential). That bound is intrinsically under 2x, so a loaded box can breach
/// it without any regression — the same disease as the redaction perf assertions
/// in #517, and unfixable by widening, since widening past 1000ms would stop
/// distinguishing concurrent from sequential at all.
///
/// A rendezvous tests the property directly: each task announces its arrival and
/// then waits for the other's. Under genuine concurrency both arrive and both
/// proceed, however slow the machine is. Under sequential execution the first
/// task waits for a partner that has not been started yet and never will be, so
/// it gives up after the guard below and the test fails. The guard is orders of
/// magnitude above any scheduling delay: reachable by the regression, never by
/// load.
///
/// Every wait is *bounded* rather than infinite. An unbounded rendezvous turns
/// the regression into a hung test instead of a failing one — a blocking task
/// parked forever also blocks tokio's runtime shutdown, so the test never even
/// reaches its assertion.
const RENDEZVOUS_GUARD: std::time::Duration = std::time::Duration::from_secs(10);

/// Verifies that parallel execution via JoinSet (the host-side pattern) really
/// runs both tasks at once. This proves the JoinSet::spawn_blocking pattern
/// achieves true concurrency rather than serializing the entries.
#[tokio::test]
async fn test_parallel_execution_is_concurrent() {
    use std::sync::mpsc;
    use tokio::task::JoinSet;

    let mut set = JoinSet::new();

    // Spawn two blocking tasks that each announce their arrival and then wait
    // for the other's, mirroring the host-side parallel execution pattern in
    // execute_host_lifecycle_phase (JoinSet::spawn_blocking). Only tasks that
    // are genuinely in flight together can both observe a peer.
    let (tx_a, rx_a) = mpsc::channel::<()>();
    let (tx_b, rx_b) = mpsc::channel::<()>();
    for (tx, rx) in [(tx_a, rx_b), (tx_b, rx_a)] {
        set.spawn_blocking(move || {
            tx.send(()).expect("peer task should still be alive");
            rx.recv_timeout(RENDEZVOUS_GUARD).is_ok()
        });
    }

    // Collect all results (mirrors "wait for ALL" pattern)
    let mut observed_peer = Vec::new();
    while let Some(result) = set.join_next().await {
        observed_peer.push(result.expect("spawned task should not panic"));
    }

    assert_eq!(
        observed_peer,
        vec![true, true],
        "spawn_blocking entries did not run concurrently: a task waited out the \
         full {RENDEZVOUS_GUARD:?} without its peer ever starting"
    );
}

/// Verifies that the futures::future::join_all pattern (container-side)
/// also achieves true concurrency with async tasks.
#[tokio::test]
async fn test_parallel_execution_is_concurrent_async() {
    use futures::channel::oneshot;

    // Each future signals its own arrival, then awaits the other's. join_all
    // polls both before either completes, so both resolve; awaiting them one at
    // a time would leave the first waiting forever. tokio::sync is not enabled
    // for this workspace, so the rendezvous uses futures' oneshot channels.
    let (tx_a, rx_a) = oneshot::channel::<()>();
    let (tx_b, rx_b) = oneshot::channel::<()>();

    let first = async move {
        tx_a.send(()).expect("peer future should still be alive");
        rx_b.await.expect("peer future should signal arrival");
    };
    let second = async move {
        tx_b.send(()).expect("peer future should still be alive");
        rx_a.await.expect("peer future should signal arrival");
    };

    let joined = tokio::time::timeout(
        RENDEZVOUS_GUARD,
        futures::future::join_all(vec![
            Box::pin(first) as std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>,
            Box::pin(second),
        ]),
    )
    .await;

    assert!(
        joined.is_ok(),
        "join_all did not drive the futures concurrently: neither future observed \
         the other's arrival within {RENDEZVOUS_GUARD:?}"
    );
}

/// Verifies that when one parallel entry fails, the others still run to
/// completion (no early cancellation). This matches Decision 8: "wait for ALL
/// results" semantics in both host-side (JoinSet) and container-side (join_all).
#[tokio::test]
async fn test_parallel_execution_waits_for_all_on_failure() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use tokio::task::JoinSet;

    let slow_task_completed = Arc::new(AtomicBool::new(false));
    let slow_flag = Arc::clone(&slow_task_completed);

    let mut set = JoinSet::new();

    // Task 1: fails immediately via a non-zero exit code simulation
    set.spawn_blocking(|| -> Result<(), String> { Err("simulated failure".to_string()) });

    // Task 2: takes 200ms but should still complete even though task 1 failed
    set.spawn_blocking(move || -> Result<(), String> {
        std::thread::sleep(Duration::from_millis(200));
        slow_flag.store(true, Ordering::SeqCst);
        Ok(())
    });

    // Collect ALL results without early cancellation
    let mut results = Vec::new();
    while let Some(join_result) = set.join_next().await {
        results.push(join_result.expect("spawned task should not panic"));
    }

    // Both tasks ran: we got two results
    assert_eq!(
        results.len(),
        2,
        "Expected 2 results, got {}",
        results.len()
    );

    // The slow task completed despite the fast task failing
    assert!(
        slow_task_completed.load(Ordering::SeqCst),
        "Slow task should have completed even though the fast task failed"
    );

    // Verify we have both a success and a failure
    let failures = results.iter().filter(|r| r.is_err()).count();
    let successes = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(failures, 1, "Expected exactly 1 failure");
    assert_eq!(successes, 1, "Expected exactly 1 success");
}

/// Verifies the same no-early-cancellation behavior using the async join_all
/// pattern (container-side execution path).
#[tokio::test]
async fn test_parallel_async_execution_waits_for_all_on_failure() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    let slow_task_completed = Arc::new(AtomicBool::new(false));
    let slow_flag = Arc::clone(&slow_task_completed);

    // Build futures mirroring the container-side pattern
    let future_fail = async { Result::<(), String>::Err("simulated failure".to_string()) };

    let future_slow = async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        slow_flag.store(true, Ordering::SeqCst);
        Result::<(), String>::Ok(())
    };

    // join_all waits for ALL futures regardless of individual results
    let results = futures::future::join_all(vec![
        Box::pin(future_fail)
            as std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>,
        Box::pin(future_slow),
    ])
    .await;

    assert_eq!(
        results.len(),
        2,
        "Expected 2 results, got {}",
        results.len()
    );
    assert!(
        slow_task_completed.load(Ordering::SeqCst),
        "Slow task should have completed even though the fast task failed"
    );

    let failures = results.iter().filter(|r| r.is_err()).count();
    let successes = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(failures, 1, "Expected exactly 1 failure");
    assert_eq!(successes, 1, "Expected exactly 1 success");
}
