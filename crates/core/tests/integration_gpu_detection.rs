//! Integration tests for GPU detection functionality
//!
//! These tests verify that GPU detection works correctly with real Docker runtime.

use deacon_core::gpu::{detect_gpu_capability, HostGpuCapability};

/// Test GPU detection with Docker runtime.
///
/// This test verifies that GPU detection:
/// - Succeeds or fails gracefully
/// - Returns a valid HostGpuCapability structure
/// - Either finds nvidia runtime or returns unavailable/probe_failed
///
/// This test requires Docker to be available and running.
#[tokio::test]
async fn test_detect_gpu_capability_with_docker() {
    let result = detect_gpu_capability("docker").await;

    // The result should be one of three valid states:
    // 1. GPU available with runtime_name set
    // 2. GPU unavailable (available=false, no error)
    // 3. Probe failed (available=false, with probe_error set)

    if result.available {
        // If available, runtime_name should be set
        assert!(result.runtime_name.is_some());
        assert!(result.probe_error.is_none());
        println!(
            "GPU detection succeeded: found runtime '{}'",
            result.runtime_name.unwrap()
        );
    } else if let Some(error) = result.probe_error {
        // If probe failed, we should have an error message
        assert!(!result.available);
        assert!(result.runtime_name.is_none());
        println!(
            "GPU detection failed (expected if Docker daemon not running): {}",
            error
        );
    } else {
        // GPU not available but detection succeeded
        assert!(!result.available);
        assert!(result.runtime_name.is_none());
        println!("GPU detection succeeded: no GPU runtime found");
    }
}

/// Test that GPU detection handles missing Docker gracefully.
#[tokio::test]
async fn test_detect_gpu_capability_missing_runtime() {
    let result = detect_gpu_capability("nonexistent-docker-xyz").await;

    assert!(!result.available);
    assert!(result.runtime_name.is_none());
    assert!(result.probe_error.is_some());

    let error = result.probe_error.unwrap();
    assert!(error.contains("Failed to execute runtime info command"));
}

/// Test JSON serialization of detection results.
#[test]
fn test_gpu_capability_serialization() {
    // Test available case
    let available = HostGpuCapability::available("nvidia");
    let json = serde_json::to_string(&available).unwrap();
    assert!(json.contains("\"available\":true"));
    assert!(json.contains("\"runtimeName\":\"nvidia\""));
    assert!(!json.contains("probeError"));

    // Test unavailable case
    let unavailable = HostGpuCapability::unavailable();
    let json = serde_json::to_string(&unavailable).unwrap();
    assert!(json.contains("\"available\":false"));
    assert!(!json.contains("runtimeName"));
    assert!(!json.contains("probeError"));

    // Test probe failed case
    let failed = HostGpuCapability::probe_failed("test error");
    let json = serde_json::to_string(&failed).unwrap();
    assert!(json.contains("\"available\":false"));
    assert!(json.contains("\"probeError\":\"test error\""));
    assert!(!json.contains("runtimeName"));
}

/// Test GPU detection via CliRuntime convenience method.
#[tokio::test]
async fn test_cli_runtime_detect_gpu_capability() {
    use deacon_core::docker::CliRuntime;

    // Test with Docker runtime
    let docker = CliRuntime::docker();
    let result = docker.detect_gpu_capability().await;

    // Should succeed or fail gracefully
    if result.available {
        assert!(result.runtime_name.is_some());
        assert!(result.probe_error.is_none());
        println!(
            "GPU detection via CliRuntime succeeded: found runtime '{}'",
            result.runtime_name.unwrap()
        );
    } else if let Some(error) = result.probe_error {
        assert!(result.runtime_name.is_none());
        println!("GPU detection via CliRuntime failed: {}", error);
    } else {
        assert!(result.runtime_name.is_none());
        println!("GPU detection via CliRuntime: no GPU runtime found");
    }
}
