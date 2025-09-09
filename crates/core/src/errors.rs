//! Error types and handling
//!
//! This module provides domain-specific error types following the CLI specification.
//! The error taxonomy is structured with specific error enums for each domain
//! (Configuration, Docker, Feature, etc.) that are then wrapped in the main
//! DeaconError enum for unified error handling.

use thiserror::Error;

/// Configuration-related errors
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Configuration file parsing error
    #[error("Failed to parse configuration file: {message}")]
    Parsing { message: String },

    /// Configuration validation error
    #[error("Configuration validation error: {message}")]
    Validation { message: String },

    /// Cycle detected in extends chain
    #[error("Cycle detected in extends chain: {chain}")]
    ExtendsCycle { chain: String },

    /// Feature not implemented
    #[error("Feature not implemented: {feature}")]
    NotImplemented { feature: String },

    /// Configuration file I/O error
    #[error("Failed to read configuration file")]
    Io(#[from] std::io::Error),

    /// Configuration file not found
    #[error("Configuration file not found: {path}")]
    NotFound { path: String },
}

/// Docker/Runtime-related errors (placeholder implementations)
#[derive(Error, Debug)]
pub enum DockerError {
    /// Docker is not installed or not accessible
    #[error("Docker is not installed or not accessible")]
    NotInstalled,

    /// Docker CLI command error
    #[error("Docker CLI error: {0}")]
    CLIError(String),
}

/// Feature-related errors
#[derive(Error, Debug)]
pub enum FeatureError {
    /// Feature not implemented
    #[error("Feature not implemented")]
    NotImplemented,

    /// Feature metadata parsing error
    #[error("Failed to parse feature metadata: {message}")]
    Parsing { message: String },

    /// Feature metadata validation error
    #[error("Feature validation error: {message}")]
    Validation { message: String },

    /// Feature metadata file I/O error
    #[error("Failed to read feature metadata file")]
    Io(#[from] std::io::Error),

    /// Feature metadata file not found
    #[error("Feature metadata file not found: {path}")]
    NotFound { path: String },

    /// JSON parsing error
    #[error("JSON parsing error")]
    Json(#[from] serde_json::Error),

    /// OCI registry error
    #[error("OCI registry error: {message}")]
    Oci { message: String },

    /// Feature download error
    #[error("Feature download error: {message}")]
    Download { message: String },

    /// Feature extraction error  
    #[error("Feature extraction error: {message}")]
    Extraction { message: String },

    /// Feature installation error
    #[error("Feature installation error: {message}")]
    Installation { message: String },
}

/// Internal/generic fallback errors
#[derive(Error, Debug)]
pub enum InternalError {
    /// Generic internal error
    #[error("Internal error: {message}")]
    Generic { message: String },

    /// Unexpected error condition
    #[error("Unexpected error: {message}")]
    Unexpected { message: String },
}

/// Main error enum wrapping all domain-specific errors
#[derive(Error, Debug)]
pub enum DeaconError {
    /// Configuration-related errors
    #[error("Configuration error")]
    Config(#[from] ConfigError),

    /// Docker/Runtime-related errors
    #[error("Docker error")]
    Docker(#[from] DockerError),

    /// Feature-related errors
    #[error("Feature error")]
    Feature(#[from] FeatureError),

    /// Template-related errors
    #[error("Template error: {message}")]
    Template { message: String },

    /// Network-related errors
    #[error("Network error: {message}")]
    Network { message: String },

    /// Authentication errors
    #[error("Authentication error: {message}")]
    Authentication { message: String },

    /// Lifecycle command execution errors
    #[error("Lifecycle error: {0}")]
    Lifecycle(String),

    /// Internal/generic errors
    #[error("Internal error")]
    Internal(#[from] InternalError),
}

/// Convenience type alias for Results with DeaconError
pub type Result<T> = std::result::Result<T, DeaconError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_config_error_display() {
        let error = ConfigError::Parsing {
            message: "Invalid JSON".to_string(),
        };
        assert_eq!(
            format!("{}", error),
            "Failed to parse configuration file: Invalid JSON"
        );

        let error = ConfigError::Validation {
            message: "Missing required field".to_string(),
        };
        assert_eq!(
            format!("{}", error),
            "Configuration validation error: Missing required field"
        );

        let error = ConfigError::NotImplemented {
            feature: "extends".to_string(),
        };
        assert_eq!(format!("{}", error), "Feature not implemented: extends");

        let error = ConfigError::NotFound {
            path: "/path/to/file".to_string(),
        };
        assert_eq!(
            format!("{}", error),
            "Configuration file not found: /path/to/file"
        );
    }

    #[test]
    fn test_docker_error_display() {
        let error = DockerError::NotInstalled;
        assert_eq!(
            format!("{}", error),
            "Docker is not installed or not accessible"
        );

        let error = DockerError::CLIError("Command failed".to_string());
        assert_eq!(format!("{}", error), "Docker CLI error: Command failed");
    }

    #[test]
    fn test_feature_error_display() {
        let error = FeatureError::NotImplemented;
        assert_eq!(format!("{}", error), "Feature not implemented");
    }

    #[test]
    fn test_internal_error_display() {
        let error = InternalError::Generic {
            message: "Something went wrong".to_string(),
        };
        assert_eq!(format!("{}", error), "Internal error: Something went wrong");

        let error = InternalError::Unexpected {
            message: "Unexpected condition".to_string(),
        };
        assert_eq!(
            format!("{}", error),
            "Unexpected error: Unexpected condition"
        );
    }

    #[test]
    fn test_deacon_error_from_domain_errors() {
        let config_error = ConfigError::Parsing {
            message: "Test".to_string(),
        };
        let deacon_error: DeaconError = config_error.into();
        assert!(matches!(deacon_error, DeaconError::Config(_)));

        let docker_error = DockerError::NotInstalled;
        let deacon_error: DeaconError = docker_error.into();
        assert!(matches!(deacon_error, DeaconError::Docker(_)));

        let feature_error = FeatureError::NotImplemented;
        let deacon_error: DeaconError = feature_error.into();
        assert!(matches!(deacon_error, DeaconError::Feature(_)));

        let internal_error = InternalError::Generic {
            message: "Test".to_string(),
        };
        let deacon_error: DeaconError = internal_error.into();
        assert!(matches!(deacon_error, DeaconError::Internal(_)));
    }

    #[test]
    fn test_anyhow_conversions() {
        let config_error = ConfigError::Parsing {
            message: "Test".to_string(),
        };
        // thiserror automatically provides the conversion
        let anyhow_error = anyhow::Error::from(config_error);
        assert!(anyhow_error
            .to_string()
            .contains("Failed to parse configuration file"));

        let deacon_error = DeaconError::Docker(DockerError::NotInstalled);
        let anyhow_error = anyhow::Error::from(deacon_error);
        assert!(anyhow_error.to_string().contains("Docker error"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let config_error: ConfigError = io_error.into();
        assert!(matches!(config_error, ConfigError::Io(_)));
    }

    #[test]
    fn test_error_source_chain() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let config_error = ConfigError::Io(io_error);
        let deacon_error = DeaconError::Config(config_error);

        // Test that the source chain is preserved
        assert!(deacon_error.source().is_some());
        if let Some(source) = deacon_error.source() {
            assert!(source.source().is_some()); // The underlying io::Error
        }
    }
}
