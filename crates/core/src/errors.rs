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