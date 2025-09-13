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

/// Progress format options
#[derive(Debug, Clone, ValueEnum)]
pub enum ProgressFormat {
    /// No progress output
    None,
    /// JSON structured progress events
    Json,
    /// Auto mode: silent unless --progress-file is set (future: TTY spinner)
    Auto,
}

impl From<ProgressFormat> for deacon_core::progress::ProgressFormat {
    /// Convert this crate's `ProgressFormat` into the corresponding
    /// `deacon_core::progress::ProgressFormat`.
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon::cli::ProgressFormat;
    /// let core: deacon_core::progress::ProgressFormat = ProgressFormat::Json.into();
    /// assert_eq!(core, deacon_core::progress::ProgressFormat::Json);
    /// ```
    fn from(format: ProgressFormat) -> Self {
        match format {
            ProgressFormat::None => deacon_core::progress::ProgressFormat::None,
            ProgressFormat::Json => deacon_core::progress::ProgressFormat::Json,
            ProgressFormat::Auto => deacon_core::progress::ProgressFormat::Auto,
        }
    }
}

/// Global options available to all subcommands
#[derive(Debug, Clone)]
#[allow(dead_code)] // Used for future command implementations
pub struct CliContext {
    /// Log format (text or json)
    pub log_format: LogFormat,
    /// Log level
    pub log_level: LogLevel,
    /// Progress format
    pub progress_format: ProgressFormat,
    /// Progress file path (for JSON output)
    pub progress_file: Option<PathBuf>,
    /// Workspace folder path
    pub workspace_folder: Option<PathBuf>,
    /// Configuration file path
    pub config: Option<PathBuf>,
    /// Override configuration file path
    pub override_config: Option<PathBuf>,
    /// Secrets file paths
    pub secrets_files: Vec<PathBuf>,
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
        /// Automatically shut down when process exits
        #[arg(long)]
        shutdown: bool,
        /// Additional features to install (JSON map of id -> value/options)
        #[arg(long)]
        additional_features: Option<String>,
        /// CLI features take precedence over config features on conflicts
        #[arg(long)]
        prefer_cli_features: bool,
        /// Override feature installation order (comma-separated list of IDs)
        #[arg(long)]
        feature_install_order: Option<String>,
        /// Ignore host requirements validation (log warnings instead of failing)
        #[arg(long)]
        ignore_host_requirements: bool,
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
        /// Additional features to install (JSON map of id -> value/options)
        #[arg(long)]
        additional_features: Option<String>,
        /// CLI features take precedence over config features on conflicts
        #[arg(long)]
        prefer_cli_features: bool,
        /// Override feature installation order (comma-separated list of IDs)
        #[arg(long)]
        feature_install_order: Option<String>,
        /// Ignore host requirements validation (log warnings instead of failing)
        #[arg(long)]
        ignore_host_requirements: bool,
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

    /// Stop and optionally remove development container or compose project
    Down {
        /// Remove containers after stopping them
        #[arg(long)]
        remove: bool,
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
#[derive(Debug, Clone, Subcommand)]
pub enum FeatureCommands {
    /// Test feature implementations
    Test {
        /// Path to feature directory to test
        path: String,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
    /// Package features for distribution
    Package {
        /// Path to feature directory to package
        path: String,
        /// Output directory for the package
        #[arg(long)]
        output: String,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
    /// Publish features to registry
    Publish {
        /// Path to feature directory to publish
        path: String,
        /// Target registry URL
        #[arg(long)]
        registry: String,
        /// Dry run (don't actually publish)
        #[arg(long)]
        dry_run: bool,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
    /// Get feature information
    Info { mode: String, feature: String },
}

/// Template management subcommands
#[derive(Debug, Clone, Subcommand)]
pub enum TemplateCommands {
    /// Apply template to current project
    Apply { template: String },
    /// Publish templates to registry
    Publish {
        /// Path to template directory to publish
        path: String,
        /// Target registry URL
        #[arg(long)]
        registry: String,
        /// Dry run (don't actually publish)
        #[arg(long)]
        dry_run: bool,
    },
    /// Get template metadata
    Metadata {
        /// Path to template directory
        path: String,
    },
    /// Generate template documentation
    GenerateDocs {
        /// Path to template directory
        path: String,
        /// Output directory for generated documentation
        #[arg(long)]
        output: String,
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

    /// Override configuration file path (highest precedence)
    #[arg(long, global = true, value_name = "PATH")]
    pub override_config: Option<PathBuf>,

    /// Secrets file path (KEY=VALUE format, can be specified multiple times)
    #[arg(long, global = true, value_name = "PATH")]
    pub secrets_file: Vec<PathBuf>,

    /// Disable secret redaction in output (debugging only - WARNING: may expose secrets)
    #[arg(long, global = true)]
    pub no_redact: bool,

    /// Progress format (json|none|auto). Auto is silent unless --progress-file is set.
    #[arg(long, global = true, value_enum, default_value = "auto")]
    pub progress: ProgressFormat,

    /// Progress file path (for JSON output when using --progress auto or json)
    #[arg(long, global = true, value_name = "PATH")]
    pub progress_file: Option<PathBuf>,

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
    /// Build a CliContext from the parsed CLI options.
    ///
    /// Returns a new `CliContext` populated with the values from this `Cli` instance
    /// (log and progress settings, workspace/config paths, secrets, and plugin list when enabled).
    ///
    /// # Examples
    ///
    /// ```
    /// use clap::Parser;
    /// // Parse CLI arguments (use just the program name to rely on defaults)
    /// let cli = deacon::cli::Cli::parse_from(&["deacon"]);
    /// let ctx = cli.context();
    /// // Context should be constructed; workspace_folder is optional by default
    /// assert!(ctx.workspace_folder.is_none());
    /// ```
    pub fn context(&self) -> CliContext {
        CliContext {
            log_format: self.log_format.clone(),
            log_level: self.log_level.clone(),
            progress_format: self.progress.clone(),
            progress_file: self.progress_file.clone(),
            workspace_folder: self.workspace_folder.clone(),
            config: self.config.clone(),
            override_config: self.override_config.clone(),
            secrets_files: self.secrets_file.clone(),
            no_redact: self.no_redact,
            #[cfg(feature = "plugins")]
            plugins: self.plugin.clone(),
        }
    }

    /// Dispatches the CLI subcommand represented by this `Cli` instance.
    ///
    /// Initializes logging and progress tracking according to the CLI options, then
    /// executes the selected subcommand. Returns `Ok(())` on success or an error
    /// propagated from the invoked command. If no subcommand is provided, a brief
    /// help-like message is printed and `Ok(())` is returned. For the `up`
    /// subcommand, a missing configuration file is mapped to a user-facing error
    /// message ("No devcontainer.json found in workspace") to preserve CLI
    /// compatibility.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::runtime::Runtime;
    /// // Construct `Cli` via your preferred method (e.g., `Cli::parse()` or manual).
    /// // let cli = Cli::parse_from(&["deacon", "build", "--no-cache"]);
    /// // For demonstration, assume `cli` is available:
    /// // let cli = ... ;
    /// // Execute the dispatcher in a tokio runtime:
    /// // Runtime::new().unwrap().block_on(cli.dispatch()).unwrap();
    /// ```
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

        // Initialize progress tracking
        let progress_format: deacon_core::progress::ProgressFormat = self.progress.clone().into();
        let progress_tracker = deacon_core::progress::create_progress_tracker(
            &progress_format,
            self.progress_file.as_deref(),
            self.workspace_folder.as_deref(),
        )?;

        // Convert to Arc<Mutex<Option<_>>> for sharing between operations
        let progress_tracker = std::sync::Arc::new(std::sync::Mutex::new(progress_tracker));

        match self.command {
            Some(Commands::Up {
                remove_existing_container,
                skip_post_create,
                skip_non_blocking_commands,
                ports_events,
                shutdown,
                additional_features,
                prefer_cli_features,
                feature_install_order,
                ignore_host_requirements,
            }) => {
                use crate::commands::up::{execute_up, UpArgs};

                let args = UpArgs {
                    remove_existing_container,
                    skip_post_create,
                    skip_non_blocking_commands,
                    ports_events,
                    shutdown,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                    additional_features,
                    prefer_cli_features,
                    feature_install_order,
                    ignore_host_requirements,
                    progress_tracker: progress_tracker.clone(),
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
                additional_features,
                prefer_cli_features,
                feature_install_order,
                ignore_host_requirements,
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
                    additional_features,
                    prefer_cli_features,
                    feature_install_order,
                    ignore_host_requirements,
                    progress_tracker: progress_tracker.clone(),
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
                    override_config_path: self.override_config,
                    secrets_files: self.secrets_file,
                };

                execute_read_configuration(args).await?;
                Ok(())
            }
            Some(Commands::Features { command }) => {
                use crate::commands::features::{execute_features, FeaturesArgs};

                let args = FeaturesArgs {
                    command,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                };

                execute_features(args).await
            }
            Some(Commands::Templates { command }) => {
                use crate::commands::templates::{execute_templates, TemplatesArgs};

                let args = TemplatesArgs {
                    command,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                };

                execute_templates(args).await
            }
            Some(Commands::RunUserCommands { .. }) => {
                Err(DeaconError::Config(ConfigError::NotImplemented {
                    feature: "run-user-commands command".to_string(),
                })
                .into())
            }
            Some(Commands::Down { remove }) => {
                use crate::commands::down::{execute_down, DownArgs};

                let args = DownArgs {
                    remove,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                };

                execute_down(args).await
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
