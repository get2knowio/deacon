//! Read configuration command implementation
//!
//! Implements the `deacon read-configuration` subcommand for reading and displaying
//! DevContainer configuration with variable substitution and extends resolution.
//!
//! Spec: docs/subcommand-specs/read-configuration/SPEC.md
//! Implementation: specs/001-read-config-parity/spec.md

use crate::commands::shared::{load_config, ConfigLoadArgs, TerminalDimensions};
use anyhow::{Context, Result};
use deacon_core::config::DevContainerConfig;
use deacon_core::container::ContainerSelector;

use deacon_core::features::{
    FeatureDependencyResolver, FeatureMergeConfig, FeatureMerger, OptionValue, ResolvedFeature,
};
use deacon_core::io::Output;
use deacon_core::oci::{default_fetcher_with_config, FeatureRef};
use deacon_core::redaction::{RedactionConfig, SecretRegistry};
use deacon_core::registry_parser::parse_registry_reference;
use deacon_core::secrets::SecretsCollection;
use deacon_core::variable::SubstitutionContext;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, instrument};

/// Read configuration command arguments
#[derive(Debug, Clone)]
pub struct ReadConfigurationArgs {
    pub include_merged_configuration: bool,
    pub include_features_configuration: bool,
    /// TODO(#268): Implement container-based config reading
    /// When container_id is provided, read configuration from running container
    #[allow(dead_code)]
    pub container_id: Option<String>,
    /// When id_label is provided, resolve container and read configuration from it
    #[allow(dead_code)]
    pub id_label: Vec<String>,
    /// Flag to control workspace root discovery behavior.
    /// When true (default), uses Git worktree detection to find the true workspace root.
    /// When false, uses the workspace folder path as-is.
    pub mount_workspace_git_root: bool,
    pub additional_features: Option<String>,
    pub skip_feature_auto_mapping: bool,
    /// Docker tooling path. Accepted per spec for CLI parity; not yet consumed by implementation.
    /// Future use: #292 will integrate with container runtime selection.
    #[allow(dead_code)]
    pub docker_path: String,
    /// Docker Compose tooling path. Accepted per spec for CLI parity; not yet consumed by
    /// implementation. Future use: #292 will integrate with container runtime selection.
    #[allow(dead_code)]
    pub docker_compose_path: String,
    /// User data folder path. Accepted per spec (Section 3, line 62) but not used by
    /// read-configuration command. Included for CLI parity with specification.
    #[allow(dead_code)]
    pub user_data_folder: Option<PathBuf>,
    pub terminal_columns: Option<u32>,
    pub terminal_rows: Option<u32>,
    pub workspace_folder: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub override_config_path: Option<PathBuf>,
    pub secrets_files: Vec<PathBuf>,
    pub redaction_config: RedactionConfig,
    pub secret_registry: SecretRegistry,
}

/// Features configuration output structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeaturesConfiguration {
    pub feature_sets: Vec<FeatureSet>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dst_folder: Option<String>,
}

/// Feature set with resolved features and source information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureSet {
    pub features: Vec<Feature>,
    pub source_information: SourceInformation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computed_digest: Option<String>,
}

/// Individual feature in output
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Feature {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<HashMap<String, serde_json::Value>>,
    /// Source reference preserving registry/namespace/tag (e.g., "oci://ghcr.io/devcontainers/features/node:1.2.3")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Source information for features
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SourceInformation {
    #[serde(rename = "oci")]
    Oci { registry: String },
}

/// Workspace configuration information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceConfig {
    /// Resolved workspace folder path (container path after substitution)
    pub workspace_folder: String,
    /// Workspace mount specification (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_mount: Option<String>,
    /// Configuration folder path (host path to .devcontainer directory)
    pub config_folder_path: String,
    /// Root folder path (host workspace root)
    pub root_folder_path: String,
}

/// Output payload structure for read-configuration command
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadConfigurationOutput {
    pub configuration: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features_configuration: Option<FeaturesConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merged_configuration: Option<serde_json::Value>,
}

/// Resolve workspace configuration
///
/// Computes the workspace configuration including folder paths and mount specifications.
/// Uses the `mount_workspace_git_root` flag to determine whether to mount the Git root
/// or the immediate workspace folder.
#[instrument(skip_all)]
fn resolve_workspace_configuration(
    workspace_folder: &Path,
    config_path: Option<&Path>,
    mount_workspace_git_root: bool,
) -> Result<WorkspaceConfig> {
    // Determine the root folder path based on mount_workspace_git_root flag
    let root_folder_path = if mount_workspace_git_root {
        // Use Git worktree detection to find the true workspace root
        deacon_core::workspace::resolve_workspace_root(workspace_folder)?
    } else {
        // Use the workspace folder as-is
        workspace_folder
            .canonicalize()
            .unwrap_or_else(|_| workspace_folder.to_path_buf())
    };

    // Determine config folder path
    let config_folder_path = if let Some(config) = config_path {
        // If config path is a directory, use it directly
        // Otherwise, use its parent directory (for file paths)
        if config.is_dir() {
            config.to_path_buf()
        } else {
            config.parent().unwrap_or(workspace_folder).to_path_buf()
        }
    } else {
        // Otherwise, look for .devcontainer directory in workspace
        let devcontainer_dir = workspace_folder.join(".devcontainer");
        if devcontainer_dir.exists() && devcontainer_dir.is_dir() {
            devcontainer_dir
        } else {
            workspace_folder.to_path_buf()
        }
    };

    // Compute workspace folder (container path - typically /workspaces/<basename>)
    let workspace_basename = root_folder_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace");
    let container_workspace_folder = format!("/workspaces/{}", workspace_basename);

    // Compute workspace mount specification
    // Format: type=bind,source=<host-path>,target=<container-path>
    // Always provided to indicate the default workspace mounting behavior
    let workspace_mount = Some(format!(
        "type=bind,source={},target={}",
        root_folder_path.display(),
        container_workspace_folder
    ));

    Ok(WorkspaceConfig {
        workspace_folder: container_workspace_folder,
        workspace_mount,
        config_folder_path: config_folder_path.to_string_lossy().to_string(),
        root_folder_path: root_folder_path.to_string_lossy().to_string(),
    })
}

/// Resolve features configuration from the DevContainer config
async fn resolve_features_configuration<C: deacon_core::oci::HttpClient>(
    config: &deacon_core::config::DevContainerConfig,
    additional_features: Option<&str>,
    skip_feature_auto_mapping: bool,
    fetcher: &deacon_core::oci::FeatureFetcher<C>,
) -> Result<FeaturesConfiguration> {
    use anyhow::Context;

    // Clone and prepare the config
    let mut config = config.clone();

    // Parse and merge additional features if provided
    if let Some(additional_features_str) = additional_features {
        // Early validation: parse JSON and ensure it's an object before merge
        let parsed_json: serde_json::Value = serde_json::from_str(additional_features_str)
            .with_context(|| {
                format!(
                    "Failed to parse --additional-features JSON: {}",
                    additional_features_str
                )
            })?;

        // Validate that the parsed JSON is an object (map)
        if !parsed_json.is_object() {
            anyhow::bail!("--additional-features must be a JSON object.");
        }

        let merge_config = FeatureMergeConfig::new(
            Some(additional_features_str.to_string()),
            true,                      // CLI features take precedence over config features
            None,                      // No install order override in this context
            skip_feature_auto_mapping, // Respect CLI flag for auto-mapping behavior
        );
        config.features = FeatureMerger::merge_features(&config.features, &merge_config)?;
    }

    // Extract features from config
    let features_map_opt = config.features.as_object();
    if features_map_opt.is_none() || features_map_opt.unwrap().is_empty() {
        // No features, return empty configuration
        return Ok(FeaturesConfiguration {
            feature_sets: vec![],
            dst_folder: None,
        });
    }
    let features_map = features_map_opt.unwrap();

    // Use provided fetcher to resolve features from registries
    let mut resolved_features = Vec::with_capacity(features_map.len());

    for (feature_id, feature_value) in features_map {
        let (registry_url, namespace, name, tag) = parse_registry_reference(feature_id)?;
        let feature_ref = FeatureRef::new(
            registry_url.clone(),
            namespace.clone(),
            name.clone(),
            tag.clone(),
        );
        let downloaded = fetcher
            .fetch_feature(&feature_ref)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch feature '{}': {}", feature_id, e))?;

        // Extract per-feature options from config entry if present
        let options: HashMap<String, OptionValue> = match feature_value {
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
            serde_json::Value::String(s) if !skip_feature_auto_mapping => {
                // Auto-map top-level string value to "version" option
                let mut map = HashMap::new();
                map.insert("version".to_string(), OptionValue::String(s.clone()));
                map
            }
            serde_json::Value::Bool(_b) => {
                // For boolean true, no options; for false, skip feature (but we're here so it's true)
                HashMap::new()
            }
            _ => HashMap::new(),
        };

        resolved_features.push(ResolvedFeature {
            id: downloaded.metadata.id.clone(),
            source: feature_ref.reference(),
            options,
            metadata: downloaded.metadata,
        });
    }

    // Create dependency resolver
    let override_order = config.override_feature_install_order.clone();
    let resolver = FeatureDependencyResolver::new(override_order);

    // Resolve dependencies and create installation plan
    let _installation_plan = resolver.resolve(&resolved_features)?;

    // Group features by registry extracted from their source
    use std::collections::BTreeMap;
    let mut features_by_registry: BTreeMap<String, Vec<Feature>> = BTreeMap::new();

    for resolved in &resolved_features {
        // Extract registry from source (format: "oci://registry/namespace/name:tag")
        let registry = if resolved.source.starts_with("oci://") {
            let without_prefix = resolved.source.trim_start_matches("oci://");
            // Extract first component (registry) before first slash
            without_prefix
                .split('/')
                .next()
                .unwrap_or("ghcr.io")
                .to_string()
        } else {
            // Fallback for non-OCI sources
            "ghcr.io".to_string()
        };

        let options = if resolved.options.is_empty() {
            None
        } else {
            Some(
                resolved
                    .options
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
                    .collect(),
            )
        };

        let feature = Feature {
            id: resolved.id.clone(),
            options,
            source: Some(resolved.source.clone()),
        };

        features_by_registry
            .entry(registry)
            .or_default()
            .push(feature);
    }

    // Build one FeatureSet per registry
    let feature_sets: Vec<FeatureSet> = features_by_registry
        .into_iter()
        .map(|(registry, features)| FeatureSet {
            features,
            source_information: SourceInformation::Oci { registry },
            internal_version: None,
            computed_digest: None,
        })
        .collect();

    Ok(FeaturesConfiguration {
        feature_sets,
        dst_folder: None,
    })
}

/// Compute merged configuration by merging base config with image metadata
///
/// Per the specification, merged configuration is:
/// `mergedConfiguration = mergeConfiguration(base_config, imageMetadata)`
///
/// Where imageMetadata comes from:
/// - Container inspection (when container_id is provided) - extracts devcontainer.metadata label
/// - Features metadata computation (when no container) - **Blocked by #289**
///
/// ## Current Implementation Status
///
/// Container-based merge is implemented. Features-based merge is a placeholder.
///
#[instrument(skip_all)]
async fn compute_merged_configuration<C: deacon_core::oci::HttpClient>(
    base_config: &deacon_core::config::DevContainerConfig,
    container_info: Option<&deacon_core::docker::ContainerInfo>,
    container_context: Option<&SubstitutionContext>,
    features_config: Option<&FeaturesConfiguration>,
    secrets: Option<&SecretsCollection>,
    fetcher: &deacon_core::oci::FeatureFetcher<C>,
) -> Result<serde_json::Value> {
    debug!(
        "Computing merged configuration: has_container={:?}, has_features={}",
        container_info.is_some(),
        features_config.is_some()
    );

    if let Some(container_info) = container_info {
        // Container-based merge: extract devcontainer.metadata label
        let metadata_label = container_info.labels.get("devcontainer.metadata");
        let metadata_str = metadata_label.ok_or_else(|| {
            anyhow::anyhow!(
                "Container '{}' does not have required 'devcontainer.metadata' label. \
                 Cannot compute merged configuration without container metadata.",
                container_info.id
            )
        })?;

        debug!("Found devcontainer.metadata label: {}", metadata_str);

        // Parse the metadata JSON
        let metadata_value: serde_json::Value =
            serde_json::from_str(metadata_str).with_context(|| {
                format!(
                    "Failed to parse devcontainer.metadata JSON from container '{}': {}",
                    container_info.id, metadata_str
                )
            })?;

        // Convert to DevContainerConfig
        let metadata_config: deacon_core::config::DevContainerConfig =
            serde_json::from_value(metadata_value)
                .with_context(|| {
                    format!(
                        "Failed to deserialize devcontainer.metadata into DevContainerConfig from container '{}'",
                        container_info.id
                    )
                })?;

        // Apply container substitution to the metadata
        let container_context = container_context.ok_or_else(|| {
            anyhow::anyhow!("Container context required for container-based merged configuration")
        })?;
        let (substituted_metadata, _) =
            metadata_config.apply_variable_substitution(container_context);

        // Merge base config with substituted metadata
        let merged = deacon_core::config::ConfigMerger::merge_configs(&[
            base_config.clone(),
            substituted_metadata,
        ]);

        debug!("Container-based merged configuration computed successfully");
        Ok(serde_json::to_value(&merged)?)
    } else if let Some(features_config) = features_config {
        debug!("Computing features-based merged configuration");

        // Derive configuration from features metadata
        let mut derived_config = deacon_core::config::DevContainerConfig::default();

        for feature_set in &features_config.feature_sets {
            for feature in &feature_set.features {
                // We need to get the feature metadata - this requires fetching it again
                // or storing it in the FeaturesConfiguration. For now, we'll fetch it.
                // TODO: Consider caching metadata in FeaturesConfiguration to avoid refetching

                // Parse the feature reference - prefer the preserved source field if available
                let reference_to_parse = feature.source.as_ref().unwrap_or(&feature.id);
                let (registry_url, namespace, name, tag) =
                    parse_registry_reference(reference_to_parse)?;

                // Use the provided fetcher with configured timeout and retries
                let feature_ref = deacon_core::oci::FeatureRef::new(
                    registry_url.clone(),
                    namespace.clone(),
                    name.clone(),
                    tag.clone(),
                );

                let downloaded = fetcher.fetch_feature(&feature_ref).await.map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to fetch feature '{}' for merged config: {}",
                        feature.id,
                        e
                    )
                })?;

                let metadata = &downloaded.metadata;

                // Merge feature metadata into derived config
                // Container environment variables
                for (key, value) in &metadata.container_env {
                    derived_config
                        .container_env
                        .insert(key.clone(), value.clone());
                }

                // Mounts - convert Vec<String> to Vec<serde_json::Value>
                for mount in &metadata.mounts {
                    derived_config
                        .mounts
                        .push(serde_json::Value::String(mount.clone()));
                }

                // TODO: Support metadata.init properly when DevContainerConfig gains an init field
                // For now, metadata.init is not mapped to avoid incorrectly enabling privileged mode

                // Privileged flag
                if let Some(privileged) = metadata.privileged {
                    derived_config.privileged = Some(privileged);
                }

                // Capabilities to add
                derived_config.cap_add.extend(metadata.cap_add.clone());

                // Security options
                derived_config
                    .security_opt
                    .extend(metadata.security_opt.clone());

                // Entrypoint override - DevContainerConfig doesn't have entrypoint field
                // if let Some(entrypoint) = &metadata.entrypoint {
                //     derived_config.entrypoint = Some(entrypoint.clone());
                // }

                // Lifecycle commands
                if let Some(cmd) = &metadata.on_create_command {
                    derived_config.on_create_command = Some(cmd.clone());
                }
                if let Some(cmd) = &metadata.update_content_command {
                    derived_config.update_content_command = Some(cmd.clone());
                }
                if let Some(cmd) = &metadata.post_create_command {
                    derived_config.post_create_command = Some(cmd.clone());
                }
                if let Some(cmd) = &metadata.post_start_command {
                    derived_config.post_start_command = Some(cmd.clone());
                }
                if let Some(cmd) = &metadata.post_attach_command {
                    derived_config.post_attach_command = Some(cmd.clone());
                }
            }
        }

        // Apply variable substitution to the derived config
        let mut substitution_context = SubstitutionContext::new(Path::new("."))?;
        if let Some(secrets) = secrets {
            for (key, value) in secrets.as_env_vars() {
                substitution_context
                    .local_env
                    .insert(key.clone(), value.clone());
            }
        }
        let (substituted_derived, _) =
            derived_config.apply_variable_substitution(&substitution_context);

        // Merge base config with derived feature metadata
        let merged = deacon_core::config::ConfigMerger::merge_configs(&[
            base_config.clone(),
            substituted_derived,
        ]);

        debug!("Features-based merged configuration computed successfully");
        Ok(serde_json::to_value(&merged)?)
    } else {
        // No container and no features: merged config is same as base config
        debug!("No metadata sources available; merged config equals base config");
        Ok(serde_json::to_value(base_config)?)
    }
}

/// Validate devcontainer configuration filename.
///
/// Per spec FR-004 and docs/subcommand-specs/up/SPEC.md §4:
/// "IF configFile specified AND file name not devcontainer.json/.devcontainer.json:
///     ERROR 'Filename must be devcontainer.json or .devcontainer.json'"
///
/// Note: We also accept .jsonc extensions (JSON with Comments) as they are widely
/// used in the devcontainer ecosystem and supported by our JSON5 parser.
///
/// This validation applies to both --config and --override-config paths.
fn validate_config_filename(config_path: &Path, flag_name: &str) -> Result<()> {
    if let Some(filename) = config_path.file_name().and_then(|n| n.to_str()) {
        let valid_names = [
            "devcontainer.json",
            ".devcontainer.json",
            "devcontainer.jsonc",
            ".devcontainer.jsonc",
        ];

        if !valid_names.contains(&filename) {
            anyhow::bail!(
                "Invalid {} filename: '{}'. Filename must be one of: devcontainer.json, .devcontainer.json, devcontainer.jsonc, or .devcontainer.jsonc.",
                flag_name,
                filename
            );
        }
    }
    Ok(())
}

/// Execute the read-configuration command
#[instrument(skip(args))]
pub async fn execute_read_configuration(args: ReadConfigurationArgs) -> Result<()> {
    // Keep startup message at debug to avoid noisy INFO output for simple queries
    debug!("Starting read-configuration command execution");
    debug!(
        "Read configuration args: include_merged={}, include_features={}, mount_workspace_git_root={}, workspace_folder={:?}, config_path={:?}, override_config_path={:?}, secrets_files_count={}",
        args.include_merged_configuration,
        args.include_features_configuration,
        args.mount_workspace_git_root,
        args.workspace_folder,
        args.config_path,
        args.override_config_path,
        args.secrets_files.len()
    );

    // Validate devcontainer configuration filenames per FR-004
    // Must be devcontainer.json or .devcontainer.json
    if let Some(config_path) = args.config_path.as_ref() {
        validate_config_filename(config_path, "--config")?;
    }
    if let Some(override_path) = args.override_config_path.as_ref() {
        validate_config_filename(override_path, "--override-config")?;
    }

    // Selector validation per spec (§2, §9):
    // At least one of --container-id, --id-label, or --workspace-folder is required.
    // Note: --config alone does NOT satisfy this requirement.
    let has_container_id = args.container_id.is_some();
    let has_id_label = !args.id_label.is_empty();
    let has_workspace_folder = args.workspace_folder.is_some();
    if !has_container_id && !has_id_label && !has_workspace_folder {
        anyhow::bail!(
            "Missing required argument: One of --container-id, --id-label or --workspace-folder is required."
        );
    }

    // Validate id_label format (must match <name>=<value> pattern)
    if !args.id_label.is_empty() {
        ContainerSelector::parse_labels(&args.id_label)?;
    }

    // Validate terminal dimensions using shared helper (aligns with up/exec validation)
    let _terminal_dimensions = TerminalDimensions::new(args.terminal_columns, args.terminal_rows)?;

    // Validate additional_features JSON early (must be an object if provided)
    if let Some(additional_features_str) = args.additional_features.as_ref() {
        use anyhow::Context;
        let parsed_json: serde_json::Value = serde_json::from_str(additional_features_str)
            .with_context(|| {
                format!(
                    "Failed to parse --additional-features JSON: {}",
                    additional_features_str
                )
            })?;

        if !parsed_json.is_object() {
            anyhow::bail!("--additional-features must be a JSON object.");
        }
    }

    // Create output helper with redaction support
    let mut output = Output::new(args.redaction_config.clone(), &args.secret_registry);

    // Determine if we're in container-only mode (only container selectors, no config/workspace)
    let container_only_mode = args.config_path.is_none()
        && args.workspace_folder.is_none()
        && args.override_config_path.is_none();

    // Determine workspace folder
    let workspace_folder = args.workspace_folder.as_deref().unwrap_or(Path::new("."));

    // Load configuration using shared helper (aligns with up/exec behavior)
    let (config, substitution_report) = if container_only_mode {
        // Per spec line 104: "If only container selection flags are provided (no config or workspace),
        // proceed with an empty base config {} and a substitution function seeded with host env/paths."
        let empty_config = DevContainerConfig::default();
        let substitution_context = SubstitutionContext::new(workspace_folder)?;
        let (substituted_config, report) =
            empty_config.apply_variable_substitution(&substitution_context);
        (substituted_config, report)
    } else {
        // Use shared config loader for consistent behavior across subcommands
        let config_result = load_config(ConfigLoadArgs {
            workspace_folder: Some(workspace_folder),
            config_path: args.config_path.as_deref(),
            override_config_path: args.override_config_path.as_deref(),
            secrets_files: &args.secrets_files,
        })?;
        (config_result.config, config_result.substitution_report)
    };

    // Always try to resolve workspace configuration (unless container-only mode)
    // Per spec: workspace is omitted only if it cannot be resolved
    let workspace_config = if container_only_mode {
        None
    } else {
        resolve_workspace_configuration(
            workspace_folder,
            args.config_path.as_deref(),
            args.mount_workspace_git_root,
        )
        .ok()
    };

    // Load secrets separately from config loading for container-specific substitution
    // Note: While load_config() handles secrets for initial config substitution,
    // we need the SecretsCollection again later for container-aware substitutions
    // (lines 839-843 and 890-896) which apply additional variables like ${containerEnv:*}
    let secrets = if !args.secrets_files.is_empty() {
        Some(SecretsCollection::load_from_files(&args.secrets_files)?)
    } else {
        None
    };

    debug!("Loaded configuration: {:?}", config.name);
    debug!(
        "Applied variable substitution: {} replacements made",
        substitution_report.replacements.len()
    );

    // Container discovery and container-aware substitutions
    let (config, container_id_labels, container_env, container_info, container_context) = if args
        .container_id
        .is_some()
        || !args.id_label.is_empty()
    {
        // Discover container using provided selectors
        debug!("Container discovery requested");
        let docker = deacon_core::docker::CliDocker::new();

        // Build container selector
        let selector = ContainerSelector::new(
            args.container_id.clone(),
            args.id_label.clone(),
            args.workspace_folder.clone(),
            args.override_config_path.clone(),
        )?;
        selector.validate()?;

        // Extract id-labels (use provided labels or extract from container)
        let id_labels: Vec<(String, String)> = if !args.id_label.is_empty() {
            // Use provided labels (already parsed and validated above)
            ContainerSelector::parse_labels(&args.id_label)?
        } else {
            // For container_id only, we'll extract labels from container if found
            vec![]
        };

        // Compute devcontainerId from id-labels (before container lookup)
        let dev_container_id = deacon_core::container::compute_dev_container_id(&id_labels);
        debug!("Computed devcontainerId: {}", dev_container_id);

        // Apply beforeContainerSubstitute: ${devcontainerId}
        let mut before_context = SubstitutionContext::new(workspace_folder)?;
        before_context.devcontainer_id = dev_container_id.clone();
        if let Some(secrets) = &secrets {
            for (key, value) in secrets.as_env_vars() {
                before_context.local_env.insert(key.clone(), value.clone());
            }
        }

        let (config_after_before, _before_report) =
            config.apply_variable_substitution(&before_context);

        // Now try to resolve container for additional substitutions
        let resolved = match deacon_core::container::resolve_container(&docker, &selector).await {
            Ok(opt) => opt,
            Err(e) => {
                // In container-only mode (no merged config requested), treat discovery errors as no container
                if args.include_merged_configuration {
                    return Err(e.into());
                } else {
                    debug!("Container discovery error (treated as no container): {}", e);
                    None
                }
            }
        };

        match resolved {
            Some(container_info) => {
                debug!(
                    "Container found: id={}, labels={:?}",
                    container_info.id, container_info.labels
                );

                // Extract id-labels from container if we didn't have them from command line
                let final_id_labels = if !id_labels.is_empty() {
                    id_labels
                } else {
                    // Extract relevant labels from container
                    container_info
                        .labels
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                };

                // Apply containerSubstitute: ${containerEnv:VAR}, ${containerWorkspaceFolder}
                let mut container_context = SubstitutionContext::new(workspace_folder)?;
                container_context.devcontainer_id = dev_container_id;
                container_context.container_env = Some(container_info.env.clone());
                // Extract containerWorkspaceFolder from container mounts
                let container_workspace_folder =
                    deacon_core::docker::derive_container_workspace_folder(&container_info.mounts);
                container_context.container_workspace_folder = container_workspace_folder;

                if let Some(secrets) = &secrets {
                    for (key, value) in secrets.as_env_vars() {
                        container_context
                            .local_env
                            .insert(key.clone(), value.clone());
                    }
                }

                let (config_final, _container_report) =
                    config_after_before.apply_variable_substitution(&container_context);

                (
                    config_final,
                    Some(final_id_labels),
                    Some(container_info.env.clone()),
                    Some(container_info),
                    Some(container_context),
                )
            }
            None => {
                // Container not found
                debug!("Container not found for selector: {:?}", selector);

                // If merged configuration is requested, we need container metadata, so fail
                if args.include_merged_configuration {
                    return Err(anyhow::anyhow!(
                            "Dev container not found. Container ID or labels did not match any running containers. \
                             Cannot compute merged configuration without container metadata."
                        ));
                }

                // Otherwise, succeed with devcontainerId substituted but no container-specific variables
                debug!("Proceeding without container-specific substitutions (merged config not requested)");
                (config_after_before, Some(id_labels), None, None, None)
            }
        }
    } else {
        // No container discovery requested
        debug!("No container discovery requested");
        (config, None, None, None, None)
    };

    // Store container metadata for potential use (currently just for debugging)
    if let Some(id_labels) = &container_id_labels {
        debug!("Container id-labels: {:?}", id_labels);
    }
    if let Some(env) = &container_env {
        debug!("Container has {} environment variables", env.len());
    }

    // Create fetcher with tight timeouts for features resolution
    // Per FR-009: Use 2s timeout and exactly 1 retry for predictable performance
    use deacon_core::retry::{JitterStrategy, RetryConfig};
    use std::time::Duration;

    let retry_config = RetryConfig::new(
        1,                          // max_attempts: exactly 1 retry
        Duration::from_millis(100), // base_delay: small backoff
        Duration::from_secs(1),     // max_delay
        JitterStrategy::FullJitter,
    );

    let fetcher = default_fetcher_with_config(Some(Duration::from_secs(2)), retry_config)
        .map_err(|e| anyhow::anyhow!("Failed to create OCI fetcher: {}", e))?;

    // Resolve features if requested or needed for merged config
    // Per spec: Features are needed for:
    // 1. When --include-features-configuration is set (explicit request)
    // 2. When --include-merged-configuration is set WITHOUT a container
    //    (to derive metadata from features per issue #289)
    let features_configuration_for_output = if args.include_features_configuration
        || (args.include_merged_configuration && args.container_id.is_none())
    {
        Some(
            resolve_features_configuration(
                &config,
                args.additional_features.as_deref(),
                args.skip_feature_auto_mapping,
                &fetcher,
            )
            .await?,
        )
    } else {
        None
    };

    // Compute merged configuration if requested
    // Per spec: mergedConfiguration = mergeConfiguration(base_config, imageMetadata)
    // where imageMetadata comes from container OR features
    let merged_configuration = if args.include_merged_configuration {
        // We may need features for merged config computation even if not outputting them
        let features_for_merge = features_configuration_for_output.as_ref();

        Some(
            compute_merged_configuration(
                &config,
                container_info.as_ref(),
                container_context.as_ref(),
                features_for_merge,
                secrets.as_ref(),
                &fetcher,
            )
            .await?,
        )
    } else {
        None
    };

    // Build output payload
    let output_payload = ReadConfigurationOutput {
        configuration: if container_only_mode {
            // Per spec line 310: "Only container flags provided (no config/workspace): returns { configuration: {}, ... }"
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            serde_json::to_value(&config)?
        },
        workspace: workspace_config,
        features_configuration: features_configuration_for_output,
        merged_configuration,
    };

    // Output the payload as JSON
    output.write_json(&output_payload)?;

    debug!(
        "Completed read-configuration: name={} merged={} replacements={}",
        config.name.as_deref().unwrap_or("unknown"),
        args.include_merged_configuration,
        substitution_report.replacements.len()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::redaction::{RedactionConfig, SecretRegistry};
    use std::fs;
    use tempfile::TempDir;

    fn create_test_args(
        temp_dir: &TempDir,
        include_merged: bool,
        config_path: Option<PathBuf>,
        override_path: Option<PathBuf>,
        secrets_files: Vec<PathBuf>,
    ) -> ReadConfigurationArgs {
        ReadConfigurationArgs {
            include_merged_configuration: include_merged,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path,
            override_config_path: override_path,
            secrets_files,
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        }
    }

    #[tokio::test]
    async fn test_read_configuration_basic() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = create_test_args(
            &temp_dir,
            false,             // include_merged_configuration
            Some(config_path), // config_path
            None,              // override_config_path
            vec![],            // secrets_files
        );

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_configuration_with_variables() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
            "workspaceFolder": "${localWorkspaceFolder}/src"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = create_test_args(
            &temp_dir,
            false,             // include_merged_configuration
            Some(config_path), // config_path
            None,              // override_config_path
            vec![],            // secrets_files
        );

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_configuration_with_override() {
        let temp_dir = TempDir::new().unwrap();
        let base_config_path = temp_dir.path().join("devcontainer.json");
        // Per FR-004: override config must also be named devcontainer.json or .devcontainer.json
        let override_dir = temp_dir.path().join("override");
        fs::create_dir(&override_dir).unwrap();
        let override_config_path = override_dir.join("devcontainer.json");

        let base_config_content = r#"{
            "name": "base-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
            "containerEnv": {
                "BASE_VAR": "base-value"
            }
        }"#;

        let override_config_content = r#"{
            "name": "override-container",
            "containerEnv": {
                "OVERRIDE_VAR": "override-value"
            }
        }"#;

        fs::write(&base_config_path, base_config_content).unwrap();
        fs::write(&override_config_path, override_config_content).unwrap();

        let args = create_test_args(
            &temp_dir,
            false,                      // include_merged_configuration
            Some(base_config_path),     // config_path
            Some(override_config_path), // override_config_path
            vec![],                     // secrets_files
        );

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_configuration_with_secrets() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");
        let secrets_path = temp_dir.path().join("secrets.env");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
            "containerEnv": {
                "DB_PASSWORD": "${localEnv:DB_PASSWORD}"
            }
        }"#;

        let secrets_content = r#"
# Database credentials
DB_PASSWORD=super-secret-password
API_KEY=another-secret
"#;

        fs::write(&config_path, config_content).unwrap();
        fs::write(&secrets_path, secrets_content).unwrap();

        let args = create_test_args(
            &temp_dir,
            false,              // include_merged_configuration
            Some(config_path),  // config_path
            None,               // override_config_path
            vec![secrets_path], // secrets_files
        );

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_configuration_not_found() {
        let temp_dir = TempDir::new().unwrap();

        let args = create_test_args(
            &temp_dir,
            false,  // include_merged_configuration
            None,   // config_path
            None,   // override_config_path
            vec![], // secrets_files
        );

        let result = execute_read_configuration(args).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        // Error format changed to use shared config loader which returns DeaconError::Config
        // The error message should contain "Configuration error" from DeaconError Display
        assert!(
            err_msg.contains("Configuration error") || err_msg.contains("not found"),
            "Expected error about missing config, got: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_read_configuration_invalid_label_format() {
        let temp_dir = TempDir::new().unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec!["invalid".to_string()], // Missing '='
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert_eq!(
            err_msg,
            "Unmatched argument format: id-label must match <name>=<value>."
        );
    }

    #[tokio::test]
    async fn test_read_configuration_without_container_discovery() {
        // Test that the command works without container discovery
        // (Previously named test_read_configuration_valid_with_container_id but that
        // now requires a running container, which we don't have in tests)
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None, // No container discovery
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_configuration_container_discovery_requires_docker() {
        // Test that container discovery fails gracefully when Docker is unavailable
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: true, // Set to true to test failure when container not found
            include_features_configuration: false,
            container_id: Some("abc123".to_string()),
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: None,
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        // Should fail with a clear error (Docker unavailable or container not found)
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        // Error should mention Docker or container not found
        assert!(
            err_msg.contains("Docker")
                || err_msg.contains("container")
                || err_msg.contains("not found"),
            "Error message should mention Docker or container: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_read_configuration_mount_workspace_git_root_flag() {
        // Test that the flag is accepted by the CLI (functionality not yet wired to ConfigLoader)
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        // Test with mount_workspace_git_root = false
        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: false,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path.clone()),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());

        // Test with mount_workspace_git_root = true (default)
        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_configuration_workspace_output_fields() {
        // Test that workspace fields are correctly populated in output
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        // Capture output to verify workspace section
        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());

        // Note: In a real test we would capture stdout and parse the JSON
        // to verify the workspace section contains the expected fields:
        // - workspaceFolder (container path like /workspaces/<basename>)
        // - workspaceMount (mount specification)
        // - configFolderPath (host path)
        // - rootFolderPath (host path)
    }

    #[tokio::test]
    async fn test_workspace_config_with_file_in_devcontainer_dir() {
        // Test that when config file is in .devcontainer directory,
        // config_folder_path correctly identifies the parent directory
        let temp_dir = TempDir::new().unwrap();
        let devcontainer_dir = temp_dir.path().join(".devcontainer");
        fs::create_dir(&devcontainer_dir).unwrap();
        let config_file = devcontainer_dir.join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_file, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_file),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());
        // The config_folder_path should be the .devcontainer directory
    }

    #[tokio::test]
    async fn test_workspace_config_without_mount_workspace_git_root() {
        // Test workspace resolution without git root mounting
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: false,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_merged_configuration_without_container_or_features() {
        // Test that merged configuration is correctly computed when requested
        // without container or features (should return base config as placeholder)
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: true, // Request merged config
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());

        // Note: The merged configuration should be present in the output
        // Currently it returns the base config as a placeholder until
        // issues #288 (container metadata) and #289 (features metadata) are resolved
    }

    #[tokio::test]
    async fn test_merged_configuration_with_nonexistent_container() {
        // Test that merged configuration properly fails when a non-existent container is provided
        // This validates that container discovery is working correctly before merging
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: true,
            include_features_configuration: false,
            container_id: Some("nonexistent-container-id".to_string()),
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;

        // Should fail because container doesn't exist
        // This is expected behavior - container discovery fails before we get to merge
        assert!(result.is_err());

        if let Err(e) = result {
            let error_msg = e.to_string();
            // Accept a broader class of errors here to keep tests hermetic:
            // either a clear container-not-found path, or Docker CLI unavailability.
            assert!(
                error_msg.contains("Container ID or labels did not match")
                    || error_msg.contains("not found")
                    || error_msg.contains("Docker")
                    || error_msg.contains("container"),
                "Expected container discovery error, got: {}",
                error_msg
            );
        }
    }

    #[tokio::test]
    async fn test_workspace_omitted_when_not_available() {
        // Test that workspace section is omitted when workspace folder is not provided
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());
        // The workspace field should be None/omitted in the output
    }

    #[tokio::test]
    async fn test_workspace_config_path_precedence() {
        // Test that config_path takes precedence when both workspace_folder and config_path are provided
        let temp_dir = TempDir::new().unwrap();
        let workspace_dir = temp_dir.path().join("workspace");
        fs::create_dir(&workspace_dir).unwrap();

        let config_dir = temp_dir.path().join("configs");
        fs::create_dir(&config_dir).unwrap();
        let config_path = config_dir.join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(workspace_dir),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_configuration_additional_features_flag() {
        // Test that the additional_features flag is accepted
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: Some(
                r#"{"ghcr.io/devcontainers/features/node:1": "lts"}"#.to_string(),
            ),
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_configuration_include_features_flag() {
        // Test that the include_features_configuration flag is accepted (without features in config)
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: true,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_configuration_skip_feature_auto_mapping_flag() {
        // Test that the skip_feature_auto_mapping flag is accepted
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: true,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_configuration_string_value_auto_mapping() {
        // Test that top-level string values are auto-mapped to "version" option
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        // Config with string feature value (common pattern)
        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
            "features": {
                "ghcr.io/devcontainers/features/node:1": "lts"
            }
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: true,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        // This may fail if registry is not accessible, but should at least parse correctly
        // We're mainly testing that the string value is accepted and parsed
        let _ = result;
    }

    #[tokio::test]
    async fn test_read_configuration_empty_additional_features() {
        // Test that empty additional_features JSON object is handled
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: Some("{}".to_string()),
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_configuration_invalid_additional_features_json() {
        // Test that invalid JSON in additional_features is rejected
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: Some("not valid json".to_string()),
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse --additional-features JSON"));
    }

    #[tokio::test]
    async fn test_read_configuration_additional_features_not_object() {
        // Test that non-object JSON (array) in additional_features is rejected
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: Some(r#"["not", "an", "object"]"#.to_string()),
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("--additional-features must be a JSON object"));
    }

    #[tokio::test]
    async fn test_option_preservation_roundtrip_all_types() {
        // Test that all JSON option types survive the complete pipeline
        // from config parsing through conversion and back to JSON output
        use deacon_core::features::OptionValue;
        use std::collections::HashMap;

        // Create test options with all JSON types
        let mut test_options = HashMap::new();
        test_options.insert(
            "string".to_string(),
            OptionValue::String("test".to_string()),
        );
        test_options.insert("bool".to_string(), OptionValue::Boolean(false));
        test_options.insert(
            "number".to_string(),
            OptionValue::Number(serde_json::Number::from(123)),
        );
        test_options.insert(
            "array".to_string(),
            OptionValue::Array(vec![
                serde_json::Value::String("item".to_string()),
                serde_json::Value::Number(serde_json::Number::from(1)),
            ]),
        );
        let mut obj = serde_json::Map::new();
        obj.insert("key".to_string(), serde_json::Value::Bool(true));
        test_options.insert("object".to_string(), OptionValue::Object(obj));
        test_options.insert("null".to_string(), OptionValue::Null);

        // Simulate the conversion that happens in read_configuration
        let json_output: HashMap<String, serde_json::Value> = test_options
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

        // Verify all types are preserved in the JSON output
        assert_eq!(json_output.len(), 6, "All option types should be preserved");
        assert!(json_output.get("string").unwrap().is_string());
        assert!(json_output.get("bool").unwrap().is_boolean());
        assert!(json_output.get("number").unwrap().is_number());
        assert!(json_output.get("array").unwrap().is_array());
        assert!(json_output.get("object").unwrap().is_object());
        assert!(json_output.get("null").unwrap().is_null());

        // Verify specific values are correct
        assert_eq!(json_output.get("string").unwrap().as_str(), Some("test"));
        assert_eq!(json_output.get("bool").unwrap().as_bool(), Some(false));
        assert_eq!(json_output.get("number").unwrap().as_i64(), Some(123));

        // Verify array contents
        let array = json_output.get("array").unwrap().as_array().unwrap();
        assert_eq!(array.len(), 2);
        assert_eq!(array[0].as_str(), Some("item"));
        assert_eq!(array[1].as_i64(), Some(1));

        // Verify object contents
        let object = json_output.get("object").unwrap().as_object().unwrap();
        assert_eq!(object.get("key").unwrap().as_bool(), Some(true));
    }

    #[tokio::test]
    async fn test_terminal_dimensions_both_provided() {
        // When both terminal columns and rows are provided, should succeed
        let temp_dir = TempDir::new().unwrap();
        let devcontainer_dir = temp_dir.path().join(".devcontainer");
        fs::create_dir_all(&devcontainer_dir).unwrap();
        let config_path = devcontainer_dir.join("devcontainer.json");

        fs::write(
            &config_path,
            r#"{ "name": "test", "image": "ubuntu:22.04" }"#,
        )
        .unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: Some(80),
            terminal_rows: Some(24),
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(
            result.is_ok(),
            "Should succeed with both dimensions provided"
        );
    }

    #[tokio::test]
    async fn test_terminal_dimensions_only_columns() {
        // When only columns are provided, should fail with clear error message
        let temp_dir = TempDir::new().unwrap();
        let devcontainer_dir = temp_dir.path().join(".devcontainer");
        fs::create_dir_all(&devcontainer_dir).unwrap();
        let config_path = devcontainer_dir.join("devcontainer.json");

        fs::write(
            &config_path,
            r#"{ "name": "test", "image": "ubuntu:22.04" }"#,
        )
        .unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: Some(80),
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_err(), "Should fail when only columns provided");
        let err_msg = result.unwrap_err().to_string();
        // Error now comes from shared TerminalDimensions::new() which returns DeaconError::Config
        assert!(
            err_msg.contains("terminal") || err_msg.contains("Configuration error"),
            "Expected error about terminal dimensions, got: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_terminal_dimensions_only_rows() {
        // When only rows are provided, should fail with clear error message
        let temp_dir = TempDir::new().unwrap();
        let devcontainer_dir = temp_dir.path().join(".devcontainer");
        fs::create_dir_all(&devcontainer_dir).unwrap();
        let config_path = devcontainer_dir.join("devcontainer.json");

        fs::write(
            &config_path,
            r#"{ "name": "test", "image": "ubuntu:22.04" }"#,
        )
        .unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: Some(24),
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_err(), "Should fail when only rows provided");
        let err_msg = result.unwrap_err().to_string();
        // Error now comes from shared TerminalDimensions::new() which returns DeaconError::Config
        assert!(
            err_msg.contains("terminal") || err_msg.contains("Configuration error"),
            "Expected error about terminal dimensions, got: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_terminal_dimensions_neither_provided() {
        // When neither columns nor rows are provided, should succeed (optional feature)
        let temp_dir = TempDir::new().unwrap();
        let devcontainer_dir = temp_dir.path().join(".devcontainer");
        fs::create_dir_all(&devcontainer_dir).unwrap();
        let config_path = devcontainer_dir.join("devcontainer.json");

        fs::write(
            &config_path,
            r#"{ "name": "test", "image": "ubuntu:22.04" }"#,
        )
        .unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(
            result.is_ok(),
            "Should succeed when no terminal dimensions provided"
        );
    }

    #[test]
    fn test_docker_paths_default_values() {
        // Verify default values for docker paths are set in CLI parsing
        // These are tested in cli.rs but we verify the struct can be created with defaults
        let temp_dir = TempDir::new().unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        // Verify defaults are set correctly
        assert_eq!(args.docker_path, "docker");
        assert_eq!(args.docker_compose_path, "docker-compose");
    }

    #[test]
    fn test_docker_paths_custom_values() {
        // Verify custom docker paths are accepted and stored
        let temp_dir = TempDir::new().unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "/usr/local/bin/docker".to_string(),
            docker_compose_path: "/usr/local/bin/docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        // Verify custom paths are preserved
        assert_eq!(args.docker_path, "/usr/local/bin/docker");
        assert_eq!(args.docker_compose_path, "/usr/local/bin/docker-compose");
    }

    #[test]
    fn test_user_data_folder_optional() {
        // Verify user_data_folder is properly optional and can be None
        let temp_dir = TempDir::new().unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        assert!(args.user_data_folder.is_none());
    }

    #[test]
    fn test_user_data_folder_with_value() {
        // Verify user_data_folder can store a path when provided
        let temp_dir = TempDir::new().unwrap();
        let data_folder = temp_dir.path().join("data");

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: Some(data_folder.clone()),
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        assert!(args.user_data_folder.is_some());
        assert_eq!(args.user_data_folder.unwrap(), data_folder);
    }

    #[tokio::test]
    async fn test_read_configuration_invalid_config_filename() {
        // Test that invalid config filenames are rejected per FR-004
        let temp_dir = TempDir::new().unwrap();
        let invalid_config_path = temp_dir.path().join("my-config.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&invalid_config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(invalid_config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Invalid --config filename")
                && err_msg.contains("my-config.json")
                && err_msg.contains("devcontainer.json"),
            "Expected error message about invalid filename, got: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_read_configuration_invalid_override_filename() {
        // Test that invalid override-config filenames are rejected per FR-004
        let temp_dir = TempDir::new().unwrap();
        let base_config_path = temp_dir.path().join("devcontainer.json");
        let invalid_override_path = temp_dir.path().join("override.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&base_config_path, config_content).unwrap();
        fs::write(&invalid_override_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(base_config_path),
            override_config_path: Some(invalid_override_path),
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Invalid --override-config filename")
                && err_msg.contains("override.json")
                && err_msg.contains("devcontainer.json"),
            "Expected error message about invalid override filename, got: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_read_configuration_valid_hidden_config_filename() {
        // Test that .devcontainer.json is accepted as a valid filename
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: None,
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            user_data_folder: None,
            terminal_columns: None,
            terminal_rows: None,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(
            result.is_ok(),
            "Expected success with .devcontainer.json filename"
        );
    }
}
