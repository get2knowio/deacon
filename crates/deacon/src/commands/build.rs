//! Build command implementation
//!
//! Implements the `deacon build` subcommand for building DevContainer images.
//! Follows the CLI specification for Docker integration.

use crate::cli::OutputFormat;
use anyhow::Result;
use deacon_core::config::{ConfigLoader, DevContainerConfig};
use deacon_core::errors::{DeaconError, DockerError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{debug, info, instrument};

/// Build command arguments
#[derive(Debug, Clone)]
pub struct BuildArgs {
    pub no_cache: bool,
    pub platform: Option<String>,
    pub build_arg: Vec<String>,
    pub force: bool,
    pub output_format: OutputFormat,
    pub workspace_folder: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
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

/// Execute the build command
#[instrument(skip(args))]
pub async fn execute_build(args: BuildArgs) -> Result<()> {
    info!("Starting build command execution");
    debug!("Build args: {:?}", args);

    // Load configuration
    let workspace_folder = args.workspace_folder.as_deref().unwrap_or(Path::new("."));

    let config = if let Some(config_path) = args.config_path.as_ref() {
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
            output_result(&cached_result, &args.output_format)?;
            return Ok(());
        }
    }

    // Execute build
    let start_time = Instant::now();
    let result = execute_docker_build(&build_config, &args, &config_hash, workspace_folder).await?;
    let build_duration = start_time.elapsed().as_secs_f64();

    let final_result = BuildResult {
        image_id: result.image_id,
        tags: result.tags,
        build_duration,
        metadata: result.metadata,
        config_hash: config_hash.clone(),
    };

    // Cache the result
    cache_build_result(&final_result, workspace_folder).await?;

    // Output result
    output_result(&final_result, &args.output_format)?;

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

    // Hash context directory mtime (simple approach)
    let context_path = workspace_folder.join(&build_config.context);
    if context_path.exists() {
        let metadata = std::fs::metadata(&context_path)?;
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                duration.as_secs().hash(&mut hasher);
            }
        }
    }

    let hash = hasher.finish();
    Ok(format!("{:x}", hash))
}

/// Check for cached build result
async fn check_build_cache(
    _config_hash: &str,
    _workspace_folder: &Path,
) -> Result<Option<BuildResult>> {
    // For now, always return None (no cache hit)
    // TODO: Implement proper cache storage and retrieval
    debug!("Cache check not implemented yet");
    Ok(None)
}

/// Cache build result
async fn cache_build_result(_result: &BuildResult, _workspace_folder: &Path) -> Result<()> {
    // For now, do nothing
    // TODO: Implement proper cache storage
    debug!("Cache storage not implemented yet");
    Ok(())
}

/// Execute Docker build
#[instrument(skip(build_config, args, workspace_folder))]
async fn execute_docker_build(
    build_config: &BuildConfig,
    args: &BuildArgs,
    config_hash: &str,
    workspace_folder: &Path,
) -> Result<BuildResult> {
    #[cfg(feature = "docker")]
    {
        use deacon_core::docker::{CliDocker, Docker};
        use std::process::Command;

        let docker = CliDocker::new();

        // Check Docker availability
        docker.check_docker_installed()?;
        docker.ping().await?;

        info!("Building Docker image");

        // Prepare build context
        let context_path = workspace_folder.join(&build_config.context);
        let dockerfile_path = context_path.join(&build_config.dockerfile);

        // Prepare docker build arguments
        let mut build_args = vec!["build".to_string()];

        // Add context
        build_args.push(
            context_path
                .to_str()
                .ok_or_else(|| {
                    DeaconError::Docker(DockerError::CLIError("Invalid context path".to_string()))
                })?
                .to_string(),
        );

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

        debug!("Docker build command: docker {}", build_args.join(" "));

        // Execute docker build
        let output = Command::new("docker")
            .args(&build_args) // Pass all args including "build" subcommand
            .current_dir(workspace_folder)
            .output()
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

        info!("Docker build completed successfully");
        Ok(result)
    }

    #[cfg(not(feature = "docker"))]
    {
        Err(DeaconError::Docker(DockerError::CLIError(
            "Docker support not available (compiled without 'docker' feature)".to_string(),
        ))
        .into())
    }
}

/// Extract image metadata using docker inspect
async fn extract_image_metadata(image_id: &str) -> Result<HashMap<String, String>> {
    #[cfg(feature = "docker")]
    {
        use std::process::Command;

        debug!("Extracting metadata for image: {}", image_id);

        let output = Command::new("docker")
            .args(["inspect", "--format={{json .Config.Labels}}", image_id])
            .output()
            .map_err(|e| DockerError::CLIError(format!("Failed to inspect image: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DockerError::CLIError(format!("Docker inspect failed: {}", stderr)).into());
        }

        let labels_json = String::from_utf8_lossy(&output.stdout);
        let labels: HashMap<String, String> = if labels_json.trim() == "null" {
            HashMap::new()
        } else {
            serde_json::from_str(&labels_json).map_err(|e| {
                DockerError::CLIError(format!("Failed to parse image labels: {}", e))
            })?
        };

        debug!("Extracted {} labels from image", labels.len());
        Ok(labels)
    }

    #[cfg(not(feature = "docker"))]
    {
        Ok(HashMap::new())
    }
}

/// Output build result in the specified format
fn output_result(result: &BuildResult, format: &OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(result).map_err(|e| {
                DeaconError::Internal(deacon_core::errors::InternalError::Generic {
                    message: format!("Failed to serialize result to JSON: {}", e),
                })
            })?;
            println!("{}", json);
        }
        OutputFormat::Text => {
            println!("Build completed successfully!");
            println!("Image ID: {}", result.image_id);
            println!("Tags: {}", result.tags.join(", "));
            println!("Build duration: {:.2}s", result.build_duration);
            println!("Config hash: {}", result.config_hash);

            if !result.metadata.is_empty() {
                println!("Labels:");
                for (key, value) in &result.metadata {
                    println!("  {}: {}", key, value);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_build_config_dockerfile_parsing() {
        let mut config = DevContainerConfig {
            extends: None,
            name: Some("test".to_string()),
            dockerfile: Some("Dockerfile".to_string()),
            build: None,
            image: None,
            features: serde_json::Value::Object(Default::default()),
            customizations: serde_json::Value::Object(Default::default()),
            workspace_folder: None,
            workspace_mount: None,
            mounts: vec![],
            container_env: HashMap::new(),
            remote_env: HashMap::new(),
            container_user: None,
            remote_user: None,
            update_remote_user_uid: None,
            forward_ports: vec![],
            app_port: None,
            ports_attributes: HashMap::new(),
            other_ports_attributes: None,
            run_args: vec![],
            shutdown_action: None,
            override_command: None,
            docker_compose_file: None,
            service: None,
            run_services: vec![],
            on_create_command: None,
            post_start_command: None,
            post_create_command: None,
            post_attach_command: None,
            initialize_command: None,
            update_content_command: None,
        };

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
            workspace_folder: None,
            config_path: None,
        };

        // Verify args are structured correctly
        assert!(args.no_cache);
        assert_eq!(args.platform, Some("linux/amd64".to_string()));
        assert_eq!(args.build_arg.len(), 2);
        assert!(args.build_arg.contains(&"ENV=dev".to_string()));
        assert!(args.build_arg.contains(&"VERSION=1.0".to_string()));
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

        // Add context
        build_args.push(context_path.to_str().unwrap().to_string());

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

        // Verify the ordering: should start with "build" subcommand
        assert_eq!(build_args[0], "build");
        assert_eq!(build_args[1], context_path.to_str().unwrap());
        assert_eq!(build_args[2], "-f");
        assert_eq!(build_args[3], dockerfile_path.to_str().unwrap());
        assert_eq!(build_args[4], "--no-cache");
        assert_eq!(build_args[5], "--platform");
        assert_eq!(build_args[6], "linux/amd64");
        assert_eq!(build_args[7], "--build-arg");
        assert_eq!(build_args[8], "ENV=test");
        assert_eq!(build_args[9], "-t");
        assert_eq!(build_args[10], "deacon-build:abcd12345678");
        assert_eq!(build_args[11], "--label");
        assert_eq!(build_args[12], "org.deacon.configHash=abcd1234567890");
        assert_eq!(build_args[13], "-q");

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
}
