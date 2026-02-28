//! Shared configuration loading helpers for CLI commands.
//!
//! Centralizes workspace/config/override resolution and secrets handling so
//! all subcommands share the same error mapping and substitution behavior.

use deacon_core::config::{ConfigLoader, DevContainerConfig, DiscoveryResult};
use deacon_core::errors::{ConfigError, DeaconError, Result};
use deacon_core::secrets::SecretsCollection;
use deacon_core::variable::SubstitutionReport;
use std::path::{Path, PathBuf};

/// Inputs for configuration loading.
pub struct ConfigLoadArgs<'a> {
    /// Optional workspace folder (defaults to current directory)
    pub workspace_folder: Option<&'a Path>,
    /// Explicit config path (--config)
    pub config_path: Option<&'a Path>,
    /// Override config path (--override-config)
    pub override_config_path: Option<&'a Path>,
    /// Secrets file paths (--secrets-file)
    pub secrets_files: &'a [PathBuf],
}

/// Loaded configuration and supporting context.
#[derive(Debug)]
pub struct ConfigLoadResult {
    pub config: DevContainerConfig,
    #[allow(dead_code)]
    pub substitution_report: SubstitutionReport,
    pub workspace_folder: PathBuf,
    #[allow(dead_code)]
    pub config_path: PathBuf,
}

/// Resolve and load configuration using shared discovery rules.
///
/// Resolution order:
/// - Use `config_path` when provided.
/// - Otherwise discover under `workspace_folder` (or current dir) via `ConfigLoader::discover_config`.
/// - If the discovered path does not exist and an override is provided, treat the override as the base config.
///
/// Secrets from `secrets_files` are threaded into substitution. Errors are surfaced
/// as `DeaconError::Config` variants to preserve upstream JSON contracts.
pub fn load_config(args: ConfigLoadArgs<'_>) -> Result<ConfigLoadResult> {
    let workspace_folder = if let Some(folder) = args.workspace_folder {
        folder.to_path_buf()
    } else {
        std::env::current_dir().map_err(|e| DeaconError::Config(ConfigError::Io(e)))?
    };

    let mut config_path = if let Some(path) = args.config_path {
        // Validate filename per spec: must be devcontainer.json(c) or .devcontainer.json(c)
        // The .jsonc extension is allowed for JSON with comments
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            let valid_names = [
                "devcontainer.json",
                "devcontainer.jsonc",
                ".devcontainer.json",
                ".devcontainer.jsonc",
            ];
            if !valid_names.contains(&file_name) {
                return Err(DeaconError::Config(ConfigError::Validation {
                    message: "Filename must be devcontainer.json(c) or .devcontainer.json(c)"
                        .to_string(),
                }));
            }
        }
        path.to_path_buf()
    } else {
        match ConfigLoader::discover_config(&workspace_folder)? {
            DiscoveryResult::Single(path) => path,
            DiscoveryResult::Multiple(paths) => {
                let display_paths: Vec<String> = paths
                    .iter()
                    .map(|p| {
                        p.strip_prefix(&workspace_folder)
                            .unwrap_or(p)
                            .to_string_lossy()
                            .to_string()
                    })
                    .collect();
                return Err(DeaconError::Config(ConfigError::MultipleConfigs {
                    paths: display_paths,
                }));
            }
            DiscoveryResult::None(default) => default,
        }
    };

    let mut override_config_path = args.override_config_path.map(|p| p.to_path_buf());

    // When the discovered/base config is missing, fall back to using the override as the base.
    if !config_path.exists() {
        if let Some(override_path) = override_config_path.take() {
            config_path = override_path;
        }
    }

    let secrets = if args.secrets_files.is_empty() {
        None
    } else {
        Some(SecretsCollection::load_from_files(args.secrets_files)?)
    };

    let (config, substitution_report) = ConfigLoader::load_with_overrides_and_substitution(
        &config_path,
        override_config_path.as_deref(),
        secrets.as_ref(),
        &workspace_folder,
    )?;

    Ok(ConfigLoadResult {
        config,
        substitution_report,
        workspace_folder,
        config_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::errors::{ConfigError, DeaconError};
    use tempfile::TempDir;

    #[test]
    fn uses_override_when_base_missing() {
        let temp = TempDir::new().unwrap();
        let override_path = temp.path().join(".devcontainer.json");
        std::fs::write(
            &override_path,
            r#"{"name":"override-only","image":"ubuntu:latest"}"#,
        )
        .unwrap();

        let result = load_config(ConfigLoadArgs {
            workspace_folder: Some(temp.path()),
            config_path: None,
            override_config_path: Some(override_path.as_path()),
            secrets_files: &[],
        })
        .unwrap();

        assert_eq!(result.config.name.as_deref(), Some("override-only"));
        assert_eq!(result.config_path, override_path);
    }

    #[test]
    fn surfaces_not_found_error() {
        let temp = TempDir::new().unwrap();

        let err = load_config(ConfigLoadArgs {
            workspace_folder: Some(temp.path()),
            config_path: None,
            override_config_path: None,
            secrets_files: &[],
        })
        .unwrap_err();

        match err {
            DeaconError::Config(ConfigError::NotFound { path }) => {
                // Use Path for cross-platform path comparison
                let path = std::path::Path::new(&path);
                assert!(
                    path.ends_with(".devcontainer/devcontainer.json")
                        || path.ends_with(r".devcontainer\devcontainer.json"),
                    "unexpected path: {path:?}"
                );
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn config_path_bypasses_discovery() {
        // When --config is provided, discover_config() is skipped
        // and the specific path is used directly
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("devcontainer.json");
        std::fs::write(
            &config_path,
            r#"{"name":"explicit","image":"ubuntu:latest"}"#,
        )
        .unwrap();

        let result = load_config(ConfigLoadArgs {
            workspace_folder: Some(temp.path()),
            config_path: Some(config_path.as_path()),
            override_config_path: None,
            secrets_files: &[],
        })
        .unwrap();

        // The explicitly provided config should be used
        assert_eq!(result.config.name.as_deref(), Some("explicit"));
        assert_eq!(result.config_path, config_path);
    }

    #[test]
    fn config_path_to_named_config_works_with_multiple_named_configs() {
        // --config to a specific named config works even when multiple named configs exist
        let temp = TempDir::new().unwrap();
        let workspace = temp.path();

        // Create multiple named configs
        for name in ["node", "python", "rust"] {
            let subdir = workspace.join(".devcontainer").join(name);
            std::fs::create_dir_all(&subdir).unwrap();
            std::fs::write(
                subdir.join("devcontainer.json"),
                format!(r#"{{"name":"{}","image":"ubuntu:latest"}}"#, name),
            )
            .unwrap();
        }

        // Use --config to explicitly select rust config
        let explicit_config = workspace
            .join(".devcontainer")
            .join("rust")
            .join("devcontainer.json");

        let result = load_config(ConfigLoadArgs {
            workspace_folder: Some(workspace),
            config_path: Some(explicit_config.as_path()),
            override_config_path: None,
            secrets_files: &[],
        })
        .unwrap();

        // Should use the explicitly specified rust config without error
        assert_eq!(result.config.name.as_deref(), Some("rust"));
        assert_eq!(result.config_path, explicit_config);
    }

    #[test]
    fn config_path_nonexistent_returns_error() {
        // --config to non-existent file returns appropriate error
        let temp = TempDir::new().unwrap();
        let nonexistent_path = temp.path().join("devcontainer.json");
        // Do NOT create the file

        let err = load_config(ConfigLoadArgs {
            workspace_folder: Some(temp.path()),
            config_path: Some(nonexistent_path.as_path()),
            override_config_path: None,
            secrets_files: &[],
        })
        .unwrap_err();

        // Should return a not-found or io error, not panic
        match err {
            DeaconError::Config(_) => {} // Expected: some config error
            other => panic!("Expected config error, got: {:?}", other),
        }
    }
}
