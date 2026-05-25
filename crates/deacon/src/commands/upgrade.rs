//! Upgrade subcommand implementation.
//!
//! `deacon upgrade` regenerates `devcontainer-lock.json` from the currently
//! resolved feature set, mirroring upstream `devcontainers/cli`'s `upgrade`
//! command. See `docs/subcommand-specs/upgrade/SPEC.md` for the authoritative
//! behavior.
//!
//! ## PR-5a scope (MVP)
//!
//! - CLI surface (all spec §2 flags accepted)
//! - Argument validation: `--feature` ↔ `--target-version` mutual requirement,
//!   `--target-version` regex `^\d+(\.\d+(\.\d+)?)?$`
//! - Config load via shared `ConfigLoader::load_with_extends`
//! - Feature resolution via the OCI fetcher (fetches manifests to obtain
//!   digests + actual versions)
//! - Lockfile assembly (private helper modeled on PR-4b's
//!   `build_lockfile_from_features` — will be deduplicated in a follow-up)
//! - `--dry-run`: print canonical lockfile JSON to stdout
//! - Default: `write_lockfile(force_init = true)` per spec §5
//!
//! ## Deferred to PR-5b
//!
//! - `--feature` / `--target-version` config-pin behavior (modifies
//!   `devcontainer.json` in place). The flags are accepted today so the CLI
//!   surface is stable, but using them returns
//!   `"--feature/--target-version pinning is not yet implemented (PR-5b)"`
//!   instead of silently doing nothing.

use anyhow::{Context, Result};
use deacon_core::config::DevContainerConfig;
use deacon_core::lockfile::{get_lockfile_path, write_lockfile, Lockfile, LockfileFeature};
use deacon_core::oci::{default_fetcher, DownloadedFeature, FeatureRef};
use deacon_core::registry_parser::parse_registry_reference;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info, instrument, warn};

use crate::commands::shared::{load_config, ConfigLoadArgs, ConfigLoadResult};

/// Arguments for the `upgrade` command. Mirrors the spec's CLI surface
/// (`docs/subcommand-specs/upgrade/SPEC.md` §2).
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
    /// together with `--target-version`. **Deferred to PR-5b.**
    pub feature: Option<String>,
    /// Hidden: target version for `--feature`. Must match
    /// `^\d+(\.\d+(\.\d+)?)?$`. **Deferred to PR-5b.**
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
    if args.feature.is_some() {
        // Surface-parity placeholder: accept the flag combination (so callers
        // can validate their wiring) but bail before touching devcontainer.json.
        return Err(anyhow::anyhow!(
            "--feature/--target-version pinning is not yet implemented (PR-5b)"
        ));
    }

    // Phase 2: Resolve the workspace + config path via the shared loader.
    // The shared loader returns the resolved config_path so we can derive the
    // lockfile path adjacent to it (spec §6 naming rule).
    let ConfigLoadResult {
        config,
        config_path,
        ..
    } = load_config(ConfigLoadArgs {
        workspace_folder: args.workspace_folder.as_deref(),
        config_path: args.config_path.as_deref(),
        override_config_path: None,
        secrets_files: &[],
    })?;

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
            })
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
        &downloaded.digest,
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
        assert!(err
            .to_string()
            .contains("Must be in the form of 'x', 'x.y', or 'x.y.z'"));
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
