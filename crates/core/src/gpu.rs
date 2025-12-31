//! GPU mode handling for devcontainer operations
//!
//! This module provides types for managing GPU resource requests and detection
//! during devcontainer lifecycle operations, particularly for the `up` command.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use tracing::{debug, instrument};

/// Represents how GPU requests should be handled during devcontainer operations.
///
/// This enum controls whether GPU resources are requested when creating containers,
/// with support for automatic detection of host GPU capabilities.
///
/// # CLI Integration
///
/// When using this type in CLI arguments, implement `clap::ValueEnum` in the binary crate
/// to enable parsing from command-line strings. The `FromStr` implementation here provides
/// the underlying parsing logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum GpuMode {
    /// Always request GPU resources regardless of host capabilities.
    All,
    /// Probe host GPU capability; request if available, else warn and skip.
    Detect,
    /// Never request GPU resources (default behavior).
    #[default]
    None,
}

impl fmt::Display for GpuMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GpuMode::All => write!(f, "all"),
            GpuMode::Detect => write!(f, "detect"),
            GpuMode::None => write!(f, "none"),
        }
    }
}

impl FromStr for GpuMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "all" => Ok(GpuMode::All),
            "detect" => Ok(GpuMode::Detect),
            "none" => Ok(GpuMode::None),
            _ => Err(format!(
                "Invalid GPU mode: '{}'. Valid values are: all, detect, none",
                s
            )),
        }
    }
}

/// Result of probing the host system for GPU capabilities.
///
/// This struct captures whether a GPU-capable runtime was detected on the host,
/// along with optional diagnostic information about the runtime or any probe failures.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostGpuCapability {
    /// Whether a GPU-capable runtime is detected on the host.
    pub available: bool,

    /// Name of the detected GPU runtime (e.g., "nvidia", "amd").
    ///
    /// This field is only present when `available` is true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_name: Option<String>,

    /// Warning detail if GPU detection fails.
    ///
    /// This field is mutually exclusive with `available == true` and provides
    /// diagnostic information when probing encounters errors but execution can continue.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probe_error: Option<String>,
}

impl HostGpuCapability {
    /// Create a new capability indicating GPU is available with the given runtime.
    pub fn available(runtime_name: impl Into<String>) -> Self {
        Self {
            available: true,
            runtime_name: Some(runtime_name.into()),
            probe_error: None,
        }
    }

    /// Create a new capability indicating GPU is not available.
    pub fn unavailable() -> Self {
        Self {
            available: false,
            runtime_name: None,
            probe_error: None,
        }
    }

    /// Create a new capability indicating probe failed with the given error.
    pub fn probe_failed(error: impl Into<String>) -> Self {
        Self {
            available: false,
            runtime_name: None,
            probe_error: Some(error.into()),
        }
    }
}

/// Tracks how a selected GPU mode is applied during a single operation.
///
/// This struct captures the application of GPU resource requests across different
/// execution paths (run, build, compose) and whether warnings were emitted for
/// unavailable GPU capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpGpuApplication {
    /// The GPU mode selected for this operation (user-specified or defaulted).
    pub selected_mode: GpuMode,

    /// Whether GPU requests were applied to docker run operations.
    pub applies_to_run: bool,

    /// Whether GPU requests were applied to docker build operations.
    pub applies_to_build: bool,

    /// Whether GPU requests were applied to compose service invocations.
    pub applies_to_compose: bool,

    /// Whether a warning was emitted for missing GPU capabilities in detect mode.
    pub warning_emitted: bool,
}

impl UpGpuApplication {
    /// Create a new GPU application tracker with the given mode.
    pub fn new(selected_mode: GpuMode) -> Self {
        Self {
            selected_mode,
            applies_to_run: false,
            applies_to_build: false,
            applies_to_compose: false,
            warning_emitted: false,
        }
    }
}

/// Detect GPU capability on the host by checking for GPU-capable runtimes.
///
/// This function queries the Docker daemon for available runtimes and checks
/// if GPU-capable runtimes (such as `nvidia`) are present.
///
/// # Arguments
/// * `runtime_path` - Path to the container runtime binary (e.g., "docker" or "podman")
///
/// # Returns
/// * `HostGpuCapability` indicating whether GPU support is available, unavailable,
///   or if detection failed
///
/// # Best-Effort Detection
/// This detection is best-effort and will not fail the overall operation:
/// - If the runtime command fails, returns `HostGpuCapability::probe_failed()`
/// - If no GPU runtime is found, returns `HostGpuCapability::unavailable()`
/// - If a GPU runtime is found, returns `HostGpuCapability::available(runtime_name)`
#[instrument(skip(runtime_path))]
pub async fn detect_gpu_capability(runtime_path: &str) -> HostGpuCapability {
    debug!("Detecting GPU capability using runtime: {}", runtime_path);

    // Execute 'docker info --format {{json .Runtimes}}' to get runtime information
    let output = tokio::process::Command::new(runtime_path)
        .arg("info")
        .arg("--format")
        .arg("{{json .Runtimes}}")
        .output()
        .await;

    match output {
        Ok(output) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let error_msg =
                    format!("Failed to query container runtime info: {}", stderr.trim());
                debug!("{}", error_msg);
                return HostGpuCapability::probe_failed(error_msg);
            }

            // Parse the JSON output
            let stdout = match String::from_utf8(output.stdout) {
                Ok(s) => s,
                Err(e) => {
                    let error_msg = format!("Invalid UTF-8 in runtime output: {}", e);
                    debug!("{}", error_msg);
                    return HostGpuCapability::probe_failed(error_msg);
                }
            };

            // Parse JSON to check for GPU runtimes
            match serde_json::from_str::<serde_json::Value>(&stdout) {
                Ok(runtimes) => {
                    // Check for nvidia runtime
                    if let Some(obj) = runtimes.as_object() {
                        if obj.contains_key("nvidia") {
                            debug!("Found nvidia runtime");
                            return HostGpuCapability::available("nvidia");
                        }
                        // Could check for other GPU runtimes here in the future
                        // (e.g., "amd", "intel")
                    }

                    debug!("No GPU-capable runtime found");
                    HostGpuCapability::unavailable()
                }
                Err(e) => {
                    let error_msg = format!("Failed to parse runtime info JSON: {}", e);
                    debug!("{}", error_msg);
                    HostGpuCapability::probe_failed(error_msg)
                }
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to execute runtime info command: {}", e);
            debug!("{}", error_msg);
            HostGpuCapability::probe_failed(error_msg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_mode_default() {
        assert_eq!(GpuMode::default(), GpuMode::None);
    }

    #[test]
    fn test_gpu_mode_display() {
        assert_eq!(GpuMode::All.to_string(), "all");
        assert_eq!(GpuMode::Detect.to_string(), "detect");
        assert_eq!(GpuMode::None.to_string(), "none");
    }

    #[test]
    fn test_gpu_mode_from_str() {
        assert_eq!("all".parse::<GpuMode>().unwrap(), GpuMode::All);
        assert_eq!("detect".parse::<GpuMode>().unwrap(), GpuMode::Detect);
        assert_eq!("none".parse::<GpuMode>().unwrap(), GpuMode::None);
        assert_eq!("ALL".parse::<GpuMode>().unwrap(), GpuMode::All);
        assert_eq!("Detect".parse::<GpuMode>().unwrap(), GpuMode::Detect);
        assert!("invalid".parse::<GpuMode>().is_err());
    }

    #[test]
    fn test_gpu_mode_serde() {
        let mode = GpuMode::Detect;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, r#""detect""#);
        let parsed: GpuMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, mode);
    }

    #[test]
    fn test_host_gpu_capability_available() {
        let cap = HostGpuCapability::available("nvidia");
        assert!(cap.available);
        assert_eq!(cap.runtime_name, Some("nvidia".to_string()));
        assert_eq!(cap.probe_error, None);
    }

    #[test]
    fn test_host_gpu_capability_unavailable() {
        let cap = HostGpuCapability::unavailable();
        assert!(!cap.available);
        assert_eq!(cap.runtime_name, None);
        assert_eq!(cap.probe_error, None);
    }

    #[test]
    fn test_host_gpu_capability_probe_failed() {
        let cap = HostGpuCapability::probe_failed("runtime error");
        assert!(!cap.available);
        assert_eq!(cap.runtime_name, None);
        assert_eq!(cap.probe_error, Some("runtime error".to_string()));
    }

    #[test]
    fn test_host_gpu_capability_serde() {
        let cap = HostGpuCapability::available("nvidia");
        let json = serde_json::to_string(&cap).unwrap();
        let parsed: HostGpuCapability = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.available, cap.available);
        assert_eq!(parsed.runtime_name, cap.runtime_name);

        // Test that None fields are omitted
        let cap_unavailable = HostGpuCapability::unavailable();
        let json = serde_json::to_string(&cap_unavailable).unwrap();
        assert!(!json.contains("runtimeName"));
        assert!(!json.contains("probeError"));
    }

    #[test]
    fn test_up_gpu_application_new() {
        let app = UpGpuApplication::new(GpuMode::Detect);
        assert_eq!(app.selected_mode, GpuMode::Detect);
        assert!(!app.applies_to_run);
        assert!(!app.applies_to_build);
        assert!(!app.applies_to_compose);
        assert!(!app.warning_emitted);
    }

    #[test]
    fn test_up_gpu_application_default() {
        let app = UpGpuApplication::default();
        assert_eq!(app.selected_mode, GpuMode::None);
    }

    #[tokio::test]
    async fn test_detect_gpu_capability_with_invalid_runtime() {
        // Test with a non-existent runtime binary
        let result = detect_gpu_capability("nonexistent-docker-binary-xyz").await;
        assert!(!result.available);
        assert!(result.probe_error.is_some());
        assert!(result
            .probe_error
            .unwrap()
            .contains("Failed to execute runtime info command"));
    }

    // Note: Integration tests with real Docker commands should be in
    // crates/core/tests/gpu_detection_integration.rs or similar
}
