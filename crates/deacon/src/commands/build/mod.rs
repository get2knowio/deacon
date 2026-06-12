//! Build command implementation
//!
//! Implements the `deacon build` subcommand for building DevContainer images.
//! Follows the CLI specification for Docker integration.

pub mod result;

use crate::cli::{BuildKitOption, OutputFormat};
use crate::commands::shared::build_resolution::resolve_devcontainer_build_config;
use crate::commands::shared::{ConfigLoadArgs, TerminalDimensions, load_config};
use anyhow::{Context, Result, anyhow};
use deacon_core::config::DevContainerConfig;
use deacon_core::errors::{DeaconError, DockerError};
use deacon_core::features::{FeatureMergeConfig, FeatureMerger};
use deacon_core::host_ca::{CorporateCaSet, discover_corporate_set};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{debug, info, instrument, warn};

/// Build command arguments
#[derive(Debug, Clone)]
pub struct BuildArgs {
    pub no_cache: bool,
    pub platform: Option<String>,
    pub build_arg: Vec<String>,
    pub force: bool,
    pub output_format: OutputFormat,
    pub cache_from: Vec<String>,
    pub cache_to: Vec<String>,
    pub buildkit: Option<BuildKitOption>,
    pub secret: Vec<String>,
    pub build_secret: Vec<String>,
    pub ssh: Vec<String>,
    pub scan_image: bool,
    pub fail_on_scan: bool,
    pub workspace_folder: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub override_config_path: Option<PathBuf>,
    pub secrets_files: Vec<PathBuf>,
    pub additional_features: Option<String>,
    pub prefer_cli_features: bool,
    pub feature_install_order: Option<String>,
    pub ignore_host_requirements: bool,
    pub progress_tracker:
        std::sync::Arc<std::sync::Mutex<Option<deacon_core::progress::ProgressTracker>>>,
    pub redaction_config: deacon_core::redaction::RedactionConfig,
    pub secret_registry: deacon_core::redaction::SecretRegistry,
    #[allow(dead_code)] // Build command doesn't yet support compose configurations
    pub env_file: Vec<PathBuf>,
    #[allow(dead_code)] // Future: Will be used for custom docker executable path
    pub docker_path: String,
    /// Optional terminal dimension hint for output formatting
    #[allow(dead_code)] // Future: Will be used for terminal output formatting
    pub terminal_dimensions: Option<TerminalDimensions>,
    /// Image names to apply as tags
    pub image_names: Vec<String>,
    /// Metadata labels to apply in key=value format
    pub label: Vec<String>,
    /// Push image to registry after build
    pub push: bool,
    /// Export image to file or directory
    pub output: Option<String>,
    /// Skip feature auto-mapping (hidden testing flag).
    pub skip_feature_auto_mapping: bool,
    /// Skip lockfile generation and verification (graduated 1.0).
    /// Currently not yet consumed by `build`'s lockfile-writing path —
    /// PR-4c (#41) hardcoded "always write when features present". Plumb
    /// this into `apply_features_and_lockfile` when wiring the gate.
    #[allow(dead_code)]
    pub no_lockfile: bool,
    /// Require an up-to-date lockfile; fail if resolution would change it.
    /// Same follow-up as `no_lockfile`: the byte-compare frozen behavior
    /// lives in `up::helpers::handle_lockfile_post_build` and needs to be
    /// lifted into a shared helper before `build` can consume it.
    #[allow(dead_code)]
    pub frozen_lockfile: bool,

    /// Resolved host-CA injection activation (016). Resolved at the CLI tier
    /// from `--inject-host-ca` > `DEACON_INJECT_HOST_CA` > `settings.json`
    /// (never the workspace — FR-015).
    pub host_ca_activation: deacon_core::host_ca::HostCaActivation,
}

impl Default for BuildArgs {
    fn default() -> Self {
        Self {
            no_cache: false,
            platform: None,
            build_arg: Vec::new(),
            force: false,
            output_format: OutputFormat::Text,
            cache_from: Vec::new(),
            cache_to: Vec::new(),
            buildkit: None,
            secret: Vec::new(),
            build_secret: Vec::new(),
            ssh: Vec::new(),
            scan_image: false,
            fail_on_scan: false,
            workspace_folder: None,
            config_path: None,
            override_config_path: None,
            secrets_files: Vec::new(),
            additional_features: None,
            prefer_cli_features: false,
            feature_install_order: None,
            ignore_host_requirements: false,
            progress_tracker: std::sync::Arc::new(std::sync::Mutex::new(None)),
            redaction_config: deacon_core::redaction::RedactionConfig::default(),
            secret_registry: deacon_core::redaction::SecretRegistry::new(),
            env_file: Vec::new(),
            docker_path: "docker".to_string(),
            terminal_dimensions: None,
            image_names: Vec::new(),
            label: Vec::new(),
            push: false,
            output: None,
            skip_feature_auto_mapping: false,
            no_lockfile: false,
            frozen_lockfile: false,
            host_ca_activation: deacon_core::host_ca::HostCaActivation::Off,
        }
    }
}

/// Build secret source type
#[derive(Debug, Clone, PartialEq)]
pub enum BuildSecretSource {
    /// Read secret from file
    File(PathBuf),
    /// Read secret from environment variable
    Env(String),
    /// Read secret from stdin
    Stdin,
}

/// Parsed build secret specification
#[derive(Debug, Clone)]
pub struct BuildSecret {
    /// Secret identifier (required)
    pub id: String,
    /// Secret source
    pub source: BuildSecretSource,
}

impl BuildSecret {
    /// Parse a build secret specification string
    ///
    /// Accepts formats:
    /// - `id=myid,src=/path/to/file`
    /// - `id=myid,env=ENV_VAR`
    /// - `id=myid` (stdin)
    pub fn parse(spec: &str) -> Result<Self> {
        let mut id: Option<String> = None;
        let mut src: Option<PathBuf> = None;
        let mut env: Option<String> = None;
        let mut stdin_flag: bool = false;

        // Parse key=value pairs and standalone flags
        for part in spec.split(',') {
            let part = part.trim();
            let kv: Vec<&str> = part.splitn(2, '=').collect();

            if kv.len() == 1 {
                // Standalone flag (no '=' found)
                match part {
                    "value-stdin" | "stdin" => {
                        stdin_flag = true;
                    }
                    _ => {
                        return Err(anyhow!(
                            "Unknown build secret parameter '{}'. Valid parameters are: id, src, env, value-stdin, stdin",
                            part
                        ));
                    }
                }
            } else {
                // Key=value pair
                let key = kv[0].trim();
                let value = kv[1].trim();

                match key {
                    "id" => {
                        if value.is_empty() {
                            return Err(anyhow!("Build secret id cannot be empty"));
                        }
                        id = Some(value.to_string());
                    }
                    "src" => {
                        if value.is_empty() {
                            return Err(anyhow!("Build secret src cannot be empty"));
                        }
                        src = Some(PathBuf::from(value));
                    }
                    "env" => {
                        if value.is_empty() {
                            return Err(anyhow!("Build secret env cannot be empty"));
                        }
                        env = Some(value.to_string());
                    }
                    _ => {
                        return Err(anyhow!(
                            "Unknown build secret parameter '{}'. Valid parameters are: id, src, env",
                            key
                        ));
                    }
                }
            }
        }

        // Validate required id
        let id = id.ok_or_else(|| anyhow!("Build secret must specify 'id' parameter"))?;

        // Validate that stdin_flag is not mixed with src or env
        if stdin_flag && (src.is_some() || env.is_some()) {
            return Err(anyhow!(
                "Build secret cannot specify 'value-stdin' or 'stdin' flag with 'src' or 'env' parameters"
            ));
        }

        // Determine source - prioritize in order: src, env, stdin (default or explicit)
        let source = if let Some(path) = src {
            if env.is_some() {
                return Err(anyhow!(
                    "Build secret cannot specify both 'src' and 'env' parameters"
                ));
            }
            BuildSecretSource::File(path)
        } else if let Some(env_var) = env {
            BuildSecretSource::Env(env_var)
        } else {
            BuildSecretSource::Stdin
        };

        Ok(Self { id, source })
    }

    /// Validate that the secret source is accessible
    pub fn validate(&self) -> Result<()> {
        match &self.source {
            BuildSecretSource::File(path) => {
                if !path.exists() {
                    return Err(anyhow!(
                        "Build secret file '{}' does not exist",
                        path.display()
                    ));
                }
                if !path.is_file() {
                    return Err(anyhow!(
                        "Build secret path '{}' is not a file",
                        path.display()
                    ));
                }
                // Check if file is readable
                std::fs::metadata(path)
                    .with_context(|| format!("Cannot read secret file '{}'", path.display()))?;
                Ok(())
            }
            BuildSecretSource::Env(env_var) => {
                if std::env::var(env_var).is_err() {
                    return Err(anyhow!(
                        "Build secret environment variable '{}' is not set",
                        env_var
                    ));
                }
                Ok(())
            }
            BuildSecretSource::Stdin => {
                // Stdin validation happens at read time
                Ok(())
            }
        }
    }

    /// Read the secret value from its source
    ///
    /// Returns the secret value as a string. The caller is responsible for
    /// registering the value with the redaction system.
    pub async fn read_value(&self) -> Result<String> {
        match &self.source {
            BuildSecretSource::File(path) => {
                let value = tokio::fs::read_to_string(path)
                    .await
                    .with_context(|| format!("Failed to read secret from '{}'", path.display()))?;
                Ok(value.trim().to_string())
            }
            BuildSecretSource::Env(env_var) => {
                let value = std::env::var(env_var).with_context(|| {
                    format!(
                        "Failed to read secret from environment variable '{}'",
                        env_var
                    )
                })?;
                Ok(value)
            }
            BuildSecretSource::Stdin => {
                use tokio::io::AsyncBufReadExt;
                let stdin = tokio::io::stdin();
                let mut reader = tokio::io::BufReader::new(stdin);
                let mut line = String::new();
                reader
                    .read_line(&mut line)
                    .await
                    .context("Failed to read secret from stdin")?;
                Ok(line.trim().to_string())
            }
        }
    }

    /// Convert to Docker build argument format
    ///
    /// For file sources, returns the id and file path.
    /// For env/stdin sources, this requires the secret to be written to a temp file first.
    pub fn to_docker_arg(&self, temp_file: Option<&Path>) -> String {
        match &self.source {
            BuildSecretSource::File(path) => {
                format!("id={},src={}", self.id, path.display())
            }
            BuildSecretSource::Env(_) | BuildSecretSource::Stdin => {
                if let Some(temp_path) = temp_file {
                    format!("id={},src={}", self.id, temp_path.display())
                } else {
                    // Fallback - should not happen if properly handled
                    format!("id={}", self.id)
                }
            }
        }
    }
}

/// Build configuration extracted from DevContainer config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    /// Dockerfile path as specified by config
    pub dockerfile: String,
    /// Resolved Dockerfile path
    pub dockerfile_path: PathBuf,
    /// Build context path
    pub context: String,
    /// Directory containing the active devcontainer config
    pub context_folder: PathBuf,
    /// Build target (optional)
    pub target: Option<String>,
    /// Build options/args
    pub options: HashMap<String, String>,
}

/// Build result summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResult {
    /// Built image ID
    pub image_id: String,
    /// Image tags
    pub tags: Vec<String>,
    /// Build duration in seconds
    pub build_duration: f64,
    /// Image metadata/labels
    pub metadata: HashMap<String, String>,
    /// Configuration hash for caching
    pub config_hash: String,
    /// Subject DNs of corporate CAs injected at build time (016, FR-028).
    /// Additive; omitted (and defaulted on read) when injection was off or
    /// yielded zero certs so the default output stays byte-stable (FR-029).
    #[serde(
        rename = "injectedCaSubjects",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub injected_ca_subjects: Vec<String>,
}

/// Build metadata stored in cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildMetadata {
    /// Configuration hash
    pub config_hash: String,
    /// Build result
    pub result: BuildResult,
    /// Build inputs summary
    pub inputs: BuildInputs,
    /// When the build was created
    pub created_at: u64,
}

/// Build inputs tracked for cache invalidation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildInputs {
    /// Dockerfile content hash
    pub dockerfile_hash: String,
    /// Build context files that affect the build
    pub context_files: Vec<ContextFile>,
    /// Feature set digest (if applicable)
    pub feature_set_digest: Option<String>,
    /// Build configuration
    pub build_config: BuildConfig,
}

/// A file in the build context that affects the build
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFile {
    /// Relative path from workspace root
    pub path: String,
    /// File size in bytes
    pub size: u64,
    /// Last modified time (seconds since UNIX epoch)
    pub mtime: u64,
}

/// Helper function to validate BuildKit availability with consistent error handling
async fn validate_buildkit_requirement(
    output_format: &OutputFormat,
    feature_name: &str,
    flag_name: &str,
) -> Result<()> {
    match deacon_core::build::buildkit::is_buildkit_available().await {
        Ok(true) => {
            // BuildKit available, proceed
            Ok(())
        }
        Ok(false) => {
            let error = result::BuildError::with_description(
                format!("BuildKit is required for {}", flag_name),
                format!("Enable BuildKit or remove {} flag", flag_name),
            );
            if matches!(output_format, OutputFormat::Json) {
                println!("{}", serde_json::to_string(&error)?);
            } else {
                eprintln!("Error: {}", error.message());
                if let Some(desc) = error.description() {
                    eprintln!("{}", desc);
                }
            }
            Err(anyhow!("BuildKit is required for {}", feature_name))
        }
        Err(e) => {
            // Failed to detect BuildKit
            Err(anyhow!("Failed to detect BuildKit: {}", e))
        }
    }
}

/// Execute the build command.
///
/// Loads the DevContainer configuration (from the provided path or by discovery),
/// validates host requirements, applies feature merges from CLI flags, and
/// derives a build configuration. It computes a deterministic configuration
/// hash, optionally uses a cached build result (unless `force` is set), and
/// performs a Docker build when needed. Progress events (BuildBegin / BuildEnd)
/// are emitted to the configured progress tracker and the build duration is
/// recorded. The final `BuildResult` is cached and printed in the requested
/// output format.
///
/// Errors are returned if configuration loading or validation fails, or if the
/// underlying build (Docker) fails when that feature is enabled.
///
/// # Examples
///
/// ```no_run
/// use deacon::commands::build::execute_build;
/// use deacon::commands::build::BuildArgs;
///
/// // Run the build in an async context (example uses Tokio).
/// #[tokio::main]
/// async fn main() {
///     let args = BuildArgs::default();
///     let _ = execute_build(args).await;
/// }
/// ```
#[instrument(skip(args))]
pub async fn execute_build(mut args: BuildArgs) -> Result<()> {
    info!("Starting build command execution");
    debug!("Build args: {:?}", args);

    // Initialize progress tracking
    let emit_progress_event =
        crate::commands::shared::progress::make_progress_callback(&args.progress_tracker);

    // Parse and validate labels from key=value format
    let parsed_labels: Result<Vec<(String, String)>> = args
        .label
        .iter()
        .map(|label_str| {
            let parts: Vec<&str> = label_str.splitn(2, '=').collect();
            if parts.len() != 2 {
                Err(anyhow!(
                    "Invalid label format '{}'. Expected key=value",
                    label_str
                ))
            } else {
                // Validate label name
                deacon_core::docker::validate_label_name(parts[0])
                    .with_context(|| format!("Invalid label name in '{}'", label_str))?;
                Ok((parts[0].to_string(), parts[1].to_string()))
            }
        })
        .collect();
    let labels = parsed_labels?;

    // Validate image names
    for image_name in &args.image_names {
        deacon_core::docker::validate_image_tag(image_name)
            .with_context(|| format!("Invalid image name: {}", image_name))?;
    }

    // Drop duplicate `--image-name` values, preserving first-seen order. Passing
    // the same tag twice is harmless to Docker (which dedups `-t`), but without
    // this the emitted `imageName` array would echo the duplicate back to
    // callers. Normalizing here keeps the result JSON clean for every path.
    {
        let mut seen = std::collections::HashSet::new();
        args.image_names.retain(|name| seen.insert(name.clone()));
    }

    // Validate push/output mutual exclusivity early
    if args.push && args.output.is_some() {
        let error = result::BuildError::with_description(
            "Cannot use both --push and --output",
            "They are mutually exclusive. Use --push to push to registry or --output to export locally",
        );
        if matches!(args.output_format, OutputFormat::Json) {
            println!("{}", serde_json::to_string(&error)?);
        } else {
            eprintln!("Error: {}", error.message());
            if let Some(desc) = error.description() {
                eprintln!("{}", desc);
            }
        }
        return Err(anyhow!("Push and output are mutually exclusive"));
    }

    // Validate BuildKit requirements for --push
    if args.push {
        validate_buildkit_requirement(&args.output_format, "push", "--push").await?;
    }

    // Validate BuildKit requirements for --output
    if args.output.is_some() {
        validate_buildkit_requirement(&args.output_format, "output", "--output").await?;
    }

    // Validate BuildKit requirements for --platform
    if args.platform.is_some() {
        validate_buildkit_requirement(&args.output_format, "platform", "--platform").await?;
    }

    // Validate BuildKit requirements for --cache-to
    if !args.cache_to.is_empty() {
        validate_buildkit_requirement(&args.output_format, "cache-to", "--cache-to").await?;
    }

    // Load configuration using shared helper for consistency with up/exec
    let load_result = load_config(ConfigLoadArgs {
        workspace_folder: args.workspace_folder.as_deref(),
        config_path: args.config_path.as_deref(),
        override_config_path: args.override_config_path.as_deref(),
        secrets_files: &args.secrets_files,
    })
    .await?;

    let mut config = load_result.config;
    let workspace_folder = load_result.workspace_folder;
    let config_path = load_result.config_path;

    debug!("Loaded configuration: {:?}", config.name);

    // Validate compose mode restrictions
    if config.uses_compose() {
        let unsupported_flags = [
            (args.push, "--push"),
            (args.output.is_some(), "--output"),
            (!args.cache_to.is_empty(), "--cache-to"),
            (args.platform.is_some(), "--platform"),
        ];

        for (flag_active, flag_name) in unsupported_flags {
            if flag_active {
                let error = result::BuildError::with_description(
                    format!(
                        "Cannot use {} with Docker Compose configurations",
                        flag_name
                    ),
                    "Docker Compose does not support this flag during build",
                );
                if matches!(args.output_format, OutputFormat::Json) {
                    println!("{}", serde_json::to_string(&error)?);
                } else {
                    eprintln!("Error: {}", error.message());
                    if let Some(desc) = error.description() {
                        eprintln!("{}", desc);
                    }
                }
                return Err(anyhow!(
                    "{} is not supported with Docker Compose configurations",
                    flag_name
                ));
            }
        }
    }

    // Validate host requirements if specified in configuration
    if let Some(host_requirements) = &config.host_requirements {
        debug!("Validating host requirements");
        let mut evaluator = deacon_core::host_requirements::HostRequirementsEvaluator::new();

        // Advisory per spec: the evaluator warns when unmet and proceeds (never
        // errors); --ignore-host-requirements downgrades the warning to debug.
        match evaluator.validate_requirements(
            host_requirements,
            Some(&workspace_folder),
            args.ignore_host_requirements,
        ) {
            Ok(evaluation) => {
                if evaluation.requirements_met {
                    debug!("Host requirements validation passed");
                }
                debug!("Host evaluation: {:?}", evaluation);
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    } else {
        debug!("No host requirements specified in configuration");
    }

    // Apply feature merging if CLI features are provided
    if args.additional_features.is_some() || args.feature_install_order.is_some() {
        let merge_config = FeatureMergeConfig::new(
            args.additional_features.clone(),
            args.prefer_cli_features,
            args.feature_install_order.clone(),
            args.skip_feature_auto_mapping,
        );

        // Merge features
        config.features = FeatureMerger::merge_features(&config.features, &merge_config)?;
        debug!("Applied feature merging");

        // Update override feature install order if provided
        if let Some(effective_order) = FeatureMerger::get_effective_install_order(
            config.override_feature_install_order.as_ref(),
            &merge_config,
        )? {
            config.override_feature_install_order = Some(effective_order);
            debug!("Updated feature install order");
        }
    }

    // Extract build configuration
    let build_config = extract_build_config(&config, &config_path)?;
    debug!("Build config: {:?}", build_config);

    // Calculate configuration hash for caching
    let config_hash = calculate_config_hash(&build_config, &workspace_folder)?;
    debug!("Configuration hash: {}", config_hash);

    // PR-4c: feature installation during build.
    //
    // Feature installation is supported for all configuration shapes:
    // - Dockerfile and image-reference builds layer features via a post-build
    //   pass (run the base build → `deacon-build:<hash>` tagged image →
    //   `build_image_with_features` FROMs that tag). See below.
    // - Compose builds resolve the target service's shape and build a
    //   feature-extended image via `execute_compose_build_with_features`
    //   (the same `resolve_compose_feature_image` helper the `up` flow uses).
    let features_present = !config.features.is_null()
        && config
            .features
            .as_object()
            .is_some_and(|obj| !obj.is_empty());

    // Check cache if not forced (skip cache if pushing or exporting).
    // When features are present we deliberately skip the cache check: the
    // current `config_hash` does not include feature digests, so a cached
    // hit would point at a base image without the feature layers.
    // Re-running keeps correctness; a future refinement can fold the
    // feature digests into the hash for proper caching.
    if !args.force && !args.push && args.output.is_none() && !features_present {
        if let Some(cached_result) = check_build_cache(&config_hash, &workspace_folder).await? {
            info!("Using cached build result");
            output_result(
                &cached_result,
                &args.output_format,
                &args.redaction_config,
                &args.secret_registry,
                false,
                None,
            )?;
            return Ok(());
        }
    }

    // Execute build
    let build_start_time = Instant::now();

    // Emit build begin event
    emit_progress_event(deacon_core::progress::ProgressEvent::BuildBegin {
        id: deacon_core::progress::ProgressTracker::next_event_id(),
        timestamp: deacon_core::progress::ProgressTracker::current_timestamp(),
        context: build_config.context.clone(),
        dockerfile: Some(build_config.dockerfile.clone()),
    })?;

    // Host-CA discovery for build-time injection (016, T039). Activation is
    // resolved at the CLI tier from machine-owner sources only (CLI flag > env >
    // settings) — never from the workspace config (FR-015); see the guard in
    // `resolve_host_ca_activation_cli`. Discover once and reuse for whichever
    // feature-layering path runs. An empty set means "nothing to inject".
    let host_ca_set: Option<CorporateCaSet> = if args.host_ca_activation.is_enabled() {
        let span = tracing::info_span!("ca.discover", mode = args.host_ca_activation.mode_str());
        let _guard = span.enter();
        let set = discover_corporate_set(&args.host_ca_activation)?;
        if set.is_empty() { None } else { Some(set) }
    } else {
        None
    };
    // FR-018a: build-time injection only happens when deacon generates a
    // feature-layering Dockerfile. Shapes without features (image-only,
    // compose-without-features, plain Dockerfile-without-features) generate no
    // such Dockerfile, so log the skip — runtime injection (`deacon up`) covers
    // them.
    if host_ca_set.is_some() && !features_present {
        info!(
            "Build-time host-CA injection skipped: this config shape generates no \
             feature-layering Dockerfile (FR-018a). Use `deacon up` for runtime injection."
        );
    }

    // Dispatch to appropriate build function based on configuration type
    let result = if config.uses_compose() {
        if features_present {
            // Compose + features: build the feature-extended image for the target
            // service directly (shape-aware), tag it, and write the lockfile.
            execute_compose_build_with_features(
                &config,
                &args,
                &workspace_folder,
                &config_path,
                &labels,
                &config_hash,
                host_ca_set.as_ref(),
            )
            .await
        } else {
            execute_compose_build(
                &config,
                &args,
                &workspace_folder,
                &config_path,
                &labels,
                &config_hash,
            )
            .await
        }
    } else if config.image.is_some() {
        execute_image_reference_build(&config, &args, &workspace_folder, &labels).await
    } else {
        execute_docker_build(
            &build_config,
            &args,
            &config_hash,
            &workspace_folder,
            &labels,
        )
        .await
    };
    let build_duration = build_start_time.elapsed();

    // Emit build end event
    let build_success = result.is_ok();
    let image_id = result.as_ref().ok().map(|r| r.image_id.clone());

    emit_progress_event(deacon_core::progress::ProgressEvent::BuildEnd {
        id: deacon_core::progress::ProgressTracker::next_event_id(),
        timestamp: deacon_core::progress::ProgressTracker::current_timestamp(),
        context: build_config.context.clone(),
        duration_ms: build_duration.as_millis() as u64,
        success: build_success,
        image_id,
    })?;

    let result = result?;

    // Record metrics
    if let Ok(tracker_guard) = args.progress_tracker.lock() {
        if let Some(tracker) = tracker_guard.as_ref() {
            tracker.record_duration("build", build_duration);
        }
    }

    // PR-4c: post-build feature install. When features are present we run a
    // second BuildKit pass that FROMs the just-built image and layers the
    // feature install stages on top, then write the lockfile next to the
    // config file. This matches `up`'s feature flow (`build_image_with_features`)
    // which also returns a `Lockfile` ready for `write_lockfile`.
    // Compose + features is fully handled in the dispatch above
    // (`execute_compose_build_with_features` already built, tagged, and wrote the
    // lockfile), so only the Dockerfile / image-reference shapes need this
    // generic post-build layering pass.
    let (image_id, feature_lockfile) = if features_present && !config.uses_compose() {
        // Layer features on top of the just-built image. We pass a real tag
        // (the deterministic `deacon-build:<hash>` tag, always applied by
        // `execute_docker_build`) rather than the bare `sha256:...` image ID:
        // the feature-install Dockerfile uses `FROM ${_DEV_CONTAINERS_BASE_IMAGE}`,
        // and BuildKit interprets a bare `sha256:<digest>` as the remote repo
        // `docker.io/library/sha256:<digest>` (pull-access-denied), whereas a
        // local tag resolves to the just-built image.
        let base_ref = result
            .tags
            .first()
            .cloned()
            .unwrap_or_else(|| result.image_id.clone());
        let (feature_image, lockfile) = apply_features_and_lockfile(
            &config,
            &base_ref,
            &workspace_folder,
            &config_path,
            host_ca_set.as_ref(),
        )
        .await?;

        // Re-point the base build's tags (the deterministic `deacon-build:<hash>`
        // tag plus any `--image-name`s) at the feature-extended image. Without
        // this, `--image-name` would still resolve to the pre-feature base image
        // and the installed features would be invisible to consumers that pull
        // the named tag.
        for tag in &result.tags {
            retag_image(&feature_image, tag).await?;
        }

        (feature_image, lockfile)
    } else {
        (result.image_id, None)
    };

    let final_result = BuildResult {
        image_id,
        tags: result.tags,
        build_duration: build_duration.as_secs_f64(),
        metadata: result.metadata,
        config_hash: config_hash.clone(),
        injected_ca_subjects: host_ca_set
            .as_ref()
            .map(|s| s.subjects.clone())
            .unwrap_or_default(),
    };

    if let Some(path) = feature_lockfile {
        debug!("Wrote feature lockfile to '{}'", path.display());
    }

    // Cache the result
    cache_build_result(&final_result, &workspace_folder).await?;

    // Execute vulnerability scan if requested
    if args.scan_image {
        let scan_success =
            execute_vulnerability_scan(&args, &final_result.image_id, &emit_progress_event).await?;
        if !scan_success && args.fail_on_scan {
            return Err(anyhow::anyhow!(
                "Vulnerability scan failed and --fail-on-scan was set"
            ));
        }
    }

    // Output result
    output_result(
        &final_result,
        &args.output_format,
        &args.redaction_config,
        &args.secret_registry,
        args.push,
        args.output.as_deref(),
    )?;

    // Output final summary in debug mode
    if let Ok(tracker_guard) = args.progress_tracker.lock() {
        if let Some(tracker) = tracker_guard.as_ref() {
            if let Some(metrics_summary) = tracker.metrics_summary() {
                debug!("Metrics summary: {:?}", metrics_summary);
            }
        }
    }

    info!("Build command completed successfully");
    Ok(())
}

/// Extract build configuration from DevContainer config
fn extract_build_config(config: &DevContainerConfig, config_path: &Path) -> Result<BuildConfig> {
    let config_folder = config_path.parent().unwrap_or_else(|| Path::new("."));

    // Check if this is a compose-based configuration
    if config.uses_compose() {
        // For compose mode, we use the service name as a placeholder
        // Actual compose build will be handled by execute_compose_build
        let service = config.service.as_ref().ok_or_else(|| {
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: "Docker Compose configuration must specify a service".to_string(),
            })
        })?;

        return Ok(BuildConfig {
            dockerfile: format!("compose-service-{}", service),
            dockerfile_path: config_folder.join(format!("compose-service-{}", service)),
            context: ".".to_string(),
            context_folder: config_folder.to_path_buf(),
            target: None,
            options: HashMap::new(),
        });
    }

    if let Some(resolved) = resolve_devcontainer_build_config(config, config_path)? {
        return Ok(BuildConfig {
            dockerfile: resolved.dockerfile,
            dockerfile_path: resolved.dockerfile_path,
            context: resolved.context,
            context_folder: resolved.context_folder,
            target: resolved.target,
            options: resolved.options,
        });
    }

    if let Some(image) = &config.image {
        // For image-reference mode, create a build config that will generate a Dockerfile
        // Actual image-reference build will be handled by execute_image_reference_build
        Ok(BuildConfig {
            dockerfile: format!("image-reference-{}", image.replace([':', '/'], "-")),
            dockerfile_path: config_folder.join(format!(
                "image-reference-{}",
                image.replace([':', '/'], "-")
            )),
            context: ".".to_string(),
            context_folder: config_folder.to_path_buf(),
            target: None,
            options: HashMap::new(),
        })
    } else {
        // No dockerfile or image specified
        Err(
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: "No 'dockerFile' or 'image' specified in configuration".to_string(),
            })
            .into(),
        )
    }
}

/// Calculate configuration hash for caching
fn calculate_config_hash(build_config: &BuildConfig, _workspace_folder: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();

    // Hash the build config
    hasher.update(build_config.dockerfile.as_bytes());
    hasher.update(build_config.context.as_bytes());
    if let Some(target) = &build_config.target {
        hasher.update(target.as_bytes());
    }

    // Hash the options in a deterministic order
    let mut options: Vec<_> = build_config.options.iter().collect();
    options.sort_by_key(|(k, _)| *k);
    for (key, value) in options {
        hasher.update(key.as_bytes());
        hasher.update(value.as_bytes());
    }

    // Hash dockerfile content
    let dockerfile_path = build_config.dockerfile_path.clone();
    if dockerfile_path.exists() {
        let dockerfile_content = std::fs::read_to_string(&dockerfile_path)?;
        hasher.update(dockerfile_content.as_bytes());
    }

    // Hash selected build context files (limit count for performance)
    let context_path = build_config.context_folder.join(&build_config.context);
    if context_path.exists() {
        let mut build_affecting_files = Vec::new();

        // Collect files that affect the build, excluding non-affecting ones like README
        // Use a breadth-first, deterministic traversal
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(context_path.clone());

        while let Some(dir) = queue.pop_front() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                let mut entries: Vec<_> = entries.flatten().collect();
                entries.sort_by_key(|e| e.path());

                // Process files first for this directory level
                for entry in &entries {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                            if !is_non_build_affecting_file(file_name) {
                                if let Ok(metadata) = std::fs::metadata(&path) {
                                    build_affecting_files.push((
                                        path.strip_prefix(&context_path)
                                            .unwrap_or(&path)
                                            .to_string_lossy()
                                            .to_string(),
                                        metadata.len(),
                                        metadata
                                            .modified()
                                            .unwrap_or(std::time::UNIX_EPOCH)
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_secs(),
                                    ));
                                }
                            }
                        }
                    }
                    if build_affecting_files.len() >= 50 {
                        break;
                    }
                }

                // Then add directories to queue for next level processing
                if build_affecting_files.len() < 50 {
                    for entry in &entries {
                        let path = entry.path();
                        if path.is_dir() {
                            // Skip cache directories and other non-build-affecting directories
                            if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                                if !is_non_build_affecting_directory(dir_name) {
                                    queue.push_back(path);
                                }
                            }
                        }
                    }
                }
            }
            if build_affecting_files.len() >= 50 {
                break;
            }
        }

        // Sort for deterministic hashing
        build_affecting_files.sort();
        for (path, size, mtime) in build_affecting_files {
            hasher.update(path.as_bytes());
            hasher.update(size.to_le_bytes());
            hasher.update(mtime.to_le_bytes());
        }
    }

    let hash = hasher.finalize();
    // Use first 16 hex chars for consistency with previous format
    Ok(format!(
        "{:016x}",
        u64::from_be_bytes(hash[0..8].try_into().unwrap())
    ))
}

/// Check if a file is unlikely to affect the build
fn is_non_build_affecting_file(filename: &str) -> bool {
    let filename_lower = filename.to_lowercase();
    matches!(
        filename_lower.as_str(),
        "readme"
            | "readme.md"
            | "readme.txt"
            | "readme.rst"
            | "changelog"
            | "changelog.md"
            | "changelog.txt"
            | "license"
            | "license.md"
            | "license.txt"
            | "authors"
            | "authors.md"
            | "authors.txt"
            | "contributors"
            | "contributors.md"
            | "contributors.txt"
            | ".gitignore"
            | ".gitattributes"
            | ".editorconfig"
            | ".vscode"
            | ".idea"
            | ".git"
    ) || filename_lower.ends_with(".md") && !filename_lower.contains("dockerfile")
}

/// Check if a directory is unlikely to affect the build
fn is_non_build_affecting_directory(dirname: &str) -> bool {
    let dirname_lower = dirname.to_lowercase();
    matches!(
        dirname_lower.as_str(),
        ".git"
            | ".vscode" 
            | ".idea"
            | ".devcontainer"  // DevContainer config and cache directory
            | "node_modules"
            | ".pytest_cache"
            | "__pycache__"
            | ".mypy_cache"
            | "build-cache"  // Our own build cache directory
            | ".next"
            | ".nuxt"
            | "target"  // Rust build directory
            | "dist"
            | "coverage"
    )
}

/// Check for cached build result
async fn check_build_cache(
    config_hash: &str,
    workspace_folder: &Path,
) -> Result<Option<BuildResult>> {
    let cache_file = get_build_cache_path(workspace_folder, config_hash);

    // Read cache file. NotFound is a normal cache-miss; other errors fall through too.
    let contents = match tokio::fs::read_to_string(&cache_file).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            debug!("No cache file found at {}", cache_file.display());
            return Ok(None);
        }
        Err(e) => {
            debug!("Failed to read cache file: {}", e);
            return Ok(None);
        }
    };

    match serde_json::from_str::<BuildMetadata>(&contents) {
        Ok(metadata) => {
            // Validate that the image still exists
            if is_image_available(&metadata.result.image_id).await? {
                debug!("Cache hit for config hash {}", config_hash);
                Ok(Some(metadata.result))
            } else {
                debug!(
                    "Cached image {} no longer available, invalidating cache",
                    metadata.result.image_id
                );
                let _ = tokio::fs::remove_file(&cache_file).await;
                Ok(None)
            }
        }
        Err(e) => {
            debug!("Failed to deserialize cache metadata: {}", e);
            let _ = tokio::fs::remove_file(&cache_file).await;
            Ok(None)
        }
    }
}

/// Cache build result
async fn cache_build_result(result: &BuildResult, workspace_folder: &Path) -> Result<()> {
    let cache_dir = get_build_cache_dir(workspace_folder);

    // Ensure cache directory exists
    if let Err(e) = tokio::fs::create_dir_all(&cache_dir).await {
        debug!("Failed to create cache directory: {}", e);
        return Ok(()); // Don't fail the build if caching fails
    }

    // Create build inputs for metadata
    let inputs = create_build_inputs(result, workspace_folder)?;

    let metadata = BuildMetadata {
        config_hash: result.config_hash.clone(),
        result: result.clone(),
        inputs,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    let cache_file = get_build_cache_path(workspace_folder, &result.config_hash);

    match serde_json::to_string_pretty(&metadata) {
        Ok(json) => {
            if let Err(e) = tokio::fs::write(&cache_file, json).await {
                debug!("Failed to write cache file: {}", e);
            } else {
                debug!("Cached build result to {}", cache_file.display());
            }
        }
        Err(e) => {
            debug!("Failed to serialize cache metadata: {}", e);
        }
    }

    Ok(())
}

/// Get the cache directory for builds
fn get_build_cache_dir(workspace_folder: &Path) -> PathBuf {
    workspace_folder.join(".devcontainer").join("build-cache")
}

/// Get the cache file path for a specific config hash
fn get_build_cache_path(workspace_folder: &Path, config_hash: &str) -> PathBuf {
    get_build_cache_dir(workspace_folder).join(format!("{}.json", config_hash))
}

/// Create build inputs for cache metadata
fn create_build_inputs(result: &BuildResult, _workspace_folder: &Path) -> Result<BuildInputs> {
    // For now, create a simplified version - full implementation would track more details
    let dockerfile_hash = result.config_hash.clone(); // Simplified
    let context_files = Vec::new(); // Would be populated from actual context scanning

    Ok(BuildInputs {
        dockerfile_hash,
        context_files,
        feature_set_digest: None, // TODO: Implement when features are integrated
        build_config: BuildConfig {
            dockerfile: "Dockerfile".to_string(), // Would be extracted from actual config
            dockerfile_path: PathBuf::from("Dockerfile"),
            context: ".".to_string(),
            context_folder: PathBuf::from("."),
            target: None,
            options: HashMap::new(),
        },
    })
}

/// Check if a Docker image is available locally
async fn is_image_available(image_id: &str) -> Result<bool> {
    // Use docker inspect to check if image exists
    let output = tokio::process::Command::new("docker")
        .args(["inspect", "--type=image", image_id])
        .output()
        .await;

    match output {
        Ok(output) => Ok(output.status.success()),
        Err(e) => {
            // If docker command fails, assume image is not available
            debug!("Failed to check image availability for {}: {}", image_id, e);
            Ok(false)
        }
    }
}

/// Detect if BuildKit should be used based on CLI flag and environment
fn should_use_buildkit(buildkit_option: Option<&BuildKitOption>) -> bool {
    match buildkit_option {
        Some(BuildKitOption::Auto) => {
            // Check DOCKER_BUILDKIT environment variable
            match std::env::var("DOCKER_BUILDKIT") {
                Ok(value) => value == "1" || value.to_lowercase() == "true",
                Err(_) => {
                    // Default to true for auto mode if no env var is set
                    // Modern Docker versions enable BuildKit by default
                    debug!("DOCKER_BUILDKIT not set, defaulting to BuildKit enabled for auto mode");
                    true
                }
            }
        }
        Some(BuildKitOption::Never) => false,
        None => {
            // Default behavior: respect DOCKER_BUILDKIT environment variable
            match std::env::var("DOCKER_BUILDKIT") {
                Ok(value) => value == "1" || value.to_lowercase() == "true",
                Err(_) => false, // Default to legacy build if no explicit setting
            }
        }
    }
}

/// Execute Compose build
#[instrument(skip(config, args, workspace_folder, labels))]
async fn execute_compose_build(
    config: &DevContainerConfig,
    args: &BuildArgs,
    workspace_folder: &Path,
    config_path: &Path,
    labels: &[(String, String)],
    config_hash: &str,
) -> Result<BuildResult> {
    use deacon_core::compose::ComposeManager;
    use std::time::Instant;

    let service = config
        .service
        .as_ref()
        .ok_or_else(|| anyhow!("Docker Compose configuration must specify a service"))?;

    info!("Building Docker Compose service: {}", service);

    let build_start = Instant::now();

    // Create compose project. Compose files resolve relative to the directory
    // containing devcontainer.json (spec parity), not the workspace folder.
    // Use the *resolved* config path (discovery may place it under
    // `.devcontainer/`); `args.config_path` is only the explicit `--config` flag.
    let compose_manager = ComposeManager::new();
    let config_dir = config_path.parent().unwrap_or(workspace_folder);
    let project = compose_manager.create_project(config, workspace_folder, config_dir)?;

    // Validate service exists
    if !compose_manager
        .validate_service_exists(&project, service)
        .await?
    {
        return Err(anyhow!(
            "Service '{}' not found in Docker Compose configuration",
            service
        ));
    }

    // Build the service
    let _build_output = compose_manager.build_service(&project, service).await?;

    let build_duration = build_start.elapsed().as_secs_f64();

    info!("Docker Compose service built successfully: {}", service);

    // Generate image names - compose services typically use project-service naming
    let mut image_names = args.image_names.clone();
    if image_names.is_empty() {
        // Use default naming: project_service
        image_names.push(format!("{}-{}", project.name, service));
    }

    // Create metadata with labels
    let mut metadata = HashMap::new();
    for (key, value) in labels {
        metadata.insert(key.clone(), value.clone());
    }

    Ok(BuildResult {
        image_id: format!("{}-{}", project.name, service),
        tags: image_names,
        build_duration,
        metadata,
        // Non-features compose build generates no feature-layering Dockerfile,
        // so build-time host-CA injection does not apply here (FR-018a).
        injected_ca_subjects: Vec::new(),
        config_hash: config_hash.to_string(),
    })
}

/// Execute a Compose build that also installs declared features.
///
/// Unlike `execute_compose_build` (a plain `docker compose build`), this resolves
/// the target service's shape (`image:` vs `build:`) and builds a
/// feature-extended image via the same helper `up` uses
/// (`resolve_compose_feature_image`), then tags that image with the
/// deterministic `deacon-build:<hash>` tag plus any `--image-name`s and writes
/// the feature lockfile next to the config.
#[instrument(skip(config, args, workspace_folder, config_path, labels))]
async fn execute_compose_build_with_features(
    config: &DevContainerConfig,
    args: &BuildArgs,
    workspace_folder: &Path,
    config_path: &Path,
    labels: &[(String, String)],
    config_hash: &str,
    host_ca_set: Option<&CorporateCaSet>,
) -> Result<BuildResult> {
    use crate::commands::up::compose::resolve_compose_feature_image;
    use deacon_core::compose::ComposeManager;
    use deacon_core::container::ContainerIdentity;
    use deacon_core::lockfile::{get_lockfile_path, write_lockfile};

    let service = config
        .service
        .as_ref()
        .ok_or_else(|| anyhow!("Docker Compose configuration must specify a service"))?;

    let compose_manager = ComposeManager::new();
    // Compose files resolve relative to the config dir (spec parity).
    let config_dir = config_path.parent().unwrap_or(workspace_folder);
    let project = compose_manager.create_project(config, workspace_folder, config_dir)?;
    if !compose_manager
        .validate_service_exists(&project, service)
        .await?
    {
        return Err(anyhow!(
            "Service '{}' not found in Docker Compose configuration",
            service
        ));
    }

    // Namespace the produced image by workspace (+ `-build`) so it does not
    // collide with `up`'s compose feature image on the same host.
    let identity = ContainerIdentity::new(workspace_folder, config);
    let workspace_hash = format!("{}-build", identity.workspace_hash);

    let feature_build = resolve_compose_feature_image(
        config,
        &compose_manager,
        &project,
        workspace_folder,
        config_path,
        &workspace_hash,
        host_ca_set,
    )
    .await?
    .ok_or_else(|| anyhow!("Compose feature build produced no image (no features declared?)"))?;

    // Tag the feature-extended image with the deterministic tag + user image names
    // so `--image-name` resolves to the image with features installed.
    let deterministic_tag = format!("deacon-build:{}", &config_hash[..12.min(config_hash.len())]);
    let mut all_tags = vec![deterministic_tag];
    all_tags.extend(args.image_names.clone());
    for tag in &all_tags {
        retag_image(&feature_build.image_tag, tag).await?;
    }

    // Write the feature lockfile next to the config (best-effort on read-only FS).
    let lockfile_path = get_lockfile_path(config_path);
    match write_lockfile(&lockfile_path, &feature_build.lockfile, true).await {
        Ok(()) => debug!("Wrote feature lockfile to '{}'", lockfile_path.display()),
        Err(e) => {
            let e = anyhow::Error::from(e);
            if is_readonly_filesystem_error(&e) {
                warn!(
                    path = %lockfile_path.display(),
                    error = %e,
                    "Lockfile write skipped (read-only workspace); continuing"
                );
            } else {
                return Err(e).with_context(|| {
                    format!("Failed to write lockfile to '{}'", lockfile_path.display())
                });
            }
        }
    }

    let mut metadata = HashMap::new();
    for (key, value) in labels {
        metadata.insert(key.clone(), value.clone());
    }

    Ok(BuildResult {
        image_id: feature_build.image_tag,
        tags: all_tags,
        build_duration: 0.0,
        metadata,
        config_hash: config_hash.to_string(),
        injected_ca_subjects: host_ca_set.map(|s| s.subjects.clone()).unwrap_or_default(),
    })
}

/// Execute image-reference build by creating a Dockerfile from the base image
#[instrument(skip(config, args, workspace_folder, labels))]
async fn execute_image_reference_build(
    config: &DevContainerConfig,
    args: &BuildArgs,
    workspace_folder: &Path,
    labels: &[(String, String)],
) -> Result<BuildResult> {
    let image = config
        .image
        .as_ref()
        .ok_or_else(|| anyhow!("Image reference configuration must specify an image"))?;

    info!("Building from image reference: {}", image);

    // Create a temporary Dockerfile that extends the base image
    let temp_dir = workspace_folder.join(".deacon-temp-build");
    tokio::fs::create_dir_all(&temp_dir).await?;

    // Build Dockerfile content with base image
    let mut dockerfile_content = format!("FROM {}\n\n", image);

    // Add labels
    if !labels.is_empty() {
        dockerfile_content.push_str("# User-specified labels\n");
        for (key, value) in labels {
            // Escape quotes in label values
            let escaped_value = value.replace('"', "\\\"");
            dockerfile_content.push_str(&format!("LABEL \"{}\"=\"{}\"\n", key, escaped_value));
        }
        dockerfile_content.push('\n');
    }

    // Add devcontainer metadata label.
    // Per spec (devcontainers/cli#1199, v0.86.0), the label value is always a
    // JSON array of partial-config entries, even when only a single entry is
    // present. Consumers (VS Code, Zed, envbuilder) iterate and merge.
    let metadata = serde_json::json!([{
        "name": config.name.as_ref().unwrap_or(&"devcontainer".to_string()),
        "image": image,
    }]);
    let metadata_str = serde_json::to_string(&metadata)?;
    let escaped_metadata = metadata_str.replace('"', "\\\"");
    dockerfile_content.push_str(&format!(
        "LABEL \"devcontainer.metadata\"=\"{}\"\n",
        escaped_metadata
    ));

    // Features (if any) are layered on top of this base by the post-build
    // `apply_features_and_lockfile` pass in `execute_build`: this synthetic
    // image is tagged `deacon-build:<hash>` by `execute_docker_build` below,
    // and that tag becomes the `FROM` base for the feature-install stage.

    let dockerfile_path = temp_dir.join("Dockerfile");
    tokio::fs::write(&dockerfile_path, dockerfile_content).await?;

    // Create a BuildConfig for this temporary Dockerfile
    let build_config = BuildConfig {
        dockerfile: "Dockerfile".to_string(),
        dockerfile_path,
        context: ".".to_string(),
        context_folder: temp_dir.to_path_buf(),
        target: None,
        options: HashMap::new(),
    };

    // Generate config hash for this image reference build
    let config_hash = format!("image-ref-{}", image.replace([':', '/'], "-"));

    // Execute the docker build
    let result =
        execute_docker_build(&build_config, args, &config_hash, workspace_folder, labels).await;

    // Clean up temporary directory
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;

    result
}

/// Apply `target` as an additional tag on the local image `source`
/// (`docker tag source target`).
///
/// Used after the post-build feature pass to re-point the base build's tags at
/// the feature-extended image, so `--image-name` resolves to the image that
/// actually contains the installed features.
async fn retag_image(source: &str, target: &str) -> Result<()> {
    let output = tokio::process::Command::new("docker")
        .args(["tag", source, target])
        .output()
        .await
        .map_err(|e| {
            DeaconError::Docker(DockerError::CLIError(format!(
                "Failed to run 'docker tag {} {}': {}",
                source, target, e
            )))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DeaconError::Docker(DockerError::CLIError(format!(
            "Failed to tag '{}' as '{}': {}",
            source,
            target,
            stderr.trim()
        )))
        .into());
    }

    debug!("Re-tagged feature image '{}' as '{}'", source, target);
    Ok(())
}

/// PR-4c: layer features on top of the just-built image and write the
/// lockfile next to the config file.
///
/// Synthesizes a config that points at the just-built `image_id` and reuses
/// `up`'s `build_image_with_features` helper to:
/// 1. Resolve + download every feature declared in `config.features`,
/// 2. Generate an extension Dockerfile (`FROM <built_image> AS dev_containers_target_stage`
///    + one BuildKit RUN-mount per feature),
/// 3. Build the extended image via `docker buildx build`,
/// 4. Hand back a `FeatureBuildOutput` whose `lockfile` field is keyed by
///    the user-provided feature ID (matching upstream `generateLockfile`).
///
/// The returned `Lockfile` is then written to disk via
/// `deacon_core::lockfile::write_lockfile(force_init = true)` — same path
/// + format as `up`'s post-build lockfile write (PR-4b). On read-only
/// workspaces (`EROFS`/`EACCES`) the write is downgraded to a WARN so a
/// read-only CI mount doesn't fail the build.
///
/// Returns `(new_image_id, Some(lockfile_path))` on success.
#[instrument(skip(config))]
async fn apply_features_and_lockfile(
    config: &DevContainerConfig,
    built_image_id: &str,
    workspace_folder: &Path,
    config_path: &Path,
    host_ca_set: Option<&CorporateCaSet>,
) -> Result<(String, Option<PathBuf>)> {
    use crate::commands::up::features_build::build_image_with_features;
    use deacon_core::container::ContainerIdentity;
    use deacon_core::lockfile::{get_lockfile_path, write_lockfile};

    info!(
        built_image = %built_image_id,
        "Layering features on top of build output"
    );

    // Synthesize a single-container config that points at the just-built
    // image. `build_image_with_features` reads `config.image`,
    // `config.features`, and `config.override_feature_install_order`; other
    // fields are ignored, so cloning + retargeting `image` is sufficient.
    let mut synth_config = config.clone();
    synth_config.image = Some(built_image_id.to_string());

    // Namespace the produced tag by workspace+config so it does not collide
    // with `up`'s feature-extended images on the same host.
    let mut identity = ContainerIdentity::new(workspace_folder, &synth_config);
    identity.workspace_hash = format!("{}-build", identity.workspace_hash);

    let feature_build = build_image_with_features(
        &synth_config,
        &identity,
        workspace_folder,
        config_path,
        None,
        host_ca_set,
    )
    .await
    .context("Failed to build feature-extended image from build output")?;

    info!(
        feature_image = %feature_build.image_tag,
        "Successfully built feature-extended image"
    );

    // Write the lockfile next to the config file (spec §6 naming rule).
    let lockfile_path = get_lockfile_path(config_path);
    let written = match write_lockfile(&lockfile_path, &feature_build.lockfile, true).await {
        Ok(()) => Some(lockfile_path),
        Err(e) => {
            let e = anyhow::Error::from(e);
            if is_readonly_filesystem_error(&e) {
                warn!(
                    path = %lockfile_path.display(),
                    error = %e,
                    "Lockfile write skipped (read-only workspace); continuing without persisting lockfile"
                );
                None
            } else {
                return Err(e).with_context(|| {
                    format!("Failed to write lockfile to '{}'", lockfile_path.display())
                });
            }
        }
    };

    Ok((feature_build.image_tag, written))
}

/// Mirror of `up::helpers::is_readonly_fs_error` for build's write path. We
/// don't share the helper today because `up::helpers` is private to `up`;
/// a future cleanup can lift both helpers into a shared module.
fn is_readonly_filesystem_error(err: &anyhow::Error) -> bool {
    use std::io;
    // Linux EROFS (28 on most arches, 30 on Linux) — checked via raw errno
    // because `io::ErrorKind::ReadOnlyFilesystem` requires Rust 1.83 and our
    // MSRV is 1.82.
    #[cfg(unix)]
    const EROFS: i32 = 30;
    err.chain().any(|cause| {
        let Some(io_err) = cause.downcast_ref::<io::Error>() else {
            return false;
        };
        if io_err.kind() == io::ErrorKind::PermissionDenied {
            return true;
        }
        #[cfg(unix)]
        {
            if io_err.raw_os_error() == Some(EROFS) {
                return true;
            }
        }
        false
    })
}

/// Execute Docker build
#[instrument(skip(build_config, args, workspace_folder, labels))]
async fn execute_docker_build(
    build_config: &BuildConfig,
    args: &BuildArgs,
    config_hash: &str,
    workspace_folder: &Path,
    labels: &[(String, String)],
) -> Result<BuildResult> {
    {
        use deacon_core::docker::{CliDocker, Docker};

        let docker = CliDocker::new();

        // Check Docker availability
        docker.check_docker_installed()?;
        docker.ping().await?;

        debug!("Building Docker image");

        // Prepare build context
        let context_path = build_config.context_folder.join(&build_config.context);
        let dockerfile_path = build_config.dockerfile_path.clone();

        // Prepare docker build arguments
        let mut build_args = vec!["build".to_string()];

        // Defer adding context until after all flags (Docker expects PATH last)

        // Add dockerfile
        build_args.push("-f".to_string());
        build_args.push(
            dockerfile_path
                .to_str()
                .ok_or_else(|| {
                    DeaconError::Docker(DockerError::CLIError(
                        "Invalid dockerfile path".to_string(),
                    ))
                })?
                .to_string(),
        );

        // Add no-cache flag
        if args.no_cache {
            build_args.push("--no-cache".to_string());
        }

        // Add platform
        if let Some(platform) = &args.platform {
            build_args.push("--platform".to_string());
            build_args.push(platform.clone());
        }

        // Add target
        if let Some(target) = &build_config.target {
            build_args.push("--target".to_string());
            build_args.push(target.clone());
        }

        // Add build args from config
        for (key, value) in &build_config.options {
            let build_arg_str = format!("{}={}", key, value);
            build_args.push("--build-arg".to_string());
            build_args.push(build_arg_str);
        }

        // Add build args from CLI
        for build_arg in &args.build_arg {
            build_args.push("--build-arg".to_string());
            build_args.push(build_arg.clone());
        }

        // Add cache-from options
        for cache_from in &args.cache_from {
            build_args.push("--cache-from".to_string());
            build_args.push(cache_from.clone());
        }

        // Add cache-to options
        for cache_to in &args.cache_to {
            build_args.push("--cache-to".to_string());
            build_args.push(cache_to.clone());
        }

        // Add secret forwarding
        for secret in &args.secret {
            build_args.push("--secret".to_string());
            build_args.push(secret.clone());
        }

        // Process and add build secrets
        let mut temp_secret_files = Vec::new();
        if !args.build_secret.is_empty() {
            debug!("Processing {} build secrets", args.build_secret.len());

            // Parse all build secrets
            let mut parsed_secrets = Vec::new();
            let mut seen_ids = HashSet::new();

            for spec in &args.build_secret {
                let secret = BuildSecret::parse(spec)
                    .with_context(|| format!("Failed to parse build secret spec: {}", spec))?;

                // Check for duplicate IDs
                if !seen_ids.insert(secret.id.clone()) {
                    return Err(anyhow!(
                        "Duplicate build secret id '{}'. Each secret must have a unique id.",
                        secret.id
                    ));
                }

                // Validate the secret source is accessible
                secret
                    .validate()
                    .with_context(|| format!("Build secret '{}' validation failed", secret.id))?;

                parsed_secrets.push(secret);
            }

            // Read all secret values first (before creating any temp files)
            // This allows early returns on errors without leaving temp files behind
            let mut secret_values = Vec::new();
            for secret in &parsed_secrets {
                let value = secret
                    .read_value()
                    .await
                    .with_context(|| format!("Failed to read build secret '{}'", secret.id))?;

                // Register the secret value for redaction
                if args.redaction_config.enabled {
                    debug!(
                        "Registering build secret '{}' for redaction (length: {})",
                        secret.id,
                        value.len()
                    );
                    args.secret_registry.add_secret(&value);
                }

                secret_values.push(value);
            }

            // Now create temp files and build args (after all validation succeeds)
            for (secret, value) in parsed_secrets.iter().zip(secret_values.iter()) {
                // For env and stdin sources, we need to write to a temp file
                let temp_file = match &secret.source {
                    BuildSecretSource::File(_) => None,
                    BuildSecretSource::Env(_) | BuildSecretSource::Stdin => {
                        let temp_file = tempfile::NamedTempFile::new()
                            .context("Failed to create temporary file for build secret")?;
                        tokio::fs::write(temp_file.path(), value)
                            .await
                            .with_context(|| {
                                format!(
                                    "Failed to write build secret '{}' to temporary file",
                                    secret.id
                                )
                            })?;
                        debug!(
                            "Wrote build secret '{}' to temp file: {}",
                            secret.id,
                            temp_file.path().display()
                        );
                        Some(temp_file)
                    }
                };

                // Generate the Docker argument
                let docker_arg = if let Some(ref temp) = temp_file {
                    secret.to_docker_arg(Some(temp.path()))
                } else {
                    secret.to_docker_arg(None)
                };

                build_args.push("--secret".to_string());
                build_args.push(docker_arg);

                // Store temp file to keep it alive during the build
                if let Some(temp) = temp_file {
                    temp_secret_files.push(temp);
                }
            }
        }

        // Add SSH forwarding
        for ssh in &args.ssh {
            build_args.push("--ssh".to_string());
            build_args.push(ssh.clone());
        }

        // Determine if BuildKit should be used
        let use_buildkit = should_use_buildkit(args.buildkit.as_ref());
        debug!("Using BuildKit: {}", use_buildkit);

        // Secrets/SSH require BuildKit; provide a clear error early.
        if !use_buildkit
            && (!args.secret.is_empty() || !args.build_secret.is_empty() || !args.ssh.is_empty())
        {
            if args.buildkit == Some(BuildKitOption::Never) {
                return Err(DockerError::CLIError(
                    "The --secret/--build-secret/--ssh options require BuildKit but --buildkit never was specified"
                        .to_string(),
                )
                .into());
            }
            return Err(DockerError::CLIError(
                "The --secret/--build-secret/--ssh options require BuildKit. Re-run with --buildkit auto or set DOCKER_BUILDKIT=1"
                    .to_string(),
            )
            .into());
        }

        // Set DOCKER_BUILDKIT environment variable if needed
        let mut cmd = tokio::process::Command::new("docker");
        if use_buildkit {
            cmd.env("DOCKER_BUILDKIT", "1");
        } else if args.buildkit == Some(BuildKitOption::Never) {
            cmd.env("DOCKER_BUILDKIT", "0");
        }

        // Add deterministic tag with config hash
        let tag = format!("deacon-build:{}", &config_hash[..12]);
        build_args.push("-t".to_string());
        build_args.push(tag.clone());

        // Add user-specified image names as additional tags
        for image_name in &args.image_names {
            build_args.push("-t".to_string());
            build_args.push(image_name.clone());
        }

        // Add label with config hash
        let label = format!("org.deacon.configHash={}", config_hash);
        build_args.push("--label".to_string());
        build_args.push(label);

        // Add devcontainer metadata label (simplified for T011).
        // Per spec (devcontainers/cli#1199, v0.86.0), the label value is always a
        // JSON array of partial-config entries, even when only a single entry is
        // present. Consumers (VS Code, Zed, envbuilder) iterate and merge.
        let metadata_json = serde_json::json!([{
            "configHash": config_hash,
        }]);
        let metadata_str = serde_json::to_string(&metadata_json)
            .map_err(|e| anyhow!("Failed to serialize metadata: {}", e))?;
        build_args.push("--label".to_string());
        build_args.push(format!("devcontainer.metadata={}", metadata_str));

        // Add user-specified labels
        for (key, value) in labels {
            build_args.push("--label".to_string());
            build_args.push(format!("{}={}", key, value));
        }

        // Add --push flag if requested
        if args.push {
            build_args.push("--push".to_string());
        }

        // Add --output flag if requested
        if let Some(output) = &args.output {
            build_args.push("--output".to_string());
            build_args.push(output.clone());
        }

        // When using BuildKit without --push or --output, add --load to ensure
        // the image is loaded into the local Docker daemon (BuildKit doesn't do this by default)
        if use_buildkit && !args.push && args.output.is_none() {
            build_args.push("--load".to_string());
        }

        // Add quiet flag to reduce output noise (only if not pushing/exporting)
        if !args.push && args.output.is_none() {
            build_args.push("-q".to_string());
        }

        // Finally add build context (must be last)
        build_args.push(
            context_path
                .to_str()
                .ok_or_else(|| {
                    DeaconError::Docker(DockerError::CLIError("Invalid context path".to_string()))
                })?
                .to_string(),
        );

        debug!("Docker build command: docker {}", build_args.join(" "));

        // Execute docker build (async) using the prepared command with env vars
        let output = cmd
            .args(&build_args) // Pass all args including "build" subcommand
            .current_dir(workspace_folder)
            .output()
            .await
            .map_err(|e| DockerError::CLIError(format!("Failed to execute docker build: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DockerError::CLIError(format!("Docker build failed: {}", stderr)).into());
        }

        // When using --push or --output, we may not get an image ID on stdout
        let image_id = if args.push || args.output.is_some() {
            // For push/export, the image may not be available locally
            // Use the first user-specified tag or the deterministic tag as a reference
            if !args.image_names.is_empty() {
                args.image_names[0].clone()
            } else {
                tag.clone()
            }
        } else {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        };

        // Extract image metadata (skip if pushing or exporting as image may not be local)
        let metadata = if args.push || args.output.is_some() {
            HashMap::new()
        } else {
            extract_image_metadata(&image_id).await?
        };

        // Collect all tags: deterministic tag plus user-specified tags
        let mut all_tags = vec![tag];
        all_tags.extend(args.image_names.clone());

        let result = BuildResult {
            image_id,
            tags: all_tags,
            build_duration: 0.0, // Will be set by caller
            metadata,
            config_hash: config_hash.to_string(),
            // Base build; feature-layering (and thus build-time host-CA
            // injection) is applied by the post-build pass in `execute_build`.
            injected_ca_subjects: Vec::new(),
        };

        debug!("Docker build completed successfully");
        Ok(result)
    }
}

/// Extract image metadata using docker inspect
#[allow(dead_code)]
async fn extract_image_metadata(image_id: &str) -> Result<HashMap<String, String>> {
    debug!("Extracting metadata for image: {}", image_id);

    let output = tokio::process::Command::new("docker")
        .args(["inspect", "--format={{json .Config.Labels}}", image_id])
        .output()
        .await
        .map_err(|e| DockerError::CLIError(format!("Failed to inspect image: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DockerError::CLIError(format!("Docker inspect failed: {}", stderr)).into());
    }

    let labels_json = String::from_utf8_lossy(&output.stdout);
    let labels: HashMap<String, String> = if labels_json.trim() == "null" {
        HashMap::new()
    } else {
        serde_json::from_str(&labels_json)
            .map_err(|e| DockerError::CLIError(format!("Failed to parse image labels: {}", e)))?
    };

    debug!("Extracted {} labels from image", labels.len());
    Ok(labels)
}

/// Output build result in the specified format with redaction
fn output_result(
    result: &BuildResult,
    format: &OutputFormat,
    redaction_config: &deacon_core::redaction::RedactionConfig,
    registry: &deacon_core::redaction::SecretRegistry,
    pushed: bool,
    export_path: Option<&str>,
) -> Result<()> {
    use deacon_core::redaction::RedactingWriter;
    use std::io::Write;

    let stdout = std::io::stdout();
    let mut writer = RedactingWriter::new(stdout, redaction_config.clone(), registry);

    match format {
        OutputFormat::Json => {
            // Build spec-compliant JSON output
            // Deterministic fallback tag (first) should NOT be included when user supplied tags.
            // If user provided image names, they appear after the fallback tag.
            let display_tags: Vec<String> = if result.tags.len() > 1 {
                // Skip the first deterministic tag
                result.tags[1..].to_vec()
            } else {
                result.tags.clone()
            };

            let mut success_result = if display_tags.is_empty() {
                result::BuildSuccess::default()
            } else if display_tags.len() == 1 {
                result::BuildSuccess::new_single(display_tags[0].clone())
            } else {
                result::BuildSuccess::new_multiple(display_tags)
            };

            // Add push status if --push was used
            if pushed {
                success_result = success_result.with_pushed(true);
            }

            // Add export path if --output was used
            if let Some(path) = export_path {
                success_result = success_result.with_export_path(path.to_string());
            }

            let json = serde_json::to_string(&success_result).map_err(|e| {
                DeaconError::Internal(deacon_core::errors::InternalError::Generic {
                    message: format!("Failed to serialize result to JSON: {}", e),
                })
            })?;
            writer.write_line(&json)?;
        }
        OutputFormat::Text => {
            writer.write_line("Build completed successfully!")?;
            if !result.image_id.is_empty() {
                writer.write_line(&format!("Image ID: {}", result.image_id))?;
            }
            writer.write_line(&format!("Tags: {}", result.tags.join(", ")))?;
            writer.write_line(&format!("Build duration: {:.2}s", result.build_duration))?;
            writer.write_line(&format!("Config hash: {}", result.config_hash))?;

            if pushed {
                writer.write_line("Image pushed to registry successfully")?;
            }

            if let Some(path) = export_path {
                writer.write_line(&format!("Image exported to: {}", path))?;
            }

            if !result.metadata.is_empty() {
                writer.write_line("Labels:")?;
                for (key, value) in &result.metadata {
                    writer.write_line(&format!("  {}: {}", key, value))?;
                }
            }
        }
    }

    writer.flush()?;
    Ok(())
}

/// Execute vulnerability scan on the built image
#[instrument(skip(args, emit_progress_event))]
async fn execute_vulnerability_scan<F>(
    args: &BuildArgs,
    image_id: &str,
    emit_progress_event: F,
) -> Result<bool>
where
    F: Fn(deacon_core::progress::ProgressEvent) -> Result<()>,
{
    // Get scan command from environment variable
    let scan_cmd_template = match std::env::var("DEACON_SCAN_CMD") {
        Ok(template) => template,
        Err(_) => {
            warn!("DEACON_SCAN_CMD environment variable not set, skipping vulnerability scan");
            return Ok(true); // Consider no scan command as success
        }
    };

    // Perform token substitution
    let scan_command = substitute_tokens(&scan_cmd_template, image_id)?;

    info!("Executing vulnerability scan: {}", scan_command);

    let scan_start_time = std::time::Instant::now();

    // Emit scan begin event
    emit_progress_event(deacon_core::progress::ProgressEvent::ScanBegin {
        id: deacon_core::progress::ProgressTracker::next_event_id(),
        timestamp: deacon_core::progress::ProgressTracker::current_timestamp(),
        image_id: image_id.to_string(),
        command: scan_command.clone(),
    })?;

    // Parse and execute the scan command
    let scan_result = execute_scan_command(&scan_command, args).await;
    let scan_duration = scan_start_time.elapsed();

    let (success, exit_code) = match scan_result {
        Ok(exit_code) => {
            let success = exit_code == 0;
            if success {
                info!("Vulnerability scan completed successfully");
            } else if args.fail_on_scan {
                warn!(
                    "Vulnerability scan failed with exit code {} (will fail build)",
                    exit_code
                );
            } else {
                warn!(
                    "Vulnerability scan failed with exit code {} (continuing build)",
                    exit_code
                );
            }
            (success, Some(exit_code))
        }
        Err(e) => {
            warn!("Failed to execute vulnerability scan: {}", e);
            (false, None)
        }
    };

    // Emit scan end event
    emit_progress_event(deacon_core::progress::ProgressEvent::ScanEnd {
        id: deacon_core::progress::ProgressTracker::next_event_id(),
        timestamp: deacon_core::progress::ProgressTracker::current_timestamp(),
        image_id: image_id.to_string(),
        duration_ms: scan_duration.as_millis() as u64,
        success,
        exit_code,
    })?;

    Ok(success)
}

/// Substitute tokens in the scan command template
pub fn substitute_tokens(template: &str, image_id: &str) -> Result<String> {
    let substituted = template.replace("{image}", image_id);
    debug!("Substituted scan command: {} -> {}", template, substituted);
    Ok(substituted)
}

/// Execute the scan command and return exit code
async fn execute_scan_command(command: &str, args: &BuildArgs) -> Result<i32> {
    use std::process::Stdio;

    // Parse command into program and arguments using shell-aware splitting
    let parts = shell_words::split(command)
        .map_err(|e| anyhow::anyhow!("Failed to parse scan command '{}': {}", command, e))?;
    if parts.is_empty() {
        return Err(anyhow::anyhow!("Empty scan command"));
    }

    let program = &parts[0];
    let command_args = &parts[1..];

    debug!(
        "Executing scan command: {} with args: {:?}",
        program, command_args
    );

    // Create redacting writer for scan output
    use deacon_core::redaction::RedactingWriter;
    use std::io::Write;
    let stdout = std::io::stdout();
    let mut writer =
        RedactingWriter::new(stdout, args.redaction_config.clone(), &args.secret_registry);

    let mut cmd = tokio::process::Command::new(program);
    cmd.args(command_args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn scan command '{}': {}", program, e))?;

    // Read stdout and stderr in parallel
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout from scan command"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture stderr from scan command"))?;

    let stdout_task = tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut output = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            output.push(line);
        }
        Ok::<Vec<String>, anyhow::Error>(output)
    });

    let stderr_task = tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        let mut output = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            output.push(line);
        }
        Ok::<Vec<String>, anyhow::Error>(output)
    });

    // Wait for command to complete
    let status = child
        .wait()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to wait for scan command: {}", e))?;

    // Collect output
    let stdout_lines = stdout_task
        .await
        .map_err(|e| anyhow::anyhow!("Failed to join stdout task: {}", e))?
        .context("Failed to read stdout from scan command")?;
    let stderr_lines = stderr_task
        .await
        .map_err(|e| anyhow::anyhow!("Failed to join stderr task: {}", e))?
        .context("Failed to read stderr from scan command")?;

    // Write output through redacting writer
    if !stdout_lines.is_empty() {
        writer.write_line("Scan stdout:")?;
        for line in &stdout_lines {
            writer.write_line(&format!("  {}", line))?;
        }
    }

    if !stderr_lines.is_empty() {
        writer.write_line("Scan stderr:")?;
        for line in &stderr_lines {
            writer.write_line(&format!("  {}", line))?;
        }
    }

    writer.flush()?;

    let exit_code = status.code().unwrap_or(-1);
    debug!("Scan command completed with exit code: {}", exit_code);

    Ok(exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_build_config_dockerfile_parsing() {
        let mut config = DevContainerConfig::default();
        config.name = Some("test".to_string());
        config.dockerfile = Some("Dockerfile".to_string());

        // Test with simple dockerfile
        let temp_dir = tempfile::tempdir().unwrap();
        let dockerfile_path = temp_dir.path().join("Dockerfile");
        std::fs::write(&dockerfile_path, "FROM alpine:3.19\nLABEL test=1\n").unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let result = extract_build_config(&config, &config_path);
        assert!(result.is_ok());
        let build_config = result.unwrap();
        assert_eq!(build_config.dockerfile, "Dockerfile");
        assert_eq!(build_config.context, ".");

        // Test with build configuration
        config.build = Some(serde_json::json!({
            "context": "docker",
            "target": "development",
            "options": {
                "BUILDKIT_INLINE_CACHE": "1"
            }
        }));

        let result = extract_build_config(&config, &config_path);
        assert!(result.is_ok());
        let build_config = result.unwrap();
        assert_eq!(build_config.context, "docker");
        assert_eq!(build_config.target, Some("development".to_string()));
        assert_eq!(
            build_config.options.get("BUILDKIT_INLINE_CACHE"),
            Some(&"1".to_string())
        );
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_build_config_nested_build_dockerfile() {
        // The canonical containers.dev form nests the Dockerfile under `build`:
        //   "build": { "dockerfile": "Dockerfile" }
        // `deacon build` must honor it just like the legacy top-level `dockerFile`
        // field (parity with `up`'s image_build path).
        let temp_dir = tempfile::tempdir().unwrap();
        let dockerfile_path = temp_dir.path().join("Dockerfile");
        std::fs::write(&dockerfile_path, "FROM alpine:3.19\nLABEL test=1\n").unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let mut config = DevContainerConfig::default();
        config.name = Some("test".to_string());
        // No top-level `dockerFile` — only the nested `build.dockerfile`.
        config.build = Some(serde_json::json!({
            "dockerfile": "Dockerfile",
            "context": ".",
            "args": { "FOO": "bar" }
        }));

        let result = extract_build_config(&config, &config_path);
        assert!(
            result.is_ok(),
            "build.dockerfile should be recognized: {:?}",
            result.err()
        );
        let build_config = result.unwrap();
        assert_eq!(build_config.dockerfile, "Dockerfile");
        assert_eq!(build_config.context, ".");
        assert_eq!(build_config.options.get("FOO"), Some(&"bar".to_string()));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_build_config_resolves_dockerfile_relative_to_config_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_dir = temp_dir.path().join(".devcontainer");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(config_dir.join("Dockerfile"), "FROM alpine:3.19\n").unwrap();
        std::fs::write(temp_dir.path().join("Dockerfile"), "FROM busybox:1.36\n").unwrap();
        let config_path = config_dir.join("devcontainer.json");

        let mut config = DevContainerConfig::default();
        config.name = Some("test".to_string());
        config.build = Some(serde_json::json!({
            "dockerfile": "Dockerfile",
            "context": ".."
        }));

        let build_config = extract_build_config(&config, &config_path).unwrap();
        assert_eq!(build_config.dockerfile_path, config_dir.join("Dockerfile"));
        assert_eq!(build_config.context_folder, config_dir);
        assert_eq!(build_config.context, "..");
    }

    #[test]
    fn test_image_name_dedup_preserves_first_seen_order() {
        // Mirror the normalization in execute_build: duplicate `--image-name`
        // values collapse to first-seen order.
        let mut image_names = vec![
            "myorg/dups:latest".to_string(),
            "myorg/dups:latest".to_string(),
            "myorg/other:v1".to_string(),
            "myorg/dups:latest".to_string(),
        ];
        let mut seen = std::collections::HashSet::new();
        image_names.retain(|name| seen.insert(name.clone()));
        assert_eq!(
            image_names,
            vec![
                "myorg/dups:latest".to_string(),
                "myorg/other:v1".to_string()
            ]
        );
    }

    #[test]
    fn test_config_hash_calculation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let build_config = BuildConfig {
            dockerfile: "Dockerfile".to_string(),
            dockerfile_path: temp_dir.path().join("Dockerfile"),
            context: ".".to_string(),
            context_folder: temp_dir.path().to_path_buf(),
            target: Some("dev".to_string()),
            options: {
                let mut map = HashMap::new();
                map.insert("ARG1".to_string(), "value1".to_string());
                map.insert("ARG2".to_string(), "value2".to_string());
                map
            },
        };

        let dockerfile_path = temp_dir.path().join("Dockerfile");
        std::fs::write(&dockerfile_path, "FROM alpine:3.19\n").unwrap();

        let hash1 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();
        let hash2 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();

        // Same config should produce same hash
        assert_eq!(hash1, hash2);

        // Different config should produce different hash
        let mut build_config2 = build_config.clone();
        build_config2.dockerfile = "Dockerfile.dev".to_string();

        let hash3 = calculate_config_hash(&build_config2, temp_dir.path()).unwrap();
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_build_args_assembly() {
        let args = BuildArgs {
            cache_from: vec![
                "registry://example.com/cache".to_string(),
                "type=local,src=/tmp/cache".to_string(),
            ],
            cache_to: vec!["registry://example.com/cache:latest".to_string()],
            buildkit: Some(BuildKitOption::Auto),
            secret: vec![
                "id=mypassword,src=./password.txt".to_string(),
                "id=mytoken".to_string(),
            ],
            ssh: vec!["default".to_string(), "mykey=/path/to/key".to_string()],
            ..Default::default()
        };

        // Verify args are structured correctly
        // Defaults currently retain cache and have no platform/build args set
        assert!(!args.no_cache);
        assert_eq!(args.platform, None);
        assert!(args.build_arg.is_empty());
    }

    #[test]
    fn test_advanced_build_args_assembly() {
        let args = BuildArgs {
            cache_from: vec![
                "registry://example.com/cache".to_string(),
                "type=local,src=/tmp/cache".to_string(),
            ],
            cache_to: vec!["registry://example.com/cache:latest".to_string()],
            buildkit: Some(BuildKitOption::Auto),
            secret: vec![
                "id=mypassword,src=./password.txt".to_string(),
                "id=mykey,env=SSH_KEY".to_string(),
            ],
            build_secret: vec!["id=mysecret,src=./secret.txt".to_string()],
            ssh: vec!["default".to_string()],
            ..Default::default()
        };

        // Verify advanced args are structured correctly
        assert_eq!(args.cache_from.len(), 2);
        assert!(
            args.cache_from
                .contains(&"registry://example.com/cache".to_string())
        );
        assert!(
            args.cache_from
                .contains(&"type=local,src=/tmp/cache".to_string())
        );

        assert_eq!(args.cache_to.len(), 1);
        assert!(
            args.cache_to
                .contains(&"registry://example.com/cache:latest".to_string())
        );

        assert_eq!(args.buildkit, Some(BuildKitOption::Auto));

        assert_eq!(args.secret.len(), 2);
        assert!(
            args.secret
                .contains(&"id=mypassword,src=./password.txt".to_string())
        );
        assert!(args.secret.contains(&"id=mykey,env=SSH_KEY".to_string()));

        // SSH defaults currently only contain explicitly provided entries
        assert_eq!(args.ssh.len(), 1);
        assert!(args.ssh.contains(&"default".to_string()));
    }

    #[test]
    fn test_buildkit_detection() {
        // Test BuildKit Auto mode with DOCKER_BUILDKIT=1
        temp_env::with_var("DOCKER_BUILDKIT", Some("1"), || {
            assert!(should_use_buildkit(Some(&BuildKitOption::Auto)));
        });

        // Test BuildKit Auto mode with DOCKER_BUILDKIT=true
        temp_env::with_var("DOCKER_BUILDKIT", Some("true"), || {
            assert!(should_use_buildkit(Some(&BuildKitOption::Auto)));
        });

        // Test BuildKit Auto mode with DOCKER_BUILDKIT=0
        temp_env::with_var("DOCKER_BUILDKIT", Some("0"), || {
            assert!(!should_use_buildkit(Some(&BuildKitOption::Auto)));
        });

        // Test BuildKit Auto mode with DOCKER_BUILDKIT=false
        temp_env::with_var("DOCKER_BUILDKIT", Some("false"), || {
            assert!(!should_use_buildkit(Some(&BuildKitOption::Auto)));
        });

        // Test BuildKit Never mode (should always be false)
        temp_env::with_var("DOCKER_BUILDKIT", Some("1"), || {
            assert!(!should_use_buildkit(Some(&BuildKitOption::Never)));
        });

        // Test None (default) mode - should respect env var
        temp_env::with_var("DOCKER_BUILDKIT", Some("1"), || {
            assert!(should_use_buildkit(None));
        });

        temp_env::with_var("DOCKER_BUILDKIT", Some("0"), || {
            assert!(!should_use_buildkit(None));
        });

        // Test None with no env var (should default to false)
        temp_env::with_var_unset("DOCKER_BUILDKIT", || {
            assert!(!should_use_buildkit(None));
        });
    }

    #[test]
    fn test_build_output_redaction() {
        use deacon_core::redaction::{RedactionConfig, SecretRegistry};
        use std::collections::HashMap;

        // Create a test BuildResult with potentially sensitive information
        let mut metadata = HashMap::new();
        metadata.insert("secret-key".to_string(), "password123".to_string());
        metadata.insert("public-key".to_string(), "public-value".to_string());

        let result = BuildResult {
            image_id: "sha256:secret123abc".to_string(),
            tags: vec!["myapp:latest".to_string()],
            metadata,
            config_hash: "hash123secret".to_string(),
            injected_ca_subjects: Vec::new(),
            build_duration: 1.5,
        };

        // Set up redaction
        let registry = SecretRegistry::new();
        registry.add_secret("password123");
        registry.add_secret("secret123");
        let config = RedactionConfig::with_custom_registry(registry.clone());

        // Test that calling output_result doesn't panic and applies redaction
        // Note: In a real test we'd capture stdout, but for now we just ensure it compiles and runs
        let result_call = output_result(
            &result,
            &OutputFormat::Text,
            &config,
            &registry,
            false,
            None,
        );
        assert!(result_call.is_ok(), "Output should not fail");
    }

    #[test]
    fn test_docker_cli_arg_ordering() {
        // Test that Docker build args are assembled in correct order
        // This simulates the argument building logic from execute_docker_build
        let temp_dir = tempfile::tempdir().unwrap();
        let dockerfile_path = temp_dir.path().join("Dockerfile");
        std::fs::write(&dockerfile_path, "FROM alpine:3.19\n").unwrap();

        let config_hash = "abcd1234567890";
        let context_path = temp_dir.path();

        // Simulate the build_args construction from execute_docker_build
        let mut build_args = vec!["build".to_string()];

        // Defer adding context until after all flags (Docker expects PATH last)

        // Add dockerfile
        build_args.push("-f".to_string());
        build_args.push(dockerfile_path.to_str().unwrap().to_string());

        // Add no-cache flag
        build_args.push("--no-cache".to_string());

        // Add platform
        build_args.push("--platform".to_string());
        build_args.push("linux/amd64".to_string());

        // Add build args
        build_args.push("--build-arg".to_string());
        build_args.push("ENV=test".to_string());

        // Add tag
        let tag = format!("deacon-build:{}", &config_hash[..12]);
        build_args.push("-t".to_string());
        build_args.push(tag.clone());

        // Add label
        let label = format!("org.deacon.configHash={}", config_hash);
        build_args.push("--label".to_string());
        build_args.push(label);

        // Add quiet flag
        build_args.push("-q".to_string());

        // Finally add context (PATH last)
        build_args.push(context_path.to_str().unwrap().to_string());

        // Verify the ordering: should start with "build" subcommand
        assert_eq!(build_args[0], "build");
        assert_eq!(build_args[1], "-f");
        assert_eq!(build_args[2], dockerfile_path.to_str().unwrap());
        assert_eq!(build_args[3], "--no-cache");
        assert_eq!(build_args[4], "--platform");
        assert_eq!(build_args[5], "linux/amd64");
        assert_eq!(build_args[6], "--build-arg");
        assert_eq!(build_args[7], "ENV=test");
        assert_eq!(build_args[8], "-t");
        assert_eq!(build_args[9], "deacon-build:abcd12345678");
        assert_eq!(build_args[10], "--label");
        assert_eq!(build_args[11], "org.deacon.configHash=abcd1234567890");
        assert_eq!(build_args[12], "-q");
        assert_eq!(build_args[13], context_path.to_str().unwrap());

        // Verify that when passed to Command::new("docker").args(&build_args),
        // it will correctly execute "docker build ..." not "docker -f ..."
        assert!(
            build_args[0] == "build",
            "First argument must be 'build' subcommand"
        );
        assert!(
            build_args.iter().position(|arg| arg == "-f").unwrap() > 0,
            "-f flag must come after build subcommand"
        );
    }

    #[test]
    fn test_docker_cli_arg_ordering_with_advanced_options() {
        // Test that Docker build args are assembled in correct order with advanced options
        // This simulates the argument building logic from execute_docker_build with all advanced options
        let temp_dir = tempfile::tempdir().unwrap();
        let dockerfile_path = temp_dir.path().join("Dockerfile");
        std::fs::write(&dockerfile_path, "FROM alpine:3.19\n").unwrap();

        let config_hash = "abcd1234567890";
        let context_path = temp_dir.path();

        // Simulate the build_args construction from execute_docker_build with advanced options
        let mut build_args = vec!["build".to_string()];

        // Add dockerfile
        build_args.push("-f".to_string());
        build_args.push(dockerfile_path.to_str().unwrap().to_string());

        // Add no-cache flag
        build_args.push("--no-cache".to_string());

        // Add platform
        build_args.push("--platform".to_string());
        build_args.push("linux/amd64".to_string());

        // Add build args
        build_args.push("--build-arg".to_string());
        build_args.push("ENV=test".to_string());

        // Add advanced build options
        // Add cache-from options
        build_args.push("--cache-from".to_string());
        build_args.push("registry://example.com/cache".to_string());
        build_args.push("--cache-from".to_string());
        build_args.push("type=local,src=/tmp/cache".to_string());

        // Add cache-to options
        build_args.push("--cache-to".to_string());
        build_args.push("registry://example.com/cache:latest".to_string());

        // Add secret forwarding
        build_args.push("--secret".to_string());
        build_args.push("id=mypassword,src=./password.txt".to_string());

        // Add SSH forwarding
        build_args.push("--ssh".to_string());
        build_args.push("default".to_string());

        // Add tag
        let tag = format!("deacon-build:{}", &config_hash[..12]);
        build_args.push("-t".to_string());
        build_args.push(tag.clone());

        // Add label
        let label = format!("org.deacon.configHash={}", config_hash);
        build_args.push("--label".to_string());
        build_args.push(label);

        // Add quiet flag
        build_args.push("-q".to_string());

        // Finally add context (PATH last)
        build_args.push(context_path.to_str().unwrap().to_string());

        // Verify advanced options are in the correct positions
        let cache_from_idx = build_args
            .iter()
            .position(|arg| arg == "--cache-from")
            .unwrap();
        let cache_to_idx = build_args
            .iter()
            .position(|arg| arg == "--cache-to")
            .unwrap();
        let secret_idx = build_args.iter().position(|arg| arg == "--secret").unwrap();
        let ssh_idx = build_args.iter().position(|arg| arg == "--ssh").unwrap();

        // Verify advanced options come after basic build args but before context
        let context_idx = build_args.len() - 1; // Context is last
        assert!(cache_from_idx < context_idx);
        assert!(cache_to_idx < context_idx);
        assert!(secret_idx < context_idx);
        assert!(ssh_idx < context_idx);

        // Verify specific advanced option values
        assert_eq!(
            build_args[cache_from_idx + 1],
            "registry://example.com/cache"
        );
        assert_eq!(
            build_args[cache_to_idx + 1],
            "registry://example.com/cache:latest"
        );
        assert_eq!(
            build_args[secret_idx + 1],
            "id=mypassword,src=./password.txt"
        );
        assert_eq!(build_args[ssh_idx + 1], "default");

        // Verify that context is still last
        assert_eq!(build_args[context_idx], context_path.to_str().unwrap());

        // Verify that the command still starts with "build"
        assert!(
            build_args[0] == "build",
            "First argument must be 'build' subcommand"
        );
    }

    #[test]
    fn test_secret_ssh_require_buildkit_validation() {
        // Test that BuildKitOption::Never always returns false
        let args_with_secret = BuildArgs {
            secret: vec!["id=test".to_string()],
            buildkit: Some(BuildKitOption::Never),
            ..BuildArgs::default()
        };

        let use_buildkit = should_use_buildkit(args_with_secret.buildkit.as_ref());
        assert!(
            !use_buildkit,
            "BuildKitOption::Never should always return false"
        );
        assert!(!args_with_secret.secret.is_empty());
        assert_eq!(args_with_secret.buildkit, Some(BuildKitOption::Never));

        // Test that None respects DOCKER_BUILDKIT environment variable
        let args_with_ssh = BuildArgs {
            ssh: vec!["default".to_string()],
            buildkit: None,
            ..BuildArgs::default()
        };

        assert!(!args_with_ssh.ssh.is_empty());
        assert_eq!(args_with_ssh.buildkit, None);

        // Test behavior with DOCKER_BUILDKIT unset (should default to false)
        temp_env::with_var_unset("DOCKER_BUILDKIT", || {
            assert!(
                !should_use_buildkit(args_with_ssh.buildkit.as_ref()),
                "should_use_buildkit should return false when DOCKER_BUILDKIT is unset and buildkit is None"
            );
        });

        // Test behavior with DOCKER_BUILDKIT=1 (should return true)
        temp_env::with_var("DOCKER_BUILDKIT", Some("1"), || {
            assert!(
                should_use_buildkit(args_with_ssh.buildkit.as_ref()),
                "should_use_buildkit should return true when DOCKER_BUILDKIT=1 and buildkit is None"
            );
        });

        // Test behavior with DOCKER_BUILDKIT=true (should return true)
        temp_env::with_var("DOCKER_BUILDKIT", Some("true"), || {
            assert!(
                should_use_buildkit(args_with_ssh.buildkit.as_ref()),
                "should_use_buildkit should return true when DOCKER_BUILDKIT=true and buildkit is None"
            );
        });

        // Test behavior with DOCKER_BUILDKIT=0 (should return false)
        temp_env::with_var("DOCKER_BUILDKIT", Some("0"), || {
            assert!(
                !should_use_buildkit(args_with_ssh.buildkit.as_ref()),
                "should_use_buildkit should return false when DOCKER_BUILDKIT=0 and buildkit is None"
            );
        });

        // Test behavior with DOCKER_BUILDKIT=false (should return false)
        temp_env::with_var("DOCKER_BUILDKIT", Some("false"), || {
            assert!(
                !should_use_buildkit(args_with_ssh.buildkit.as_ref()),
                "should_use_buildkit should return false when DOCKER_BUILDKIT=false and buildkit is None"
            );
        });

        // Test explicit Never option with SSH
        let args_ssh_never = BuildArgs {
            ssh: vec!["default".to_string()],
            buildkit: Some(BuildKitOption::Never),
            ..BuildArgs::default()
        };
        let use_buildkit_never = should_use_buildkit(args_ssh_never.buildkit.as_ref());
        assert!(
            !use_buildkit_never,
            "BuildKitOption::Never should return false even with SSH"
        );
    }

    #[test]
    fn test_is_non_build_affecting_file() {
        // Files that should not affect builds
        assert!(is_non_build_affecting_file("README.md"));
        assert!(is_non_build_affecting_file("readme"));
        assert!(is_non_build_affecting_file("CHANGELOG.md"));
        assert!(is_non_build_affecting_file("LICENSE"));
        assert!(is_non_build_affecting_file(".gitignore"));
        assert!(is_non_build_affecting_file("docs.md"));

        // Files that should affect builds
        assert!(!is_non_build_affecting_file("Dockerfile"));
        assert!(!is_non_build_affecting_file("main.py"));
        assert!(!is_non_build_affecting_file("package.json"));
        assert!(!is_non_build_affecting_file("requirements.txt"));
        assert!(!is_non_build_affecting_file("docker-compose.yml"));
        assert!(!is_non_build_affecting_file("dockerfile.dev"));
    }

    #[test]
    fn test_config_hash_with_context_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let build_config = BuildConfig {
            dockerfile: "Dockerfile".to_string(),
            dockerfile_path: temp_dir.path().join("Dockerfile"),
            context: ".".to_string(),
            context_folder: temp_dir.path().to_path_buf(),
            target: None,
            options: HashMap::new(),
        };

        // Create Dockerfile
        std::fs::write(temp_dir.path().join("Dockerfile"), "FROM alpine:3.19\n").unwrap();

        // Create files that affect build
        std::fs::write(temp_dir.path().join("main.py"), "print('hello')").unwrap();
        std::fs::write(temp_dir.path().join("requirements.txt"), "flask==2.0.0").unwrap();

        // Create files that don't affect build
        std::fs::write(temp_dir.path().join("README.md"), "# Project").unwrap();
        std::fs::write(temp_dir.path().join(".gitignore"), "*.pyc").unwrap();

        let hash1 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();

        // Modifying non-build-affecting file should not change hash
        std::fs::write(temp_dir.path().join("README.md"), "# Updated Project").unwrap();
        let hash2 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();
        assert_eq!(
            hash1, hash2,
            "Hash should not change when non-build-affecting files change"
        );

        // Modifying build-affecting file should change hash
        std::fs::write(temp_dir.path().join("main.py"), "print('updated')").unwrap();
        let hash3 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();
        assert_ne!(
            hash1, hash3,
            "Hash should change when build-affecting files change"
        );
    }

    #[test]
    fn test_config_hash_recursive_directory_traversal() {
        let temp_dir = tempfile::tempdir().unwrap();
        let build_config = BuildConfig {
            dockerfile: "Dockerfile".to_string(),
            dockerfile_path: temp_dir.path().join("Dockerfile"),
            context: ".".to_string(),
            context_folder: temp_dir.path().to_path_buf(),
            target: None,
            options: HashMap::new(),
        };

        // Create Dockerfile
        std::fs::write(temp_dir.path().join("Dockerfile"), "FROM alpine:3.19\n").unwrap();

        // Create nested directory structure
        let src_dir = temp_dir.path().join("src");
        let utils_dir = src_dir.join("utils");
        std::fs::create_dir_all(&utils_dir).unwrap();

        // Create files in nested directories
        std::fs::write(src_dir.join("main.py"), "print('hello')").unwrap();
        std::fs::write(utils_dir.join("helper.py"), "def help(): pass").unwrap();

        let hash1 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();

        // Modify nested file should change hash
        std::fs::write(utils_dir.join("helper.py"), "def help(): return 'updated'").unwrap();
        let hash2 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();
        assert_ne!(hash1, hash2, "Hash should change when nested file changes");

        // Add non-affecting file in nested directory should not change hash
        std::fs::write(utils_dir.join("README.md"), "# Utils module").unwrap();
        let hash3 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();
        assert_eq!(
            hash2, hash3,
            "Hash should not change when non-affecting nested file is added"
        );
    }

    #[test]
    fn test_config_hash_excludes_devcontainer_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let build_config = BuildConfig {
            dockerfile: "Dockerfile".to_string(),
            dockerfile_path: temp_dir.path().join("Dockerfile"),
            context: ".".to_string(),
            context_folder: temp_dir.path().to_path_buf(),
            target: None,
            options: HashMap::new(),
        };

        // Create Dockerfile
        std::fs::write(temp_dir.path().join("Dockerfile"), "FROM alpine:3.19\n").unwrap();

        // Create .devcontainer directory with cache
        let devcontainer_dir = temp_dir.path().join(".devcontainer");
        let cache_dir = devcontainer_dir.join("build-cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(devcontainer_dir.join("devcontainer.json"), "{}").unwrap();

        let hash1 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();

        // Add/modify files in .devcontainer should not change hash
        std::fs::write(cache_dir.join("somecache.json"), "{}").unwrap();
        std::fs::write(devcontainer_dir.join("another_file.json"), "{}").unwrap();
        let hash2 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();
        assert_eq!(
            hash1, hash2,
            "Hash should not change when .devcontainer directory contents change"
        );
    }

    #[test]
    fn test_cache_paths() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace = temp_dir.path();
        let config_hash = "abcd1234efgh5678";

        let cache_dir = get_build_cache_dir(workspace);
        let expected_cache_dir = workspace.join(".devcontainer").join("build-cache");
        assert_eq!(cache_dir, expected_cache_dir);

        let cache_file = get_build_cache_path(workspace, config_hash);
        let expected_cache_file = expected_cache_dir.join("abcd1234efgh5678.json");
        assert_eq!(cache_file, expected_cache_file);
    }

    #[test]
    fn test_build_metadata_serialization() {
        let build_result = BuildResult {
            image_id: "sha256:abcd1234".to_string(),
            tags: vec!["myapp:latest".to_string()],
            build_duration: 123.45,
            metadata: {
                let mut map = HashMap::new();
                map.insert("test".to_string(), "value".to_string());
                map
            },
            config_hash: "hash123".to_string(),
            injected_ca_subjects: Vec::new(),
        };

        let inputs = BuildInputs {
            dockerfile_hash: "dockerfile_hash".to_string(),
            context_files: vec![ContextFile {
                path: "main.py".to_string(),
                size: 100,
                mtime: 1234567890,
            }],
            feature_set_digest: Some("features_hash".to_string()),
            build_config: BuildConfig {
                dockerfile: "Dockerfile".to_string(),
                dockerfile_path: PathBuf::from("Dockerfile"),
                context: ".".to_string(),
                context_folder: PathBuf::from("."),
                target: None,
                options: HashMap::new(),
            },
        };

        let metadata = BuildMetadata {
            config_hash: "hash123".to_string(),
            result: build_result,
            inputs,
            created_at: 1234567890,
        };

        // Test serialization
        let json = serde_json::to_string(&metadata).unwrap();
        assert!(!json.is_empty());

        // Test deserialization
        let deserialized: BuildMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.config_hash, metadata.config_hash);
        assert_eq!(deserialized.result.image_id, metadata.result.image_id);
        assert_eq!(
            deserialized.inputs.dockerfile_hash,
            metadata.inputs.dockerfile_hash
        );
    }

    #[test]
    fn test_token_substitution() {
        let template = "trivy image {image}";
        let image_id = "sha256:abc123def456";
        let result = substitute_tokens(template, image_id).unwrap();
        assert_eq!(result, "trivy image sha256:abc123def456");

        // Test with multiple occurrences
        let template = "scanner --image {image} --output /tmp/{image}.json";
        let result = substitute_tokens(template, image_id).unwrap();
        assert_eq!(
            result,
            "scanner --image sha256:abc123def456 --output /tmp/sha256:abc123def456.json"
        );

        // Test with no tokens
        let template = "trivy image latest";
        let result = substitute_tokens(template, image_id).unwrap();
        assert_eq!(result, "trivy image latest");
    }

    #[test]
    fn test_build_args_with_scan_options() {
        let args = BuildArgs {
            scan_image: true,
            fail_on_scan: true,
            ..BuildArgs::default()
        };

        assert!(args.scan_image);
        assert!(args.fail_on_scan);
    }

    #[test]
    fn test_build_args_default_scan_options() {
        let args = BuildArgs::default();
        assert!(!args.scan_image);
        assert!(!args.fail_on_scan);
    }

    #[test]
    fn test_shell_command_parsing() {
        // Test that shell command parsing handles quoted arguments correctly
        let command_simple = "trivy image my-image";
        let parts_simple = shell_words::split(command_simple).unwrap();
        assert_eq!(parts_simple, vec!["trivy", "image", "my-image"]);

        // Test with quoted arguments
        let command_quoted = r#"sh -c "trivy image --severity 'CRITICAL,HIGH' my-image""#;
        let parts_quoted = shell_words::split(command_quoted).unwrap();
        assert_eq!(
            parts_quoted,
            vec![
                "sh",
                "-c",
                "trivy image --severity 'CRITICAL,HIGH' my-image"
            ]
        );

        // Test with spaces in arguments
        let command_spaces = r#"scanner --output "/path with spaces/scan.json" my-image"#;
        let parts_spaces = shell_words::split(command_spaces).unwrap();
        assert_eq!(
            parts_spaces,
            vec![
                "scanner",
                "--output",
                "/path with spaces/scan.json",
                "my-image"
            ]
        );
    }

    #[test]
    fn test_build_secret_parse_file_source() {
        let spec = "id=mytoken,src=/path/to/secret.txt";
        let secret = BuildSecret::parse(spec).unwrap();
        assert_eq!(secret.id, "mytoken");
        assert_eq!(
            secret.source,
            BuildSecretSource::File(PathBuf::from("/path/to/secret.txt"))
        );
    }

    #[test]
    fn test_build_secret_parse_env_source() {
        let spec = "id=apikey,env=API_TOKEN";
        let secret = BuildSecret::parse(spec).unwrap();
        assert_eq!(secret.id, "apikey");
        assert_eq!(
            secret.source,
            BuildSecretSource::Env("API_TOKEN".to_string())
        );
    }

    #[test]
    fn test_build_secret_parse_stdin_default() {
        let spec = "id=password";
        let secret = BuildSecret::parse(spec).unwrap();
        assert_eq!(secret.id, "password");
        assert_eq!(secret.source, BuildSecretSource::Stdin);
    }

    #[test]
    fn test_build_secret_parse_stdin_explicit_value_stdin() {
        let spec = "id=password,value-stdin";
        let secret = BuildSecret::parse(spec).unwrap();
        assert_eq!(secret.id, "password");
        assert_eq!(secret.source, BuildSecretSource::Stdin);
    }

    #[test]
    fn test_build_secret_parse_stdin_explicit_stdin() {
        let spec = "id=password,stdin";
        let secret = BuildSecret::parse(spec).unwrap();
        assert_eq!(secret.id, "password");
        assert_eq!(secret.source, BuildSecretSource::Stdin);
    }

    #[test]
    fn test_build_secret_parse_stdin_flag_with_src_error() {
        let spec = "id=test,stdin,src=/path/to/file";
        let result = BuildSecret::parse(spec);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("cannot specify 'value-stdin' or 'stdin' flag with 'src' or 'env'")
        );
    }

    #[test]
    fn test_build_secret_parse_stdin_flag_with_env_error() {
        let spec = "id=test,value-stdin,env=MY_VAR";
        let result = BuildSecret::parse(spec);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("cannot specify 'value-stdin' or 'stdin' flag with 'src' or 'env'")
        );
    }

    #[test]
    fn test_build_secret_parse_missing_id() {
        let spec = "src=/path/to/file";
        let result = BuildSecret::parse(spec);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("must specify 'id'")
        );
    }

    #[test]
    fn test_build_secret_parse_empty_id() {
        let spec = "id=,src=/path/to/file";
        let result = BuildSecret::parse(spec);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_build_secret_parse_both_src_and_env() {
        let spec = "id=test,src=/path,env=VAR";
        let result = BuildSecret::parse(spec);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("cannot specify both")
        );
    }

    #[test]
    fn test_build_secret_parse_unknown_parameter() {
        let spec = "id=test,unknown=value";
        let result = BuildSecret::parse(spec);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unknown build secret parameter")
        );
    }

    #[test]
    fn test_build_secret_parse_unknown_flag() {
        let spec = "id=test,invalid";
        let result = BuildSecret::parse(spec);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unknown build secret parameter")
        );
    }

    #[test]
    fn test_build_secret_validate_missing_file() {
        let secret = BuildSecret {
            id: "test".to_string(),
            source: BuildSecretSource::File(PathBuf::from("/nonexistent/path")),
        };
        let result = secret.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_build_secret_validate_missing_env() {
        // Make sure this env var doesn't exist
        temp_env::with_var_unset("NONEXISTENT_SECRET_VAR_12345", || {
            let secret = BuildSecret {
                id: "test".to_string(),
                source: BuildSecretSource::Env("NONEXISTENT_SECRET_VAR_12345".to_string()),
            };
            let result = secret.validate();
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("is not set"));
        });
    }

    #[test]
    fn test_build_secret_to_docker_arg_file() {
        let secret = BuildSecret {
            id: "mytoken".to_string(),
            source: BuildSecretSource::File(PathBuf::from("/secrets/token.txt")),
        };
        let docker_arg = secret.to_docker_arg(None);
        assert_eq!(docker_arg, "id=mytoken,src=/secrets/token.txt");
    }

    #[test]
    fn test_build_secret_to_docker_arg_with_temp() {
        let secret = BuildSecret {
            id: "apikey".to_string(),
            source: BuildSecretSource::Env("API_KEY".to_string()),
        };
        let temp_path = PathBuf::from("/tmp/secret123");
        let docker_arg = secret.to_docker_arg(Some(&temp_path));
        assert_eq!(docker_arg, "id=apikey,src=/tmp/secret123");
    }

    #[tokio::test]
    async fn test_build_secret_read_from_env() {
        temp_env::async_with_vars(
            [("TEST_BUILD_SECRET_12345", Some("secret_value_here"))],
            async {
                let secret = BuildSecret {
                    id: "test".to_string(),
                    source: BuildSecretSource::Env("TEST_BUILD_SECRET_12345".to_string()),
                };
                let value = secret.read_value().await.unwrap();
                assert_eq!(value, "secret_value_here");
            },
        )
        .await;
    }

    #[tokio::test]
    async fn test_build_secret_read_from_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let secret_file = temp_dir.path().join("secret.txt");
        std::fs::write(&secret_file, "my_secret_token\n").unwrap();

        let secret = BuildSecret {
            id: "test".to_string(),
            source: BuildSecretSource::File(secret_file),
        };
        let value = secret.read_value().await.unwrap();
        assert_eq!(value, "my_secret_token");
    }

    // =========================================================================
    // PR-4c: features-during-build (helpers tested in isolation)
    // =========================================================================

    #[test]
    fn is_readonly_filesystem_error_detects_permission_denied() {
        // EACCES surfaces as PermissionDenied — the most common cause of
        // a "can't write the lockfile" path on container CI mounts.
        let inner = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let err: anyhow::Error = anyhow::anyhow!(inner).context("write failed");
        assert!(super::is_readonly_filesystem_error(&err));
    }

    #[test]
    fn is_readonly_filesystem_error_ignores_unrelated_io_errors() {
        // Other IO errors (NotFound, BrokenPipe, etc.) must propagate —
        // downgrading them all would hide real bugs.
        let inner = std::io::Error::from(std::io::ErrorKind::NotFound);
        let err: anyhow::Error = anyhow::anyhow!(inner).context("read failed");
        assert!(!super::is_readonly_filesystem_error(&err));
    }

    #[test]
    fn is_readonly_filesystem_error_ignores_non_io_errors() {
        let err = anyhow::anyhow!("not an io error");
        assert!(!super::is_readonly_filesystem_error(&err));
    }
}
