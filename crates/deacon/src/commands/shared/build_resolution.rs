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

/// Resolve the external base image a Dockerfile's target stage ultimately
/// derives from, following multi-stage `FROM … AS <name>` chains and applying
/// global `ARG` defaults (`build.args` override them). Returns `None` when the
/// base cannot be statically determined (`FROM scratch`, an unresolved `ARG`,
/// or no `FROM` at all).
///
/// Used by `read-configuration --include-merged-configuration` to locate the
/// image whose `devcontainer.metadata` label seeds `mergedConfiguration` for
/// Dockerfile-based configs — matching the reference CLI, which reads the base
/// image's baked-in metadata (e.g. `mcr.microsoft.com/devcontainers/base`'s
/// `git` customizations) even before the image is built.
pub(crate) fn resolve_dockerfile_base_image(
    content: &str,
    build_args: &HashMap<String, String>,
    target: Option<&str>,
) -> Option<String> {
    // Global ARGs (declared before the first FROM) are the only ones usable in
    // FROM lines. Seed defaults from the Dockerfile; `build.args` override them.
    let mut global_args: HashMap<String, String> = HashMap::new();
    // (lowercased stage name, raw FROM reference) in declaration order.
    let mut stages: Vec<(Option<String>, String)> = Vec::new();
    let mut seen_from = false;

    for logical in join_continuations(content) {
        let line = logical.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.split_whitespace();
        let Some(instr) = it.next() else { continue };
        match instr.to_ascii_uppercase().as_str() {
            "ARG" if !seen_from => {
                if let Some(decl) = it.next() {
                    if let Some((name, default)) = decl.split_once('=') {
                        global_args.insert(name.to_string(), default.to_string());
                    }
                }
            }
            "FROM" => {
                seen_from = true;
                let Some(from_ref) = it.next() else { continue };
                // Optional `AS <name>` (case-insensitive keyword).
                let mut name = None;
                if let Some(kw) = it.next() {
                    if kw.eq_ignore_ascii_case("AS") {
                        name = it.next().map(|n| n.to_ascii_lowercase());
                    }
                }
                stages.push((name, from_ref.to_string()));
            }
            _ => {}
        }
    }

    if stages.is_empty() {
        return None;
    }

    // Pick the target stage (by name, case-insensitive) or the last stage.
    let start = match target {
        Some(t) => {
            let t = t.to_ascii_lowercase();
            stages
                .iter()
                .position(|(n, _)| n.as_deref() == Some(t.as_str()))
                .unwrap_or(stages.len() - 1)
        }
        None => stages.len() - 1,
    };

    // Follow the FROM chain until it points at an external image rather than an
    // earlier stage name. Bounded by the stage count to avoid cycles.
    let mut idx = start;
    for _ in 0..stages.len() {
        let resolved = substitute_dockerfile_args(&stages[idx].1, &global_args, build_args);
        if let Some(prev) = stages
            .iter()
            .position(|(n, _)| n.as_deref() == Some(resolved.to_ascii_lowercase().as_str()))
        {
            idx = prev;
            continue;
        }
        if resolved.eq_ignore_ascii_case("scratch") {
            return None;
        }
        return Some(resolved);
    }
    None
}

/// Join Dockerfile line continuations (`\` at end of line) into logical lines.
fn join_continuations(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut acc = String::new();
    for line in content.lines() {
        let trimmed_end = line.trim_end();
        if let Some(prefix) = trimmed_end.strip_suffix('\\') {
            acc.push_str(prefix);
            acc.push(' ');
        } else {
            acc.push_str(line);
            out.push(std::mem::take(&mut acc));
        }
    }
    if !acc.is_empty() {
        out.push(acc);
    }
    out
}

/// Substitute `${VAR}` / `$VAR` in a Dockerfile FROM reference using build args
/// (highest precedence) then global ARG defaults. Unresolved refs are left as-is.
fn substitute_dockerfile_args(
    input: &str,
    global_args: &HashMap<String, String>,
    build_args: &HashMap<String, String>,
) -> String {
    let lookup = |name: &str| -> Option<String> {
        build_args
            .get(name)
            .or_else(|| global_args.get(name))
            .cloned()
    };
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'{' {
                // ${VAR} (tolerate ${VAR:-default} / ${VAR:+x} by taking the name up to ':' or '}')
                if let Some(end) = input[i + 2..].find('}') {
                    let raw = &input[i + 2..i + 2 + end];
                    let name = raw.split_once(':').map(|(n, _)| n).unwrap_or(raw);
                    if let Some(v) = lookup(name) {
                        out.push_str(&v);
                    }
                    i += 2 + end + 1;
                    continue;
                }
            } else {
                // $VAR — name is [A-Za-z0-9_]+
                let start = i + 1;
                let mut j = start;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                if j > start {
                    if let Some(v) = lookup(&input[start..j]) {
                        out.push_str(&v);
                    }
                    i = j;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
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

    #[test]
    fn base_image_follows_multistage_and_arg_default() {
        // Mirrors fixtures/parity-corpus/dockerfile-build/.devcontainer/Dockerfile:
        // target `dev` FROMs `base`, which FROMs an ARG-parameterized external image.
        let content = "\
ARG VARIANT=bookworm
FROM mcr.microsoft.com/devcontainers/base:${VARIANT} AS base
ARG NODE_VERSION=20
RUN echo hi
FROM base AS dev
RUN echo dev
";
        let base = resolve_dockerfile_base_image(content, &HashMap::new(), Some("dev"));
        assert_eq!(
            base.as_deref(),
            Some("mcr.microsoft.com/devcontainers/base:bookworm")
        );
    }

    #[test]
    fn base_image_build_arg_overrides_default() {
        let content = "ARG VARIANT=bookworm\nFROM debian:${VARIANT}\n";
        let mut args = HashMap::new();
        args.insert("VARIANT".to_string(), "bullseye".to_string());
        assert_eq!(
            resolve_dockerfile_base_image(content, &args, None).as_deref(),
            Some("debian:bullseye")
        );
    }

    #[test]
    fn base_image_defaults_to_last_stage_without_target() {
        let content = "FROM alpine:3.19 AS build\nFROM ubuntu:22.04 AS run\n";
        assert_eq!(
            resolve_dockerfile_base_image(content, &HashMap::new(), None).as_deref(),
            Some("ubuntu:22.04")
        );
    }

    #[test]
    fn base_image_scratch_and_empty_are_none() {
        assert_eq!(
            resolve_dockerfile_base_image("FROM scratch\n", &HashMap::new(), None),
            None
        );
        assert_eq!(
            resolve_dockerfile_base_image("RUN echo hi\n", &HashMap::new(), None),
            None
        );
    }
}
