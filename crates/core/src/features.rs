//! DevContainer features system
//!
//! This module handles feature discovery, installation, and lifecycle management.

use crate::errors::{FeatureError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, instrument};

/// Processed option value supporting different types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OptionValue {
    Boolean(bool),
    String(String),
}

impl OptionValue {
    /// Get as boolean if it's a boolean value
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            OptionValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// Get as string if it's a string value
    pub fn as_str(&self) -> Option<&str> {
        match self {
            OptionValue::String(s) => Some(s),
            _ => None,
        }
    }
}

/// Feature option definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FeatureOption {
    #[serde(rename = "boolean")]
    Boolean {
        #[serde(default)]
        default: Option<bool>,
        #[serde(default)]
        description: Option<String>,
    },
    #[serde(rename = "string")]
    String {
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        r#enum: Option<Vec<String>>,
        #[serde(default)]
        proposals: Option<Vec<String>>,
    },
}

impl FeatureOption {
    /// Get the default value for this option
    pub fn default_value(&self) -> Option<OptionValue> {
        match self {
            FeatureOption::Boolean { default, .. } => default.map(OptionValue::Boolean),
            FeatureOption::String { default, .. } => {
                default.as_ref().map(|s| OptionValue::String(s.clone()))
            }
        }
    }

    /// Validate a value against this option definition
    pub fn validate_value(&self, value: &OptionValue) -> std::result::Result<(), String> {
        match (self, value) {
            (FeatureOption::Boolean { .. }, OptionValue::Boolean(_)) => Ok(()),
            (FeatureOption::String { r#enum, .. }, OptionValue::String(s)) => {
                if let Some(allowed_values) = r#enum {
                    if allowed_values.contains(s) {
                        Ok(())
                    } else {
                        Err(format!(
                            "Value '{}' is not one of the allowed values: {:?}",
                            s, allowed_values
                        ))
                    }
                } else {
                    Ok(())
                }
            }
            _ => Err("Type mismatch between option definition and provided value".to_string()),
        }
    }
}

/// Feature metadata structure representing devcontainer-feature.json
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureMetadata {
    /// Feature identifier (required)
    pub id: String,

    /// Feature version
    #[serde(default)]
    pub version: Option<String>,

    /// Human-readable name
    #[serde(default)]
    pub name: Option<String>,

    /// Feature description
    #[serde(default)]
    pub description: Option<String>,

    /// Documentation URL
    #[serde(default)]
    pub documentation_url: Option<String>,

    /// License URL
    #[serde(default)]
    pub license_url: Option<String>,

    /// Feature options
    #[serde(default)]
    pub options: HashMap<String, FeatureOption>,

    /// Container environment variables
    #[serde(default)]
    pub container_env: HashMap<String, String>,

    /// Container mounts
    #[serde(default)]
    pub mounts: Vec<String>,

    /// Whether to use init
    #[serde(default)]
    pub init: Option<bool>,

    /// Whether to run privileged
    #[serde(default)]
    pub privileged: Option<bool>,

    /// Capabilities to add
    #[serde(default)]
    pub cap_add: Vec<String>,

    /// Security options
    #[serde(default)]
    pub security_opt: Vec<String>,

    /// Features to install after
    #[serde(default)]
    pub installs_after: Vec<String>,

    /// Feature dependencies
    #[serde(default)]
    pub depends_on: HashMap<String, serde_json::Value>,

    /// onCreate lifecycle command
    #[serde(default)]
    pub on_create_command: Option<serde_json::Value>,

    /// updateContent lifecycle command
    #[serde(default)]
    pub update_content_command: Option<serde_json::Value>,

    /// postCreate lifecycle command
    #[serde(default)]
    pub post_create_command: Option<serde_json::Value>,

    /// postStart lifecycle command
    #[serde(default)]
    pub post_start_command: Option<serde_json::Value>,

    /// postAttach lifecycle command
    #[serde(default)]
    pub post_attach_command: Option<serde_json::Value>,
}

impl FeatureMetadata {
    /// Check if any lifecycle commands are present
    pub fn has_lifecycle_commands(&self) -> bool {
        self.on_create_command.is_some()
            || self.update_content_command.is_some()
            || self.post_create_command.is_some()
            || self.post_start_command.is_some()
            || self.post_attach_command.is_some()
    }

    /// Validate the feature metadata
    pub fn validate(&self) -> std::result::Result<(), FeatureError> {
        // Required field validation
        if self.id.is_empty() {
            return Err(FeatureError::Validation {
                message: "Feature id is required and cannot be empty".to_string(),
            });
        }

        // Validate option defaults
        for (option_name, option_def) in &self.options {
            if let Some(default_value) = option_def.default_value() {
                if let Err(err) = option_def.validate_value(&default_value) {
                    return Err(FeatureError::Validation {
                        message: format!(
                            "Default value for option '{}' is invalid: {}",
                            option_name, err
                        ),
                    });
                }
            }
        }

        Ok(())
    }
}

/// Parse feature metadata from a devcontainer-feature.json file
#[instrument(level = "debug")]
pub fn parse_feature_metadata(path: &Path) -> Result<FeatureMetadata> {
    debug!("Parsing feature metadata from: {}", path.display());

    // Check if file exists
    if !path.exists() {
        return Err(FeatureError::NotFound {
            path: path.display().to_string(),
        }
        .into());
    }

    // Read file content
    let content = std::fs::read_to_string(path).map_err(FeatureError::Io)?;

    // Parse JSON
    let metadata: FeatureMetadata =
        serde_json::from_str(&content).map_err(|e| FeatureError::Parsing {
            message: e.to_string(),
        })?;

    debug!(
        "Parsed feature: id={}, name={:?}",
        metadata.id, metadata.name
    );

    // Log options
    for (option_name, option_def) in &metadata.options {
        debug!("Option '{}': {:?}", option_name, option_def);
    }

    // Log lifecycle presence
    if metadata.has_lifecycle_commands() {
        debug!("Feature has lifecycle commands");
    }

    // Validate metadata
    metadata.validate()?;

    Ok(metadata)
}

/// Placeholder for feature system
pub struct Feature;

impl Feature {
    /// Placeholder feature installer
    pub fn install() -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_option_value_conversions() {
        let bool_val = OptionValue::Boolean(true);
        assert_eq!(bool_val.as_bool(), Some(true));
        assert_eq!(bool_val.as_str(), None);

        let string_val = OptionValue::String("test".to_string());
        assert_eq!(string_val.as_bool(), None);
        assert_eq!(string_val.as_str(), Some("test"));
    }

    #[test]
    fn test_feature_option_default_values() {
        let bool_option = FeatureOption::Boolean {
            default: Some(true),
            description: None,
        };
        assert_eq!(
            bool_option.default_value(),
            Some(OptionValue::Boolean(true))
        );

        let string_option = FeatureOption::String {
            default: Some("default_value".to_string()),
            description: None,
            r#enum: None,
            proposals: None,
        };
        assert_eq!(
            string_option.default_value(),
            Some(OptionValue::String("default_value".to_string()))
        );
    }

    #[test]
    fn test_feature_option_validation() {
        let bool_option = FeatureOption::Boolean {
            default: Some(true),
            description: None,
        };
        assert!(bool_option
            .validate_value(&OptionValue::Boolean(false))
            .is_ok());
        assert!(bool_option
            .validate_value(&OptionValue::String("test".to_string()))
            .is_err());

        let enum_option = FeatureOption::String {
            default: None,
            description: None,
            r#enum: Some(vec!["value1".to_string(), "value2".to_string()]),
            proposals: None,
        };
        assert!(enum_option
            .validate_value(&OptionValue::String("value1".to_string()))
            .is_ok());
        assert!(enum_option
            .validate_value(&OptionValue::String("invalid".to_string()))
            .is_err());
    }

    #[test]
    fn test_parse_minimal_feature_metadata() {
        let minimal_feature = r#"
        {
            "id": "test-feature"
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(minimal_feature.as_bytes()).unwrap();

        let metadata = parse_feature_metadata(temp_file.path()).unwrap();
        assert_eq!(metadata.id, "test-feature");
        assert_eq!(metadata.name, None);
        assert_eq!(metadata.options.len(), 0);
        assert!(!metadata.has_lifecycle_commands());
    }

    #[test]
    fn test_parse_feature_with_options() {
        let feature_with_options = r#"
        {
            "id": "test-feature",
            "name": "Test Feature",
            "description": "A test feature",
            "options": {
                "enableFeature": {
                    "type": "boolean",
                    "default": true,
                    "description": "Enable the feature"
                },
                "version": {
                    "type": "string",
                    "enum": ["latest", "stable"],
                    "default": "stable",
                    "description": "Version to install"
                }
            },
            "onCreateCommand": "echo 'Feature installed'"
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file
            .write_all(feature_with_options.as_bytes())
            .unwrap();

        let metadata = parse_feature_metadata(temp_file.path()).unwrap();
        assert_eq!(metadata.id, "test-feature");
        assert_eq!(metadata.name, Some("Test Feature".to_string()));
        assert_eq!(metadata.options.len(), 2);
        assert!(metadata.has_lifecycle_commands());

        // Check boolean option
        let enable_option = metadata.options.get("enableFeature").unwrap();
        match enable_option {
            FeatureOption::Boolean { default, .. } => {
                assert_eq!(*default, Some(true));
            }
            _ => panic!("Expected boolean option"),
        }

        // Check string option with enum
        let version_option = metadata.options.get("version").unwrap();
        match version_option {
            FeatureOption::String {
                default, r#enum, ..
            } => {
                assert_eq!(*default, Some("stable".to_string()));
                assert_eq!(r#enum.as_ref().unwrap(), &vec!["latest", "stable"]);
            }
            _ => panic!("Expected string option"),
        }
    }

    #[test]
    fn test_parse_invalid_feature_schema() {
        let invalid_feature = r#"
        {
            "id": "",
            "options": {
                "badOption": {
                    "type": "string",
                    "enum": ["value1", "value2"],
                    "default": "invalid_default"
                }
            }
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(invalid_feature.as_bytes()).unwrap();

        let result = parse_feature_metadata(temp_file.path());
        assert!(result.is_err());

        if let Err(crate::errors::DeaconError::Feature(FeatureError::Validation { message })) =
            result
        {
            assert!(message.contains("Feature id is required"));
        } else {
            panic!("Expected validation error for empty id");
        }
    }

    #[test]
    fn test_parse_nonexistent_file() {
        let result = parse_feature_metadata(Path::new("/nonexistent/path/feature.json"));
        assert!(result.is_err());

        if let Err(crate::errors::DeaconError::Feature(FeatureError::NotFound { .. })) = result {
            // Expected
        } else {
            panic!("Expected NotFound error");
        }
    }

    #[test]
    fn test_parse_invalid_json() {
        let invalid_json = r#"
        {
            "id": "test-feature",
            "invalid": json
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(invalid_json.as_bytes()).unwrap();

        let result = parse_feature_metadata(temp_file.path());
        assert!(result.is_err());

        if let Err(crate::errors::DeaconError::Feature(FeatureError::Parsing { .. })) = result {
            // Expected
        } else {
            panic!("Expected parsing error for invalid JSON");
        }
    }
}
