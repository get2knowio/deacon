//! Integration tests for secret redaction functionality
//!
//! These tests validate the complete redaction workflow including CLI integration,
//! lifecycle command execution, and performance characteristics.

use assert_cmd::Command;
use deacon_core::{
    lifecycle::{run_phase, ExecutionContext, LifecycleCommands, LifecyclePhase},
    redaction::{
        add_global_secret, global_registry, redact_if_enabled, RedactingWriter, RedactionConfig,
        SecretRegistry,
    },
};
use serde_json::json;
use std::collections::HashMap;
use std::time::Instant;

#[test]
fn test_redaction_in_lifecycle_execution() {
    // Create a custom registry for this test to avoid global state interference
    let registry = SecretRegistry::new();
    registry.add_secret("my-secret-password");

    // Create commands that will output the secret
    let commands_json = json!([
        "echo 'The password is my-secret-password'",
        "echo 'No secrets here'"
    ]);
    let env = HashMap::new();
    let commands = LifecycleCommands::from_json_value(&commands_json, &env).unwrap();

    // Create execution context with redaction enabled using custom registry
    let ctx = ExecutionContext::new()
        .with_redaction_config(RedactionConfig::with_custom_registry(registry));

    // Execute the lifecycle phase
    let result = run_phase(LifecyclePhase::PostCreate, &commands, &ctx).unwrap();

    // Check that the secret was redacted in the result
    assert!(result.stdout.contains("****"));
    assert!(!result.stdout.contains("my-secret-password"));
    assert!(result.stdout.contains("No secrets here"));
}

#[test]
fn test_redaction_disabled_shows_secrets() {
    // Create a custom registry for this test
    let registry = SecretRegistry::new();
    registry.add_secret("test-secret-123");

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
}

#[test]
fn test_multiple_secrets_redaction() {
    // Create a custom registry for this test to avoid global state interference
    let registry = SecretRegistry::new();
    registry.add_secret("password123");
    registry.add_secret("api-key-xyz");
    registry.add_secret("token-abc-def");

    // Create commands that output multiple secrets
    let commands_json = json!(["echo 'Pass: password123, API: api-key-xyz, Token: token-abc-def'"]);
    let env = HashMap::new();
    let commands = LifecycleCommands::from_json_value(&commands_json, &env).unwrap();

    // Create execution context with redaction enabled using custom registry
    let ctx = ExecutionContext::new()
        .with_redaction_config(RedactionConfig::with_custom_registry(registry));

    // Execute the lifecycle phase
    let result = run_phase(LifecyclePhase::PostCreate, &commands, &ctx).unwrap();

    // Check that all secrets were redacted
    assert!(!result.stdout.contains("password123"));
    assert!(!result.stdout.contains("api-key-xyz"));
    assert!(!result.stdout.contains("token-abc-def"));
    assert_eq!(result.stdout.matches("****").count(), 3);
}

#[test]
fn test_partial_secret_matches_not_redacted() {
    // Create a custom registry for this test
    let registry = SecretRegistry::new();
    registry.add_secret("supersecret");

    // Create commands with partial matches only (no full secret)
    let commands_json = json!(["echo 'This has super and secret but not the full match'"]);
    let env = HashMap::new();
    let commands = LifecycleCommands::from_json_value(&commands_json, &env).unwrap();

    // Create execution context with redaction enabled using custom registry
    let ctx = ExecutionContext::new()
        .with_redaction_config(RedactionConfig::with_custom_registry(registry));

    // Execute the lifecycle phase
    let result = run_phase(LifecyclePhase::PostCreate, &commands, &ctx).unwrap();

    // Check that partial matches were NOT redacted
    assert!(result.stdout.contains("super"));
    assert!(result.stdout.contains("secret"));
    assert!(!result.stdout.contains("****"));
}

#[test]
fn test_secrets_in_stderr() {
    // Create a custom registry for this test to avoid global state interference
    let registry = SecretRegistry::new();
    registry.add_secret("error-secret");

    // Create commands that output secret to stderr
    let commands_json = json!(["sh -c 'echo \"Error: error-secret\" >&2'"]);
    let env = HashMap::new();
    let commands = LifecycleCommands::from_json_value(&commands_json, &env).unwrap();

    // Create execution context with redaction enabled using custom registry
    let ctx = ExecutionContext::new()
        .with_redaction_config(RedactionConfig::with_custom_registry(registry));

    // Execute the lifecycle phase
    let result = run_phase(LifecyclePhase::PostCreate, &commands, &ctx).unwrap();

    // Check that the secret was redacted in stderr
    assert!(result.stderr.contains("****"));
    assert!(!result.stderr.contains("error-secret"));
}

#[test]
fn test_custom_redaction_placeholder() {
    // Create a custom registry for this test
    let registry = SecretRegistry::new();
    registry.add_secret("my-custom-secret");

    // Test custom placeholder
    let config = RedactionConfig::with_placeholder_and_registry("[HIDDEN]".to_string(), registry);
    let text = "This contains my-custom-secret";

    let result = redact_if_enabled(text, &config);
    assert_eq!(result, "This contains [HIDDEN]");
    assert!(!result.contains("my-custom-secret"));
    assert!(!result.contains("****"));
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
    // Create a custom registry for this test
    let registry = SecretRegistry::new();

    // Try to add secrets that are too short (should be ignored)
    registry.add_secret("short"); // 5 chars - should be ignored
    registry.add_secret("tiny"); // 4 chars - should be ignored

    // Add a secret that meets the length requirement
    registry.add_secret("long-enough"); // 11 chars - should be added

    // Verify only the long secret was added
    assert_eq!(registry.secret_count(), 1);

    let config = RedactionConfig::with_custom_registry(registry);

    // Test that short secrets are not redacted
    let result1 = redact_if_enabled("This contains short", &config);
    assert_eq!(result1, "This contains short");

    let result2 = redact_if_enabled("This contains tiny", &config);
    assert_eq!(result2, "This contains tiny");

    // Test that long secret is redacted
    let result3 = redact_if_enabled("This contains long-enough", &config);
    assert_eq!(result3, "This contains ****");
}

#[test]
fn test_thread_safety() {
    use std::sync::Barrier;
    use std::thread;

    // Clear any existing secrets
    global_registry().clear();

    // Create a barrier to synchronize thread execution
    let barrier = std::sync::Arc::new(Barrier::new(10));

    // Test concurrent access to the global registry
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let barrier = barrier.clone();
            thread::spawn(move || {
                // Each thread adds its own secret
                add_global_secret(&format!("thread-secret-{}", i));

                // Wait for all threads to finish adding secrets
                barrier.wait();

                // Now each thread tries to redact text
                let config = RedactionConfig::default();
                let text = format!("This contains thread-secret-{}", i);

                // Should be redacted since all secrets are now added
                redact_if_enabled(&text, &config)
            })
        })
        .collect();

    // Wait for all threads to complete
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // All results should show redaction occurred since we synchronized
    let redacted_count = results.iter().filter(|r| r.contains("****")).count();
    assert_eq!(
        redacted_count, 10,
        "All threads should have had their secrets redacted"
    );

    // Registry should have secrets from all threads
    assert_eq!(global_registry().secret_count(), 10);

    // Clean up
    global_registry().clear();
}

#[test]
fn test_overlapping_secrets() {
    // Create a custom registry for this test
    let registry = SecretRegistry::new();

    // Add overlapping secrets where one is a substring of another
    registry.add_secret("secret123");
    registry.add_secret("mysecret123data");
    registry.add_secret("123dataXXX"); // Make this longer to meet minimum length

    let config = RedactionConfig::with_custom_registry(registry);

    // Test with the longer secret that contains the shorter ones
    let result1 = redact_if_enabled("Found mysecret123data here", &config);
    assert!(result1.contains("****"));
    assert!(!result1.contains("mysecret123data"));

    // Test with the shorter secret alone
    let result2 = redact_if_enabled("Found secret123 here", &config);
    assert!(result2.contains("****"));
    assert!(!result2.contains("secret123"));

    // Test with the other secret (now long enough)
    let result3 = redact_if_enabled("Found 123dataXXX here", &config);
    assert!(result3.contains("****"));
    assert!(!result3.contains("123dataXXX"));

    // Test with text containing multiple overlapping secrets
    let result4 = redact_if_enabled("secret123 and mysecret123data", &config);
    // Both should be redacted
    assert_eq!(result4.matches("****").count(), 2);
    assert!(!result4.contains("secret123"));
    assert!(!result4.contains("mysecret123data"));
}

#[test]
fn test_multiline_output_redaction() {
    // Create a custom registry for this test
    let registry = SecretRegistry::new();
    registry.add_secret("multiline-secret");
    registry.add_secret("another-secret");

    let config = RedactionConfig::with_custom_registry(registry);

    // Test multiline text with secrets on different lines
    let multiline_text = "Line 1: This is normal text\n\
                          Line 2: Contains multiline-secret here\n\
                          Line 3: Normal text again\n\
                          Line 4: Has another-secret in it\n\
                          Line 5: Final line";

    let result = redact_if_enabled(multiline_text, &config);

    // Verify secrets are redacted but structure is preserved
    assert!(result.contains("Line 1: This is normal text"));
    assert!(result.contains("Line 2: Contains **** here"));
    assert!(result.contains("Line 3: Normal text again"));
    assert!(result.contains("Line 4: Has **** in it"));
    assert!(result.contains("Line 5: Final line"));
    assert!(!result.contains("multiline-secret"));
    assert!(!result.contains("another-secret"));
}

#[test]
fn test_secret_at_string_boundaries() {
    // Create a custom registry for this test
    let registry = SecretRegistry::new();
    registry.add_secret("boundary-secret");

    let config = RedactionConfig::with_custom_registry(registry);

    // Test secret at start of string
    let result1 = redact_if_enabled("boundary-secret is at start", &config);
    assert_eq!(result1, "**** is at start");

    // Test secret at end of string
    let result2 = redact_if_enabled("This ends with boundary-secret", &config);
    assert_eq!(result2, "This ends with ****");

    // Test secret as entire string
    let result3 = redact_if_enabled("boundary-secret", &config);
    assert_eq!(result3, "****");
}

#[test]
fn test_repeated_secrets_in_same_line() {
    // Create a custom registry for this test
    let registry = SecretRegistry::new();
    registry.add_secret("repeated-secret");

    let config = RedactionConfig::with_custom_registry(registry);

    // Test multiple occurrences of same secret in one line
    let result = redact_if_enabled(
        "repeated-secret and repeated-secret and repeated-secret",
        &config,
    );

    // All occurrences should be redacted
    assert_eq!(result, "**** and **** and ****");
    assert!(!result.contains("repeated-secret"));
    assert_eq!(result.matches("****").count(), 3);
}

#[test]
fn test_secrets_with_special_characters() {
    // Create a custom registry for this test
    let registry = SecretRegistry::new();
    registry.add_secret("secret@#$%^&*()");
    registry.add_secret("secret-with-dashes");
    registry.add_secret("secret_with_underscores");
    registry.add_secret("secret.with.dots");

    let config = RedactionConfig::with_custom_registry(registry);

    // Test secrets with various special characters
    let result1 = redact_if_enabled("Found secret@#$%^&*() here", &config);
    assert_eq!(result1, "Found **** here");

    let result2 = redact_if_enabled("Found secret-with-dashes here", &config);
    assert_eq!(result2, "Found **** here");

    let result3 = redact_if_enabled("Found secret_with_underscores here", &config);
    assert_eq!(result3, "Found **** here");

    let result4 = redact_if_enabled("Found secret.with.dots here", &config);
    assert_eq!(result4, "Found **** here");
}

#[test]
fn test_secrets_with_unicode() {
    // Create a custom registry for this test
    let registry = SecretRegistry::new();
    registry.add_secret("秘密パスワード123"); // Japanese secret
    registry.add_secret("пароль456"); // Russian secret
    registry.add_secret("🔐secret789"); // Emoji secret

    let config = RedactionConfig::with_custom_registry(registry);

    // Test Unicode secrets
    let result1 = redact_if_enabled("Found 秘密パスワード123 in log", &config);
    assert_eq!(result1, "Found **** in log");

    let result2 = redact_if_enabled("Found пароль456 in log", &config);
    assert_eq!(result2, "Found **** in log");

    let result3 = redact_if_enabled("Found 🔐secret789 in log", &config);
    assert_eq!(result3, "Found **** in log");
}

#[test]
fn test_port_event_redaction() {
    // Test that PORT_EVENT lines are also redacted
    let registry = SecretRegistry::new();
    registry.add_secret("port-secret-token");

    // Create commands that simulate PORT_EVENT output with secrets
    let commands_json =
        json!(["echo 'PORT_EVENT: {\"port\": 3000, \"token\": \"port-secret-token\"}'"]);
    let env = HashMap::new();
    let commands = LifecycleCommands::from_json_value(&commands_json, &env).unwrap();

    // Create execution context with redaction enabled using custom registry
    let ctx = ExecutionContext::new()
        .with_redaction_config(RedactionConfig::with_custom_registry(registry));

    // Execute the lifecycle phase
    let result = run_phase(LifecyclePhase::PostCreate, &commands, &ctx).unwrap();

    // Check that secrets in PORT_EVENT are redacted
    assert!(result.stdout.contains("PORT_EVENT"));
    assert!(result.stdout.contains("****"));
    assert!(!result.stdout.contains("port-secret-token"));
}

#[test]
fn test_very_long_lines_with_secrets() {
    // Create a custom registry for this test
    let registry = SecretRegistry::new();
    registry.add_secret("long-line-secret");

    let config = RedactionConfig::with_custom_registry(registry);

    // Create a very long line with secret buried in it
    let mut long_line = "START ".to_string();
    long_line.push_str(&"normal-text ".repeat(1000));
    long_line.push_str("long-line-secret ");
    long_line.push_str(&"more-normal-text ".repeat(1000));
    long_line.push_str("END");

    let result = redact_if_enabled(&long_line, &config);

    // Secret should be redacted even in very long lines
    assert!(result.contains("****"));
    assert!(!result.contains("long-line-secret"));
    assert!(result.contains("START"));
    assert!(result.contains("END"));
}

#[test]
fn test_empty_and_whitespace_handling() {
    // Create a custom registry for this test
    let registry = SecretRegistry::new();
    registry.add_secret("whitespace-secret");

    let config = RedactionConfig::with_custom_registry(registry);

    // Test empty string
    let result1 = redact_if_enabled("", &config);
    assert_eq!(result1, "");

    // Test whitespace-only strings
    let result2 = redact_if_enabled("   ", &config);
    assert_eq!(result2, "   ");

    let result3 = redact_if_enabled("\n\t\r", &config);
    assert_eq!(result3, "\n\t\r");

    // Test secret with surrounding whitespace
    let result4 = redact_if_enabled("  whitespace-secret  ", &config);
    assert_eq!(result4, "  ****  ");
}

#[test]
fn test_structured_secret_basic() {
    use deacon_core::redaction::StructuredSecret;

    let registry = SecretRegistry::new();

    // Add a structured secret that only redacts in key-value context
    let structured_secret = StructuredSecret::new(
        "commonword".to_string(),
        Some("password".to_string()),
        None,
        true,
    )
    .unwrap();
    registry.add_structured_secret(structured_secret);

    let config = RedactionConfig::with_custom_registry(registry);

    // Should NOT redact when appearing in normal text
    let result1 = redact_if_enabled("This is a commonword in normal text", &config);
    assert_eq!(result1, "This is a commonword in normal text");

    // Should redact when appearing in key-value context
    let result2 = redact_if_enabled("password=commonword", &config);
    assert_eq!(result2, "password=****");

    let result3 = redact_if_enabled("password: commonword", &config);
    assert_eq!(result3, "password: ****");

    let result4 = redact_if_enabled("\"password\":\"commonword\"", &config);
    assert_eq!(result4, "\"password\":\"****\"");
}

#[test]
fn test_structured_secret_with_context_pattern() {
    use deacon_core::redaction::StructuredSecret;

    let registry = SecretRegistry::new();

    // Add a structured secret that only redacts when a context pattern is present
    let structured_secret = StructuredSecret::new(
        "testvalue123".to_string(),
        None,
        Some("login".to_string()),
        false,
    )
    .unwrap();
    registry.add_structured_secret(structured_secret);

    let config = RedactionConfig::with_custom_registry(registry);

    // Should NOT redact when context pattern is absent
    let result1 = redact_if_enabled("Found testvalue123 in logs", &config);
    assert_eq!(result1, "Found testvalue123 in logs");

    // Should redact when context pattern is present
    let result2 = redact_if_enabled("login attempt with testvalue123", &config);
    assert_eq!(result2, "login attempt with ****");
}

#[test]
fn test_add_secret_with_key_context() {
    let registry = SecretRegistry::new();

    // Add a secret that should only be redacted in specific key contexts
    registry.add_secret_with_key_context(
        "secretvalue",
        vec![
            "password".to_string(),
            "token".to_string(),
            "api_key".to_string(),
        ],
    );

    let config = RedactionConfig::with_custom_registry(registry);

    // Should NOT redact in normal text
    let result1 = redact_if_enabled("The secretvalue is mentioned here", &config);
    assert_eq!(result1, "The secretvalue is mentioned here");

    // Should redact in password context
    let result2 = redact_if_enabled("password=secretvalue", &config);
    assert_eq!(result2, "password=****");

    // Should redact in token context
    let result3 = redact_if_enabled("token: secretvalue", &config);
    assert_eq!(result3, "token: ****");

    // Should redact in api_key context
    let result4 = redact_if_enabled("\"api_key\":\"secretvalue\"", &config);
    assert_eq!(result4, "\"api_key\":\"****\"");
}

#[test]
fn test_mixed_redaction_types() {
    use deacon_core::redaction::StructuredSecret;

    let registry = SecretRegistry::new();

    // Add regular secret (always redacted)
    registry.add_secret("alwayssecret");

    // Add structured secret (only in context)
    let structured_secret = StructuredSecret::new(
        "contextsecret".to_string(),
        Some("password".to_string()),
        None,
        true,
    )
    .unwrap();
    registry.add_structured_secret(structured_secret);

    let config = RedactionConfig::with_custom_registry(registry);

    // Regular secret should always be redacted
    let result1 = redact_if_enabled("Found alwayssecret here", &config);
    assert_eq!(result1, "Found **** here");

    // Structured secret should only be redacted in context
    let result2 = redact_if_enabled("Found contextsecret here", &config);
    assert_eq!(result2, "Found contextsecret here");

    let result3 = redact_if_enabled("password=contextsecret", &config);
    assert_eq!(result3, "password=****");

    // Both in same text with different behaviors
    let result4 = redact_if_enabled("alwayssecret and password=contextsecret", &config);
    assert_eq!(result4, "**** and password=****");
}

#[test]
fn test_doctor_output_redaction() {
    use deacon_core::doctor::DoctorContext;
    use tempfile::TempDir;

    // Create a temporary directory for the test
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();

    // Create a test doctor context (for completeness, though not used in this test)
    let _context = DoctorContext {
        workspace_folder: Some(workspace_path),
        config: None,
    };

    // Add some secrets to test redaction
    let registry = SecretRegistry::new();
    registry.add_secret("secret-api-key-123");
    registry.add_secret("docker-secret-token");

    let config = RedactionConfig::with_custom_registry(registry);

    // Note: This test verifies that doctor output would be redacted if it contained secrets
    // Since we can't easily run the full doctor command in a unit test, we test the
    // redaction of sample output that could contain secrets
    let sample_doctor_output = r#"
Deacon Doctor Diagnostics
========================

CLI Version: 1.0.0

Host OS:
  Name: Linux
  Version: Ubuntu 20.04
  Architecture: x86_64

Docker:
  Installed: true
  Version: Docker version 20.10.12 with secret-api-key-123
  Daemon Running: true
  Storage Driver: overlay2 with docker-secret-token

Configuration Discovery:
  Workspace: /workspace
  Primary Config: /workspace/.devcontainer/devcontainer.json
"#;

    let redacted_output = redact_if_enabled(sample_doctor_output, &config);

    // Verify that secrets are redacted
    assert!(redacted_output.contains("****"));
    assert!(!redacted_output.contains("secret-api-key-123"));
    assert!(!redacted_output.contains("docker-secret-token"));

    // Verify that normal content is preserved
    assert!(redacted_output.contains("Deacon Doctor Diagnostics"));
    assert!(redacted_output.contains("CLI Version: 1.0.0"));
    assert!(redacted_output.contains("Ubuntu 20.04"));
}

#[test]
fn test_audit_log_redaction() {
    use deacon_core::progress::{AuditLog, ProgressEvent, ProgressTracker};
    use tempfile::TempDir;

    // Create a temporary directory for the test
    let temp_dir = TempDir::new().unwrap();

    // Add some secrets to test redaction
    let registry = SecretRegistry::new();
    registry.add_secret("audit-secret-123");

    let config = RedactionConfig::with_custom_registry(registry);

    // Create an audit log with redaction enabled
    let mut audit_log = AuditLog::new(temp_dir.path(), 1024, config).unwrap();

    // Create an event that contains a secret
    let event = ProgressEvent::LifecycleCommandBegin {
        id: 1,
        timestamp: ProgressTracker::current_timestamp(),
        phase: "postCreate".to_string(),
        command_id: "cmd-1".to_string(),
        command: "echo 'Running audit-secret-123 in command'".to_string(),
    };

    // Log the event
    audit_log.log_event(&event).unwrap();

    // Read the log content
    let log_content = std::fs::read_to_string(temp_dir.path().join("audit.jsonl")).unwrap();

    // Verify that the secret was redacted in the audit log
    assert!(log_content.contains("****"));
    assert!(!log_content.contains("audit-secret-123"));

    // Verify that other content is preserved
    assert!(log_content.contains("lifecycle.command.begin"));
    assert!(log_content.contains("postCreate"));
    assert!(log_content.contains("echo"));
}

#[test]
fn test_redacting_writer_port_events() {
    use deacon_core::redaction::{RedactingWriter, RedactionConfig, SecretRegistry};
    #[allow(unused_imports)] // Used by RedactingWriter.write_line() method
    use std::io::Write;

    // Create a test secret registry
    let registry = SecretRegistry::new();
    registry.add_secret("port-secret-token");
    let config = RedactionConfig::with_custom_registry(registry.clone());

    // Test port event output with secrets
    let mut output = Vec::new();
    {
        let mut writer = RedactingWriter::new(&mut output, config, &registry);

        // Simulate port event output with secrets
        let port_event_line = "PORT_EVENT: {\"port\":8080,\"label\":\"API with port-secret-token\",\"description\":\"Service containing port-secret-token data\"}";

        writer.write_line(port_event_line).unwrap();
    }

    let result = String::from_utf8(output).unwrap();

    // Verify that secrets are redacted
    assert!(result.contains("****"));
    assert!(!result.contains("port-secret-token"));

    // Verify that other content is preserved
    assert!(result.contains("PORT_EVENT"));
    assert!(result.contains("8080"));
    assert!(result.contains("API"));
    assert!(result.contains("Service"));
}

#[test]
fn test_redacting_writer_stdout_integration() {
    use deacon_core::redaction::{RedactingWriter, RedactionConfig, SecretRegistry};
    #[allow(unused_imports)] // Used by RedactingWriter.write_line() method
    use std::io::Write;

    // Test with stdout (though we can't easily capture it in tests)
    let registry = SecretRegistry::new();
    registry.add_secret("stdout-secret-456");
    let config = RedactionConfig::with_custom_registry(registry.clone());

    // This test mainly verifies that RedactingWriter can wrap stdout without panicking
    {
        let stdout = std::io::stdout();
        let mut writer = RedactingWriter::new(stdout, config, &registry);

        // This would normally print to stdout but be redacted
        // We can't easily test the actual output without complex stdout capture
        let _result = writer.write_line("Test output with stdout-secret-456");
        // Don't actually write to stdout in tests - just verify it compiles and doesn't panic
    }
}

#[test]
fn test_hash_redaction_in_lifecycle() {
    // Create a custom registry for this test
    let registry = SecretRegistry::new();
    let secret = "lifecycle-hash-secret";
    registry.add_secret(secret);

    // Get the hash of the secret
    let hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(secret.as_bytes());
        format!("{:x}", hasher.finalize())
    };

    // Create commands that will output both the secret and its hash
    let commands_json = json!([
        format!("echo 'Secret: {}'", secret),
        format!("echo 'Hash: {}'", hash),
    ]);
    let env = HashMap::new();
    let commands = LifecycleCommands::from_json_value(&commands_json, &env).unwrap();

    // Create execution context with redaction enabled
    let ctx = ExecutionContext::new()
        .with_redaction_config(RedactionConfig::with_custom_registry(registry));

    // Execute the lifecycle phase
    let result = run_phase(LifecyclePhase::PostCreate, &commands, &ctx).unwrap();

    // Check that both the secret and its hash were redacted
    assert!(result.stdout.contains("****"));
    assert!(!result.stdout.contains(secret));
    assert!(!result.stdout.contains(&hash));
}

#[test]
fn test_redaction_consistency_across_outputs() {
    // This test verifies that redaction works consistently across all output types
    let registry = SecretRegistry::new();
    registry.add_secret("consistent-secret-123");
    let config = RedactionConfig::with_custom_registry(registry.clone());

    // Test 1: Direct text redaction
    let text1 = "This contains consistent-secret-123";
    let redacted1 = redact_if_enabled(text1, &config);
    assert!(!redacted1.contains("consistent-secret-123"));
    assert!(redacted1.contains("****"));

    // Test 2: Writer-based redaction
    let mut output2 = Vec::new();
    {
        let mut writer = RedactingWriter::new(&mut output2, config.clone(), &registry);
        writer.write_line(text1).unwrap();
    }
    let redacted2 = String::from_utf8(output2).unwrap();
    assert!(!redacted2.contains("consistent-secret-123"));
    assert!(redacted2.contains("****"));

    // Test 3: Redaction in JSON
    let json_text = r#"{"secret": "consistent-secret-123"}"#;
    let redacted3 = redact_if_enabled(json_text, &config);
    assert!(!redacted3.contains("consistent-secret-123"));
    assert!(redacted3.contains("****"));
}

#[test]
fn test_no_redact_flag_honored() {
    // Test that --no-redact flag properly disables redaction
    let registry = SecretRegistry::new();
    registry.add_secret("flag-test-secret");

    // With redaction disabled
    let config_disabled = RedactionConfig::disabled();
    let text = "This contains flag-test-secret";
    let result = redact_if_enabled(text, &config_disabled);
    assert_eq!(result, text);

    // With redaction enabled
    let config_enabled = RedactionConfig::with_custom_registry(registry);
    let result = redact_if_enabled(text, &config_enabled);
    assert!(!result.contains("flag-test-secret"));
    assert!(result.contains("****"));
}

#[test]
fn test_hash_and_exact_redaction_coexist() {
    use sha2::{Digest, Sha256};

    let registry = SecretRegistry::new();
    let secret = "coexist-secret";
    registry.add_secret(secret);

    // Compute hash manually
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    let config = RedactionConfig::with_custom_registry(registry);

    // Text with both secret and hash
    let text = format!("Secret: {} Hash: {}", secret, hash);
    let redacted = redact_if_enabled(&text, &config);

    // Both should be redacted
    assert_eq!(redacted.matches("****").count(), 2);
    assert!(!redacted.contains(secret));
    assert!(!redacted.contains(&hash));
}

#[test]
fn test_structured_secret_redaction_with_hashes() {
    use deacon_core::redaction::StructuredSecret;
    use sha2::{Digest, Sha256};

    let registry = SecretRegistry::new();

    // Add a structured secret
    let secret_value = "structured-value";
    let structured = StructuredSecret::new(
        secret_value.to_string(),
        Some("password".to_string()),
        None,
        true,
    )
    .unwrap();
    registry.add_structured_secret(structured);

    // Also add a regular secret for its hash
    let regular_secret = "regular-secret-123";
    registry.add_secret(regular_secret);

    let config = RedactionConfig::with_custom_registry(registry);

    // Compute hash of regular secret
    let mut hasher = Sha256::new();
    hasher.update(regular_secret.as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    // Test structured secret in context
    let text1 = format!("password={}", secret_value);
    let redacted1 = redact_if_enabled(&text1, &config);
    assert!(redacted1.contains("password=****"));

    // Test regular secret and hash
    let text2 = format!("Secret: {} Hash: {}", regular_secret, hash);
    let redacted2 = redact_if_enabled(&text2, &config);
    assert_eq!(redacted2.matches("****").count(), 2);
}

#[test]
fn test_redaction_with_multiple_output_sinks() {
    // Test that redaction works correctly when writing to multiple sinks
    let registry = SecretRegistry::new();
    registry.add_secret("multi-sink-secret");
    let config = RedactionConfig::with_custom_registry(registry.clone());

    let test_text = "This contains multi-sink-secret";

    // Sink 1: Direct string redaction
    let redacted_string = redact_if_enabled(test_text, &config);
    assert!(!redacted_string.contains("multi-sink-secret"));

    // Sink 2: Vec<u8> writer
    let mut vec_output = Vec::new();
    {
        let mut writer = RedactingWriter::new(&mut vec_output, config.clone(), &registry);
        writer.write_line(test_text).unwrap();
    }
    let vec_result = String::from_utf8(vec_output).unwrap();
    assert!(!vec_result.contains("multi-sink-secret"));

    // Both outputs should be redacted identically
    assert!(redacted_string.contains("****"));
    assert!(vec_result.contains("****"));
}

#[test]
fn test_cryptographic_hash_security_properties() {
    use sha2::{Digest, Sha256};

    // Test that hashes are cryptographically secure and not reversible
    let secret = "security-test-secret";

    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    // Hash should be 64 characters (256 bits in hex)
    assert_eq!(hash.len(), 64);

    // Hash should be alphanumeric hex
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));

    // Hash should not contain the original secret
    assert!(!hash.contains(secret));

    // Hash should be deterministic
    let mut hasher2 = Sha256::new();
    hasher2.update(secret.as_bytes());
    let hash2 = format!("{:x}", hasher2.finalize());
    assert_eq!(hash, hash2);

    // Different input should produce different hash
    let mut hasher3 = Sha256::new();
    hasher3.update("different-secret".as_bytes());
    let hash3 = format!("{:x}", hasher3.finalize());
    assert_ne!(hash, hash3);
}
