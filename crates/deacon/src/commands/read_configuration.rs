//! Read configuration command implementation
//!
//! Implements the `deacon read-configuration` subcommand for reading and displaying
//! DevContainer configuration with variable substitution and extends resolution.

use anyhow::Result;
use deacon_core::config::ConfigLoader;
use deacon_core::errors::{ConfigError, DeaconError};
use deacon_core::io::Output;
use deacon_core::redaction::{RedactionConfig, SecretRegistry};
use deacon_core::secrets::SecretsCollection;
use deacon_core::variable::SubstitutionContext;
use std::path::{Path, PathBuf};
use tracing::{debug, instrument};

/// Read configuration command arguments
#[derive(Debug, Clone)]
pub struct ReadConfigurationArgs {
    pub include_merged_configuration: bool,
    pub workspace_folder: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub override_config_path: Option<PathBuf>,
    pub secrets_files: Vec<PathBuf>,
    pub redaction_config: RedactionConfig,
    pub secret_registry: SecretRegistry,
}

/// Execute the read-configuration command
#[instrument(skip(args))]
pub async fn execute_read_configuration(args: ReadConfigurationArgs) -> Result<()> {
    // Keep startup message at debug to avoid noisy INFO output for simple queries
    debug!("Starting read-configuration command execution");
    debug!("Read configuration args: {:?}", args);

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

    if args.include_merged_configuration {
        // Use enhanced resolution with metadata tracking
        let (merged_config, substitution_report) =
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
        debug!(
            "Applied variable substitution: {} replacements made",
            substitution_report.replacements.len()
        );

        // Output the merged configuration with metadata as JSON
        output.write_json(&merged_config)?;

        // Single concise completion info line (keep info noise low)
        debug!(
            "Completed read-configuration: name={} merged=true layers={} replacements={}",
            merged_config.config.name.as_deref().unwrap_or("unknown"),
            merged_config
                .meta
                .as_ref()
                .map(|m| m.layers.len())
                .unwrap_or(0),
            substitution_report.replacements.len()
        );
    } else {
        // Use standard resolution without metadata
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

        // Output the configuration as JSON
        output.write_json(&config)?;

        // Single concise completion info line (keep info noise low)
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
            false,                              // include_merged_configuration
            Some(config_path),                  // config_path
            None,                               // override_config_path
            vec![],                             // secrets_files
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
            false,                              // include_merged_configuration
            Some(config_path),                  // config_path
            None,                               // override_config_path
            vec![],                             // secrets_files
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
            false,                              // include_merged_configuration
            Some(base_config_path),             // config_path
            Some(override_config_path),         // override_config_path
            vec![],                             // secrets_files
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
            false,                              // include_merged_configuration
            Some(config_path),                  // config_path
            None,                               // override_config_path
            vec![secrets_path],                 // secrets_files
        );

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_configuration_not_found() {
        let temp_dir = TempDir::new().unwrap();

        let args = create_test_args(
            &temp_dir,
            false,                              // include_merged_configuration
            None,                               // config_path
            None,                               // override_config_path
            vec![],                             // secrets_files
        );

        let result = execute_read_configuration(args).await;
        assert!(result.is_err());
    }
}
