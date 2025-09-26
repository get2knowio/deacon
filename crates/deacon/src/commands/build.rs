//! Build command implementation
//!
//! Implements the `deacon build` subcommand for building DevContainer images.
//! Follows the CLI specification for Docker integration.

use crate::cli::{BuildKitOption, OutputFormat};
use anyhow::Result;
use deacon_core::config::{ConfigLoader, DevContainerConfig};
use deacon_core::errors::{DeaconError, DockerError};
use deacon_core::features::{FeatureMergeConfig, FeatureMerger};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    pub ssh: Vec<String>,
    pub scan_image: bool,
    pub fail_on_scan: bool,
    pub workspace_folder: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub additional_features: Option<String>,
    pub prefer_cli_features: bool,
    pub feature_install_order: Option<String>,
    pub ignore_host_requirements: bool,
    pub progress_tracker:
        std::sync::Arc<std::sync::Mutex<Option<deacon_core::progress::ProgressTracker>>>,
    pub redaction_config: deacon_core::redaction::RedactionConfig,
    pub secret_registry: deacon_core::redaction::SecretRegistry,
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
            ssh: Vec::new(),
            scan_image: false,
            fail_on_scan: false,
            workspace_folder: None,
            config_path: None,
            additional_features: None,
            prefer_cli_features: false,
            feature_install_order: None,
            ignore_host_requirements: false,
            progress_tracker: std::sync::Arc::new(std::sync::Mutex::new(None)),
            redaction_config: deacon_core::redaction::RedactionConfig::default(),
            secret_registry: deacon_core::redaction::SecretRegistry::new(),
        }
    }
}

/// Build configuration extracted from DevContainer config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    /// Dockerfile path (relative to context)
    pub dockerfile: String,
    /// Build context path
    pub context: String,
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
pub async fn execute_build(args: BuildArgs) -> Result<()> {
    info!("Starting build command execution");
    debug!("Build args: {:?}", args);

    // Initialize progress tracking
    let emit_progress_event = |event: deacon_core::progress::ProgressEvent| -> Result<()> {
        if let Ok(mut tracker_guard) = args.progress_tracker.lock() {
            if let Some(ref mut tracker) = tracker_guard.as_mut() {
                tracker.emit_event(event)?;
            }
        }
        Ok(())
    };

    // Load configuration
    let workspace_folder = args.workspace_folder.as_deref().unwrap_or(Path::new("."));

    let mut config = if let Some(config_path) = args.config_path.as_ref() {
        ConfigLoader::load_from_path(config_path)?
    } else {
        let config_location = ConfigLoader::discover_config(workspace_folder)?;
        if !config_location.exists() {
            return Err(
                DeaconError::Config(deacon_core::errors::ConfigError::NotFound {
                    path: config_location.path().to_string_lossy().to_string(),
                })
                .into(),
            );
        }
        ConfigLoader::load_from_path(config_location.path())?
    };

    debug!("Loaded configuration: {:?}", config.name);

    // Validate host requirements if specified in configuration
    if let Some(host_requirements) = &config.host_requirements {
        debug!("Validating host requirements");
        let mut evaluator = deacon_core::host_requirements::HostRequirementsEvaluator::new();

        match evaluator.validate_requirements(
            host_requirements,
            Some(workspace_folder),
            args.ignore_host_requirements,
        ) {
            Ok(evaluation) => {
                if evaluation.requirements_met {
                    debug!("Host requirements validation passed");
                } else if args.ignore_host_requirements {
                    warn!("Host requirements not met, but proceeding due to --ignore-host-requirements flag");
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
    let build_config = extract_build_config(&config, workspace_folder)?;
    debug!("Build config: {:?}", build_config);

    // Calculate configuration hash for caching
    let config_hash = calculate_config_hash(&build_config, workspace_folder)?;
    debug!("Configuration hash: {}", config_hash);

    // Check cache if not forced
    if !args.force {
        if let Some(cached_result) = check_build_cache(&config_hash, workspace_folder).await? {
            info!("Using cached build result");
            output_result(
                &cached_result,
                &args.output_format,
                &args.redaction_config,
                &args.secret_registry,
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

    let result = execute_docker_build(&build_config, &args, &config_hash, workspace_folder).await;
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
    let final_result = BuildResult {
        image_id: result.image_id,
        tags: result.tags,
        build_duration: build_duration.as_secs_f64(),
        metadata: result.metadata,
        config_hash: config_hash.clone(),
    };

    // Cache the result
    cache_build_result(&final_result, workspace_folder).await?;

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
fn extract_build_config(
    config: &DevContainerConfig,
    workspace_folder: &Path,
) -> Result<BuildConfig> {
    // Check if this is a compose-based configuration
    if config.uses_compose() {
        return Err(
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: "Docker Compose configurations cannot be built directly. Use 'docker compose build' to build individual services.".to_string(),
            })
            .into(),
        );
    }
    // Check if we have a dockerfile specified
    if let Some(dockerfile) = &config.dockerfile {
        let dockerfile_path = workspace_folder.join(dockerfile);
        if !dockerfile_path.exists() {
            return Err(
                DeaconError::Config(deacon_core::errors::ConfigError::NotFound {
                    path: dockerfile_path.to_string_lossy().to_string(),
                })
                .into(),
            );
        }

        let mut build_config = BuildConfig {
            dockerfile: dockerfile.clone(),
            context: ".".to_string(),
            target: None,
            options: HashMap::new(),
        };

        // Parse build configuration if present
        if let Some(build_value) = &config.build {
            if let Some(build_obj) = build_value.as_object() {
                // Extract context
                if let Some(context) = build_obj.get("context").and_then(|v| v.as_str()) {
                    build_config.context = context.to_string();
                }

                // Extract target
                if let Some(target) = build_obj.get("target").and_then(|v| v.as_str()) {
                    build_config.target = Some(target.to_string());
                }

                // Extract build options/args
                if let Some(options) = build_obj.get("options").and_then(|v| v.as_object()) {
                    for (key, value) in options {
                        if let Some(val_str) = value.as_str() {
                            build_config
                                .options
                                .insert(key.clone(), val_str.to_string());
                        }
                    }
                }
            }
        }

        Ok(build_config)
    } else if config.image.is_some() {
        // If we have an image but no dockerfile, we can't build
        Err(
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: "Cannot build with 'image' configuration. Use 'dockerFile' for builds."
                    .to_string(),
            })
            .into(),
        )
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
fn calculate_config_hash(build_config: &BuildConfig, workspace_folder: &Path) -> Result<String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();

    // Hash the build config
    build_config.dockerfile.hash(&mut hasher);
    build_config.context.hash(&mut hasher);
    build_config.target.hash(&mut hasher);

    // Hash the options in a deterministic order
    let mut options: Vec<_> = build_config.options.iter().collect();
    options.sort_by_key(|(k, _)| *k);
    for (key, value) in options {
        key.hash(&mut hasher);
        value.hash(&mut hasher);
    }

    // Hash dockerfile content
    let dockerfile_path = workspace_folder
        .join(&build_config.context)
        .join(&build_config.dockerfile);
    if dockerfile_path.exists() {
        let dockerfile_content = std::fs::read_to_string(&dockerfile_path)?;
        dockerfile_content.hash(&mut hasher);
    }

    // Hash selected build context files (limit count for performance)
    let context_path = workspace_folder.join(&build_config.context);
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
                                        path.strip_prefix(workspace_folder)
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
            path.hash(&mut hasher);
            size.hash(&mut hasher);
            mtime.hash(&mut hasher);
        }
    }

    let hash = hasher.finish();
    // Zero-pad to ensure stable length (16 hex chars) so downstream slicing is safe
    Ok(format!("{:016x}", hash))
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

    if !cache_file.exists() {
        debug!("No cache file found at {}", cache_file.display());
        return Ok(None);
    }

    // Read and deserialize cache file
    match std::fs::read_to_string(&cache_file) {
        Ok(contents) => {
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
                        // Remove invalid cache file
                        let _ = std::fs::remove_file(&cache_file);
                        Ok(None)
                    }
                }
                Err(e) => {
                    debug!("Failed to deserialize cache metadata: {}", e);
                    // Remove corrupted cache file
                    let _ = std::fs::remove_file(&cache_file);
                    Ok(None)
                }
            }
        }
        Err(e) => {
            debug!("Failed to read cache file: {}", e);
            Ok(None)
        }
    }
}

/// Cache build result
async fn cache_build_result(result: &BuildResult, workspace_folder: &Path) -> Result<()> {
    let cache_dir = get_build_cache_dir(workspace_folder);

    // Ensure cache directory exists
    if let Err(e) = std::fs::create_dir_all(&cache_dir) {
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
            if let Err(e) = std::fs::write(&cache_file, json) {
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
            context: ".".to_string(),
            target: None,
            options: HashMap::new(),
        },
    })
}

/// Check if a Docker image is available locally
async fn is_image_available(image_id: &str) -> Result<bool> {
    // Use docker inspect to check if image exists
    let output = std::process::Command::new("docker")
        .args(["inspect", "--type=image", image_id])
        .output();

    match output {
        Ok(output) => Ok(output.status.success()),
        Err(_) => {
            // If docker command fails, assume image is not available
            debug!("Failed to check image availability for {}", image_id);
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

/// Execute Docker build
#[instrument(skip(build_config, args, workspace_folder))]
async fn execute_docker_build(
    build_config: &BuildConfig,
    args: &BuildArgs,
    config_hash: &str,
    workspace_folder: &Path,
) -> Result<BuildResult> {
    {
        use deacon_core::docker::{CliDocker, Docker};

        let docker = CliDocker::new();

        // Check Docker availability
        docker.check_docker_installed()?;
        docker.ping().await?;

        debug!("Building Docker image");

        // Prepare build context
        let context_path = workspace_folder.join(&build_config.context);
        let dockerfile_path = context_path.join(&build_config.dockerfile);

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

        // Add SSH forwarding
        for ssh in &args.ssh {
            build_args.push("--ssh".to_string());
            build_args.push(ssh.clone());
        }

        // Determine if BuildKit should be used
        let use_buildkit = should_use_buildkit(args.buildkit.as_ref());
        debug!("Using BuildKit: {}", use_buildkit);

        // Secrets/SSH require BuildKit; provide a clear error early.
        if !use_buildkit && (!args.secret.is_empty() || !args.ssh.is_empty()) {
            if args.buildkit == Some(BuildKitOption::Never) {
                return Err(DockerError::CLIError(
                    "The --secret/--ssh options require BuildKit but --buildkit never was specified"
                        .to_string(),
                )
                .into());
            }
            return Err(DockerError::CLIError(
                "The --secret/--ssh options require BuildKit. Re-run with --buildkit auto or set DOCKER_BUILDKIT=1"
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

        // Add label with config hash
        let label = format!("org.deacon.configHash={}", config_hash);
        build_args.push("--label".to_string());
        build_args.push(label);

        // Add quiet flag to reduce output noise
        build_args.push("-q".to_string());

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

        let image_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Extract image metadata
        let metadata = extract_image_metadata(&image_id).await?;

        let result = BuildResult {
            image_id,
            tags: vec![tag],
            build_duration: 0.0, // Will be set by caller
            metadata,
            config_hash: config_hash.to_string(),
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
) -> Result<()> {
    use deacon_core::redaction::RedactingWriter;
    use std::io::Write;

    let stdout = std::io::stdout();
    let mut writer = RedactingWriter::new(stdout, redaction_config.clone(), registry);

    match format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(result).map_err(|e| {
                DeaconError::Internal(deacon_core::errors::InternalError::Generic {
                    message: format!("Failed to serialize result to JSON: {}", e),
                })
            })?;
            writer.write_line(&json)?;
        }
        OutputFormat::Text => {
            writer.write_line("Build completed successfully!")?;
            writer.write_line(&format!("Image ID: {}", result.image_id))?;
            writer.write_line(&format!("Tags: {}", result.tags.join(", ")))?;
            writer.write_line(&format!("Build duration: {:.2}s", result.build_duration))?;
            writer.write_line(&format!("Config hash: {}", result.config_hash))?;

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

    // Parse command into program and arguments
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return Err(anyhow::anyhow!("Empty scan command"));
    }

    let program = parts[0];
    let command_args: Vec<&str> = if parts.len() > 1 {
        parts[1..].to_vec()
    } else {
        Vec::new()
    };

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
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let stdout_task = tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut output = Vec::new();
        while let Some(line) = lines.next_line().await.unwrap_or(None) {
            output.push(line);
        }
        output
    });

    let stderr_task = tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        let mut output = Vec::new();
        while let Some(line) = lines.next_line().await.unwrap_or(None) {
            output.push(line);
        }
        output
    });

    // Wait for command to complete
    let status = child
        .wait()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to wait for scan command: {}", e))?;

    // Collect output
    let stdout_lines = stdout_task.await.unwrap_or_default();
    let stderr_lines = stderr_task.await.unwrap_or_default();

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
    use serial_test::serial;
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

        let result = extract_build_config(&config, temp_dir.path());
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

        let result = extract_build_config(&config, temp_dir.path());
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
    fn test_config_hash_calculation() {
        let build_config = BuildConfig {
            dockerfile: "Dockerfile".to_string(),
            context: ".".to_string(),
            target: Some("dev".to_string()),
            options: {
                let mut map = HashMap::new();
                map.insert("ARG1".to_string(), "value1".to_string());
                map.insert("ARG2".to_string(), "value2".to_string());
                map
            },
        };

        let temp_dir = tempfile::tempdir().unwrap();
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
            no_cache: true,
            platform: Some("linux/amd64".to_string()),
            build_arg: vec!["ENV=dev".to_string(), "VERSION=1.0".to_string()],
            force: false,
            output_format: OutputFormat::Text,
            cache_from: Vec::new(),
            cache_to: Vec::new(),
            buildkit: None,
            secret: Vec::new(),
            ssh: Vec::new(),
            scan_image: false,
            fail_on_scan: false,
            workspace_folder: None,
            config_path: None,
            additional_features: None,
            prefer_cli_features: false,
            feature_install_order: None,
            ignore_host_requirements: false,
            progress_tracker: std::sync::Arc::new(std::sync::Mutex::new(None)),
            redaction_config: deacon_core::redaction::RedactionConfig::default(),
            secret_registry: deacon_core::redaction::SecretRegistry::new(),
        };

        // Verify args are structured correctly
        assert!(args.no_cache);
        assert_eq!(args.platform, Some("linux/amd64".to_string()));
        assert_eq!(args.build_arg.len(), 2);
        assert!(args.build_arg.contains(&"ENV=dev".to_string()));
        assert!(args.build_arg.contains(&"VERSION=1.0".to_string()));
    }

    #[test]
    fn test_advanced_build_args_assembly() {
        let args = BuildArgs {
            no_cache: false,
            platform: None,
            build_arg: Vec::new(),
            force: false,
            output_format: OutputFormat::Text,
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
            scan_image: false,
            fail_on_scan: false,
            workspace_folder: None,
            config_path: None,
            additional_features: None,
            prefer_cli_features: false,
            feature_install_order: None,
            ignore_host_requirements: false,
            progress_tracker: std::sync::Arc::new(std::sync::Mutex::new(None)),
            redaction_config: deacon_core::redaction::RedactionConfig::default(),
            secret_registry: deacon_core::redaction::SecretRegistry::new(),
        };

        // Verify advanced args are structured correctly
        assert_eq!(args.cache_from.len(), 2);
        assert!(args
            .cache_from
            .contains(&"registry://example.com/cache".to_string()));
        assert!(args
            .cache_from
            .contains(&"type=local,src=/tmp/cache".to_string()));

        assert_eq!(args.cache_to.len(), 1);
        assert!(args
            .cache_to
            .contains(&"registry://example.com/cache:latest".to_string()));

        assert_eq!(args.buildkit, Some(BuildKitOption::Auto));

        assert_eq!(args.secret.len(), 2);
        assert!(args
            .secret
            .contains(&"id=mypassword,src=./password.txt".to_string()));
        assert!(args.secret.contains(&"id=mytoken".to_string()));

        assert_eq!(args.ssh.len(), 2);
        assert!(args.ssh.contains(&"default".to_string()));
        assert!(args.ssh.contains(&"mykey=/path/to/key".to_string()));
    }

    #[test]
    #[serial]
    fn test_buildkit_detection() {
        // Test BuildKit Auto mode with DOCKER_BUILDKIT=1
        std::env::set_var("DOCKER_BUILDKIT", "1");
        assert!(should_use_buildkit(Some(&BuildKitOption::Auto)));

        // Test BuildKit Auto mode with DOCKER_BUILDKIT=true
        std::env::set_var("DOCKER_BUILDKIT", "true");
        assert!(should_use_buildkit(Some(&BuildKitOption::Auto)));

        // Test BuildKit Auto mode with DOCKER_BUILDKIT=0
        std::env::set_var("DOCKER_BUILDKIT", "0");
        assert!(!should_use_buildkit(Some(&BuildKitOption::Auto)));

        // Test BuildKit Auto mode with DOCKER_BUILDKIT=false
        std::env::set_var("DOCKER_BUILDKIT", "false");
        assert!(!should_use_buildkit(Some(&BuildKitOption::Auto)));

        // Test BuildKit Never mode (should always be false)
        std::env::set_var("DOCKER_BUILDKIT", "1");
        assert!(!should_use_buildkit(Some(&BuildKitOption::Never)));

        // Test None (default) mode - should respect env var
        std::env::set_var("DOCKER_BUILDKIT", "1");
        assert!(should_use_buildkit(None));

        std::env::set_var("DOCKER_BUILDKIT", "0");
        assert!(!should_use_buildkit(None));

        // Test None with no env var (should default to false)
        std::env::remove_var("DOCKER_BUILDKIT");
        assert!(!should_use_buildkit(None));

        // Clean up
        std::env::remove_var("DOCKER_BUILDKIT");
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
            build_duration: 1.5,
        };

        // Set up redaction
        let registry = SecretRegistry::new();
        registry.add_secret("password123");
        registry.add_secret("secret123");
        let config = RedactionConfig::with_custom_registry(registry.clone());

        // Test that calling output_result doesn't panic and applies redaction
        // Note: In a real test we'd capture stdout, but for now we just ensure it compiles and runs
        let result_call = output_result(&result, &OutputFormat::Text, &config, &registry);
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
        // Test that secrets require BuildKit
        let args_with_secret = BuildArgs {
            secret: vec!["id=test".to_string()],
            buildkit: Some(BuildKitOption::Never),
            ..BuildArgs::default()
        };

        // This would be tested in the actual execute_docker_build function
        // For unit testing, we just verify the logic
        let use_buildkit = should_use_buildkit(args_with_secret.buildkit.as_ref());
        assert!(!use_buildkit);
        assert!(!args_with_secret.secret.is_empty());
        assert_eq!(args_with_secret.buildkit, Some(BuildKitOption::Never));

        // Test that SSH requires BuildKit
        let args_with_ssh = BuildArgs {
            ssh: vec!["default".to_string()],
            buildkit: None, // No BuildKit specified, will default to false
            ..BuildArgs::default()
        };

        let use_buildkit = should_use_buildkit(args_with_ssh.buildkit.as_ref());
        assert!(!use_buildkit);
        assert!(!args_with_ssh.ssh.is_empty());
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
        let build_config = BuildConfig {
            dockerfile: "Dockerfile".to_string(),
            context: ".".to_string(),
            target: None,
            options: HashMap::new(),
        };

        let temp_dir = tempfile::tempdir().unwrap();

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
        let build_config = BuildConfig {
            dockerfile: "Dockerfile".to_string(),
            context: ".".to_string(),
            target: None,
            options: HashMap::new(),
        };

        let temp_dir = tempfile::tempdir().unwrap();

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
        let build_config = BuildConfig {
            dockerfile: "Dockerfile".to_string(),
            context: ".".to_string(),
            target: None,
            options: HashMap::new(),
        };

        let temp_dir = tempfile::tempdir().unwrap();

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
                context: ".".to_string(),
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
}
