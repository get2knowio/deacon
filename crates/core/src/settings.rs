//! User-level machine settings (`{user_data_folder}/settings.json`).
//!
//! A small, read-only-in-this-feature settings store that lives alongside the
//! workspace-trust store (`trusted_workspaces.json`) under the host user-data
//! folder. Today its sole field is [`Settings::host_ca`], the persistent
//! activation source for corporate-CA injection (016).
//!
//! **Read-only**: deacon loads this file (tolerating a missing file and unknown
//! keys for forward compatibility) but does not write it here. A
//! `deacon settings get/set` write command — atomic temp-file + `fs::rename`
//! per `cache/disk.rs::save_index`, user-data folder only — is deferred to
//! issue #198.
//!
//! **Source boundary**: read only from the user-data folder, never from the
//! workspace. Nothing in `devcontainer.json` can populate it (machine-owner
//! controlled, mirrors the trust gate's threat model — see `SECURITY.md`).

use crate::errors::{DeaconError, InternalError, Result};
use crate::trust::user_data_root;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::debug;

/// User-level machine settings, persisted at `{user_data_folder}/settings.json`.
///
/// `#[serde(default)]` + the lack of `deny_unknown_fields` means a missing or
/// unknown key is tolerated rather than fatal, so a newer deacon writing extra
/// keys never breaks an older one (forward compatibility).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    /// Corporate-CA injection activation: `"auto"` or an absolute PEM path.
    /// Absent ⇒ no setting (the lowest-precedence activation source).
    #[serde(rename = "hostCa", default, skip_serializing_if = "Option::is_none")]
    pub host_ca: Option<String>,

    /// Browser program for port auto-open (`onAutoForward: openBrowser`). A bare
    /// program name/path (the forwarded URL is appended). Absent ⇒ fall back to
    /// `DEACON_BROWSER` then the OS default opener. See [`crate::browser`].
    #[serde(rename = "browser", default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<String>,
}

impl Settings {
    /// Read the settings file under `user_data_folder`.
    ///
    /// A missing file yields [`Settings::default`] (no settings) rather than an
    /// error. Unknown keys are tolerated. A present-but-corrupt file is an
    /// error so a malformed machine policy never silently degrades to "off".
    pub fn load(user_data_folder: Option<&Path>) -> Result<Self> {
        let path = settings_path(user_data_folder)?;
        match std::fs::read(&path) {
            Ok(bytes) => {
                let settings: Settings = serde_json::from_slice(&bytes).map_err(|e| {
                    DeaconError::Internal(InternalError::Generic {
                        message: format!("Corrupt settings file at {}: {}", path.display(), e),
                    })
                })?;
                debug!(path = %path.display(), host_ca = ?settings.host_ca, "Loaded settings");
                Ok(settings)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                debug!(path = %path.display(), "No settings file; using defaults");
                Ok(Settings::default())
            }
            Err(e) => Err(DeaconError::Internal(InternalError::Generic {
                message: format!("Failed to read settings file at {}: {}", path.display(), e),
            })),
        }
    }
}

/// Path to the settings file under `user_data_folder` (a sibling of
/// `trusted_workspaces.json`). Honors `--user-data-folder`; falls back to
/// `~/.deacon/settings.json`.
pub fn settings_path(user_data_folder: Option<&Path>) -> Result<PathBuf> {
    Ok(user_data_root(user_data_folder)?.join("settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn missing_file_is_default_off() {
        let tmp = TempDir::new().unwrap();
        let settings = Settings::load(Some(tmp.path())).unwrap();
        assert_eq!(settings, Settings::default());
        assert!(settings.host_ca.is_none());
    }

    #[test]
    fn host_ca_auto_parses() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("settings.json"), r#"{ "hostCa": "auto" }"#).unwrap();
        let settings = Settings::load(Some(tmp.path())).unwrap();
        assert_eq!(settings.host_ca.as_deref(), Some("auto"));
    }

    #[test]
    fn host_ca_path_parses() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("settings.json"),
            r#"{ "hostCa": "/etc/corp/root.pem" }"#,
        )
        .unwrap();
        let settings = Settings::load(Some(tmp.path())).unwrap();
        assert_eq!(settings.host_ca.as_deref(), Some("/etc/corp/root.pem"));
    }

    #[test]
    fn unknown_keys_tolerated() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("settings.json"),
            r#"{ "hostCa": "auto", "futureKey": 42, "nested": { "a": 1 } }"#,
        )
        .unwrap();
        let settings = Settings::load(Some(tmp.path())).unwrap();
        assert_eq!(settings.host_ca.as_deref(), Some("auto"));
    }

    #[test]
    fn empty_object_is_off() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("settings.json"), "{}").unwrap();
        let settings = Settings::load(Some(tmp.path())).unwrap();
        assert!(settings.host_ca.is_none());
        assert!(settings.browser.is_none());
    }

    #[test]
    fn browser_parses_alongside_host_ca() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("settings.json"),
            r#"{ "hostCa": "auto", "browser": "firefox" }"#,
        )
        .unwrap();
        let settings = Settings::load(Some(tmp.path())).unwrap();
        assert_eq!(settings.host_ca.as_deref(), Some("auto"));
        assert_eq!(settings.browser.as_deref(), Some("firefox"));
    }

    #[test]
    fn corrupt_file_is_error() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("settings.json"), "{ not valid json").unwrap();
        let err = Settings::load(Some(tmp.path())).unwrap_err();
        assert!(err.to_string().contains("Corrupt settings file"));
    }

    #[test]
    fn settings_path_is_sibling_of_trust_store() {
        let tmp = TempDir::new().unwrap();
        let p = settings_path(Some(tmp.path())).unwrap();
        assert_eq!(p, tmp.path().join("settings.json"));
    }
}
