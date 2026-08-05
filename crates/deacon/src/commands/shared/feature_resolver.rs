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
use std::path::{Path, PathBuf};

use deacon_core::config::DevContainerConfig;
use deacon_core::feature_ref::canonicalize_user_feature_id;
use deacon_core::features::{
    FeatureDependencyResolver, ResolvedFeature, canonical_feature_id, options_from_json,
    parse_feature_metadata, resolve_local_feature_dir,
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
    workspace_root: &Path,
    fetcher: &FeatureFetcher<C>,
) -> Result<ResolvedFeature> {
    let is_local = feature_id.starts_with("./")
        || feature_id.starts_with("../")
        || feature_id.starts_with('/');

    let (canonical_id, source_string, metadata) = if is_local {
        let canonical_path = resolve_local_feature_dir(feature_id, config_dir, workspace_root)?;
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
        let canonical_ref = canonicalize_user_feature_id(feature_id)?;
        let (registry_url, namespace, name, tag) = parse_registry_reference(&canonical_ref)?;
        let feature_ref = FeatureRef::new(registry_url, namespace, name, tag);
        let downloaded = fetcher
            .fetch_feature(&feature_ref)
            .await
            .with_context(|| format!("Failed to fetch feature '{}'", feature_id))?;
        // The canonical id is the TAG-BEARING reference, not the Feature's metadata id.
        // A `features` map may declare one Feature at two versions, which the spec
        // defines as two distinct Features that both install
        // (`feature-dependencies.md` §Definition: Feature Equality, §Feature authorship);
        // keying on the metadata id — `"git"` for both — collapsed them into one node of
        // the dependency graph and dropped the second (#430). The Feature's OWN id is
        // still reported from `metadata.id`; this field is the graph key.
        (
            feature_ref.reference(),
            feature_ref.reference(),
            downloaded.metadata,
        )
    };

    Ok(ResolvedFeature {
        id: canonical_id,
        source: source_string,
        options: options_from_json(feature_value),
        metadata,
    })
}

/// Is this exact Feature — same resource AND same option set — already in the set?
///
/// `feature-dependencies.md` §(B1) skips a `dependsOn` target only "if the **exact**
/// Feature (see Feature Equality) has already been added", and §Definition: Feature
/// Equality makes the options part of that identity. So two `dependsOn` references to one
/// Feature with different options are two instances that both install (#489), while
/// identical option sets still collapse to one (measured against oracle 0.87.0).
///
/// The resource half stays the TAG-LESS name, per #430: a hard dependency is written
/// without a pin more often than not, and a user who declared that Feature at a specific
/// version has already satisfied it.
pub(crate) fn same_feature_already_resolved(
    resolved: &[ResolvedFeature],
    candidate: &ResolvedFeature,
) -> bool {
    let resource = canonical_feature_id(&candidate.id);
    let options = candidate.option_set();
    resolved
        .iter()
        .any(|f| canonical_feature_id(&f.id) == resource && f.option_set() == options)
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
    workspace_root: &Path,
    fetcher: &FeatureFetcher<C>,
) -> Result<Vec<ResolvedFeature>> {
    let features_map = match config.features().as_object() {
        Some(map) if !map.is_empty() => map,
        _ => return Ok(Vec::new()),
    };

    let mut resolved_features = Vec::with_capacity(features_map.len());

    for (feature_id, feature_value) in features_map {
        resolved_features.push(
            resolve_one_feature(
                feature_id,
                feature_value,
                config_dir,
                workspace_root,
                fetcher,
            )
            .await?,
        );
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
            let dep =
                resolve_one_feature(&dep_key, &dep_value, config_dir, workspace_root, fetcher)
                    .await?;
            if !same_feature_already_resolved(&resolved_features, &dep) {
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

/// Root of the host tree `up`/`build` stage feature content into, deterministic
/// in the workspace hash: `${TMPDIR}/deacon-features-<workspace_hash>`.
///
/// Deriving the path does NOT create it. `read-configuration` builds nothing and
/// must be able to report where a build *would* stage without leaving a
/// directory behind; the staging pass creates the tree itself.
pub(crate) fn feature_staging_root(workspace_hash: &str) -> PathBuf {
    std::env::temp_dir().join(format!("deacon-features-{}", workspace_hash))
}

/// The folder that directly contains the staged per-feature directories
/// (`<id>_<install index>`), which is what the reference CLI reports as
/// `featuresConfiguration.dstFolder`.
pub(crate) fn feature_staging_dst_folder(workspace_hash: &str) -> PathBuf {
    feature_staging_root(workspace_hash).join("features")
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::oci::default_fetcher;

    /// The config directory of a workspace laid out the way the spec requires:
    /// local Features live under `<workspace>/.devcontainer/`, which is also the
    /// containment root `resolve_local_feature_dir` enforces (#488).
    fn config_dir_of(workspace: &Path) -> PathBuf {
        workspace.join(".devcontainer")
    }

    #[tokio::test]
    async fn resolves_local_feature_with_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let feat = config_dir_of(dir.path()).join("features/hi");
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
        let resolved =
            resolve_features_ordered(&config, &config_dir_of(dir.path()), dir.path(), &fetcher)
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
        let feats = config_dir_of(dir.path()).join("features");
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
        let resolved =
            resolve_features_ordered(&config, &config_dir_of(dir.path()), dir.path(), &fetcher)
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

    /// Write a tree of local Features under `<dir>/.devcontainer/<name>`.
    fn write_local_features(dir: &Path, features: &[(&str, &str)]) {
        for (name, body) in features {
            let d = dir.join(".devcontainer").join(name);
            std::fs::create_dir_all(&d).expect("create feature dir");
            std::fs::write(d.join("devcontainer-feature.json"), body).expect("write metadata");
            std::fs::write(d.join("install.sh"), "#!/bin/sh\ntrue\n").expect("write install.sh");
        }
    }

    /// `(userFeatureId, option set)` for each node of the resolved plan — the same two
    /// columns the reference's `featuresConfiguration.featureSets` reports.
    fn plan_shape(features: &[ResolvedFeature]) -> Vec<String> {
        features
            .iter()
            .map(|f| format!("{} {}", f.source, f.option_set()))
            .collect()
    }

    /// #489 — the reference's own `dependsOn/local-with-options` e2e fixture: `./b` is
    /// requested with five different option sets and must yield five nodes, one per set.
    ///
    /// MEASURED at oracle 0.87.0 (`read-configuration --include-features-configuration`
    /// over `parity/fixtures/fx-upstream-dependson-local-with-options`): nine nodes,
    /// `b_0`…`b_4` then `./d`, `./e`, `./c`, `./a`. Before the fix deacon produced five,
    /// with a single `./b` carrying the CONFIGURATION's options — so the four dependents
    /// that asked for `./b` with their own options silently received someone else's.
    #[tokio::test]
    async fn depends_on_yields_one_instance_per_requested_option_set() {
        let dir = tempfile::tempdir().unwrap();
        write_local_features(
            dir.path(),
            &[
                (
                    "a",
                    r#"{ "id": "a", "version": "0.0.1",
                         "dependsOn": { "./b": { "optA": "a", "optB": "a" }, "./c": {} } }"#,
                ),
                ("b", r#"{ "id": "b", "version": "0.0.1" }"#),
                (
                    "c",
                    r#"{ "id": "c", "version": "0.0.1",
                         "dependsOn": { "./b": { "optA": "b", "optB": "a" }, "./d": {}, "./e": {} } }"#,
                ),
                (
                    "d",
                    r#"{ "id": "d", "version": "0.0.1",
                         "dependsOn": { "./b": { "optA": "b", "optB": "b" } } }"#,
                ),
                (
                    "e",
                    r#"{ "id": "e", "version": "0.0.1", "dependsOn": { "./b": {} } }"#,
                ),
            ],
        );

        let config: DevContainerConfig = serde_json::from_value(serde_json::json!({
            "features": { "./a": { "optA": "a", "optB": "b" }, "./b": { "optA": "a", "optB": "b" } }
        }))
        .unwrap();

        let fetcher = default_fetcher().unwrap();
        let resolved =
            resolve_features_ordered(&config, &config_dir_of(dir.path()), dir.path(), &fetcher)
                .await
                .expect("local dependsOn closure resolves");

        assert_eq!(
            plan_shape(&resolved),
            vec![
                "./b {}",
                "./b {optA=a,optB=a}",
                "./b {optA=a,optB=b}",
                "./b {optA=b,optB=a}",
                "./b {optA=b,optB=b}",
                "./d {}",
                "./e {}",
                "./c {}",
                "./a {optA=a,optB=b}",
            ],
            "nine nodes, five of them `./b` — one per distinct option set requested \
             (measured at oracle 0.87.0)"
        );
    }

    /// The other half of the equality rule: IDENTICAL option sets are the same Feature and
    /// still collapse to one node. Measured at oracle 0.87.0 on the same shape — two
    /// nodes, not three.
    #[tokio::test]
    async fn depends_on_with_identical_options_still_dedups() {
        let dir = tempfile::tempdir().unwrap();
        write_local_features(
            dir.path(),
            &[
                (
                    "a",
                    r#"{ "id": "a", "version": "0.0.1",
                         "dependsOn": { "./b": { "optA": "a", "optB": "b" } } }"#,
                ),
                ("b", r#"{ "id": "b", "version": "0.0.1" }"#),
            ],
        );

        let config: DevContainerConfig = serde_json::from_value(serde_json::json!({
            "features": { "./a": {}, "./b": { "optA": "a", "optB": "b" } }
        }))
        .unwrap();

        let fetcher = default_fetcher().unwrap();
        let resolved =
            resolve_features_ordered(&config, &config_dir_of(dir.path()), dir.path(), &fetcher)
                .await
                .expect("identical option sets resolve");

        assert_eq!(
            plan_shape(&resolved),
            vec!["./b {optA=a,optB=b}", "./a {}"],
            "a `dependsOn` asking for the option set already declared is the SAME Feature"
        );
    }

    /// #430's rule survives #489: an unpinned hard dependency is satisfied by the user's
    /// pinned declaration when the options agree, rather than double-installing.
    #[tokio::test]
    async fn depends_on_auto_install_still_dedups_against_the_user_declaration() {
        let dir = tempfile::tempdir().unwrap();
        write_local_features(
            dir.path(),
            &[
                (
                    "app",
                    r#"{ "id": "app", "version": "0.0.1", "dependsOn": { "./lib": {} } }"#,
                ),
                ("lib", r#"{ "id": "lib", "version": "0.0.1" }"#),
            ],
        );

        let config: DevContainerConfig = serde_json::from_value(serde_json::json!({
            "features": { "./app": {}, "./lib": {} }
        }))
        .unwrap();

        let fetcher = default_fetcher().unwrap();
        let resolved =
            resolve_features_ordered(&config, &config_dir_of(dir.path()), dir.path(), &fetcher)
                .await
                .expect("declared dependency resolves");

        assert_eq!(
            plan_shape(&resolved),
            vec!["./lib {}", "./app {}"],
            "the user's own declaration satisfies the hard dependency — no second copy"
        );
    }

    #[tokio::test]
    async fn no_features_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let config = DevContainerConfig::default();
        let fetcher = default_fetcher().unwrap();
        let resolved =
            resolve_features_ordered(&config, &config_dir_of(dir.path()), dir.path(), &fetcher)
                .await
                .unwrap();
        assert!(resolved.is_empty());
    }

    #[tokio::test]
    async fn missing_local_feature_fails_fast() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(config_dir_of(dir.path())).unwrap();
        let config: DevContainerConfig =
            serde_json::from_value(serde_json::json!({ "features": { "./features/nope": {} } }))
                .unwrap();
        let fetcher = default_fetcher().unwrap();
        let err =
            resolve_features_ordered(&config, &config_dir_of(dir.path()), dir.path(), &fetcher)
                .await
                .expect_err("missing local feature must error");
        assert!(
            err.to_string().contains("not accessible") || err.to_string().contains("nope"),
            "unexpected error: {err}"
        );
    }

    /// #488: `devcontainer-features-distribution.md` §Locally Referenced
    /// Features requires a local Feature to live under `.devcontainer/`. A
    /// Feature that exists but sits outside it must be REJECTED, not resolved —
    /// deacon used to resolve it happily where the reference CLI exits 1.
    #[tokio::test]
    async fn local_feature_outside_devcontainer_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        // The Feature is real and parseable — only its LOCATION is illegal.
        let feat = dir.path().join("local-features/hi");
        std::fs::create_dir_all(&feat).unwrap();
        std::fs::write(
            feat.join("devcontainer-feature.json"),
            r#"{ "id": "hi", "version": "1.0.0", "name": "Hi" }"#,
        )
        .unwrap();
        std::fs::create_dir_all(config_dir_of(dir.path())).unwrap();

        let config: DevContainerConfig = serde_json::from_value(
            serde_json::json!({ "features": { "../local-features/hi": {} } }),
        )
        .unwrap();
        let fetcher = default_fetcher().unwrap();
        let err =
            resolve_features_ordered(&config, &config_dir_of(dir.path()), dir.path(), &fetcher)
                .await
                .expect_err("a local Feature outside .devcontainer/ must be rejected");
        assert!(
            err.to_string()
                .contains("must be a child of the .devcontainer/ folder"),
            "unexpected error: {err}"
        );
    }
}
