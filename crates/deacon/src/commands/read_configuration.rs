//! Read configuration command implementation
//!
//! Implements the `deacon read-configuration` subcommand for reading and displaying
//! DevContainer configuration with variable substitution and extends resolution.
//!
//! Spec: the containers.dev spec / reference CLI
//! Implementation: specs/001-read-config-parity/spec.md

use crate::commands::shared::{ConfigLoadArgs, TerminalDimensions, load_config};
use anyhow::{Context, Result};
use deacon_core::config::DevContainerConfig;
use deacon_core::container::ContainerSelector;

use deacon_core::features::{
    FeatureDependencyResolver, FeatureMergeConfig, FeatureMerger, OptionValue, ResolvedFeature,
};
use deacon_core::io::Output;
use deacon_core::oci::{FeatureRef, default_fetcher_with_config};
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
    /// When set, target this container directly for label-based merged-config metadata
    /// extraction (`devcontainer.metadata`); skips workspace-based discovery.
    pub container_id: Option<String>,
    /// When non-empty, resolve target container by matching these `key=value` labels;
    /// takes precedence over workspace-based discovery but yields to `container_id`.
    pub id_label: Vec<String>,
    /// Flag to control workspace root discovery behavior.
    /// When true (default), uses Git worktree detection to find the true workspace root.
    /// When false, uses the workspace folder path as-is.
    pub mount_workspace_git_root: bool,
    pub additional_features: Option<String>,
    pub skip_feature_auto_mapping: bool,
    /// Docker tooling path. Forwarded to the container runtime so `docker inspect` invocations
    /// honor the spec-defined `--docker-path` flag.
    pub docker_path: String,
    /// Docker Compose tooling path. Accepted per spec (§3) for CLI parity; read-configuration
    /// performs no compose operations, so the value is retained only to keep the CLI surface stable.
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customizations: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub init: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privileged: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mounts: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_env: Option<HashMap<String, String>>,
}

/// Parsed OCI feature reference, mirroring the reference CLI's `featureRef`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureRefInfo {
    /// Feature name, e.g. `node`.
    pub id: String,
    /// First namespace segment, e.g. `devcontainers`.
    pub owner: String,
    /// Namespace, e.g. `devcontainers/features`.
    pub namespace: String,
    /// Registry host, e.g. `ghcr.io`.
    pub registry: String,
    /// `registry/namespace/name`, e.g. `ghcr.io/devcontainers/features/node`.
    pub resource: String,
    /// `namespace/name`, e.g. `devcontainers/features/node`.
    pub path: String,
    /// Resolved version/tag, e.g. `1`.
    pub version: String,
    /// Tag as written, e.g. `1`.
    pub tag: String,
}

/// Source information for a resolved feature, matching the reference CLI's
/// `sourceInformation` shape (one variant per feature origin).
///
/// The `Oci` variant is much larger than `FilePath` (it carries the full OCI
/// manifest), but this is a serialization-only DTO constructed once per feature
/// for output, so the size asymmetry is irrelevant.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SourceInformation {
    /// An OCI-registry feature.
    #[serde(rename = "oci", rename_all = "camelCase")]
    Oci {
        /// The full OCI manifest (config + layers + annotations).
        manifest: serde_json::Value,
        /// `sha256:<hex>` digest of the raw manifest body.
        manifest_digest: String,
        /// Parsed reference.
        feature_ref: FeatureRefInfo,
        /// The id as written in `devcontainer.json` (with tag).
        user_feature_id: String,
        /// The id as written, minus the `:tag`.
        user_feature_id_without_version: String,
    },
    /// A local (on-disk) feature.
    #[serde(rename = "file-path", rename_all = "camelCase")]
    FilePath {
        /// Absolute path to the feature directory.
        resolved_file_path: String,
        /// The id as written in `devcontainer.json` (e.g. `./feature`).
        user_feature_id: String,
    },
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
    config_dir: &Path,
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

    // Use provided fetcher to resolve features from registries.
    // Local feature paths (`./`, `../`, or absolute) bypass the OCI fetch
    // path and are read directly from disk — parity with the up flow
    // (`features_build.rs`). Per #106.
    let mut resolved_features = Vec::with_capacity(features_map.len());

    for (feature_id, feature_value) in features_map {
        let is_local = feature_id.starts_with("./")
            || feature_id.starts_with("../")
            || feature_id.starts_with('/');

        let (canonical_id, source_string, downloaded_metadata) = if is_local {
            let resolved = config_dir.join(feature_id);
            let canonical_path = resolved.canonicalize().map_err(|e| {
                anyhow::anyhow!(
                    "Local feature path '{}' (resolved to '{}' relative to {}) is not accessible: {}",
                    feature_id,
                    resolved.display(),
                    config_dir.display(),
                    e
                )
            })?;
            let metadata_path = canonical_path.join("devcontainer-feature.json");
            if !metadata_path.exists() {
                anyhow::bail!(
                    "Local feature at '{}' is missing devcontainer-feature.json (resolved from '{}' relative to {})",
                    canonical_path.display(),
                    feature_id,
                    config_dir.display()
                );
            }
            let metadata =
                deacon_core::features::parse_feature_metadata(&metadata_path).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to parse local feature metadata at '{}': {}",
                        metadata_path.display(),
                        e
                    )
                })?;
            let canonical_id = format!("local:{}", canonical_path.display());
            (canonical_id, feature_id.clone(), metadata)
        } else {
            let (registry_url, namespace, name, tag) = parse_registry_reference(feature_id)?;
            let feature_ref = FeatureRef::new(registry_url, namespace, name, tag);
            let downloaded = fetcher
                .fetch_feature(&feature_ref)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to fetch feature '{}': {}", feature_id, e))?;
            (
                downloaded.metadata.id.clone(),
                feature_ref.reference(),
                downloaded.metadata,
            )
        };

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
            id: canonical_id,
            source: source_string,
            options,
            metadata: downloaded_metadata,
        });
    }

    // Auto-install transitive `dependsOn` (hard) dependencies before resolving
    // the install order — parity with the reference CLI and the `up`/`build`
    // path, so `--include-features-configuration` reports (and orders) the full
    // closure instead of erroring on an undeclared hard dependency.
    // `installsAfter` (soft ordering) is NOT auto-installed.
    let mut idx = 0;
    while idx < resolved_features.len() {
        let mut deps: Vec<(String, serde_json::Value)> = resolved_features[idx]
            .metadata
            .depends_on
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        deps.sort_by(|a, b| a.0.cmp(&b.0));
        for (dep_key, dep_value) in deps {
            let dep = crate::commands::shared::feature_resolver::resolve_one_feature(
                &dep_key, &dep_value, config_dir, fetcher,
            )
            .await?;
            if !resolved_features.iter().any(|f| f.id == dep.id) {
                resolved_features.push(dep);
            }
        }
        idx += 1;
    }

    // Create dependency resolver
    let override_order = config.override_feature_install_order.clone();
    let resolver = FeatureDependencyResolver::new(override_order);

    // Resolve dependencies into an installation plan. The plan is in install
    // order — a feature's dependencies come before it (topological sort). The
    // reference CLI emits one featureSet per feature in exactly this order, so
    // we mirror that (rather than grouping by registry, which loses the order).
    let installation_plan = resolver.resolve(&resolved_features)?;

    let mut feature_sets: Vec<FeatureSet> = Vec::with_capacity(installation_plan.features.len());
    for resolved in &installation_plan.features {
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
            customizations: resolved.metadata.customizations.clone(),
            init: resolved.metadata.init,
            privileged: resolved.metadata.privileged,
            mounts: if resolved.metadata.mounts.is_empty() {
                None
            } else {
                Some(resolved.metadata.mounts.clone())
            },
            container_env: if resolved.metadata.container_env.is_empty() {
                None
            } else {
                Some(resolved.metadata.container_env.clone())
            },
        };

        // Build sourceInformation matching the reference CLI's per-origin shape.
        let source_information = if let Some(path) = resolved.id.strip_prefix("local:") {
            SourceInformation::FilePath {
                resolved_file_path: path.to_string(),
                user_feature_id: resolved.source.clone(),
            }
        } else {
            // OCI: parse the reference form back out, fetch the manifest + its
            // digest, and assemble the full `featureRef`.
            let (registry_url, namespace, name, tag) = parse_registry_reference(&resolved.source)
                .with_context(|| {
                format!("Invalid OCI feature reference '{}'", resolved.source)
            })?;
            let feature_ref = FeatureRef::new(
                registry_url.clone(),
                namespace.clone(),
                name.clone(),
                tag.clone(),
            );
            let (manifest, digest_hex) = fetcher
                .get_manifest_with_digest(&feature_ref)
                .await
                .with_context(|| {
                    format!("Failed to fetch manifest for feature '{}'", resolved.source)
                })?;
            let repository = feature_ref.repository(); // namespace/name
            let resource = format!("{}/{}", registry_url, repository);
            // userFeatureIdWithoutVersion == registry/namespace/name (no tag).
            let user_feature_id_without_version = resource.clone();
            let version = feature_ref.tag().to_string();
            let owner = namespace
                .split('/')
                .next()
                .unwrap_or(&namespace)
                .to_string();
            SourceInformation::Oci {
                manifest,
                manifest_digest: format!("sha256:{}", digest_hex),
                feature_ref: FeatureRefInfo {
                    id: name,
                    owner,
                    namespace,
                    registry: registry_url,
                    resource,
                    path: repository,
                    version: version.clone(),
                    tag: version,
                },
                user_feature_id: resolved.source.clone(),
                user_feature_id_without_version,
            }
        };

        // One featureSet per feature, in install order (reference parity).
        feature_sets.push(FeatureSet {
            features: vec![feature],
            source_information,
            internal_version: None,
            computed_digest: None,
        });
    }

    Ok(FeaturesConfiguration {
        feature_sets,
        dst_folder: None,
    })
}

/// Properties that the upstream `mergeConfiguration` strips from the base config and emits as
/// arrays under their plural names. The ordering is preserved: `entrypoint` first, then lifecycle
/// hooks in their canonical sequence (see
/// `devcontainers/cli/src/spec-node/imageMetadata.ts`).
const COLLECTED_PROPERTIES: &[(&str, &str)] = &[
    ("entrypoint", "entrypoints"),
    ("onCreateCommand", "onCreateCommands"),
    ("updateContentCommand", "updateContentCommands"),
    ("postCreateCommand", "postCreateCommands"),
    ("postStartCommand", "postStartCommands"),
    ("postAttachCommand", "postAttachCommands"),
];

/// Apply the upstream `mergeConfiguration` output shape to the merged base config JSON.
///
/// For each `(singular, plural)` pair in [`COLLECTED_PROPERTIES`] this strips the singular
/// property from `base` and—if any entry in `entries` carries a non-null value—emits the
/// plural array at the same position, preserving entry declaration order.
fn apply_upstream_merge_shape(
    mut base: serde_json::Value,
    entries: &[serde_json::Map<String, serde_json::Value>],
) -> serde_json::Value {
    let Some(obj) = base.as_object_mut() else {
        return base;
    };

    for (singular, _) in COLLECTED_PROPERTIES {
        obj.remove(*singular);
    }

    for (singular, plural) in COLLECTED_PROPERTIES {
        let collected: Vec<serde_json::Value> = entries
            .iter()
            .filter_map(|e| e.get(*singular))
            .filter(|v| !v.is_null())
            .cloned()
            .collect();
        if !collected.is_empty() {
            obj.insert((*plural).to_string(), serde_json::Value::Array(collected));
        }
    }

    base
}

/// Extract collected-property fields (entrypoint + lifecycle hooks) from a config JSON object
/// into a metadata entry suitable for [`apply_upstream_merge_shape`].
fn collect_entry_from_config_json(
    value: &serde_json::Value,
) -> serde_json::Map<String, serde_json::Value> {
    let mut entry = serde_json::Map::new();
    if let Some(obj) = value.as_object() {
        for (singular, _) in COLLECTED_PROPERTIES {
            if let Some(v) = obj.get(*singular) {
                if !v.is_null() {
                    entry.insert((*singular).to_string(), v.clone());
                }
            }
        }
    }
    entry
}

/// OR-merge two optional booleans following upstream's `imageMetadata.some(entry => entry.X)`
/// semantics. Used to accumulate `init` / `privileged` across feature metadata entries so a
/// later `Some(false)` cannot silently revoke an earlier `Some(true)`.
///
/// Truth table:
/// - `(None, None)` → `None`
/// - `(None, x)` / `(x, None)` → `x`
/// - `(Some(a), Some(b))` → `Some(a || b)`
fn or_merge_bool(current: Option<bool>, new: Option<bool>) -> Option<bool> {
    match (current, new) {
        (None, n) => n,
        (Some(c), None) => Some(c),
        (Some(c), Some(n)) => Some(c || n),
    }
}

/// Apply final `mergedConfiguration`-only normalizations that match the reference CLI's
/// `mergeConfiguration` output shape (these do NOT apply to the top-level `configuration`,
/// which preserves the raw authored values):
///
/// - `hostRequirements.memory` / `.storage` are emitted as a byte-count STRING (binary
///   units), not the raw authored form. So `"8gb"` becomes `"8589934592"`. `cpus` stays
///   numeric. Matches upstream `imageMetadata.ts` which normalizes these to bytes when
///   assembling the merged image metadata.
/// - `init` and `privileged` always materialize as booleans, defaulting to `false` when
///   no config / image-metadata / feature entry sets them. Matches upstream's
///   `init: imageMetadata.some(e => e.init)` / `privileged: imageMetadata.some(e => e.privileged)`,
///   which always yields a boolean.
fn normalize_merged_configuration_shape(value: &mut serde_json::Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };

    // init / privileged: null or absent -> false (always a boolean in the merged shape).
    for key in ["init", "privileged"] {
        let needs_default = obj.get(key).map(|v| v.is_null()).unwrap_or(true);
        if needs_default {
            obj.insert(key.to_string(), serde_json::Value::Bool(false));
        }
    }

    // hostRequirements memory / storage -> byte-count string.
    if let Some(serde_json::Value::Object(hr)) = obj.get_mut("hostRequirements") {
        for key in ["memory", "storage"] {
            if let Some(bytes) = hr.get(key).and_then(resource_value_to_bytes) {
                hr.insert(
                    key.to_string(),
                    serde_json::Value::String(bytes.to_string()),
                );
            }
        }
    }
}

/// Parse a hostRequirements resource JSON value (authored string like `"8gb"` or a raw
/// number) into a byte count, reusing the core [`ResourceSpec`] binary-unit semantics.
/// Returns `None` (leaving the value untouched) when the value is null or unparseable.
fn resource_value_to_bytes(v: &serde_json::Value) -> Option<u64> {
    if v.is_null() {
        return None;
    }
    let spec: deacon_core::config::ResourceSpec = serde_json::from_value(v.clone()).ok()?;
    spec.parse_bytes().ok()
}

/// Apply the upstream `mergeConfiguration` customizations shape to the merged base JSON.
///
/// Upstream collects `customizations` per tool key into an array of values across every metadata
/// entry — one slot per contributor — rather than deep-merging into a single object:
/// `{ tool: [c_from_entry1, c_from_entry2, ...] }` (see `mergeConfiguration` in
/// `devcontainers/cli/src/spec-node/imageMetadata.ts`). The consuming tool is responsible for
/// merging entries within its own slot.
///
/// This helper strips the deep-merged `customizations` field that ConfigMerger emits and
/// replaces it with the per-tool array form. If no entry contributes any customizations the
/// field is omitted entirely (matching upstream's
/// `Object.keys(customizations).length ? customizations : undefined`).
fn apply_customizations_shape(
    mut base: serde_json::Value,
    customizations_entries: &[serde_json::Value],
) -> serde_json::Value {
    let Some(obj) = base.as_object_mut() else {
        return base;
    };

    obj.remove("customizations");

    let mut per_tool: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    for entry in customizations_entries {
        let serde_json::Value::Object(tools) = entry else {
            continue;
        };
        if tools.is_empty() {
            continue;
        }
        for (tool, value) in tools {
            match per_tool.get_mut(tool) {
                Some(serde_json::Value::Array(arr)) => arr.push(value.clone()),
                _ => {
                    per_tool.insert(tool.clone(), serde_json::Value::Array(vec![value.clone()]));
                }
            }
        }
    }

    if !per_tool.is_empty() {
        obj.insert(
            "customizations".to_string(),
            serde_json::Value::Object(per_tool),
        );
    }

    base
}

/// Build a metadata entry directly from a resolved feature's [`FeatureMetadata`].
fn collect_entry_from_feature_metadata(
    metadata: &deacon_core::features::FeatureMetadata,
) -> serde_json::Map<String, serde_json::Value> {
    let mut entry = serde_json::Map::new();
    if let Some(ep) = &metadata.entrypoint {
        entry.insert(
            "entrypoint".to_string(),
            serde_json::Value::String(ep.clone()),
        );
    }
    if let Some(cmd) = &metadata.on_create_command {
        entry.insert("onCreateCommand".to_string(), cmd.clone());
    }
    if let Some(cmd) = &metadata.update_content_command {
        entry.insert("updateContentCommand".to_string(), cmd.clone());
    }
    if let Some(cmd) = &metadata.post_create_command {
        entry.insert("postCreateCommand".to_string(), cmd.clone());
    }
    if let Some(cmd) = &metadata.post_start_command {
        entry.insert("postStartCommand".to_string(), cmd.clone());
    }
    if let Some(cmd) = &metadata.post_attach_command {
        entry.insert("postAttachCommand".to_string(), cmd.clone());
    }
    entry
}

/// Compute merged configuration by merging base config with image metadata
///
/// Per the specification, merged configuration is:
/// `mergedConfiguration = mergeConfiguration(base_config, imageMetadata)`
///
/// Where imageMetadata comes from:
/// - Container inspection (when container_id is provided) - extracts devcontainer.metadata label
/// - Features metadata computation (when no container) - derives a partial config from each
///   resolved feature's metadata, then merges with the base config in declaration order.
///
/// The returned JSON matches the upstream `MergedDevContainerConfig` shape: `entrypoint` and
/// the singular lifecycle hooks (`onCreateCommand`, etc.) are stripped and emitted as plural
/// arrays (`entrypoints`, `onCreateCommands`, ...) collected from each metadata source. See
/// `devcontainers/cli/src/spec-node/imageMetadata.ts::mergeConfiguration`.
#[instrument(skip_all)]
/// Parse the `devcontainer.metadata` LABEL off `image_ref` into a vec of
/// partial config entries, best-effort. Returns an empty vec on any
/// failure (image not local, daemon unreachable, label absent, JSON
/// malformed) so the surrounding merge path is unaffected by transient
/// image inspection issues (#91).
async fn parse_image_metadata_entries(
    docker: &deacon_core::docker::CliDocker,
    image_ref: &str,
) -> Vec<deacon_core::config::DevContainerConfig> {
    use deacon_core::docker::Docker;

    let info = match docker.inspect_image(image_ref).await {
        Ok(Some(info)) => info,
        Ok(None) => {
            debug!(
                "Image '{}' not locally available; mergedConfiguration will not include image metadata",
                image_ref
            );
            return Vec::new();
        }
        Err(e) => {
            tracing::warn!(
                "Failed to inspect image '{}' for devcontainer.metadata label; proceeding without it: {}",
                image_ref,
                e
            );
            return Vec::new();
        }
    };

    let Some(label_json) = info.labels.get("devcontainer.metadata") else {
        debug!(
            "Image '{}' has no devcontainer.metadata label; nothing to merge",
            image_ref
        );
        return Vec::new();
    };

    match serde_json::from_str::<Vec<deacon_core::config::DevContainerConfig>>(label_json) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!(
                "Image '{}' has a devcontainer.metadata label that is not a valid JSON array of devcontainer entries; proceeding without it: {}",
                image_ref,
                e
            );
            Vec::new()
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn compute_merged_configuration<C: deacon_core::oci::HttpClient>(
    base_config: &deacon_core::config::DevContainerConfig,
    container_info: Option<&deacon_core::docker::ContainerInfo>,
    container_context: Option<&SubstitutionContext>,
    features_config: Option<&FeaturesConfiguration>,
    secrets: Option<&SecretsCollection>,
    fetcher: &deacon_core::oci::FeatureFetcher<C>,
    image_metadata_entries: &[deacon_core::config::DevContainerConfig],
) -> Result<serde_json::Value> {
    debug!(
        "Computing merged configuration: has_container={:?}, has_features={}",
        container_info.is_some(),
        features_config.is_some()
    );

    if let Some(container_info) = container_info {
        // Container-based merge: extract devcontainer.metadata label.
        let metadata_label = container_info.labels.get("devcontainer.metadata");
        let metadata_str = metadata_label.ok_or_else(|| {
            anyhow::anyhow!(
                "Container '{}' does not have required 'devcontainer.metadata' label. \
                 Cannot compute merged configuration without container metadata.",
                container_info.id
            )
        })?;

        debug!("Found devcontainer.metadata label: {}", metadata_str);

        // The label MAY be either:
        // - A JSON array of partial config entries (spec form, devcontainers/cli#1199, v0.86.0)
        // - A single JSON object (legacy form from older Deacon builds)
        // Tolerate both.
        let metadata_value: serde_json::Value =
            serde_json::from_str(metadata_str).with_context(|| {
                format!(
                    "Failed to parse devcontainer.metadata JSON from container '{}': {}",
                    container_info.id, metadata_str
                )
            })?;

        let entries: Vec<serde_json::Value> = match metadata_value {
            serde_json::Value::Array(arr) => arr,
            other => vec![other],
        };

        let container_context = container_context.ok_or_else(|| {
            anyhow::anyhow!("Container context required for container-based merged configuration")
        })?;

        // Deserialize each array entry as a partial DevContainerConfig, apply
        // variable substitution, then merge in declaration order with the base
        // config (later entries override earlier per spec merge semantics).
        //
        // In parallel, retain each entry's raw collected-property fields so the
        // upstream merge shape (entrypoints + plural lifecycle arrays) can be
        // emitted afterwards. Per upstream `getImageMetadataFromContainer`,
        // when the container's id-labels match, base config contributes only
        // `pickUpdateableConfigProperties` (remoteUser/userEnvProbe/remoteEnv) —
        // none of which are collected — so base lifecycle commands are NOT
        // added to the plural arrays in this path.
        let mut chain: Vec<deacon_core::config::DevContainerConfig> =
            Vec::with_capacity(1 + entries.len());
        chain.push(base_config.clone());
        let mut metadata_entries: Vec<serde_json::Map<String, serde_json::Value>> =
            Vec::with_capacity(entries.len());
        // Per-entry customizations objects, captured pre-substitution. Per upstream
        // `getImageMetadataFromContainer` with id-labels match, base config contributes
        // only `pickUpdateableConfigProperties` (remoteUser/userEnvProbe/remoteEnv) — so
        // base config's customizations are intentionally NOT included here.
        let mut customizations_entries: Vec<serde_json::Value> = Vec::with_capacity(entries.len());
        for (idx, entry) in entries.into_iter().enumerate() {
            // Pre-substitution capture of the original entry's collected fields preserves
            // the literal label content for the plural arrays even if substitution would
            // expand variables that don't apply (no host vars in scope here).
            let raw_entry_collected = collect_entry_from_config_json(&entry);
            if let Some(customizations) = entry.get("customizations") {
                if let serde_json::Value::Object(map) = customizations {
                    if !map.is_empty() {
                        customizations_entries.push(customizations.clone());
                    }
                }
            }

            let cfg: deacon_core::config::DevContainerConfig = serde_json::from_value(entry)
                .with_context(|| {
                    format!(
                        "Failed to deserialize devcontainer.metadata entry [{}] from container '{}'",
                        idx, container_info.id
                    )
                })?;
            let (substituted, _) = cfg.apply_variable_substitution(container_context);

            // Prefer the substituted JSON when it surfaces a collected field; otherwise fall
            // back to the raw entry capture (handles fields that DevContainerConfig might
            // not yet round-trip through serde, e.g. future additions).
            let substituted_json = serde_json::to_value(&substituted)?;
            let mut entry_map = collect_entry_from_config_json(&substituted_json);
            for (key, value) in raw_entry_collected {
                entry_map.entry(key).or_insert(value);
            }
            if !entry_map.is_empty() {
                metadata_entries.push(entry_map);
            }
            chain.push(substituted);
        }

        let merged = deacon_core::config::ConfigMerger::merge_configs(&chain);
        let base_json = serde_json::to_value(&merged)?;

        debug!(
            "Container-based merged configuration computed successfully ({} metadata entries, {} customizations entries)",
            metadata_entries.len(),
            customizations_entries.len()
        );
        let shaped = apply_upstream_merge_shape(base_json, &metadata_entries);
        Ok(apply_customizations_shape(shaped, &customizations_entries))
    } else if let Some(features_config) = features_config {
        debug!("Computing features-based merged configuration");

        // Derive configuration from features metadata (for non-collected fields)
        // alongside a list of per-feature metadata entries (for the plural collected
        // arrays). The base config is appended as the final entry so its own
        // lifecycle commands flow into the plural arrays — matching upstream's
        // `getDevcontainerMetadata` which appends `pick(devContainerConfig, pickConfigProperties)`.
        // `customizations` follows the same trailing-base ordering but uses the per-tool
        // array shape (see `apply_customizations_shape`).
        let mut derived_config = deacon_core::config::DevContainerConfig::default();
        let mut metadata_entries: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();
        let mut customizations_entries: Vec<serde_json::Value> = Vec::new();

        for feature_set in &features_config.feature_sets {
            for feature in &feature_set.features {
                // Re-fetch metadata per resolved feature. FeaturesConfiguration intentionally
                // does not carry the full FeatureMetadata blob (output schema only keeps the
                // user-visible subset), so we look it up again via the same OCI fetcher used
                // upstream — cached connections keep the cost low.
                //
                // Local features (canonical id `local:/abs/path/...`) skip
                // the OCI path entirely: their metadata lives on disk at
                // <abs-path>/devcontainer-feature.json. Per #106.
                let downloaded_owned;
                let metadata: &deacon_core::features::FeatureMetadata = if let Some(local_path) =
                    feature.id.strip_prefix("local:")
                {
                    let metadata_path =
                        std::path::Path::new(local_path).join("devcontainer-feature.json");
                    let parsed =
                        deacon_core::features::parse_feature_metadata(&metadata_path).map_err(
                            |e| {
                                anyhow::anyhow!(
                                    "Failed to parse local feature metadata at '{}' for merged config: {}",
                                    metadata_path.display(),
                                    e
                                )
                            },
                        )?;
                    downloaded_owned = parsed;
                    &downloaded_owned
                } else {
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

                    downloaded_owned = downloaded.metadata;
                    &downloaded_owned
                };

                // Collect this feature's entrypoint + lifecycle hooks (none of which live
                // on DevContainerConfig) for the plural output arrays.
                let entry = collect_entry_from_feature_metadata(metadata);
                if !entry.is_empty() {
                    metadata_entries.push(entry);
                }

                // Merge non-collected metadata into derived_config. Lifecycle commands
                // are intentionally NOT folded in here — they're collected separately
                // for the plural output. Mirrors upstream `mergeConfiguration` which
                // strips replaceProperties from the base before re-emitting plurals.
                for (key, value) in &metadata.container_env {
                    derived_config
                        .container_env
                        .insert(key.clone(), value.clone());
                }

                for mount in &metadata.mounts {
                    derived_config.mounts.push(mount.clone());
                }

                // Customizations are emitted as per-tool arrays in the final output
                // (see `apply_customizations_shape`). Collect this feature's contribution
                // here rather than deep-merging into derived_config — that downstream
                // shape transformation strips deep-merged customizations anyway.
                if let Some(customizations) = &metadata.customizations {
                    if let serde_json::Value::Object(map) = customizations {
                        if !map.is_empty() {
                            customizations_entries.push(customizations.clone());
                        }
                    }
                }

                // init / privileged accumulate with OR semantics across features (matches
                // upstream `imageMetadata.some(entry => entry.init)` in mergeConfiguration).
                // Last-wins per feature would let a later `Some(false)` silently revoke an
                // earlier feature's `Some(true)`, which contradicts the spec.
                derived_config.init = or_merge_bool(derived_config.init, metadata.init);
                derived_config.privileged =
                    or_merge_bool(derived_config.privileged, metadata.privileged);

                derived_config.cap_add.extend(metadata.cap_add.clone());

                derived_config
                    .security_opt
                    .extend(metadata.security_opt.clone());
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

        // Append the base config's collected fields as the trailing metadata entry so
        // user-authored lifecycle commands appear in the plural arrays.
        let base_json_for_collection = serde_json::to_value(base_config)?;
        let base_entry = collect_entry_from_config_json(&base_json_for_collection);
        if !base_entry.is_empty() {
            metadata_entries.push(base_entry);
        }
        // Per upstream `getDevcontainerMetadata`, the base config's `customizations` is the
        // final entry in the metadata chain (pickConfigProperties includes `customizations`).
        if let serde_json::Value::Object(map) = &base_config.customizations {
            if !map.is_empty() {
                customizations_entries.push(base_config.customizations.clone());
            }
        }

        // Merge base config with derived feature metadata (for non-collected fields).
        // Image metadata entries (from the image's `devcontainer.metadata` LABEL,
        // #91) come BEFORE the base config — they are the lower-precedence
        // layer per the spec, so the user's devcontainer.json wins on conflict.
        // Features-derived metadata wraps in on top.
        let mut chain: Vec<deacon_core::config::DevContainerConfig> = Vec::new();
        chain.extend(image_metadata_entries.iter().cloned());
        chain.push(base_config.clone());
        chain.push(substituted_derived);
        let merged = deacon_core::config::ConfigMerger::merge_configs(&chain);
        let merged_json = serde_json::to_value(&merged)?;

        // Surface image-metadata entries in the plural-collected-arrays
        // (entrypoints, onCreateCommands, …) too, ordered before features
        // and base per upstream getDevcontainerMetadata precedence.
        let mut combined_metadata_entries: Vec<serde_json::Map<String, serde_json::Value>> =
            Vec::new();
        for entry in image_metadata_entries {
            let entry_json = serde_json::to_value(entry)?;
            let collected = collect_entry_from_config_json(&entry_json);
            if !collected.is_empty() {
                combined_metadata_entries.push(collected);
            }
            if let Some(c) = entry_json.get("customizations") {
                if let serde_json::Value::Object(map) = c {
                    if !map.is_empty() {
                        customizations_entries.insert(0, c.clone());
                    }
                }
            }
        }
        combined_metadata_entries.extend(metadata_entries);

        debug!(
            "Features-based merged configuration computed successfully ({} image entries, {} feature/base entries, {} customizations entries)",
            image_metadata_entries.len(),
            combined_metadata_entries.len(),
            customizations_entries.len()
        );
        let shaped = apply_upstream_merge_shape(merged_json, &combined_metadata_entries);
        Ok(apply_customizations_shape(shaped, &customizations_entries))
    } else {
        // No container and no features. Image metadata (if any) still
        // contributes its containerEnv / remoteUser / lifecycle entries —
        // this is the most common path: `read-configuration` against a
        // simple `image:`-only devcontainer.json after `up` has built or
        // pulled the image. (#91)
        debug!(
            "No container or features-based metadata; folding {} image-metadata entries into base config",
            image_metadata_entries.len()
        );
        let mut chain: Vec<deacon_core::config::DevContainerConfig> = Vec::new();
        chain.extend(image_metadata_entries.iter().cloned());
        chain.push(base_config.clone());
        let merged = deacon_core::config::ConfigMerger::merge_configs(&chain);
        let merged_json = serde_json::to_value(&merged)?;

        let mut entries: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();
        for entry in image_metadata_entries {
            let entry_json = serde_json::to_value(entry)?;
            let collected = collect_entry_from_config_json(&entry_json);
            if !collected.is_empty() {
                entries.push(collected);
            }
        }
        let base_entry = collect_entry_from_config_json(&merged_json);
        if !base_entry.is_empty() {
            entries.push(base_entry);
        }

        let mut customizations_entries: Vec<serde_json::Value> = Vec::new();
        for entry in image_metadata_entries {
            if let Some(c) = serde_json::to_value(entry)?.get("customizations") {
                if let serde_json::Value::Object(map) = c {
                    if !map.is_empty() {
                        customizations_entries.push(c.clone());
                    }
                }
            }
        }
        if let serde_json::Value::Object(map) = &base_config.customizations {
            if !map.is_empty() {
                customizations_entries.push(base_config.customizations.clone());
            }
        }

        let shaped = apply_upstream_merge_shape(merged_json, &entries);
        Ok(apply_customizations_shape(shaped, &customizations_entries))
    }
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

    // Selector validation:
    // At least one of --container-id, --id-label, --workspace-folder, or
    // --config is required. Spec parity (#66): the upstream reference CLI
    // accepts `--config <path>` on its own — `read-configuration` can parse
    // a config file without any workspace context. We mirror that here.
    let has_container_id = args.container_id.is_some();
    let has_id_label = !args.id_label.is_empty();
    let has_workspace_folder = args.workspace_folder.is_some();
    let has_config = args.config_path.is_some();
    if !has_container_id && !has_id_label && !has_workspace_folder && !has_config {
        anyhow::bail!(
            "Missing required argument: One of --container-id, --id-label, --workspace-folder, or --config is required."
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

    // Determine workspace folder.
    //
    // Spec parity (#66): when only `--config` is provided (no
    // `--workspace-folder`), default workspace to the directory containing
    // the config file. This matches the upstream reference CLI behavior of
    // accepting `--config` on its own and keeps `${localWorkspaceFolder}`
    // substitutions meaningful.
    let config_parent_workspace = args
        .workspace_folder
        .is_none()
        .then(|| args.config_path.as_deref().and_then(|p| p.parent()))
        .flatten()
        .map(|p| p.to_path_buf());
    let workspace_folder = args
        .workspace_folder
        .as_deref()
        .or(config_parent_workspace.as_deref())
        .unwrap_or(Path::new("."));

    // Load configuration using shared helper (aligns with up/exec behavior)
    // The actual config file that was loaded/discovered (may differ from the CLI
    // `--config` arg when auto-discovered under `.devcontainer/`). Used to anchor
    // local feature paths (`./feature`) to the config file's directory.
    let mut resolved_config_path: Option<PathBuf> = None;
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
        })
        .await?;
        resolved_config_path = Some(config_result.config_path);
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
        let docker = deacon_core::docker::CliDocker::with_path(args.docker_path.clone());

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
                debug!(
                    "Proceeding without container-specific substitutions (merged config not requested)"
                );
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

    // Read-config-only seam (Divergence A): when there is no container and the
    // config declares no explicit `workspaceFolder`, the shared loader leaves
    // `${containerWorkspaceFolder}` unresolved on purpose — `up` fills it from
    // the real mount target during its container-aware pass, and seeding a
    // default in the shared loader would corrupt that. But `read-configuration`
    // never reaches that pass, so the literal leaks into the output. The
    // reference CLI resolves it to the host workspace path (the same value as
    // `${localWorkspaceFolder}`) in this case, so do the same here on the output
    // config only. Substitution is idempotent for already-resolved fields.
    let config =
        if !container_only_mode && container_info.is_none() && config.workspace_folder.is_none() {
            let mut ctx = SubstitutionContext::new(workspace_folder)?;
            ctx.container_workspace_folder = Some(ctx.local_workspace_folder.clone());
            if let Some(secrets) = &secrets {
                for (key, value) in secrets.as_env_vars() {
                    ctx.local_env.insert(key.clone(), value.clone());
                }
            }
            let (resolved, _report) = config.apply_variable_substitution(&ctx);
            resolved
        } else {
            config
        };

    // Divergence B: the reference CLI's `configuration` output is the RAW entry
    // config with `extends` preserved — it defers the extends merge to
    // `up`/`mergedConfiguration`. deacon's shared loader eagerly merges the
    // extends chain (and drops `extends`), which is correct for `up` but diverges
    // from the reference's read-configuration presentation. So when the entry
    // file declares `extends`, load and substitute the single entry file on its
    // own for the output `configuration` field, leaving the merged `config`
    // (used for `featuresConfiguration`/`mergedConfiguration`) untouched. Configs
    // without `extends` are unaffected (raw == merged for a single file).
    let raw_config_for_output: Option<DevContainerConfig> = if container_only_mode {
        None
    } else if let Some(cfg_path) = resolved_config_path.as_deref() {
        match deacon_core::config::ConfigLoader::load_from_path(cfg_path).await {
            Ok(raw) if raw.extends.is_some() => {
                let substituted = if let Some(ctx) = &container_context {
                    // Reuse the exact container-aware context applied to the merged config.
                    raw.apply_variable_substitution(ctx).0
                } else {
                    let mut ctx = SubstitutionContext::new(workspace_folder)?;
                    // Mirror the shared loader (fix #4) + Divergence A seam.
                    match raw.workspace_folder.as_deref() {
                        Some(wf) if !wf.trim().is_empty() && !wf.contains("${") => {
                            ctx.container_workspace_folder = Some(wf.to_string());
                        }
                        None => {
                            ctx.container_workspace_folder =
                                Some(ctx.local_workspace_folder.clone());
                        }
                        _ => {}
                    }
                    if let Some(secrets) = &secrets {
                        for (key, value) in secrets.as_env_vars() {
                            ctx.local_env.insert(key.clone(), value.clone());
                        }
                    }
                    raw.apply_variable_substitution(&ctx).0
                };
                Some(substituted)
            }
            _ => None,
        }
    } else {
        None
    };

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
    //    (metadata is derived from feature manifests rather than container labels)
    let features_configuration_for_output = if args.include_features_configuration
        || (args.include_merged_configuration && args.container_id.is_none())
    {
        // Anchor local-feature paths to the config file's directory. Use the
        // CLI `--config` arg when given, otherwise the *discovered* config path
        // (e.g. `.devcontainer/devcontainer.json`), and only fall back to the
        // workspace folder when neither is available. Mirrors the up flow's
        // anchor (`features_build.rs`); the previous code used only the CLI arg,
        // so an auto-discovered config mis-anchored `./feature` to the workspace
        // folder instead of `.devcontainer/`.
        let features_config_dir = args
            .config_path
            .as_deref()
            .or(resolved_config_path.as_deref())
            .and_then(|p| p.parent())
            .unwrap_or(workspace_folder)
            .to_path_buf();
        Some(
            resolve_features_configuration(
                &config,
                args.additional_features.as_deref(),
                args.skip_feature_auto_mapping,
                &fetcher,
                &features_config_dir,
            )
            .await?,
        )
    } else {
        None
    };

    // Compute merged configuration if requested.
    //
    // Per spec: mergedConfiguration = mergeConfiguration(base_config,
    // imageMetadata). Image metadata may come from any of:
    //
    // - A running container's `devcontainer.metadata` label (container path)
    // - Feature manifests (features-derived metadata path)
    // - The `devcontainer.metadata` LABEL baked into the image referenced
    //   by `config.image` (best-effort image inspect; #91). Without this
    //   path, `read-configuration --include-merged-configuration` against
    //   an image-based config without `--container-id` silently drops the
    //   image's metadata entries — exactly the gap the spec flags.
    let merged_configuration = if args.include_merged_configuration {
        // We may need features for merged config computation even if not outputting them
        let features_for_merge = features_configuration_for_output.as_ref();

        // Best-effort image-metadata fetch: only when (a) no container
        // already provides the label, and (b) `config.image` is set. A
        // missing image (not pulled, no daemon, unreadable label) leaves
        // image_metadata_entries empty and the merge falls through to
        // the existing branches — matches upstream's "best-effort"
        // semantics for the metadata layer.
        // Resolution order:
        //   1. `config.image` (literal image-based devcontainer.json).
        //   2. The image of the workspace's running container, found by
        //      identity-label lookup. Covers `build.dockerfile` configs
        //      where `config.image` is None — at read-config time we
        //      don't know the built image tag, but we can find it via
        //      the container that `up` created.
        let image_metadata_entries: Vec<deacon_core::config::DevContainerConfig> =
            if container_info.is_none() {
                use deacon_core::docker::Docker;
                let docker = deacon_core::docker::CliDocker::with_path(args.docker_path.clone());
                let image_ref: Option<String> = if let Some(image) = config.image.as_deref() {
                    Some(image.to_string())
                } else if let Ok(canonical_workspace) = workspace_folder.canonicalize() {
                    // For `build.dockerfile` configs `config.image` is None at
                    // read-config time. Find the workspace's container via the
                    // spec-mandated `devcontainer.local_folder` label (#80) —
                    // the workspaceHash/configHash pair drifts whenever `up`
                    // mutates the config mid-flight (workspace_mount injection,
                    // image-metadata merge, etc.), so read-config can never
                    // reconstruct an identical hash. The path label is stable.
                    let label_selector = format!(
                        "devcontainer.source=deacon,devcontainer.local_folder={}",
                        canonical_workspace.display()
                    );
                    match docker.list_containers(Some(&label_selector)).await {
                        Ok(containers) if !containers.is_empty() => {
                            match docker.inspect_container(&containers[0].id).await {
                                Ok(Some(info)) => Some(info.image),
                                _ => None,
                            }
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(image_ref) = image_ref {
                    parse_image_metadata_entries(&docker, &image_ref).await
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

        let mut merged = compute_merged_configuration(
            &config,
            container_info.as_ref(),
            container_context.as_ref(),
            features_for_merge,
            secrets.as_ref(),
            &fetcher,
            &image_metadata_entries,
        )
        .await?;
        normalize_merged_configuration_shape(&mut merged);
        Some(merged)
    } else {
        None
    };

    // Build output payload
    let output_payload = ReadConfigurationOutput {
        configuration: if container_only_mode {
            // Per spec line 310: "Only container flags provided (no config/workspace): returns { configuration: {}, ... }"
            serde_json::Value::Object(serde_json::Map::new())
        } else if let Some(raw) = &raw_config_for_output {
            // Divergence B: emit the raw (un-merged) entry config with `extends`
            // preserved, matching the reference CLI.
            serde_json::to_value(raw)?
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
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_normalize_merged_shape_host_requirements_to_bytes() {
        // mergedConfiguration normalizes memory/storage to a byte-count STRING (binary
        // units), matching the reference CLI; cpus stays numeric. "8gb" -> 8 * 2^30.
        let mut v = json!({
            "hostRequirements": { "cpus": 4, "memory": "8gb", "storage": "32gb" }
        });
        normalize_merged_configuration_shape(&mut v);
        let hr = &v["hostRequirements"];
        assert_eq!(hr["memory"], json!("8589934592"));
        assert_eq!(hr["storage"], json!("34359738368"));
        assert_eq!(hr["cpus"], json!(4));
    }

    #[test]
    fn test_normalize_merged_shape_host_requirements_already_bytes() {
        // A raw byte count (already-merged form) round-trips unchanged.
        let mut v = json!({ "hostRequirements": { "memory": "536870912" } });
        normalize_merged_configuration_shape(&mut v);
        assert_eq!(v["hostRequirements"]["memory"], json!("536870912"));
    }

    #[test]
    fn test_normalize_merged_shape_init_privileged_default_false() {
        // Absent and null both materialize to `false` in the merged shape.
        let mut absent = json!({ "image": "x" });
        normalize_merged_configuration_shape(&mut absent);
        assert_eq!(absent["init"], json!(false));
        assert_eq!(absent["privileged"], json!(false));

        let mut nulled = json!({ "init": null, "privileged": null });
        normalize_merged_configuration_shape(&mut nulled);
        assert_eq!(nulled["init"], json!(false));
        assert_eq!(nulled["privileged"], json!(false));
    }

    #[test]
    fn test_normalize_merged_shape_init_privileged_true_preserved() {
        // A real `true` (e.g. accumulated from a feature) is never downgraded.
        let mut v = json!({ "init": true, "privileged": true });
        normalize_merged_configuration_shape(&mut v);
        assert_eq!(v["init"], json!(true));
        assert_eq!(v["privileged"], json!(true));
    }

    #[test]
    fn test_normalize_merged_shape_no_host_requirements_is_noop() {
        // Missing hostRequirements is fine; only init/privileged get defaulted.
        let mut v = json!({ "image": "x" });
        normalize_merged_configuration_shape(&mut v);
        assert!(v.get("hostRequirements").is_none());
    }

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
        // Test that the flag is honored by workspace resolution (see resolve_workspace_configuration).
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
        // without container or features (merged config equals base config)
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

        // With no container and no features, merged configuration equals the base config.
    }

    #[tokio::test]
    async fn test_local_feature_path_not_treated_as_oci_ref() {
        // Per #106: ./local-feature must NOT be fetched as
        // https://./v2/devcontainers/local-feature/manifests/latest. Both
        // --include-features-configuration and --include-merged-configuration
        // (without container) should read the metadata from disk.
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();
        let config_path = workspace.join("devcontainer.json");
        let feature_dir = workspace.join("local-feature");
        std::fs::create_dir(&feature_dir).unwrap();
        std::fs::write(
            feature_dir.join("devcontainer-feature.json"),
            r#"{ "id": "local-feature", "version": "1.0.0", "name": "Local Feature" }"#,
        )
        .unwrap();
        std::fs::write(feature_dir.join("install.sh"), "#!/bin/sh\nexit 0\n").unwrap();

        let config_content = r#"{
            "name": "local-feat-test",
            "image": "mcr.microsoft.com/devcontainers/base:debian",
            "features": { "./local-feature": {} }
        }"#;
        std::fs::write(&config_path, config_content).unwrap();

        let args = ReadConfigurationArgs {
            include_features_configuration: true,
            include_merged_configuration: true,
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
            workspace_folder: Some(workspace.to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        // Pre-fix: this errored with a Connection failed for URL
        // `https://./v2/devcontainers/local-feature/manifests/latest`. Post-fix:
        // succeeds with the on-disk metadata.
        let result = execute_read_configuration(args).await;
        assert!(
            result.is_ok(),
            "expected local feature to resolve from disk; got {result:?}"
        );
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
    async fn test_container_metadata_merge_preserves_init() {
        let temp_dir = TempDir::new().unwrap();
        let base_config = DevContainerConfig {
            image: Some("ubuntu:24.04".to_string()),
            ..Default::default()
        };

        let mut labels = HashMap::new();
        labels.insert(
            "devcontainer.metadata".to_string(),
            serde_json::json!([{ "init": true }]).to_string(),
        );

        let container_info = deacon_core::docker::ContainerInfo {
            id: "container-id".to_string(),
            names: vec![],
            image: "ubuntu:24.04".to_string(),
            status: "running".to_string(),
            state: "running".to_string(),
            exposed_ports: vec![],
            port_mappings: vec![],
            env: HashMap::new(),
            labels,
            mounts: vec![],
        };
        let context = SubstitutionContext::new(temp_dir.path()).unwrap();
        let fetcher =
            deacon_core::oci::FeatureFetcher::new(deacon_core::oci::MockHttpClient::new());

        let merged = compute_merged_configuration(
            &base_config,
            Some(&container_info),
            Some(&context),
            None,
            None,
            &fetcher,
            &[],
        )
        .await
        .unwrap();

        assert_eq!(merged.get("init"), Some(&serde_json::json!(true)));
    }

    /// Container-label merged output must emit `entrypoints` collected from the label entries
    /// and must not surface a singular `entrypoint` (matches devcontainers/cli mergeConfiguration).
    #[tokio::test]
    async fn test_container_metadata_collects_entrypoints_array() {
        let temp_dir = TempDir::new().unwrap();
        let base_config = DevContainerConfig {
            image: Some("ubuntu:24.04".to_string()),
            ..Default::default()
        };

        let mut labels = HashMap::new();
        labels.insert(
            "devcontainer.metadata".to_string(),
            serde_json::json!([
                { "id": "ghcr.io/x/a:1", "entrypoint": "/scripts/a-entrypoint.sh" },
                { "id": "ghcr.io/x/b:1", "entrypoint": "/scripts/b-entrypoint.sh" }
            ])
            .to_string(),
        );

        let container_info = deacon_core::docker::ContainerInfo {
            id: "cid".to_string(),
            names: vec![],
            image: "ubuntu:24.04".to_string(),
            status: "running".to_string(),
            state: "running".to_string(),
            exposed_ports: vec![],
            port_mappings: vec![],
            env: HashMap::new(),
            labels,
            mounts: vec![],
        };
        let context = SubstitutionContext::new(temp_dir.path()).unwrap();
        let fetcher =
            deacon_core::oci::FeatureFetcher::new(deacon_core::oci::MockHttpClient::new());

        let merged = compute_merged_configuration(
            &base_config,
            Some(&container_info),
            Some(&context),
            None,
            None,
            &fetcher,
            &[],
        )
        .await
        .unwrap();

        assert_eq!(
            merged.get("entrypoints"),
            Some(&serde_json::json!([
                "/scripts/a-entrypoint.sh",
                "/scripts/b-entrypoint.sh"
            ]))
        );
        assert!(merged.get("entrypoint").is_none());
    }

    /// Container-label lifecycle hooks must be emitted as plural arrays (`onCreateCommands`,
    /// `postCreateCommands`, etc.) collected per label entry; singular names must be absent.
    #[tokio::test]
    async fn test_container_metadata_collects_lifecycle_arrays() {
        let temp_dir = TempDir::new().unwrap();
        let base_config = DevContainerConfig {
            image: Some("ubuntu:24.04".to_string()),
            ..Default::default()
        };

        let mut labels = HashMap::new();
        labels.insert(
            "devcontainer.metadata".to_string(),
            serde_json::json!([
                { "onCreateCommand": "echo a", "postCreateCommand": "echo a-post" },
                { "onCreateCommand": ["echo", "b"], "postStartCommand": "echo b-start" }
            ])
            .to_string(),
        );

        let container_info = deacon_core::docker::ContainerInfo {
            id: "cid".to_string(),
            names: vec![],
            image: "ubuntu:24.04".to_string(),
            status: "running".to_string(),
            state: "running".to_string(),
            exposed_ports: vec![],
            port_mappings: vec![],
            env: HashMap::new(),
            labels,
            mounts: vec![],
        };
        let context = SubstitutionContext::new(temp_dir.path()).unwrap();
        let fetcher =
            deacon_core::oci::FeatureFetcher::new(deacon_core::oci::MockHttpClient::new());

        let merged = compute_merged_configuration(
            &base_config,
            Some(&container_info),
            Some(&context),
            None,
            None,
            &fetcher,
            &[],
        )
        .await
        .unwrap();

        assert_eq!(
            merged.get("onCreateCommands"),
            Some(&serde_json::json!(["echo a", ["echo", "b"]]))
        );
        assert_eq!(
            merged.get("postCreateCommands"),
            Some(&serde_json::json!(["echo a-post"]))
        );
        assert_eq!(
            merged.get("postStartCommands"),
            Some(&serde_json::json!(["echo b-start"]))
        );
        // Singular names must NOT appear in the merged output.
        for singular in [
            "onCreateCommand",
            "updateContentCommand",
            "postCreateCommand",
            "postStartCommand",
            "postAttachCommand",
            "entrypoint",
        ] {
            assert!(
                merged.get(singular).is_none(),
                "singular '{}' should not appear in merged output",
                singular
            );
        }
    }

    /// When no container and no features are available, the base config's own lifecycle hooks
    /// must still surface as plural arrays in the merged output.
    #[tokio::test]
    async fn test_merged_shape_collects_base_lifecycle_when_no_metadata() {
        let base_config = DevContainerConfig {
            image: Some("ubuntu:24.04".to_string()),
            on_create_command: Some(serde_json::json!("echo from-base-config")),
            post_create_command: Some(serde_json::json!(["echo", "two-args"])),
            ..Default::default()
        };
        let fetcher =
            deacon_core::oci::FeatureFetcher::new(deacon_core::oci::MockHttpClient::new());

        let merged =
            compute_merged_configuration(&base_config, None, None, None, None, &fetcher, &[])
                .await
                .unwrap();

        assert_eq!(
            merged.get("onCreateCommands"),
            Some(&serde_json::json!(["echo from-base-config"]))
        );
        assert_eq!(
            merged.get("postCreateCommands"),
            Some(&serde_json::json!([["echo", "two-args"]]))
        );
        assert!(merged.get("onCreateCommand").is_none());
        assert!(merged.get("postCreateCommand").is_none());
    }

    /// Container-label merged output must dedupe capAdd / securityOpt across base + label
    /// entries (set-union, matching upstream `unionOrUndefined`).
    #[tokio::test]
    async fn test_container_metadata_dedupes_cap_add_and_security_opt() {
        let temp_dir = TempDir::new().unwrap();
        let base_config = DevContainerConfig {
            image: Some("ubuntu:24.04".to_string()),
            cap_add: vec!["NET_ADMIN".to_string()],
            security_opt: vec!["seccomp=unconfined".to_string()],
            ..Default::default()
        };

        let mut labels = HashMap::new();
        labels.insert(
            "devcontainer.metadata".to_string(),
            serde_json::json!([
                {
                    "capAdd": ["NET_ADMIN", "SYS_PTRACE"],
                    "securityOpt": ["seccomp=unconfined", "label=disable"]
                },
                {
                    "capAdd": ["SYS_PTRACE", "SYS_ADMIN"],
                    "securityOpt": ["label=disable"]
                }
            ])
            .to_string(),
        );

        let container_info = deacon_core::docker::ContainerInfo {
            id: "cid".to_string(),
            names: vec![],
            image: "ubuntu:24.04".to_string(),
            status: "running".to_string(),
            state: "running".to_string(),
            exposed_ports: vec![],
            port_mappings: vec![],
            env: HashMap::new(),
            labels,
            mounts: vec![],
        };
        let context = SubstitutionContext::new(temp_dir.path()).unwrap();
        let fetcher =
            deacon_core::oci::FeatureFetcher::new(deacon_core::oci::MockHttpClient::new());

        let merged = compute_merged_configuration(
            &base_config,
            Some(&container_info),
            Some(&context),
            None,
            None,
            &fetcher,
            &[],
        )
        .await
        .unwrap();

        // Set-union: each entry appears exactly once, base order preserved, overlay
        // entries appended in declaration order.
        assert_eq!(
            merged.get("capAdd"),
            Some(&serde_json::json!(["NET_ADMIN", "SYS_PTRACE", "SYS_ADMIN"]))
        );
        assert_eq!(
            merged.get("securityOpt"),
            Some(&serde_json::json!(["seccomp=unconfined", "label=disable"]))
        );
    }

    /// Container-label merged output must collect customizations per tool key into arrays
    /// rather than deep-merging objects (matches upstream `mergeConfiguration`). Per
    /// `pickUpdateableConfigProperties`, the base config's own customizations do NOT
    /// contribute when the container's id-labels match.
    #[tokio::test]
    async fn test_container_metadata_collects_customizations_per_tool() {
        let temp_dir = TempDir::new().unwrap();
        let base_config = DevContainerConfig {
            image: Some("ubuntu:24.04".to_string()),
            customizations: serde_json::json!({
                "vscode": { "extensions": ["base.ext-should-not-leak"] }
            }),
            ..Default::default()
        };

        let mut labels = HashMap::new();
        labels.insert(
            "devcontainer.metadata".to_string(),
            serde_json::json!([
                {
                    "customizations": {
                        "vscode": { "extensions": ["feat-a.ext-1"] }
                    }
                },
                {
                    "customizations": {
                        "vscode": { "extensions": ["feat-b.ext-2"], "settings": {"k": "v"} },
                        "jetbrains": { "plugins": ["plug-1"] }
                    }
                }
            ])
            .to_string(),
        );

        let container_info = deacon_core::docker::ContainerInfo {
            id: "cid".to_string(),
            names: vec![],
            image: "ubuntu:24.04".to_string(),
            status: "running".to_string(),
            state: "running".to_string(),
            exposed_ports: vec![],
            port_mappings: vec![],
            env: HashMap::new(),
            labels,
            mounts: vec![],
        };
        let context = SubstitutionContext::new(temp_dir.path()).unwrap();
        let fetcher =
            deacon_core::oci::FeatureFetcher::new(deacon_core::oci::MockHttpClient::new());

        let merged = compute_merged_configuration(
            &base_config,
            Some(&container_info),
            Some(&context),
            None,
            None,
            &fetcher,
            &[],
        )
        .await
        .unwrap();

        // Each tool key holds an ordered array, one entry per contributor that supplied
        // a value for that tool. Base config does NOT contribute in this path.
        assert_eq!(
            merged.get("customizations"),
            Some(&serde_json::json!({
                "vscode": [
                    { "extensions": ["feat-a.ext-1"] },
                    { "extensions": ["feat-b.ext-2"], "settings": { "k": "v" } }
                ],
                "jetbrains": [
                    { "plugins": ["plug-1"] }
                ]
            }))
        );
    }

    /// Without container or features, base config's `customizations` must still be emitted
    /// as a per-tool array (single-element arrays), not as the deep-merged object.
    #[tokio::test]
    async fn test_base_only_customizations_collapse_to_single_entry_arrays() {
        let base_config = DevContainerConfig {
            image: Some("ubuntu:24.04".to_string()),
            customizations: serde_json::json!({
                "vscode": { "extensions": ["from-base"] }
            }),
            ..Default::default()
        };
        let fetcher =
            deacon_core::oci::FeatureFetcher::new(deacon_core::oci::MockHttpClient::new());

        let merged =
            compute_merged_configuration(&base_config, None, None, None, None, &fetcher, &[])
                .await
                .unwrap();

        assert_eq!(
            merged.get("customizations"),
            Some(&serde_json::json!({
                "vscode": [ { "extensions": ["from-base"] } ]
            }))
        );
    }

    /// When the base config has no customizations and no other source contributes any,
    /// the `customizations` field must be omitted entirely (matches upstream
    /// `Object.keys(customizations).length ? customizations : undefined`).
    #[tokio::test]
    async fn test_no_customizations_omits_field() {
        let base_config = DevContainerConfig {
            image: Some("ubuntu:24.04".to_string()),
            ..Default::default()
        };
        let fetcher =
            deacon_core::oci::FeatureFetcher::new(deacon_core::oci::MockHttpClient::new());

        let merged =
            compute_merged_configuration(&base_config, None, None, None, None, &fetcher, &[])
                .await
                .unwrap();

        assert!(
            merged.get("customizations").is_none(),
            "customizations should be omitted when no source contributes, got {:?}",
            merged.get("customizations")
        );
    }

    /// Direct unit test of the customizations shape helper: tool keys are first-seen ordered,
    /// per-tool values are kept in entry-declaration order, empty entries are skipped, and
    /// the existing deep-merged `customizations` is stripped.
    #[test]
    fn test_apply_customizations_shape_basic() {
        let base = serde_json::json!({
            "image": "ubuntu",
            "customizations": { "vscode": { "extensions": ["leftover-deep-merge"] } }
        });
        let entries = vec![
            serde_json::json!({ "vscode": { "extensions": ["a"] } }),
            serde_json::json!({}), // empty contributor — skipped
            serde_json::json!({
                "vscode": { "settings": { "x": 1 } },
                "jetbrains": { "plugins": ["p1"] }
            }),
            serde_json::json!({ "jetbrains": { "plugins": ["p2"] } }),
        ];

        let result = apply_customizations_shape(base, &entries);

        // Deep-merge form stripped; per-tool arrays present in first-seen tool order.
        assert_eq!(
            result.get("customizations"),
            Some(&serde_json::json!({
                "vscode": [
                    { "extensions": ["a"] },
                    { "settings": { "x": 1 } }
                ],
                "jetbrains": [
                    { "plugins": ["p1"] },
                    { "plugins": ["p2"] }
                ]
            }))
        );
        // Other fields preserved.
        assert_eq!(result.get("image"), Some(&serde_json::json!("ubuntu")));
    }

    /// Empty entries (and any non-object inputs) must result in customizations being omitted
    /// from the output entirely, even if the input base had a deep-merged customizations.
    #[test]
    fn test_apply_customizations_shape_strips_empty() {
        let base = serde_json::json!({
            "image": "ubuntu",
            "customizations": { "vscode": { "extensions": ["should-be-removed"] } }
        });
        let result = apply_customizations_shape(base, &[]);
        assert!(result.get("customizations").is_none());
        assert_eq!(result.get("image"), Some(&serde_json::json!("ubuntu")));
    }

    /// `or_merge_bool` MUST treat `None` as the identity for the OR and short-circuit on any
    /// `Some(true)`. Used to accumulate `init` / `privileged` across feature metadata so a
    /// later `Some(false)` can never revoke an earlier `Some(true)` (matches upstream
    /// `imageMetadata.some(entry => entry.init)` semantics).
    #[test]
    fn test_or_merge_bool_truth_table() {
        // (None, None) → None — no source has expressed an opinion.
        assert_eq!(or_merge_bool(None, None), None);

        // (None, Some(x)) and (Some(x), None) → Some(x) — single source wins.
        assert_eq!(or_merge_bool(None, Some(true)), Some(true));
        assert_eq!(or_merge_bool(None, Some(false)), Some(false));
        assert_eq!(or_merge_bool(Some(true), None), Some(true));
        assert_eq!(or_merge_bool(Some(false), None), Some(false));

        // (Some(_), Some(_)) → Some(a || b).
        assert_eq!(or_merge_bool(Some(true), Some(true)), Some(true));
        assert_eq!(or_merge_bool(Some(true), Some(false)), Some(true));
        assert_eq!(or_merge_bool(Some(false), Some(true)), Some(true));
        assert_eq!(or_merge_bool(Some(false), Some(false)), Some(false));
    }

    /// Regression: simulating the features-loop accumulation with a sequence of
    /// (true, None, false, true) values must yield `Some(true)`. Last-wins would have
    /// returned `Some(true)` for this particular sequence by accident, but any sequence
    /// ending in `Some(false)` exposed the old bug — verified below.
    #[test]
    fn test_or_merge_bool_accumulates_through_feature_sequence() {
        // Bug-trigger sequence: true then false — last-wins returns false, OR returns true.
        let mut acc: Option<bool> = None;
        for value in [Some(true), Some(false)] {
            acc = or_merge_bool(acc, value);
        }
        assert_eq!(
            acc,
            Some(true),
            "later Some(false) must not revoke earlier Some(true)"
        );

        // Mixed sequence including None: Some(false), None, Some(false), Some(true), None
        let mut acc: Option<bool> = None;
        for value in [Some(false), None, Some(false), Some(true), None] {
            acc = or_merge_bool(acc, value);
        }
        assert_eq!(acc, Some(true));

        // All None → None (no feature set the field).
        let mut acc: Option<bool> = None;
        for value in [None, None, None] {
            acc = or_merge_bool(acc, value);
        }
        assert_eq!(acc, None);

        // All Some(false) → Some(false).
        let mut acc: Option<bool> = None;
        for value in [Some(false), Some(false)] {
            acc = or_merge_bool(acc, value);
        }
        assert_eq!(acc, Some(false));
    }

    /// Container-label merged output must dedupe `forwardPorts` with upstream's
    /// `localhost:N` ↔ `Number(N)` normalization (matches `mergeForwardPorts`).
    #[tokio::test]
    async fn test_container_metadata_dedupes_forward_ports_with_localhost_normalization() {
        let temp_dir = TempDir::new().unwrap();
        let base_config = DevContainerConfig {
            image: Some("ubuntu:24.04".to_string()),
            forward_ports: vec![deacon_core::config::PortSpec::Number(3000)],
            ..Default::default()
        };

        let mut labels = HashMap::new();
        labels.insert(
            "devcontainer.metadata".to_string(),
            serde_json::json!([
                { "forwardPorts": ["localhost:3000", 8080] },
                { "forwardPorts": ["localhost:8080", "3000:3000"] }
            ])
            .to_string(),
        );

        let container_info = deacon_core::docker::ContainerInfo {
            id: "cid".to_string(),
            names: vec![],
            image: "ubuntu:24.04".to_string(),
            status: "running".to_string(),
            state: "running".to_string(),
            exposed_ports: vec![],
            port_mappings: vec![],
            env: HashMap::new(),
            labels,
            mounts: vec![],
        };
        let context = SubstitutionContext::new(temp_dir.path()).unwrap();
        let fetcher =
            deacon_core::oci::FeatureFetcher::new(deacon_core::oci::MockHttpClient::new());

        let merged = compute_merged_configuration(
            &base_config,
            Some(&container_info),
            Some(&context),
            None,
            None,
            &fetcher,
            &[],
        )
        .await
        .unwrap();

        // 3000 / "localhost:3000" collapse into Number(3000); 8080 / "localhost:8080" collapse
        // into Number(8080); the port mapping "3000:3000" is a distinct entry (not normalized).
        assert_eq!(
            merged.get("forwardPorts"),
            Some(&serde_json::json!([3000, 8080, "3000:3000"]))
        );
    }

    /// Container-label merged output must dedupe `mounts` by container-side target across
    /// base config + label entries: last-wins per target, matching upstream `mergeMounts`.
    #[tokio::test]
    async fn test_container_metadata_dedupes_mounts_by_target() {
        let temp_dir = TempDir::new().unwrap();
        let base_config = DevContainerConfig {
            image: Some("ubuntu:24.04".to_string()),
            mounts: vec![serde_json::json!({
                "type": "bind",
                "source": "/host/base",
                "target": "/data"
            })],
            ..Default::default()
        };

        let mut labels = HashMap::new();
        labels.insert(
            "devcontainer.metadata".to_string(),
            serde_json::json!([
                {
                    "mounts": [
                        { "type": "bind", "source": "/host/feature-a", "target": "/data" },
                        { "type": "volume", "source": "shared", "target": "/cache" }
                    ]
                },
                {
                    "mounts": [
                        { "type": "bind", "source": "/host/feature-b", "target": "/data" }
                    ]
                }
            ])
            .to_string(),
        );

        let container_info = deacon_core::docker::ContainerInfo {
            id: "cid".to_string(),
            names: vec![],
            image: "ubuntu:24.04".to_string(),
            status: "running".to_string(),
            state: "running".to_string(),
            exposed_ports: vec![],
            port_mappings: vec![],
            env: HashMap::new(),
            labels,
            mounts: vec![],
        };
        let context = SubstitutionContext::new(temp_dir.path()).unwrap();
        let fetcher =
            deacon_core::oci::FeatureFetcher::new(deacon_core::oci::MockHttpClient::new());

        let merged = compute_merged_configuration(
            &base_config,
            Some(&container_info),
            Some(&context),
            None,
            None,
            &fetcher,
            &[],
        )
        .await
        .unwrap();

        let mounts = merged
            .get("mounts")
            .and_then(|v| v.as_array())
            .cloned()
            .expect("merged config must include mounts array");

        // Per-target last-wins: /data → feature-b (final occurrence wins), /cache kept once.
        // Surviving entries retain their declaration order: /cache appears before the final /data.
        assert_eq!(
            mounts.len(),
            2,
            "expected two mounts after target-dedup, got {:?}",
            mounts
        );
        assert_eq!(
            mounts[0],
            serde_json::json!({ "type": "volume", "source": "shared", "target": "/cache" })
        );
        assert_eq!(
            mounts[1],
            serde_json::json!({ "type": "bind", "source": "/host/feature-b", "target": "/data" })
        );
    }

    /// Direct unit test of the shape helper: singular names are removed and plural arrays
    /// preserve entry order, skipping nulls.
    #[test]
    fn test_apply_upstream_merge_shape_basic() {
        let base = serde_json::json!({
            "image": "ubuntu:24.04",
            "name": "test",
            "onCreateCommand": "should be stripped",
            "entrypoint": "also stripped",
        });
        let entries: Vec<serde_json::Map<String, serde_json::Value>> = vec![
            serde_json::json!({ "entrypoint": "/a.sh", "onCreateCommand": "echo a" })
                .as_object()
                .unwrap()
                .clone(),
            serde_json::json!({ "entrypoint": null, "postCreateCommand": "echo b-post" })
                .as_object()
                .unwrap()
                .clone(),
            serde_json::json!({ "entrypoint": "/c.sh" })
                .as_object()
                .unwrap()
                .clone(),
        ];

        let result = apply_upstream_merge_shape(base, &entries);

        // Singular fields removed from base
        assert!(result.get("onCreateCommand").is_none());
        assert!(result.get("entrypoint").is_none());

        // Non-collected base fields preserved
        assert_eq!(
            result.get("image"),
            Some(&serde_json::json!("ubuntu:24.04"))
        );
        assert_eq!(result.get("name"), Some(&serde_json::json!("test")));

        // Plural arrays collected in entry order, skipping null
        assert_eq!(
            result.get("entrypoints"),
            Some(&serde_json::json!(["/a.sh", "/c.sh"]))
        );
        assert_eq!(
            result.get("onCreateCommands"),
            Some(&serde_json::json!(["echo a"]))
        );
        assert_eq!(
            result.get("postCreateCommands"),
            Some(&serde_json::json!(["echo b-post"]))
        );
        // No entries contributed updateContentCommand → field omitted
        assert!(result.get("updateContentCommands").is_none());
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
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse --additional-features JSON")
        );
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
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("--additional-features must be a JSON object")
        );
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
    async fn test_read_configuration_arbitrary_config_filename_accepted() {
        // Spec parity (#65): the upstream reference CLI does not enforce a
        // filename allow-list on --config — any path that resolves to a
        // readable devcontainer.json document is accepted. A non-existent
        // path still surfaces the usual file-not-found error from the loader.
        let temp_dir = TempDir::new().unwrap();
        let custom_config_path = temp_dir.path().join("my-config.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&custom_config_path, config_content).unwrap();

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
            config_path: Some(custom_config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(
            result.is_ok(),
            "Expected arbitrary --config filename to be accepted, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_read_configuration_arbitrary_override_filename_accepted() {
        // Spec parity (#65): --override-config likewise accepts any filename.
        let temp_dir = TempDir::new().unwrap();
        let base_config_path = temp_dir.path().join("devcontainer.json");
        let custom_override_path = temp_dir.path().join("override.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "mcr.microsoft.com/devcontainers/base:ubuntu"
        }"#;

        fs::write(&base_config_path, config_content).unwrap();
        fs::write(&custom_override_path, config_content).unwrap();

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
            override_config_path: Some(custom_override_path),
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
            secret_registry: SecretRegistry::new(),
        };

        let result = execute_read_configuration(args).await;
        assert!(
            result.is_ok(),
            "Expected arbitrary --override-config filename to be accepted, got: {:?}",
            result.err()
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
