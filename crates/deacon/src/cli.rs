use crate::ui::spinner::{PlainSpinner, SpinnerEmitter};
use anyhow::Result;
use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use std::io::IsTerminal;
use std::path::PathBuf;

/// Runtime selection options
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq)]
pub enum RuntimeOption {
    /// Docker runtime
    Docker,
    /// Podman runtime
    Podman,
}

impl From<RuntimeOption> for deacon_core::runtime::RuntimeKind {
    fn from(runtime: RuntimeOption) -> Self {
        match runtime {
            RuntimeOption::Docker => deacon_core::runtime::RuntimeKind::Docker,
            RuntimeOption::Podman => deacon_core::runtime::RuntimeKind::Podman,
        }
    }
}

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
#[derive(Debug, Clone, ValueEnum, PartialEq, Eq)]
pub enum ProgressFormat {
    /// No progress output
    None,
    /// JSON structured progress events
    Json,
    /// Auto mode: silent unless --progress-file is set (future: TTY spinner)
    Auto,
}

/// BuildKit usage control options
#[derive(Debug, Clone, ValueEnum, PartialEq)]
pub enum BuildKitOption {
    /// Automatically detect and use BuildKit if available (respects DOCKER_BUILDKIT)
    Auto,
    /// Never use BuildKit, force legacy docker build
    Never,
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
    pub plugins: Vec<String>,
    /// Container runtime selection
    pub runtime: Option<deacon_core::runtime::RuntimeKind>,
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
        /// Forward port(s) from container to host (can be repeated)
        /// Format: PORT or HOST_PORT:CONTAINER_PORT
        #[arg(long = "forward-port")]
        forward_ports: Vec<String>,
        /// Custom container name (overrides generated name)
        #[arg(long)]
        container_name: Option<String>,
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
        /// Environment file(s) to pass to docker compose (can be repeated)
        #[arg(long)]
        env_file: Vec<PathBuf>,
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
        /// Cache source images (external cache sources like registry://<ref>)
        #[arg(long)]
        cache_from: Vec<String>,
        /// Cache destination (external cache destinations like registry://<ref>)
        #[arg(long)]
        cache_to: Vec<String>,
        /// BuildKit usage control (auto respects DOCKER_BUILDKIT, never disables)
        #[arg(long, value_enum)]
        buildkit: Option<BuildKitOption>,
        /// Secret to expose to the build (format: id=secretname[,src=path])
        #[arg(long)]
        secret: Vec<String>,
        /// Build secret (format: id=<id>[,src=<path>|env=<var>], requires BuildKit)
        #[arg(long)]
        build_secret: Vec<String>,
        /// SSH agent socket or keys to expose to the build
        #[arg(long)]
        ssh: Vec<String>,
        /// Run vulnerability scan on built image
        #[arg(long)]
        scan_image: bool,
        /// Fail build if vulnerability scan returns non-zero exit code
        #[arg(long, requires = "scan_image")]
        fail_on_scan: bool,
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
        /// Environment file(s) to pass to docker compose (can be repeated)
        #[arg(long)]
        env_file: Vec<PathBuf>,
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
        /// Working directory for command execution
        #[arg(short = 'w', long)]
        workdir: Option<String>,
        /// Target container ID directly
        #[arg(long)]
        container_id: Option<String>,
        /// Identify container by labels (KEY=VALUE format, can be specified multiple times)
        #[arg(long, action = clap::ArgAction::Append)]
        id_label: Vec<String>,
        /// Target specific service in Docker Compose projects (defaults to primary service)
        #[arg(long)]
        service: Option<String>,
        /// Command to execute
        command: Vec<String>,
    },

    /// Read and display configuration
    ReadConfiguration {
        /// Include merged configuration
        #[arg(long)]
        include_merged_configuration: bool,
        /// Target container ID directly
        #[arg(long)]
        container_id: Option<String>,
        /// Identify container by labels (KEY=VALUE format, can be specified multiple times)
        #[arg(long, action = clap::ArgAction::Append)]
        id_label: Vec<String>,
    },

    /// Configuration management commands
    Config {
        /// Config subcommand
        #[command(subcommand)]
        command: ConfigCommands,
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

    /// Run user-defined lifecycle commands
    #[allow(clippy::enum_variant_names)]
    RunUserCommands {
        /// Skip postCreate lifecycle phase
        #[arg(long)]
        skip_post_create: bool,
        /// Skip postAttach lifecycle phase  
        #[arg(long)]
        skip_post_attach: bool,
        /// Skip non-blocking commands (postStart & postAttach phases)
        #[arg(long)]
        skip_non_blocking_commands: bool,
        /// Stop after updateContentCommand (prebuild mode)
        #[arg(long)]
        prebuild: bool,
        /// Stop before personalization
        #[arg(long)]
        stop_for_personalization: bool,
        /// Target container ID directly
        #[arg(long)]
        container_id: Option<String>,
        /// Identify container by labels (KEY=VALUE format, can be specified multiple times)
        #[arg(long, action = clap::ArgAction::Append)]
        id_label: Vec<String>,
    },

    /// Stop and optionally remove development container or compose project
    Down {
        /// Remove containers after stopping them
        #[arg(long)]
        remove: bool,
        /// Include all containers matching labels (stale containers)
        #[arg(long)]
        all: bool,
        /// Remove associated anonymous volumes
        #[arg(long)]
        volumes: bool,
        /// Force removal of running containers
        #[arg(long)]
        force: bool,
        /// Timeout in seconds for stopping containers (default: 30)
        #[arg(long)]
        timeout: Option<u32>,
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
    /// Pull features from registry
    Pull {
        /// Registry reference (registry/namespace/name:version)
        registry_ref: String,
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
        /// Username for registry authentication
        #[arg(long)]
        username: Option<String>,
        /// Read password from stdin
        #[arg(long)]
        password_stdin: bool,
    },
    /// Get feature information
    Info {
        /// Information mode (manifest, tags, dependencies, verbose)
        mode: String,
        /// Feature path (local directory) or registry reference
        feature: String,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
    /// Generate feature installation plan
    Plan {
        /// Output in JSON format
        #[arg(long, default_value_t = true, action = ArgAction::Set)]
        json: bool,
        /// Additional features to install (JSON map of id -> value/options)
        #[arg(long)]
        additional_features: Option<String>,
    },
}

/// Template management subcommands
#[derive(Debug, Clone, Subcommand)]
pub enum TemplateCommands {
    /// Apply template to current project
    Apply {
        /// Template path (local directory) or registry reference
        template: String,
        /// Template option in key=value format
        #[arg(long)]
        option: Vec<String>,
        /// Output directory for applied template (default: current directory)
        #[arg(long)]
        output: Option<String>,
        /// Force overwrite existing files
        #[arg(long)]
        force: bool,
        /// Dry run mode - preview operations without making changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Pull templates from registry
    Pull {
        /// Registry reference (registry/namespace/name:version)
        registry_ref: String,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
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
        /// Username for registry authentication
        #[arg(long)]
        username: Option<String>,
        /// Read password from stdin
        #[arg(long)]
        password_stdin: bool,
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

/// Configuration management subcommands
#[derive(Debug, Clone, Subcommand)]
pub enum ConfigCommands {
    /// Apply variable substitution to configuration and preview results
    Substitute {
        /// Preview substitution without applying changes (dry-run mode)
        #[arg(long)]
        dry_run: bool,
        /// Use strict substitution mode (fail on unresolved variables)
        #[arg(long)]
        strict_substitution: bool,
        /// Maximum recursion depth for nested variable substitution
        #[arg(long, default_value = "5")]
        max_depth: usize,
        /// Enable multi-pass nested variable resolution
        #[arg(long = "nested", default_value_t = true, action = clap::ArgAction::Set)]
        nested: bool,
        /// Output format (text or json)
        #[arg(long, value_enum, default_value = "json")]
        output_format: OutputFormat,
    },
}

#[derive(Parser, Debug)]
#[command(
    name = env!("CARGO_PKG_NAME"),
    version,
    about = "Development container CLI",
    long_about = "Development container CLI\n\nImplements the Development Containers specification for creating and managing development environments.",
    color = clap::ColorChoice::Auto
)]
pub struct Cli {
    /// Log format (text or json, defaults to text, can be set via DEACON_LOG_FORMAT env var)
    #[arg(long, global = true, value_enum)]
    pub log_format: Option<LogFormat>,

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

    #[arg(long, global = true, value_name = "NAME")]
    pub plugin: Vec<String>,

    /// Container runtime to use (docker or podman, can be set via DEACON_RUNTIME env var)
    #[arg(long, global = true, value_enum)]
    pub runtime: Option<RuntimeOption>,

    /// Path to docker executable
    #[arg(long, global = true, default_value = "docker")]
    pub docker_path: String,

    /// Path to docker-compose executable
    #[arg(long, global = true, default_value = "docker-compose")]
    pub docker_compose_path: String,

    /// Terminal columns for output formatting (requires --terminal-rows)
    #[arg(long, global = true, requires = "terminal_rows")]
    pub terminal_columns: Option<u32>,

    /// Terminal rows for output formatting (requires --terminal-columns)
    #[arg(long, global = true, requires = "terminal_columns")]
    pub terminal_rows: Option<u32>,

    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Option<Commands>,
}

impl Cli {
    /// Validate CLI arguments after parsing
    ///
    /// Performs additional validation beyond what clap provides automatically.
    /// Currently validates that terminal dimensions (if provided) are positive integers.
    ///
    /// # Errors
    ///
    /// Returns an error if terminal dimensions are zero or if any other validation fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use clap::Parser;
    /// let cli = deacon::cli::Cli::parse_from(&["deacon"]);
    /// assert!(cli.validate().is_ok());
    /// ```
    pub fn validate(&self) -> Result<()> {
        // Clap's `requires` attribute already ensures both terminal dimensions are provided together
        // Here we add additional validation for positive values
        if let (Some(cols), Some(rows)) = (self.terminal_columns, self.terminal_rows) {
            if cols == 0 || rows == 0 {
                anyhow::bail!("Terminal dimensions must be positive integers");
            }
        }
        Ok(())
    }

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
            log_format: self.log_format.clone().unwrap_or(LogFormat::Text), // Default to Text if not specified
            log_level: self.log_level.clone(),
            progress_format: self.progress.clone(),
            progress_file: self.progress_file.clone(),
            workspace_folder: self.workspace_folder.clone(),
            config: self.config.clone(),
            override_config: self.override_config.clone(),
            secrets_files: self.secrets_file.clone(),
            no_redact: self.no_redact,

            plugins: self.plugin.clone(),
            runtime: self.runtime.map(|r| r.into()),
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

        // Validate CLI arguments
        self.validate()?;

        // Initialize logging based on global options
        let log_format = match self.log_format {
            Some(LogFormat::Text) => Some("text"),
            Some(LogFormat::Json) => Some("json"),
            None => None, // Let logging module check environment variable
        };

        let mut log_level = match self.log_level {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        };

        // Determine if spinner-friendly session: progress auto, no progress_file, stderr is TTY, non-JSON format
        let stderr_is_tty = std::io::stderr().is_terminal();
        let json_format = matches!(log_format, Some("json"));
        let spinner_eligible = self.progress == ProgressFormat::Auto
            && self.progress_file.is_none()
            && stderr_is_tty
            && !json_format;

        // Set environment variable for log level before initializing logging
        if std::env::var_os("DEACON_LOG").is_none() && std::env::var_os("RUST_LOG").is_none() {
            // In spinner sessions, prefer quieter default unless user overrode via flag/env
            if spinner_eligible {
                log_level = "warn";
            }
            std::env::set_var(
                "RUST_LOG",
                format!("deacon={},deacon_core={}", log_level, log_level),
            );
        }
        deacon_core::logging::init(log_format)?;

        // Emit a debug log to help with testing
        tracing::debug!("CLI initialized with log level: {}", log_level);

        // Warn if redaction is disabled
        if self.no_redact {
            tracing::warn!("Secret redaction is DISABLED via --no-redact flag. This may expose sensitive information in logs and output. Use only for debugging purposes!");
        }

        // Create redaction configuration from CLI flags
        let redaction_config = if self.no_redact {
            deacon_core::redaction::RedactionConfig::disabled()
        } else {
            deacon_core::redaction::RedactionConfig::default()
        };

        // Get global secret registry
        let secret_registry = deacon_core::redaction::global_registry();

        // Initialize progress tracking
        let progress_format: deacon_core::progress::ProgressFormat = self.progress.clone().into();

        // Prefer spinner emitter in eligible sessions; otherwise fall back to core helper
        let progress_tracker = if spinner_eligible {
            // Build a tracker with SpinnerEmitter
            use deacon_core::progress::get_cache_dir;
            use deacon_core::progress::ProgressTracker;
            let cache_dir = get_cache_dir()?;
            let emitter: Box<dyn deacon_core::progress::ProgressEmitter> =
                Box::new(SpinnerEmitter::new());
            Some(ProgressTracker::new(
                Some(emitter),
                Some(&cache_dir),
                redaction_config.clone(),
            )?)
        } else {
            deacon_core::progress::create_progress_tracker(
                &progress_format,
                self.progress_file.as_deref(),
                self.workspace_folder.as_deref(),
                &redaction_config,
                secret_registry,
            )?
        };

        // Convert to Arc<Mutex<Option<_>>> for sharing between operations
        let progress_tracker = std::sync::Arc::new(std::sync::Mutex::new(progress_tracker));

        match self.command {
            Some(Commands::Up {
                remove_existing_container,
                skip_post_create,
                skip_non_blocking_commands,
                ports_events,
                shutdown,
                forward_ports,
                container_name,
                additional_features,
                prefer_cli_features,
                feature_install_order,
                ignore_host_requirements,
                env_file,
            }) => {
                use crate::commands::up::{execute_up, UpArgs};

                let args = UpArgs {
                    remove_existing_container,
                    skip_post_create,
                    skip_non_blocking_commands,
                    ports_events,
                    shutdown,
                    forward_ports,
                    container_name,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                    additional_features,
                    prefer_cli_features,
                    feature_install_order,
                    ignore_host_requirements,
                    progress_tracker: progress_tracker.clone(),
                    runtime: self.runtime.map(|r| r.into()),
                    redaction_config: redaction_config.clone(),
                    secret_registry: secret_registry.clone(),
                    env_file,
                    docker_path: self.docker_path.clone(),
                    docker_compose_path: self.docker_compose_path.clone(),
                    terminal_columns: self.terminal_columns,
                    terminal_rows: self.terminal_rows,
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
                cache_from,
                cache_to,
                buildkit,
                secret,
                build_secret,
                ssh,
                scan_image,
                fail_on_scan,
                additional_features,
                prefer_cli_features,
                feature_install_order,
                ignore_host_requirements,
                env_file,
            }) => {
                use crate::commands::build::{execute_build, BuildArgs};

                let args = BuildArgs {
                    no_cache,
                    platform,
                    build_arg,
                    force,
                    output_format,
                    cache_from,
                    cache_to,
                    buildkit,
                    secret,
                    build_secret,
                    ssh,
                    scan_image,
                    fail_on_scan,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                    additional_features,
                    prefer_cli_features,
                    feature_install_order,
                    ignore_host_requirements,
                    progress_tracker: progress_tracker.clone(),
                    redaction_config: redaction_config.clone(),
                    secret_registry: secret_registry.clone(),
                    env_file,
                    docker_path: self.docker_path.clone(),
                    terminal_columns: self.terminal_columns,
                    terminal_rows: self.terminal_rows,
                };

                execute_build(args).await?;
                Ok(())
            }
            Some(Commands::Exec {
                user,
                no_tty,
                env,
                workdir,
                container_id,
                id_label,
                service,
                command,
            }) => {
                use crate::commands::exec::{execute_exec, ExecArgs};

                // Exec attaches to an interactive shell. If a spinner-based progress tracker
                // was initialized earlier (eligible session), drop it now to avoid the spinner
                // continuing to tick while the terminal is attached to the container.
                {
                    let mut guard = progress_tracker.lock().unwrap();
                    // Take and drop any existing tracker/emitter immediately to prevent spinner from ticking.
                    let _ = (*guard).take();
                }

                let args = ExecArgs {
                    user,
                    no_tty,
                    env,
                    workdir,
                    container_id,
                    id_label,
                    service,
                    command,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                    docker_path: self.docker_path.clone(),
                    docker_compose_path: self.docker_compose_path.clone(),
                };

                execute_exec(args).await
            }
            Some(Commands::ReadConfiguration {
                include_merged_configuration,
                container_id,
                id_label,
            }) => {
                use crate::commands::read_configuration::{
                    execute_read_configuration, ReadConfigurationArgs,
                };

                let args = ReadConfigurationArgs {
                    include_merged_configuration,
                    container_id,
                    id_label,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                    override_config_path: self.override_config,
                    secrets_files: self.secrets_file,
                    redaction_config: redaction_config.clone(),
                    secret_registry: secret_registry.clone(),
                };

                execute_read_configuration(args).await?;
                Ok(())
            }
            Some(Commands::Config { command }) => {
                use crate::commands::config::{execute_config, ConfigArgs};

                let args = ConfigArgs {
                    command,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                    override_config_path: self.override_config,
                    secrets_files: self.secrets_file,
                    redaction_config: redaction_config.clone(),
                };

                execute_config(args).await
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
            Some(Commands::RunUserCommands {
                skip_post_create,
                skip_post_attach,
                skip_non_blocking_commands,
                prebuild,
                stop_for_personalization,
                container_id,
                id_label,
            }) => {
                use crate::commands::run_user_commands::{
                    execute_run_user_commands, RunUserCommandsArgs,
                };

                let args = RunUserCommandsArgs {
                    skip_post_create,
                    skip_post_attach,
                    skip_non_blocking_commands,
                    prebuild,
                    stop_for_personalization,
                    container_id,
                    id_label,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                    override_config_path: self.override_config,
                    secrets_files: self.secrets_file,
                    progress_tracker: progress_tracker.clone(),
                    docker_path: self.docker_path.clone(),
                };

                execute_run_user_commands(args).await
            }
            Some(Commands::Down {
                remove,
                all,
                volumes,
                force,
                timeout,
            }) => {
                use crate::commands::down::{execute_down, DownArgs};

                let args = DownArgs {
                    remove,
                    all,
                    volumes,
                    force,
                    timeout,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                    docker_path: self.docker_path.clone(),
                    docker_compose_path: self.docker_compose_path.clone(),
                };

                // If spinner is eligible, wrap the down execution with a plain spinner
                if spinner_eligible {
                    if stderr_is_tty {
                        let sp = PlainSpinner::start("Stopping environment…");
                        let res = execute_down(args).await;
                        match res {
                            Ok(()) => {
                                sp.finish_with_message("Shutdown completed");
                                Ok(())
                            }
                            Err(e) => {
                                sp.fail_with_message("Shutdown failed");
                                Err(e)
                            }
                        }
                    } else {
                        execute_down(args).await
                    }
                } else {
                    execute_down(args).await
                }
            }
            Some(Commands::Doctor { json, bundle }) => {
                // Create a DoctorContext for doctor command
                let context = deacon_core::doctor::DoctorContext {
                    workspace_folder: self.workspace_folder.clone(),
                    config: self.config.clone(),
                };

                // Execute doctor command with redaction config
                match deacon_core::doctor::run_doctor(json, bundle, context, redaction_config).await
                {
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_global_flags_default_values() {
        let cli = Cli::parse_from(["deacon"]);
        assert_eq!(cli.docker_path, "docker");
        assert_eq!(cli.docker_compose_path, "docker-compose");
        assert!(cli.terminal_columns.is_none());
        assert!(cli.terminal_rows.is_none());
    }

    #[test]
    fn test_global_flags_custom_values() {
        let cli = Cli::parse_from([
            "deacon",
            "--docker-path",
            "/usr/local/bin/docker",
            "--docker-compose-path",
            "/usr/local/bin/docker-compose",
        ]);
        assert_eq!(cli.docker_path, "/usr/local/bin/docker");
        assert_eq!(cli.docker_compose_path, "/usr/local/bin/docker-compose");
    }

    #[test]
    fn test_terminal_dimensions_both_required() {
        // Should fail if only columns provided
        let result = Cli::try_parse_from(["deacon", "--terminal-columns", "80"]);
        assert!(result.is_err());

        // Should fail if only rows provided
        let result = Cli::try_parse_from(["deacon", "--terminal-rows", "24"]);
        assert!(result.is_err());

        // Should succeed if both provided
        let result = Cli::try_parse_from([
            "deacon",
            "--terminal-columns",
            "80",
            "--terminal-rows",
            "24",
        ]);
        assert!(result.is_ok());
        let cli = result.unwrap();
        assert_eq!(cli.terminal_columns, Some(80));
        assert_eq!(cli.terminal_rows, Some(24));
    }

    #[test]
    fn test_validate_rejects_zero_dimensions() {
        // Zero columns should fail
        let cli = Cli::parse_from(["deacon", "--terminal-columns", "0", "--terminal-rows", "24"]);
        assert!(cli.validate().is_err());

        // Zero rows should fail
        let cli = Cli::parse_from(["deacon", "--terminal-columns", "80", "--terminal-rows", "0"]);
        assert!(cli.validate().is_err());

        // Both zero should fail
        let cli = Cli::parse_from(["deacon", "--terminal-columns", "0", "--terminal-rows", "0"]);
        assert!(cli.validate().is_err());
    }

    #[test]
    fn test_validate_accepts_positive_dimensions() {
        let cli = Cli::parse_from([
            "deacon",
            "--terminal-columns",
            "80",
            "--terminal-rows",
            "24",
        ]);
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn test_validate_accepts_no_dimensions() {
        let cli = Cli::parse_from(["deacon"]);
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn test_global_flags_with_subcommand() {
        let cli = Cli::parse_from([
            "deacon",
            "--docker-path",
            "/custom/docker",
            "--terminal-columns",
            "120",
            "--terminal-rows",
            "30",
            "build",
        ]);
        assert_eq!(cli.docker_path, "/custom/docker");
        assert_eq!(cli.terminal_columns, Some(120));
        assert_eq!(cli.terminal_rows, Some(30));
        assert!(matches!(cli.command, Some(Commands::Build { .. })));
    }
}
