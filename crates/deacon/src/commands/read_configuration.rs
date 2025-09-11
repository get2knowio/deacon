//! Read configuration command implementation
//!
//! Implements the `deacon read-configuration` subcommand for reading and displaying
//! DevContainer configuration with variable substitution and extends resolution.

use anyhow::Result;
use deacon_core::config::ConfigLoader;
use deacon_core::variable::SubstitutionContext;
use deacon_core::errors::{DeaconError, ConfigError};
use std::path::{Path, PathBuf};
use tracing::{debug, info, instrument};
use serde_json;

/// Read configuration command arguments
#[derive(Debug, Clone)]
pub struct ReadConfigurationArgs {
    pub include_merged_configuration: bool,
    pub workspace_folder: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
}

/// Execute the read-configuration command
#[instrument(skip(args))]
pub async fn execute_read_configuration(args: ReadConfigurationArgs) -> Result<()> {
    info!("Starting read-configuration command execution");
    debug!("Read configuration args: {:?}", args);

    // Determine workspace folder
    let workspace_folder = args.workspace_folder.as_deref().unwrap_or(Path::new("."));

    // Load configuration
    let config = if let Some(config_path) = args.config_path.as_ref() {
        if args.include_merged_configuration {
            ConfigLoader::load_with_extends(config_path)?
        } else {
            ConfigLoader::load_from_path(config_path)?
        }
    } else {
        // Discover configuration
        let config_location = ConfigLoader::discover_config(workspace_folder)?;
        if !config_location.exists() {
            return Err(
                DeaconError::Config(ConfigError::NotFound {
                    path: config_location.path().to_string_lossy().to_string(),
                })
                .into(),
            );
        }
        
        if args.include_merged_configuration {
            ConfigLoader::load_with_extends(config_location.path())?
        } else {
            ConfigLoader::load_from_path(config_location.path())?
        }
    };

    debug!("Loaded configuration: {:?}", config.name);

    // Apply variable substitution (phase 1)
    let substitution_context = SubstitutionContext::new(workspace_folder)?;
    let (substituted_config, substitution_report) = config.apply_variable_substitution(&substitution_context);

    debug!("Applied variable substitution: {} replacements made", substitution_report.replacements.len());

    // Output the configuration as JSON
    let json_output = serde_json::to_string_pretty(&substituted_config)?;
    println!("{}", json_output);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

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
        };

        let result = execute_read_configuration(args).await;
        assert!(result.is_err());
    }
}