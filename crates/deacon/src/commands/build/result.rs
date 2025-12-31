//! Build command result types
//!
//! This module defines the spec-compliant JSON output structures for `deacon build`.
//! These types conform to the contract defined in `build-cli-contract.yaml`.

use serde::{Deserialize, Serialize};

/// Successful build result conforming to CLI contract.
///
/// This struct represents the JSON payload emitted on stdout when a build succeeds.
/// It matches the `BuildSuccess` schema in the contract.
///
/// # JSON Schema
///
/// ```json
/// {
///   "outcome": "success",
///   "imageName": "myimage:latest" | ["myimage:latest", "myimage:v1.0"],
///   "exportPath": "/path/to/export.tar",  // optional
///   "pushed": true  // optional
/// }
/// ```
#[allow(dead_code)] // Used in Phase 3 implementation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildSuccess {
    /// Always "success" for successful builds
    outcome: String,

    /// Deterministic fallback tag or list of tags when multiple provided.
    /// Can be either a single string or an array of strings.
    #[serde(skip_serializing_if = "Option::is_none")]
    image_name: Option<ImageNameOutput>,

    /// Destination written when `--output` is used.
    #[serde(skip_serializing_if = "Option::is_none")]
    export_path: Option<String>,

    /// Indicates registry push was attempted and succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pushed: Option<bool>,
}

impl BuildSuccess {
    /// Creates a new successful build result with a single image name.
    #[allow(dead_code)] // Used in Phase 3 implementation
    pub fn new_single(image_name: String) -> Self {
        Self {
            outcome: "success".to_string(),
            image_name: Some(ImageNameOutput::Single(image_name)),
            export_path: None,
            pushed: None,
        }
    }

    /// Creates a new successful build result with multiple image names.
    pub fn new_multiple(image_names: Vec<String>) -> Self {
        Self {
            outcome: "success".to_string(),
            image_name: Some(ImageNameOutput::Multiple(image_names)),
            export_path: None,
            pushed: None,
        }
    }

    /// Sets the export path for the build result.
    #[allow(dead_code)] // Used in Phase 4 for --output support
    pub fn with_export_path(mut self, path: String) -> Self {
        self.export_path = Some(path);
        self
    }

    /// Sets the pushed flag for the build result.
    #[allow(dead_code)] // Used in Phase 4 for --push support
    pub fn with_pushed(mut self, pushed: bool) -> Self {
        self.pushed = Some(pushed);
        self
    }

    /// Returns the outcome field.
    #[allow(dead_code)] // Public API accessor
    pub fn outcome(&self) -> &str {
        &self.outcome
    }

    /// Returns the image name, if any.
    #[allow(dead_code)] // Public API accessor
    pub fn image_name(&self) -> Option<&ImageNameOutput> {
        self.image_name.as_ref()
    }

    /// Returns the export path, if any.
    #[allow(dead_code)] // Public API accessor
    pub fn export_path(&self) -> Option<&str> {
        self.export_path.as_deref()
    }

    /// Returns whether the image was pushed.
    #[allow(dead_code)] // Public API accessor
    pub fn pushed(&self) -> Option<bool> {
        self.pushed
    }
}

impl Default for BuildSuccess {
    fn default() -> Self {
        Self {
            outcome: "success".to_string(),
            image_name: None,
            export_path: None,
            pushed: None,
        }
    }
}

/// Image name output can be either a single string or an array of strings.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ImageNameOutput {
    /// Single image name
    Single(String),
    /// Multiple image names
    Multiple(Vec<String>),
}

/// Build error result conforming to CLI contract.
///
/// This struct represents the JSON payload emitted on stdout when a build fails.
/// It matches the `BuildError` schema in the contract.
///
/// # JSON Schema
///
/// ```json
/// {
///   "outcome": "error",
///   "message": "BuildKit is required for --push",
///   "description": "Enable BuildKit or remove --push flag"  // optional
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildError {
    /// Always "error" for failed builds
    outcome: String,

    /// Short validation or failure message matching spec text
    message: String,

    /// Additional context for the error
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

impl BuildError {
    /// Creates a new build error with just a message.
    #[allow(dead_code)] // Alternative constructor for simple errors
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            outcome: "error".to_string(),
            message: message.into(),
            description: None,
        }
    }

    /// Creates a new build error with message and description.
    pub fn with_description(message: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            outcome: "error".to_string(),
            message: message.into(),
            description: Some(description.into()),
        }
    }

    /// Returns the outcome field.
    #[allow(dead_code)] // Public API accessor
    pub fn outcome(&self) -> &str {
        &self.outcome
    }

    /// Returns the error message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the error description, if any.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(desc) = &self.description {
            write!(f, ": {}", desc)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_success_single_serialization() {
        let result = BuildSuccess::new_single("myimage:latest".to_string());
        let json = serde_json::to_string(&result).unwrap();

        assert!(json.contains(r#""outcome":"success"#));
        assert!(json.contains(r#""imageName":"myimage:latest"#));
    }

    #[test]
    fn test_build_success_multiple_serialization() {
        let result = BuildSuccess::new_multiple(vec![
            "myimage:latest".to_string(),
            "myimage:v1.0".to_string(),
        ]);
        let json = serde_json::to_string(&result).unwrap();

        assert!(json.contains(r#""outcome":"success"#));
        assert!(json.contains(r#""imageName":["myimage:latest","myimage:v1.0"]"#));
    }

    #[test]
    fn test_build_success_with_export() {
        let result = BuildSuccess::new_single("myimage:latest".to_string())
            .with_export_path("/tmp/export.tar".to_string());
        let json = serde_json::to_string(&result).unwrap();

        assert!(json.contains(r#""exportPath":"/tmp/export.tar"#));
    }

    #[test]
    fn test_build_success_with_pushed() {
        let result = BuildSuccess::new_single("myimage:latest".to_string()).with_pushed(true);
        let json = serde_json::to_string(&result).unwrap();

        assert!(json.contains(r#""pushed":true"#));
    }

    #[test]
    fn test_build_error_serialization() {
        let error = BuildError::new("BuildKit is required for --push");
        let json = serde_json::to_string(&error).unwrap();

        assert!(json.contains(r#""outcome":"error"#));
        assert!(json.contains(r#""message":"BuildKit is required for --push"#));
        assert!(!json.contains("description"));
    }

    #[test]
    fn test_build_error_with_description() {
        let error = BuildError::with_description(
            "BuildKit is required for --push",
            "Enable BuildKit or remove --push flag",
        );
        let json = serde_json::to_string(&error).unwrap();

        assert!(json.contains(r#""outcome":"error"#));
        assert!(json.contains(r#""message":"BuildKit is required for --push"#));
        assert!(json.contains(r#""description":"Enable BuildKit or remove --push flag"#));
    }

    #[test]
    fn test_build_success_deserialization() {
        let json = r#"{"outcome":"success","imageName":"test:latest"}"#;
        let result: BuildSuccess = serde_json::from_str(json).unwrap();

        assert_eq!(result.outcome, "success");
        assert_eq!(
            result.image_name,
            Some(ImageNameOutput::Single("test:latest".to_string()))
        );
    }

    #[test]
    fn test_build_error_deserialization() {
        let json = r#"{"outcome":"error","message":"Test error"}"#;
        let error: BuildError = serde_json::from_str(json).unwrap();

        assert_eq!(error.outcome, "error");
        assert_eq!(error.message, "Test error");
        assert_eq!(error.description, None);
    }
}
