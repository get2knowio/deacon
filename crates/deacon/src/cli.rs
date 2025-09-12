use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Output format options
#[derive(Debug, Clone, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable text format
    Text,
    /// JSON structured format
    Json,
}

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
    /// Whether secret redaction is disabled
    pub no_redact: bool,
    /// Enabled plugins
    #[cfg(feature = "plugins")]
    pub plugins: Vec<String>,
}

/// DevContainer CLI subcommands
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Create and run development container
    Up {
        /// Remove existing container(s) first
        #[arg(long)]
        remove_existing_container: bool,
        /// Skip postCreate lifecycle phase
        #[arg(long)]
        skip_post_create: bool,
        /// Skip non-blocking commands (postStart & postAttach phases)
        #[arg(long)]
        skip_non_blocking_commands: bool,
        /// Emit machine-readable port events to stdout with PORT_EVENT prefix
        #[arg(long)]
        ports_events: bool,
    },

    /// Build development container image
    Build {
        /// Build without cache
        #[arg(long)]
        no_cache: bool,
        /// Target platform for build (e.g. linux/amd64)
        #[arg(long)]
        platform: Option<String>,
        /// Build argument in key=value format
        #[arg(long)]
        build_arg: Vec<String>,
        /// Force rebuild even if cache is valid
        #[arg(long)]
        force: bool,
        /// Output format (text or json)
        #[arg(long, value_enum, default_value = "text")]
        output_format: OutputFormat,
    },

    /// Execute command in running container
    Exec {
        /// User to run the command as
        #[arg(long)]
        user: Option<String>,
        /// Disable TTY allocation
        #[arg(long)]
        no_tty: bool,
        /// Environment variables to set (KEY=VALUE format)
        #[arg(long, action = clap::ArgAction::Append)]
        env: Vec<String>,
        /// Command to execute
        command: Vec<String>,
    },

    /// Read and display configuration
    ReadConfiguration {
        /// Include merged configuration
        #[arg(long)]
        include_merged_configuration: bool,
    },

    /// Feature management commands
    Features {
        /// Feature subcommand
        #[command(subcommand)]
        command: FeatureCommands,
    },

    /// Template management commands
    Templates {
        /// Template subcommand
        #[command(subcommand)]
        command: TemplateCommands,
    },

    /// Run user-defined commands
    #[allow(clippy::enum_variant_names)]
    RunUserCommands {
        /// Commands to run
        commands: Vec<String>,
    },

    /// Environment diagnostics and support bundle creation
    ///
    /// Collects system information for troubleshooting and support
    Doctor {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Create support bundle at specified path
        #[arg(long)]
        bundle: Option<PathBuf>,
    },
}

/// Feature management subcommands
#[derive(Debug, Subcommand)]
pub enum FeatureCommands {
    /// Test feature implementations
    Test { target: Option<String> },
    /// Package features for distribution
    Package { target: String },
    /// Publish features to registry
    Publish { target: String },
    /// Get feature information
    Info { mode: String, feature: String },
}

/// Template management subcommands
#[derive(Debug, Subcommand)]
pub enum TemplateCommands {
    /// Apply template to current project
    Apply { template: String },
    /// Publish templates to registry
    Publish { target: String },
    /// Get template metadata
    Metadata { template_id: String },
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

    /// Disable secret redaction in output (debugging only - WARNING: may expose secrets)
    #[arg(long, global = true)]
    pub no_redact: bool,

    /// Enable specific plugins
    #[cfg(feature = "plugins")]
    #[arg(long, global = true, value_name = "NAME")]
    pub plugin: Vec<String>,

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
            no_redact: self.no_redact,
            #[cfg(feature = "plugins")]
            plugins: self.plugin.clone(),
        }
    }

    pub async fn dispatch(self) -> Result<()> {
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

        // Warn if redaction is disabled
        if self.no_redact {
            tracing::warn!("Secret redaction is DISABLED via --no-redact flag. This may expose sensitive information in logs and output. Use only for debugging purposes!");
        }

        match self.command {
            Some(Commands::Up {
                remove_existing_container,
                skip_post_create,
                skip_non_blocking_commands,
                ports_events,
            }) => {
                use crate::commands::up::{execute_up, UpArgs};

                let args = UpArgs {
                    remove_existing_container,
                    skip_post_create,
                    skip_non_blocking_commands,
                    ports_events,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                };

                match execute_up(args).await {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        if let Some(DeaconError::Config(ConfigError::NotFound { .. })) =
                            e.downcast_ref::<DeaconError>()
                        {
                            // Match legacy CLI message expected by tests
                            Err(anyhow::anyhow!("No devcontainer.json found in workspace"))
                        } else {
                            Err(e)
                        }
                    }
                }
            }
            Some(Commands::Build {
                no_cache,
                platform,
                build_arg,
                force,
                output_format,
            }) => {
                use crate::commands::build::{execute_build, BuildArgs};

                let args = BuildArgs {
                    no_cache,
                    platform,
                    build_arg,
                    force,
                    output_format,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                };

                execute_build(args).await?;
                Ok(())
            }
            Some(Commands::Exec {
                user,
                no_tty,
                env,
                command,
            }) => {
                use crate::commands::exec::{execute_exec, ExecArgs};

                let args = ExecArgs {
                    user,
                    no_tty,
                    env,
                    command,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                };

                execute_exec(args).await
            }
            Some(Commands::ReadConfiguration {
                include_merged_configuration,
            }) => {
                use crate::commands::read_configuration::{
                    execute_read_configuration, ReadConfigurationArgs,
                };

                let args = ReadConfigurationArgs {
                    include_merged_configuration,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                };

                execute_read_configuration(args).await?;
                Ok(())
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
            Some(Commands::Doctor { json, bundle }) => {
                // Create a DoctorContext for doctor command
                let context = deacon_core::doctor::DoctorContext {
                    workspace_folder: self.workspace_folder.clone(),
                    config: self.config.clone(),
                };

                // Execute doctor command
                match deacon_core::doctor::run_doctor(json, bundle, context).await {
                    Ok(()) => Ok(()),
                    Err(e) => Err(e.into()),
                }
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
