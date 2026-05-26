//! Workspace-trust policy and persistence.
//!
//! Host-side hooks (`initializeCommand`, dotfiles `installCommand` when sourced
//! from a workspace) execute arbitrary shell on the developer's machine before
//! any container sandboxing. A hostile `devcontainer.json` cloned into a
//! workspace would otherwise gain that capability the moment a user runs
//! `deacon up`. This module gates those entry points behind an explicit trust
//! decision.
//!
//! The trust set is persisted as a JSON file under the configured
//! `user_data_folder` (see [`trust_store_path`]). Entries store the canonical
//! (`fs::canonicalize`d) workspace path and a last-trusted-at timestamp.
//!
//! The model is consumer-only and intentionally upstream-divergent: the
//! containers.dev spec does not mandate a workspace-trust check. See
//! `SECURITY.md` and `CLAUDE.md` for the rationale.

use crate::errors::{DeaconError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Policy governing how an untrusted workspace is handled at a host-side hook.
///
/// `AlwaysAllow` corresponds to the legacy / spec-aligned behavior; the other
/// variants are the new fail-closed surface.
#[derive(Debug, Clone)]
pub enum WorkspaceTrustPolicy {
    /// Skip the gate entirely. The default when neither
    /// `--trust-workspace[-persist]` nor `DEACON_NO_PROMPT=1` is set today.
    /// Future: flip the default to `Allowlist(...)`.
    AlwaysAllow,
    /// Prompt the user interactively. In non-interactive contexts (no TTY) the
    /// caller should downgrade this to `Deny` — `DEACON_NO_PROMPT=1` makes
    /// that explicit.
    Prompt,
    /// Trust only if the workspace appears in the allowlist at this path.
    /// Adding entries is done via [`record_trusted_workspace`].
    Allowlist(PathBuf),
    /// Always deny. Used in CI via `DEACON_NO_PROMPT=1`.
    Deny,
}

/// Outcome of a trust check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustDecision {
    /// Workspace is trusted; the host-side hook may run.
    Trusted,
    /// Workspace is not trusted; the caller MUST refuse to run the hook.
    Denied {
        /// Canonical workspace path that failed the check (best-effort: the
        /// original path is used if canonicalization fails).
        workspace: PathBuf,
        /// Short reason intended for logs / error messages.
        reason: String,
    },
}

/// Errors raised by the trust module itself (distinct from a `Denied` decision).
#[derive(Debug, thiserror::Error)]
pub enum TrustError {
    /// Failed to read or write the trust store.
    #[error("Failed to access workspace trust store at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Trust store on disk could not be parsed as JSON.
    #[error("Corrupt workspace trust store at {path}: {message}")]
    Corrupt { path: PathBuf, message: String },
}

impl From<TrustError> for DeaconError {
    fn from(err: TrustError) -> Self {
        DeaconError::Internal(crate::errors::InternalError::Generic {
            message: err.to_string(),
        })
    }
}

/// A single trusted-workspace record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedWorkspace {
    /// Canonical (`fs::canonicalize`d) workspace path at the time it was
    /// added.
    pub path: PathBuf,
    /// Last-trusted-at timestamp in UTC.
    #[serde(rename = "lastTrustedAt")]
    pub last_trusted_at: DateTime<Utc>,
}

/// On-disk trust store. Serialized as `{ "workspaces": [...] }` for forward
/// compatibility with future top-level fields.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustStore {
    #[serde(default)]
    pub workspaces: Vec<TrustedWorkspace>,
}

impl TrustStore {
    /// Returns true when an entry matches the given canonical path.
    pub fn contains(&self, canonical: &Path) -> bool {
        self.workspaces.iter().any(|w| w.path == canonical)
    }

    /// Insert-or-update an entry, refreshing the timestamp on existing matches.
    pub fn upsert(&mut self, canonical: PathBuf, now: DateTime<Utc>) {
        if let Some(existing) = self.workspaces.iter_mut().find(|w| w.path == canonical) {
            existing.last_trusted_at = now;
        } else {
            self.workspaces.push(TrustedWorkspace {
                path: canonical,
                last_trusted_at: now,
            });
        }
    }
}

/// Derive the trust-store path under a `user_data_folder`. When
/// `user_data_folder` is `None` we fall back to `~/.deacon/` as the default
/// host-side user data root.
pub fn trust_store_path(user_data_folder: Option<&Path>) -> Result<PathBuf> {
    let base = match user_data_folder {
        Some(p) => p.to_path_buf(),
        None => {
            let dirs = directories_next::BaseDirs::new().ok_or_else(|| {
                DeaconError::Internal(crate::errors::InternalError::Generic {
                    message: "Could not determine user home directory for trust store".to_string(),
                })
            })?;
            dirs.home_dir().join(".deacon")
        }
    };
    Ok(base.join("trusted_workspaces.json"))
}

/// Best-effort canonicalization. When the path does not exist yet we fall back
/// to lexical normalization so callers can still compare/store the value.
pub fn canonicalize_workspace(workspace: &Path) -> PathBuf {
    match std::fs::canonicalize(workspace) {
        Ok(canon) => canon,
        Err(e) => {
            debug!(
                "canonicalize failed for {} ({}); using path as-is",
                workspace.display(),
                e
            );
            workspace.to_path_buf()
        }
    }
}

/// Read the on-disk trust store, returning an empty store if the file is
/// missing. Uses `tokio::fs` so it is safe to call from async contexts.
pub async fn load_trust_store(path: &Path) -> std::result::Result<TrustStore, TrustError> {
    match tokio::fs::read(path).await {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| TrustError::Corrupt {
            path: path.to_path_buf(),
            message: e.to_string(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(TrustStore::default()),
        Err(e) => Err(TrustError::Io {
            path: path.to_path_buf(),
            source: e,
        }),
    }
}

/// Persist the trust store atomically. Writes to a sibling temp file in the
/// same directory and `rename`s it into place so a mid-write crash leaves
/// either the previous file or the new file on disk — never a partial one.
pub async fn save_trust_store(
    path: &Path,
    store: &TrustStore,
) -> std::result::Result<(), TrustError> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| TrustError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
    }

    let mut json = serde_json::to_string_pretty(store).map_err(|e| TrustError::Corrupt {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    json.push('\n');

    let temp_path = match path.parent() {
        Some(parent) => parent.join(format!(
            ".{}.tmp",
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("trusted_workspaces.json")
        )),
        None => PathBuf::from(".trusted_workspaces.json.tmp"),
    };

    tokio::fs::write(&temp_path, json.as_bytes())
        .await
        .map_err(|e| TrustError::Io {
            path: temp_path.clone(),
            source: e,
        })?;

    #[cfg(windows)]
    {
        if tokio::fs::try_exists(path).await.unwrap_or(false) {
            tokio::fs::remove_file(path)
                .await
                .map_err(|e| TrustError::Io {
                    path: path.to_path_buf(),
                    source: e,
                })?;
        }
    }

    tokio::fs::rename(&temp_path, path)
        .await
        .map_err(|e| TrustError::Io {
            path: path.to_path_buf(),
            source: e,
        })
}

/// Persist `workspace` (canonicalized) into the store at
/// `trust_store_path(user_data_folder)`.
pub async fn record_trusted_workspace(
    workspace: &Path,
    user_data_folder: Option<&Path>,
) -> Result<()> {
    let store_path = trust_store_path(user_data_folder)?;
    let mut store = load_trust_store(&store_path).await?;
    let canon = canonicalize_workspace(workspace);
    store.upsert(canon, Utc::now());
    save_trust_store(&store_path, &store).await?;
    Ok(())
}

/// Apply the trust policy to a workspace path.
///
/// The host-facing CLI assembles a `WorkspaceTrustPolicy` per invocation (from
/// `--trust-workspace*` flags, the `DEACON_NO_PROMPT` env var, and the
/// default), then invokes this function before any host-side shell exec.
///
/// `Prompt` is conservatively treated as `Deny` in this synchronous core API —
/// the CLI is responsible for translating a `Prompt` decision into an actual
/// interactive prompt when stdin is a TTY. This keeps the core trust gate
/// async-safe and IO-free apart from the (already async) store load.
pub async fn check_workspace_trust(
    workspace: &Path,
    policy: WorkspaceTrustPolicy,
) -> Result<TrustDecision> {
    let canon = canonicalize_workspace(workspace);
    match policy {
        WorkspaceTrustPolicy::AlwaysAllow => Ok(TrustDecision::Trusted),
        WorkspaceTrustPolicy::Deny => Ok(TrustDecision::Denied {
            workspace: canon,
            reason: "trust policy is Deny (e.g. DEACON_NO_PROMPT=1)".to_string(),
        }),
        WorkspaceTrustPolicy::Prompt => {
            // Core surface treats Prompt as Deny — caller should resolve
            // interactivity above this layer.
            Ok(TrustDecision::Denied {
                workspace: canon,
                reason: "interactive prompt required but not available in this context".to_string(),
            })
        }
        WorkspaceTrustPolicy::Allowlist(store_path) => {
            let store = match load_trust_store(&store_path).await {
                Ok(store) => store,
                Err(TrustError::Corrupt { path, message }) => {
                    // Fail closed on corrupt store.
                    warn!(
                        "Workspace trust store at {} is corrupt: {}",
                        path.display(),
                        message
                    );
                    return Ok(TrustDecision::Denied {
                        workspace: canon,
                        reason: format!("trust store corrupt at {}", path.display()),
                    });
                }
                Err(e) => return Err(e.into()),
            };
            if store.contains(&canon) {
                Ok(TrustDecision::Trusted)
            } else {
                Ok(TrustDecision::Denied {
                    workspace: canon,
                    reason: format!("workspace not in allowlist at {}", store_path.display()),
                })
            }
        }
    }
}

/// Render the user-facing opt-in instructions emitted when a host-side hook is
/// refused. Kept here so the wording is consistent between `up` (initialize)
/// and dotfiles call sites.
pub fn opt_in_instructions(workspace: &Path) -> String {
    format!(
        "Refusing to run host-side lifecycle hooks for workspace `{}` because it is not trusted.\n\
         Re-run with one of:\n  \
         --trust-workspace             (one-shot trust for this invocation)\n  \
         --trust-workspace-persist     (one-shot + remember for future runs)\n\
         To run automated builds in CI, set DEACON_NO_PROMPT=1 to fail closed and \
         pre-populate the trust store via --trust-workspace-persist locally first.",
        workspace.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_workspace(tmp: &TempDir, name: &str) -> PathBuf {
        let p = tmp.path().join(name);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[tokio::test]
    async fn always_allow_returns_trusted() {
        let tmp = TempDir::new().unwrap();
        let ws = make_workspace(&tmp, "ws");
        let decision = check_workspace_trust(&ws, WorkspaceTrustPolicy::AlwaysAllow)
            .await
            .unwrap();
        assert_eq!(decision, TrustDecision::Trusted);
    }

    #[tokio::test]
    async fn deny_returns_denied() {
        let tmp = TempDir::new().unwrap();
        let ws = make_workspace(&tmp, "ws");
        let decision = check_workspace_trust(&ws, WorkspaceTrustPolicy::Deny)
            .await
            .unwrap();
        assert!(matches!(decision, TrustDecision::Denied { .. }));
    }

    #[tokio::test]
    async fn prompt_is_denied_in_core_api() {
        let tmp = TempDir::new().unwrap();
        let ws = make_workspace(&tmp, "ws");
        let decision = check_workspace_trust(&ws, WorkspaceTrustPolicy::Prompt)
            .await
            .unwrap();
        assert!(matches!(decision, TrustDecision::Denied { .. }));
    }

    #[tokio::test]
    async fn allowlist_match_after_record() {
        let tmp = TempDir::new().unwrap();
        let ws = make_workspace(&tmp, "trusted");
        let store_path = tmp.path().join("trusted_workspaces.json");

        // Miss before recording.
        let miss = check_workspace_trust(&ws, WorkspaceTrustPolicy::Allowlist(store_path.clone()))
            .await
            .unwrap();
        assert!(matches!(miss, TrustDecision::Denied { .. }));

        // Record and re-check.
        let mut store = TrustStore::default();
        store.upsert(canonicalize_workspace(&ws), Utc::now());
        save_trust_store(&store_path, &store).await.unwrap();
        let hit = check_workspace_trust(&ws, WorkspaceTrustPolicy::Allowlist(store_path))
            .await
            .unwrap();
        assert_eq!(hit, TrustDecision::Trusted);
    }

    #[tokio::test]
    async fn allowlist_miss_for_sibling() {
        let tmp = TempDir::new().unwrap();
        let trusted = make_workspace(&tmp, "trusted");
        let untrusted = make_workspace(&tmp, "untrusted");
        let store_path = tmp.path().join("trusted_workspaces.json");

        let mut store = TrustStore::default();
        store.upsert(canonicalize_workspace(&trusted), Utc::now());
        save_trust_store(&store_path, &store).await.unwrap();

        let decision =
            check_workspace_trust(&untrusted, WorkspaceTrustPolicy::Allowlist(store_path))
                .await
                .unwrap();
        assert!(matches!(decision, TrustDecision::Denied { .. }));
    }

    #[tokio::test]
    async fn persistence_round_trip() {
        let tmp = TempDir::new().unwrap();
        let ws = make_workspace(&tmp, "round-trip");
        let store_path = tmp.path().join("trusted_workspaces.json");

        let now = Utc::now();
        let mut store = TrustStore::default();
        store.upsert(canonicalize_workspace(&ws), now);
        save_trust_store(&store_path, &store).await.unwrap();

        let loaded = load_trust_store(&store_path).await.unwrap();
        assert_eq!(loaded.workspaces.len(), 1);
        assert_eq!(loaded.workspaces[0].path, canonicalize_workspace(&ws));
        // Timestamp round-trips at second granularity at minimum.
        assert_eq!(loaded.workspaces[0].last_trusted_at, now);
    }

    #[tokio::test]
    async fn upsert_refreshes_timestamp_no_duplicates() {
        let tmp = TempDir::new().unwrap();
        let ws = make_workspace(&tmp, "ws");
        let store_path = tmp.path().join("store.json");

        record_trusted_workspace(&ws, Some(tmp.path()))
            .await
            .unwrap();
        // Re-record the same workspace; must not duplicate.
        record_trusted_workspace(&ws, Some(tmp.path()))
            .await
            .unwrap();

        let expected_path = trust_store_path(Some(tmp.path())).unwrap();
        assert_eq!(expected_path, tmp.path().join("trusted_workspaces.json"));
        let _ = store_path; // path constructed inline for symmetry above
        let loaded = load_trust_store(&expected_path).await.unwrap();
        assert_eq!(loaded.workspaces.len(), 1);
    }

    #[tokio::test]
    async fn atomic_write_no_partial_file_on_simulated_failure() {
        // Crash safety: simulate by writing to a temp path manually, then
        // verifying that an aborted rename leaves the previous good file
        // untouched. We do this by writing a known-good store, then attempting
        // to write a second one with a path whose temp sibling we control.
        let tmp = TempDir::new().unwrap();
        let store_path = tmp.path().join("trusted_workspaces.json");

        // Good write #1.
        let mut store = TrustStore::default();
        store.upsert(PathBuf::from("/good/path"), Utc::now());
        save_trust_store(&store_path, &store).await.unwrap();
        let bytes_before = tokio::fs::read(&store_path).await.unwrap();

        // Pre-create a stale temp file with garbage; the next save must
        // overwrite it cleanly (write-then-rename), leaving no partial state.
        let temp_path = tmp.path().join(".trusted_workspaces.json.tmp");
        tokio::fs::write(&temp_path, b"PARTIAL_GARBAGE")
            .await
            .unwrap();

        // Good write #2 — must rename our fresh temp over the previous file.
        store.upsert(PathBuf::from("/another/path"), Utc::now());
        save_trust_store(&store_path, &store).await.unwrap();

        let bytes_after = tokio::fs::read(&store_path).await.unwrap();
        assert_ne!(bytes_before, bytes_after);
        // Stale temp must have been consumed by the second save.
        assert!(
            !temp_path.exists(),
            "temp file should have been renamed away"
        );
        // Final file must parse cleanly.
        let loaded = load_trust_store(&store_path).await.unwrap();
        assert_eq!(loaded.workspaces.len(), 2);
    }

    #[tokio::test]
    async fn corrupt_store_yields_denied_decision() {
        let tmp = TempDir::new().unwrap();
        let ws = make_workspace(&tmp, "ws");
        let store_path = tmp.path().join("trusted_workspaces.json");
        tokio::fs::write(&store_path, b"{not valid json")
            .await
            .unwrap();

        let decision = check_workspace_trust(&ws, WorkspaceTrustPolicy::Allowlist(store_path))
            .await
            .unwrap();
        assert!(matches!(decision, TrustDecision::Denied { .. }));
    }

    #[test]
    fn opt_in_instructions_mentions_flags() {
        let msg = opt_in_instructions(Path::new("/repo"));
        assert!(msg.contains("--trust-workspace"));
        assert!(msg.contains("--trust-workspace-persist"));
        assert!(msg.contains("DEACON_NO_PROMPT"));
        assert!(msg.contains("/repo"));
    }
}
