//! Error types and handling
//!
//! This module provides domain-specific error types following the CLI specification.
//! The error taxonomy is structured with specific error enums for each domain
//! (Configuration, Docker, Feature, etc.) that are then wrapped in the main
//! DeaconError enum for unified error handling.

use std::path::PathBuf;
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

    /// Extends chain exceeded maximum allowed depth
    #[error("Extends chain too deep (max {max}): {chain}")]
    ExtendsTooDeep { max: usize, chain: String },

    /// Feature not implemented
    #[error("Feature not implemented: {feature}")]
    NotImplemented { feature: String },

    /// Configuration file I/O error
    #[error("Failed to read configuration file")]
    Io(#[from] std::io::Error),

    /// Multiple configuration files found — user must select one
    #[error("Multiple devcontainer configurations found. Use --config to specify one:\n{}",
        paths.iter().map(|p| format!("  {}", p)).collect::<Vec<_>>().join("\n"))]
    MultipleConfigs { paths: Vec<String> },

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

    /// Container not found
    #[error("Container not found: {id}")]
    ContainerNotFound { id: String },

    /// Command execution failed
    #[error("Command execution failed with exit code {code}")]
    ExecFailed { code: i32 },

    /// TTY allocation failed
    #[error("TTY allocation failed: {reason}")]
    TTYFailed { reason: String },
}

/// Git-related errors
#[derive(Error, Debug)]
pub enum GitError {
    /// Git is not installed or not accessible
    #[error("Git is not installed or not accessible")]
    NotInstalled,

    /// Git CLI command error
    #[error("Git CLI error: {0}")]
    CLIError(String),

    /// Repository clone failed
    #[error("Failed to clone repository: {0}")]
    CloneFailed(String),

    /// Invalid repository URL
    #[error("Invalid repository URL: {0}")]
    InvalidUrl(String),
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

    /// Content integrity verification failed
    ///
    /// Raised when downloaded bytes (a manifest or layer blob) do not hash to
    /// the digest declared in the manifest, a digest-pinned reference, or the
    /// lockfile. Fail closed: never trust content that fails this check.
    #[error("Integrity verification failed for {context}: expected {expected}, computed {actual}")]
    IntegrityMismatch {
        context: String,
        expected: String,
        actual: String,
    },

    /// Feature installation error (generic installation failure)
    #[error("Feature installation error: {message}")]
    Installation { message: String },

    /// Feature installation failed for a specific feature (per-feature failure reporting)
    ///
    /// This variant is used when a specific feature installation fails and provides
    /// the feature ID for better error reporting and debugging. Use this instead of
    /// `Installation` when you have context about which specific feature failed.
    #[error("Feature installation failed for {feature_id}: {message}")]
    InstallationFailed { feature_id: String, message: String },

    /// Feature dependency cycle detected
    #[error("Dependency cycle detected in features: {cycle_path}")]
    DependencyCycle { cycle_path: String },

    /// Invalid dependency reference
    #[error("Invalid dependency reference: {message}")]
    InvalidDependency { message: String },

    /// Feature dependency resolution error
    #[error("Feature dependency resolution error: {message}")]
    DependencyResolution { message: String },

    /// Authentication error (HTTP 401 Unauthorized)
    #[error("Authentication failed: {message}")]
    Unauthorized { message: String },

    /// Authorization error (HTTP 403 Forbidden)
    #[error("Authorization denied: {message}")]
    Forbidden { message: String },

    /// Generic authentication/authorization error (for backward compatibility)
    #[error("Authentication error: {message}")]
    Authentication { message: String },
}

/// Template-related errors
#[derive(Error, Debug)]
pub enum TemplateError {
    /// Template not implemented
    #[error("Template not implemented")]
    NotImplemented,

    /// Template metadata parsing error
    #[error("Failed to parse template metadata: {message}")]
    Parsing { message: String },

    /// Template metadata validation error
    #[error("Template validation error: {message}")]
    Validation { message: String },

    /// Template metadata file I/O error
    #[error("Failed to read template metadata file")]
    Io(#[from] std::io::Error),

    /// Template metadata file not found
    #[error("Template metadata file not found: {path}")]
    NotFound { path: String },

    /// JSON parsing error
    #[error("JSON parsing error")]
    Json(#[from] serde_json::Error),

    /// Template application error
    #[error("Template application error: {message}")]
    Application { message: String },

    /// File operation error during template application
    #[error("Template file operation error: {message}")]
    FileOperation { message: String },
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

/// Lockfile-related errors
#[derive(Error, Debug)]
pub enum LockfileError {
    /// I/O error reading or writing a lockfile
    #[error("Lockfile I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// JSON parsing or serialization error
    #[error("Lockfile JSON error at {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    /// Lockfile already exists and force_init=false
    #[error("Lockfile already exists at {path}. Use force_init=true to overwrite.")]
    AlreadyExists { path: PathBuf },

    /// Lockfile contents fail validation (semver, OCI ref, sha256, deps)
    #[error("Lockfile validation failed: {message}")]
    Validation { message: String },

    /// Dependency cycle detected in `dependsOn`
    #[error("Circular dependency detected in depends_on fields: {cycle_path}")]
    DependencyCycle { cycle_path: String },
}

/// Cache-related errors
#[derive(Error, Debug)]
pub enum CacheError {
    /// I/O error during cache operations
    #[error("Cache I/O error: {message}")]
    Io {
        message: String,
        #[source]
        source: std::io::Error,
    },

    /// Serialization/deserialization error for cache payloads
    #[error("Cache serialization error: {message}")]
    Serialization { message: String },

    /// Entry size exceeds the cache's maximum
    #[error("Cache entry size ({size} bytes) exceeds maximum size ({max} bytes)")]
    EntryTooLarge { size: usize, max: usize },
}

/// Workspace state/marker errors
#[derive(Error, Debug)]
pub enum StateError {
    /// I/O error while reading or writing workspace state / phase markers
    #[error("State I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Failed to serialize state to JSON
    #[error("Failed to serialize state for {kind}: {source}")]
    Serialize {
        kind: String,
        #[source]
        source: serde_json::Error,
    },

    /// Underlying cache operation failed
    #[error("State cache error: {0}")]
    Cache(#[from] CacheError),

    /// Could not resolve the host user-data folder that roots lifecycle markers
    #[error("Could not resolve user-data folder for lifecycle markers: {message}")]
    UserDataFolder { message: String },
}

/// Output / stdout-stderr separation errors
#[derive(Error, Debug)]
pub enum IoError {
    /// Failed to write to the underlying stream
    #[error("Output write error: {0}")]
    Write(#[from] std::io::Error),

    /// Failed to serialize a value to JSON for output
    #[error("Output JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Progress / metrics / audit logging errors
#[derive(Error, Debug)]
pub enum ProgressError {
    /// I/O error while writing progress events or audit log
    #[error("Progress I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Failed to serialize a progress event
    #[error("Progress event serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Container selector / lookup errors
#[derive(Error, Debug)]
pub enum ContainerSelectorError {
    /// id-label string did not match `name=value`
    #[error("Unmatched argument format: id-label must match <name>=<value>.")]
    InvalidLabelFormat,

    /// At least one selector is required (container_id, id_labels, or workspace_folder)
    #[error(
        "Missing required argument: One of --container-id, --id-label or --workspace-folder is required."
    )]
    NoSelector,
}

/// Main error enum wrapping all domain-specific errors
#[derive(Error, Debug)]
pub enum DeaconError {
    /// Configuration-related errors
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    /// Docker/Runtime-related errors
    #[error("Docker error: {0}")]
    Docker(#[from] DockerError),

    /// Git-related errors
    #[error("Git error: {0}")]
    Git(#[from] GitError),

    /// Feature-related errors
    #[error("Feature error: {0}")]
    Feature(#[from] FeatureError),

    /// Template-related errors
    #[error("Template error: {0}")]
    Template(#[from] TemplateError),

    /// Network-related errors
    #[error("Network error: {message}")]
    Network { message: String },

    /// Authentication errors
    #[error("Authentication error: {message}")]
    Authentication { message: String },

    /// Lifecycle command execution errors
    #[error("Lifecycle error: {0}")]
    Lifecycle(String),

    /// Container runtime errors (Docker, Podman, etc.)
    #[error("Runtime error: {0}")]
    Runtime(String),

    /// Workspace is not trusted; host-side lifecycle hooks were refused.
    ///
    /// See `crates/core/src/trust.rs` and the `--trust-workspace[-persist]`
    /// CLI surface for opt-in mechanics.
    #[error("Workspace `{workspace}` is not trusted: {reason}\n{instructions}")]
    WorkspaceUntrusted {
        /// Workspace path that failed the check.
        workspace: std::path::PathBuf,
        /// Short reason intended for logs.
        reason: String,
        /// User-facing opt-in instructions.
        instructions: String,
    },

    /// Feature not implemented
    #[error("Feature not implemented: {feature}")]
    NotImplemented { feature: String },

    /// Internal/generic errors
    #[error("Internal error: {0}")]
    Internal(#[from] InternalError),

    /// Lockfile errors
    #[error("Lockfile error: {0}")]
    Lockfile(#[from] LockfileError),

    /// Cache errors
    #[error("Cache error: {0}")]
    Cache(#[from] CacheError),

    /// Workspace state errors
    #[error("State error: {0}")]
    State(#[from] StateError),

    /// Output stream errors
    #[error("Output error: {0}")]
    Output(#[from] IoError),

    /// Progress/audit errors
    #[error("Progress error: {0}")]
    Progress(#[from] ProgressError),

    /// Container selector errors
    #[error("Container selector error: {0}")]
    ContainerSelector(#[from] ContainerSelectorError),
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

        let error = ConfigError::MultipleConfigs {
            paths: vec![
                ".devcontainer/node/devcontainer.json".to_string(),
                ".devcontainer/python/devcontainer.json".to_string(),
            ],
        };
        assert_eq!(
            format!("{}", error),
            "Multiple devcontainer configurations found. Use --config to specify one:\n  .devcontainer/node/devcontainer.json\n  .devcontainer/python/devcontainer.json"
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
    fn test_git_error_display() {
        let error = GitError::NotInstalled;
        assert_eq!(
            format!("{}", error),
            "Git is not installed or not accessible"
        );

        let error = GitError::CLIError("Command failed".to_string());
        assert_eq!(format!("{}", error), "Git CLI error: Command failed");

        let error = GitError::CloneFailed("Permission denied".to_string());
        assert_eq!(
            format!("{}", error),
            "Failed to clone repository: Permission denied"
        );

        let error = GitError::InvalidUrl("not-a-url".to_string());
        assert_eq!(format!("{}", error), "Invalid repository URL: not-a-url");
    }

    #[test]
    fn test_feature_error_display() {
        let error = FeatureError::NotImplemented;
        assert_eq!(format!("{}", error), "Feature not implemented");
    }

    #[test]
    fn test_template_error_display() {
        let error = TemplateError::NotImplemented;
        assert_eq!(format!("{}", error), "Template not implemented");

        let error = TemplateError::Parsing {
            message: "Invalid JSON".to_string(),
        };
        assert_eq!(
            format!("{}", error),
            "Failed to parse template metadata: Invalid JSON"
        );

        let error = TemplateError::Validation {
            message: "Missing required field".to_string(),
        };
        assert_eq!(
            format!("{}", error),
            "Template validation error: Missing required field"
        );

        let error = TemplateError::NotFound {
            path: "/path/to/template.json".to_string(),
        };
        assert_eq!(
            format!("{}", error),
            "Template metadata file not found: /path/to/template.json"
        );
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

        let git_error = GitError::NotInstalled;
        let deacon_error: DeaconError = git_error.into();
        assert!(matches!(deacon_error, DeaconError::Git(_)));

        let feature_error = FeatureError::NotImplemented;
        let deacon_error: DeaconError = feature_error.into();
        assert!(matches!(deacon_error, DeaconError::Feature(_)));

        let template_error = TemplateError::NotImplemented;
        let deacon_error: DeaconError = template_error.into();
        assert!(matches!(deacon_error, DeaconError::Template(_)));

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
        assert!(
            anyhow_error
                .to_string()
                .contains("Failed to parse configuration file")
        );

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
    fn test_lockfile_error_variants() {
        let path = PathBuf::from("/tmp/lock.json");

        let err = LockfileError::AlreadyExists { path: path.clone() };
        assert!(err.to_string().contains("Lockfile already exists"));
        assert!(err.to_string().contains("/tmp/lock.json"));

        let err = LockfileError::Validation {
            message: "invalid integrity".to_string(),
        };
        assert!(err.to_string().contains("invalid integrity"));

        let err = LockfileError::DependencyCycle {
            cycle_path: "a -> b -> a".to_string(),
        };
        assert!(err.to_string().contains("Circular dependency"));
        assert!(err.to_string().contains("a -> b -> a"));

        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let err = LockfileError::Io {
            path: path.clone(),
            source: io_err,
        };
        assert!(err.to_string().contains("Lockfile I/O error"));
        assert!(err.source().is_some());
    }

    #[test]
    fn test_cache_error_variants() {
        let err = CacheError::EntryTooLarge {
            size: 4096,
            max: 1024,
        };
        assert!(err.to_string().contains("4096"));
        assert!(err.to_string().contains("1024"));

        let err = CacheError::Serialization {
            message: "bad value".to_string(),
        };
        assert!(err.to_string().contains("bad value"));
    }

    #[test]
    fn test_state_error_variants() {
        let path = PathBuf::from("/tmp/state.json");
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err = StateError::Io {
            path: path.clone(),
            source: io_err,
        };
        assert!(err.to_string().contains("State I/O error"));
        assert!(err.to_string().contains("/tmp/state.json"));
        assert!(err.source().is_some());
    }

    #[test]
    fn test_io_error_variants() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken");
        let err: IoError = io_err.into();
        assert!(err.to_string().contains("Output write error"));
    }

    #[test]
    fn test_progress_error_variants() {
        let io_err = std::io::Error::other("noisy");
        let err: ProgressError = io_err.into();
        assert!(err.to_string().contains("Progress I/O error"));
    }

    #[test]
    fn test_container_selector_error_variants() {
        let err = ContainerSelectorError::InvalidLabelFormat;
        assert!(err.to_string().contains("id-label must match"));

        let err = ContainerSelectorError::NoSelector;
        assert!(err.to_string().contains("Missing required argument"));
    }

    #[test]
    fn test_new_errors_into_deacon_error() {
        let de: DeaconError = LockfileError::Validation {
            message: "x".into(),
        }
        .into();
        assert!(matches!(de, DeaconError::Lockfile(_)));

        let de: DeaconError = CacheError::EntryTooLarge { size: 1, max: 0 }.into();
        assert!(matches!(de, DeaconError::Cache(_)));

        let de: DeaconError = StateError::Serialize {
            kind: "ContainerState".into(),
            source: serde_json::from_str::<serde_json::Value>("not json").unwrap_err(),
        }
        .into();
        assert!(matches!(de, DeaconError::State(_)));

        let de: DeaconError = IoError::from(std::io::Error::other("oops")).into();
        assert!(matches!(de, DeaconError::Output(_)));

        let de: DeaconError = ProgressError::from(std::io::Error::other("oops")).into();
        assert!(matches!(de, DeaconError::Progress(_)));

        let de: DeaconError = ContainerSelectorError::NoSelector.into();
        assert!(matches!(de, DeaconError::ContainerSelector(_)));
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
