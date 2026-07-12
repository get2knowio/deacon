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
    /// REPLACE base (`--override-config`): when set, THIS file (resolved through
    /// its own `extends` chain) becomes the base config instead of discovery or
    /// `--config`. Reference parity (#285). The merge ladder still overlays on top.
    pub override_config_path: Option<&'a Path>,
    /// Settings-sourced merge fragments (root `mergeConfig` then the selected
    /// profile's, resolved by the profile helper), lowest→highest precedence.
    /// Deep-overlaid on the base. Empty ⇒ today's behavior.
    pub settings_merge_paths: &'a [PathBuf],
    /// CLI `--merge-config` fragments, in given order (later wins), the
    /// highest-precedence deep-overlay layer.
    pub cli_merge_paths: &'a [PathBuf],
    /// Secrets file paths (--secrets-file)
    pub secrets_files: &'a [PathBuf],
    /// Whether `${devcontainerId}` should be resolved during load-time
    /// substitution. Runtime commands (`up`/`exec`/`build`/…) pass `true`;
    /// `read-configuration` passes `false` so the token stays literal in its
    /// pre-container output, matching the reference CLI.
    pub resolve_devcontainer_id: bool,
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
/// Base selection (the config the merge ladder overlays onto):
/// - `--override-config` when provided (REPLACE base; resolved through its own
///   `extends` chain — reference parity #285).
/// - Otherwise `--config` when provided.
/// - Otherwise discover under `workspace_folder` (or current dir) via
///   `ConfigLoader::discover_config`.
///
/// The merge ladder (low→high) is the settings/profile `mergeConfig` fragments
/// then the CLI `--merge-config` fragments, deep-overlaid on the base.
///
/// Secrets from `secrets_files` are threaded into substitution. Errors are surfaced
/// as `DeaconError::Config` variants to preserve upstream JSON contracts.
pub async fn load_config(args: ConfigLoadArgs<'_>) -> Result<ConfigLoadResult> {
    let workspace_folder = if let Some(folder) = args.workspace_folder {
        folder.to_path_buf()
    } else {
        std::env::current_dir().map_err(|e| DeaconError::Config(ConfigError::Io(e)))?
    };

    // Base config: `--override-config` replaces discovery/`--config` outright
    // (its own extends chain runs in the loader). Otherwise `--config`, else
    // discovery. A merge fragment is never promoted to base — merge needs a base.
    let config_path = if let Some(override_path) = args.override_config_path {
        override_path.to_path_buf()
    } else if let Some(path) = args.config_path {
        // Spec parity (#65): accept any --config filename; the loader will
        // surface the usual file-not-found error if the path does not exist.
        path.to_path_buf()
    } else {
        match ConfigLoader::discover_config(&workspace_folder).await? {
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

    // Assemble the ordered merge chain (low→high): settings-sourced fragments
    // (root mergeConfig then selected profile) then the CLI `--merge-config`.
    let mut merge_paths: Vec<PathBuf> = args
        .settings_merge_paths
        .iter()
        .map(|p| p.to_path_buf())
        .collect();
    merge_paths.extend(args.cli_merge_paths.iter().map(|p| p.to_path_buf()));

    let secrets = if args.secrets_files.is_empty() {
        None
    } else {
        Some(SecretsCollection::load_from_files(args.secrets_files)?)
    };

    let merge_refs: Vec<&Path> = merge_paths.iter().map(|p| p.as_path()).collect();
    let (config, substitution_report) = ConfigLoader::load_with_overrides_and_substitution(
        &config_path,
        &merge_refs,
        secrets.as_ref(),
        &workspace_folder,
        args.resolve_devcontainer_id,
    )
    .await?;

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

    #[tokio::test]
    async fn override_replaces_base() {
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
            settings_merge_paths: &[],
            cli_merge_paths: &[],
            override_config_path: Some(override_path.as_path()),
            secrets_files: &[],
            resolve_devcontainer_id: true,
        })
        .await
        .unwrap();

        assert_eq!(result.config.name.as_deref(), Some("override-only"));
        assert_eq!(result.config_path, override_path);
    }

    /// Reference parity (#285): `--override-config` REPLACES the discovered base
    /// — the discovered config's fields must NOT survive into the result.
    #[tokio::test]
    async fn override_config_replaces_not_merges_discovered_base() {
        let temp = TempDir::new().unwrap();
        // A real discovered base config with a field the override does not set.
        let dc_dir = temp.path().join(".devcontainer");
        std::fs::create_dir_all(&dc_dir).unwrap();
        std::fs::write(
            dc_dir.join("devcontainer.json"),
            r#"{"name":"discovered","remoteUser":"baseuser","image":"ubuntu:latest"}"#,
        )
        .unwrap();
        // The override file sets only `name` (+ image); no remoteUser.
        let override_path = temp.path().join("override.json");
        std::fs::write(
            &override_path,
            r#"{"name":"from-override","image":"debian:bookworm-slim"}"#,
        )
        .unwrap();

        let result = load_config(ConfigLoadArgs {
            workspace_folder: Some(temp.path()),
            config_path: None,
            settings_merge_paths: &[],
            cli_merge_paths: &[],
            override_config_path: Some(override_path.as_path()),
            secrets_files: &[],
            resolve_devcontainer_id: true,
        })
        .await
        .unwrap();

        assert_eq!(result.config.name.as_deref(), Some("from-override"));
        // The discovered base's remoteUser must be GONE — replace, not merge.
        assert_eq!(result.config.remote_user, None);
        assert_eq!(result.config_path, override_path);
    }

    /// The merge ladder still overlays on top of an `--override-config` base
    /// (composable, user-approved): `--merge-config` wins on conflicts.
    #[tokio::test]
    async fn merge_config_overlays_on_override_base() {
        let temp = TempDir::new().unwrap();
        let override_path = temp.path().join("base.json");
        std::fs::write(
            &override_path,
            r#"{"name":"base","remoteUser":"baseuser","image":"ubuntu:latest"}"#,
        )
        .unwrap();
        let fragment = temp.path().join("frag.json");
        std::fs::write(&fragment, r#"{"name":"fragment-wins"}"#).unwrap();

        let result = load_config(ConfigLoadArgs {
            workspace_folder: Some(temp.path()),
            config_path: None,
            settings_merge_paths: &[],
            cli_merge_paths: std::slice::from_ref(&fragment),
            override_config_path: Some(override_path.as_path()),
            secrets_files: &[],
            resolve_devcontainer_id: true,
        })
        .await
        .unwrap();

        // Fragment overlays the override base: name from fragment, remoteUser
        // inherited from the base (fragment did not set it).
        assert_eq!(result.config.name.as_deref(), Some("fragment-wins"));
        assert_eq!(result.config.remote_user.as_deref(), Some("baseuser"));
    }

    #[tokio::test]
    async fn surfaces_not_found_error() {
        let temp = TempDir::new().unwrap();

        let err = load_config(ConfigLoadArgs {
            workspace_folder: Some(temp.path()),
            config_path: None,
            settings_merge_paths: &[],
            cli_merge_paths: &[],
            override_config_path: None,
            secrets_files: &[],
            resolve_devcontainer_id: true,
        })
        .await
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

    #[tokio::test]
    async fn config_path_bypasses_discovery() {
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
            settings_merge_paths: &[],
            cli_merge_paths: &[],
            override_config_path: None,
            secrets_files: &[],
            resolve_devcontainer_id: true,
        })
        .await
        .unwrap();

        // The explicitly provided config should be used
        assert_eq!(result.config.name.as_deref(), Some("explicit"));
        assert_eq!(result.config_path, config_path);
    }

    #[tokio::test]
    async fn config_path_to_named_config_works_with_multiple_named_configs() {
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
            settings_merge_paths: &[],
            cli_merge_paths: &[],
            override_config_path: None,
            secrets_files: &[],
            resolve_devcontainer_id: true,
        })
        .await
        .unwrap();

        // Should use the explicitly specified rust config without error
        assert_eq!(result.config.name.as_deref(), Some("rust"));
        assert_eq!(result.config_path, explicit_config);
    }

    #[tokio::test]
    async fn config_path_nonexistent_returns_error() {
        // --config to non-existent file returns appropriate error
        let temp = TempDir::new().unwrap();
        let nonexistent_path = temp.path().join("devcontainer.json");
        // Do NOT create the file

        let err = load_config(ConfigLoadArgs {
            workspace_folder: Some(temp.path()),
            config_path: Some(nonexistent_path.as_path()),
            settings_merge_paths: &[],
            cli_merge_paths: &[],
            override_config_path: None,
            secrets_files: &[],
            resolve_devcontainer_id: true,
        })
        .await
        .unwrap_err();

        // Should return a not-found or io error, not panic
        match err {
            DeaconError::Config(_) => {} // Expected: some config error
            other => panic!("Expected config error, got: {:?}", other),
        }
    }
}
