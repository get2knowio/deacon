//! Integration tests for secret redaction functionality
//!
//! These tests validate the complete redaction workflow including CLI integration,
//! lifecycle command execution, and performance characteristics.

use assert_cmd::Command;
use deacon_core::{
    lifecycle::{run_phase, ExecutionContext, LifecycleCommands, LifecyclePhase},
    redaction::{add_global_secret, global_registry, redact_if_enabled, RedactionConfig},
};
use serde_json::json;
use std::collections::HashMap;
use std::time::Instant;

#[test]
fn test_redaction_in_lifecycle_execution() {
    // Clear any existing secrets
    global_registry().clear();

    // Add a test secret
    add_global_secret("my-secret-password");

    // Create commands that will output the secret
    let commands_json = json!([
        "echo 'The password is my-secret-password'",
        "echo 'No secrets here'"
    ]);
    let env = HashMap::new();
    let commands = LifecycleCommands::from_json_value(&commands_json, &env).unwrap();

    // Create execution context with redaction enabled
    let ctx = ExecutionContext::new().with_redaction_config(RedactionConfig::default());

    // Execute the lifecycle phase
    let result = run_phase(LifecyclePhase::PostCreate, &commands, &ctx).unwrap();

    // Check that the secret was redacted in the result
    assert!(result.stdout.contains("****"));
    assert!(!result.stdout.contains("my-secret-password"));
    assert!(result.stdout.contains("No secrets here"));

    // Clean up
    global_registry().clear();
}

#[test]
fn test_redaction_disabled_shows_secrets() {
    // Clear any existing secrets
    global_registry().clear();

    // Add a test secret
    add_global_secret("test-secret-123");

    // Create commands that will output the secret
    let commands_json = json!(["echo 'The secret is test-secret-123'"]);
    let env = HashMap::new();
    let commands = LifecycleCommands::from_json_value(&commands_json, &env).unwrap();

    // Create execution context with redaction disabled
    let ctx = ExecutionContext::new().with_redaction_config(RedactionConfig::disabled());

    // Execute the lifecycle phase
    let result = run_phase(LifecyclePhase::PostCreate, &commands, &ctx).unwrap();

    // Check that the secret was NOT redacted
    assert!(result.stdout.contains("test-secret-123"));
    assert!(!result.stdout.contains("****"));

    // Clean up
    global_registry().clear();
}

#[test]
fn test_multiple_secrets_redaction() {
    // Clear any existing secrets
    global_registry().clear();

    // Add multiple secrets
    add_global_secret("password123");
    add_global_secret("api-key-xyz");
    add_global_secret("token-abc-def");

    // Create commands that output multiple secrets
    let commands_json = json!(["echo 'Pass: password123, API: api-key-xyz, Token: token-abc-def'"]);
    let env = HashMap::new();
    let commands = LifecycleCommands::from_json_value(&commands_json, &env).unwrap();

    // Create execution context with redaction enabled
    let ctx = ExecutionContext::new().with_redaction_config(RedactionConfig::default());

    // Execute the lifecycle phase
    let result = run_phase(LifecyclePhase::PostCreate, &commands, &ctx).unwrap();

    // Check that all secrets were redacted
    assert!(!result.stdout.contains("password123"));
    assert!(!result.stdout.contains("api-key-xyz"));
    assert!(!result.stdout.contains("token-abc-def"));
    assert_eq!(result.stdout.matches("****").count(), 3);

    // Clean up
    global_registry().clear();
}

#[test]
fn test_partial_secret_matches_not_redacted() {
    // Clear any existing secrets
    global_registry().clear();

    // Add a secret
    add_global_secret("supersecret");

    // Create commands with partial matches only (no full secret)
    let commands_json = json!(["echo 'This has super and secret but not the full match'"]);
    let env = HashMap::new();
    let commands = LifecycleCommands::from_json_value(&commands_json, &env).unwrap();

    // Create execution context with redaction enabled
    let ctx = ExecutionContext::new().with_redaction_config(RedactionConfig::default());

    // Execute the lifecycle phase
    let result = run_phase(LifecyclePhase::PostCreate, &commands, &ctx).unwrap();

    // Check that partial matches were NOT redacted
    assert!(result.stdout.contains("super"));
    assert!(result.stdout.contains("secret"));
    assert!(!result.stdout.contains("****"));

    // Clean up
    global_registry().clear();
}

#[test]
fn test_secrets_in_stderr() {
    // Clear any existing secrets
    global_registry().clear();

    // Add a test secret
    add_global_secret("error-secret");

    // Create commands that output secret to stderr
    let commands_json = json!(["sh -c 'echo \"Error: error-secret\" >&2'"]);
    let env = HashMap::new();
    let commands = LifecycleCommands::from_json_value(&commands_json, &env).unwrap();

    // Create execution context with redaction enabled
    let ctx = ExecutionContext::new().with_redaction_config(RedactionConfig::default());

    // Execute the lifecycle phase
    let result = run_phase(LifecyclePhase::PostCreate, &commands, &ctx).unwrap();

    // Check that the secret was redacted in stderr
    assert!(result.stderr.contains("****"));
    assert!(!result.stderr.contains("error-secret"));

    // Clean up
    global_registry().clear();
}

#[test]
fn test_custom_redaction_placeholder() {
    // Clear any existing secrets
    global_registry().clear();

    // Add a test secret
    add_global_secret("my-custom-secret");

    // Test custom placeholder
    let config = RedactionConfig::with_placeholder("[HIDDEN]".to_string());
    let text = "This contains my-custom-secret";

    let result = redact_if_enabled(text, &config);
    assert_eq!(result, "This contains [HIDDEN]");
    assert!(!result.contains("my-custom-secret"));
    assert!(!result.contains("****"));

    // Clean up
    global_registry().clear();
}

#[test]
fn test_no_redact_cli_flag() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("--no-redact").arg("--help");

    // The command should succeed and show help
    cmd.assert().success();
}

#[test]
fn test_redaction_performance_short_lines() {
    // Clear any existing secrets
    global_registry().clear();

    // Add some secrets to the global registry
    for i in 0..100 {
        add_global_secret(&format!("secret-{:03}", i));
    }

    let config = RedactionConfig::default();
    let test_lines = vec![
        "This is a normal log line without any secrets",
        "Another line with different content",
        "Error: something went wrong",
        "DEBUG: processing request",
        "INFO: operation completed successfully",
    ];

    // Measure performance for short lines
    let start = Instant::now();
    for _ in 0..1000 {
        for line in &test_lines {
            let _result = redact_if_enabled(line, &config);
        }
    }
    let duration = start.elapsed();

    // Performance should be reasonable - less than 500ms for 5000 operations with 100 secrets
    // This is a rough benchmark to ensure redaction doesn't add excessive overhead
    assert!(
        duration.as_millis() < 500,
        "Redaction took too long: {:?}",
        duration
    );

    // Clean up
    global_registry().clear();
}

#[test]
fn test_redaction_performance_with_secrets() {
    // Clear any existing secrets
    global_registry().clear();

    // Add some secrets
    add_global_secret("performance-secret-1");
    add_global_secret("performance-secret-2");
    add_global_secret("performance-secret-3");

    let config = RedactionConfig::default();
    let test_lines = vec![
        "Log line with performance-secret-1 in it",
        "Another line with performance-secret-2 here",
        "Third line contains performance-secret-3",
        "Normal line without secrets",
        "Another normal line",
    ];

    // Measure performance when secrets are present
    let start = Instant::now();
    for _ in 0..1000 {
        for line in &test_lines {
            let _result = redact_if_enabled(line, &config);
        }
    }
    let duration = start.elapsed();

    // Performance should still be reasonable even with secret redaction
    assert!(
        duration.as_millis() < 200,
        "Redaction with secrets took too long: {:?}",
        duration
    );

    // Clean up
    global_registry().clear();
}

#[test]
fn test_secret_length_threshold() {
    // Clear any existing secrets
    global_registry().clear();

    // Try to add secrets that are too short (should be ignored)
    add_global_secret("short"); // 5 chars - should be ignored
    add_global_secret("tiny"); // 4 chars - should be ignored

    // Add a secret that meets the length requirement
    add_global_secret("long-enough"); // 11 chars - should be added

    // Verify only the long secret was added
    assert_eq!(global_registry().secret_count(), 1);

    let config = RedactionConfig::default();

    // Test that short secrets are not redacted
    let result1 = redact_if_enabled("This contains short", &config);
    assert_eq!(result1, "This contains short");

    let result2 = redact_if_enabled("This contains tiny", &config);
    assert_eq!(result2, "This contains tiny");

    // Test that long secret is redacted
    let result3 = redact_if_enabled("This contains long-enough", &config);
    assert_eq!(result3, "This contains ****");

    // Clean up
    global_registry().clear();
}

#[test]
fn test_thread_safety() {
    use std::thread;

    // Clear any existing secrets
    global_registry().clear();

    // Test concurrent access to the global registry
    let handles: Vec<_> = (0..10)
        .map(|i| {
            thread::spawn(move || {
                // Each thread adds its own secret
                add_global_secret(&format!("thread-secret-{}", i));

                // Each thread tries to redact text
                let config = RedactionConfig::default();
                let text = format!("This contains thread-secret-{}", i);

                // Should eventually be redacted (might not be immediate due to race conditions)
                redact_if_enabled(&text, &config)
            })
        })
        .collect();

    // Wait for all threads to complete
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // At least some results should show redaction occurred
    let redacted_count = results.iter().filter(|r| r.contains("****")).count();
    assert!(redacted_count > 0, "No redaction occurred in threaded test");

    // Registry should have secrets from multiple threads
    assert!(global_registry().secret_count() > 0);

    // Clean up
    global_registry().clear();
}
