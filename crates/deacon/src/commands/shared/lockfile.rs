//! Shared lockfile policy for the subcommands that resolve Features.
//!
//! `up` and `build` both resolve the declared Features and both then have to
//! decide what to do about `.devcontainer-lock.json`. The reference CLI makes
//! that decision ONCE, in `writeLockfile`
//! (`src/spec-configuration/lockfile.ts`), reached from
//! `generateFeaturesConfig` — so both of its subcommands get the same answer by
//! construction. deacon reached it from two places and, until #556, only `up`
//! consulted the flags at all: `build` parsed `--no-lockfile` /
//! `--frozen-lockfile` and dropped them.
//!
//! This module is the single decision, so a lockfile's fate never depends on
//! which subcommand resolved the Features.
//!
//! ## The reference's shape, transcribed
//!
//! From the pinned oracle's bundle (`@devcontainers/cli@0.87.0`,
//! `dist/spec-node/devContainersSpecCLI.js`, minified `mQ`):
//!
//! ```js
//! if (params.noLockfile) return;
//! const existing = await readFile(path).catch(ENOENT => undefined);
//! const next = JSON.stringify(lockfile, null, 2) + "\n";
//! if (params.frozenLockfile && !existing) throw new Error("Lockfile does not exist.");
//! let normalized;
//! if (existing) try { normalized = JSON.stringify(JSON.parse(existing), null, 2) + "\n"; } catch {}
//! if (!normalized || normalized !== next) {
//!     if (params.frozenLockfile) throw new Error("Lockfile does not match.");
//!     await writeFile(path, next);
//! }
//! ```
//!
//! Two properties of that are load-bearing and are why the gate below exists:
//!
//! - The comparison is **semantic**: the on-disk text is re-serialized before
//!   being compared, so key order and whitespace are not content (#563 landed
//!   the same rule on deacon's side as
//!   [`deacon_core::lockfile::lockfile_text_matches`]).
//! - `writeLockfile` is only reached from `generateFeaturesConfig`, which
//!   returns early when the configuration declares **no Features**. So a
//!   configuration with no `features` never consults the lockfile — measured at
//!   0.87.0: `build --frozen-lockfile` on `{"image": "debian:bookworm-slim"}`
//!   with no lockfile on disk exits **0**.

use anyhow::{Context, Result};
use deacon_core::errors::DeaconError;
use deacon_core::lockfile::{
    Lockfile, LockfileValidationResult, get_lockfile_path, lockfile_text_matches, read_lockfile,
    validate_lockfile_against_config, write_lockfile,
};
use std::io;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Linux `EROFS` errno value — "Read-only file system". Checked by raw errno in
/// the best-effort write path because `io::ErrorKind::ReadOnlyFilesystem` was
/// stabilized in Rust 1.83 and this workspace's MSRV predates it.
#[cfg(unix)]
const EROFS: i32 = 30;

/// What the CLI flags ask us to do with the lockfile.
///
/// `--no-lockfile` and `--frozen-lockfile` are rejected together at the CLI
/// tier (`bhv-lockfile-flags-mutually-exclusive`), so the two-flag pair only
/// ever names one of these three. [`LockfilePolicy::from_flags`] is still
/// defensive about the impossible combination and resolves it the way the
/// reference does — `noLockfile` is tested first, so it wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LockfilePolicy {
    /// `--no-lockfile`: never read, compare or write the lockfile.
    Skip,
    /// `--frozen-lockfile`: the lockfile must already exist and must already
    /// say what resolution says. Never written.
    Frozen,
    /// Default: write the freshly-resolved lockfile next to the config.
    Write,
}

impl LockfilePolicy {
    /// Resolve the policy from the two CLI flags.
    pub(crate) fn from_flags(no_lockfile: bool, frozen_lockfile: bool) -> Self {
        if no_lockfile {
            Self::Skip
        } else if frozen_lockfile {
            Self::Frozen
        } else {
            Self::Write
        }
    }
}

/// Does this configuration declare any Features?
///
/// The reference reaches its lockfile policy only through
/// `generateFeaturesConfig`, which returns before touching the lockfile when
/// the user declared no Features. Local (`./…`) Features count: the reference's
/// early return keys off the *declared* set, and only the lockfile's CONTENT is
/// filtered to OCI / direct-tarball sources.
fn declares_features(config_features: &serde_json::Value) -> bool {
    config_features
        .as_object()
        .is_some_and(|obj| !obj.is_empty())
}

/// Refuse, before anything is built, when `--frozen-lockfile` cannot possibly
/// be satisfied.
///
/// The reference throws `Lockfile does not exist.` from the same pass that
/// resolves Features, i.e. before it builds the Feature-extended image, and
/// leaves the workspace untouched. Checking here rather than after the build
/// keeps that ordering: the user asked for "resolution must not change the
/// lockfile", and with no lockfile on disk there is nothing resolution could
/// agree with, so no image needs building to know the answer.
///
/// The id-set comparison against the declared Features is deacon's own earlier,
/// cheaper diagnostic — the reference only learns of a mismatch after
/// resolution computes digests, which deacon also still checks in
/// [`apply_lockfile_policy`]. Both report the upstream-aligned summary strings
/// (`Lockfile does not exist.` / `Lockfile does not match.`).
///
/// A no-op under [`LockfilePolicy::Skip`] / [`LockfilePolicy::Write`], and a
/// no-op when the configuration declares no Features (see [`declares_features`]).
pub(crate) async fn ensure_frozen_lockfile_usable(
    policy: LockfilePolicy,
    config_path: &Path,
    config_features: &serde_json::Value,
) -> Result<()> {
    if policy != LockfilePolicy::Frozen || !declares_features(config_features) {
        return Ok(());
    }

    let lockfile_path = get_lockfile_path(config_path);
    info!(
        "Frozen lockfile mode enabled: validating features against '{}'",
        lockfile_path.display()
    );

    let lockfile = read_lockfile(&lockfile_path).await.with_context(|| {
        format!(
            "Failed to read lockfile at '{}'. \
             The file may be corrupted or contain invalid JSON. \
             To regenerate, remove the file and run without --frozen-lockfile.",
            lockfile_path.display()
        )
    })?;

    match validate_lockfile_against_config(lockfile.as_ref(), config_features, &lockfile_path) {
        LockfileValidationResult::Matched => {
            info!(
                "Lockfile validation passed: all features match '{}'",
                lockfile_path.display()
            );
            Ok(())
        }
        other => Err(
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: other.format_error(),
            })
            .into(),
        ),
    }
}

/// Apply the lockfile policy once the Features have been resolved.
///
/// - [`LockfilePolicy::Skip`]: do nothing at all — no read, no compare, no
///   write. Measured at 0.87.0: `--no-lockfile` leaves even a stale existing
///   lockfile byte-identical.
/// - [`LockfilePolicy::Frozen`]: compare the on-disk file to the freshly
///   resolved set **as documents** and fail with the upstream-aligned string if
///   they differ. Never writes.
/// - [`LockfilePolicy::Write`]: write the freshly resolved lockfile.
///
/// Returns the path written, or `None` when nothing was written (skipped,
/// frozen, or a read-only workspace).
///
/// On read-only workspaces (`EROFS`/`EACCES` on write) the write is downgraded
/// to a WARN so a read-only mount doesn't break the command. Frozen mode never
/// reaches that branch — it only reads — so the fallback is write-side only.
pub(crate) async fn apply_lockfile_policy(
    policy: LockfilePolicy,
    config_path: &Path,
    lockfile: &Lockfile,
) -> Result<Option<PathBuf>> {
    if policy == LockfilePolicy::Skip {
        debug!("--no-lockfile set; skipping lockfile write/compare");
        return Ok(None);
    }

    let lockfile_path = get_lockfile_path(config_path);

    if policy == LockfilePolicy::Frozen {
        compare_lockfile_frozen(&lockfile_path, lockfile).await?;
        return Ok(None);
    }

    write_lockfile_best_effort(&lockfile_path, lockfile).await
}

/// Frozen-mode comparison: compare the on-disk lockfile to the freshly-resolved
/// one **as documents**, not as bytes.
///
/// `--frozen-lockfile` asks "would resolution change what this file says?", and
/// key order, indentation and a trailing newline are serialisation choices
/// rather than content. Byte-comparing answered a different question and made
/// deacon reject every lockfile the reference CLI writes (#557); the reference
/// normalises before comparing for exactly this reason and carries the test
/// `frozen lockfile matches despite formatting differences`.
///
/// A missing file, unparseable text, or a genuine content difference all fail
/// with the upstream-aligned summary string.
async fn compare_lockfile_frozen(lockfile_path: &Path, lockfile: &Lockfile) -> Result<()> {
    let actual_text = match tokio::fs::read_to_string(lockfile_path).await {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
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
        Err(e) => {
            return Err(anyhow::Error::from(e).context(format!(
                "Failed to read existing lockfile at '{}'",
                lockfile_path.display()
            )));
        }
    };

    if !lockfile_text_matches(&actual_text, lockfile) {
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

/// Best-effort write: succeeds normally, but downgrades EROFS/EACCES to WARN so
/// a read-only workspace (CI mount, read-only volume) doesn't break the
/// command. All other write errors propagate.
async fn write_lockfile_best_effort(
    lockfile_path: &Path,
    lockfile: &Lockfile,
) -> Result<Option<PathBuf>> {
    match write_lockfile(lockfile_path, lockfile, true).await {
        Ok(()) => {
            debug!("Wrote lockfile to '{}'", lockfile_path.display());
            Ok(Some(lockfile_path.to_path_buf()))
        }
        Err(e) => {
            let e = anyhow::Error::from(e);
            if is_readonly_fs_error(&e) {
                warn!(
                    path = %lockfile_path.display(),
                    error = %e,
                    "Lockfile write skipped (read-only workspace); continuing without persisting lockfile"
                );
                Ok(None)
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
/// variant was stabilized in Rust 1.83 and our MSRV predates it.
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

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::lockfile::{LockfileFeature, canonical_lockfile_json};
    use std::collections::HashMap;
    use tempfile::TempDir;

    /// `seed` is expanded to a full 64-hex digest — `write_lockfile` validates
    /// the integrity field's shape, so a short stand-in fails on the write path
    /// rather than on the thing under test.
    fn one_feature_lockfile(id: &str, version: &str, seed: char) -> Lockfile {
        let digest_hex: String = std::iter::repeat_n(seed, 64).collect();
        let mut features = HashMap::new();
        let repo = id.rsplit_once(':').map(|(r, _)| r).unwrap_or(id);
        features.insert(
            id.to_string(),
            LockfileFeature {
                version: version.to_string(),
                resolved: format!("{}@sha256:{}", repo, digest_hex),
                integrity: format!("sha256:{}", digest_hex),
                depends_on: None,
            },
        );
        Lockfile { features }
    }

    fn config_path_in(tmp: &TempDir) -> PathBuf {
        let config_path = tmp.path().join(".devcontainer/devcontainer.json");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        config_path
    }

    #[test]
    fn policy_resolves_from_flags() {
        assert_eq!(
            LockfilePolicy::from_flags(false, false),
            LockfilePolicy::Write
        );
        assert_eq!(
            LockfilePolicy::from_flags(false, true),
            LockfilePolicy::Frozen
        );
        assert_eq!(
            LockfilePolicy::from_flags(true, false),
            LockfilePolicy::Skip
        );
        // The CLI rejects the pair, but the reference tests `noLockfile` first
        // and so does this.
        assert_eq!(LockfilePolicy::from_flags(true, true), LockfilePolicy::Skip);
    }

    /// The gate the reference gets for free by only reaching its lockfile
    /// policy from `generateFeaturesConfig`: a configuration with no Features
    /// never consults the lockfile, so `--frozen-lockfile` cannot fail on one.
    #[tokio::test(flavor = "current_thread")]
    async fn frozen_precheck_is_a_noop_without_features() {
        let tmp = TempDir::new().unwrap();
        let config_path = config_path_in(&tmp);

        ensure_frozen_lockfile_usable(LockfilePolicy::Frozen, &config_path, &serde_json::json!({}))
            .await
            .expect("featureless config must not require a lockfile");

        ensure_frozen_lockfile_usable(
            LockfilePolicy::Frozen,
            &config_path,
            &serde_json::Value::Null,
        )
        .await
        .expect("absent features must not require a lockfile");
    }

    /// …but a configuration that DOES declare a Feature is refused before
    /// anything is built, with the upstream string.
    #[tokio::test(flavor = "current_thread")]
    async fn frozen_precheck_refuses_missing_lockfile_when_features_declared() {
        let tmp = TempDir::new().unwrap();
        let config_path = config_path_in(&tmp);

        let err = ensure_frozen_lockfile_usable(
            LockfilePolicy::Frozen,
            &config_path,
            &serde_json::json!({ "ghcr.io/devcontainers/features/git:1": {} }),
        )
        .await
        .expect_err("frozen + missing lockfile must refuse");
        let msg = format!("{:#}", err);
        assert!(
            msg.contains("Lockfile does not exist."),
            "expected the upstream summary, got: {msg}"
        );
        assert!(
            !get_lockfile_path(&config_path).exists(),
            "the refusal must not create the lockfile it refused over"
        );
    }

    /// A local (`./…`) Feature counts toward the gate: the reference's early
    /// return keys off the DECLARED set, and only the lockfile's content is
    /// filtered to OCI / tarball sources.
    #[tokio::test(flavor = "current_thread")]
    async fn frozen_precheck_counts_local_features() {
        let tmp = TempDir::new().unwrap();
        let config_path = config_path_in(&tmp);

        let err = ensure_frozen_lockfile_usable(
            LockfilePolicy::Frozen,
            &config_path,
            &serde_json::json!({ "./features/local": {} }),
        )
        .await
        .expect_err("a declared local Feature still requires the lockfile to exist");
        assert!(format!("{:#}", err).contains("Lockfile does not exist."));
    }

    /// `--no-lockfile` short-circuits both halves: no refusal, no write.
    #[tokio::test(flavor = "current_thread")]
    async fn skip_policy_does_no_io_at_all() {
        let tmp = TempDir::new().unwrap();
        let config_path = config_path_in(&tmp);
        let lockfile = one_feature_lockfile("ghcr.io/devcontainers/features/git:1", "1.0.0", 'a');

        ensure_frozen_lockfile_usable(
            LockfilePolicy::Skip,
            &config_path,
            &serde_json::json!({ "ghcr.io/devcontainers/features/git:1": {} }),
        )
        .await
        .expect("skip policy never refuses");

        let written = apply_lockfile_policy(LockfilePolicy::Skip, &config_path, &lockfile)
            .await
            .expect("skip policy never fails");
        assert!(written.is_none());
        assert!(
            !get_lockfile_path(&config_path).exists(),
            "--no-lockfile must not write the lockfile to disk"
        );
    }

    /// `--no-lockfile` also leaves an EXISTING lockfile untouched — measured at
    /// oracle 0.87.0 with a deliberately stale file, which came back
    /// byte-identical.
    #[tokio::test(flavor = "current_thread")]
    async fn skip_policy_leaves_an_existing_lockfile_untouched() {
        let tmp = TempDir::new().unwrap();
        let config_path = config_path_in(&tmp);
        let lockfile_path = get_lockfile_path(&config_path);
        let stale = "{\"features\":{\"stale\":{\"version\":\"0.0.0\",\"resolved\":\"x\",\"integrity\":\"y\"}}}";
        std::fs::write(&lockfile_path, stale).unwrap();

        let fresh = one_feature_lockfile("ghcr.io/devcontainers/features/git:1", "1.0.0", 'b');
        apply_lockfile_policy(LockfilePolicy::Skip, &config_path, &fresh)
            .await
            .expect("skip policy never fails");

        assert_eq!(
            std::fs::read_to_string(&lockfile_path).unwrap(),
            stale,
            "--no-lockfile must not rewrite an existing lockfile"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn write_policy_writes_canonical_bytes() {
        let tmp = TempDir::new().unwrap();
        let config_path = config_path_in(&tmp);
        let lockfile = one_feature_lockfile("ghcr.io/devcontainers/features/git:1", "1.3.2", 'c');

        let written = apply_lockfile_policy(LockfilePolicy::Write, &config_path, &lockfile)
            .await
            .expect("write policy")
            .expect("a path is reported");
        assert_eq!(written, get_lockfile_path(&config_path));
        assert_eq!(
            std::fs::read_to_string(&written).unwrap(),
            canonical_lockfile_json(&lockfile).expect("canonicalize"),
        );
    }

    /// Frozen mode accepts a lockfile whose CONTENT matches even when its
    /// serialization differs (#563's rule, now shared with `build`).
    #[tokio::test(flavor = "current_thread")]
    async fn frozen_policy_matches_despite_formatting_differences() {
        let tmp = TempDir::new().unwrap();
        let config_path = config_path_in(&tmp);
        let lockfile = one_feature_lockfile("ghcr.io/devcontainers/features/git:1", "1.3.2", 'd');

        // Re-serialize compactly with no trailing newline: same document,
        // different bytes.
        let compact = serde_json::to_string(&lockfile).unwrap();
        std::fs::write(get_lockfile_path(&config_path), compact).unwrap();

        apply_lockfile_policy(LockfilePolicy::Frozen, &config_path, &lockfile)
            .await
            .expect("formatting is not content");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn frozen_policy_rejects_a_different_document() {
        let tmp = TempDir::new().unwrap();
        let config_path = config_path_in(&tmp);
        let on_disk = one_feature_lockfile("ghcr.io/devcontainers/features/git:1", "1.3.2", 'e');
        std::fs::write(
            get_lockfile_path(&config_path),
            canonical_lockfile_json(&on_disk).unwrap(),
        )
        .unwrap();

        let resolved = one_feature_lockfile("ghcr.io/devcontainers/features/git:1", "1.3.8", 'f');
        let err = apply_lockfile_policy(LockfilePolicy::Frozen, &config_path, &resolved)
            .await
            .expect_err("a changed resolution must be refused");
        assert!(format!("{:#}", err).contains("Lockfile does not match."));
    }

    #[test]
    fn is_readonly_filesystem_error_detects_permission_denied() {
        // EACCES surfaces as PermissionDenied — the most common cause of
        // a "can't write the lockfile" path on container CI mounts.
        let inner = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let err: anyhow::Error = anyhow::anyhow!(inner).context("write failed");
        assert!(is_readonly_fs_error(&err));
    }

    #[test]
    fn is_readonly_filesystem_error_ignores_unrelated_io_errors() {
        // Other IO errors (NotFound, BrokenPipe, etc.) must propagate —
        // downgrading them all would hide real bugs.
        let inner = std::io::Error::from(std::io::ErrorKind::NotFound);
        let err: anyhow::Error = anyhow::anyhow!(inner).context("read failed");
        assert!(!is_readonly_fs_error(&err));
    }

    #[test]
    fn is_readonly_filesystem_error_ignores_non_io_errors() {
        let err = anyhow::anyhow!("not an io error");
        assert!(!is_readonly_fs_error(&err));
    }
}
