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

/// Verifies that parallel execution via JoinSet (the host-side pattern)
/// completes two 500ms tasks in roughly 500ms wall-clock time, not 1000ms.
/// This proves the JoinSet::spawn_blocking pattern achieves true concurrency.
#[tokio::test]
async fn test_parallel_execution_is_concurrent() {
    use std::time::{Duration, Instant};
    use tokio::task::JoinSet;

    let start = Instant::now();
    let mut set = JoinSet::new();

    // Spawn two blocking tasks that each sleep for 500ms,
    // mirroring the host-side parallel execution pattern in
    // execute_host_lifecycle_phase (JoinSet::spawn_blocking).
    for _ in 0..2 {
        set.spawn_blocking(|| {
            std::thread::sleep(Duration::from_millis(500));
        });
    }

    // Collect all results (mirrors "wait for ALL" pattern)
    while let Some(result) = set.join_next().await {
        result.expect("spawned task should not panic");
    }

    let elapsed = start.elapsed();
    // If concurrent: ~500ms. If sequential: ~1000ms.
    // Use 900ms as threshold to allow generous margin for CI.
    assert!(
        elapsed < Duration::from_millis(900),
        "Parallel execution took {:?}, expected < 900ms (should be ~500ms if concurrent)",
        elapsed
    );
}

/// Verifies that the futures::future::join_all pattern (container-side)
/// also achieves true concurrency with async tasks.
#[tokio::test]
async fn test_parallel_execution_is_concurrent_async() {
    use std::time::{Duration, Instant};

    let start = Instant::now();

    // Build futures for two async tasks that each sleep 500ms,
    // mirroring the container-side parallel execution pattern
    // in execute_container_lifecycle_phase (join_all).
    let futures: Vec<_> = (0..2)
        .map(|_| async {
            tokio::time::sleep(Duration::from_millis(500)).await;
        })
        .collect();

    futures::future::join_all(futures).await;

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(900),
        "Async parallel execution took {:?}, expected < 900ms (should be ~500ms if concurrent)",
        elapsed
    );
}

/// Verifies that when one parallel entry fails, the others still run to
/// completion (no early cancellation). This matches Decision 8: "wait for ALL
/// results" semantics in both host-side (JoinSet) and container-side (join_all).
#[tokio::test]
async fn test_parallel_execution_waits_for_all_on_failure() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
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
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
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
