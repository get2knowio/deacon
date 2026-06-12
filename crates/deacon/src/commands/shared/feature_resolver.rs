//! Shared feature resolution.
//!
//! Resolves the features declared in a `DevContainerConfig` into an ordered
//! `Vec<ResolvedFeature>` (full metadata included), honoring local paths
//! (`./`, `../`, `/abs`) and OCI references, then applies dependency / install
//! order resolution. This is the common primitive behind `read-configuration`
//! (which groups the result by registry) and `run-user-commands` (which feeds
//! it to `aggregate_lifecycle_commands` for feature-contributed lifecycle
//! hooks).

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

use deacon_core::config::DevContainerConfig;
use deacon_core::features::{
    FeatureDependencyResolver, OptionValue, ResolvedFeature, parse_feature_metadata,
};
use deacon_core::oci::{FeatureFetcher, FeatureRef, HttpClient};
use deacon_core::registry_parser::parse_registry_reference;
use tracing::debug;

/// Resolve a single feature reference (local `./`,`../`,`/abs` or OCI) plus its
/// option value into a `ResolvedFeature`. Shared by the declared-feature loop
/// and the transitive-`dependsOn` closure in [`resolve_features_ordered`], and
/// reused by `read-configuration` to resolve its own `dependsOn` closure.
pub(crate) async fn resolve_one_feature<C: HttpClient>(
    feature_id: &str,
    feature_value: &serde_json::Value,
    config_dir: &Path,
    fetcher: &FeatureFetcher<C>,
) -> Result<ResolvedFeature> {
    let is_local = feature_id.starts_with("./")
        || feature_id.starts_with("../")
        || feature_id.starts_with('/');

    let (canonical_id, source_string, metadata) = if is_local {
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
        let metadata = parse_feature_metadata(&metadata_path).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse local feature metadata at '{}': {}",
                metadata_path.display(),
                e
            )
        })?;
        let canonical_id = format!("local:{}", canonical_path.display());
        (canonical_id, feature_id.to_string(), metadata)
    } else {
        let (registry_url, namespace, name, tag) = parse_registry_reference(feature_id)?;
        let feature_ref = FeatureRef::new(registry_url, namespace, name, tag);
        let downloaded = fetcher
            .fetch_feature(&feature_ref)
            .await
            .with_context(|| format!("Failed to fetch feature '{}'", feature_id))?;
        (
            downloaded.metadata.id.clone(),
            feature_ref.reference(),
            downloaded.metadata,
        )
    };

    Ok(ResolvedFeature {
        id: canonical_id,
        source: source_string,
        options: options_from_value(feature_value),
        metadata,
    })
}

/// Extract per-feature options from a `devcontainer.json` feature/`dependsOn`
/// value: an object yields typed options, a bare string is treated as
/// `{"version": <string>}`, anything else yields no options.
fn options_from_value(feature_value: &serde_json::Value) -> HashMap<String, OptionValue> {
    match feature_value {
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(k, v)| {
                let option_value = match v {
                    serde_json::Value::Bool(b) => OptionValue::Boolean(*b),
                    serde_json::Value::String(s) => OptionValue::String(s.clone()),
                    serde_json::Value::Number(n) => OptionValue::Number(n.clone()),
                    serde_json::Value::Array(a) => OptionValue::Array(a.clone()),
                    serde_json::Value::Object(o) => OptionValue::Object(o.clone()),
                    serde_json::Value::Null => OptionValue::Null,
                };
                (k.clone(), option_value)
            })
            .collect(),
        serde_json::Value::String(s) => {
            let mut map = HashMap::new();
            map.insert("version".to_string(), OptionValue::String(s.clone()));
            map
        }
        _ => HashMap::new(),
    }
}

/// Resolve `config.features` into install-ordered `ResolvedFeature`s.
///
/// - Local feature ids (`./`, `../`, absolute) are read from disk relative to
///   `config_dir`; OCI ids are fetched via `fetcher`.
/// - Returns an empty vec when no features are declared.
/// - **Fails fast**: any unresolvable feature (missing local path, missing
///   `devcontainer-feature.json`, OCI fetch error, dependency cycle) is
///   propagated with context rather than silently dropped.
// Only reachable through `full`-gated CLI dispatch (e.g. run-user-commands), so
// it is dead code in a `--no-default-features` MVP build; tests still exercise it.
pub(crate) async fn resolve_features_ordered<C: HttpClient>(
    config: &DevContainerConfig,
    config_dir: &Path,
    fetcher: &FeatureFetcher<C>,
) -> Result<Vec<ResolvedFeature>> {
    let features_map = match config.features.as_object() {
        Some(map) if !map.is_empty() => map,
        _ => return Ok(Vec::new()),
    };

    let mut resolved_features = Vec::with_capacity(features_map.len());

    for (feature_id, feature_value) in features_map {
        resolved_features
            .push(resolve_one_feature(feature_id, feature_value, config_dir, fetcher).await?);
    }

    // Auto-install transitive `dependsOn` (hard) dependencies — parity with the
    // reference CLI and with the `up`/`build` install path, so a feature that
    // hard-`dependsOn` an undeclared one no longer errors here and the
    // dependency's contributed lifecycle hooks are aggregated. `installsAfter`
    // (soft ordering) is NOT auto-installed — that stays the resolver's job.
    // The `while idx` walk also scans features pushed by the closure (transitive
    // closure); the dedup-by-id guard terminates on cycles.
    let mut idx = 0;
    while idx < resolved_features.len() {
        let mut deps: Vec<(String, serde_json::Value)> = resolved_features[idx]
            .metadata
            .depends_on
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        deps.sort_by(|a, b| a.0.cmp(&b.0)); // deterministic despite the unordered map
        for (dep_key, dep_value) in deps {
            let dep = resolve_one_feature(&dep_key, &dep_value, config_dir, fetcher).await?;
            if !resolved_features.iter().any(|f| f.id == dep.id) {
                debug!(dependency = %dep_key, "Auto-installing transitive dependsOn feature");
                resolved_features.push(dep);
            }
        }
        idx += 1;
    }

    // Apply dependency / install-order resolution (honors
    // overrideFeatureInstallOrder). Propagate cycle/ordering errors.
    let resolver = FeatureDependencyResolver::new(config.override_feature_install_order.clone());
    let plan = resolver
        .resolve(&resolved_features)
        .context("Failed to resolve feature installation order")?;

    Ok(plan.features)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::oci::default_fetcher;

    #[tokio::test]
    async fn resolves_local_feature_with_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let feat = dir.path().join("features/hi");
        std::fs::create_dir_all(&feat).unwrap();
        std::fs::write(
            feat.join("devcontainer-feature.json"),
            r#"{ "id": "hi", "version": "1.0.0", "name": "Hi",
                 "postCreateCommand": "echo hi" }"#,
        )
        .unwrap();
        std::fs::write(feat.join("install.sh"), "#!/bin/sh\ntrue\n").unwrap();

        let config: DevContainerConfig =
            serde_json::from_value(serde_json::json!({ "features": { "./features/hi": {} } }))
                .unwrap();

        let fetcher = default_fetcher().unwrap();
        let resolved = resolve_features_ordered(&config, dir.path(), &fetcher)
            .await
            .expect("local feature resolves without network");
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].metadata.name.as_deref(), Some("Hi"));
        assert!(resolved[0].metadata.post_create_command.is_some());
    }

    #[tokio::test]
    async fn auto_installs_transitive_depends_on() {
        // Local feature "app" hard-dependsOn local feature "lib"; only "app" is
        // declared. The reference auto-installs "lib" — so must we, and "lib"
        // must order before "app" (dependency edge).
        let dir = tempfile::tempdir().unwrap();
        let feats = dir.path().join("features");
        for (name, body) in [
            (
                "lib",
                r#"{ "id": "lib", "version": "1.0.0", "name": "Lib", "postCreateCommand": "echo lib" }"#,
            ),
            (
                "app",
                r#"{ "id": "app", "version": "1.0.0", "name": "App", "dependsOn": { "./features/lib": {} } }"#,
            ),
        ] {
            let d = feats.join(name);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("devcontainer-feature.json"), body).unwrap();
            std::fs::write(d.join("install.sh"), "#!/bin/sh\ntrue\n").unwrap();
        }

        // Declare ONLY app.
        let config: DevContainerConfig =
            serde_json::from_value(serde_json::json!({ "features": { "./features/app": {} } }))
                .unwrap();
        let fetcher = default_fetcher().unwrap();
        let resolved = resolve_features_ordered(&config, dir.path(), &fetcher)
            .await
            .expect("transitive dependsOn resolves");

        let names: Vec<&str> = resolved
            .iter()
            .filter_map(|f| f.metadata.name.as_deref())
            .collect();
        assert!(
            names.contains(&"Lib"),
            "auto-installed dep missing: {names:?}"
        );
        assert!(
            names.contains(&"App"),
            "declared feature missing: {names:?}"
        );
        // Dependency installs before the dependent.
        let pos = |n: &str| names.iter().position(|x| *x == n).unwrap();
        assert!(
            pos("Lib") < pos("App"),
            "dep must order before dependent: {names:?}"
        );
    }

    #[tokio::test]
    async fn no_features_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let config = DevContainerConfig::default();
        let fetcher = default_fetcher().unwrap();
        let resolved = resolve_features_ordered(&config, dir.path(), &fetcher)
            .await
            .unwrap();
        assert!(resolved.is_empty());
    }

    #[tokio::test]
    async fn missing_local_feature_fails_fast() {
        let dir = tempfile::tempdir().unwrap();
        let config: DevContainerConfig =
            serde_json::from_value(serde_json::json!({ "features": { "./features/nope": {} } }))
                .unwrap();
        let fetcher = default_fetcher().unwrap();
        let err = resolve_features_ordered(&config, dir.path(), &fetcher)
            .await
            .expect_err("missing local feature must error");
        assert!(
            err.to_string().contains("not accessible") || err.to_string().contains("nope"),
            "unexpected error: {err}"
        );
    }
}
