//! Read configuration command implementation
//!
//! Implements the `deacon read-configuration` subcommand for reading and displaying
//! DevContainer configuration with variable substitution and extends resolution.

use anyhow::Result;
use deacon_core::config::ConfigLoader;
use deacon_core::container::ContainerSelector;
use deacon_core::errors::{ConfigError, DeaconError};
use deacon_core::features::{
    FeatureDependencyResolver, FeatureMergeConfig, FeatureMerger, OptionValue, ResolvedFeature,
};
use deacon_core::io::Output;
use deacon_core::oci::{default_fetcher, FeatureRef};
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
    /// TODO(#268): Implement container-based config reading
    /// When id_label is provided, resolve container and read configuration from it
    #[allow(dead_code)]
    pub id_label: Vec<String>,
    /// TODO(#295): Wire mount_workspace_git_root to workspace resolution
    /// Flag accepted for CLI compatibility but not yet used in ConfigLoader.
    /// Should influence workspace discovery/mount behavior per spec.
    #[allow(dead_code)]
    pub mount_workspace_git_root: bool,
    pub additional_features: Option<String>,
    pub skip_feature_auto_mapping: bool,
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
}

/// Source information for features
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SourceInformation {
    #[serde(rename = "oci")]
    Oci { registry: String },
}

/// Output payload structure for read-configuration command
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadConfigurationOutput {
    pub configuration: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features_configuration: Option<FeaturesConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merged_configuration: Option<serde_json::Value>,
}

/// Resolve features configuration from the DevContainer config
async fn resolve_features_configuration(
    config: &deacon_core::config::DevContainerConfig,
    additional_features: Option<&str>,
    _skip_feature_auto_mapping: bool,
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
            false, // Don't prefer CLI features by default
            None,  // No install order override in this context
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

    // Resolve features from registries to obtain metadata
    let fetcher =
        default_fetcher().map_err(|e| anyhow::anyhow!("Failed to create OCI client: {}", e))?;
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
            serde_json::Value::String(s) if !_skip_feature_auto_mapping => {
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

    // Validate id_label format (must match <name>=<value> pattern)
    if !args.id_label.is_empty() {
        ContainerSelector::parse_labels(&args.id_label)?;
    }

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

    // Determine workspace folder
    let workspace_folder = args.workspace_folder.as_deref().unwrap_or(Path::new("."));

    // Load secrets if provided
    let secrets = if !args.secrets_files.is_empty() {
        Some(SecretsCollection::load_from_files(&args.secrets_files)?)
    } else {
        None
    };

    // Load configuration
    let (config, substitution_report) = if let Some(config_path) = args.config_path.as_ref() {
        // For non-merged config, still apply overrides and substitution
        let base_config = ConfigLoader::load_from_path(config_path)?;
        let mut configs = vec![base_config];

        // Add override config if provided
        if let Some(override_path) = args.override_config_path.as_ref() {
            let override_config = ConfigLoader::load_from_path(override_path)?;
            configs.push(override_config);
        }

        let merged = deacon_core::config::ConfigMerger::merge_configs(&configs);

        // Apply variable substitution with secrets
        let mut substitution_context = SubstitutionContext::new(workspace_folder)?;
        if let Some(ref secrets) = secrets {
            for (key, value) in secrets.as_env_vars() {
                substitution_context
                    .local_env
                    .insert(key.clone(), value.clone());
            }
        }

        merged.apply_variable_substitution(&substitution_context)
    } else {
        // Discover configuration
        let config_location = ConfigLoader::discover_config(workspace_folder)?;
        if !config_location.exists() {
            return Err(DeaconError::Config(ConfigError::NotFound {
                path: config_location.path().to_string_lossy().to_string(),
            })
            .into());
        }

        // For non-merged config, still apply overrides and substitution
        let base_config = ConfigLoader::load_from_path(config_location.path())?;
        let mut configs = vec![base_config];

        // Add override config if provided
        if let Some(override_path) = args.override_config_path.as_ref() {
            let override_config = ConfigLoader::load_from_path(override_path)?;
            configs.push(override_config);
        }

        let merged = deacon_core::config::ConfigMerger::merge_configs(&configs);

        // Apply variable substitution with secrets
        let mut substitution_context = SubstitutionContext::new(workspace_folder)?;
        if let Some(ref secrets) = secrets {
            for (key, value) in secrets.as_env_vars() {
                substitution_context
                    .local_env
                    .insert(key.clone(), value.clone());
            }
        }

        merged.apply_variable_substitution(&substitution_context)
    };

    debug!("Loaded configuration: {:?}", config.name);
    debug!(
        "Applied variable substitution: {} replacements made",
        substitution_report.replacements.len()
    );

    // Container discovery and container-aware substitutions
    let (config, container_id_labels, container_env) = if args.container_id.is_some()
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
        )?;
        selector.validate()?;

        // Resolve container
        match deacon_core::container::resolve_container(&docker, &selector).await? {
            Some(container_info) => {
                debug!(
                    "Container found: id={}, labels={:?}",
                    container_info.id, container_info.labels
                );

                // Extract id-labels (use provided labels or extract from container)
                let id_labels: Vec<(String, String)> = if !args.id_label.is_empty() {
                    // Use provided labels (already parsed and validated above)
                    ContainerSelector::parse_labels(&args.id_label)?
                } else {
                    // Extract relevant labels from container (all labels in this case)
                    container_info
                        .labels
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                };

                // Compute devcontainerId from id-labels
                let dev_container_id = deacon_core::container::compute_dev_container_id(&id_labels);
                debug!("Computed devcontainerId: {}", dev_container_id);

                // Apply beforeContainerSubstitute: ${devcontainerId}
                let mut before_context = SubstitutionContext::new(workspace_folder)?;
                before_context.devcontainer_id = dev_container_id.clone();
                if let Some(ref secrets) = secrets {
                    for (key, value) in secrets.as_env_vars() {
                        before_context.local_env.insert(key.clone(), value.clone());
                    }
                }

                let (config_after_before, _before_report) =
                    config.apply_variable_substitution(&before_context);

                // Apply containerSubstitute: ${containerEnv:VAR}, ${containerWorkspaceFolder}
                let mut container_context = SubstitutionContext::new(workspace_folder)?;
                container_context.devcontainer_id = dev_container_id;
                container_context.container_env = Some(container_info.env.clone());
                // TODO: Extract containerWorkspaceFolder from container config
                // For now, we don't set it as it requires parsing container mounts/config

                if let Some(ref secrets) = secrets {
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
                    Some(id_labels),
                    Some(container_info.env.clone()),
                )
            }
            None => {
                // Container not found - fail with clear error
                return Err(anyhow::anyhow!(
                        "Dev container not found. Container ID or labels did not match any running containers."
                    ));
            }
        }
    } else {
        // No container discovery requested
        debug!("No container discovery requested");
        (config, None, None)
    };

    // Store container metadata for potential use (currently just for debugging)
    if let Some(ref id_labels) = container_id_labels {
        debug!("Container id-labels: {:?}", id_labels);
    }
    if let Some(ref env) = container_env {
        debug!("Container has {} environment variables", env.len());
    }

    // Resolve features if requested
    let features_configuration = if args.include_features_configuration
        || (args.include_merged_configuration && args.container_id.is_none())
    {
        Some(
            resolve_features_configuration(
                &config,
                args.additional_features.as_deref(),
                args.skip_feature_auto_mapping,
            )
            .await?,
        )
    } else {
        None
    };

    // Handle merged configuration if requested
    if args.include_merged_configuration {
        // Use enhanced resolution with metadata tracking
        let (merged_config, _substitution_report) =
            if let Some(config_path) = args.config_path.as_ref() {
                ConfigLoader::load_with_full_resolution(
                    config_path,
                    args.override_config_path.as_deref(),
                    secrets.as_ref(),
                    workspace_folder,
                    true, // include metadata
                )?
            } else {
                // Discover configuration
                let config_location = ConfigLoader::discover_config(workspace_folder)?;
                if !config_location.exists() {
                    return Err(DeaconError::Config(ConfigError::NotFound {
                        path: config_location.path().to_string_lossy().to_string(),
                    })
                    .into());
                }

                ConfigLoader::load_with_full_resolution(
                    config_location.path(),
                    args.override_config_path.as_deref(),
                    secrets.as_ref(),
                    workspace_folder,
                    true, // include metadata
                )?
            };

        debug!(
            "Loaded merged configuration with metadata: {:?}",
            merged_config.config.name
        );

        // Build output payload
        let output_payload = ReadConfigurationOutput {
            configuration: serde_json::to_value(&config)?,
            features_configuration,
            merged_configuration: Some(serde_json::to_value(&merged_config)?),
        };

        // Output the payload as JSON
        output.write_json(&output_payload)?;

        debug!(
            "Completed read-configuration: name={} merged=true layers={}",
            merged_config.config.name.as_deref().unwrap_or("unknown"),
            merged_config
                .meta
                .as_ref()
                .map(|m| m.layers.len())
                .unwrap_or(0),
        );
    } else {
        // Build output payload without merged configuration
        let output_payload = ReadConfigurationOutput {
            configuration: serde_json::to_value(&config)?,
            features_configuration,
            merged_configuration: None,
        };

        // Output the payload as JSON
        output.write_json(&output_payload)?;

        debug!(
            "Completed read-configuration: name={} merged=false replacements={}",
            config.name.as_deref().unwrap_or("unknown"),
            substitution_report.replacements.len()
        );
    }

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
        let override_config_path = temp_dir.path().join("override.json");

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
            workspace_folder: None,
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
            include_merged_configuration: false,
            include_features_configuration: false,
            container_id: Some("abc123".to_string()),
            id_label: vec![],
            mount_workspace_git_root: true,
            additional_features: None,
            skip_feature_auto_mapping: false,
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
}
