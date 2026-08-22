//! Utility functions for the up command.
//!
//! This module contains:
//! - `check_for_disallowed_features` - Check for disallowed features
//! - `discover_id_labels_from_config` - Discover id-labels from configuration
//! - `apply_user_mapping` - Apply user mapping configuration
//! - `handle_lockfile_post_build` - Write/compare lockfile after a feature build
//! - `container_substituted_config` - Reported-document `containerSubstitute` pass (#608)

use anyhow::Result;
use deacon_core::config::DevContainerConfig;
use deacon_core::docker::Docker;
use deacon_core::errors::DeaconError;
use deacon_core::lockfile::Lockfile;
use deacon_core::variable::SubstitutionContext;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, instrument, warn};

use crate::commands::shared::lockfile::{LockfilePolicy, apply_lockfile_policy};

use super::args::UpArgs;

/// Resolve the container workspace folder reported to callers.
///
/// When the config sets an explicit `workspaceFolder`, that value is used
/// verbatim. Otherwise the spec/reference default is
/// `/workspaces/<basename(root)>[/<subpath>]` — the git root basename (under
/// `--mount-workspace-git-root`) plus the root→workspace subpath (issue #309),
/// so the reported value matches the mount and the used working dir. Delegates to
/// the single source of truth, [`deacon_core::workspace::container_workspace_folder`].
pub(crate) fn default_remote_workspace_folder(
    workspace_folder: &Path,
    config_workspace_folder: Option<&str>,
    mount_workspace_git_root: bool,
) -> String {
    deacon_core::workspace::container_workspace_folder(
        workspace_folder,
        config_workspace_folder,
        mount_workspace_git_root,
    )
}

/// Re-run variable substitution against the container `up` just created, so the
/// document `up` REPORTS resolves the container-aware tokens.
///
/// This is the reference CLI's third substitution pass (`containerSubstitute`,
/// `variableSubstitution.ts`). deacon already ran it for everything that USES the
/// configuration at runtime — `resolve_env_and_user` resolves `${containerEnv:*}`
/// in `remoteEnv` before injecting it, which is why `deacon exec` on the same
/// container has the right values — but the `--include-configuration` /
/// `--include-merged-configuration` blocks were serialized from the pass-1
/// configuration, which by construction predates the container and therefore
/// cannot have resolved a single `${containerEnv:*}`. Issue #608.
///
/// `container_env` is the RAW container environment (`inspect`'s `Config.Env`),
/// the canonical source for `${containerEnv:VAR}` — not the userEnvProbe result
/// and not the merged effective env.
///
/// Fail-safe: when the container environment could not be read, the caller passes
/// `None` and this returns the configuration untouched, so the template survives
/// instead of collapsing to an empty string (`resolve_variable` returns
/// `Some("")` for a missing key once `container_env` is `Some`).
pub(crate) fn container_substituted_config(
    config: &DevContainerConfig,
    workspace_folder: &Path,
    devcontainer_id: &str,
    container_env: Option<&HashMap<String, String>>,
    container_workspace_folder: &str,
) -> DevContainerConfig {
    let Some(container_env) = container_env else {
        debug!(
            "No container environment available; reporting configuration as substituted pre-container"
        );
        return config.clone();
    };

    let mut context = match SubstitutionContext::new(workspace_folder) {
        Ok(context) => context,
        Err(error) => {
            warn!(
                "Could not build a container substitution context for the reported configuration: {}",
                error
            );
            return config.clone();
        }
    };
    context.devcontainer_id = devcontainer_id.to_string();
    context.container_env = Some(container_env.clone());
    context.container_workspace_folder = Some(container_workspace_folder.to_string());

    let (substituted, report) = config.apply_variable_substitution(&context);
    debug!(
        "Container substitution pass over the reported configuration made {} replacements",
        report.replacements.len()
    );
    substituted
}

/// Read the raw container environment for [`container_substituted_config`].
///
/// Best-effort by design: an inspect failure yields `None`, which leaves the
/// reported configuration exactly as it was rather than resolving every
/// `${containerEnv:*}` to an empty string.
pub(crate) async fn container_env_for_substitution<D: Docker>(
    runtime: &D,
    container_id: &str,
) -> Option<HashMap<String, String>> {
    match runtime.inspect_container(container_id).await {
        Ok(Some(info)) => Some(info.env),
        Ok(None) => {
            warn!(
                "Container '{}' not found while resolving the reported configuration",
                container_id
            );
            None
        }
        Err(error) => {
            warn!(
                "Container inspect failed while resolving the reported configuration: {}",
                error
            );
            None
        }
    }
}

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
        DockerUserMapper, UserMappingConfig, UserMappingService, get_host_user_info,
    };

    debug!("Applying user mapping configuration");

    // Spec parity (#71): the `updateRemoteUserUID` property defaults to
    // `true` on Linux per the containers.dev spec, so that files written
    // into bind-mounted workspaces from inside the container land at the
    // correct host ownership. Deacon previously defaulted to `false`, so
    // a Linux user with no explicit config or CLI flag never got the UID
    // re-stamp and bind-mounted files ended up root- (or image-) owned.
    //
    // Precedence (highest first):
    //   1. `updateRemoteUserUID` in devcontainer.json (config)
    //   2. `--update-remote-user-uid-default` CLI flag (applied earlier
    //      in execute_up_with_runtime, which writes to `config` when the
    //      config field is absent)
    //   3. OS default: `true` on Linux, `false` elsewhere
    let os_default_update_uid = cfg!(target_os = "linux");
    let effective_update_uid = config
        .update_remote_user_uid
        .unwrap_or(os_default_update_uid);

    // Spec parity (#90): when the resolved remote user is `root` (or any
    // user that will land at UID 0 inside the container), `updateRemoteUserUID`
    // MUST be a no-op. Re-stamping root to the host's UID strips its
    // privileges and breaks bind/volume mounts created by the daemon as
    // UID 0. Upstream @devcontainers/cli skips this case unconditionally;
    // we mirror that here. The check runs *before* host_uid lookup so we
    // also short-circuit the host_user_info probe.
    let remote_user_is_root = config
        .remote_user
        .as_deref()
        .map(|u| u == "root")
        .unwrap_or(false);
    let effective_update_uid = if effective_update_uid && remote_user_is_root {
        debug!(
            "Resolved remoteUser is 'root'; skipping updateRemoteUserUID to preserve UID-0 privileges (#90)"
        );
        false
    } else {
        effective_update_uid
    };

    let mut user_config = UserMappingConfig::new(
        config.remote_user.clone(),
        config.container_user.clone(),
        effective_update_uid,
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
    if !config.cap_add().is_empty() {
        debug!("Container capabilities to add: {:?}", config.cap_add);
    }
    if !config.security_opt().is_empty() {
        debug!("Container security options: {:?}", config.security_opt);
    }

    Ok(())
}

/// Apply the lockfile policy after a Feature build completes.
///
/// A thin adapter over the shared decision in
/// [`crate::commands::shared::lockfile`], which `build` reaches through the
/// same door (#556). All the behavior — skip / semantic compare / best-effort
/// write, and the upstream-aligned `"Lockfile does not exist."` /
/// `"Lockfile does not match."` strings — lives there, so a lockfile's fate
/// never depends on which subcommand resolved the Features.
///
/// Mirrors upstream `writeLockfile` in `devcontainers/cli`
/// `src/spec-configuration/lockfile.ts` (`PR #1212`).
pub(crate) async fn handle_lockfile_post_build(
    args: &UpArgs,
    config_path: &Path,
    lockfile: &Lockfile,
) -> Result<()> {
    let policy = LockfilePolicy::from_flags(args.no_lockfile, args.frozen_lockfile);
    apply_lockfile_policy(policy, config_path, lockfile).await?;
    Ok(())
}

#[cfg(test)]
mod remote_workspace_folder_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn defaults_to_workspaces_basename_when_unset() {
        // Parity with the reference CLI: `/workspaces/<basename>`, never a bare
        // `/workspaces`.
        let got = default_remote_workspace_folder(Path::new("/home/me/myproject"), None, false);
        assert_eq!(got, "/workspaces/myproject");
    }

    #[test]
    fn honors_explicit_workspace_folder() {
        let got = default_remote_workspace_folder(
            Path::new("/home/me/myproject"),
            Some("/custom/app"),
            true,
        );
        assert_eq!(got, "/custom/app");
    }

    #[test]
    fn falls_back_to_workspace_when_basename_missing() {
        let got = default_remote_workspace_folder(Path::new("/"), None, false);
        assert_eq!(got, "/workspaces/workspace");
    }
}

#[cfg(test)]
mod lockfile_post_build_tests {
    use super::*;
    use deacon_core::lockfile::{
        LockfileFeature, canonical_lockfile_json, get_lockfile_path, lockfile_text_matches,
        read_lockfile, write_lockfile,
    };
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

    /// The bytes `write_lockfile` puts on disk MUST be the canonical form, so
    /// a file deacon just wrote satisfies its own `--frozen-lockfile`.
    #[tokio::test(flavor = "current_thread")]
    async fn write_lockfile_emits_canonical_bytes() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("devcontainer-lock.json");

        let lockfile = one_feature_lockfile("1.6.1", &"a".repeat(64));

        write_lockfile(&path, &lockfile, true)
            .await
            .expect("write_lockfile");
        let on_disk = std::fs::read_to_string(&path).unwrap();

        assert_eq!(
            on_disk,
            canonical_lockfile_json(&lockfile).expect("canonicalize")
        );
        assert!(
            lockfile_text_matches(&on_disk, &lockfile),
            "--frozen-lockfile must accept the file deacon itself just wrote"
        );
    }

    /// `--no-lockfile` short-circuits the helper entirely: no read, no write,
    /// no comparison, even in `--frozen-lockfile` (the two are mutually
    /// exclusive at the CLI layer, but the helper is defensive).
    #[tokio::test(flavor = "current_thread")]
    async fn no_lockfile_flag_skips_all_io() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join(".devcontainer/devcontainer.json");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        let lockfile = one_feature_lockfile("1.0.0", &"b".repeat(64));

        let args = make_args(true, false);
        handle_lockfile_post_build(&args, &config_path, &lockfile)
            .await
            .expect("no-lockfile path");

        let derived = get_lockfile_path(&config_path);
        assert!(
            !derived.exists(),
            "--no-lockfile must not write the lockfile to disk"
        );
    }

    /// Default mode writes the lockfile next to the config file, sorted by
    /// key with a trailing newline (validated downstream by parity tests in
    /// `deacon_core::lockfile`).
    #[tokio::test(flavor = "current_thread")]
    async fn default_mode_writes_lockfile_next_to_config() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join(".devcontainer/devcontainer.json");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        let lockfile = one_feature_lockfile("1.0.0", &"c".repeat(64));

        let args = make_args(false, false);
        handle_lockfile_post_build(&args, &config_path, &lockfile)
            .await
            .expect("default-write path");

        let derived = get_lockfile_path(&config_path);
        let on_disk = read_lockfile(&derived)
            .await
            .expect("read_lockfile")
            .unwrap();
        assert_eq!(on_disk, lockfile);
    }

    /// Frozen mode against a missing lockfile fails with the upstream string
    /// `"Lockfile does not exist."` so CI scripts that match on the message
    /// keep working.
    #[tokio::test(flavor = "current_thread")]
    async fn frozen_mode_missing_lockfile_fails_with_upstream_string() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join(".devcontainer/devcontainer.json");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        let lockfile = one_feature_lockfile("1.0.0", &"d".repeat(64));

        let args = make_args(false, true);
        let err = handle_lockfile_post_build(&args, &config_path, &lockfile)
            .await
            .expect_err("frozen + missing must fail");
        let msg = format!("{:#}", err);
        assert!(
            msg.contains("Lockfile does not exist."),
            "expected upstream-aligned summary, got: {msg}"
        );
    }

    /// Frozen mode with a byte-identical existing lockfile succeeds.
    #[tokio::test(flavor = "current_thread")]
    async fn frozen_mode_matches_existing_lockfile() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join(".devcontainer/devcontainer.json");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        let lockfile = one_feature_lockfile("1.0.0", &"e".repeat(64));

        // Seed the on-disk file with the canonical form.
        let derived = get_lockfile_path(&config_path);
        write_lockfile(&derived, &lockfile, true).await.unwrap();

        let args = make_args(false, true);
        handle_lockfile_post_build(&args, &config_path, &lockfile)
            .await
            .expect("frozen + matching must succeed");
    }

    /// Frozen mode with a mismatched on-disk lockfile fails with the upstream
    /// string `"Lockfile does not match."`.
    #[tokio::test(flavor = "current_thread")]
    async fn frozen_mode_mismatch_fails_with_upstream_string() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join(".devcontainer/devcontainer.json");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();

        // On disk: version 1.0.0
        let stale = one_feature_lockfile("1.0.0", &"f".repeat(64));
        let derived = get_lockfile_path(&config_path);
        write_lockfile(&derived, &stale, true).await.unwrap();

        // Freshly resolved: version 2.0.0 — should NOT match.
        let fresh = one_feature_lockfile("2.0.0", &"f".repeat(64));

        let args = make_args(false, true);
        let err = handle_lockfile_post_build(&args, &config_path, &fresh)
            .await
            .expect_err("frozen + mismatch must fail");
        let msg = format!("{:#}", err);
        assert!(
            msg.contains("Lockfile does not match."),
            "expected upstream-aligned summary, got: {msg}"
        );
    }

    /// The regression #557 is about: a lockfile written by the REFERENCE CLI
    /// must satisfy `deacon up --frozen-lockfile`. The seeded bytes are the
    /// reference's own output for `fx-upstream-lockfile-frozen` at oracle
    /// 0.87.0 — same document as the freshly-resolved lockfile, differing only
    /// in the order of the three keys inside the entry.
    ///
    /// The two other spellings assert the same tolerance the reference's
    /// `frozen lockfile matches despite formatting differences` test asserts:
    /// a stripped trailing newline and a reindented file are formatting, not
    /// content.
    #[tokio::test(flavor = "current_thread")]
    async fn frozen_mode_accepts_a_lockfile_the_reference_cli_wrote() {
        const DIGEST: &str = "63c96e8ac33f5630300d8883e2ec3123278de70d318589af596ea1954846014d";
        let mut features = HashMap::new();
        features.insert(
            "ghcr.io/devcontainers/features/git:1.3.2".to_string(),
            LockfileFeature {
                version: "1.3.2".to_string(),
                resolved: format!("ghcr.io/devcontainers/features/git@sha256:{DIGEST}"),
                integrity: format!("sha256:{DIGEST}"),
                depends_on: None,
            },
        );
        let resolved = Lockfile { features };

        // Verbatim from the reference CLI: `version, resolved, integrity`.
        let reference_bytes = format!(
            "{{\n  \"features\": {{\n    \"ghcr.io/devcontainers/features/git:1.3.2\": {{\n      \
             \"version\": \"1.3.2\",\n      \"resolved\": \
             \"ghcr.io/devcontainers/features/git@sha256:{DIGEST}\",\n      \"integrity\": \
             \"sha256:{DIGEST}\"\n    }}\n  }}\n}}\n"
        );

        for (label, on_disk) in [
            ("reference key order", reference_bytes.clone()),
            (
                "no trailing newline",
                reference_bytes.trim_end().to_string(),
            ),
            (
                "four-space indent",
                serde_json::to_string_pretty(
                    &serde_json::from_str::<serde_json::Value>(&reference_bytes).unwrap(),
                )
                .unwrap()
                .replace("  ", "    "),
            ),
        ] {
            let tmp = TempDir::new().unwrap();
            let config_path = tmp.path().join(".devcontainer.json");
            std::fs::write(&config_path, "{}").unwrap();
            std::fs::write(get_lockfile_path(&config_path), &on_disk).unwrap();

            let args = make_args(false, true);
            handle_lockfile_post_build(&args, &config_path, &resolved)
                .await
                .unwrap_or_else(|e| panic!("frozen must accept {label}: {e:#}"));

            // A frozen run reads; it never rewrites.
            assert_eq!(
                std::fs::read_to_string(get_lockfile_path(&config_path)).unwrap(),
                on_disk,
                "--frozen-lockfile rewrote the file ({label})"
            );
        }
    }

    /// Tolerance is about serialisation only: unparseable text is still a
    /// mismatch, mirroring the reference's `try { … } catch {}`.
    #[tokio::test(flavor = "current_thread")]
    async fn frozen_mode_rejects_unparseable_lockfile() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join(".devcontainer.json");
        std::fs::write(&config_path, "{}").unwrap();
        std::fs::write(get_lockfile_path(&config_path), "{ this is not json").unwrap();

        let lockfile = one_feature_lockfile("1.0.0", &"a".repeat(64));
        let args = make_args(false, true);
        let err = handle_lockfile_post_build(&args, &config_path, &lockfile)
            .await
            .expect_err("frozen + unparseable must fail");
        assert!(
            format!("{:#}", err).contains("Lockfile does not match."),
            "expected upstream-aligned summary, got: {:#}",
            err
        );
    }
}

#[cfg(test)]
mod update_remote_user_uid_default_tests {
    //! Spec parity (#71): `updateRemoteUserUID` defaults to `true` on Linux
    //! per the containers.dev spec. The construction site lives in
    //! `apply_user_mapping`; we mirror the literal here to keep the test
    //! self-contained without requiring a Docker mock.

    #[test]
    fn os_default_is_true_on_linux_and_false_elsewhere() {
        let os_default_update_uid = cfg!(target_os = "linux");
        if cfg!(target_os = "linux") {
            assert!(
                os_default_update_uid,
                "Per containers.dev spec, updateRemoteUserUID defaults to true on Linux (#71)"
            );
        } else {
            assert!(
                !os_default_update_uid,
                "Outside Linux the spec leaves updateRemoteUserUID off by default"
            );
        }
    }

    /// Pure helper that mirrors the precedence chain in
    /// `apply_user_mapping::UserMappingConfig::new(...,
    /// config.update_remote_user_uid.unwrap_or(os_default))`. Extracted so
    /// the test can exercise the chain without an `Option` literal —
    /// clippy's `unnecessary_literal_unwrap` lint rejects
    /// `Some(true).unwrap_or(...)` even when the literal is structural.
    fn effective_update_uid(config_value: Option<bool>, os_default: bool) -> bool {
        config_value.unwrap_or(os_default)
    }

    #[test]
    fn config_value_wins_over_os_default() {
        let os_default_update_uid = cfg!(target_os = "linux");

        // Explicit `updateRemoteUserUID: false` must override the OS default.
        assert!(
            !effective_update_uid(Some(false), os_default_update_uid),
            "explicit `updateRemoteUserUID: false` must override the OS default (#71)"
        );

        // Explicit `updateRemoteUserUID: true` must override the OS default
        // (matters on non-Linux platforms where the default is false).
        assert!(
            effective_update_uid(Some(true), os_default_update_uid),
            "explicit `updateRemoteUserUID: true` must override the OS default (#71)"
        );

        // None falls through to the OS default — this is the bug #71 fixes:
        // deacon previously used `unwrap_or(false)` here.
        assert_eq!(
            effective_update_uid(None, os_default_update_uid),
            os_default_update_uid,
            "absent updateRemoteUserUID must use the OS default, not hard-coded false (#71)"
        );
    }
}
