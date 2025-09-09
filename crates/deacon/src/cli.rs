use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Log format options
#[derive(Debug, Clone, ValueEnum)]
pub enum LogFormat {
    /// Human-readable text format
    Text,
    /// JSON structured format
    Json,
}

/// Log level options
#[derive(Debug, Clone, ValueEnum)]
pub enum LogLevel {
    /// Error messages only
    Error,
    /// Warning and error messages
    Warn,
    /// Informational messages and above
    Info,
    /// Debug messages and above
    Debug,
    /// All messages including trace
    Trace,
}

/// Global options available to all subcommands
#[derive(Debug, Clone)]
#[allow(dead_code)] // Used for future command implementations
pub struct CliContext {
    /// Log format (text or json)
    pub log_format: LogFormat,
    /// Log level
    pub log_level: LogLevel,
    /// Workspace folder path
    pub workspace_folder: Option<PathBuf>,
    /// Configuration file path
    pub config: Option<PathBuf>,
}

/// DevContainer CLI subcommands
///
/// References CLI-SPEC.md sections:
/// - Container Lifecycle Management
/// - Configuration System
/// - Feature System
/// - Template System
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Create and run development container
    ///
    /// References: CLI-SPEC.md "Container Lifecycle Management"
    Up {
        /// Additional container arguments
        #[arg(long)]
        remove_existing_container: bool,
    },

    /// Build development container image
    ///
    /// References: CLI-SPEC.md "Docker Integration"
    Build {
        /// Build without cache
        #[arg(long)]
        no_cache: bool,
    },

    /// Execute command in running container
    ///
    /// References: CLI-SPEC.md "Process Management and Shell Integration"
    Exec {
        /// Command to execute
        command: Vec<String>,
    },

    /// Read and display configuration
    ///
    /// References: CLI-SPEC.md "Configuration System"
    ReadConfiguration {
        /// Include merged configuration
        #[arg(long)]
        include_merged_configuration: bool,
    },

    /// Feature management commands
    ///
    /// References: CLI-SPEC.md "Feature System"
    Features {
        /// Feature subcommand
        #[command(subcommand)]
        command: FeatureCommands,
    },

    /// Template management commands
    ///
    /// References: CLI-SPEC.md "Template System"
    Templates {
        /// Template subcommand
        #[command(subcommand)]
        command: TemplateCommands,
    },

    /// Run user-defined commands
    ///
    /// References: CLI-SPEC.md "Container Lifecycle Management"
    #[allow(clippy::enum_variant_names)]
    RunUserCommands {
        /// Commands to run
        commands: Vec<String>,
    },
}

/// Feature management subcommands
#[derive(Debug, Subcommand)]
pub enum FeatureCommands {
    /// Test feature implementations
    Test {
        /// Target to test
        target: Option<String>,
    },
    /// Package features for distribution
    Package {
        /// Target to package
        target: String,
    },
    /// Publish features to registry
    Publish {
        /// Target to publish
        target: String,
    },
    /// Get feature information
    Info {
        /// Information mode
        mode: String,
        /// Feature identifier
        feature: String,
    },
}

/// Template management subcommands
#[derive(Debug, Subcommand)]
pub enum TemplateCommands {
    /// Apply template to current project
    Apply {
        /// Template identifier
        template: String,
    },
    /// Publish templates to registry
    Publish {
        /// Target to publish
        target: String,
    },
    /// Get template metadata
    Metadata {
        /// Template identifier
        template_id: String,
    },
}

#[derive(Parser, Debug)]
#[command(
    name = env!("CARGO_PKG_NAME"),
    version,
    about = "Development container CLI",
    long_about = "Development container CLI (Rust reimplementation)\n\nImplements the Development Containers specification for creating and managing development environments."
)]
pub struct Cli {
    /// Log format (text or json)
    #[arg(long, global = true, value_enum, default_value = "text")]
    pub log_format: LogFormat,

    /// Log level
    #[arg(long, global = true, value_enum, default_value = "info")]
    pub log_level: LogLevel,

    /// Workspace folder path
    #[arg(long, global = true, value_name = "PATH")]
    pub workspace_folder: Option<PathBuf>,

    /// Configuration file path
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Option<Commands>,
}

impl Cli {
    /// Extract global options into CliContext
    #[allow(dead_code)] // For future command implementations
    pub fn context(&self) -> CliContext {
        CliContext {
            log_format: self.log_format.clone(),
            log_level: self.log_level.clone(),
            workspace_folder: self.workspace_folder.clone(),
            config: self.config.clone(),
        }
    }

    pub fn dispatch(self) -> Result<()> {
        use deacon_core::errors::{ConfigError, DeaconError};

        // Initialize logging based on global options
        let log_format = match self.log_format {
            LogFormat::Text => None,
            LogFormat::Json => Some("json".to_string()),
        };

        let log_level = match self.log_level {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        };

        // Set environment variable for log level before initializing logging
        std::env::set_var(
            "RUST_LOG",
            format!("deacon={},deacon_core={}", log_level, log_level),
        );
        deacon_core::logging::init(log_format.as_deref())?;

        // Emit a debug log to help with testing
        tracing::debug!("CLI initialized with log level: {}", log_level);

        match self.command {
            Some(Commands::Up { .. }) => {
                // Attempt Docker health check and log availability
                #[cfg(feature = "docker")]
                {
                    use deacon_core::docker::{CliDocker, Docker};

                    tracing::info!("Checking Docker availability...");
                    let docker_client = CliDocker::new();

                    // Check if docker binary is installed first
                    match docker_client.check_docker_installed() {
                        Ok(()) => {
                            // Try to ping the Docker daemon
                            let runtime = tokio::runtime::Runtime::new().map_err(|e| {
                                anyhow::anyhow!("Failed to create async runtime: {}", e)
                            })?;

                            match runtime.block_on(docker_client.ping()) {
                                Ok(()) => {
                                    tracing::info!("Docker is available and running");
                                }
                                Err(e) => {
                                    tracing::warn!("Docker daemon is not available: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Docker is not installed or not accessible: {}", e);
                        }
                    }
                }

                #[cfg(not(feature = "docker"))]
                {
                    tracing::warn!(
                        "Docker support is disabled (compiled without 'docker' feature)"
                    );
                }

                Err(DeaconError::Config(ConfigError::NotImplemented {
                    feature: "up command".to_string(),
                })
                .into())
            }
            Some(Commands::Build { .. }) => Err(DeaconError::Config(ConfigError::NotImplemented {
                feature: "build command".to_string(),
            })
            .into()),
            Some(Commands::Exec { .. }) => Err(DeaconError::Config(ConfigError::NotImplemented {
                feature: "exec command".to_string(),
            })
            .into()),
            Some(Commands::ReadConfiguration { .. }) => {
                Err(DeaconError::Config(ConfigError::NotImplemented {
                    feature: "read-configuration command".to_string(),
                })
                .into())
            }
            Some(Commands::Features { .. }) => {
                Err(DeaconError::Config(ConfigError::NotImplemented {
                    feature: "features command".to_string(),
                })
                .into())
            }
            Some(Commands::Templates { .. }) => {
                Err(DeaconError::Config(ConfigError::NotImplemented {
                    feature: "templates command".to_string(),
                })
                .into())
            }
            Some(Commands::RunUserCommands { .. }) => {
                Err(DeaconError::Config(ConfigError::NotImplemented {
                    feature: "run-user-commands command".to_string(),
                })
                .into())
            }
            None => {
                // No subcommand provided - show help-like message
                println!("Development container CLI");
                println!("Run 'deacon --help' to see available commands.");
                Ok(())
            }
        }
    }
}
