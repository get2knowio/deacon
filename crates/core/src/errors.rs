//! Error types and handling
//!
//! This module provides domain-specific error types following the CLI specification.

use thiserror::Error;

/// Domain errors for the DevContainer CLI
#[derive(Error, Debug)]
pub enum DeaconError {
    /// Configuration-related errors
    #[error("Configuration error: {message}")]
    Configuration { message: String },

    /// Configuration file not found
    #[error("Configuration file not found: {path}")]
    ConfigurationNotFound { path: String },

    /// Configuration file parsing error
    #[error("Failed to parse configuration file: {message}")]
    ConfigurationParse { message: String },

    /// Configuration file I/O error
    #[error("Failed to read configuration file: {source}")]
    ConfigurationIo {
        #[from]
        source: std::io::Error,
    },

    /// Configuration validation error
    #[error("Configuration validation error: {message}")]
    ConfigurationValidation { message: String },

    /// Feature not implemented
    #[error("Feature not implemented: {feature}")]
    NotImplemented { feature: String },

    /// Docker/Runtime-related errors
    #[error("Docker runtime error: {message}")]
    Docker { message: String },

    /// Feature-related errors
    #[error("Feature error: {message}")]
    Feature { message: String },

    /// Template-related errors
    #[error("Template error: {message}")]
    Template { message: String },

    /// Network-related errors
    #[error("Network error: {message}")]
    Network { message: String },

    /// Validation errors
    #[error("Validation error: {message}")]
    Validation { message: String },

    /// Authentication errors
    #[error("Authentication error: {message}")]
    Authentication { message: String },
}

/// Convenience type alias for Results with DeaconError
pub type Result<T> = std::result::Result<T, DeaconError>;
