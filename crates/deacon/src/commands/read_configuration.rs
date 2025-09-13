//! Read configuration command implementation
//!
//! Implements the `deacon read-configuration` subcommand for reading and displaying
//! DevContainer configuration with variable substitution and extends resolution.

use anyhow::Result;
use deacon_core::config::ConfigLoader;
use deacon_core::errors::{ConfigError, DeaconError};
use deacon_core::secrets::SecretsCollection;
use deacon_core::variable::SubstitutionContext;
use serde_json;
use std::path::{Path, PathBuf};
use tracing::{debug, info, instrument};

/// Read configuration command arguments
#[derive(Debug, Clone)]
pub struct ReadConfigurationArgs {
    pub include_merged_configuration: bool,
    pub workspace_folder: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub override_config_path: Option<PathBuf>,
    pub secrets_files: Vec<PathBuf>,
}

/// Execute the read-configuration command
#[instrument(skip(args))]
pub async fn execute_read_configuration(args: ReadConfigurationArgs) -> Result<()> {
    info!("Starting read-configuration command execution");
    debug!("Read configuration args: {:?}", args);

    // Determine workspace folder
    let workspace_folder = args.workspace_folder.as_deref().unwrap_or(Path::new("."));

    // Load secrets if provided
    let secrets = if !args.secrets_files.is_empty() {
        Some(SecretsCollection::load_from_files(&args.secrets_files)?)
    } else {
        None
    };

    // Load configuration with override and secrets support
    let (config, substitution_report) = if let Some(config_path) = args.config_path.as_ref() {
        if args.include_merged_configuration {
            ConfigLoader::load_with_overrides_and_substitution(
                config_path,
                args.override_config_path.as_deref(),
                secrets.as_ref(),
                workspace_folder,
            )?
        } else {
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
        }
    } else {
        // Discover configuration
        let config_location = ConfigLoader::discover_config(workspace_folder)?;
        if !config_location.exists() {
            return Err(DeaconError::Config(ConfigError::NotFound {
                path: config_location.path().to_string_lossy().to_string(),
            })
            .into());
        }

        if args.include_merged_configuration {
            ConfigLoader::load_with_overrides_and_substitution(
                config_location.path(),
                args.override_config_path.as_deref(),
                secrets.as_ref(),
                workspace_folder,
            )?
        } else {
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
        }
    };

    debug!("Loaded configuration: {:?}", config.name);
    debug!(
        "Applied variable substitution: {} replacements made",
        substitution_report.replacements.len()
    );

    // Output the configuration as JSON
    let json_output = serde_json::to_string_pretty(&config)?;
    println!("{}", json_output);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_read_configuration_basic() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
        };

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

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
        };

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

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(base_config_path),
            override_config_path: Some(override_config_path),
            secrets_files: vec![],
        };

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

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![secrets_path],
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_configuration_not_found() {
        let temp_dir = TempDir::new().unwrap();

        let args = ReadConfigurationArgs {
            include_merged_configuration: false,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: None,
            override_config_path: None,
            secrets_files: vec![],
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_err());
    }
}
