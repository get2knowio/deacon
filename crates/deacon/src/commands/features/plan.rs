//! Features plan subcommand implementation
//!
//! Implements the `deacon features plan` subcommand for computing feature installation order.
//! Follows the specification in docs/subcommand-specs/features-plan/SPEC.md.

use crate::cli::FeatureCommands;
use crate::commands::shared::config_loader::{load_config, ConfigLoadArgs};
use anyhow::{Context, Result};
use deacon_core::config::DevContainerConfig;
use deacon_core::errors::{ConfigError, DeaconError, FeatureError};
use deacon_core::features::{
    canonicalize_feature_id, FeatureDependencyResolver, FeatureMergeConfig, FeatureMerger,
    OptionValue, ResolvedFeature,
};
use deacon_core::observability::{feature_plan_span, TimedSpan};
use deacon_core::oci::{default_fetcher, FeatureRef};
use deacon_core::registry_parser::parse_registry_reference;
use futures::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tracing::debug;

/// Plan result structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturesPlanResult {
    /// Features in installation order
    pub order: Vec<String>,
    /// Dependency graph for visibility
    pub graph: serde_json::Value,
}

/// Features command arguments (subset needed for plan)
#[derive(Debug, Clone)]
pub struct FeaturesArgs {
    pub command: FeatureCommands,
    pub workspace_folder: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub override_config_path: Option<PathBuf>,
    pub secrets_files: Vec<PathBuf>,
}

/// Error message for local feature paths
const LOCAL_FEATURE_ERROR_MSG: &str =
    "Local feature paths are not supported by 'features plan'—use a registry reference";

/// Default concurrency limit for parallel OCI metadata fetching
const DEFAULT_FETCH_CONCURRENCY: usize = 6;

/// Get the concurrency limit for parallel OCI metadata fetching
///
/// Reads from the DEACON_FETCH_CONCURRENCY environment variable if set,
/// otherwise uses the default of 6. The limit is clamped to a minimum of 1
/// and maximum of 32 to prevent resource exhaustion.
fn get_fetch_concurrency() -> usize {
    get_fetch_concurrency_impl(std::env::var("DEACON_FETCH_CONCURRENCY").ok().as_deref())
}

/// Internal implementation for getting fetch concurrency from an optional environment value
///
/// This function is separated to allow testing without global state mutation.
/// Takes an optional string value and returns the parsed, clamped concurrency limit.
fn get_fetch_concurrency_impl(env_value: Option<&str>) -> usize {
    env_value
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(DEFAULT_FETCH_CONCURRENCY)
        .clamp(1, 32)
}

/// Check if a feature identifier looks like a local path
fn is_local_path(feature_id: &str) -> bool {
    // Check for relative paths
    if feature_id.starts_with("./") || feature_id.starts_with("../") {
        return true;
    }

    // Check for absolute Unix paths
    if feature_id.starts_with('/') {
        return true;
    }

    // Check for Windows absolute paths (C:\, D:\, etc.)
    if feature_id.len() >= 3 {
        let chars: Vec<char> = feature_id.chars().collect();
        if chars.len() >= 3
            && chars[0].is_ascii_alphabetic()
            && chars[1] == ':'
            && (chars[2] == '\\' || chars[2] == '/')
        {
            return true;
        }
    }

    false
}

/// Execute features plan command
pub(super) async fn execute_features_plan(
    json: bool,
    additional_features: Option<&str>,
    args: &FeaturesArgs,
) -> Result<()> {
    // Load configuration with shared resolution (workspace/config/override/secrets)
    // For features plan, allow missing config if additional features are provided
    let config_load_result = load_config(ConfigLoadArgs {
        workspace_folder: args.workspace_folder.as_deref(),
        config_path: args.config_path.as_deref(),
        override_config_path: args.override_config_path.as_deref(),
        secrets_files: &args.secrets_files,
    });

    let (workspace_folder, mut config) = match config_load_result {
        Ok(result) => (result.workspace_folder, result.config),
        Err(DeaconError::Config(ConfigError::NotFound { .. })) => {
            // If config not found, use default config (empty features)
            // This allows features plan to work with only --additional-features
            let workspace_folder = match args.workspace_folder.clone() {
                Some(folder) => folder,
                None => std::env::current_dir().context(
                    "Failed to determine workspace folder: could not get current directory",
                )?,
            };
            (workspace_folder, DevContainerConfig::default())
        }
        Err(e) => return Err(e.into()),
    };

    // Start standardized span for feature planning
    let timed_span = TimedSpan::new(feature_plan_span(&workspace_folder));

    let result = {
        let _guard = timed_span.span().enter();

        debug!("Generating feature installation plan");

        // Parse and merge additional features if provided
        if let Some(additional_features_str) = additional_features {
            // Early validation: parse JSON and ensure it's an object before merge
            let parsed_json: serde_json::Value = serde_json::from_str(additional_features_str)
                .with_context(|| {
                    format!(
                        "Failed to parse --additional-features during feature plan initialization: {}",
                        additional_features_str
                    )
                })?;

            // Validate that the parsed JSON is an object (map)
            if !parsed_json.is_object() {
                anyhow::bail!("Failed to validate --additional-features: must be a JSON object.");
            }

            let merge_config = FeatureMergeConfig::new(
                Some(additional_features_str.to_string()),
                true,  // Prefer CLI features for planner precedence
                None,  // No install order override in this context
                false, // Do not skip auto-mapping in features plan
            );
            config.features = FeatureMerger::merge_features(&config.features, &merge_config)
                .context("Failed to merge additional features with devcontainer configuration.")?;
        }

        // Canonicalize feature IDs after merging
        if let Some(features_obj) = config.features.as_object_mut() {
            let mut canonicalized = serde_json::Map::new();
            for (key, value) in features_obj.iter() {
                let canonical_key = canonicalize_feature_id(key);
                canonicalized.insert(canonical_key, value.clone());
            }
            config.features = serde_json::Value::Object(canonicalized);
        }

        // Extract features from config
        let features_map_opt = config.features.as_object();
        if features_map_opt.is_none() || features_map_opt.unwrap().is_empty() {
            let result = FeaturesPlanResult {
                order: vec![],
                graph: serde_json::json!({}),
            };
            output_plan_result(&result, json)?;
            return Ok(());
        }
        let features_map = features_map_opt.unwrap();

        // Resolve features from registries to obtain metadata (deps, installsAfter, etc.)
        let fetcher = default_fetcher()
            .context("Failed to initialize OCI client for fetching feature metadata.")?;

        // Add span for metadata fetch loop with structured fields
        let resolved_features = {
            let fetch_start = std::time::Instant::now();
            let concurrency = get_fetch_concurrency();
            let fetch_span = tracing::span!(
                target: "deacon::features",
                tracing::Level::INFO,
                "features.fetch_metadata",
                feature_count = features_map.len(),
                concurrency = concurrency,
                fetch_ms = tracing::field::Empty
            );
            let _fetch_guard = fetch_span.enter();

            // Pre-validate all feature IDs before starting any fetches
            for (feature_id, _) in features_map.iter() {
                if is_local_path(feature_id) {
                    anyhow::bail!("{}. Feature key: '{}'", LOCAL_FEATURE_ERROR_MSG, feature_id);
                }
            }

            // Convert features_map to a Vec for parallel processing
            // Use a BTreeMap to ensure deterministic ordering of results
            let features_to_fetch: Vec<_> = features_map.iter().collect();

            // Fetch metadata in parallel with bounded concurrency
            let fetch_results: BTreeMap<String, ResolvedFeature> = stream::iter(features_to_fetch)
                .map(|(feature_id, feature_value)| {
                    let fetcher = &fetcher;
                    let feature_id = feature_id.to_string();
                    let feature_value = feature_value.clone();

                    async move {
                        // Parse registry reference
                        let (registry_url, namespace, name, tag) =
                            parse_registry_reference(&feature_id).with_context(|| {
                                format!(
                                    "Failed to parse registry reference for feature '{}' during feature resolution.",
                                    feature_id
                                )
                            })?;

                        let feature_ref = FeatureRef::new(
                            registry_url.clone(),
                            namespace.clone(),
                            name.clone(),
                            tag.clone(),
                        );

                        // Fetch feature metadata from OCI registry
                        let downloaded = fetcher.fetch_feature(&feature_ref).await.map_err(|e| {
                            // Pattern match on DeaconError to access the concrete error variant
                            let error_msg = format!(
                                "Failed to fetch feature metadata from OCI registry for feature '{}'",
                                feature_id
                            );
                            match e {
                                DeaconError::Feature(feature_err) => {
                                    // Map specific FeatureError variants to categorized errors with context
                                    match feature_err {
                                        FeatureError::Authentication { message } => {
                                            DeaconError::Feature(FeatureError::Authentication {
                                                message: format!("{}: {}", error_msg, message),
                                            })
                                        }
                                        FeatureError::Download { message } => {
                                            DeaconError::Feature(FeatureError::Download {
                                                message: format!("{}: {}", error_msg, message),
                                            })
                                        }
                                        _ => {
                                            // For all other FeatureError variants, wrap as OCI error
                                            DeaconError::Feature(FeatureError::Oci {
                                                message: format!("{}: {}", error_msg, feature_err),
                                            })
                                        }
                                    }
                                }
                                other => {
                                    // For non-Feature DeaconErrors, wrap as OCI error
                                    DeaconError::Feature(FeatureError::Oci {
                                        message: format!("{}: {}", error_msg, other),
                                    })
                                }
                            }
                        })?;

                        // Extract per-feature options from config entry if present
                        let options: std::collections::HashMap<String, OptionValue> = match feature_value {
                            serde_json::Value::Object(map) => map
                                .into_iter()
                                .map(|(k, v)| {
                                    // Convert serde_json::Value to OptionValue, preserving all types
                                    let option_value = match v {
                                        serde_json::Value::Bool(b) => OptionValue::Boolean(b),
                                        serde_json::Value::String(s) => OptionValue::String(s.clone()),
                                        serde_json::Value::Number(n) => OptionValue::Number(n.clone()),
                                        serde_json::Value::Array(a) => OptionValue::Array(a.clone()),
                                        serde_json::Value::Object(o) => OptionValue::Object(o.clone()),
                                        serde_json::Value::Null => OptionValue::Null,
                                    };
                                    (k.clone(), option_value)
                                })
                                .collect(),
                            _ => std::collections::HashMap::new(),
                        };

                        let resolved = ResolvedFeature {
                            id: downloaded.metadata.id.clone(),
                            source: feature_ref.reference(),
                            options,
                            metadata: downloaded.metadata,
                        };

                        // Return the feature_id as key for deterministic ordering
                        Ok::<(String, ResolvedFeature), anyhow::Error>((feature_id, resolved))
                    }
                })
                .buffer_unordered(concurrency)
                .collect::<Vec<Result<(String, ResolvedFeature)>>>()
                .await
                .into_iter()
                .collect::<Result<BTreeMap<String, ResolvedFeature>>>()?;

            // Record fetch duration before exiting span
            let fetch_duration_ms = fetch_start.elapsed().as_millis() as u64;
            fetch_span.record("fetch_ms", fetch_duration_ms);

            // Convert BTreeMap to Vec, maintaining deterministic order
            fetch_results.into_values().collect::<Vec<_>>()
        };

        // Create dependency resolver with override order from config
        let override_order = config.override_feature_install_order.clone();
        let resolver = FeatureDependencyResolver::new(override_order);

        // Add span for dependency resolution with structured fields
        let installation_plan = {
            let resolve_start = std::time::Instant::now();
            let node_count = resolved_features.len();
            // Count total edges (dependencies) across all features
            let edge_count: usize = resolved_features
                .iter()
                .map(|f| f.metadata.installs_after.len() + f.metadata.depends_on.len())
                .sum();

            let resolve_span = tracing::span!(
                target: "deacon::features",
                tracing::Level::INFO,
                "features.resolve_dependencies",
                node_count = node_count,
                edge_count = edge_count,
                resolve_ms = tracing::field::Empty
            );
            let _resolve_guard = resolve_span.enter();

            // Resolve dependencies and create installation plan
            let plan = resolver.resolve(&resolved_features).context(
                "Failed to resolve feature dependencies and compute installation order.",
            )?;

            // Record resolution duration before exiting span
            let resolve_duration_ms = resolve_start.elapsed().as_millis() as u64;
            resolve_span.record("resolve_ms", resolve_duration_ms);

            plan
        };

        // Extract order and create graph representation
        let order = installation_plan.feature_ids();
        let graph = build_graph_representation(&resolved_features);

        let result = FeaturesPlanResult { order, graph };

        output_plan_result(&result, json)?;

        Ok(())
    };

    // Complete the timed span with duration
    timed_span.complete();
    result
}

/// Build a graph representation from resolved features
///
/// Creates an adjacency list representation where each feature ID maps to an array
/// of its dependencies. The graph structure follows the format:
/// ```json
/// {
///   "featureA": [],
///   "featureB": ["featureA"],
///   "featureC": ["featureA", "featureB"]
/// }
/// ```
///
/// ## Graph Direction
/// The graph encodes **dependencies** (not dependents):
/// - `"featureB": ["featureA"]` means featureB depends on featureA
/// - Empty array means the feature has no dependencies
/// - This representation makes it clear what must be installed before each feature
///
/// ## Dependency Union
/// The dependency list for each feature is the **union** of:
/// - `installsAfter`: Ordering constraints from the feature metadata
/// - `dependsOn`: Hard dependencies from the feature metadata
///
/// Both fields are combined to provide a complete view of installation constraints.
/// Uses `BTreeSet` internally to ensure deterministic ordering and deduplication.
///
/// ## Conformance
/// Matches specification in `docs/subcommand-specs/features-plan/DATA-STRUCTURES.md`
/// and design decision in `docs/subcommand-specs/features-plan/SPEC.md` §16.
fn build_graph_representation(features: &[ResolvedFeature]) -> serde_json::Value {
    let mut graph = serde_json::Map::new();

    for feature in features {
        // Use a set for dedupe and deterministic ordering
        let mut deps = std::collections::BTreeSet::new();
        for dep in &feature.metadata.installs_after {
            deps.insert(dep.clone());
        }
        for dep_id in feature.metadata.depends_on.keys() {
            deps.insert(dep_id.clone());
        }
        graph.insert(
            feature.id.clone(),
            serde_json::Value::Array(deps.into_iter().map(serde_json::Value::String).collect()),
        );
    }

    serde_json::Value::Object(graph)
}

/// Output plan result in the specified format
fn output_plan_result(result: &FeaturesPlanResult, json: bool) -> Result<()> {
    if json {
        let json_output = serde_json::to_string_pretty(result)?;
        println!("{}", json_output);
    } else {
        println!("Feature Installation Plan:");
        println!("Order: {:?}", result.order);
        println!("Graph: {}", serde_json::to_string_pretty(&result.graph)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_fetch_concurrency_default() {
        assert_eq!(get_fetch_concurrency_impl(None), 6);
    }

    #[test]
    fn test_get_fetch_concurrency_custom() {
        assert_eq!(get_fetch_concurrency_impl(Some("10")), 10);
    }

    #[test]
    fn test_get_fetch_concurrency_clamped_min() {
        assert_eq!(get_fetch_concurrency_impl(Some("0")), 1);
    }

    #[test]
    fn test_get_fetch_concurrency_clamped_max() {
        assert_eq!(get_fetch_concurrency_impl(Some("100")), 32);
    }

    #[test]
    fn test_is_local_path_relative() {
        assert!(is_local_path("./foo"));
        assert!(is_local_path("../bar"));
    }

    #[test]
    fn test_is_local_path_absolute_unix() {
        assert!(is_local_path("/foo/bar"));
    }

    #[test]
    fn test_is_local_path_absolute_windows() {
        assert!(is_local_path("C:\\foo"));
        assert!(is_local_path("D:/bar"));
    }

    #[test]
    fn test_is_local_path_registry() {
        assert!(!is_local_path("ghcr.io/devcontainers/features/node"));
    }
}
