//! The disallowed-Features policy gate, shared by `up` and `build`.
//!
//! An operator names Features they refuse to have installed, and this gate
//! refuses the run before deacon touches a registry or a daemon. There are two
//! sources and they compose:
//!
//! 1. **`DEACON_DISALLOWED_FEATURES`** — a comma-separated list, deacon's own
//!    knob, needing no network and no file.
//! 2. **`--control-manifest`** ([`ControlManifestSource`]) — a URL or file in
//!    the reference CLI's manifest format ([#676]). Unset by default: deacon
//!    fetches nothing unless asked. See [`deacon_core::control_manifest`] for
//!    why the reference's own URL is not a default here.
//!
//! Both use one matching rule — an entry matches by PREFIX terminated at a
//! Feature-id separator (`/`, `:`, `@`), so `ghcr.io/devcontainers/features/node`
//! covers `…/node:1` — which lives in
//! [`deacon_core::control_manifest::feature_id_covered_by`] so the two lists can
//! never drift apart.
//!
//! Scope and placement, both fixed in [#675]:
//! - the gate sees the Features a run will actually install, which is the
//!   configuration's union with `--additional-features`;
//! - `build` consults it too.
//!
//! [#675]: https://github.com/get2knowio/deacon/issues/675
//! [#676]: https://github.com/get2knowio/deacon/issues/676

use anyhow::Result;
use deacon_core::control_manifest::{ControlManifestSource, feature_id_covered_by};
use deacon_core::errors::{ConfigError, DeaconError};
use std::path::Path;
use tracing::debug;

/// Features refused regardless of configuration. Deliberately empty: deacon
/// ships no opinion about which Features are problematic, which is the whole
/// point of making the source an operator's choice.
const DISALLOWED_FEATURES: &[&str] = &[];

/// The name reported when the environment variable is what refused a run.
const ENV_SOURCE: &str = "DEACON_DISALLOWED_FEATURES";

/// The entries `DEACON_DISALLOWED_FEATURES` contributes, in written order.
fn env_entries() -> Vec<String> {
    let mut entries: Vec<String> = DISALLOWED_FEATURES.iter().map(|e| e.to_string()).collect();
    if let Ok(raw) = std::env::var("DEACON_DISALLOWED_FEATURES") {
        entries.extend(
            raw.split(',')
                .map(str::trim)
                // A stray comma yields an empty entry, which is a prefix of
                // everything. Blocking every Feature is never what that meant.
                .filter(|entry| !entry.is_empty())
                .map(str::to_string),
        );
    }
    entries
}

/// Refuse the run when any Feature it would install is disallowed.
///
/// `features` is the configuration's `features` object AFTER any
/// `--additional-features` merge, so the gate sees exactly the set that would
/// be installed — which is also why `--ignore-additional-features` correctly
/// takes an overlay back out of scope.
///
/// Callers MUST invoke this before any registry or daemon work and before
/// `initializeCommand`; the reference refuses without touching either.
///
/// A named-but-unusable `--control-manifest` is an ERROR, not a shrug: the
/// operator asked for a policy, and proceeding without one would defeat it.
pub(crate) async fn check_for_disallowed_features(
    features: &serde_json::Value,
    manifest_source: Option<&ControlManifestSource>,
    cache_dir: &Path,
) -> Result<()> {
    let Some(features_obj) = features.as_object() else {
        return Ok(());
    };

    // The environment list first: it needs no I/O, so a run refused by it never
    // pays for a fetch.
    let env = env_entries();
    if !env.is_empty() {
        debug!(entries = ?env, count = features_obj.len(), "checking Features against {ENV_SOURCE}");
        for feature_id in features_obj.keys() {
            if let Some(entry) = env
                .iter()
                .find(|entry| feature_id_covered_by(entry, feature_id))
            {
                return Err(DeaconError::Config(ConfigError::DisallowedFeature {
                    feature_id: feature_id.clone(),
                    matched: entry.clone(),
                    refused_by: ENV_SOURCE.to_string(),
                    documentation_url: None,
                })
                .into());
            }
        }
    }

    let Some(source) = manifest_source else {
        return Ok(());
    };

    // Loaded even when the configuration declares no Features: a broken source
    // should be reported the same way regardless of what it would have matched,
    // rather than being noticed only once someone adds a Feature.
    let manifest = deacon_core::control_manifest::load(source, cache_dir).await?;
    debug!(
        %source,
        disallowed = manifest.disallowed_features.len(),
        advisories = manifest.feature_advisories.len(),
        "loaded control manifest"
    );

    for feature_id in features_obj.keys() {
        if let Some(entry) = manifest.disallowed_entry_for(feature_id) {
            return Err(DeaconError::Config(ConfigError::DisallowedFeature {
                feature_id: feature_id.clone(),
                matched: entry.feature_id_prefix.clone(),
                refused_by: format!("the control manifest at {source}"),
                documentation_url: entry.documentation_url.clone(),
            })
            .into());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn no_manifest() -> (Option<ControlManifestSource>, PathBuf) {
        (None, PathBuf::from("/nonexistent-cache"))
    }

    async fn check(features: serde_json::Value) -> Result<()> {
        let (source, cache) = no_manifest();
        check_for_disallowed_features(&features, source.as_ref(), &cache).await
    }

    #[tokio::test]
    async fn a_versioned_feature_is_blocked_by_its_unversioned_entry() {
        // The regression #675 was filed for: this is the entry an operator writes.
        let rendered = temp_env::async_with_vars(
            [(
                "DEACON_DISALLOWED_FEATURES",
                Some("ghcr.io/devcontainers/features/node"),
            )],
            async {
                check(json!({ "ghcr.io/devcontainers/features/node:1": {} }))
                    .await
                    .expect_err("must be blocked")
                    .to_string()
            },
        )
        .await;
        assert!(
            rendered.contains("ghcr.io/devcontainers/features/node:1"),
            "the diagnostic must name the blocked Feature: {rendered}"
        );
        assert!(
            rendered.contains(ENV_SOURCE),
            "and must name which list refused it: {rendered}"
        );
    }

    #[tokio::test]
    async fn a_neighbouring_feature_is_not_blocked() {
        let result = temp_env::async_with_vars(
            [(
                "DEACON_DISALLOWED_FEATURES",
                Some("ghcr.io/devcontainers/features/node"),
            )],
            async { check(json!({ "ghcr.io/devcontainers/features/nodejs:1": {} })).await },
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn entries_are_trimmed_and_empty_segments_dropped() {
        let (blocked, allowed) = temp_env::async_with_vars(
            [("DEACON_DISALLOWED_FEATURES", Some(" , ghcr.io/x/y , "))],
            async {
                (
                    check(json!({ "ghcr.io/x/y:2": {} })).await,
                    check(json!({ "ghcr.io/a/b:1": {} })).await,
                )
            },
        )
        .await;
        assert!(blocked.is_err(), "a real entry still blocks");
        assert!(allowed.is_ok(), "an empty entry must not block the world");
    }

    #[tokio::test]
    async fn an_unset_variable_and_no_manifest_block_nothing() {
        let result =
            temp_env::async_with_vars([("DEACON_DISALLOWED_FEATURES", None::<&str>)], async {
                check(json!({ "ghcr.io/a/b:1": {} })).await
            })
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn a_non_object_features_value_is_not_a_policy_question() {
        let result = temp_env::async_with_vars(
            [("DEACON_DISALLOWED_FEATURES", Some("ghcr.io/x/y"))],
            async { check(json!(null)).await },
        )
        .await;
        assert!(result.is_ok());
    }

    /// A manifest on disk, so the whole gate is exercised without a network.
    fn manifest_file(body: &str) -> (tempfile::TempDir, ControlManifestSource) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control-manifest.json");
        std::fs::write(&path, body).unwrap();
        (dir, ControlManifestSource::File(path))
    }

    #[tokio::test]
    async fn a_manifest_entry_blocks_by_the_same_prefix_rule() {
        let (dir, source) = manifest_file(
            r#"{"disallowedFeatures":[{"featureIdPrefix":"ghcr.io/devcontainers/features/node",
                 "documentationURL":"https://example.invalid/why"}]}"#,
        );
        let err = check_for_disallowed_features(
            &json!({ "ghcr.io/devcontainers/features/node:1": {} }),
            Some(&source),
            dir.path(),
        )
        .await
        .expect_err("the manifest must block it");
        let rendered = err.to_string();
        assert!(rendered.contains("ghcr.io/devcontainers/features/node:1"));
        assert!(
            rendered.contains("control manifest"),
            "the diagnostic must say WHICH list refused: {rendered}"
        );
        assert!(
            rendered.contains("https://example.invalid/why"),
            "and must carry the documentation URL: {rendered}"
        );
    }

    #[tokio::test]
    async fn a_manifest_that_lists_nothing_relevant_allows_the_run() {
        let (dir, source) =
            manifest_file(r#"{"disallowedFeatures":[{"featureIdPrefix":"ghcr.io/other/thing"}]}"#);
        assert!(
            check_for_disallowed_features(
                &json!({ "ghcr.io/devcontainers/features/node:1": {} }),
                Some(&source),
                dir.path(),
            )
            .await
            .is_ok()
        );
    }

    #[tokio::test]
    async fn a_named_but_missing_manifest_fails_rather_than_allowing_everything() {
        // The failure mode that matters: an operator who asked for a policy must
        // never silently get no policy.
        let dir = tempfile::tempdir().unwrap();
        let source = ControlManifestSource::File(dir.path().join("absent.json"));
        let err = check_for_disallowed_features(
            &json!({ "ghcr.io/a/b:1": {} }),
            Some(&source),
            dir.path(),
        )
        .await
        .expect_err("an unreadable manifest must fail the run");
        assert!(err.to_string().contains("could not be read"));
    }

    #[tokio::test]
    async fn a_broken_manifest_is_reported_even_when_no_feature_is_declared() {
        // Otherwise a typo'd path lies dormant until someone adds a Feature.
        let dir = tempfile::tempdir().unwrap();
        let source = ControlManifestSource::File(dir.path().join("absent.json"));
        assert!(
            check_for_disallowed_features(&json!({}), Some(&source), dir.path())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn the_environment_list_is_consulted_before_the_manifest_is_loaded() {
        // A run the env list already refuses must not pay for — or fail on — a
        // manifest load.
        let dir = tempfile::tempdir().unwrap();
        let source = ControlManifestSource::File(dir.path().join("absent.json"));
        let rendered = temp_env::async_with_vars(
            [("DEACON_DISALLOWED_FEATURES", Some("ghcr.io/a/b"))],
            async {
                check_for_disallowed_features(
                    &json!({ "ghcr.io/a/b:1": {} }),
                    Some(&source),
                    dir.path(),
                )
                .await
                .expect_err("blocked")
                .to_string()
            },
        )
        .await;
        assert!(
            rendered.contains(ENV_SOURCE),
            "the env list must be what refused, not the broken manifest: {rendered}"
        );
    }
}
