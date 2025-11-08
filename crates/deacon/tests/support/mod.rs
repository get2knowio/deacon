//! Shared test utilities for deacon CLI tests.

#![allow(dead_code)]

/// Helper to skip tests that require network access.
///
/// Tests that make network requests should use this as a guard at the beginning:
/// if the environment variable `DEACON_NETWORK_TESTS` is not set, the function
/// prints a message and returns `true` (test should skip), otherwise returns `false`.
///
/// # Usage
/// ```ignore
/// #[test]
/// fn test_with_network() {
///     if skip_if_no_network_tests() {
///         return;
///     }
///     // ... test code that requires network
/// }
/// ```
pub fn skip_if_no_network_tests() -> bool {
    if std::env::var("DEACON_NETWORK_TESTS").is_err() {
        eprintln!("Skipping network test - set DEACON_NETWORK_TESTS=1 to enable");
        return true;
    }
    false
}

/// Helper function to extract JSON from mixed output (logs + JSON).
///
/// When running CLI tests with logging enabled, the output may contain log lines
/// before the JSON output. This helper skips log lines and extracts valid JSON.
pub fn extract_json_from_output(output: &str) -> Result<serde_json::Value, serde_json::Error> {
    // Try to find JSON by looking for complete JSON objects
    // Skip lines that look like log messages (contain timestamp patterns)
    for line in output.lines() {
        let trimmed = line.trim();
        // Skip lines that contain log timestamps or ANSI codes
        if trimmed.contains("Z ") || trimmed.contains("\x1b[") || trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            if let Ok(json) = serde_json::from_str(trimmed) {
                return Ok(json);
            }
        }
    }

    // If that doesn't work, try to extract everything after the last log line
    let lines: Vec<&str> = output.lines().collect();
    for i in (0..lines.len()).rev() {
        let line = lines[i].trim();
        if line.starts_with('{') {
            // Collect all lines from this point onwards and try to parse as JSON
            let json_part = lines[i..].join("\n");
            if let Ok(json) = serde_json::from_str(&json_part) {
                return Ok(json);
            }
        }
    }

    // Last resort - try the whole output
    serde_json::from_str(output)
}
