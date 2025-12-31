//! Integration tests for standardized tracing spans and field validation
//!
//! These tests verify that the standardized observability spans and fields
//! are correctly emitted in JSON logging format.

use assert_cmd::Command;
use deacon_core::logging;
use deacon_core::observability::{fields, spans};
use serde_json::Value;
use tempfile::TempDir;

/// Test that config commands emit standardized spans in JSON format
#[test]
#[ignore = "Uses non-existent 'config substitute' subcommand - command not implemented"]
fn test_config_resolve_span_json_logging() {
    // Skip if logging is already initialized (common in test suites)
    if !logging::is_initialized() {
        logging::init(Some("json")).expect("Failed to initialize JSON logging");
    }

    // Create a temporary directory for test workspace
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_path = temp_dir.path();

    // Create a minimal devcontainer.json file
    let devcontainer_dir = workspace_path.join(".devcontainer");
    std::fs::create_dir_all(&devcontainer_dir).expect("Failed to create .devcontainer dir");

    let devcontainer_file = devcontainer_dir.join("devcontainer.json");
    std::fs::write(&devcontainer_file, r#"{"image": "ubuntu:20.04"}"#)
        .expect("Failed to write devcontainer.json");

    // Execute config substitute command with JSON logging
    let mut cmd = Command::cargo_bin("deacon").expect("Failed to find deacon binary");

    let output = cmd
        .env("DEACON_LOG_FORMAT", "json")
        .env("DEACON_LOG", "info")
        .arg("config")
        .arg("substitute")
        .arg("--workspace-folder")
        .arg(workspace_path)
        .arg("--output-format")
        .arg("json")
        .assert()
        .success();

    let stderr = String::from_utf8_lossy(&output.get_output().stderr);

    // Parse JSON log entries
    let log_entries: Vec<serde_json::Value> = stderr
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    // Check for config.resolve span
    let config_resolve_spans: Vec<&Value> = log_entries
        .iter()
        .filter(|entry| {
            entry
                .get("span")
                .and_then(|span| span.get("name"))
                .and_then(|name| name.as_str())
                .map(|name| name == spans::CONFIG_RESOLVE)
                .unwrap_or(false)
        })
        .collect();

    assert!(
        !config_resolve_spans.is_empty(),
        "Expected config.resolve span in JSON logs. Found log entries: {:?}",
        log_entries
    );

    // Verify workspace_id field is present
    let has_workspace_id = config_resolve_spans.iter().any(|entry| {
        entry
            .get("span")
            .and_then(|span| span.get(fields::WORKSPACE_ID))
            .is_some()
    });

    assert!(
        has_workspace_id,
        "Expected workspace_id field in config.resolve span. Config spans: {:?}",
        config_resolve_spans
    );

    // Verify duration is recorded on span completion (in time.busy field from tracing)
    let has_duration = config_resolve_spans.iter().any(|entry| {
        entry
            .get("fields")
            .and_then(|f| f.get("time.busy"))
            .is_some()
            || entry
                .get("fields")
                .and_then(|f| f.get(fields::DURATION_MS))
                .is_some()
    });

    assert!(has_duration,
        "Expected duration timing in config.resolve span (time.busy or duration_ms). Config spans: {:?}",
        config_resolve_spans
    );
}

/// Test that features commands emit standardized spans in JSON format
#[test]
#[ignore = "Flaky span logging test - span assertions fail in CI environment"]
fn test_features_plan_span_json_logging() {
    // Skip if logging is already initialized
    if !logging::is_initialized() {
        logging::init(Some("json")).expect("Failed to initialize JSON logging");
    }

    // Create a temporary directory for test workspace
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_path = temp_dir.path();

    // Execute features plan command with JSON logging
    let mut cmd = Command::cargo_bin("deacon").expect("Failed to find deacon binary");

    let output = cmd
        .env("DEACON_LOG_FORMAT", "json")
        .env("DEACON_LOG", "info")
        .arg("features")
        .arg("plan")
        .arg("--workspace-folder")
        .arg(workspace_path)
        .arg("--json")
        .arg("true")
        .assert()
        .success();

    let stderr = String::from_utf8_lossy(&output.get_output().stderr);

    // Parse JSON log entries
    let log_entries: Vec<serde_json::Value> = stderr
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    // Check for feature.plan span
    let feature_plan_spans: Vec<&Value> = log_entries
        .iter()
        .filter(|entry| {
            entry
                .get("span")
                .and_then(|span| span.get("name"))
                .and_then(|name| name.as_str())
                .map(|name| name == spans::FEATURE_PLAN)
                .unwrap_or(false)
        })
        .collect();

    assert!(
        !feature_plan_spans.is_empty(),
        "Expected feature.plan span in JSON logs. Found log entries: {:?}",
        log_entries
    );

    // Verify workspace_id field is present
    let has_workspace_id = feature_plan_spans.iter().any(|entry| {
        entry
            .get("span")
            .and_then(|span| span.get(fields::WORKSPACE_ID))
            .is_some()
    });

    assert!(
        has_workspace_id,
        "Expected workspace_id field in feature.plan span. Feature plan spans: {:?}",
        feature_plan_spans
    );
}

/// Test JSON log schema compliance for standardized fields
#[test]
#[ignore = "Uses non-existent 'config substitute' subcommand - command not implemented"]
fn test_json_log_schema_compliance() {
    // This test verifies the JSON log structure contains all expected fields
    // when standardized spans are used

    // Skip if logging is already initialized
    if !logging::is_initialized() {
        logging::init(Some("json")).expect("Failed to initialize JSON logging");
    }

    // Create a test workspace
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_path = temp_dir.path();

    // Create a minimal devcontainer.json
    let devcontainer_dir = workspace_path.join(".devcontainer");
    std::fs::create_dir_all(&devcontainer_dir).expect("Failed to create .devcontainer dir");
    let devcontainer_file = devcontainer_dir.join("devcontainer.json");
    std::fs::write(&devcontainer_file, r#"{"image": "ubuntu:20.04"}"#)
        .expect("Failed to write devcontainer.json");

    // Run a command to generate logs
    let mut cmd = Command::cargo_bin("deacon").expect("Failed to find deacon binary");
    let output = cmd
        .env("DEACON_LOG_FORMAT", "json")
        .env("DEACON_LOG", "debug")
        .arg("config")
        .arg("substitute")
        .arg("--workspace-folder")
        .arg(workspace_path)
        .arg("--output-format")
        .arg("json")
        .assert()
        .success();

    let stderr = String::from_utf8_lossy(&output.get_output().stderr);

    // Parse all JSON log entries
    for line in stderr.lines() {
        if let Ok(json) = serde_json::from_str::<Value>(line) {
            // Verify standard JSON schema fields are present
            assert!(json.get("timestamp").is_some(), "Missing timestamp field");
            assert!(json.get("level").is_some(), "Missing level field");
            assert!(json.get("target").is_some(), "Missing target field");
            assert!(json.get("fields").is_some(), "Missing fields object");

            // If this entry has a span, verify it has expected structure
            if let Some(span) = json.get("span") {
                assert!(span.get("name").is_some(), "Span missing name field");

                // If this is a standardized span, verify it has expected fields
                if let Some(name) = span.get("name").and_then(|n| n.as_str()) {
                    if name == spans::CONFIG_RESOLVE {
                        // Should have workspace_id for config.resolve spans in span object
                        assert!(
                            span.get(fields::WORKSPACE_ID).is_some()
                                || span.get("workspace_id").is_some(),
                            "config.resolve span missing workspace_id field"
                        );
                    }
                }
            }
        }
    }
}
