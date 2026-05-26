//! Utility functions for the up command.
//!
//! This module contains:
//! - `check_for_disallowed_features` - Check for disallowed features
//! - `discover_id_labels_from_config` - Discover id-labels from configuration
//! - `apply_user_mapping` - Apply user mapping configuration
//! - `handle_lockfile_post_build` - Write/compare lockfile after a feature build

use anyhow::{Context, Result};
use deacon_core::config::DevContainerConfig;
use deacon_core::errors::DeaconError;
use deacon_core::lockfile::{get_lockfile_path, write_lockfile, Lockfile};
use std::io;
use std::path::Path;
use tracing::{debug, info, instrument, warn};

use super::args::UpArgs;

/// Linux `EROFS` errno value — "Read-only file system". Used in the
/// best-effort lockfile write path because `io::ErrorKind::ReadOnlyFilesystem`
/// is unavailable on MSRV 1.82 (stabilized in 1.83).
#[cfg(unix)]
const EROFS: i32 = 30;

/// Check if any features are disallowed and return an error if found.
///
/// Per FR-004: Configuration resolution MUST block disallowed Features.
///
/// This function checks features against a policy-defined list of disallowed features.
/// The disallowed list can be:
/// - Statically defined (DISALLOWED_FEATURES constant)
/// - Loaded from environment variable DEACON_DISALLOWED_FEATURES (comma-separated)
/// - Extended by policy enforcement systems
///
/// Returns Ok(()) if no disallowed features are found, or an error with the
/// disallowed feature ID if one is detected.
pub(crate) fn check_for_disallowed_features(features: &serde_json::Value) -> Result<()> {
    // Static list of disallowed features (currently empty - can be extended as needed)
    const DISALLOWED_FEATURES: &[&str] = &[];

    // Check for environment-based disallowed features
    let env_disallowed: Vec<String> = std::env::var("DEACON_DISALLOWED_FEATURES")
        .ok()
        .map(|s| s.split(',').map(|f| f.trim().to_string()).collect())
        .unwrap_or_default();

    debug!("Checking features against disallowed list");
    debug!("Static disallowed features: {:?}", DISALLOWED_FEATURES);
    debug!("Environment disallowed features: {:?}", env_disallowed);

    if let Some(features_obj) = features.as_object() {
        for (feature_id, _) in features_obj {
            // Check against static list
            if DISALLOWED_FEATURES.contains(&feature_id.as_str()) {
                return Err(
                    DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                        message: format!("Feature '{}' is not allowed by policy", feature_id),
                    })
                    .into(),
                );
            }

            // Check against environment list
            if env_disallowed.contains(feature_id) {
                return Err(
                    DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                        message: format!(
                            "Feature '{}' is disallowed by DEACON_DISALLOWED_FEATURES",
                            feature_id
                        ),
                    })
                    .into(),
                );
            }

            debug!("Validated feature: {}", feature_id);
        }
    }

    Ok(())
}

/// Discover id-labels from configuration when not explicitly provided via CLI.
///
/// Per FR-004: Configuration resolution MUST discover id labels when not provided.
///
/// ID labels are used to uniquely identify containers for reconnection scenarios.
/// When not provided via --id-label flags, they can be derived from:
/// - Configuration metadata
/// - Workspace folder path
/// - Container name from config
///
/// Returns a list of (name, value) tuples representing discovered labels.
pub(crate) fn discover_id_labels_from_config(
    provided_labels: &[(String, String)],
    workspace_folder: &Path,
    config: &DevContainerConfig,
) -> Vec<(String, String)> {
    // If labels were provided via CLI, use those
    if !provided_labels.is_empty() {
        debug!("Using provided id-labels: {:?}", provided_labels);
        return provided_labels.to_vec();
    }

    // Otherwise, discover labels from context
    let mut labels = Vec::new();

    // Add workspace folder as a label (standard devcontainer practice)
    if let Ok(canonical_path) = workspace_folder.canonicalize() {
        labels.push((
            "devcontainer.local_folder".to_string(),
            canonical_path.to_string_lossy().to_string(),
        ));
        debug!(
            "Discovered id-label from workspace: devcontainer.local_folder={}",
            canonical_path.display()
        );
    }

    // Add config name as a label if available
    if let Some(name) = &config.name {
        labels.push(("devcontainer.config_name".to_string(), name.clone()));
        debug!(
            "Discovered id-label from config: devcontainer.config_name={}",
            name
        );
    }

    labels
}

/// Apply user mapping configuration to the container.
///
/// When `updateRemoteUserUID` is enabled and a `remoteUser` is configured, this function
/// executes the full user mapping workflow inside the running container:
/// 1. Creates the remote user if it doesn't exist
/// 2. Updates UID/GID to match the host user
/// 3. Sets up the home directory
/// 4. Adjusts workspace ownership
#[instrument(skip(runtime, config))]
pub(crate) async fn apply_user_mapping<R: deacon_core::docker::Docker + Send + Sync>(
    runtime: &R,
    container_id: &str,
    config: &DevContainerConfig,
    workspace_folder: &Path,
) -> Result<()> {
    use deacon_core::user_mapping::{
        get_host_user_info, DockerUserMapper, UserMappingConfig, UserMappingService,
    };

    debug!("Applying user mapping configuration");

    // Create user mapping configuration
    let mut user_config = UserMappingConfig::new(
        config.remote_user.clone(),
        config.container_user.clone(),
        config.update_remote_user_uid.unwrap_or(false),
    );

    // Add host user information if updateRemoteUserUID is enabled
    if user_config.update_remote_user_uid {
        match get_host_user_info().await {
            Ok((uid, gid)) => {
                if uid == 0 {
                    debug!("Host user is root (UID 0), skipping UID mapping");
                    user_config.update_remote_user_uid = false;
                } else {
                    user_config = user_config.with_host_user(uid, gid);
                    debug!("Host user: UID={}, GID={}", uid, gid);
                }
            }
            Err(e) => {
                warn!("Failed to get host user info, skipping UID mapping: {}", e);
            }
        }
    }

    // Set workspace path for ownership adjustments
    if let Some(container_workspace_folder) = &config.workspace_folder {
        user_config = user_config.with_workspace_path(container_workspace_folder.clone());
    } else {
        // Default container workspace folder
        let workspace_name = workspace_folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace");
        user_config = user_config.with_workspace_path(format!("/workspaces/{}", workspace_name));
    }

    // Execute user mapping via UserMappingService
    if user_config.needs_user_mapping() {
        debug!(
            "User mapping required: remote_user={:?}, update_uid={}, workspace={}",
            user_config.remote_user,
            user_config.update_remote_user_uid,
            user_config
                .workspace_path
                .as_ref()
                .unwrap_or(&"<none>".to_string())
        );

        let mapper = DockerUserMapper::new(runtime);
        let service = UserMappingService::new(mapper);
        let result = service.apply_user_mapping(container_id, &user_config).await;

        match result {
            Ok(mapping_result) => {
                debug!(
                    "User mapping applied: user={}, uid={}, gid={}, created={}, uid_updated={}, home_created={}, workspace_adjusted={}",
                    mapping_result.user_info.username,
                    mapping_result.user_info.uid,
                    mapping_result.user_info.gid,
                    mapping_result.user_created,
                    mapping_result.uid_updated,
                    mapping_result.home_created,
                    mapping_result.workspace_ownership_adjusted,
                );
            }
            Err(e) => {
                warn!("User mapping failed (non-fatal): {}", e);
            }
        }
    }

    // Log security options (applied during container creation, not here)
    if config.privileged.unwrap_or(false) {
        debug!("Container will run in privileged mode");
    }
    if !config.cap_add.is_empty() {
        debug!("Container capabilities to add: {:?}", config.cap_add);
    }
    if !config.security_opt.is_empty() {
        debug!("Container security options: {:?}", config.security_opt);
    }

    Ok(())
}

/// Apply the lockfile policy after a feature build completes.
///
/// Dispatches on the CLI flags (`--no-lockfile`, `--frozen-lockfile`,
/// deprecated `--experimental-lockfile <PATH>`):
///
/// - `--no-lockfile`: skip entirely.
/// - `--frozen-lockfile` (or `--experimental-lockfile` set): serialize the
///   freshly-built lockfile, byte-compare it to the on-disk file, and fail
///   with the upstream-aligned `"Lockfile does not match."` /
///   `"Lockfile does not exist."` strings if they differ.
/// - Default: write the freshly-built lockfile to disk.
///
/// On read-only workspaces (EROFS/EACCES on write), emit a WARN and continue
/// so a read-only mount doesn't break `up`. Frozen mode never reaches this
/// branch — it only reads — so this fallback is write-side only.
///
/// Mirrors upstream `writeLockfile` in `devcontainers/cli`
/// `src/spec-configuration/lockfile.ts` (`PR #1212`).
pub(crate) fn handle_lockfile_post_build(
    args: &UpArgs,
    config_path: &Path,
    lockfile: &Lockfile,
) -> Result<()> {
    if args.no_lockfile {
        debug!("--no-lockfile set; skipping lockfile write/compare");
        return Ok(());
    }

    let lockfile_path = get_lockfile_path(config_path);

    if args.frozen_lockfile {
        compare_lockfile_frozen(&lockfile_path, lockfile)
    } else {
        write_lockfile_best_effort(&lockfile_path, lockfile)
    }
}

/// Frozen-mode comparison: serialize the in-memory lockfile to the same
/// canonical byte form `write_lockfile` would emit, then compare byte-for-byte
/// with the on-disk file. Any deviation (missing file, mismatched bytes)
/// fails the build with the upstream-aligned summary string.
fn compare_lockfile_frozen(lockfile_path: &Path, lockfile: &Lockfile) -> Result<()> {
    if !lockfile_path.exists() {
        return Err(
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: format!(
                    "Lockfile does not exist.\nExpected at '{}'.\n\
                     Run without --frozen-lockfile to generate a lockfile, or \
                     generate one with `deacon upgrade`.",
                    lockfile_path.display()
                ),
            })
            .into(),
        );
    }

    let expected_bytes = canonical_lockfile_bytes(lockfile)?;
    let actual_bytes = std::fs::read(lockfile_path).with_context(|| {
        format!(
            "Failed to read existing lockfile at '{}'",
            lockfile_path.display()
        )
    })?;

    if expected_bytes != actual_bytes {
        return Err(
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: format!(
                    "Lockfile does not match.\n\
                     The on-disk lockfile at '{}' differs from the freshly-resolved feature set.\n\
                     Run without --frozen-lockfile to update the lockfile, or run `deacon upgrade`.",
                    lockfile_path.display()
                ),
            })
            .into(),
        );
    }

    info!(
        "Lockfile up-to-date: '{}' matches the resolved feature set",
        lockfile_path.display()
    );
    Ok(())
}

/// Best-effort write: succeeds normally, but downgrades EROFS/EACCES to WARN
/// so a read-only workspace (e.g. CI mount, container with a read-only volume)
/// doesn't break `up`. All other write errors propagate.
fn write_lockfile_best_effort(lockfile_path: &Path, lockfile: &Lockfile) -> Result<()> {
    match write_lockfile(lockfile_path, lockfile, true) {
        Ok(()) => {
            debug!("Wrote lockfile to '{}'", lockfile_path.display());
            Ok(())
        }
        Err(e) => {
            let e = anyhow::Error::from(e);
            if is_readonly_fs_error(&e) {
                warn!(
                    path = %lockfile_path.display(),
                    error = %e,
                    "Lockfile write skipped (read-only workspace); continuing without persisting lockfile"
                );
                Ok(())
            } else {
                Err(e).with_context(|| {
                    format!("Failed to write lockfile to '{}'", lockfile_path.display())
                })
            }
        }
    }
}

/// Inspect an anyhow error chain for an `io::Error` whose kind indicates a
/// read-only / permission-denied filesystem.
///
/// `EACCES` surfaces as `io::ErrorKind::PermissionDenied`. `EROFS` is checked
/// via `raw_os_error()` because the dedicated `ErrorKind::ReadOnlyFilesystem`
/// variant was stabilized in Rust 1.83 and our MSRV is 1.82.
fn is_readonly_fs_error(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        let Some(io_err) = cause.downcast_ref::<io::Error>() else {
            return false;
        };
        if io_err.kind() == io::ErrorKind::PermissionDenied {
            return true;
        }
        #[cfg(unix)]
        {
            if io_err.raw_os_error() == Some(EROFS) {
                return true;
            }
        }
        false
    })
}

/// Canonical lockfile bytes — exactly what `write_lockfile` would put on disk.
///
/// We can't ask `write_lockfile` for the bytes directly (it writes through to
/// the filesystem), so we replay the same shape: `serde_json::to_value`,
/// recursively sort object keys, pretty-print with 2-space indent, append a
/// trailing newline. Any change to the on-disk format must update both this
/// helper and `deacon_core::lockfile::write_lockfile` in lockstep, otherwise
/// `--frozen-lockfile` will report spurious mismatches.
fn canonical_lockfile_bytes(lockfile: &Lockfile) -> Result<Vec<u8>> {
    let mut value =
        serde_json::to_value(lockfile).context("Failed to serialize lockfile to JSON value")?;
    sort_json_object(&mut value);
    let mut json =
        serde_json::to_string_pretty(&value).context("Failed to serialize lockfile to JSON")?;
    json.push('\n');
    Ok(json.into_bytes())
}

/// Recursively sort all keys in a JSON object for deterministic output.
///
/// Kept private to this module to mirror the private helper inside
/// `deacon_core::lockfile`; both produce identical orderings.
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
mod lockfile_post_build_tests {
    use super::*;
    use deacon_core::lockfile::{read_lockfile, LockfileFeature};
    use std::collections::HashMap;
    use tempfile::TempDir;

    /// Build a lockfile with a single deterministic entry for assertions.
    fn one_feature_lockfile(version: &str, digest_hex: &str) -> Lockfile {
        let mut features = HashMap::new();
        features.insert(
            "ghcr.io/devcontainers/features/node:1".to_string(),
            LockfileFeature {
                version: version.to_string(),
                resolved: format!("ghcr.io/devcontainers/features/node@sha256:{}", digest_hex),
                integrity: format!("sha256:{}", digest_hex),
                depends_on: None,
            },
        );
        Lockfile { features }
    }

    fn make_args(no_lockfile: bool, frozen_lockfile: bool) -> UpArgs {
        UpArgs {
            no_lockfile,
            frozen_lockfile,
            ..UpArgs::default()
        }
    }

    /// `canonical_lockfile_bytes` MUST match what `write_lockfile` actually
    /// puts on disk — otherwise `--frozen-lockfile` would report spurious
    /// mismatches because the two paths produce different bytes.
    #[test]
    fn canonical_bytes_match_write_lockfile_output() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("devcontainer-lock.json");

        let lockfile = one_feature_lockfile("1.6.1", &"a".repeat(64));

        write_lockfile(&path, &lockfile, true).expect("write_lockfile");
        let on_disk = std::fs::read(&path).unwrap();
        let in_memory = canonical_lockfile_bytes(&lockfile).expect("canonicalize");

        assert_eq!(
            in_memory, on_disk,
            "canonical_lockfile_bytes diverged from write_lockfile output; \
             --frozen-lockfile would report spurious mismatches"
        );
    }

    /// `--no-lockfile` short-circuits the helper entirely: no read, no write,
    /// no comparison, even in `--frozen-lockfile` (the two are mutually
    /// exclusive at the CLI layer, but the helper is defensive).
    #[test]
    fn no_lockfile_flag_skips_all_io() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join(".devcontainer/devcontainer.json");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        let lockfile = one_feature_lockfile("1.0.0", &"b".repeat(64));

        let args = make_args(true, false);
        handle_lockfile_post_build(&args, &config_path, &lockfile).expect("no-lockfile path");

        let derived = get_lockfile_path(&config_path);
        assert!(
            !derived.exists(),
            "--no-lockfile must not write the lockfile to disk"
        );
    }

    /// Default mode writes the lockfile next to the config file, sorted by
    /// key with a trailing newline (validated downstream by parity tests in
    /// `deacon_core::lockfile`).
    #[test]
    fn default_mode_writes_lockfile_next_to_config() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join(".devcontainer/devcontainer.json");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        let lockfile = one_feature_lockfile("1.0.0", &"c".repeat(64));

        let args = make_args(false, false);
        handle_lockfile_post_build(&args, &config_path, &lockfile).expect("default-write path");

        let derived = get_lockfile_path(&config_path);
        let on_disk = read_lockfile(&derived).expect("read_lockfile").unwrap();
        assert_eq!(on_disk, lockfile);
    }

    /// Frozen mode against a missing lockfile fails with the upstream string
    /// `"Lockfile does not exist."` so CI scripts that match on the message
    /// keep working.
    #[test]
    fn frozen_mode_missing_lockfile_fails_with_upstream_string() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join(".devcontainer/devcontainer.json");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        let lockfile = one_feature_lockfile("1.0.0", &"d".repeat(64));

        let args = make_args(false, true);
        let err = handle_lockfile_post_build(&args, &config_path, &lockfile)
            .expect_err("frozen + missing must fail");
        let msg = format!("{:#}", err);
        assert!(
            msg.contains("Lockfile does not exist."),
            "expected upstream-aligned summary, got: {msg}"
        );
    }

    /// Frozen mode with a byte-identical existing lockfile succeeds.
    #[test]
    fn frozen_mode_matches_existing_lockfile() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join(".devcontainer/devcontainer.json");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        let lockfile = one_feature_lockfile("1.0.0", &"e".repeat(64));

        // Seed the on-disk file with the canonical form.
        let derived = get_lockfile_path(&config_path);
        write_lockfile(&derived, &lockfile, true).unwrap();

        let args = make_args(false, true);
        handle_lockfile_post_build(&args, &config_path, &lockfile)
            .expect("frozen + matching must succeed");
    }

    /// Frozen mode with a mismatched on-disk lockfile fails with the upstream
    /// string `"Lockfile does not match."`.
    #[test]
    fn frozen_mode_mismatch_fails_with_upstream_string() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join(".devcontainer/devcontainer.json");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();

        // On disk: version 1.0.0
        let stale = one_feature_lockfile("1.0.0", &"f".repeat(64));
        let derived = get_lockfile_path(&config_path);
        write_lockfile(&derived, &stale, true).unwrap();

        // Freshly resolved: version 2.0.0 — should NOT match.
        let fresh = one_feature_lockfile("2.0.0", &"f".repeat(64));

        let args = make_args(false, true);
        let err = handle_lockfile_post_build(&args, &config_path, &fresh)
            .expect_err("frozen + mismatch must fail");
        let msg = format!("{:#}", err);
        assert!(
            msg.contains("Lockfile does not match."),
            "expected upstream-aligned summary, got: {msg}"
        );
    }

    #[test]
    fn is_readonly_fs_error_detects_permission_denied() {
        let inner = io::Error::from(io::ErrorKind::PermissionDenied);
        let err: anyhow::Error = anyhow::anyhow!(inner).context("write failed");
        assert!(is_readonly_fs_error(&err));
    }

    #[test]
    fn is_readonly_fs_error_ignores_other_io_errors() {
        let inner = io::Error::from(io::ErrorKind::NotFound);
        let err: anyhow::Error = anyhow::anyhow!(inner).context("read failed");
        assert!(!is_readonly_fs_error(&err));
    }
}
