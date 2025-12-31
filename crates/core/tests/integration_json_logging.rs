//! Integration tests for JSON logging format validation
//!
//! These tests verify that the JSON logging output follows the expected schema
//! and includes all required fields for structured logging.

use deacon_core::logging;

/// Test logging initialization with different formats
#[test]
fn test_logging_initialization_formats() {
    // Test JSON format
    assert!(
        logging::init(Some("json")).is_ok(),
        "JSON format initialization should succeed"
    );

    // Test text format
    assert!(
        logging::init(Some("text")).is_ok(),
        "Text format initialization should succeed"
    );

    // Test default (None)
    assert!(
        logging::init(None).is_ok(),
        "Default format initialization should succeed"
    );

    // Test invalid format (should default to text)
    assert!(
        logging::init(Some("invalid")).is_ok(),
        "Invalid format should default gracefully"
    );
}

/// Test the JSON logging output using our CLI binary
#[test]
fn test_json_logging_output_schema() {
    use assert_cmd::Command;
    use serde_json::Value;

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .args(["--log-format", "json", "--log-level", "debug", "--help"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Parse each line of stderr as JSON
    for line in stderr.lines() {
        if line.trim().is_empty() {
            continue;
        }

        // Each line should be valid JSON
        let json: Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("Failed to parse JSON line: {}\nError: {}", line, e));

        // Verify required fields according to our schema
        assert!(
            json["timestamp"].is_string(),
            "timestamp field should be a string"
        );
        assert!(json["level"].is_string(), "level field should be a string");
        assert!(
            json["target"].is_string(),
            "target field should be a string"
        );
        assert!(json["fields"].is_object(), "fields should be an object");
        assert!(
            json["fields"]["message"].is_string(),
            "message should be present in fields"
        );

        // Verify timestamp format (ISO 8601)
        let timestamp = json["timestamp"].as_str().unwrap();
        assert!(
            timestamp.contains('T'),
            "timestamp should be in ISO 8601 format"
        );
        assert!(timestamp.contains('Z'), "timestamp should include timezone");

        // Verify level is valid
        let level = json["level"].as_str().unwrap();
        assert!(
            matches!(level, "TRACE" | "DEBUG" | "INFO" | "WARN" | "ERROR"),
            "level should be a valid log level, got: {}",
            level
        );

        // If span information is present, validate it
        if let Some(span) = json.get("span") {
            assert!(span["name"].is_string(), "span name should be a string");
        }

        if let Some(spans) = json.get("spans") {
            assert!(spans.is_array(), "spans should be an array");
        }
    }
}

/// Test text logging format produces readable output
#[test]
fn test_text_logging_output_format() {
    use assert_cmd::Command;

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .args(["--log-format", "text", "--log-level", "info", "--help"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Either stderr should have log output, or the command should succeed
    let has_output = !stderr.is_empty() || !stdout.is_empty();
    assert!(has_output, "Should have some output (stdout or stderr)");

    // If there are log lines in stderr, verify timestamp format
    let lines: Vec<&str> = stderr
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if !lines.is_empty() {
        if let Some(first_line) = lines.first() {
            // Should start with a timestamp if it's a log line
            if first_line.starts_with("20") {
                assert!(
                    first_line.contains("T"),
                    "Should contain ISO 8601 T separator"
                );
                assert!(
                    first_line.contains("Z"),
                    "Should contain timezone indicator"
                );
            }
        }
    }
}

/// Test that JSON format initialization works correctly
#[test]
fn test_json_format_initialization() {
    // This test verifies that JSON format can be initialized without errors
    // Additional JSON structure testing is done in other tests
    assert!(
        logging::init(Some("json")).is_ok(),
        "JSON format initialization should succeed"
    );
}

/// Test environment variable support for log level
#[test]
fn test_environment_variable_support() {
    use assert_cmd::Command;

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let output = cmd
        .env("RUST_LOG", "debug")
        .args(["--log-format", "json"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should have log output - the CLI always produces some logging
    assert!(
        !stderr.is_empty() || !output.stdout.is_empty(),
        "Should have some output with RUST_LOG set"
    );

    // If there's stderr output, verify JSON structure is maintained
    for line in stderr.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let _: serde_json::Value = serde_json::from_str(line).unwrap_or_else(|e| {
            panic!(
                "Failed to parse JSON line with RUST_LOG: {}\nError: {}",
                line, e
            )
        });
    }
}
