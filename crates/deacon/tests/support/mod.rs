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

/// Generate a unique, docker-safe resource name with a prefix.
///
/// Incorporates nextest slot info when available so parallel runs don’t
/// collide across test processes, then adds PID + timestamp as a final tie-breaker.
pub fn unique_name(prefix: &str) -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let slot = std::env::var("NEXTEST_GLOBAL_SLOT").ok();
    let group = std::env::var("NEXTEST_TEST_GROUP").ok();
    let mut suffix = String::new();
    if let Some(slot) = slot {
        suffix.push_str("-slot");
        suffix.push_str(&slot);
    }
    if let Some(group) = group {
        let sanitized: String = group
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        suffix.push_str("-group-");
        suffix.push_str(&sanitized);
    }

    format!("{}{}-{}-{}", prefix, suffix, pid, nanos)
}
