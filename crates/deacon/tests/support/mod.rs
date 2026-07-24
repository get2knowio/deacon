//! Shared test utilities for deacon CLI tests.

#![allow(dead_code)]

use assert_cmd::Command;
use std::sync::OnceLock;
use tempfile::TempDir;

/// Process-global isolated temp home, created once per test process.
///
/// deacon's workspace-state cache lives at `std::env::temp_dir()/deacon-state/`
/// — a path shared across every process. Its on-disk `index.json` is a single
/// read-modify-write file, so two concurrent `deacon` invocations (e.g. two
/// parallel `up`/`down` tests) can clobber each other's index entry
/// (last-writer-wins), making a later `down` "lose" the state its `up` saved.
/// See `crates/core/src/state.rs::default_cache_dir`.
static ISOLATED_TMP: OnceLock<TempDir> = OnceLock::new();

fn isolated_tmp_home() -> &'static std::path::Path {
    ISOLATED_TMP
        .get_or_init(|| TempDir::new().expect("failed to create isolated temp home for tests"))
        .path()
}

/// Build a `deacon` CLI command with an isolated temp home, so its
/// workspace-state cache can't collide with other test processes.
///
/// Prefer this over `Command::cargo_bin("deacon")` in any test that runs
/// `up`/`down`/`exec` (or otherwise touches workspace state). Under nextest each
/// test runs in its own process, so the [`OnceLock`] home is unique per test;
/// an `up` and its later `down` within the same test share it (as they must).
/// The redirect is via `TMPDIR` (Unix) plus `TMP`/`TEMP` (Windows), which is
/// what `std::env::temp_dir()` honors on each platform.
pub fn deacon_command() -> Command {
    let home = isolated_tmp_home();
    let mut cmd = Command::cargo_bin("deacon").expect("deacon binary should build");
    cmd.env("TMPDIR", home);
    cmd.env("TMP", home);
    cmd.env("TEMP", home);
    cmd
}

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

/// The container runtime binary under test, honoring `DEACON_CONTAINER_RUNTIME`
/// (the same env var deacon reads). Defaults to `docker`.
///
/// Tests that create/query containers or images directly (bypassing deacon)
/// MUST use this rather than hardcoding `"docker"`, otherwise they set up state
/// in docker while deacon-under-podman looks in podman's separate store — the
/// container/image is invisible and the test fails spuriously.
pub fn runtime_bin() -> String {
    std::env::var("DEACON_CONTAINER_RUNTIME").unwrap_or_else(|_| "docker".to_string())
}

/// Whether the active runtime ([`runtime_bin`]) is reachable.
pub fn is_runtime_available() -> bool {
    std::process::Command::new(runtime_bin())
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Whether the active runtime is docker (not podman). testcontainers-rs is a
/// docker-API client and cannot drive rootless podman without a podman socket,
/// so testcontainers-based tests skip when this is false.
pub fn runtime_is_docker() -> bool {
    runtime_bin() == "docker"
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
