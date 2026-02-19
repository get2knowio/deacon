use crate::commands::shared::TerminalDimensions;
use crate::ui::spinner::{PlainSpinner, SpinnerEmitter};
use anyhow::Result;
#[cfg(feature = "full")]
use clap::ArgAction;
use clap::{Parser, Subcommand, ValueEnum};
use deacon_core::container_env_probe::ContainerProbeMode;

/// CLI-facing probe enum (value_enum for clap) to map into core probe mode
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DefaultUserEnvProbe {
    None,
    LoginInteractiveShell,
    InteractiveShell,
    LoginShell,
}

impl From<DefaultUserEnvProbe> for ContainerProbeMode {
    fn from(p: DefaultUserEnvProbe) -> Self {
        match p {
            DefaultUserEnvProbe::None => ContainerProbeMode::None,
            DefaultUserEnvProbe::LoginInteractiveShell => ContainerProbeMode::LoginInteractiveShell,
            DefaultUserEnvProbe::InteractiveShell => ContainerProbeMode::LoginShell,
            DefaultUserEnvProbe::LoginShell => ContainerProbeMode::LoginShell,
        }
    }
}
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
#[allow(clippy::large_enum_variant)]
pub enum Commands {
    /// Create and run development container
    #[command(long_about = "Create and run development container\n\n\
        When dev container features are configured, the following behaviors apply during container creation:\n\n  \
        - Security options (privileged, init, capAdd, securityOpt) from features are automatically merged into the container configuration\n  \
        - Feature lifecycle commands (onCreateCommand, postCreateCommand, etc.) execute before the corresponding config-level commands\n  \
        - Feature mounts are merged with config mounts; config mounts take precedence on target path conflicts\n  \
        - When multiple features define entrypoints, they are chained via a wrapper script to ensure all run in sequence")]
    Up {
        // Container identity and discovery
        /// Container ID label(s) for identification (format: name=value, can be repeated)
        #[arg(long, action = clap::ArgAction::Append)]
        id_label: Vec<String>,

        // Runtime behavior
        /// Remove existing container(s) first
        #[arg(long)]
        remove_existing_container: bool,
        /// Expect existing container (fail if not found)
        #[arg(long)]
        expect_existing_container: bool,
        /// Stop after updateContentCommand (prebuild mode)
        #[arg(long)]
        prebuild: bool,
        /// Skip postCreate lifecycle phase
        #[arg(long)]
        skip_post_create: bool,
        /// Skip postAttach lifecycle phase
        #[arg(long)]
        skip_post_attach: bool,
        /// Skip non-blocking commands (postStart & postAttach phases)
        #[arg(long)]
        skip_non_blocking_commands: bool,
        /// Default user environment probe mode when config omits userEnvProbe.
        /// Allowed values: `none`, `loginInteractiveShell`, `interactiveShell`, `loginShell`.
        /// Default: `loginInteractiveShell`.
        #[arg(long, value_enum, default_value = "login-interactive-shell")]
        default_user_env_probe: DefaultUserEnvProbe,

        // Mounts and environment
        /// Additional mount (format: type=bind|volume,source=<path>,target=<path>[,external=true|false], can be repeated)
        #[arg(long)]
        mount: Vec<String>,
        /// Remote environment variable (format: NAME=value, can be repeated)
        #[arg(long)]
        remote_env: Vec<String>,
        /// Mount workspace git root instead of workspace folder
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        mount_workspace_git_root: bool,
        /// Workspace mount consistency (consistent, cached, delegated)
        #[arg(long)]
        workspace_mount_consistency: Option<String>,

        // Build and cache options
        /// Build without using cache
        #[arg(long)]
        build_no_cache: bool,
        /// External cache source (can be repeated, e.g. type=registry,ref=<image>)
        #[arg(long)]
        cache_from: Vec<String>,
        /// External cache destination (e.g. type=registry,ref=<image>)
        #[arg(long)]
        cache_to: Option<String>,
        /// BuildKit usage control (auto respects DOCKER_BUILDKIT, never disables)
        #[arg(long, value_enum)]
        buildkit: Option<BuildKitOption>,

        // Features and dotfiles
        /// Additional features to install (JSON map of id -> value/options)
        #[arg(long)]
        additional_features: Option<String>,
        /// CLI features take precedence over config features on conflicts
        #[arg(long)]
        prefer_cli_features: bool,
        /// Override feature installation order (comma-separated list of IDs)
        #[arg(long)]
        feature_install_order: Option<String>,
        /// Skip feature auto-mapping (hidden testing flag)
        #[arg(long, hide = true)]
        skip_feature_auto_mapping: bool,
        /// Path to feature lockfile for validation (experimental, hidden)
        #[arg(long, hide = true)]
        experimental_lockfile: Option<PathBuf>,
        /// Require lockfile to exist and match config features exactly (experimental, hidden)
        /// Implies --experimental-lockfile if not specified; uses default lockfile location.
        #[arg(long, hide = true)]
        experimental_frozen_lockfile: bool,
        /// Dotfiles repository URL
        #[arg(long)]
        dotfiles_repository: Option<String>,
        /// Dotfiles installation command
        #[arg(long)]
        dotfiles_install_command: Option<String>,
        /// Dotfiles target path inside container
        #[arg(long)]
        dotfiles_target_path: Option<String>,

        // Metadata and output control
        /// Omit config remoteEnv from image metadata
        #[arg(long)]
        omit_config_remote_env_from_metadata: bool,
        /// Omit Dockerfile syntax directive workaround
        #[arg(long)]
        omit_syntax_directive: bool,
        /// Include configuration in JSON output
        #[arg(long)]
        include_configuration: bool,
        /// Include merged configuration in JSON output
        #[arg(long)]
        include_merged_configuration: bool,

        // GPU and advanced options
        /// GPU handling mode for container operations
        ///
        /// Controls how GPU resources are requested when creating containers.
        ///
        /// Values:
        ///   all    - Always request GPU resources (--gpus all)
        ///   detect - Auto-detect GPU availability; warn once if absent
        ///   none   - No GPU requests, no GPU-related output (default)
        ///
        /// In detect mode, the system will probe for GPU capabilities and emit
        /// a single warning if no GPU runtime is found, then continue without GPU support.
        #[arg(long = "gpu-mode", default_value = "none", value_enum)]
        gpu_mode: deacon_core::gpu::GpuMode,
        /// Update remote user UID default behavior (never, on, off)
        #[arg(long)]
        update_remote_user_uid_default: Option<String>,

        // Port handling
        /// Emit machine-readable port events to stdout with PORT_EVENT prefix
        #[arg(long)]
        ports_events: bool,
        /// Forward port(s) from container to host (can be repeated)
        /// Format: PORT or HOST_PORT:CONTAINER_PORT
        #[arg(long = "forward-port")]
        forward_ports: Vec<String>,

        // Lifecycle
        /// Automatically shut down when process exits
        #[arg(long)]
        shutdown: bool,
        /// Custom container name (overrides generated name)
        #[arg(long)]
        container_name: Option<String>,

        // Host requirements
        /// Ignore host requirements validation (log warnings instead of failing)
        #[arg(long)]
        ignore_host_requirements: bool,

        // Compose
        /// Environment file(s) to pass to docker compose (can be repeated)
        #[arg(long)]
        env_file: Vec<PathBuf>,
    },

    /// Build development container image
    #[cfg(feature = "full")]
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
        /// Image name(s) to apply as tags (can be repeated)
        #[arg(long = "image-name")]
        image_names: Vec<String>,
        /// Metadata label to apply to the image in key=value format (can be repeated)
        #[arg(long)]
        label: Vec<String>,
        /// Push image to registry after build (requires BuildKit)
        #[arg(long)]
        push: bool,
        /// Export image to file or directory (BuildKit format: type=...,dest=...)
        #[arg(long)]
        output: Option<String>,
        /// Skip feature auto-mapping (hidden testing flag)
        #[arg(long, hide = true)]
        skip_feature_auto_mapping: bool,
        /// Do not persist customizations from features into image metadata
        #[arg(long, hide = true)]
        skip_persisting_customizations_from_features: bool,
        /// Write feature lockfile (experimental)
        #[arg(long, hide = true)]
        experimental_lockfile: bool,
        /// Fail if lockfile changes would occur (experimental)
        #[arg(long, hide = true)]
        experimental_frozen_lockfile: bool,
        /// Omit Dockerfile syntax directive workaround
        #[arg(long, hide = true)]
        omit_syntax_directive: bool,
    },

    /// Execute a command inside a running container.
    ///
    /// Usage examples:
    /// - `deacon exec --container-id <id> -- echo hello`
    /// - `deacon exec --id-label devcontainer.local_folder=/abs/path -- sh -lc 'pwd'`
    ///
    /// Note: At least one of `--container-id`, `--id-label` or `--workspace-folder` must be provided
    /// unless the command is invoked in a context where the target container can be inferred.
    Exec {
        /// User to run the command as inside the container (overrides config `remoteUser`).
        #[arg(long)]
        user: Option<String>,
        /// Disable TTY allocation (force non-interactive mode).
        /// Use this when piping output or in CI where a PTY is not desired.
        #[arg(long)]
        no_tty: bool,
        /// Environment variables to set inside the container (KEY=VALUE format).
        ///
        /// Alias: `--remote-env` (visible). Accepts empty values (e.g. `FOO=`) which will be
        /// injected as present with an empty string value.
        #[arg(long, action = clap::ArgAction::Append, alias = "remote-env")]
        env: Vec<String>,
        /// Working directory inside the container for command execution (overrides default).
        #[arg(short = 'w', long)]
        workdir: Option<String>,
        /// Target container ID directly (highest precedence selection).
        #[arg(long)]
        container_id: Option<String>,
        /// Identify container by labels (KEY=VALUE format, repeatable).
        /// Validated as `<name>=<value>`; multiple labels are combined as AND selectors.
        #[arg(long, action = clap::ArgAction::Append)]
        id_label: Vec<String>,
        /// Target specific service in Docker Compose projects (defaults to the primary service).
        #[arg(long)]
        service: Option<String>,
        /// Environment file(s) to pass to docker compose (can be repeated).
        #[arg(long)]
        env_file: Vec<PathBuf>,
        /// Default user environment probe mode when config omits `userEnvProbe`.
        /// Allowed values: `none`, `loginInteractiveShell`, `interactiveShell`, `loginShell`.
        /// Default: `loginInteractiveShell` (collects shell-initialized environment where possible).
        #[arg(long, value_enum, default_value = "login-interactive-shell")]
        default_user_env_probe: DefaultUserEnvProbe,
        /// Command and arguments to execute inside the container (positional; required).
        command: Vec<String>,
    },

    /// Read and display configuration
    ReadConfiguration {
        /// Include merged configuration
        #[arg(long)]
        include_merged_configuration: bool,
        /// Include features configuration
        #[arg(long)]
        include_features_configuration: bool,
        /// Target container ID directly
        #[arg(long)]
        container_id: Option<String>,
        /// Identify container by labels (KEY=VALUE format, can be specified multiple times).
        /// Used to locate the container if --container-id is not provided. If neither --container-id nor --id-label is set, one is inferred from --workspace-folder.
        #[arg(long, action = clap::ArgAction::Append)]
        id_label: Vec<String>,
        /// Mount workspace git root (default: true)
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        mount_workspace_git_root: bool,
        /// Additional features to install (JSON map of id -> value/options)
        #[arg(long)]
        additional_features: Option<String>,
        /// Skip feature auto-mapping (hidden testing flag)
        #[arg(long, hide = true)]
        skip_feature_auto_mapping: bool,
        /// Terminal columns (requires --terminal-rows)
        #[arg(long)]
        terminal_columns: Option<u32>,
        /// Terminal rows (requires --terminal-columns)
        #[arg(long)]
        terminal_rows: Option<u32>,
        /// User data folder (accepted but not used by this subcommand)
        #[arg(long)]
        user_data_folder: Option<PathBuf>,
    },

    /// Configuration management commands
    #[cfg(feature = "full")]
    Config {
        /// Config subcommand
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Feature management commands
    #[cfg(feature = "full")]
    Features {
        /// Feature subcommand
        #[command(subcommand)]
        command: FeatureCommands,
    },

    /// Template management commands
    #[cfg(feature = "full")]
    Templates {
        /// Template subcommand
        #[command(subcommand)]
        command: TemplateCommands,
    },

    /// Run user-defined lifecycle commands
    #[cfg(feature = "full")]
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
    #[cfg(feature = "full")]
    Doctor {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Create support bundle at specified path
        #[arg(long)]
        bundle: Option<PathBuf>,
    },

    /// Report outdated features (current | wanted | latest)
    ///
    /// Examples:
    ///   deacon outdated --workspace-folder .
    ///       # Human-readable table (default)
    ///   deacon outdated --output json
    ///       # Machine-readable JSON written to stdout (logs to stderr)
    ///   deacon outdated --output json --fail-on-outdated
    ///       # Exit with code 2 when any feature is outdated (CI gating)
    ///
    /// Output contracts: by default a text table is written to stdout; when
    /// `--output json` is specified a compact JSON map is written to stdout and
    /// all logs/diagnostic messages are sent to stderr. This ensures deterministic
    /// machine-readable behavior for CI and tooling.
    #[cfg(feature = "full")]
    Outdated {
        /// Workspace folder to inspect (default: current directory)
        #[arg(long, value_name = "PATH")]
        workspace_folder: Option<PathBuf>,
        /// Output format (text or json)
        #[arg(long, value_enum, default_value = "text")]
        output: OutputFormat,
        /// Fail CI with exit code 2 when any outdated feature is detected
        #[arg(long)]
        fail_on_outdated: bool,
    },
}

/// Feature management subcommands
#[cfg(feature = "full")]
#[derive(Debug, Clone, Subcommand)]
pub enum FeatureCommands {
    /// Test feature implementations
    Test {
        /// Path to feature collection directory to test (deprecated: use --project-folder instead)
        #[arg(value_name = "TARGET", default_value = ".")]
        target: String,
        /// Path to feature collection directory to test (overrides positional TARGET)
        #[arg(long, short = 'p')]
        project_folder: Option<String>,
        /// Specific features to test (can be specified multiple times)
        #[arg(long, short = 'f')]
        features: Vec<String>,
        /// Case-sensitive substring filter for scenario names
        #[arg(long)]
        filter: Option<String>,
        /// Only run global scenarios (_global directory)
        #[arg(long)]
        global_scenarios_only: bool,
        /// Skip scenario tests
        #[arg(long)]
        skip_scenarios: bool,
        /// Skip autogenerated tests
        #[arg(long)]
        skip_autogenerated: bool,
        /// Skip duplicate/idempotence tests
        #[arg(long)]
        skip_duplicated: bool,
        /// Permit randomization (not implemented yet)
        #[arg(long, hide = true)]
        permit_randomization: bool,
        /// Base image for test containers (default: ubuntu:focal)
        #[arg(long)]
        base_image: Option<String>,
        /// Remote user for test containers
        #[arg(long)]
        remote_user: Option<String>,
        /// Preserve test containers after execution
        #[arg(long)]
        preserve_test_containers: bool,
        /// Suppress non-error output
        #[arg(long)]
        quiet: bool,
        /// Log level (info, debug, trace)
        #[arg(long, value_enum, default_value = "info")]
        log_level: LogLevel,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
    /// Package features for distribution
    Package {
        /// Path to feature directory to package
        #[arg(default_value = ".")]
        path: String,
        /// Output directory for the package
        #[arg(long, default_value = "./output")]
        output: String,
        /// Force clean output folder before writing artifacts
        #[arg(long)]
        force_clean_output_folder: bool,
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
    ///
    /// Publishes a packaged feature to an OCI registry with semantic tags.
    /// Requires --namespace flag to specify the target namespace (owner/repo format).
    ///
    /// Authentication can be provided via environment variables:
    /// - DEACON_REGISTRY_TOKEN: Bearer token authentication
    /// - DEACON_REGISTRY_USER + DEACON_REGISTRY_PASS: Basic authentication
    /// - Docker config.json credentials are also supported
    Publish {
        /// Path to feature directory to publish
        path: String,
        /// Target registry URL
        #[arg(long, default_value = "ghcr.io")]
        registry: String,
        /// Target namespace (owner/repo format)
        #[arg(long)]
        namespace: String,
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
        /// Output format (text or json)
        #[arg(long, value_enum, default_value = "text")]
        output_format: OutputFormat,
    },
    /// Generate feature installation plan
    ///
    /// Note: Variable substitution is not performed during planning; feature IDs are treated as opaque strings;
    /// option values pass through unchanged and are not normalized or transformed.
    Plan {
        /// Output in JSON format
        #[arg(long, default_value_t = true, action = ArgAction::Set)]
        json: bool,
        /// Additional features to install (JSON object map of id -> value/options)
        /// Accepts a JSON object like {"ghcr.io/devcontainers/node": "18", "git": true}
        #[arg(long)]
        additional_features: Option<String>,
    },
}

/// Template management subcommands
#[cfg(feature = "full")]
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
#[cfg(feature = "full")]
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

    /// Container-side data folder for user state inside the container
    #[arg(long, global = true)]
    pub container_data_folder: Option<PathBuf>,

    /// Container-side system data folder inside the container
    #[arg(long, global = true)]
    pub container_system_data_folder: Option<PathBuf>,

    /// Host-side user data folder for persistent user state
    #[arg(long, global = true)]
    pub user_data_folder: Option<PathBuf>,

    /// Container-side session data folder for temporary session state
    #[arg(long, global = true)]
    pub container_session_data_folder: Option<PathBuf>,

    /// Force PTY (pseudo-terminal) allocation for lifecycle exec commands when using JSON log format.
    ///
    /// This flag only takes effect when --log-format json is active. It allows interactive
    /// commands in lifecycle hooks (onCreate, postCreate, etc.) to behave correctly while
    /// maintaining structured JSON logs on stderr and machine-readable output on stdout.
    ///
    /// Precedence: CLI flag > DEACON_FORCE_TTY_IF_JSON environment variable > default (no PTY).
    ///
    /// Environment variable: DEACON_FORCE_TTY_IF_JSON
    /// - Truthy values (case-insensitive): true, 1, yes
    /// - Falsey values or unset: false, 0, no, or absent
    ///
    /// When disabled (default), lifecycle commands run without PTY allocation. This is suitable
    /// for non-interactive scripts and automated environments.
    #[arg(long, global = true)]
    pub force_tty_if_json: bool,

    /// Default user env probe mode (none|loginInteractiveShell|interactiveShell|loginShell)
    #[arg(
        long,
        global = true,
        value_enum,
        default_value = "login-interactive-shell"
    )]
    pub default_user_env_probe: DefaultUserEnvProbe,

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
    fn normalized_terminal_dimensions(&self) -> Result<Option<TerminalDimensions>> {
        TerminalDimensions::new(self.terminal_columns, self.terminal_rows)
    }

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
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn validate(&self) -> Result<()> {
        self.normalized_terminal_dimensions()?;
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
        // Normalize terminal dimensions once for downstream consumers
        let terminal_dimensions = self.normalized_terminal_dimensions()?;

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

        // Emit logs to help with testing and log-level verification
        tracing::debug!("CLI initialized with log level: {}", log_level);
        tracing::trace!("Trace-level logging enabled (probe)");

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
                id_label,
                remove_existing_container,
                expect_existing_container,
                prebuild,
                skip_post_create,
                skip_post_attach,
                skip_non_blocking_commands,
                default_user_env_probe,
                mount,
                remote_env,
                mount_workspace_git_root,
                workspace_mount_consistency,
                build_no_cache,
                cache_from,
                cache_to,
                buildkit,
                additional_features,
                prefer_cli_features,
                feature_install_order,
                skip_feature_auto_mapping,
                experimental_lockfile,
                experimental_frozen_lockfile,
                dotfiles_repository,
                dotfiles_install_command,
                dotfiles_target_path,
                omit_config_remote_env_from_metadata,
                omit_syntax_directive,
                include_configuration,
                include_merged_configuration,
                gpu_mode,
                update_remote_user_uid_default,
                ports_events,
                forward_ports,
                shutdown,
                container_name,
                ignore_host_requirements,
                env_file,
            }) => {
                use crate::commands::up::{execute_up, UpArgs};

                let args = UpArgs {
                    id_label,
                    remove_existing_container,
                    expect_existing_container,
                    prebuild,
                    skip_post_create,
                    skip_post_attach,
                    skip_non_blocking_commands,
                    default_user_env_probe: default_user_env_probe.into(),
                    mount,
                    remote_env,
                    mount_workspace_git_root,
                    workspace_mount_consistency,
                    build_no_cache,
                    cache_from,
                    cache_to,
                    buildkit,
                    skip_feature_auto_mapping,
                    experimental_lockfile,
                    experimental_frozen_lockfile,
                    dotfiles_repository,
                    dotfiles_install_command,
                    dotfiles_target_path,
                    omit_config_remote_env_from_metadata,
                    omit_syntax_directive,
                    include_configuration,
                    include_merged_configuration,
                    gpu_mode,
                    update_remote_user_uid_default,
                    ports_events,
                    shutdown,
                    forward_ports,
                    container_name,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                    override_config_path: self.override_config,
                    additional_features,
                    prefer_cli_features,
                    feature_install_order,
                    ignore_host_requirements,
                    progress_tracker: progress_tracker.clone(),
                    runtime: self.runtime.map(|r| r.into()),
                    redaction_config: redaction_config.clone(),
                    secret_registry: secret_registry.clone(),
                    secrets_files: self.secrets_file.clone(),
                    env_file,
                    docker_path: self.docker_path.clone(),
                    docker_compose_path: self.docker_compose_path.clone(),
                    container_data_folder: self.container_data_folder.clone(),
                    container_system_data_folder: self.container_system_data_folder.clone(),
                    user_data_folder: self.user_data_folder.clone(),
                    container_session_data_folder: self.container_session_data_folder.clone(),
                    terminal_dimensions,
                    force_tty_if_json: self.force_tty_if_json,
                };

                // Execute up and emit JSON output per contract (specs/001-up-gap-spec/contracts/up.md)
                match execute_up(args).await {
                    Ok(container_info) => {
                        // Build success result
                        let mut result = crate::commands::UpResult::success(
                            container_info.container_id,
                            container_info.remote_user,
                            container_info.remote_workspace_folder,
                        );

                        // Add compose project name if present
                        if let Some(project_name) = container_info.compose_project_name {
                            result = result.with_compose_project_name(project_name);
                        }

                        // Add effective mounts if present
                        if let Some(mounts) = container_info.effective_mounts {
                            result = result.with_effective_mounts(mounts);
                        }

                        // Add effective env if present
                        if let Some(env) = container_info.effective_env {
                            result = result.with_effective_env(env);
                        }

                        // Add profiles applied if present
                        if let Some(profiles) = container_info.profiles_applied {
                            result = result.with_profiles_applied(profiles);
                        }

                        // Add external volumes preserved if present
                        if let Some(volumes) = container_info.external_volumes_preserved {
                            result = result.with_external_volumes_preserved(volumes);
                        }

                        // Add configuration if requested
                        if let Some(config) = container_info.configuration {
                            result = result.with_configuration(config);
                        }

                        // Add merged configuration if requested
                        if let Some(merged_config) = container_info.merged_configuration {
                            result = result.with_merged_configuration(merged_config);
                        }

                        // Emit JSON to stdout
                        let json = serde_json::to_string_pretty(&result)?;
                        println!("{}", json);
                        Ok(())
                    }
                    Err(e) => {
                        // Map error to standardized JSON result
                        let result = crate::commands::UpResult::from_error(e);

                        // Emit JSON to stdout
                        let json = serde_json::to_string_pretty(&result)?;
                        println!("{}", json);

                        // Extract message and description for exit error
                        let error_text = if let crate::commands::UpResult::Error(ref err) = result {
                            if !err.description.is_empty() {
                                format!("{}\n{}", err.message, err.description)
                            } else {
                                err.message.clone()
                            }
                        } else {
                            "Unknown error".to_string()
                        };

                        // Return error to trigger exit code 1
                        Err(anyhow::anyhow!(error_text))
                    }
                }
            }
            #[cfg(feature = "full")]
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
                image_names,
                label,
                push,
                output,
                skip_feature_auto_mapping,
                skip_persisting_customizations_from_features,
                experimental_lockfile,
                experimental_frozen_lockfile,
                omit_syntax_directive,
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
                    override_config_path: self.override_config,
                    secrets_files: self.secrets_file.clone(),
                    additional_features,
                    prefer_cli_features,
                    feature_install_order,
                    ignore_host_requirements,
                    progress_tracker: progress_tracker.clone(),
                    redaction_config: redaction_config.clone(),
                    secret_registry: secret_registry.clone(),
                    env_file,
                    docker_path: self.docker_path.clone(),
                    terminal_dimensions,
                    image_names,
                    label,
                    push,
                    output,
                    skip_feature_auto_mapping,
                    skip_persisting_customizations_from_features,
                    experimental_lockfile,
                    experimental_frozen_lockfile,
                    omit_syntax_directive,
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
                env_file,
                default_user_env_probe,
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
                    env_file,
                    command,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                    override_config_path: self.override_config,
                    secrets_files: self.secrets_file.clone(),
                    docker_path: self.docker_path.clone(),
                    docker_compose_path: self.docker_compose_path.clone(),
                    // Thread global options
                    force_tty_if_json: self.force_tty_if_json,
                    default_user_env_probe: Some(default_user_env_probe.into()),
                    container_data_folder: self.container_data_folder.clone(),
                    container_system_data_folder: self.container_system_data_folder.clone(),
                    terminal_dimensions,
                };

                execute_exec(args).await
            }
            Some(Commands::ReadConfiguration {
                include_merged_configuration,
                include_features_configuration,
                container_id,
                id_label,
                mount_workspace_git_root,
                additional_features,
                skip_feature_auto_mapping,
                terminal_columns,
                terminal_rows,
                user_data_folder,
            }) => {
                use crate::commands::read_configuration::{
                    execute_read_configuration, ReadConfigurationArgs,
                };

                let args = ReadConfigurationArgs {
                    include_merged_configuration,
                    include_features_configuration,
                    container_id,
                    id_label,
                    mount_workspace_git_root,
                    additional_features,
                    skip_feature_auto_mapping,
                    docker_path: self.docker_path.clone(),
                    docker_compose_path: self.docker_compose_path.clone(),
                    user_data_folder,
                    terminal_columns,
                    terminal_rows,
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
            #[cfg(feature = "full")]
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
            #[cfg(feature = "full")]
            Some(Commands::Features { command }) => {
                use crate::commands::features::{execute_features, FeaturesArgs};

                // Check for unsupported JSON output mode on package command
                if let crate::cli::FeatureCommands::Package { .. } = &command {
                    if matches!(self.log_format, Some(LogFormat::Json)) {
                        return Err(anyhow::anyhow!(
                            "JSON output is not supported for features package"
                        ));
                    }
                }

                let args = FeaturesArgs {
                    command,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                    override_config_path: self.override_config,
                    secrets_files: self.secrets_file.clone(),
                };

                execute_features(args).await
            }
            #[cfg(feature = "full")]
            Some(Commands::Templates { command }) => {
                use crate::commands::templates::{execute_templates, TemplatesArgs};

                let args = TemplatesArgs {
                    command,
                    workspace_folder: self.workspace_folder,
                    config_path: self.config,
                };

                execute_templates(args).await
            }
            #[cfg(feature = "full")]
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
                    container_data_folder: self.container_data_folder.clone(),
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
            #[cfg(feature = "full")]
            Some(Commands::Outdated {
                workspace_folder,
                output,
                fail_on_outdated,
            }) => {
                use crate::commands::outdated::{run as run_outdated, OutdatedArgs};

                // Determine workspace folder precedence: explicit flag -> global flag -> current_dir
                let wf = if let Some(wf) = workspace_folder {
                    wf
                } else if let Some(global_wf) = self.workspace_folder.clone() {
                    global_wf
                } else {
                    std::env::current_dir()?
                };

                let args = OutdatedArgs {
                    workspace_folder: wf.to_string_lossy().to_string(),
                    config: self.config.clone(),
                    override_config: self.override_config.clone(),
                    output: output.clone(),
                    fail_on_outdated,
                };

                run_outdated(args).await
            }

            #[cfg(feature = "full")]
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
    #[cfg(feature = "full")]
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
