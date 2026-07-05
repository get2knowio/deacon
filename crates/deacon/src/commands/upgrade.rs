//! Upgrade subcommand implementation.
//!
//! `deacon upgrade` regenerates `devcontainer-lock.json` from the currently
//! resolved feature set, mirroring upstream `devcontainers/cli`'s `upgrade`
//! command. See the containers.dev spec / reference CLI for the authoritative
//! behavior.
//!
//! ## Scope (PR-5a + PR-5b)
//!
//! - CLI surface (all spec §2 flags)
//! - Argument validation: `--feature` ↔ `--target-version` mutual requirement,
//!   `--target-version` regex `^\d+(\.\d+(\.\d+)?)?$`
//! - **`--feature` / `--target-version` config-pin behavior** (PR-5b):
//!   text-level surgical edit of the matching feature key in
//!   `devcontainer.json`. Preserves comments and whitespace since we never
//!   parse-and-re-emit. Spec §5 phase 2.
//! - Config load via shared `ConfigLoader::load_with_extends`
//! - Feature resolution via the OCI fetcher (fetches manifests to obtain
//!   digests + actual versions)
//! - Lockfile assembly (private helper modeled on PR-4b's
//!   `build_lockfile_from_features` — will be deduplicated in a follow-up)
//! - `--dry-run`: print canonical lockfile JSON to stdout
//! - Default: `write_lockfile(force_init = true)` per spec §5

use anyhow::{Context, Result};
use deacon_core::config::DevContainerConfig;
use deacon_core::lockfile::{Lockfile, LockfileFeature, get_lockfile_path, write_lockfile};
use deacon_core::oci::{DownloadedFeature, FeatureRef, default_fetcher};
use deacon_core::registry_parser::parse_registry_reference;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info, instrument, warn};

use crate::commands::shared::{ConfigLoadArgs, load_config};

/// Arguments for the `upgrade` command. Mirrors the spec's CLI surface
/// (the containers.dev spec / reference CLI).
#[derive(Debug, Clone)]
pub struct UpgradeArgs {
    /// Workspace folder. Required by spec §2; if omitted, defaults to the
    /// current working directory (matches upstream's `getCliHost` behavior
    /// when invoked without `--workspace-folder`).
    pub workspace_folder: Option<PathBuf>,
    /// Optional `--config <PATH>`.
    pub config_path: Option<PathBuf>,
    /// Docker CLI path; default `"docker"`. Spec §2 surface parity only —
    /// upgrade itself does not invoke docker (the OCI fetcher uses HTTP).
    #[allow(dead_code)]
    pub docker_path: String,
    /// Docker Compose CLI path; default `"docker-compose"`. Spec §2 surface
    /// parity only — upgrade does not invoke compose.
    #[allow(dead_code)]
    pub docker_compose_path: String,
    /// When true, print the generated lockfile JSON to stdout instead of
    /// writing to disk. Spec §2 (`--dry-run`).
    pub dry_run: bool,
    /// Hidden: pin the version of a specific feature in `devcontainer.json`
    /// before regenerating the lockfile. Used by Dependabot. Must be set
    /// together with `--target-version`.
    pub feature: Option<String>,
    /// Hidden: target version for `--feature`. Must match
    /// `^\d+(\.\d+(\.\d+)?)?$`.
    pub target_version: Option<String>,
}

/// Execute the `upgrade` command end-to-end.
#[instrument(skip(args))]
pub async fn execute_upgrade(args: UpgradeArgs) -> Result<()> {
    info!("Starting upgrade execution");

    // Phase 1: Fail-fast validation (spec §2/§3).
    validate_pin_flag_pairing(args.feature.as_deref(), args.target_version.as_deref())?;
    if let Some(tv) = args.target_version.as_deref() {
        validate_target_version_format(tv)?;
    }

    // Phase 2: Resolve the workspace + config path via the shared loader.
    // The shared loader returns the resolved config_path so we can derive the
    // lockfile path adjacent to it (spec §6 naming rule).
    let initial = load_config(ConfigLoadArgs {
        workspace_folder: args.workspace_folder.as_deref(),
        config_path: args.config_path.as_deref(),
        override_config_path: None,
        secrets_files: &[],
        resolve_devcontainer_id: true,
    })
    .await?;
    let config_path = initial.config_path.clone();

    // Phase 2.5: Optional config edit (spec §5 phase 2). When both
    // `--feature` and `--target-version` are set, rewrite the matching
    // feature key in `devcontainer.json` in place. The edit is text-level
    // so comments and whitespace are preserved. Per spec, we then re-read
    // the config so the resolution phase sees the pinned form.
    let config = if let (Some(feature), Some(target_version)) =
        (args.feature.as_deref(), args.target_version.as_deref())
    {
        info!(
            "Updating '{}' to '{}' in {}",
            feature,
            target_version,
            config_path.display()
        );
        pin_feature_in_config_file(&config_path, feature, target_version).await?;
        // Re-read the config so downstream resolution sees the pinned tag.
        load_config(ConfigLoadArgs {
            workspace_folder: args.workspace_folder.as_deref(),
            config_path: args.config_path.as_deref(),
            override_config_path: None,
            secrets_files: &[],
            resolve_devcontainer_id: true,
        })
        .await?
        .config
    } else {
        initial.config
    };

    debug!(
        "Loaded configuration from '{}' (features: {:?})",
        config_path.display(),
        config
            .features
            .as_object()
            .map(|m| m.keys().collect::<Vec<_>>())
    );

    // Phase 3: Resolve features against the OCI registry. This is the
    // "regenerate" step — every feature in config is re-fetched so we
    // obtain the current matching digest + version.
    let lockfile = resolve_lockfile_from_config(&config).await?;

    // Phase 4: Output.
    if args.dry_run {
        emit_lockfile_json(&lockfile)?;
        info!("Dry-run: lockfile JSON printed to stdout; nothing written to disk");
        return Ok(());
    }

    let lockfile_path = get_lockfile_path(&config_path);
    // Spec §5 phase 4: force_init = true so the writer always overwrites.
    write_lockfile(&lockfile_path, &lockfile, true)
        .await
        .with_context(|| format!("Failed to write lockfile to '{}'", lockfile_path.display()))?;
    info!("Wrote lockfile to '{}'", lockfile_path.display());
    Ok(())
}

/// Validate that `--feature` and `--target-version` are either both set or
/// both absent (spec §2/§3: mutually constrained).
fn validate_pin_flag_pairing(feature: Option<&str>, target_version: Option<&str>) -> Result<()> {
    match (feature.is_some(), target_version.is_some()) {
        (false, false) | (true, true) => Ok(()),
        _ => Err(anyhow::anyhow!(
            "The '--target-version' and '--feature' flag must be used together."
        )),
    }
}

/// Validate `--target-version` matches the spec-§2 regex
/// `^\d+(\.\d+(\.\d+)?)?$` — accepts `X`, `X.Y`, or `X.Y.Z`.
fn validate_target_version_format(version: &str) -> Result<()> {
    if !is_valid_target_version(version) {
        return Err(anyhow::anyhow!(
            "Invalid version '{}'.  Must be in the form of 'x', 'x.y', or 'x.y.z'",
            version
        ));
    }
    Ok(())
}

/// Pin a feature to a specific version by rewriting its key in
/// `devcontainer.json`. Spec §5 phase 2.
///
/// This is a **text-level surgical edit**: we read the file as a string,
/// find the literal JSON key for `--feature`, and replace it with the
/// pinned form. We never parse-and-re-emit the JSON, so comments and
/// whitespace are preserved — important for hand-maintained
/// `devcontainer.json` files that often carry inline documentation.
///
/// The "matching Feature key" rule (spec §5) is interpreted as an exact
/// match on the `--feature` value as the user provided it. If a user passed
/// `--feature ghcr.io/devcontainers/features/node:1`, we look for the
/// literal `"ghcr.io/devcontainers/features/node:1"` JSON key and replace
/// with `"ghcr.io/devcontainers/features/node:<target_version>"`.
///
/// Errors:
/// - Config file read/write failures bubble up with context.
/// - Missing-key in the JSON text returns an error so the caller knows the
///   `--feature` they specified isn't actually in the config.
/// - Multiple matches return an error to prevent ambiguous edits (a feature
///   ID appearing both as a key and inside a string value, for example).
async fn pin_feature_in_config_file(
    config_path: &std::path::Path,
    feature: &str,
    target_version: &str,
) -> Result<()> {
    let contents = tokio::fs::read_to_string(config_path)
        .await
        .with_context(|| {
            format!(
                "Failed to read devcontainer config from '{}'",
                config_path.display()
            )
        })?;

    let pinned_key = pinned_feature_key(feature, target_version);
    let (updated, occurrences) = rewrite_feature_key(&contents, feature, &pinned_key);

    match occurrences {
        0 => Err(anyhow::anyhow!(
            "Feature '{}' was not found in '{}'. Add it to the config's `features` map before pinning.",
            feature,
            config_path.display()
        )),
        1 => {
            tokio::fs::write(config_path, updated)
                .await
                .with_context(|| {
                    format!(
                        "Failed to write pinned devcontainer config to '{}'",
                        config_path.display()
                    )
                })?;
            debug!(
                feature = %feature,
                target_version = %target_version,
                "Pinned feature key in {}",
                config_path.display()
            );
            Ok(())
        }
        n => Err(anyhow::anyhow!(
            "Feature '{}' appears {} times in '{}' (ambiguous edit). \
             Resolve manually before re-running upgrade with --feature/--target-version.",
            feature,
            n,
            config_path.display()
        )),
    }
}

/// Compute the pinned form of a feature key.
///
/// Strips any existing `:<tag>` suffix from `feature` and re-appends
/// `:<target_version>`. Features without a tag get one appended.
///
/// Naive `rsplit(':')` is the right tool here: registry ports (e.g.
/// `myregistry.io:5000/...`) live before the first `/`, while the tag is
/// the segment after the LAST `:` AND with no `/` after it. The exact rule:
/// the tag separator is the last `:` that occurs after the last `/`.
fn pinned_feature_key(feature: &str, target_version: &str) -> String {
    let base = match (feature.rfind(':'), feature.rfind('/')) {
        (Some(colon), Some(slash)) if colon > slash => &feature[..colon],
        (Some(colon), None) => &feature[..colon],
        _ => feature,
    };
    format!("{}:{}", base, target_version)
}

/// Surgically rewrite the JSON key `"<feature>"` to `"<pinned>"` in
/// `contents`. Returns the rewritten text and the number of replacements.
///
/// We match against the literal quoted form (`"<feature>"`) so we don't
/// accidentally substitute matching substrings inside string *values*
/// (only a JSON key would be both fully quoted and followed by `:` —
/// but the leading `"` is enough to be precise without parsing).
fn rewrite_feature_key(contents: &str, feature: &str, pinned: &str) -> (String, usize) {
    let needle = format!("\"{}\"", feature);
    let replacement = format!("\"{}\"", pinned);
    let occurrences = contents.matches(&needle).count();
    (contents.replace(&needle, &replacement), occurrences)
}

/// Pure predicate that mirrors the spec-§2 regex. Hand-rolled to avoid
/// pulling `regex` into the call site (we already depend on it transitively,
/// but the check is trivial enough to inline).
fn is_valid_target_version(version: &str) -> bool {
    if version.is_empty() {
        return false;
    }
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() > 3 {
        return false;
    }
    parts
        .iter()
        .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Re-fetch every feature in the config and assemble the lockfile.
///
/// Each feature is parsed from its user-provided ID, looked up via the OCI
/// fetcher (which resolves tags to concrete manifests + digests), and
/// converted into a `LockfileFeature` entry keyed by the user-provided ID
/// per upstream `generateLockfile`.
async fn resolve_lockfile_from_config(config: &DevContainerConfig) -> Result<Lockfile> {
    let features_obj = match config.features.as_object() {
        Some(obj) => obj,
        // Per spec §5: an empty/missing features map is not an error;
        // upgrade just produces an empty lockfile.
        None => {
            return Ok(Lockfile {
                features: HashMap::new(),
            });
        }
    };

    if features_obj.is_empty() {
        return Ok(Lockfile {
            features: HashMap::new(),
        });
    }

    let fetcher = default_fetcher().context("Failed to construct OCI fetcher for upgrade")?;
    let mut entries: HashMap<String, LockfileFeature> = HashMap::new();

    for (user_id, _opts) in features_obj.iter() {
        // Per #126 — skip local features. Lockfile entries are OCI-only
        // (`features_build.rs::build_lockfile_from_features` drops local
        // features by design: there's no OCI identity to record). Without
        // this gate, `upgrade` would try to OCI-fetch `./minimal-feature`
        // and die with "Invalid feature ID".
        if user_id.starts_with("./") || user_id.starts_with("../") || user_id.starts_with('/') {
            debug!(feature = %user_id, "Skipping local feature (no OCI identity to lock)");
            continue;
        }

        let (registry, namespace, name, tag) = parse_registry_reference(user_id)
            .with_context(|| format!("Invalid feature ID '{}'", user_id))?;
        let feature_ref = FeatureRef::new(registry, namespace, name, tag);

        debug!(feature = %user_id, "Re-resolving feature against OCI registry");
        let downloaded: DownloadedFeature = fetcher
            .fetch_feature(&feature_ref)
            .await
            .with_context(|| format!("Failed to fetch feature '{}' from OCI registry", user_id))?;

        let entry = lockfile_entry_for(&feature_ref, &downloaded, user_id);
        entries.insert(user_id.clone(), entry);
    }

    Ok(Lockfile { features: entries })
}

/// Build a single `LockfileFeature` from a resolved feature reference +
/// download. Mirrors PR-4b's `build_lockfile_from_features` entry logic
/// (duplicated here intentionally so PR-5 doesn't depend on PR-4b's diff;
/// will be deduplicated once both land on main).
fn lockfile_entry_for(
    feature_ref: &FeatureRef,
    downloaded: &DownloadedFeature,
    user_id: &str,
) -> LockfileFeature {
    let version = match &downloaded.metadata.version {
        Some(v) if !v.is_empty() => v.clone(),
        _ => {
            let fallback = feature_ref.tag();
            warn!(
                feature = %user_id,
                fallback = %fallback,
                "Feature metadata has no version field; using tag as fallback for lockfile entry"
            );
            fallback.to_string()
        }
    };

    let depends_on = if downloaded.metadata.depends_on.is_empty() {
        None
    } else {
        let mut deps: Vec<String> = downloaded.metadata.depends_on.keys().cloned().collect();
        deps.sort();
        Some(deps)
    };

    LockfileFeature::from_resolved(
        &feature_ref.registry,
        &feature_ref.repository(),
        &downloaded.manifest_digest,
        version,
        depends_on,
    )
}

/// Print the lockfile to stdout as canonical JSON.
///
/// Format mirrors `deacon_core::lockfile::write_lockfile`: serde-derived
/// JSON, recursively sorted object keys, 2-space pretty-printed, with a
/// trailing newline. A `--dry-run` consumer should be able to redirect this
/// directly into the lockfile file and get the same bytes as a non-dry-run.
fn emit_lockfile_json(lockfile: &Lockfile) -> Result<()> {
    let mut value =
        serde_json::to_value(lockfile).context("Failed to serialize lockfile to JSON value")?;
    sort_json_object(&mut value);
    let json =
        serde_json::to_string_pretty(&value).context("Failed to serialize lockfile to JSON")?;
    println!("{}", json);
    Ok(())
}

/// Recursively sort all keys in a JSON object for deterministic output.
/// Kept private here to mirror `deacon_core::lockfile`'s private helper
/// (same logic; both produce identical orderings).
fn sort_json_object(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            let sorted: std::collections::BTreeMap<_, _> = map.iter().collect();
            *map = sorted
                .into_iter()
                .map(|(k, v)| {
                    let mut v = v.clone();
                    sort_json_object(&mut v);
                    (k.clone(), v)
                })
                .collect();
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                sort_json_object(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::features::FeatureMetadata;
    use std::path::PathBuf;

    fn make_downloaded(version: Option<&str>, digest: &str) -> DownloadedFeature {
        DownloadedFeature {
            path: PathBuf::from("/tmp/unused"),
            metadata: FeatureMetadata {
                id: "node".to_string(),
                version: version.map(|s| s.to_string()),
                ..FeatureMetadata::default()
            },
            digest: digest.to_string(),
            manifest_digest: digest.to_string(),
        }
    }

    fn make_feature_ref() -> FeatureRef {
        FeatureRef::new(
            "ghcr.io".to_string(),
            "devcontainers".to_string(),
            "node".to_string(),
            Some("1".to_string()),
        )
    }

    // =========================================================================
    // Validation
    // =========================================================================

    #[test]
    fn validate_pin_pairing_accepts_both_set() {
        assert!(validate_pin_flag_pairing(Some("node"), Some("1.2.3")).is_ok());
    }

    #[test]
    fn validate_pin_pairing_accepts_both_absent() {
        assert!(validate_pin_flag_pairing(None, None).is_ok());
    }

    #[test]
    fn validate_pin_pairing_rejects_only_feature() {
        let err = validate_pin_flag_pairing(Some("node"), None).unwrap_err();
        assert!(
            err.to_string().contains("must be used together"),
            "got: {err}"
        );
    }

    #[test]
    fn validate_pin_pairing_rejects_only_target_version() {
        let err = validate_pin_flag_pairing(None, Some("1.2.3")).unwrap_err();
        assert!(err.to_string().contains("must be used together"));
    }

    #[test]
    fn is_valid_target_version_accepts_x_xy_xyz() {
        // Spec §2: `--target-version` regex `^\d+(\.\d+(\.\d+)?)?$`.
        assert!(is_valid_target_version("1"));
        assert!(is_valid_target_version("1.2"));
        assert!(is_valid_target_version("1.2.3"));
        assert!(is_valid_target_version("10"));
        assert!(is_valid_target_version("10.20.30"));
    }

    #[test]
    fn is_valid_target_version_rejects_non_numeric_and_extras() {
        assert!(!is_valid_target_version(""));
        assert!(!is_valid_target_version("v1"));
        assert!(!is_valid_target_version("1.2.3.4"));
        assert!(!is_valid_target_version("1."));
        assert!(!is_valid_target_version(".1"));
        assert!(!is_valid_target_version("1.x.3"));
        assert!(!is_valid_target_version("1.2-beta"));
        assert!(!is_valid_target_version("latest"));
    }

    #[test]
    fn validate_target_version_format_surfaces_spec_message() {
        let err = validate_target_version_format("v1").unwrap_err();
        // Spec §2 mandates this exact summary string so existing CI
        // scripts that grep for it keep working.
        assert!(err.to_string().contains("Invalid version 'v1'"));
        assert!(
            err.to_string()
                .contains("Must be in the form of 'x', 'x.y', or 'x.y.z'")
        );
    }

    // =========================================================================
    // Lockfile entry assembly
    // =========================================================================

    #[test]
    fn lockfile_entry_uses_metadata_version_when_present() {
        let feature_ref = make_feature_ref();
        let downloaded = make_downloaded(
            Some("1.6.1"),
            "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        );
        let entry = lockfile_entry_for(&feature_ref, &downloaded, "ghcr.io/devcontainers/node:1");

        assert_eq!(entry.version, "1.6.1");
        assert_eq!(
            entry.resolved,
            "ghcr.io/devcontainers/node@sha256:1111111111111111111111111111111111111111111111111111111111111111"
        );
        assert_eq!(
            entry.integrity,
            "sha256:1111111111111111111111111111111111111111111111111111111111111111"
        );
        assert!(entry.depends_on.is_none());
    }

    #[test]
    fn lockfile_entry_falls_back_to_tag_when_metadata_version_missing() {
        // Mirrors PR-4b's fallback so the two helpers produce identical
        // entries for the same input — important for the eventual dedup.
        let feature_ref = make_feature_ref();
        let downloaded = make_downloaded(
            None,
            "sha256:2222222222222222222222222222222222222222222222222222222222222222",
        );
        let entry = lockfile_entry_for(&feature_ref, &downloaded, "ghcr.io/devcontainers/node:1");
        assert_eq!(entry.version, "1");
    }

    #[test]
    fn lockfile_entry_sorts_depends_on() {
        let feature_ref = make_feature_ref();
        let mut downloaded = make_downloaded(
            Some("1.0.0"),
            "sha256:3333333333333333333333333333333333333333333333333333333333333333",
        );
        downloaded
            .metadata
            .depends_on
            .insert("zeta".to_string(), serde_json::Value::Bool(true));
        downloaded
            .metadata
            .depends_on
            .insert("alpha".to_string(), serde_json::Value::Bool(true));

        let entry = lockfile_entry_for(&feature_ref, &downloaded, "ghcr.io/devcontainers/node:1");
        assert_eq!(
            entry.depends_on.as_deref(),
            Some(&["alpha".to_string(), "zeta".to_string()][..])
        );
    }

    // =========================================================================
    // Empty config short-circuit (no network needed)
    // =========================================================================

    #[tokio::test]
    async fn resolve_lockfile_returns_empty_for_no_features() {
        let config = DevContainerConfig::default();
        let lockfile = resolve_lockfile_from_config(&config).await.unwrap();
        assert!(lockfile.features.is_empty());
    }

    #[tokio::test]
    async fn resolve_lockfile_skips_local_features() {
        // Per #126: `upgrade` must not OCI-fetch local-path features.
        // Pre-fix, ./, ../, and /abs/path entries flunked
        // `parse_registry_reference` with "Invalid feature ID".
        let config = DevContainerConfig {
            features: serde_json::json!({
                "./local-feature": {},
                "../shared/another-local": {},
                "/abs/path/feature": {},
            }),
            ..DevContainerConfig::default()
        };
        let lockfile = resolve_lockfile_from_config(&config).await.unwrap();
        // All local features dropped (no OCI identity to lock); no entries.
        assert!(lockfile.features.is_empty());
    }

    #[tokio::test]
    async fn resolve_lockfile_returns_empty_for_empty_features_object() {
        let config = DevContainerConfig {
            features: serde_json::json!({}),
            ..DevContainerConfig::default()
        };
        let lockfile = resolve_lockfile_from_config(&config).await.unwrap();
        assert!(lockfile.features.is_empty());
    }

    // =========================================================================
    // Args defaults
    // =========================================================================

    // =========================================================================
    // PR-5b: pin behavior — pure helpers
    // =========================================================================

    #[test]
    fn pinned_feature_key_replaces_existing_tag() {
        // Common case: tag-suffixed user ID gets its tag replaced.
        assert_eq!(
            pinned_feature_key("ghcr.io/devcontainers/features/node:1", "1.2.3"),
            "ghcr.io/devcontainers/features/node:1.2.3"
        );
    }

    #[test]
    fn pinned_feature_key_appends_tag_when_absent() {
        // No `:` in the user ID → append `:<target_version>`. This handles
        // the rare case of an unversioned reference (which would otherwise
        // default to `latest` and is brittle for reproducible builds).
        assert_eq!(
            pinned_feature_key("ghcr.io/devcontainers/features/node", "1.2.3"),
            "ghcr.io/devcontainers/features/node:1.2.3"
        );
    }

    #[test]
    fn pinned_feature_key_preserves_registry_port() {
        // Registry hostnames may carry a port (e.g. `myregistry.io:5000/...`).
        // The port colon lives BEFORE the last `/`, so it must NOT be treated
        // as the tag separator.
        assert_eq!(
            pinned_feature_key("myregistry.io:5000/features/node:1", "1.2.3"),
            "myregistry.io:5000/features/node:1.2.3"
        );
        // And the port-only case (no tag) should still get the version appended.
        assert_eq!(
            pinned_feature_key("myregistry.io:5000/features/node", "1.2.3"),
            "myregistry.io:5000/features/node:1.2.3"
        );
    }

    #[test]
    fn rewrite_feature_key_replaces_single_match() {
        let contents = r#"{
  "features": {
    "ghcr.io/devcontainers/features/node:1": {}
  }
}"#;
        let (out, n) = rewrite_feature_key(
            contents,
            "ghcr.io/devcontainers/features/node:1",
            "ghcr.io/devcontainers/features/node:1.2.3",
        );
        assert_eq!(n, 1);
        assert!(out.contains("\"ghcr.io/devcontainers/features/node:1.2.3\""));
        assert!(!out.contains("\"ghcr.io/devcontainers/features/node:1\":"));
    }

    #[test]
    fn rewrite_feature_key_preserves_unrelated_keys() {
        // Sibling features in the same map must not be touched.
        let contents = r#"{
  "features": {
    "ghcr.io/devcontainers/features/node:1": {},
    "ghcr.io/devcontainers/features/go:1": {}
  }
}"#;
        let (out, _) = rewrite_feature_key(
            contents,
            "ghcr.io/devcontainers/features/node:1",
            "ghcr.io/devcontainers/features/node:1.2.3",
        );
        assert!(out.contains("\"ghcr.io/devcontainers/features/node:1.2.3\""));
        // The go feature must be untouched.
        assert!(out.contains("\"ghcr.io/devcontainers/features/go:1\""));
    }

    #[test]
    fn rewrite_feature_key_reports_zero_for_missing_feature() {
        let contents = r#"{ "features": {} }"#;
        let (out, n) = rewrite_feature_key(
            contents,
            "ghcr.io/devcontainers/features/node:1",
            "ghcr.io/devcontainers/features/node:1.2.3",
        );
        assert_eq!(n, 0);
        assert_eq!(out, contents, "no-op when nothing to replace");
    }

    #[test]
    fn rewrite_feature_key_reports_ambiguous_when_multiple_matches() {
        // The literal quoted form appears twice — once as a JSON key, once
        // as an unescaped string value. We refuse to edit because picking
        // one is ambiguous (the second occurrence is JSON-valid: the
        // outer `"`s of the value are the same `"`s that bracket our
        // needle, so the substring match catches it).
        let contents = r#"{
  "features": {
    "ghcr.io/devcontainers/features/node:1": {}
  },
  "originalFeature": "ghcr.io/devcontainers/features/node:1"
}"#;
        let (_out, n) = rewrite_feature_key(
            contents,
            "ghcr.io/devcontainers/features/node:1",
            "ghcr.io/devcontainers/features/node:1.2.3",
        );
        assert_eq!(n, 2, "must detect the ambiguity for caller error reporting");
    }

    #[test]
    fn rewrite_feature_key_ignores_escaped_quoted_substring() {
        // A JSON value containing the feature ID with escaped quotes is a
        // *different* byte sequence (`\"` vs `"`), so the substring search
        // correctly skips it. This documents the robustness of the
        // text-level edit against the common "feature id mentioned in a
        // comment-as-value" pattern.
        let contents = r#"{
  "features": {
    "ghcr.io/devcontainers/features/node:1": {}
  },
  "note": "we use \"ghcr.io/devcontainers/features/node:1\" here"
}"#;
        let (_out, n) = rewrite_feature_key(
            contents,
            "ghcr.io/devcontainers/features/node:1",
            "ghcr.io/devcontainers/features/node:1.2.3",
        );
        assert_eq!(
            n, 1,
            "escaped-quote occurrence inside a string value must not count as a match"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pin_feature_in_config_file_round_trips_through_disk() {
        // End-to-end sanity check: write a fixture, pin a feature, read back.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("devcontainer.json");
        let original = r#"{
  "name": "test",
  "image": "alpine:3.18",
  // comment that must survive
  "features": {
    "ghcr.io/devcontainers/features/node:1": { "version": "lts" }
  }
}"#;
        std::fs::write(&path, original).unwrap();

        pin_feature_in_config_file(&path, "ghcr.io/devcontainers/features/node:1", "1.2.3")
            .await
            .unwrap();

        let updated = std::fs::read_to_string(&path).unwrap();
        // Key rewritten.
        assert!(updated.contains("\"ghcr.io/devcontainers/features/node:1.2.3\""));
        // Comment preserved (this is the whole point of text-level editing).
        assert!(updated.contains("// comment that must survive"));
        // Other content preserved.
        assert!(updated.contains("\"version\": \"lts\""));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pin_feature_in_config_file_errors_when_feature_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("devcontainer.json");
        std::fs::write(&path, r#"{ "features": {} }"#).unwrap();

        let err =
            pin_feature_in_config_file(&path, "ghcr.io/devcontainers/features/node:1", "1.2.3")
                .await
                .unwrap_err();
        assert!(err.to_string().contains("was not found"), "got: {err}");
    }

    #[test]
    fn upgrade_args_defaults_are_sensible() {
        let args = UpgradeArgs {
            workspace_folder: None,
            config_path: None,
            docker_path: "docker".to_string(),
            docker_compose_path: "docker-compose".to_string(),
            dry_run: false,
            feature: None,
            target_version: None,
        };
        assert!(!args.dry_run);
        assert!(args.feature.is_none());
        assert!(args.target_version.is_none());
    }
}
