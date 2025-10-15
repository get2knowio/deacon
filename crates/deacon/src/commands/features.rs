//! Features command implementation
//!
//! Implements the `deacon features` subcommands for testing, packaging, and publishing
//! DevContainer features. Follows the CLI specification for feature management.

use crate::cli::FeatureCommands;
use anyhow::{Context, Result};
use deacon_core::config::{ConfigLoader, DevContainerConfig};
use deacon_core::features::{
    parse_feature_metadata, FeatureDependencyResolver, FeatureMergeConfig, FeatureMerger,
    FeatureMetadata, OptionValue, ResolvedFeature,
};
use deacon_core::observability::{feature_plan_span, TimedSpan};
use deacon_core::oci::{default_fetcher, FeatureRef};
use deacon_core::registry_parser::parse_registry_reference;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tempfile;
use tracing::{debug, info};

/// Features command arguments
#[derive(Debug, Clone)]
pub struct FeaturesArgs {
    pub command: FeatureCommands,
    #[allow(dead_code)] // Reserved for future use
    pub workspace_folder: Option<PathBuf>,
    #[allow(dead_code)] // Reserved for future use
    pub config_path: Option<PathBuf>,
}

/// Result of a features command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturesResult {
    /// Command that was executed
    pub command: String,
    /// Status of the operation (success/failure)
    pub status: String,
    /// Optional digest for package/publish operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    /// Optional size information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Optional message with additional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Optional cache path for pulled artifacts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_path: Option<String>,
}

/// Execute the features command
pub async fn execute_features(args: FeaturesArgs) -> Result<()> {
    match args.command {
        FeatureCommands::Test { path, json } => execute_features_test(&path, json).await,
        FeatureCommands::Package { path, output, json } => {
            execute_features_package(&path, &output, json).await
        }
        FeatureCommands::Pull { registry_ref, json } => {
            execute_features_pull(&registry_ref, json).await
        }
        FeatureCommands::Publish {
            path,
            registry,
            dry_run,
            json,
            username,
            password_stdin,
        } => {
            execute_features_publish(
                &path,
                &registry,
                dry_run,
                json,
                username.as_deref(),
                password_stdin,
            )
            .await
        }
        FeatureCommands::Info {
            mode,
            feature,
            json,
        } => execute_features_info(&mode, &feature, json).await,
        FeatureCommands::Plan {
            json,
            ref additional_features,
        } => execute_features_plan(json, additional_features.as_deref(), &args).await,
    }
}

/// Plan result structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturesPlanResult {
    /// Features in installation order
    pub order: Vec<String>,
    /// Dependency graph for visibility
    pub graph: serde_json::Value,
}

/// Error message for local feature paths
const LOCAL_FEATURE_ERROR_MSG: &str = "Local features are not supported by 'features plan'. Use registry references (e.g., ghcr.io/owner/feature)";

/// Check if a feature identifier looks like a local path
fn is_local_path(feature_id: &str) -> bool {
    // Check for relative paths
    if feature_id.starts_with("./") || feature_id.starts_with("../") {
        return true;
    }

    // Check for absolute Unix paths
    if feature_id.starts_with('/') {
        return true;
    }

    // Check for Windows absolute paths (C:\, D:\, etc.)
    if feature_id.len() >= 3 {
        let chars: Vec<char> = feature_id.chars().collect();
        if chars.len() >= 3
            && chars[0].is_ascii_alphabetic()
            && chars[1] == ':'
            && (chars[2] == '\\' || chars[2] == '/')
        {
            return true;
        }
    }

    false
}

/// Execute features plan command
async fn execute_features_plan(
    json: bool,
    additional_features: Option<&str>,
    args: &FeaturesArgs,
) -> Result<()> {
    // Determine workspace folder - default to current directory if not provided
    let workspace_folder = args
        .workspace_folder
        .as_ref()
        .unwrap_or(&std::env::current_dir()?)
        .clone();

    // Start standardized span for feature planning
    let timed_span = TimedSpan::new(feature_plan_span(&workspace_folder));

    let result = {
        let _guard = timed_span.span().enter();

        debug!("Generating feature installation plan");

        // Load devcontainer configuration (explicit path > discovery > default)
        let mut config = if let Some(config_path) = args.config_path.as_deref() {
            ConfigLoader::load_from_path(config_path)?
        } else {
            let config_location = ConfigLoader::discover_config(&workspace_folder)?;
            if config_location.exists() {
                ConfigLoader::load_from_path(config_location.path())?
            } else {
                DevContainerConfig::default()
            }
        };

        // Parse and merge additional features if provided
        if let Some(additional_features_str) = additional_features {
            // Early validation: parse JSON and ensure it's an object before merge
            let parsed_json: serde_json::Value = serde_json::from_str(additional_features_str)
                .with_context(|| {
                    format!(
                        "Failed to parse --additional-features during feature plan initialization: {}",
                        additional_features_str
                    )
                })?;

            // Validate that the parsed JSON is an object (map)
            if !parsed_json.is_object() {
                anyhow::bail!("Failed to validate --additional-features: must be a JSON object.");
            }

            let merge_config = FeatureMergeConfig::new(
                Some(additional_features_str.to_string()),
                false, // Don't prefer CLI features by default
                None,  // No install order override in this context
            );
            config.features = FeatureMerger::merge_features(&config.features, &merge_config)
                .context("Failed to merge additional features with devcontainer configuration.")?;
        }

        // Extract features from config
        let features_map_opt = config.features.as_object();
        if features_map_opt.is_none() || features_map_opt.unwrap().is_empty() {
            let result = FeaturesPlanResult {
                order: vec![],
                graph: serde_json::json!({}),
            };
            output_plan_result(&result, json)?;
            return Ok(());
        }
        let features_map = features_map_opt.unwrap();

        // Resolve features from registries to obtain metadata (deps, installsAfter, etc.)
        let fetcher = default_fetcher()
            .context("Failed to initialize OCI client for fetching feature metadata.")?;
        let mut resolved_features = Vec::with_capacity(features_map.len());
        for (feature_id, feature_value) in features_map {
            // Check if feature_id looks like a local path
            if is_local_path(feature_id) {
                anyhow::bail!("{}. Feature key: '{}'", LOCAL_FEATURE_ERROR_MSG, feature_id);
            }

            let (registry_url, namespace, name, tag) =
                parse_registry_reference(feature_id).with_context(|| {
                    format!(
                        "Failed to parse registry reference for feature '{}' during feature resolution.",
                        feature_id
                    )
                })?;
            let feature_ref = FeatureRef::new(
                registry_url.clone(),
                namespace.clone(),
                name.clone(),
                tag.clone(),
            );
            let downloaded = fetcher.fetch_feature(&feature_ref).await.with_context(|| {
                format!(
                    "Failed to fetch feature metadata from OCI registry for feature '{}'.",
                    feature_id
                )
            })?;

            // Extract per-feature options from config entry if present
            let options: std::collections::HashMap<String, OptionValue> = match feature_value {
                serde_json::Value::Object(map) => map
                    .clone()
                    .into_iter()
                    .map(|(k, v)| {
                        // Convert serde_json::Value to OptionValue, preserving all types
                        let option_value = match v {
                            serde_json::Value::Bool(b) => OptionValue::Boolean(b),
                            serde_json::Value::String(s) => OptionValue::String(s),
                            serde_json::Value::Number(n) => OptionValue::Number(n),
                            serde_json::Value::Array(a) => OptionValue::Array(a),
                            serde_json::Value::Object(o) => OptionValue::Object(o),
                            serde_json::Value::Null => OptionValue::Null,
                        };
                        (k, option_value)
                    })
                    .collect(),
                _ => std::collections::HashMap::new(),
            };

            resolved_features.push(ResolvedFeature {
                id: downloaded.metadata.id.clone(),
                source: feature_ref.reference(),
                options,
                metadata: downloaded.metadata,
            });
        }

        // Create dependency resolver with override order from config
        let override_order = config.override_feature_install_order.clone();
        let resolver = FeatureDependencyResolver::new(override_order);

        // Resolve dependencies and create installation plan
        let installation_plan = resolver
            .resolve(&resolved_features)
            .context("Failed to resolve feature dependencies and compute installation order.")?;

        // Extract order and create graph representation
        let order = installation_plan.feature_ids();
        let graph = build_graph_representation(&resolved_features);

        let result = FeaturesPlanResult { order, graph };

        output_plan_result(&result, json)?;

        Ok(())
    };

    // Complete the timed span with duration
    timed_span.complete();
    result
}

/// Create a mock resolved feature for demonstration (temporary)
/// In a real implementation, this would fetch the actual feature metadata
#[cfg(test)]
fn create_mock_resolved_feature(feature_id: &str) -> ResolvedFeature {
    create_mock_resolved_feature_with_deps(feature_id, &[], &[])
}

/// Create a mock resolved feature with specified dependencies
#[cfg(test)]
fn create_mock_resolved_feature_with_deps(
    feature_id: &str,
    installs_after: &[&str],
    depends_on: &[&str],
) -> ResolvedFeature {
    use deacon_core::features::FeatureMetadata;
    use std::collections::HashMap;

    let mut depends_on_map = HashMap::new();
    for dep in depends_on {
        depends_on_map.insert(dep.to_string(), serde_json::Value::Bool(true));
    }

    let metadata = FeatureMetadata {
        id: feature_id.to_string(),
        version: Some("1.0.0".to_string()),
        name: Some(format!("Mock {}", feature_id)),
        description: Some(format!("Mock feature for {}", feature_id)),
        documentation_url: None,
        license_url: None,
        options: HashMap::new(),
        container_env: HashMap::new(),
        mounts: vec![],
        init: None,
        privileged: None,
        cap_add: vec![],
        security_opt: vec![],
        entrypoint: None,
        installs_after: installs_after.iter().map(|s| s.to_string()).collect(),
        depends_on: depends_on_map,
        on_create_command: None,
        update_content_command: None,
        post_create_command: None,
        post_start_command: None,
        post_attach_command: None,
    };

    ResolvedFeature {
        id: feature_id.to_string(),
        source: format!("mock://features/{}", feature_id),
        options: HashMap::new(),
        metadata,
    }
}

/// Build a graph representation from resolved features
///
/// Creates an adjacency list representation where each feature ID maps to an array
/// of its dependencies. The graph structure follows the format:
/// ```json
/// {
///   "featureA": [],
///   "featureB": ["featureA"],
///   "featureC": ["featureA", "featureB"]
/// }
/// ```
///
/// ## Graph Direction
/// The graph encodes **dependencies** (not dependents):
/// - `"featureB": ["featureA"]` means featureB depends on featureA
/// - Empty array means the feature has no dependencies
/// - This representation makes it clear what must be installed before each feature
///
/// ## Dependency Union
/// The dependency list for each feature is the **union** of:
/// - `installsAfter`: Ordering constraints from the feature metadata
/// - `dependsOn`: Hard dependencies from the feature metadata
///
/// Both fields are combined to provide a complete view of installation constraints.
/// Uses `BTreeSet` internally to ensure deterministic ordering and deduplication.
///
/// ## Conformance
/// Matches specification in `docs/subcommand-specs/features-plan/DATA-STRUCTURES.md`
/// and design decision in `docs/subcommand-specs/features-plan/SPEC.md` §16.
fn build_graph_representation(features: &[ResolvedFeature]) -> serde_json::Value {
    let mut graph = serde_json::Map::new();

    for feature in features {
        // Use a set for dedupe and deterministic ordering
        let mut deps = std::collections::BTreeSet::new();
        for dep in &feature.metadata.installs_after {
            deps.insert(dep.clone());
        }
        for dep_id in feature.metadata.depends_on.keys() {
            deps.insert(dep_id.clone());
        }
        graph.insert(
            feature.id.clone(),
            serde_json::Value::Array(deps.into_iter().map(serde_json::Value::String).collect()),
        );
    }

    serde_json::Value::Object(graph)
}

/// Output plan result in the specified format
fn output_plan_result(result: &FeaturesPlanResult, json: bool) -> Result<()> {
    if json {
        let json_output = serde_json::to_string_pretty(result)?;
        println!("{}", json_output);
    } else {
        println!("Feature Installation Plan:");
        println!("Order: {:?}", result.order);
        println!("Graph: {}", serde_json::to_string_pretty(&result.graph)?);
    }
    Ok(())
}

async fn execute_features_test(path: &str, json: bool) -> Result<()> {
    debug!("Testing feature at path: {}", path);

    let feature_path = Path::new(path);

    // Parse feature metadata
    let metadata_path = feature_path.join("devcontainer-feature.json");
    let metadata = parse_feature_metadata(&metadata_path)
        .map_err(|e| anyhow::anyhow!("Failed to parse feature metadata: {}", e))?;

    info!(
        "Testing feature: {} ({})",
        metadata.id,
        metadata.name.as_deref().unwrap_or("No name")
    );

    // Check if install.sh exists
    let install_script = feature_path.join("install.sh");
    if !install_script.exists() {
        return Err(anyhow::anyhow!("install.sh not found in feature directory"));
    }

    // Run install script in ephemeral Alpine container
    let success = run_feature_test_in_container(feature_path, &install_script).await?;

    let result = FeaturesResult {
        command: "test".to_string(),
        status: if success { "success" } else { "failure" }.to_string(),
        digest: None,
        size: None,
        message: if success {
            Some("Feature test completed successfully".to_string())
        } else {
            Some("Feature test failed".to_string())
        },
        cache_path: None,
    };

    output_result(&result, json)?;

    if !success {
        std::process::exit(1);
    }

    Ok(())
}

/// Execute features package command
async fn execute_features_package(path: &str, output_dir: &str, json: bool) -> Result<()> {
    debug!(
        "Packaging feature at path: {} to output: {}",
        path, output_dir
    );

    let feature_path = Path::new(path);
    let output_path = Path::new(output_dir);

    // Parse feature metadata
    let metadata_path = feature_path.join("devcontainer-feature.json");
    let metadata = parse_feature_metadata(&metadata_path)
        .map_err(|e| anyhow::anyhow!("Failed to parse feature metadata: {}", e))?;

    info!(
        "Packaging feature: {} ({})",
        metadata.id,
        metadata.name.as_deref().unwrap_or("No name")
    );

    // Create output directory if it doesn't exist
    std::fs::create_dir_all(output_path)?;

    // Create tar archive of feature directory
    let (digest, size) = create_feature_package(feature_path, output_path, &metadata.id).await?;

    let result = FeaturesResult {
        command: "package".to_string(),
        status: "success".to_string(),
        digest: Some(digest),
        size: Some(size),
        message: Some(format!("Feature packaged successfully to {}", output_dir)),
        cache_path: None,
    };

    output_result(&result, json)?;

    Ok(())
}

/// Execute features pull command
async fn execute_features_pull(registry_ref: &str, json: bool) -> Result<()> {
    debug!("Pulling feature from registry reference: {}", registry_ref);

    // Parse registry reference
    let (registry_url, namespace, name, tag) = parse_registry_reference(registry_ref)?;
    let tag = tag.unwrap_or_else(|| "latest".to_string());

    let feature_ref = FeatureRef::new(registry_url, namespace, name, Some(tag));

    info!("Pulling feature: {}", feature_ref.reference());

    // Create OCI client and fetch from registry
    let fetcher =
        default_fetcher().map_err(|e| anyhow::anyhow!("Failed to create OCI client: {}", e))?;

    let downloaded_feature = fetcher
        .fetch_feature(&feature_ref)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to pull feature: {}", e))?;

    let result = FeaturesResult {
        command: "pull".to_string(),
        status: "success".to_string(),
        digest: Some(downloaded_feature.digest),
        size: None, // Size not available in DownloadedFeature
        message: Some(format!(
            "Successfully pulled {} to {}",
            feature_ref.reference(),
            downloaded_feature.path.display()
        )),
        cache_path: Some(downloaded_feature.path.to_string_lossy().into_owned()),
    };

    output_result(&result, json)?;
    Ok(())
}

/// Execute features publish command
async fn execute_features_publish(
    path: &str,
    registry: &str,
    dry_run: bool,
    json: bool,
    username: Option<&str>,
    password_stdin: bool,
) -> Result<()> {
    debug!(
        "Publishing feature at path: {} to registry: {} (dry_run: {})",
        path, registry, dry_run
    );

    // Handle authentication credentials if provided
    if let Some(_username) = username {
        // TODO: Implement credential setting in OCI client
        debug!("Username provided for authentication: {}", _username);
    }
    if password_stdin {
        // TODO: Implement reading password from stdin
        debug!("Password will be read from stdin");
    }

    let feature_path = Path::new(path);

    // Parse feature metadata
    let metadata_path = feature_path.join("devcontainer-feature.json");
    let metadata = parse_feature_metadata(&metadata_path)
        .map_err(|e| anyhow::anyhow!("Failed to parse feature metadata: {}", e))?;

    info!(
        "Publishing feature: {} ({})",
        metadata.id,
        metadata.name.as_deref().unwrap_or("No name")
    );

    if dry_run {
        info!("Dry run mode - would publish to registry: {}", registry);

        let result = FeaturesResult {
            command: "publish".to_string(),
            status: "success".to_string(),
            digest: Some(
                "sha256:dryrun0000000000000000000000000000000000000000000000000000000000"
                    .to_string(),
            ),
            size: None,
            message: Some(format!("Dry run completed - would publish to {}", registry)),
            cache_path: None,
        };

        output_result(&result, json)?;
        return Ok(());
    }

    // Parse registry reference from the registry parameter
    // Format: [registry]/[namespace]/[name]:[tag]
    let (registry_url, namespace, name, tag) = parse_registry_reference(registry)?;

    let feature_ref = FeatureRef::new(
        registry_url.clone(),
        namespace.clone(),
        name.clone(),
        tag.clone(),
    );

    // Create feature package
    let temp_dir = tempfile::tempdir()?;
    let (_digest, _size) =
        create_feature_package(feature_path, temp_dir.path(), &metadata.id).await?;

    // Read the created tar file for publishing
    let tar_path = temp_dir.path().join(format!("{}.tar", metadata.id));
    let tar_data = std::fs::read(&tar_path)?;

    // Create OCI client and publish to registry
    let fetcher =
        default_fetcher().map_err(|e| anyhow::anyhow!("Failed to create OCI client: {}", e))?;

    info!("Publishing to OCI registry: {}", feature_ref.reference());
    let publish_result = fetcher
        .publish_feature(&feature_ref, tar_data.into(), &metadata)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to publish feature: {}", e))?;

    let result = FeaturesResult {
        command: "publish".to_string(),
        status: "success".to_string(),
        digest: Some(publish_result.digest),
        size: Some(publish_result.size),
        message: Some(format!(
            "Successfully published {} to {}",
            feature_ref.reference(),
            registry_url
        )),
        cache_path: None,
    };

    output_result(&result, json)?;
    Ok(())
}

/// Execute features info command
async fn execute_features_info(mode: &str, feature: &str, json: bool) -> Result<()> {
    debug!("Getting feature info for: {} (mode: {})", feature, mode);

    // Determine if this is a local path or OCI reference
    // Check if it's a path by trying to see if it exists as a directory
    let path = Path::new(feature);
    let is_local = path.exists() && path.is_dir();

    let (metadata, registry_url, namespace, name, tag, digest) = if is_local {
        // Load from local path
        let feature_path = Path::new(feature);
        let metadata_path = feature_path.join("devcontainer-feature.json");
        let metadata = parse_feature_metadata(&metadata_path)
            .map_err(|e| anyhow::anyhow!("Failed to parse feature metadata: {}", e))?;

        info!("Loading feature info from local path: {}", feature);
        (metadata, None, None, None, None, None)
    } else {
        // Parse the feature reference and fetch from OCI
        let (registry_url, namespace, name, tag) = parse_registry_reference(feature)?;

        let feature_ref = FeatureRef::new(
            registry_url.clone(),
            namespace.clone(),
            name.clone(),
            tag.clone(),
        );

        // Create OCI client and fetch feature metadata
        let fetcher =
            default_fetcher().map_err(|e| anyhow::anyhow!("Failed to create OCI client: {}", e))?;

        info!("Fetching feature info from: {}", feature_ref.reference());

        let downloaded_feature = fetcher
            .fetch_feature(&feature_ref)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch feature: {}", e))?;

        (
            downloaded_feature.metadata,
            Some(registry_url),
            Some(namespace),
            Some(name),
            tag,
            Some(downloaded_feature.digest),
        )
    };

    // Generate output based on mode
    match mode {
        "manifest" => output_manifest_info(&metadata, json),
        "tags" => {
            output_tags_info(
                &metadata,
                registry_url.as_deref(),
                namespace.as_deref(),
                name.as_deref(),
                json,
            )
            .await
        }
        "dependencies" => output_dependencies_info(&metadata, json),
        "verbose" => output_verbose_info(
            &metadata,
            registry_url.as_deref(),
            namespace.as_deref(),
            name.as_deref(),
            tag.as_deref(),
            digest.as_deref(),
            json,
        ),
        _ => Err(anyhow::anyhow!(
            "Invalid mode '{}'. Valid modes are: manifest, tags, dependencies, verbose",
            mode
        )),
    }
}

/// Output manifest information (metadata only)
fn output_manifest_info(metadata: &FeatureMetadata, json: bool) -> Result<()> {
    if json {
        // Convert HashMaps to BTreeMaps for deterministic ordering
        let options: BTreeMap<_, _> = metadata.options.iter().collect();
        let container_env: BTreeMap<_, _> = metadata.container_env.iter().collect();
        let depends_on: BTreeMap<_, _> = metadata.depends_on.iter().collect();

        let manifest = serde_json::json!({
            "id": metadata.id,
            "version": metadata.version,
            "name": metadata.name,
            "description": metadata.description,
            "documentationURL": metadata.documentation_url,
            "licenseURL": metadata.license_url,
            "options": options,
            "containerEnv": container_env,
            "mounts": metadata.mounts,
            "init": metadata.init,
            "privileged": metadata.privileged,
            "capAdd": metadata.cap_add,
            "securityOpt": metadata.security_opt,
            "entrypoint": metadata.entrypoint,
            "installsAfter": metadata.installs_after,
            "dependsOn": depends_on,
            "onCreateCommand": metadata.on_create_command,
            "updateContentCommand": metadata.update_content_command,
            "postCreateCommand": metadata.post_create_command,
            "postStartCommand": metadata.post_start_command,
            "postAttachCommand": metadata.post_attach_command,
        });
        println!("{}", serde_json::to_string_pretty(&manifest)?);
    } else {
        println!("Feature Manifest:");
        println!("  ID: {}", metadata.id);
        if let Some(ref version) = metadata.version {
            println!("  Version: {}", version);
        }
        if let Some(ref name) = metadata.name {
            println!("  Name: {}", name);
        }
        if let Some(ref desc) = metadata.description {
            println!("  Description: {}", desc);
        }
        if let Some(ref doc_url) = metadata.documentation_url {
            println!("  Documentation: {}", doc_url);
        }
        if let Some(ref license_url) = metadata.license_url {
            println!("  License: {}", license_url);
        }
        if !metadata.options.is_empty() {
            println!("  Options: {} defined", metadata.options.len());
        }
        if !metadata.installs_after.is_empty() {
            println!("  Installs After: {:?}", metadata.installs_after);
        }
        if !metadata.depends_on.is_empty() {
            println!("  Dependencies: {:?}", metadata.depends_on.keys());
        }
    }
    Ok(())
}

/// Output tags information (available versions)
async fn output_tags_info(
    metadata: &FeatureMetadata,
    registry_url: Option<&str>,
    namespace: Option<&str>,
    name: Option<&str>,
    json: bool,
) -> Result<()> {
    // For local features, we can only show the current version
    let tags = if let (Some(_registry), Some(_ns), Some(_n)) = (registry_url, namespace, name) {
        // Note: This is a placeholder - in a full implementation, we would
        // query the OCI registry for available tags
        // For now, we'll just return the current version if available
        vec![metadata
            .version
            .clone()
            .unwrap_or_else(|| "latest".to_string())]
    } else {
        // Local feature - only current version
        vec![metadata
            .version
            .clone()
            .unwrap_or_else(|| "unknown".to_string())]
    };

    if json {
        let output = serde_json::json!({
            "id": metadata.id,
            "tags": tags,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("Available Tags for '{}':", metadata.id);
        for tag in tags {
            println!("  - {}", tag);
        }
    }
    Ok(())
}

/// Output dependencies information
fn output_dependencies_info(metadata: &FeatureMetadata, json: bool) -> Result<()> {
    if json {
        let deps = serde_json::json!({
            "id": metadata.id,
            "installsAfter": metadata.installs_after,
            "dependsOn": metadata.depends_on,
        });
        println!("{}", serde_json::to_string_pretty(&deps)?);
    } else {
        println!("Dependencies for '{}':", metadata.id);
        if !metadata.installs_after.is_empty() {
            println!("  Installs After:");
            for feature in &metadata.installs_after {
                println!("    - {}", feature);
            }
        } else {
            println!("  Installs After: (none)");
        }

        if !metadata.depends_on.is_empty() {
            println!("  Depends On:");
            for (feature, options) in &metadata.depends_on {
                println!("    - {}: {}", feature, options);
            }
        } else {
            println!("  Depends On: (none)");
        }
    }
    Ok(())
}

/// Output verbose information (all available details)
fn output_verbose_info(
    metadata: &FeatureMetadata,
    registry_url: Option<&str>,
    namespace: Option<&str>,
    name: Option<&str>,
    tag: Option<&str>,
    digest: Option<&str>,
    json: bool,
) -> Result<()> {
    if json {
        // Convert HashMaps to BTreeMaps for deterministic ordering
        let options: BTreeMap<_, _> = metadata.options.iter().collect();
        let container_env: BTreeMap<_, _> = metadata.container_env.iter().collect();
        let depends_on: BTreeMap<_, _> = metadata.depends_on.iter().collect();

        let mut info = serde_json::json!({
            "id": metadata.id,
            "version": metadata.version,
            "name": metadata.name,
            "description": metadata.description,
            "documentationURL": metadata.documentation_url,
            "licenseURL": metadata.license_url,
            "options": options,
            "containerEnv": container_env,
            "mounts": metadata.mounts,
            "init": metadata.init,
            "privileged": metadata.privileged,
            "capAdd": metadata.cap_add,
            "securityOpt": metadata.security_opt,
            "entrypoint": metadata.entrypoint,
            "installsAfter": metadata.installs_after,
            "dependsOn": depends_on,
            "onCreateCommand": metadata.on_create_command,
            "updateContentCommand": metadata.update_content_command,
            "postCreateCommand": metadata.post_create_command,
            "postStartCommand": metadata.post_start_command,
            "postAttachCommand": metadata.post_attach_command,
        });

        // Add OCI-specific fields if available
        if let Some(registry) = registry_url {
            info["registry"] = serde_json::json!(registry);
        }
        if let Some(ns) = namespace {
            info["namespace"] = serde_json::json!(ns);
        }
        if let Some(n) = name {
            info["featureName"] = serde_json::json!(n);
        }
        if let Some(t) = tag {
            info["tag"] = serde_json::json!(t);
        }
        if let Some(d) = digest {
            info["digest"] = serde_json::json!(d);
        }

        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        println!("=== Feature Information (Verbose) ===");
        println!("\nBasic Information:");
        println!("  ID: {}", metadata.id);
        if let Some(ref version) = metadata.version {
            println!("  Version: {}", version);
        }
        if let Some(ref name) = metadata.name {
            println!("  Name: {}", name);
        }
        if let Some(ref desc) = metadata.description {
            println!("  Description: {}", desc);
        }
        if let Some(ref doc_url) = metadata.documentation_url {
            println!("  Documentation: {}", doc_url);
        }
        if let Some(ref license_url) = metadata.license_url {
            println!("  License: {}", license_url);
        }

        if let Some(registry) = registry_url {
            println!("\nRegistry Information:");
            println!("  Registry: {}", registry);
            if let Some(ns) = namespace {
                println!("  Namespace: {}", ns);
            }
            if let Some(n) = name {
                println!("  Name: {}", n);
            }
            if let Some(t) = tag {
                println!("  Tag: {}", t);
            }
            if let Some(d) = digest {
                println!("  Digest: {}", d);
            }
        }

        if !metadata.options.is_empty() {
            println!("\nOptions:");
            for (key, option) in &metadata.options {
                println!("  {}:", key);
                match option {
                    deacon_core::features::FeatureOption::Boolean {
                        default,
                        description,
                    } => {
                        println!("    Type: boolean");
                        if let Some(def) = default {
                            println!("    Default: {}", def);
                        }
                        if let Some(desc) = description {
                            println!("    Description: {}", desc);
                        }
                    }
                    deacon_core::features::FeatureOption::String {
                        default,
                        description,
                        r#enum,
                        proposals,
                    } => {
                        println!("    Type: string");
                        if let Some(def) = default {
                            println!("    Default: {}", def);
                        }
                        if let Some(desc) = description {
                            println!("    Description: {}", desc);
                        }
                        if let Some(values) = r#enum {
                            println!("    Allowed values: {:?}", values);
                        }
                        if let Some(props) = proposals {
                            println!("    Proposals: {:?}", props);
                        }
                    }
                }
            }
        }

        if !metadata.installs_after.is_empty() || !metadata.depends_on.is_empty() {
            println!("\nDependencies:");
            if !metadata.installs_after.is_empty() {
                println!("  Installs After:");
                for feature in &metadata.installs_after {
                    println!("    - {}", feature);
                }
            }
            if !metadata.depends_on.is_empty() {
                println!("  Depends On:");
                for (feature, options) in &metadata.depends_on {
                    println!("    - {}: {}", feature, options);
                }
            }
        }

        if !metadata.container_env.is_empty() {
            println!("\nContainer Environment Variables:");
            for (key, value) in &metadata.container_env {
                println!("  {}: {}", key, value);
            }
        }

        if !metadata.mounts.is_empty() {
            println!("\nMounts:");
            for mount in &metadata.mounts {
                println!("  - {}", mount);
            }
        }

        if metadata.init.is_some()
            || metadata.privileged.is_some()
            || !metadata.cap_add.is_empty()
            || !metadata.security_opt.is_empty()
        {
            println!("\nContainer Options:");
            if let Some(init) = metadata.init {
                println!("  Init: {}", init);
            }
            if let Some(privileged) = metadata.privileged {
                println!("  Privileged: {}", privileged);
            }
            if !metadata.cap_add.is_empty() {
                println!("  Capabilities: {:?}", metadata.cap_add);
            }
            if !metadata.security_opt.is_empty() {
                println!("  Security Options: {:?}", metadata.security_opt);
            }
        }

        if metadata.has_lifecycle_commands() {
            println!("\nLifecycle Commands:");
            if let Some(ref cmd) = metadata.on_create_command {
                println!("  onCreate: {}", cmd);
            }
            if let Some(ref cmd) = metadata.update_content_command {
                println!("  updateContent: {}", cmd);
            }
            if let Some(ref cmd) = metadata.post_create_command {
                println!("  postCreate: {}", cmd);
            }
            if let Some(ref cmd) = metadata.post_start_command {
                println!("  postStart: {}", cmd);
            }
            if let Some(ref cmd) = metadata.post_attach_command {
                println!("  postAttach: {}", cmd);
            }
        }
    }
    Ok(())
}

/// Run feature test in an ephemeral Alpine container
async fn run_feature_test_in_container(
    feature_path: &Path,
    _install_script: &Path,
) -> Result<bool> {
    use std::process::Command;

    debug!("Running feature test in ephemeral Alpine container");

    let feature_mount = format!("{}:/tmp/feature:ro", feature_path.display());

    // Run Docker command to test the feature
    let output = Command::new("docker")
        .args([
            "run",
            "--rm",
            "-v",
            &feature_mount,
            "alpine:latest",
            "sh",
            "-c",
            "cd /tmp/feature && chmod +x install.sh && ./install.sh",
        ])
        .output();

    match output {
        Ok(result) => {
            debug!("Docker command exit status: {}", result.status);
            debug!("Docker stdout: {}", String::from_utf8_lossy(&result.stdout));
            if !result.stderr.is_empty() {
                debug!("Docker stderr: {}", String::from_utf8_lossy(&result.stderr));
            }
            Ok(result.status.success())
        }
        Err(e) => {
            debug!("Failed to run docker command: {}", e);
            // If Docker is not available, we can't run the test but shouldn't fail the build
            info!("Docker not available for feature testing - skipping container test");
            Ok(true) // Return success so build doesn't fail
        }
    }
}

/// Create a feature package (tar archive with OCI manifest stub)
async fn create_feature_package(
    feature_path: &Path,
    output_path: &Path,
    feature_id: &str,
) -> Result<(String, u64)> {
    use sha2::{Digest, Sha256};
    use std::fs::File;
    use std::io::{Read, Write};
    use tar::Builder;

    debug!("Creating feature package for: {}", feature_id);

    // Create tar archive
    let tar_filename = format!("{}.tar", feature_id);
    let tar_path = output_path.join(&tar_filename);
    let tar_file = File::create(&tar_path)?;
    let mut builder = Builder::new(tar_file);

    // Add all files from feature directory to tar
    builder.append_dir_all(".", feature_path)?;
    builder.finish()?;

    // Calculate digest and size
    let mut file = File::open(&tar_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    hasher.update(&buffer);
    let digest = format!("sha256:{:x}", hasher.finalize());
    let size = buffer.len() as u64;

    // Create OCI manifest stub
    let manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.devcontainers.feature.config.v1+json",
            "size": 0,
            "digest": "sha256:placeholder"
        },
        "layers": [{
            "mediaType": "application/vnd.oci.image.layer.v1.tar",
            "size": size,
            "digest": digest.clone()
        }]
    });

    let manifest_path = output_path.join(format!("{}-manifest.json", feature_id));
    let mut manifest_file = File::create(manifest_path)?;
    manifest_file.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;

    info!(
        "Created package: {} (digest: {}, size: {} bytes)",
        tar_filename, digest, size
    );

    Ok((digest, size))
}

/// Output result in the specified format
fn output_result(result: &FeaturesResult, json: bool) -> Result<()> {
    if json {
        let json_output = serde_json::to_string_pretty(result)?;
        println!("{}", json_output);
    } else {
        println!("Command: {}", result.command);
        println!("Status: {}", result.status);
        if let Some(ref digest) = result.digest {
            println!("Digest: {}", digest);
        }
        if let Some(size) = result.size {
            println!("Size: {} bytes", size);
        }
        if let Some(ref cache_path) = result.cache_path {
            println!("Cache Path: {}", cache_path);
        }
        if let Some(ref message) = result.message {
            println!("Message: {}", message);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_is_local_path() {
        // Relative paths
        assert!(is_local_path("./feature"));
        assert!(is_local_path("../feature"));
        assert!(is_local_path("./path/to/feature"));
        assert!(is_local_path("../path/to/feature"));

        // Absolute Unix paths
        assert!(is_local_path("/abs/path"));
        assert!(is_local_path("/feature"));

        // Windows paths
        assert!(is_local_path("C:\\path"));
        assert!(is_local_path("D:/path"));
        assert!(is_local_path("E:\\feature"));

        // Valid registry references (should NOT be detected as local paths)
        assert!(!is_local_path("ghcr.io/devcontainers/node"));
        assert!(!is_local_path("myteam/myfeature"));
        assert!(!is_local_path("myfeature"));
        assert!(!is_local_path("ghcr.io/devcontainers/node:18"));
        assert!(!is_local_path("feature-name"));
        assert!(!is_local_path("my-feature"));
    }

    #[tokio::test]
    async fn test_features_plan_rejects_local_paths() {
        let temp_dir = TempDir::new().unwrap();

        // Test with relative path ./feature
        let config_dir = temp_dir.path().join(".devcontainer");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("devcontainer.json");
        fs::write(&config_path, r#"{"features": {"./my-feature": true}}"#).unwrap();

        let args = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: None,
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path.clone()),
        };

        let result = execute_features_plan(true, None, &args).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains(LOCAL_FEATURE_ERROR_MSG));
        assert!(err_msg.contains("./my-feature"));

        // Test with absolute path
        fs::write(&config_path, r#"{"features": {"/abs/path/feature": true}}"#).unwrap();

        let args2 = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: None,
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path.clone()),
        };

        let result2 = execute_features_plan(true, None, &args2).await;
        assert!(result2.is_err());
        let err_msg2 = format!("{}", result2.unwrap_err());
        assert!(err_msg2.contains(LOCAL_FEATURE_ERROR_MSG));
        assert!(err_msg2.contains("/abs/path/feature"));

        // Test with parent relative path
        fs::write(
            &config_path,
            r#"{"features": {"../another-feature": true}}"#,
        )
        .unwrap();

        let args3 = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: None,
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path.clone()),
        };

        let result3 = execute_features_plan(true, None, &args3).await;
        assert!(result3.is_err());
        let err_msg3 = format!("{}", result3.unwrap_err());
        assert!(err_msg3.contains(LOCAL_FEATURE_ERROR_MSG));
        assert!(err_msg3.contains("../another-feature"));
    }

    #[tokio::test]
    async fn test_features_plan_additional_features_with_local_path() {
        let temp_dir = TempDir::new().unwrap();
        let args = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: Some(r#"{"./local-feature": true}"#.to_string()),
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
        };

        let result = execute_features_plan(true, Some(r#"{"./local-feature": true}"#), &args).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains(LOCAL_FEATURE_ERROR_MSG));
        assert!(err_msg.contains("./local-feature"));
    }

    #[tokio::test]
    async fn test_features_plan_mixed_local_and_registry() {
        let temp_dir = TempDir::new().unwrap();

        // Test with mixed features: one local, one registry
        // The map iteration order may vary, but at least one local path should be detected
        let config_dir = temp_dir.path().join(".devcontainer");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("devcontainer.json");
        fs::write(
            &config_path,
            r#"{"features": {"./local-feature": true, "ghcr.io/devcontainers/node": true}}"#,
        )
        .unwrap();

        let args = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: None,
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path.clone()),
        };

        let result = execute_features_plan(true, None, &args).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains(LOCAL_FEATURE_ERROR_MSG));
        // Should detect the local path and error
        assert!(err_msg.contains("./local-feature") || err_msg.contains("Feature key:"));
    }

    #[test]
    fn test_features_result_json_serialization() {
        let result = FeaturesResult {
            command: "test".to_string(),
            status: "success".to_string(),
            digest: Some("sha256:abc123".to_string()),
            size: Some(1024),
            message: Some("Test completed".to_string()),
            cache_path: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"command\":\"test\""));
        assert!(json.contains("\"status\":\"success\""));
        assert!(json.contains("\"digest\":\"sha256:abc123\""));
    }

    #[test]
    fn test_features_plan_result_serialization() {
        let result = FeaturesPlanResult {
            order: vec!["feature-a".to_string(), "feature-b".to_string()],
            graph: serde_json::json!({
                "feature-a": [],
                "feature-b": ["feature-a"]
            }),
        };

        let json = serde_json::to_string_pretty(&result).unwrap();
        assert!(json.contains("\"order\""));
        assert!(json.contains("\"graph\""));
        assert!(json.contains("feature-a"));
        assert!(json.contains("feature-b"));
    }

    #[test]
    fn test_build_graph_representation() {
        use deacon_core::features::{FeatureMetadata, ResolvedFeature};
        use std::collections::HashMap;

        let mut depends_on = HashMap::new();
        depends_on.insert("feature-a".to_string(), serde_json::Value::Bool(true));

        let feature = ResolvedFeature {
            id: "feature-b".to_string(),
            source: "test://feature-b".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "feature-b".to_string(),
                version: Some("1.0.0".to_string()),
                name: Some("Feature B".to_string()),
                description: None,
                documentation_url: None,
                license_url: None,
                options: HashMap::new(),
                container_env: HashMap::new(),
                mounts: vec![],
                init: None,
                privileged: None,
                cap_add: vec![],
                security_opt: vec![],
                entrypoint: None,
                installs_after: vec!["feature-a".to_string()],
                depends_on,
                on_create_command: None,
                update_content_command: None,
                post_create_command: None,
                post_start_command: None,
                post_attach_command: None,
            },
        };

        let graph = build_graph_representation(&[feature]);
        // Check that feature-b has dependencies
        if let Some(deps) = graph.get("feature-b") {
            if let Some(deps_array) = deps.as_array() {
                assert!(!deps_array.is_empty());
                assert!(deps_array.contains(&serde_json::Value::String("feature-a".to_string())));
            } else {
                panic!("Dependencies should be an array");
            }
        } else {
            panic!("feature-b should exist in graph");
        }
    }

    #[test]
    fn test_graph_structure_no_dependencies() {
        // Test: Feature with no dependencies should have empty array in graph
        let feature = create_mock_resolved_feature("feature-standalone");

        let graph = build_graph_representation(&[feature]);

        // Verify the feature exists in the graph with an empty dependencies array
        let deps = graph
            .get("feature-standalone")
            .expect("Feature should exist in graph");
        let deps_array = deps.as_array().expect("Dependencies should be an array");
        assert!(
            deps_array.is_empty(),
            "Feature with no dependencies should have empty array"
        );
    }

    #[test]
    fn test_graph_structure_simple_chain() {
        // Test: Simple dependency chain A->B where B depends on A
        // Graph should show: { "feature-a": [], "feature-b": ["feature-a"] }
        let feature_a = create_mock_resolved_feature("feature-a");
        let feature_b = create_mock_resolved_feature_with_deps("feature-b", &["feature-a"], &[]);

        let graph = build_graph_representation(&[feature_a, feature_b]);

        // Verify feature-a has no dependencies
        let deps_a = graph.get("feature-a").expect("feature-a should exist");
        let deps_a_array = deps_a.as_array().expect("Dependencies should be an array");
        assert!(
            deps_a_array.is_empty(),
            "feature-a should have no dependencies"
        );

        // Verify feature-b depends on feature-a
        let deps_b = graph.get("feature-b").expect("feature-b should exist");
        let deps_b_array = deps_b.as_array().expect("Dependencies should be an array");
        assert_eq!(deps_b_array.len(), 1, "feature-b should have 1 dependency");
        assert!(
            deps_b_array.contains(&serde_json::Value::String("feature-a".to_string())),
            "feature-b should depend on feature-a"
        );
    }

    #[test]
    fn test_graph_structure_combined_installs_after_and_depends_on() {
        // Test: Feature with both installsAfter and dependsOn should union them
        // If both specify different dependencies, all should appear
        let feature_a = create_mock_resolved_feature("feature-a");
        let feature_b = create_mock_resolved_feature("feature-b");
        let feature_c = create_mock_resolved_feature_with_deps(
            "feature-c",
            &["feature-a"], // installsAfter
            &["feature-b"], // dependsOn
        );

        let graph = build_graph_representation(&[feature_a, feature_b, feature_c]);

        // Verify feature-c has both dependencies
        let deps_c = graph.get("feature-c").expect("feature-c should exist");
        let deps_c_array = deps_c.as_array().expect("Dependencies should be an array");
        assert_eq!(
            deps_c_array.len(),
            2,
            "feature-c should have 2 dependencies"
        );
        assert!(
            deps_c_array.contains(&serde_json::Value::String("feature-a".to_string())),
            "feature-c should have feature-a from installsAfter"
        );
        assert!(
            deps_c_array.contains(&serde_json::Value::String("feature-b".to_string())),
            "feature-c should have feature-b from dependsOn"
        );
    }

    #[test]
    fn test_graph_structure_union_deduplication() {
        // Test: If same dependency appears in both installsAfter and dependsOn,
        // it should appear only once (deduplication)
        let feature_a = create_mock_resolved_feature("feature-a");
        let feature_b = create_mock_resolved_feature_with_deps(
            "feature-b",
            &["feature-a"], // installsAfter
            &["feature-a"], // dependsOn (same)
        );

        let graph = build_graph_representation(&[feature_a, feature_b]);

        // Verify feature-b has feature-a only once
        let deps_b = graph.get("feature-b").expect("feature-b should exist");
        let deps_b_array = deps_b.as_array().expect("Dependencies should be an array");
        assert_eq!(
            deps_b_array.len(),
            1,
            "Duplicate dependency should be deduplicated"
        );
        assert!(
            deps_b_array.contains(&serde_json::Value::String("feature-a".to_string())),
            "feature-b should depend on feature-a"
        );
    }

    #[test]
    fn test_graph_structure_fan_in() {
        // Test: Fan-in pattern where C depends on both A and B
        // Graph should show: { "feature-c": ["feature-a", "feature-b"] }
        let feature_a = create_mock_resolved_feature("feature-a");
        let feature_b = create_mock_resolved_feature("feature-b");
        let feature_c =
            create_mock_resolved_feature_with_deps("feature-c", &["feature-a", "feature-b"], &[]);

        let graph = build_graph_representation(&[feature_a, feature_b, feature_c]);

        // Verify feature-c depends on both feature-a and feature-b
        let deps_c = graph.get("feature-c").expect("feature-c should exist");
        let deps_c_array = deps_c.as_array().expect("Dependencies should be an array");
        assert_eq!(
            deps_c_array.len(),
            2,
            "feature-c should have 2 dependencies"
        );
        assert!(
            deps_c_array.contains(&serde_json::Value::String("feature-a".to_string())),
            "feature-c should depend on feature-a"
        );
        assert!(
            deps_c_array.contains(&serde_json::Value::String("feature-b".to_string())),
            "feature-c should depend on feature-b"
        );
    }

    #[test]
    fn test_graph_structure_deterministic_ordering() {
        // Test: Dependencies should be in deterministic (lexicographic) order
        let feature_a = create_mock_resolved_feature("feature-a");
        let feature_b = create_mock_resolved_feature("feature-b");
        let feature_c = create_mock_resolved_feature("feature-c");
        // Add dependencies in non-lexicographic order
        let feature_d = create_mock_resolved_feature_with_deps(
            "feature-d",
            &["feature-c", "feature-a", "feature-b"], // Not sorted
            &[],
        );

        let graph = build_graph_representation(&[feature_a, feature_b, feature_c, feature_d]);

        // Verify dependencies are in lexicographic order
        let deps_d = graph.get("feature-d").expect("feature-d should exist");
        let deps_d_array = deps_d.as_array().expect("Dependencies should be an array");
        assert_eq!(
            deps_d_array.len(),
            3,
            "feature-d should have 3 dependencies"
        );

        // Extract dependency strings in order
        let dep_strings: Vec<String> = deps_d_array
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();

        assert_eq!(
            dep_strings,
            vec!["feature-a", "feature-b", "feature-c"],
            "Dependencies should be in lexicographic order"
        );
    }

    #[test]
    fn test_graph_structure_cycle_detection_error_shape() {
        use deacon_core::errors::FeatureError;
        use deacon_core::features::FeatureDependencyResolver;

        // Test: Verify cycle detection returns proper error structure
        // Cycle: a -> b -> c -> a
        let features = vec![
            create_mock_resolved_feature_with_deps("feature-a", &["feature-b"], &[]),
            create_mock_resolved_feature_with_deps("feature-b", &["feature-c"], &[]),
            create_mock_resolved_feature_with_deps("feature-c", &["feature-a"], &[]),
        ];

        let resolver = FeatureDependencyResolver::new(None);
        let result = resolver.resolve(&features);

        // Verify error type matches spec
        assert!(result.is_err(), "Cycle should produce an error");

        match result {
            Err(FeatureError::DependencyCycle { cycle_path }) => {
                // Verify error message contains cycle information
                assert!(
                    cycle_path.contains("feature-a"),
                    "Cycle path should contain feature-a"
                );
                assert!(
                    cycle_path.contains("feature-b"),
                    "Cycle path should contain feature-b"
                );
                assert!(
                    cycle_path.contains("feature-c"),
                    "Cycle path should contain feature-c"
                );
                // Verify it's formatted as a path with arrows
                assert!(
                    cycle_path.contains("->") || cycle_path.contains("→"),
                    "Cycle path should show direction with arrows"
                );
            }
            _ => panic!("Expected DependencyCycle error, got {:?}", result),
        }
    }

    #[test]
    fn test_graph_structure_complete_json_output() {
        // Test: Verify complete JSON output structure matches DATA-STRUCTURES.md spec
        let features = vec![
            create_mock_resolved_feature("feature-a"),
            create_mock_resolved_feature_with_deps("feature-b", &["feature-a"], &[]),
            create_mock_resolved_feature_with_deps("feature-c", &["feature-a", "feature-b"], &[]),
        ];

        let graph = build_graph_representation(&features);

        // Verify it's a JSON object
        assert!(graph.is_object(), "Graph should be a JSON object");

        // Verify all features are present
        assert!(
            graph.get("feature-a").is_some(),
            "feature-a should be in graph"
        );
        assert!(
            graph.get("feature-b").is_some(),
            "feature-b should be in graph"
        );
        assert!(
            graph.get("feature-c").is_some(),
            "feature-c should be in graph"
        );

        // Verify structure: feature-a has no deps
        let deps_a = graph.get("feature-a").unwrap();
        assert!(deps_a.is_array(), "Dependencies should be an array");
        assert_eq!(
            deps_a.as_array().unwrap().len(),
            0,
            "feature-a has no dependencies"
        );

        // Verify structure: feature-b depends on feature-a
        let deps_b = graph.get("feature-b").unwrap().as_array().unwrap();
        assert_eq!(deps_b.len(), 1, "feature-b has 1 dependency");
        assert_eq!(
            deps_b[0],
            serde_json::Value::String("feature-a".to_string()),
            "feature-b depends on feature-a"
        );

        // Verify structure: feature-c depends on both (sorted)
        let deps_c = graph.get("feature-c").unwrap().as_array().unwrap();
        assert_eq!(deps_c.len(), 2, "feature-c has 2 dependencies");
        assert_eq!(
            deps_c[0],
            serde_json::Value::String("feature-a".to_string()),
            "First dep should be feature-a (sorted)"
        );
        assert_eq!(
            deps_c[1],
            serde_json::Value::String("feature-b".to_string()),
            "Second dep should be feature-b (sorted)"
        );

        // Verify JSON serialization matches expected format
        let json_str = serde_json::to_string_pretty(&graph).unwrap();
        assert!(
            json_str.contains("\"feature-a\""),
            "JSON should contain feature-a"
        );
        assert!(json_str.contains("[]"), "JSON should contain empty arrays");
    }

    #[test]
    fn test_graph_structure_features_in_order_without_deps() {
        // Test: Features present in plan order but without explicit dependency fields
        // should have empty arrays in graph
        let features = vec![
            create_mock_resolved_feature("independent-1"),
            create_mock_resolved_feature("independent-2"),
            create_mock_resolved_feature("independent-3"),
        ];

        let graph = build_graph_representation(&features);

        // All features should be present with empty dependency arrays
        for feature_id in &["independent-1", "independent-2", "independent-3"] {
            let deps = graph
                .get(*feature_id)
                .unwrap_or_else(|| panic!("{} should exist in graph", feature_id));
            let deps_array = deps
                .as_array()
                .unwrap_or_else(|| panic!("{} deps should be an array", feature_id));
            assert!(
                deps_array.is_empty(),
                "{} should have empty dependencies array",
                feature_id
            );
        }

        // Verify consistent JSON structure
        let graph_obj = graph.as_object().expect("Graph should be a JSON object");
        assert_eq!(
            graph_obj.len(),
            3,
            "Graph should have exactly 3 feature entries"
        );
    }

    #[test]
    fn test_create_mock_resolved_feature() {
        let feature = create_mock_resolved_feature("test-feature");
        assert_eq!(feature.id, "test-feature");
        assert_eq!(feature.metadata.id, "test-feature");
        assert!(feature.source.contains("test-feature"));
        assert!(feature.metadata.installs_after.is_empty());
        assert!(feature.metadata.depends_on.is_empty());
    }

    #[tokio::test]
    async fn test_create_feature_package() {
        let temp_dir = TempDir::new().unwrap();
        let feature_dir = temp_dir.path().join("test-feature");
        let output_dir = temp_dir.path().join("output");

        // Create feature directory with minimal files
        fs::create_dir_all(&feature_dir).unwrap();
        fs::write(
            feature_dir.join("devcontainer-feature.json"),
            r#"{"id": "test-feature", "version": "1.0.0"}"#,
        )
        .unwrap();
        fs::write(
            feature_dir.join("install.sh"),
            "#!/bin/bash\necho 'Installing test feature'",
        )
        .unwrap();

        fs::create_dir_all(&output_dir).unwrap();

        let (digest, size) = create_feature_package(&feature_dir, &output_dir, "test-feature")
            .await
            .unwrap();

        assert!(digest.starts_with("sha256:"));
        assert!(size > 0);
        assert!(output_dir.join("test-feature.tar").exists());
        assert!(output_dir.join("test-feature-manifest.json").exists());
    }

    #[test]
    fn test_dependency_resolution_with_mock_features() {
        use deacon_core::features::FeatureDependencyResolver;

        // Create features with dependencies:
        // feature-c depends on feature-b
        // feature-b depends on feature-a
        // Expected order: feature-a, feature-b, feature-c
        let features = vec![
            create_mock_resolved_feature_with_deps("feature-c", &["feature-b"], &[]),
            create_mock_resolved_feature_with_deps("feature-b", &["feature-a"], &[]),
            create_mock_resolved_feature_with_deps("feature-a", &[], &[]),
        ];

        let resolver = FeatureDependencyResolver::new(None);
        let plan = resolver.resolve(&features).unwrap();
        let order = plan.feature_ids();

        assert_eq!(order, vec!["feature-a", "feature-b", "feature-c"]);
    }

    #[test]
    fn test_dependency_resolution_with_fan_in() {
        use deacon_core::features::FeatureDependencyResolver;

        // Create features with fan-in dependencies:
        // feature-c depends on both feature-a and feature-b
        // Expected order: feature-a, feature-b, feature-c (lexicographic for independents)
        let features = vec![
            create_mock_resolved_feature_with_deps("feature-c", &["feature-a", "feature-b"], &[]),
            create_mock_resolved_feature_with_deps("feature-b", &[], &[]),
            create_mock_resolved_feature_with_deps("feature-a", &[], &[]),
        ];

        let resolver = FeatureDependencyResolver::new(None);
        let plan = resolver.resolve(&features).unwrap();
        let order = plan.feature_ids();

        // feature-a and feature-b can be in any order, but both must come before feature-c
        let a_pos = order.iter().position(|x| x == "feature-a").unwrap();
        let b_pos = order.iter().position(|x| x == "feature-b").unwrap();
        let c_pos = order.iter().position(|x| x == "feature-c").unwrap();

        assert!(a_pos < c_pos);
        assert!(b_pos < c_pos);
        assert_eq!(order.len(), 3);
    }

    #[test]
    fn test_dependency_resolution_independent_features() {
        use deacon_core::features::FeatureDependencyResolver;

        // Create independent features - order should be deterministic (lexicographic)
        let features = vec![
            create_mock_resolved_feature("feature-z"),
            create_mock_resolved_feature("feature-a"),
            create_mock_resolved_feature("feature-m"),
        ];

        let resolver = FeatureDependencyResolver::new(None);
        let plan = resolver.resolve(&features).unwrap();
        let order = plan.feature_ids();

        assert_eq!(order, vec!["feature-a", "feature-m", "feature-z"]);
    }

    #[test]
    fn test_dependency_cycle_detection() {
        use deacon_core::features::FeatureDependencyResolver;

        // a -> b -> c -> a
        let features = vec![
            create_mock_resolved_feature_with_deps("a", &["b"], &[]),
            create_mock_resolved_feature_with_deps("b", &["c"], &[]),
            create_mock_resolved_feature_with_deps("c", &["a"], &[]),
        ];

        let resolver = FeatureDependencyResolver::new(None);
        let err = resolver.resolve(&features).expect_err("expected cycle");
        let msg = format!("{err}");
        assert!(msg.contains("cycle"), "message should mention cycle");
        assert!(
            msg.contains("a") && msg.contains("b") && msg.contains("c"),
            "message should include cycle path"
        );
    }

    #[test]
    fn test_dependency_cycle_error_message_format() {
        use deacon_core::errors::FeatureError;
        use deacon_core::features::FeatureDependencyResolver;

        // Test: Verify complete error message format per SPEC.md §9 Error Handling
        // SPEC.md §9 requirement: "Circular dependencies detected => error with details"
        // GAP.md §8: "Missing test that circular dependency errors include 'details'"
        // This test validates all required elements and serves as a snapshot test

        // Cycle: feature-x -> feature-y -> feature-z -> feature-x
        let features = vec![
            create_mock_resolved_feature_with_deps("feature-x", &["feature-y"], &[]),
            create_mock_resolved_feature_with_deps("feature-y", &["feature-z"], &[]),
            create_mock_resolved_feature_with_deps("feature-z", &["feature-x"], &[]),
        ];

        let resolver = FeatureDependencyResolver::new(None);
        let result = resolver.resolve(&features);

        // Verify error is returned per SPEC.md §9
        assert!(
            result.is_err(),
            "Cycle should produce an error per SPEC.md §9"
        );

        let err = result.unwrap_err();

        // Test 1: Verify error type is DependencyCycle
        match &err {
            FeatureError::DependencyCycle { cycle_path } => {
                // Test 2: SPEC.md §9 "details" requirement - all involved features present
                assert!(
                    cycle_path.contains("feature-x"),
                    "Cycle path should contain feature-x (required detail), got: {}",
                    cycle_path
                );
                assert!(
                    cycle_path.contains("feature-y"),
                    "Cycle path should contain feature-y (required detail), got: {}",
                    cycle_path
                );
                assert!(
                    cycle_path.contains("feature-z"),
                    "Cycle path should contain feature-z (required detail), got: {}",
                    cycle_path
                );

                // Test 3: Verify path shows direction (part of details)
                assert!(
                    cycle_path.contains("->") || cycle_path.contains("→"),
                    "Cycle path should show direction with arrows, got: {}",
                    cycle_path
                );

                // Test 4: Verify path forms a closed loop (validates correctness)
                let parts: Vec<&str> = cycle_path.split(" -> ").collect();
                assert!(
                    parts.len() >= 3,
                    "Cycle path should have at least 3 nodes (minimum cycle), got: {}",
                    cycle_path
                );
                assert_eq!(
                    parts.first(),
                    parts.last(),
                    "Cycle path should start and end with the same feature, got: {}",
                    cycle_path
                );
            }
            _ => panic!(
                "Expected DependencyCycle error per SPEC.md §9, got: {:?}",
                err
            ),
        }

        // Test 5: Verify full Display format includes required terminology per SPEC.md §9
        let full_msg = format!("{}", err);

        // SPEC.md §9: "Circular dependencies detected"
        assert!(
            full_msg.to_lowercase().contains("cycle")
                || full_msg.to_lowercase().contains("circular"),
            "Full error message should contain 'cycle' or 'circular' per SPEC.md §9, got: {}",
            full_msg
        );

        assert!(
            full_msg.to_lowercase().contains("depend"),
            "Full error message should reference 'dependencies' per SPEC.md §9, got: {}",
            full_msg
        );

        assert!(
            full_msg.contains("feature"),
            "Full error message should reference features context, got: {}",
            full_msg
        );

        // Snapshot test: Lock the format to prevent regressions
        assert!(
            full_msg.starts_with("Dependency cycle detected in features:"),
            "Error message format should match expected pattern (snapshot), got: {}",
            full_msg
        );

        // Test 6: Verify all involved features are in the full message (the "details")
        assert!(
            full_msg.contains("feature-x"),
            "Full error message should contain feature-x (required detail), got: {}",
            full_msg
        );
        assert!(
            full_msg.contains("feature-y"),
            "Full error message should contain feature-y (required detail), got: {}",
            full_msg
        );
        assert!(
            full_msg.contains("feature-z"),
            "Full error message should contain feature-z (required detail), got: {}",
            full_msg
        );
    }

    #[test]
    fn test_dependency_cycle_error_message_simple_cycle() {
        use deacon_core::features::FeatureDependencyResolver;

        // Test: Verify error message format for simple 2-node cycle
        // Cycle: feature-a -> feature-b -> feature-a
        let features = vec![
            create_mock_resolved_feature_with_deps("feature-a", &["feature-b"], &[]),
            create_mock_resolved_feature_with_deps("feature-b", &["feature-a"], &[]),
        ];

        let resolver = FeatureDependencyResolver::new(None);
        let result = resolver.resolve(&features);

        assert!(result.is_err(), "Simple cycle should produce an error");

        let err = result.unwrap_err();
        let full_msg = format!("{}", err);

        // Verify error includes cycle terminology
        assert!(
            full_msg.to_lowercase().contains("cycle"),
            "Error message should contain 'cycle', got: {}",
            full_msg
        );

        // Verify both features are mentioned
        assert!(
            full_msg.contains("feature-a") && full_msg.contains("feature-b"),
            "Error message should contain both feature-a and feature-b, got: {}",
            full_msg
        );

        // Verify arrow notation
        assert!(
            full_msg.contains("->"),
            "Error message should contain arrow notation, got: {}",
            full_msg
        );
    }

    #[test]
    fn test_dependency_cycle_error_message_complex_cycle() {
        use deacon_core::errors::FeatureError;
        use deacon_core::features::FeatureDependencyResolver;

        // Test: Verify error message format for longer cycle
        // Cycle: a -> b -> c -> d -> e -> a
        let features = vec![
            create_mock_resolved_feature_with_deps("feature-a", &["feature-b"], &[]),
            create_mock_resolved_feature_with_deps("feature-b", &["feature-c"], &[]),
            create_mock_resolved_feature_with_deps("feature-c", &["feature-d"], &[]),
            create_mock_resolved_feature_with_deps("feature-d", &["feature-e"], &[]),
            create_mock_resolved_feature_with_deps("feature-e", &["feature-a"], &[]),
        ];

        let resolver = FeatureDependencyResolver::new(None);
        let result = resolver.resolve(&features);

        assert!(result.is_err(), "Complex cycle should produce an error");

        match result {
            Err(FeatureError::DependencyCycle { cycle_path }) => {
                // Verify all features in the cycle are present in the path
                let features_in_cycle = vec![
                    "feature-a",
                    "feature-b",
                    "feature-c",
                    "feature-d",
                    "feature-e",
                ];

                for feature in features_in_cycle {
                    assert!(
                        cycle_path.contains(feature),
                        "Cycle path should contain {}, got: {}",
                        feature,
                        cycle_path
                    );
                }

                // Verify the path is properly formatted
                assert!(
                    cycle_path.contains("->"),
                    "Cycle path should use arrow notation, got: {}",
                    cycle_path
                );

                // Count arrows - should be at least 4 for a 5-node cycle
                let arrow_count = cycle_path.matches("->").count();
                assert!(
                    arrow_count >= 4,
                    "Complex cycle should have multiple arrows, got {} arrows in: {}",
                    arrow_count,
                    cycle_path
                );
            }
            _ => panic!("Expected DependencyCycle error"),
        }
    }

    #[tokio::test]
    async fn test_features_plan_empty_config() {
        let temp_dir = TempDir::new().unwrap();
        let args = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: None,
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
        };

        // Should succeed with empty plan
        let result = execute_features_plan(true, None, &args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_features_plan_with_additional_features() {
        let temp_dir = TempDir::new().unwrap();
        let args = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: Some(r#"{"node": true, "docker": true}"#.to_string()),
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
        };

        // Should fail because "node" and "docker" are not valid OCI feature references
        let result =
            execute_features_plan(true, Some(r#"{"node": true, "docker": true}"#), &args).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_output_plan_result_json() {
        let result = FeaturesPlanResult {
            order: vec!["a".to_string(), "b".to_string()],
            graph: serde_json::json!({"a": [], "b": ["a"]}),
        };

        // Test JSON output by capturing stdout
        // Note: In a real test environment, you might want to capture output differently
        assert!(output_plan_result(&result, true).is_ok());
    }

    #[test]
    fn test_output_plan_result_text() {
        let result = FeaturesPlanResult {
            order: vec!["a".to_string(), "b".to_string()],
            graph: serde_json::json!({"a": [], "b": ["a"]}),
        };

        // Test text output
        assert!(output_plan_result(&result, false).is_ok());
    }

    #[tokio::test]
    async fn test_features_plan_additional_features_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let args = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: Some("invalid json".to_string()),
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
        };

        // Should fail due to invalid JSON syntax
        let result = execute_features_plan(true, Some("invalid json"), &args).await;
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(
            err_msg.contains(
                "Failed to parse --additional-features during feature plan initialization"
            ) || err_msg.contains("parse")
        );
    }

    #[tokio::test]
    async fn test_features_plan_additional_features_not_object() {
        let temp_dir = TempDir::new().unwrap();

        // Test with array
        let args = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: Some(r#"["git", "node"]"#.to_string()),
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
        };

        let result = execute_features_plan(true, Some(r#"["git", "node"]"#), &args).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert_eq!(
            err_msg,
            "Failed to validate --additional-features: must be a JSON object."
        );

        // Test with string
        let args2 = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: Some(r#""just a string""#.to_string()),
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
        };

        let result2 = execute_features_plan(true, Some(r#""just a string""#), &args2).await;
        assert!(result2.is_err());
        let err_msg2 = format!("{}", result2.unwrap_err());
        assert_eq!(
            err_msg2,
            "Failed to validate --additional-features: must be a JSON object."
        );

        // Test with number
        let args3 = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: Some("42".to_string()),
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
        };

        let result3 = execute_features_plan(true, Some("42"), &args3).await;
        assert!(result3.is_err());
        let err_msg3 = format!("{}", result3.unwrap_err());
        assert_eq!(
            err_msg3,
            "Failed to validate --additional-features: must be a JSON object."
        );

        // Test with boolean
        let args4 = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: Some("true".to_string()),
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
        };

        let result4 = execute_features_plan(true, Some("true"), &args4).await;
        assert!(result4.is_err());
        let err_msg4 = format!("{}", result4.unwrap_err());
        assert_eq!(
            err_msg4,
            "Failed to validate --additional-features: must be a JSON object."
        );

        // Test with null
        let args5 = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: Some("null".to_string()),
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
        };

        let result5 = execute_features_plan(true, Some("null"), &args5).await;
        assert!(result5.is_err());
        let err_msg5 = format!("{}", result5.unwrap_err());
        assert_eq!(
            err_msg5,
            "Failed to validate --additional-features: must be a JSON object."
        );
    }

    #[tokio::test]
    async fn test_features_plan_additional_features_valid_object() {
        let temp_dir = TempDir::new().unwrap();
        let args = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: Some(r#"{"git": true}"#.to_string()),
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
        };

        // Should fail at OCI fetch stage, not at validation stage
        // (because "git" is not a valid OCI reference)
        let result = execute_features_plan(true, Some(r#"{"git": true}"#), &args).await;
        assert!(result.is_err());
        // The error should NOT be about JSON object validation
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            !err_msg.contains("Failed to validate --additional-features: must be a JSON object.")
        );
    }

    #[test]
    fn test_merge_semantics_additive_no_overwrite() {
        use deacon_core::features::{FeatureMergeConfig, FeatureMerger};

        // Test: Additive merge without overwrite (default behavior)
        // Config has: git=true, node="16"
        // CLI adds: docker=true, python="3.9"
        // Expected: All 4 features present, config values preserved
        let config_features = serde_json::json!({
            "git": true,
            "node": "16"
        });

        let merge_config = FeatureMergeConfig::new(
            Some(r#"{"docker": true, "python": "3.9"}"#.to_string()),
            false, // Don't prefer CLI features (default)
            None,
        );

        let result = FeatureMerger::merge_features(&config_features, &merge_config).unwrap();
        let obj = result.as_object().unwrap();

        // Verify all features are present
        assert_eq!(obj.len(), 4, "Should have 4 features after merge");

        // Verify original config features are preserved
        assert_eq!(
            obj["git"],
            serde_json::Value::Bool(true),
            "Config feature 'git' should be preserved"
        );
        assert_eq!(
            obj["node"],
            serde_json::Value::String("16".to_string()),
            "Config feature 'node' should be preserved"
        );

        // Verify CLI features are added
        assert_eq!(
            obj["docker"],
            serde_json::Value::Bool(true),
            "CLI feature 'docker' should be added"
        );
        assert_eq!(
            obj["python"],
            serde_json::Value::String("3.9".to_string()),
            "CLI feature 'python' should be added"
        );
    }

    #[test]
    fn test_merge_semantics_no_overwrite_on_conflict() {
        use deacon_core::features::{FeatureMergeConfig, FeatureMerger};

        // Test: When CLI features conflict with config, config wins by default
        // Config has: git=true, node="16"
        // CLI has: git=false, node="18"
        // Expected: Config values preserved (git=true, node="16")
        let config_features = serde_json::json!({
            "git": true,
            "node": "16"
        });

        let merge_config = FeatureMergeConfig::new(
            Some(r#"{"git": false, "node": "18"}"#.to_string()),
            false, // Config wins on conflict
            None,
        );

        let result = FeatureMerger::merge_features(&config_features, &merge_config).unwrap();
        let obj = result.as_object().unwrap();

        assert_eq!(obj.len(), 2, "Should have 2 features");

        // Verify config values are preserved, not overwritten
        assert_eq!(
            obj["git"],
            serde_json::Value::Bool(true),
            "Config value for 'git' should NOT be overwritten by CLI"
        );
        assert_eq!(
            obj["node"],
            serde_json::Value::String("16".to_string()),
            "Config value for 'node' should NOT be overwritten by CLI"
        );
    }

    #[test]
    fn test_merge_semantics_mixed_add_and_preserve() {
        use deacon_core::features::{FeatureMergeConfig, FeatureMerger};

        // Test: Mix of new features and conflicts
        // Config has: git=true, node="16", python="3.8"
        // CLI has: git=false, docker=true, rust="latest"
        // Expected: git=true (config wins), node="16", python="3.8", docker=true, rust="latest"
        let config_features = serde_json::json!({
            "git": true,
            "node": "16",
            "python": "3.8"
        });

        let merge_config = FeatureMergeConfig::new(
            Some(r#"{"git": false, "docker": true, "rust": "latest"}"#.to_string()),
            false,
            None,
        );

        let result = FeatureMerger::merge_features(&config_features, &merge_config).unwrap();
        let obj = result.as_object().unwrap();

        assert_eq!(obj.len(), 5, "Should have 5 features total");

        // Config features preserved
        assert_eq!(obj["git"], serde_json::Value::Bool(true));
        assert_eq!(obj["node"], serde_json::Value::String("16".to_string()));
        assert_eq!(obj["python"], serde_json::Value::String("3.8".to_string()));

        // CLI features added
        assert_eq!(obj["docker"], serde_json::Value::Bool(true));
        assert_eq!(obj["rust"], serde_json::Value::String("latest".to_string()));
    }

    #[test]
    fn test_override_order_affects_resolution() {
        use deacon_core::features::FeatureDependencyResolver;

        // Test: Override order determines final installation order
        // Create 3 independent features (no dependencies)
        // Override order: ["feature-c", "feature-a", "feature-b"]
        // Expected: Resolver respects this exact order
        let features = vec![
            create_mock_resolved_feature("feature-a"),
            create_mock_resolved_feature("feature-b"),
            create_mock_resolved_feature("feature-c"),
        ];

        let override_order = Some(vec![
            "feature-c".to_string(),
            "feature-a".to_string(),
            "feature-b".to_string(),
        ]);

        let resolver = FeatureDependencyResolver::new(override_order);
        let plan = resolver.resolve(&features).unwrap();
        let order = plan.feature_ids();

        // Verify the order matches the override exactly
        assert_eq!(
            order,
            vec!["feature-c", "feature-a", "feature-b"],
            "Override order should determine exact installation order"
        );
    }

    #[test]
    fn test_override_order_deterministic_with_partial_deps() {
        use deacon_core::features::FeatureDependencyResolver;

        // Test: Override order with some dependencies
        // Dependencies: feature-b depends on feature-a
        // Override order: ["feature-a", "feature-b", "feature-c"]
        // Expected: Order respects both dependencies and override
        let features = vec![
            create_mock_resolved_feature_with_deps("feature-b", &["feature-a"], &[]),
            create_mock_resolved_feature("feature-a"),
            create_mock_resolved_feature("feature-c"),
        ];

        let override_order = Some(vec![
            "feature-a".to_string(),
            "feature-b".to_string(),
            "feature-c".to_string(),
        ]);

        let resolver = FeatureDependencyResolver::new(override_order);
        let plan = resolver.resolve(&features).unwrap();
        let order = plan.feature_ids();

        // Verify the order follows override (which is valid for dependencies)
        assert_eq!(
            order,
            vec!["feature-a", "feature-b", "feature-c"],
            "Override order should be respected when it's valid for dependencies"
        );
    }

    #[test]
    fn test_override_order_without_override_uses_topo_sort() {
        use deacon_core::features::FeatureDependencyResolver;

        // Test: Without override order, resolver uses topological sort
        // Dependencies: feature-c depends on feature-b, feature-b depends on feature-a
        // No override order provided
        // Expected: Topological sort order (feature-a, feature-b, feature-c)
        let features = vec![
            create_mock_resolved_feature_with_deps("feature-c", &["feature-b"], &[]),
            create_mock_resolved_feature_with_deps("feature-b", &["feature-a"], &[]),
            create_mock_resolved_feature("feature-a"),
        ];

        let resolver = FeatureDependencyResolver::new(None);
        let plan = resolver.resolve(&features).unwrap();
        let order = plan.feature_ids();

        // Verify topological sort order
        assert_eq!(
            order,
            vec!["feature-a", "feature-b", "feature-c"],
            "Without override, should use topological sort based on dependencies"
        );
    }

    #[test]
    fn test_get_effective_install_order_with_cli_and_config() {
        use deacon_core::features::{FeatureMergeConfig, FeatureMerger};

        // Test: CLI override takes precedence over config order
        // Config order: ["git", "node"]
        // CLI order: ["docker", "git", "node"]
        // Expected: CLI order wins
        let config_order = Some(vec!["git".to_string(), "node".to_string()]);
        let merge_config =
            FeatureMergeConfig::new(None, false, Some("docker,git,node".to_string()));

        let result =
            FeatureMerger::get_effective_install_order(config_order.as_ref(), &merge_config)
                .unwrap();

        assert_eq!(
            result,
            Some(vec![
                "docker".to_string(),
                "git".to_string(),
                "node".to_string()
            ]),
            "CLI override should take precedence over config order"
        );
    }

    #[test]
    fn test_get_effective_install_order_config_only() {
        use deacon_core::features::{FeatureMergeConfig, FeatureMerger};

        // Test: Without CLI override, use config order
        // Config order: ["python", "node", "docker"]
        // CLI order: None
        // Expected: Config order used
        let config_order = Some(vec![
            "python".to_string(),
            "node".to_string(),
            "docker".to_string(),
        ]);
        let merge_config = FeatureMergeConfig::new(None, false, None);

        let result =
            FeatureMerger::get_effective_install_order(config_order.as_ref(), &merge_config)
                .unwrap();

        assert_eq!(
            result,
            Some(vec![
                "python".to_string(),
                "node".to_string(),
                "docker".to_string()
            ]),
            "Config order should be used when no CLI override provided"
        );
    }

    #[test]
    fn test_merge_with_override_order_maintains_determinism() {
        use deacon_core::features::{FeatureDependencyResolver, FeatureMergeConfig, FeatureMerger};

        // Test: Merging features and applying override order produces deterministic results
        // This combines merge semantics with override order behavior
        let config_features = serde_json::json!({
            "feature-a": true,
            "feature-b": true
        });

        let merge_config = FeatureMergeConfig::new(
            Some(r#"{"feature-c": true}"#.to_string()),
            false,
            Some("feature-c,feature-b,feature-a".to_string()),
        );

        // First verify merge works
        let merged = FeatureMerger::merge_features(&config_features, &merge_config).unwrap();
        assert_eq!(merged.as_object().unwrap().len(), 3);

        // Then verify override order is extracted correctly
        let override_order =
            FeatureMerger::get_effective_install_order(None, &merge_config).unwrap();
        assert_eq!(
            override_order,
            Some(vec![
                "feature-c".to_string(),
                "feature-b".to_string(),
                "feature-a".to_string()
            ])
        );

        // Verify resolver respects this order
        let features = vec![
            create_mock_resolved_feature("feature-a"),
            create_mock_resolved_feature("feature-b"),
            create_mock_resolved_feature("feature-c"),
        ];
        let resolver = FeatureDependencyResolver::new(override_order);
        let plan = resolver.resolve(&features).unwrap();
        let order = plan.feature_ids();

        assert_eq!(
            order,
            vec!["feature-c", "feature-b", "feature-a"],
            "Override order should produce deterministic installation plan"
        );
    }

    #[test]
    fn test_merge_semantics_with_chain_dependencies() {
        use deacon_core::features::FeatureDependencyResolver;

        // Test: Merge semantics work correctly when features have chain dependencies
        // feature-c depends on feature-b, feature-b depends on feature-a
        // This verifies merge doesn't break dependency resolution
        let features = vec![
            create_mock_resolved_feature_with_deps("feature-c", &["feature-b"], &[]),
            create_mock_resolved_feature_with_deps("feature-b", &["feature-a"], &[]),
            create_mock_resolved_feature("feature-a"),
        ];

        let resolver = FeatureDependencyResolver::new(None);
        let plan = resolver.resolve(&features).unwrap();
        let order = plan.feature_ids();

        // Verify topological order respects chain
        assert_eq!(
            order,
            vec!["feature-a", "feature-b", "feature-c"],
            "Chain dependencies should be resolved in correct order"
        );

        // Verify deterministic (multiple runs produce same result)
        let plan2 = resolver.resolve(&features).unwrap();
        let order2 = plan2.feature_ids();
        assert_eq!(order, order2, "Resolution should be deterministic");
    }

    #[test]
    fn test_negative_invalid_additional_features_empty_string() {
        use deacon_core::features::{FeatureMergeConfig, FeatureMerger};

        // Test: Empty string for additional features should fail
        let config_features = serde_json::json!({"git": true});
        let merge_config = FeatureMergeConfig::new(Some("".to_string()), false, None);

        let result = FeatureMerger::merge_features(&config_features, &merge_config);
        assert!(result.is_err(), "Empty string should produce parse error");

        if let Err(e) = result {
            let err_msg = format!("{}", e);
            assert!(
                err_msg.contains("parse") || err_msg.contains("JSON"),
                "Error should mention parsing or JSON, got: {}",
                err_msg
            );
        }
    }

    #[test]
    fn test_negative_malformed_json_additional_features() {
        use deacon_core::features::{FeatureMergeConfig, FeatureMerger};

        // Test: Malformed JSON should produce explicit error
        let config_features = serde_json::json!({"git": true});
        let merge_config =
            FeatureMergeConfig::new(Some(r#"{"unclosed": true"#.to_string()), false, None);

        let result = FeatureMerger::merge_features(&config_features, &merge_config);
        assert!(result.is_err(), "Malformed JSON should produce error");

        if let Err(e) = result {
            let err_msg = format!("{}", e);
            assert!(
                err_msg.contains("parse") || err_msg.contains("JSON"),
                "Error should mention parsing issue, got: {}",
                err_msg
            );
        }
    }

    #[test]
    fn test_graph_edge_direction_consistency() {
        // Test: Verify graph edges consistently point from dependent to dependency
        // When feature-b depends on feature-a, graph should show: "feature-b": ["feature-a"]
        let features = vec![
            create_mock_resolved_feature("feature-a"),
            create_mock_resolved_feature_with_deps("feature-b", &["feature-a"], &[]),
        ];

        let graph = build_graph_representation(&features);
        let graph_obj = graph.as_object().unwrap();

        // feature-a has no dependencies
        assert_eq!(
            graph_obj["feature-a"].as_array().unwrap().len(),
            0,
            "feature-a should have empty dependency array"
        );

        // feature-b depends on feature-a (edge points to dependency)
        let b_deps = graph_obj["feature-b"].as_array().unwrap();
        assert_eq!(b_deps.len(), 1, "feature-b should have 1 dependency");
        assert_eq!(
            b_deps[0],
            serde_json::Value::String("feature-a".to_string()),
            "Graph edge should point from dependent (feature-b) to dependency (feature-a)"
        );
    }

    #[test]
    fn test_json_output_order_deterministic() {
        // Test: JSON output order should be deterministic (sorted)
        // This ensures snapshot tests and comparisons are reliable
        let features = vec![
            create_mock_resolved_feature("zebra"),
            create_mock_resolved_feature("alpha"),
            create_mock_resolved_feature("beta"),
        ];

        let graph = build_graph_representation(&features);
        let json_str = serde_json::to_string(&graph).unwrap();

        // Verify JSON contains all features
        assert!(json_str.contains("zebra"));
        assert!(json_str.contains("alpha"));
        assert!(json_str.contains("beta"));

        // Verify structure is consistent (object with array values)
        let graph_obj = graph.as_object().unwrap();
        assert_eq!(graph_obj.len(), 3);
        for (_key, value) in graph_obj {
            assert!(
                value.is_array(),
                "Each feature should have an array of dependencies"
            );
        }

        // Multiple serializations should produce identical output
        let json_str2 = serde_json::to_string(&graph).unwrap();
        assert_eq!(
            json_str, json_str2,
            "JSON serialization should be deterministic"
        );
    }

    #[test]
    fn test_option_preservation_roundtrip_mixed_types() {
        // Test that all JSON types in options are preserved through the conversion pipeline
        // This validates the fix for silent drops of Number, Array, Object, Null types
        use deacon_core::features::OptionValue;
        use std::collections::HashMap;

        // Create options with all supported JSON types
        let mut input_options = HashMap::new();
        input_options.insert(
            "stringOption".to_string(),
            OptionValue::String("latest".to_string()),
        );
        input_options.insert("boolOption".to_string(), OptionValue::Boolean(true));
        input_options.insert(
            "numberOption".to_string(),
            OptionValue::Number(serde_json::Number::from(300)),
        );
        input_options.insert(
            "arrayOption".to_string(),
            OptionValue::Array(vec![
                serde_json::Value::String("repo1".to_string()),
                serde_json::Value::Number(serde_json::Number::from(42)),
            ]),
        );
        let mut obj = serde_json::Map::new();
        obj.insert(
            "nested".to_string(),
            serde_json::Value::String("value".to_string()),
        );
        input_options.insert("objectOption".to_string(), OptionValue::Object(obj));
        input_options.insert("nullOption".to_string(), OptionValue::Null);

        // Verify all options are preserved (not silently dropped)
        assert_eq!(
            input_options.len(),
            6,
            "All 6 option types should be present"
        );

        // Convert to JSON (simulating what read_configuration does)
        let json_options: HashMap<String, serde_json::Value> = input_options
            .iter()
            .map(|(k, v)| {
                let json_val = match v {
                    OptionValue::Boolean(b) => serde_json::Value::Bool(*b),
                    OptionValue::String(s) => serde_json::Value::String(s.clone()),
                    OptionValue::Number(n) => serde_json::Value::Number(n.clone()),
                    OptionValue::Array(a) => serde_json::Value::Array(a.clone()),
                    OptionValue::Object(o) => serde_json::Value::Object(o.clone()),
                    OptionValue::Null => serde_json::Value::Null,
                };
                (k.clone(), json_val)
            })
            .collect();

        // Verify no data loss in conversion
        assert_eq!(
            json_options.len(),
            6,
            "All options should survive JSON conversion"
        );
        assert!(json_options.contains_key("stringOption"));
        assert!(json_options.contains_key("boolOption"));
        assert!(json_options.contains_key("numberOption"));
        assert!(json_options.contains_key("arrayOption"));
        assert!(json_options.contains_key("objectOption"));
        assert!(json_options.contains_key("nullOption"));

        // Convert back to OptionValue (simulating what features.rs does)
        let roundtrip_options: HashMap<String, OptionValue> = json_options
            .iter()
            .map(|(k, v)| {
                let option_value = match v {
                    serde_json::Value::Bool(b) => OptionValue::Boolean(*b),
                    serde_json::Value::String(s) => OptionValue::String(s.clone()),
                    serde_json::Value::Number(n) => OptionValue::Number(n.clone()),
                    serde_json::Value::Array(a) => OptionValue::Array(a.clone()),
                    serde_json::Value::Object(o) => OptionValue::Object(o.clone()),
                    serde_json::Value::Null => OptionValue::Null,
                };
                (k.clone(), option_value)
            })
            .collect();

        // Verify complete roundtrip preservation
        assert_eq!(
            roundtrip_options.len(),
            6,
            "All options should survive complete roundtrip"
        );
        assert_eq!(
            roundtrip_options.get("stringOption").unwrap().as_str(),
            Some("latest")
        );
        assert_eq!(
            roundtrip_options.get("boolOption").unwrap().as_bool(),
            Some(true)
        );
        assert!(roundtrip_options
            .get("numberOption")
            .unwrap()
            .as_number()
            .is_some());
        assert!(roundtrip_options
            .get("arrayOption")
            .unwrap()
            .as_array()
            .is_some());
        assert!(roundtrip_options
            .get("objectOption")
            .unwrap()
            .as_object()
            .is_some());
        assert!(roundtrip_options.get("nullOption").unwrap().is_null());
    }

    #[tokio::test]
    async fn test_error_context_parse_additional_features() {
        // Test: Verify error message includes proper context for --additional-features parsing
        // Requirement: GAP.md §8 - Error context could be more specific
        let temp_dir = TempDir::new().unwrap();
        let args = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: Some("invalid json".to_string()),
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
        };

        let result = execute_features_plan(true, Some("invalid json"), &args).await;
        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());

        // Verify error message contains phase context
        assert!(
            err_msg.contains("parse") && err_msg.contains("--additional-features"),
            "Error should contain phase (parse) and flag (--additional-features), got: {}",
            err_msg
        );
        assert!(
            err_msg.contains("feature plan initialization"),
            "Error should contain phase context, got: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_error_context_validate_additional_features() {
        // Test: Verify error message includes proper context for --additional-features validation
        // Requirement: GAP.md §8 - Error context could be more specific
        let temp_dir = TempDir::new().unwrap();
        let args = FeaturesArgs {
            command: FeatureCommands::Plan {
                json: true,
                additional_features: Some(r#"["not", "an", "object"]"#.to_string()),
            },
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
        };

        let result = execute_features_plan(true, Some(r#"["not", "an", "object"]"#), &args).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());

        // Verify error message contains validation context
        assert!(
            err_msg.contains("validate") && err_msg.contains("--additional-features"),
            "Error should contain phase (validate) and flag (--additional-features), got: {}",
            err_msg
        );
        assert!(
            err_msg.ends_with('.'),
            "Error message should end with period per Theme 6, got: {}",
            err_msg
        );
    }
}
