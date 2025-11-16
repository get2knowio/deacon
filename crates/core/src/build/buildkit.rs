//! BuildKit capability detection and validation
//!
//! This module provides helpers to detect BuildKit availability and flag
//! operations that require BuildKit execution.

use crate::errors::Result;
use std::process::Command;
use tracing::{debug, instrument};

/// Detects if BuildKit is available on the host.
///
/// Uses `docker buildx version` to determine BuildKit availability.
///
/// # Returns
///
/// * `Ok(true)` - BuildKit is available
/// * `Ok(false)` - BuildKit is not available
/// * `Err(_)` - Failed to detect BuildKit (should be treated as unavailable)
#[instrument]
pub fn is_buildkit_available() -> Result<bool> {
    debug!("Checking BuildKit availability");

    let output = Command::new("docker").args(["buildx", "version"]).output();

    match output {
        Ok(output) => {
            let available = output.status.success();
            debug!("BuildKit available: {}", available);
            Ok(available)
        }
        Err(e) => {
            debug!("Failed to check BuildKit: {}", e);
            Ok(false)
        }
    }
}

/// Validates that BuildKit is available for the given operation.
///
/// # Arguments
///
/// * `operation` - The operation name that requires BuildKit (e.g., "--push", "--output")
///
/// # Errors
///
/// Returns a validation error with a spec-compliant message if BuildKit is not available.
#[instrument]
pub fn require_buildkit(operation: &str) -> Result<()> {
    if !is_buildkit_available()? {
        return Err(crate::errors::DeaconError::Runtime(format!(
            "BuildKit is required for {}. Enable BuildKit or remove {} flag.",
            operation, operation
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_buildkit_available() {
        // This test will pass regardless of BuildKit availability
        // The function should not panic
        let _ = is_buildkit_available();
    }
}
