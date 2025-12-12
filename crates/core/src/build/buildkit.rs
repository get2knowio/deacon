//! BuildKit capability detection and validation
//!
//! This module provides helpers to detect BuildKit availability and flag
//! operations that require BuildKit execution.
//!
//! ## Usage
//!
//! Use [`BuildKitOptions`] to track which CLI options require BuildKit support,
//! then call [`require_buildkit_for_options`] to fail-fast if BuildKit is unavailable:
//!
//! ```no_run
//! use deacon_core::build::buildkit::{BuildKitOptions, require_buildkit_for_options};
//!
//! let options = BuildKitOptions {
//!     cache_from: vec!["type=registry,ref=myregistry.io/cache:latest".to_string()],
//!     cache_to: vec![],
//!     builder: None,
//! };
//!
//! // This will fail with a clear error if BuildKit is not available
//! require_buildkit_for_options(&options)?;
//! # Ok::<(), deacon_core::errors::DeaconError>(())
//! ```

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

/// Tracks CLI options that require BuildKit support.
///
/// This struct captures the build options that depend on BuildKit/buildx functionality.
/// Use [`requires_buildkit`](Self::requires_buildkit) to check if any options need BuildKit,
/// and [`buildkit_required_options`](Self::buildkit_required_options) to get a list of
/// which specific options require it.
///
/// # Example
///
/// ```
/// use deacon_core::build::buildkit::BuildKitOptions;
///
/// let options = BuildKitOptions {
///     cache_from: vec!["type=registry,ref=cache:latest".to_string()],
///     cache_to: vec![],
///     builder: Some("mybuilder".to_string()),
/// };
///
/// assert!(options.requires_buildkit());
/// let required = options.buildkit_required_options();
/// assert!(required.contains(&"--cache-from"));
/// assert!(required.contains(&"--builder"));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildKitOptions {
    /// Cache sources for BuildKit builds (--cache-from)
    pub cache_from: Vec<String>,

    /// Cache destinations for BuildKit builds (--cache-to)
    pub cache_to: Vec<String>,

    /// BuildKit builder instance name (--builder)
    pub builder: Option<String>,
}

impl BuildKitOptions {
    /// Returns `true` if any of the options require BuildKit support.
    ///
    /// # Example
    ///
    /// ```
    /// use deacon_core::build::buildkit::BuildKitOptions;
    ///
    /// let empty = BuildKitOptions::default();
    /// assert!(!empty.requires_buildkit());
    ///
    /// let with_cache = BuildKitOptions {
    ///     cache_from: vec!["type=registry,ref=cache".to_string()],
    ///     ..Default::default()
    /// };
    /// assert!(with_cache.requires_buildkit());
    /// ```
    pub fn requires_buildkit(&self) -> bool {
        !self.cache_from.is_empty() || !self.cache_to.is_empty() || self.builder.is_some()
    }

    /// Returns a list of option names that require BuildKit support.
    ///
    /// This is useful for error messages that need to tell the user which
    /// specific options triggered the BuildKit requirement.
    ///
    /// # Example
    ///
    /// ```
    /// use deacon_core::build::buildkit::BuildKitOptions;
    ///
    /// let options = BuildKitOptions {
    ///     cache_from: vec!["type=registry,ref=cache".to_string()],
    ///     cache_to: vec!["type=local,dest=/tmp".to_string()],
    ///     builder: None,
    /// };
    ///
    /// let required = options.buildkit_required_options();
    /// assert_eq!(required.len(), 2);
    /// assert!(required.contains(&"--cache-from"));
    /// assert!(required.contains(&"--cache-to"));
    /// ```
    pub fn buildkit_required_options(&self) -> Vec<&'static str> {
        let mut options = Vec::new();

        if !self.cache_from.is_empty() {
            options.push("--cache-from");
        }
        if !self.cache_to.is_empty() {
            options.push("--cache-to");
        }
        if self.builder.is_some() {
            options.push("--builder");
        }

        options
    }
}

/// Validates that BuildKit is available when required by the given options.
///
/// This function implements fail-fast behavior per spec edge case:
/// "Unsupported or conflicting build options (e.g., buildx requested when BuildKit is
/// unavailable) must fail fast with an actionable error before builds start."
///
/// # Arguments
///
/// * `options` - The build options that may require BuildKit support
///
/// # Errors
///
/// Returns a validation error with a spec-compliant message listing which options
/// require BuildKit if any require it and BuildKit is not available.
///
/// # Example
///
/// ```no_run
/// use deacon_core::build::buildkit::{BuildKitOptions, require_buildkit_for_options};
///
/// let options = BuildKitOptions {
///     cache_from: vec!["type=registry,ref=cache".to_string()],
///     cache_to: vec!["type=local,dest=/tmp".to_string()],
///     builder: Some("mybuilder".to_string()),
/// };
///
/// // Will fail if BuildKit is not available, with message like:
/// // "BuildKit is required for --cache-from, --cache-to, --builder. Enable BuildKit or remove these flags."
/// require_buildkit_for_options(&options)?;
/// # Ok::<(), deacon_core::errors::DeaconError>(())
/// ```
#[instrument(skip(options), fields(requires_buildkit = options.requires_buildkit()))]
pub fn require_buildkit_for_options(options: &BuildKitOptions) -> Result<()> {
    // If no options require BuildKit, validation passes immediately
    if !options.requires_buildkit() {
        debug!("No BuildKit-requiring options present, skipping availability check");
        return Ok(());
    }

    // Check BuildKit availability
    if !is_buildkit_available()? {
        let required_options = options.buildkit_required_options();
        let options_list = required_options.join(", ");

        return Err(crate::errors::DeaconError::Runtime(format!(
            "BuildKit is required for {}. Enable BuildKit or remove these flags.",
            options_list
        )));
    }

    debug!("BuildKit is available for requested options");
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

    // ==========================================================================
    // BuildKitOptions tests
    // ==========================================================================

    #[test]
    fn buildkit_options_default_does_not_require_buildkit() {
        let options = BuildKitOptions::default();

        assert!(!options.requires_buildkit());
        assert!(options.buildkit_required_options().is_empty());
    }

    #[test]
    fn buildkit_options_requires_buildkit_for_cache_from() {
        let options = BuildKitOptions {
            cache_from: vec!["type=registry,ref=test".to_string()],
            cache_to: vec![],
            builder: None,
        };

        assert!(options.requires_buildkit());
        assert_eq!(options.buildkit_required_options(), vec!["--cache-from"]);
    }

    #[test]
    fn buildkit_options_requires_buildkit_for_cache_to() {
        let options = BuildKitOptions {
            cache_from: vec![],
            cache_to: vec!["type=local,dest=/tmp".to_string()],
            builder: None,
        };

        assert!(options.requires_buildkit());
        assert_eq!(options.buildkit_required_options(), vec!["--cache-to"]);
    }

    #[test]
    fn buildkit_options_requires_buildkit_for_builder() {
        let options = BuildKitOptions {
            cache_from: vec![],
            cache_to: vec![],
            builder: Some("mybuilder".to_string()),
        };

        assert!(options.requires_buildkit());
        assert_eq!(options.buildkit_required_options(), vec!["--builder"]);
    }

    #[test]
    fn buildkit_options_requires_buildkit_for_combined_options() {
        let options = BuildKitOptions {
            cache_from: vec!["type=registry,ref=from".to_string()],
            cache_to: vec!["type=registry,ref=to".to_string()],
            builder: Some("mybuilder".to_string()),
        };

        assert!(options.requires_buildkit());

        let required = options.buildkit_required_options();
        assert_eq!(required.len(), 3);
        assert!(required.contains(&"--cache-from"));
        assert!(required.contains(&"--cache-to"));
        assert!(required.contains(&"--builder"));
    }

    #[test]
    fn buildkit_options_multiple_cache_entries_still_one_option() {
        let options = BuildKitOptions {
            cache_from: vec![
                "type=registry,ref=cache1".to_string(),
                "type=registry,ref=cache2".to_string(),
                "type=local,src=/tmp".to_string(),
            ],
            cache_to: vec![
                "type=registry,ref=output1".to_string(),
                "type=registry,ref=output2".to_string(),
            ],
            builder: None,
        };

        assert!(options.requires_buildkit());

        // Multiple entries in cache_from/cache_to still count as single options
        let required = options.buildkit_required_options();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&"--cache-from"));
        assert!(required.contains(&"--cache-to"));
    }

    // ==========================================================================
    // require_buildkit_for_options tests
    // ==========================================================================

    #[test]
    fn require_buildkit_for_options_passes_when_empty() {
        let options = BuildKitOptions::default();

        // Should always pass when no BuildKit-requiring options are set
        // (does not even check BuildKit availability)
        let result = require_buildkit_for_options(&options);
        assert!(result.is_ok());
    }

    #[test]
    fn require_buildkit_for_options_preserves_option_order() {
        // When BuildKit is not available and options require it,
        // the error message should list options in consistent order
        let options = BuildKitOptions {
            cache_from: vec!["test".to_string()],
            cache_to: vec!["test".to_string()],
            builder: Some("test".to_string()),
        };

        let required = options.buildkit_required_options();
        // Order should be: cache_from, cache_to, builder
        assert_eq!(required[0], "--cache-from");
        assert_eq!(required[1], "--cache-to");
        assert_eq!(required[2], "--builder");
    }
}
