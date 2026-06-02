//! Shared build configuration resolution for devcontainer Dockerfile builds.

use anyhow::Result;
use deacon_core::config::DevContainerConfig;
use deacon_core::errors::{ConfigError, DeaconError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Build configuration resolved from a devcontainer configuration.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedBuildConfig {
    pub dockerfile: String,
    pub dockerfile_path: PathBuf,
    pub context: String,
    pub context_folder: PathBuf,
    pub target: Option<String>,
    pub options: HashMap<String, String>,
}

/// Resolve a devcontainer Dockerfile build configuration.
///
/// Dev Container `dockerFile` and `build.dockerfile` paths are resolved
/// relative to the directory containing the active config file. The build
/// context is also interpreted relative to that config directory by callers.
pub(crate) fn resolve_devcontainer_build_config(
    config: &DevContainerConfig,
    config_path: &Path,
) -> Result<Option<ResolvedBuildConfig>> {
    if config.image.is_some() {
        return Ok(None);
    }

    let config_folder = config_path.parent().unwrap_or_else(|| Path::new("."));
    let mut context = ".".to_string();
    let mut target = None;
    let mut options = HashMap::new();

    let build_dockerfile = if let Some(build_value) = &config.build {
        let build_obj = build_value.as_object().ok_or_else(|| {
            DeaconError::Config(ConfigError::Validation {
                message: "build field must be an object".to_string(),
            })
        })?;

        if let Some(build_context) = build_obj.get("context").and_then(|v| v.as_str()) {
            context = build_context.to_string();
        }

        if let Some(build_target) = build_obj.get("target").and_then(|v| v.as_str()) {
            target = Some(build_target.to_string());
        }

        if let Some(options_obj) = build_obj.get("options").and_then(|v| v.as_object()) {
            insert_stringified_values(&mut options, options_obj);
        }

        if let Some(args_obj) = build_obj.get("args").and_then(|v| v.as_object()) {
            insert_stringified_values(&mut options, args_obj);
        }

        build_obj
            .get("dockerfile")
            .and_then(|v| v.as_str())
            .map(str::to_string)
    } else {
        None
    };

    let dockerfile = config.dockerfile.clone().or(build_dockerfile);

    match dockerfile {
        Some(dockerfile) => {
            let dockerfile_path = config_folder.join(&dockerfile);
            if !dockerfile_path.exists() {
                return Err(DeaconError::Config(ConfigError::NotFound {
                    path: dockerfile_path.to_string_lossy().to_string(),
                })
                .into());
            }

            Ok(Some(ResolvedBuildConfig {
                dockerfile,
                dockerfile_path,
                context,
                context_folder: config_folder.to_path_buf(),
                target,
                options,
            }))
        }
        None if config.build.is_some() => Err(DeaconError::Config(ConfigError::Validation {
            message: "build.dockerfile is required when using build object".to_string(),
        })
        .into()),
        None => Ok(None),
    }
}

fn insert_stringified_values(
    options: &mut HashMap<String, String>,
    values: &serde_json::Map<String, serde_json::Value>,
) {
    for (key, value) in values {
        let val_str = value
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| value.to_string());
        options.insert(key.clone(), val_str);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn resolves_top_level_dockerfile_relative_to_config_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_dir = temp_dir.path().join(".devcontainer");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(config_dir.join("Dockerfile"), "FROM alpine:3.19\n").unwrap();
        std::fs::write(temp_dir.path().join("Dockerfile"), "FROM busybox:1.36\n").unwrap();
        let config_path = config_dir.join("devcontainer.json");

        let mut config = DevContainerConfig::default();
        config.dockerfile = Some("Dockerfile".to_string());

        let resolved = resolve_devcontainer_build_config(&config, &config_path)
            .unwrap()
            .unwrap();

        assert_eq!(resolved.dockerfile, "Dockerfile");
        assert_eq!(resolved.dockerfile_path, config_dir.join("Dockerfile"));
        assert_eq!(resolved.context_folder, config_dir);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn resolves_build_object_fields() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_dir = temp_dir.path().join(".devcontainer");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(config_dir.join("Containerfile"), "FROM alpine:3.19\n").unwrap();
        let config_path = config_dir.join("devcontainer.json");

        let mut config = DevContainerConfig::default();
        config.build = Some(serde_json::json!({
            "dockerfile": "Containerfile",
            "context": "..",
            "target": "dev",
            "args": { "FOO": "bar" },
            "options": { "BUILDKIT_INLINE_CACHE": "1" }
        }));

        let resolved = resolve_devcontainer_build_config(&config, &config_path)
            .unwrap()
            .unwrap();

        assert_eq!(resolved.dockerfile_path, config_dir.join("Containerfile"));
        assert_eq!(resolved.context, "..");
        assert_eq!(resolved.target, Some("dev".to_string()));
        assert_eq!(resolved.options.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(
            resolved.options.get("BUILDKIT_INLINE_CACHE"),
            Some(&"1".to_string())
        );
    }
}
