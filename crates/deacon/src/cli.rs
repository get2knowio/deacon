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
    /// Enabled plugins
    #[cfg(feature = "plugins")]
    pub plugins: Vec<String>,
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
        /// Skip postCreate lifecycle phase
        #[arg(long)]
        skip_post_create: bool,
    },

    /// Build development container image
    ///
    /// References: CLI-SPEC.md "Docker Integration"
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
    ///
    /// References: CLI-SPEC.md "Process Management and Shell Integration"
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
            #[cfg(feature = "plugins")]
            plugins: self.plugin.clone(),
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
            Some(Commands::Up {
                remove_existing_container,
                skip_post_create,
            }) => {
                // Initialize configuration and workspace discovery
                let workspace_folder = self.workspace_folder.clone().unwrap_or_else(|| {
                    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
                });

                tracing::info!(
                    "Starting up container for workspace: {}",
                    workspace_folder.display()
                );

                #[cfg(feature = "docker")]
                {
                    use deacon_core::config::ConfigLoader;
                    use deacon_core::container::ContainerIdentity;
                    use deacon_core::docker::{CliDocker, Docker, DockerLifecycle};

                    // Load configuration
                    let config_location = ConfigLoader::discover_config(&workspace_folder)?;

                    if !config_location.exists() {
                        return Err(anyhow::anyhow!("No devcontainer.json found in workspace"));
                    }

                    let config = ConfigLoader::load_from_path(config_location.path())?;

                    // Ensure we have an image configured
                    if config.image.is_none() {
                        return Err(anyhow::anyhow!("No image specified in devcontainer.json"));
                    }

                    // Create container identity for this workspace and configuration
                    let identity = ContainerIdentity::new(&workspace_folder, &config);

                    // Check Docker availability
                    let docker_client = CliDocker::new();
                    match docker_client.check_docker_installed() {
                        Ok(()) => {
                            tracing::info!("Docker is available");
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!("Docker is not available: {}", e));
                        }
                    }

                    // Create async runtime and execute up workflow
                    let runtime = tokio::runtime::Runtime::new()
                        .map_err(|e| anyhow::anyhow!("Failed to create async runtime: {}", e))?;

                    let result = runtime.block_on(async {
                        // Check Docker daemon availability
                        docker_client.ping().await?;

                        // Execute up workflow
                        docker_client
                            .up(
                                &identity,
                                &config,
                                &workspace_folder,
                                remove_existing_container,
                            )
                            .await
                    })?;

                    // Output JSON result
                    let json_output = serde_json::to_string_pretty(&result)?;
                    println!("{}", json_output);

                    if skip_post_create {
                        tracing::info!(
                            "Skipping postCreate lifecycle phase due to --skip-post-create flag"
                        );
                    }

                    tracing::info!(
                        "Container {} (reused: {})",
                        result.container_id,
                        result.reused
                    );
                    Ok(())
                }

                #[cfg(not(feature = "docker"))]
                {
                    return Err(anyhow::anyhow!(
                        "Docker support is disabled (compiled without 'docker' feature)"
                    ));
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

                let runtime = tokio::runtime::Runtime::new()
                    .map_err(|e| anyhow::anyhow!("Failed to create async runtime: {}", e))?;

                runtime.block_on(execute_build(args))?;
                Ok(())
            }
            Some(Commands::Exec {
                user,
                no_tty,
                env,
                command,
            }) => {
                if command.is_empty() {
                    return Err(anyhow::anyhow!("No command specified for exec"));
                }

                #[cfg(feature = "docker")]
                {
                    use deacon_core::docker::{CliDocker, Docker, ExecConfig};
                    use std::collections::HashMap;

                    tracing::info!("Executing command in container: {:?}", command);

                    let docker_client = CliDocker::new();

                    // Parse environment variables
                    let mut env_map = HashMap::new();
                    for env_var in env {
                        if let Some((key, value)) = env_var.split_once('=') {
                            env_map.insert(key.to_string(), value.to_string());
                        } else {
                            return Err(anyhow::anyhow!(
                                "Invalid environment variable format: '{}'. Expected KEY=VALUE",
                                env_var
                            ));
                        }
                    }

                    // Determine TTY allocation
                    let should_use_tty = !no_tty && CliDocker::is_tty();

                    // Create exec config
                    let exec_config = ExecConfig {
                        user: user.clone(),
                        working_dir: self
                            .workspace_folder
                            .as_ref()
                            .map(|p| p.to_string_lossy().to_string()),
                        env: env_map,
                        tty: should_use_tty,
                        interactive: should_use_tty,
                        detach: false,
                    };

                    let runtime = tokio::runtime::Runtime::new()
                        .map_err(|e| anyhow::anyhow!("Failed to create async runtime: {}", e))?;

                    // For now, use a default container ID - in a real implementation,
                    // this would be discovered from the workspace configuration
                    let container_id = "devcontainer"; // This should be discovered

                    match runtime.block_on(docker_client.exec(container_id, &command, exec_config))
                    {
                        Ok(result) => {
                            tracing::info!(
                                "Command completed with exit code: {}",
                                result.exit_code
                            );
                            std::process::exit(result.exit_code);
                        }
                        Err(e) => {
                            tracing::error!("Failed to execute command: {}", e);
                            Err(e.into())
                        }
                    }
                }

                #[cfg(not(feature = "docker"))]
                {
                    tracing::warn!(
                        "Docker support is disabled (compiled without 'docker' feature)"
                    );
                    Err(DeaconError::Config(ConfigError::NotImplemented {
                        feature: "exec command (docker support disabled)".to_string(),
                    })
                    .into())
                }
            }
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
