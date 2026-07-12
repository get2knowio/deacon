//! User-level machine settings (`{user_data_folder}/settings.json`).
//!
//! A small, read-only-in-this-feature settings store that lives alongside the
//! workspace-trust store (`trusted_workspaces.json`) under the host user-data
//! folder. It carries machine-wide scalar preferences ([`Settings::host_ca`],
//! [`Settings::browser`]) and, since 017, named [`Profile`]s a developer can
//! select per run to layer devcontainer configuration fragments and override
//! those scalars without touching the project.
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
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::debug;

/// User-level machine settings, persisted at `{user_data_folder}/settings.json`.
///
/// `#[serde(default)]` + the lack of `deny_unknown_fields` means a missing or
/// unknown key is tolerated rather than fatal, so a newer deacon writing extra
/// keys never breaks an older one (forward compatibility). `Eq` is intentionally
/// not derived: the `extra` passthrough carries `serde_json::Value`, which is
/// only `PartialEq`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    /// Corporate-CA injection activation: `"auto"` or an absolute PEM path.
    /// Absent ⇒ no setting (the lowest-precedence activation source).
    #[serde(rename = "hostCa", default, skip_serializing_if = "Option::is_none")]
    pub host_ca: Option<String>,

    /// Browser program for port auto-open (`onAutoForward: openBrowser`). A bare
    /// program name/path (the forwarded URL is appended). Absent ⇒ fall back to
    /// `DEACON_BROWSER` then the OS default opener. The reserved value `"none"`
    /// (case-insensitive) disables auto-open entirely. See [`crate::browser`].
    #[serde(rename = "browser", default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<String>,

    /// Root-level universal configuration merge fragment (precedence rung 2 —
    /// deep-overlaid on every run, below any selected profile and the CLI
    /// `--merge-config`). The CLI `--override-config` replaces the base config
    /// this layer overlays onto; it does not compete on this rung.
    #[serde(
        rename = "mergeConfig",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub merge_config: Option<MergeConfigPaths>,

    /// Named, mutually-exclusive profiles. Declaration order is preserved so the
    /// "available profiles" list in fail-fast errors reads back in author order.
    /// Empty ⇒ behavior identical to having no profiles at all.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub profiles: IndexMap<String, Profile>,

    /// Profile applied on a bare invocation (no `--profile`/`DEACON_PROFILE`).
    /// Naming an undefined profile is a hard error at resolve time.
    #[serde(
        rename = "defaultProfile",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub default_profile: Option<String>,

    /// Unknown top-level keys, preserved verbatim for forward/backward
    /// compatibility (a newer file loads on an older deacon and vice versa).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A single named entry in [`Settings::profiles`].
///
/// Open to unknown keys via `extra` so future per-profile preferences can be
/// added without breaking older readers. An "empty" profile (no `mergeConfig`
/// and no scalar override) is valid — selecting it applies nothing (an explicit
/// "plain/vanilla" opt-out of a configured default).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// Configuration fragment(s) to deep-overlay when this profile is selected
    /// (precedence rung 3). String or ordered array; later entries win.
    #[serde(
        rename = "mergeConfig",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub merge_config: Option<MergeConfigPaths>,

    /// Overrides the root `hostCa` for this profile; inherits root when unset.
    #[serde(rename = "hostCa", default, skip_serializing_if = "Option::is_none")]
    pub host_ca: Option<String>,

    /// Overrides the root `browser` for this profile; inherits root when unset.
    /// `"none"` (case-insensitive) disables port auto-open.
    #[serde(rename = "browser", default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<String>,

    /// Unknown/future per-profile keys, preserved verbatim.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A profile's `mergeConfig`: a single path or an ordered list of paths.
///
/// Mirrors the [`crate::config::AppPort`] single-or-list idiom. Order is
/// significant: later entries take precedence over earlier ones (FR-012).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MergeConfigPaths {
    /// A single fragment path.
    Single(PathBuf),
    /// An ordered list of fragment paths (later entries win).
    Multiple(Vec<PathBuf>),
}

impl MergeConfigPaths {
    /// View the configured path(s) as an ordered slice (low→high precedence),
    /// regardless of whether the source JSON was a single value or an array.
    pub fn as_slice(&self) -> Cow<'_, [PathBuf]> {
        match self {
            MergeConfigPaths::Single(p) => Cow::Borrowed(std::slice::from_ref(p)),
            MergeConfigPaths::Multiple(v) => Cow::Borrowed(v.as_slice()),
        }
    }
}

/// The resolved outcome of applying a profile selection to a [`Settings`].
///
/// Produced by [`Settings::resolve`]; this is what the CLI consumes. It carries
/// no raw profile map — selection, path resolution, and validation are done.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedSettings {
    /// The applied profile name, or `None` when none applies. Drives the
    /// FR-009b stderr diagnostic.
    pub active_profile: Option<String>,
    /// Ordered, resolved, existence-checked merge fragments (root mergeConfig
    /// then the active profile's mergeConfig). Lowest→highest precedence.
    pub merge_paths: Vec<PathBuf>,
    /// Effective (profile-else-root) corporate-CA activation value.
    pub host_ca: Option<String>,
    /// Effective (profile-else-root) browser value.
    pub browser: Option<String>,
}

/// Fail-fast errors from profile selection/resolution.
#[derive(Debug, Error)]
pub enum ProfileError {
    /// A selected `--profile`/`DEACON_PROFILE`, or a dangling `defaultProfile`,
    /// names a profile that is not defined. Lists available names in
    /// declaration order.
    #[error(
        "Unknown profile '{name}'. Available profiles: {}",
        format_available(available)
    )]
    UnknownProfile {
        /// The undefined name that was requested.
        name: String,
        /// Defined profile names, in declaration order.
        available: Vec<String>,
    },

    /// A referenced `mergeConfig` fragment does not exist. `profile` is
    /// `None` for the root fragment, `Some(name)` for a profile.
    #[error(
        "Profile '{}' references a configuration fragment that does not exist: {}",
        profile.as_deref().unwrap_or("<root>"),
        path.display()
    )]
    MissingFragment {
        /// The owning profile name, or `None` for the root override.
        profile: Option<String>,
        /// The resolved path that could not be found.
        path: PathBuf,
    },
}

/// Render the available-profile list for [`ProfileError::UnknownProfile`].
fn format_available(available: &[String]) -> String {
    if available.is_empty() {
        "(none defined)".to_string()
    } else {
        available.join(", ")
    }
}

impl From<ProfileError> for DeaconError {
    fn from(err: ProfileError) -> Self {
        DeaconError::Config(crate::errors::ConfigError::Validation {
            message: err.to_string(),
        })
    }
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
                debug!(path = %path.display(), host_ca = ?settings.host_ca, profiles = settings.profiles.len(), "Loaded settings");
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

    /// Resolve the effective profile selection against these settings.
    ///
    /// `selected` is the explicit `--profile`/`DEACON_PROFILE` value (if any);
    /// `settings_dir` is the directory the fragment paths resolve against (the
    /// user-data folder). Selection precedence is `selected`, then
    /// `defaultProfile`, then none. The ordered merge paths are
    /// `[root mergeConfig…, active profile mergeConfig…]` (low→high);
    /// relative paths resolve against `settings_dir`, absolute paths are accepted
    /// as-is. Effective scalars are the active-profile value else the root value.
    ///
    /// Fails fast with [`ProfileError::UnknownProfile`] for an unknown selection
    /// or dangling default, and [`ProfileError::MissingFragment`] for a
    /// referenced fragment that does not exist.
    pub fn resolve(
        &self,
        selected: Option<&str>,
        settings_dir: &Path,
    ) -> std::result::Result<ResolvedSettings, ProfileError> {
        // Selection: explicit `--profile`/env wins over `defaultProfile`, which
        // wins over none. A named-but-undefined selection is a hard error,
        // covering both an unknown `--profile` and a dangling `defaultProfile`.
        let active_name = selected.or(self.default_profile.as_deref());
        let active_profile = match active_name {
            Some(name) => {
                let profile =
                    self.profiles
                        .get(name)
                        .ok_or_else(|| ProfileError::UnknownProfile {
                            name: name.to_string(),
                            available: self.profiles.keys().cloned().collect(),
                        })?;
                Some((name.to_string(), profile))
            }
            None => None,
        };

        // Ordered merge paths: root mergeConfig first (rung 2), then the active
        // profile's mergeConfig (rung 3). Later entries win in the merge chain.
        let mut merge_paths = Vec::new();
        if let Some(mc) = &self.merge_config {
            for p in mc.as_slice().iter() {
                merge_paths.push(resolve_fragment(p, settings_dir, None)?);
            }
        }
        if let Some((name, profile)) = &active_profile {
            if let Some(mc) = &profile.merge_config {
                for p in mc.as_slice().iter() {
                    merge_paths.push(resolve_fragment(p, settings_dir, Some(name))?);
                }
            }
        }

        // Effective scalars: active-profile value else root value. With no
        // active profile both fall back to root — identical to today.
        let (host_ca, browser) = match &active_profile {
            Some((_, profile)) => (
                profile.host_ca.clone().or_else(|| self.host_ca.clone()),
                profile.browser.clone().or_else(|| self.browser.clone()),
            ),
            None => (self.host_ca.clone(), self.browser.clone()),
        };

        Ok(ResolvedSettings {
            active_profile: active_profile.map(|(name, _)| name),
            merge_paths,
            host_ca,
            browser,
        })
    }
}

/// Resolve a single fragment path against `settings_dir` (absolute paths pass
/// through) and confirm it exists, else [`ProfileError::MissingFragment`].
fn resolve_fragment(
    path: &Path,
    settings_dir: &Path,
    profile: Option<&str>,
) -> std::result::Result<PathBuf, ProfileError> {
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        settings_dir.join(path)
    };
    if !resolved.exists() {
        return Err(ProfileError::MissingFragment {
            profile: profile.map(String::from),
            path: resolved,
        });
    }
    Ok(resolved)
}

/// Directory the settings file lives in (the user-data folder). Fragment paths
/// in `mergeConfig` resolve relative to it. Honors `--user-data-folder`;
/// falls back to `~/.deacon`.
pub fn settings_dir(user_data_folder: Option<&Path>) -> Result<PathBuf> {
    user_data_root(user_data_folder)
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

    fn write_settings(dir: &Path, json: &str) {
        std::fs::write(dir.join("settings.json"), json).unwrap();
    }

    #[test]
    fn missing_file_is_default_off() {
        let tmp = TempDir::new().unwrap();
        let settings = Settings::load(Some(tmp.path())).unwrap();
        assert_eq!(settings, Settings::default());
        assert!(settings.host_ca.is_none());
        assert!(settings.profiles.is_empty());
    }

    #[test]
    fn host_ca_auto_parses() {
        let tmp = TempDir::new().unwrap();
        write_settings(tmp.path(), r#"{ "hostCa": "auto" }"#);
        let settings = Settings::load(Some(tmp.path())).unwrap();
        assert_eq!(settings.host_ca.as_deref(), Some("auto"));
    }

    #[test]
    fn host_ca_path_parses() {
        let tmp = TempDir::new().unwrap();
        write_settings(tmp.path(), r#"{ "hostCa": "/etc/corp/root.pem" }"#);
        let settings = Settings::load(Some(tmp.path())).unwrap();
        assert_eq!(settings.host_ca.as_deref(), Some("/etc/corp/root.pem"));
    }

    #[test]
    fn unknown_keys_tolerated() {
        let tmp = TempDir::new().unwrap();
        write_settings(
            tmp.path(),
            r#"{ "hostCa": "auto", "futureKey": 42, "nested": { "a": 1 } }"#,
        );
        let settings = Settings::load(Some(tmp.path())).unwrap();
        assert_eq!(settings.host_ca.as_deref(), Some("auto"));
        // Unknown top-level keys round-trip through `extra`.
        assert_eq!(settings.extra.get("futureKey"), Some(&Value::from(42)));
        assert!(settings.extra.contains_key("nested"));
    }

    #[test]
    fn empty_object_is_off() {
        let tmp = TempDir::new().unwrap();
        write_settings(tmp.path(), "{}");
        let settings = Settings::load(Some(tmp.path())).unwrap();
        assert!(settings.host_ca.is_none());
        assert!(settings.browser.is_none());
        assert!(settings.profiles.is_empty());
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn browser_parses_alongside_host_ca() {
        let tmp = TempDir::new().unwrap();
        write_settings(tmp.path(), r#"{ "hostCa": "auto", "browser": "firefox" }"#);
        let settings = Settings::load(Some(tmp.path())).unwrap();
        assert_eq!(settings.host_ca.as_deref(), Some("auto"));
        assert_eq!(settings.browser.as_deref(), Some("firefox"));
    }

    #[test]
    fn corrupt_file_is_error() {
        let tmp = TempDir::new().unwrap();
        write_settings(tmp.path(), "{ not valid json");
        let err = Settings::load(Some(tmp.path())).unwrap_err();
        assert!(err.to_string().contains("Corrupt settings file"));
    }

    #[test]
    fn settings_path_is_sibling_of_trust_store() {
        let tmp = TempDir::new().unwrap();
        let p = settings_path(Some(tmp.path())).unwrap();
        assert_eq!(p, tmp.path().join("settings.json"));
    }

    // --- MergeConfigPaths (T002) ---

    #[test]
    fn merge_config_paths_accepts_string_or_array() {
        let single: MergeConfigPaths = serde_json::from_str(r#""overrides/a.json""#).unwrap();
        assert_eq!(
            single,
            MergeConfigPaths::Single(PathBuf::from("overrides/a.json"))
        );
        assert_eq!(
            single.as_slice().as_ref(),
            &[PathBuf::from("overrides/a.json")]
        );

        let multi: MergeConfigPaths =
            serde_json::from_str(r#"["overrides/a.json", "overrides/b.json"]"#).unwrap();
        assert_eq!(
            multi.as_slice().as_ref(),
            &[
                PathBuf::from("overrides/a.json"),
                PathBuf::from("overrides/b.json"),
            ]
        );
    }

    #[test]
    fn merge_config_paths_preserve_order() {
        let multi: MergeConfigPaths = serde_json::from_str(r#"["z.json", "a.json"]"#).unwrap();
        let slice = multi.as_slice();
        assert_eq!(slice[0], PathBuf::from("z.json"));
        assert_eq!(slice[1], PathBuf::from("a.json"));
    }

    // --- Settings deserialization (T003) ---

    #[test]
    fn profiles_preserve_declaration_order() {
        let tmp = TempDir::new().unwrap();
        write_settings(
            tmp.path(),
            r#"{ "profiles": { "zeta": {}, "alpha": {}, "mid": {} } }"#,
        );
        let settings = Settings::load(Some(tmp.path())).unwrap();
        let names: Vec<&str> = settings.profiles.keys().map(String::as_str).collect();
        assert_eq!(names, vec!["zeta", "alpha", "mid"]);
    }

    #[test]
    fn unknown_per_profile_keys_tolerated() {
        let tmp = TempDir::new().unwrap();
        write_settings(
            tmp.path(),
            r#"{ "profiles": { "dev": { "browser": "firefox", "mode": "future", "nested": { "x": 1 } } } }"#,
        );
        let settings = Settings::load(Some(tmp.path())).unwrap();
        let dev = settings.profiles.get("dev").unwrap();
        assert_eq!(dev.browser.as_deref(), Some("firefox"));
        assert_eq!(dev.extra.get("mode"), Some(&Value::from("future")));
        assert!(dev.extra.contains_key("nested"));
    }

    #[test]
    fn empty_profiles_equals_default() {
        let tmp = TempDir::new().unwrap();
        write_settings(tmp.path(), r#"{ "browser": "firefox", "profiles": {} }"#);
        let settings = Settings::load(Some(tmp.path())).unwrap();
        assert!(settings.profiles.is_empty());
        // With no profiles, resolving no selection yields root scalars only.
        let resolved = settings.resolve(None, tmp.path()).unwrap();
        assert_eq!(
            resolved,
            ResolvedSettings {
                active_profile: None,
                merge_paths: vec![],
                host_ca: None,
                browser: Some("firefox".to_string()),
            }
        );
    }

    // --- ProfileError Display (T004) ---

    #[test]
    fn unknown_profile_lists_available_in_order() {
        let err = ProfileError::UnknownProfile {
            name: "nope".to_string(),
            available: vec!["dev".to_string(), "agent".to_string()],
        };
        let msg = err.to_string();
        assert!(msg.contains("nope"), "{msg}");
        assert!(msg.contains("dev, agent"), "{msg}");
    }

    #[test]
    fn unknown_profile_with_no_profiles_reads_none_defined() {
        let err = ProfileError::UnknownProfile {
            name: "nope".to_string(),
            available: vec![],
        };
        assert!(err.to_string().contains("(none defined)"));
    }

    #[test]
    fn missing_fragment_names_owning_profile_or_root() {
        let profile_err = ProfileError::MissingFragment {
            profile: Some("dev".to_string()),
            path: PathBuf::from("/x/gone.json"),
        };
        assert!(profile_err.to_string().contains("'dev'"));
        assert!(profile_err.to_string().contains("gone.json"));

        let root_err = ProfileError::MissingFragment {
            profile: None,
            path: PathBuf::from("/x/gone.json"),
        };
        assert!(root_err.to_string().contains("<root>"));
    }

    #[test]
    fn profile_error_maps_to_deacon_error() {
        let err: DeaconError = ProfileError::UnknownProfile {
            name: "nope".to_string(),
            available: vec!["dev".to_string()],
        }
        .into();
        assert!(err.to_string().contains("nope"));
    }

    // --- Settings::resolve explicit selection (T008) ---

    fn dir_with_fragments(names: &[&str]) -> TempDir {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("overrides")).unwrap();
        for n in names {
            std::fs::write(tmp.path().join(n), "{}").unwrap();
        }
        tmp
    }

    #[test]
    fn resolve_selected_profile_returns_ordered_paths_root_then_profile() {
        let tmp = dir_with_fragments(&["overrides/root.json", "overrides/dev.json"]);
        write_settings(
            tmp.path(),
            r#"{
                "mergeConfig": "overrides/root.json",
                "profiles": { "dev": { "mergeConfig": "overrides/dev.json" } }
            }"#,
        );
        let settings = Settings::load(Some(tmp.path())).unwrap();
        let resolved = settings.resolve(Some("dev"), tmp.path()).unwrap();
        assert_eq!(resolved.active_profile.as_deref(), Some("dev"));
        assert_eq!(
            resolved.merge_paths,
            vec![
                tmp.path().join("overrides/root.json"),
                tmp.path().join("overrides/dev.json"),
            ]
        );
    }

    #[test]
    fn resolve_unknown_selected_is_error() {
        let tmp = TempDir::new().unwrap();
        write_settings(tmp.path(), r#"{ "profiles": { "dev": {}, "agent": {} } }"#);
        let settings = Settings::load(Some(tmp.path())).unwrap();
        let err = settings.resolve(Some("nope"), tmp.path()).unwrap_err();
        match err {
            ProfileError::UnknownProfile { name, available } => {
                assert_eq!(name, "nope");
                assert_eq!(available, vec!["dev".to_string(), "agent".to_string()]);
            }
            other => panic!("unexpected: {other}"),
        }
    }

    #[test]
    fn resolve_missing_fragment_names_profile() {
        let tmp = TempDir::new().unwrap();
        write_settings(
            tmp.path(),
            r#"{ "profiles": { "dev": { "mergeConfig": "overrides/gone.json" } } }"#,
        );
        let settings = Settings::load(Some(tmp.path())).unwrap();
        let err = settings.resolve(Some("dev"), tmp.path()).unwrap_err();
        match err {
            ProfileError::MissingFragment { profile, path } => {
                assert_eq!(profile.as_deref(), Some("dev"));
                assert_eq!(path, tmp.path().join("overrides/gone.json"));
            }
            other => panic!("unexpected: {other}"),
        }
    }

    #[test]
    fn resolve_relative_paths_against_settings_dir_absolute_as_is() {
        let tmp = dir_with_fragments(&["overrides/rel.json"]);
        let abs = tmp.path().join("abs.json");
        std::fs::write(&abs, "{}").unwrap();
        write_settings(
            tmp.path(),
            &format!(
                r#"{{ "profiles": {{ "dev": {{ "mergeConfig": ["overrides/rel.json", {abs:?}] }} }} }}"#
            ),
        );
        let settings = Settings::load(Some(tmp.path())).unwrap();
        let resolved = settings.resolve(Some("dev"), tmp.path()).unwrap();
        assert_eq!(
            resolved.merge_paths,
            vec![tmp.path().join("overrides/rel.json"), abs]
        );
    }

    #[test]
    fn resolve_ordered_list_preserves_order() {
        let tmp = dir_with_fragments(&["overrides/a.json", "overrides/b.json"]);
        write_settings(
            tmp.path(),
            r#"{ "profiles": { "p": { "mergeConfig": ["overrides/a.json", "overrides/b.json"] } } }"#,
        );
        let settings = Settings::load(Some(tmp.path())).unwrap();
        let resolved = settings.resolve(Some("p"), tmp.path()).unwrap();
        assert_eq!(
            resolved.merge_paths,
            vec![
                tmp.path().join("overrides/a.json"),
                tmp.path().join("overrides/b.json")
            ]
        );
    }

    #[test]
    fn resolve_empty_profile_applies_nothing() {
        let tmp = TempDir::new().unwrap();
        write_settings(
            tmp.path(),
            r#"{ "browser": "firefox", "profiles": { "vanilla": {} } }"#,
        );
        let settings = Settings::load(Some(tmp.path())).unwrap();
        let resolved = settings.resolve(Some("vanilla"), tmp.path()).unwrap();
        assert_eq!(resolved.active_profile.as_deref(), Some("vanilla"));
        assert!(resolved.merge_paths.is_empty());
        // Empty profile inherits the root scalar.
        assert_eq!(resolved.browser.as_deref(), Some("firefox"));
    }

    // --- Three-state selection model (T017) ---

    #[test]
    fn resolve_falls_back_to_default_profile() {
        let tmp = TempDir::new().unwrap();
        write_settings(
            tmp.path(),
            r#"{ "defaultProfile": "dev", "profiles": { "dev": {}, "agent": {} } }"#,
        );
        let settings = Settings::load(Some(tmp.path())).unwrap();
        let resolved = settings.resolve(None, tmp.path()).unwrap();
        assert_eq!(resolved.active_profile.as_deref(), Some("dev"));
    }

    #[test]
    fn resolve_explicit_overrides_default() {
        let tmp = TempDir::new().unwrap();
        write_settings(
            tmp.path(),
            r#"{ "defaultProfile": "dev", "profiles": { "dev": {}, "agent": {} } }"#,
        );
        let settings = Settings::load(Some(tmp.path())).unwrap();
        let resolved = settings.resolve(Some("agent"), tmp.path()).unwrap();
        assert_eq!(resolved.active_profile.as_deref(), Some("agent"));
    }

    #[test]
    fn resolve_no_default_no_selection_is_none() {
        let tmp = TempDir::new().unwrap();
        write_settings(tmp.path(), r#"{ "profiles": { "dev": {}, "agent": {} } }"#);
        let settings = Settings::load(Some(tmp.path())).unwrap();
        let resolved = settings.resolve(None, tmp.path()).unwrap();
        assert_eq!(resolved.active_profile, None);
        assert!(resolved.merge_paths.is_empty());
    }

    #[test]
    fn resolve_dangling_default_is_error() {
        let tmp = TempDir::new().unwrap();
        write_settings(
            tmp.path(),
            r#"{ "defaultProfile": "typo", "profiles": { "dev": {} } }"#,
        );
        let settings = Settings::load(Some(tmp.path())).unwrap();
        let err = settings.resolve(None, tmp.path()).unwrap_err();
        assert!(matches!(err, ProfileError::UnknownProfile { .. }));
    }

    // --- Effective scalar resolution (T019, C7) ---

    #[test]
    fn resolve_profile_scalar_overrides_root() {
        let tmp = TempDir::new().unwrap();
        write_settings(
            tmp.path(),
            r#"{ "browser": "firefox", "profiles": { "agent": { "browser": "none" } } }"#,
        );
        let settings = Settings::load(Some(tmp.path())).unwrap();
        let resolved = settings.resolve(Some("agent"), tmp.path()).unwrap();
        assert_eq!(resolved.browser.as_deref(), Some("none"));
    }

    #[test]
    fn resolve_unset_profile_scalar_inherits_root() {
        let tmp = TempDir::new().unwrap();
        write_settings(
            tmp.path(),
            r#"{ "browser": "firefox", "hostCa": "auto", "profiles": { "dev": {} } }"#,
        );
        let settings = Settings::load(Some(tmp.path())).unwrap();
        let resolved = settings.resolve(Some("dev"), tmp.path()).unwrap();
        assert_eq!(resolved.browser.as_deref(), Some("firefox"));
        assert_eq!(resolved.host_ca.as_deref(), Some("auto"));
    }

    #[test]
    fn resolve_no_active_profile_uses_root_scalars() {
        let tmp = TempDir::new().unwrap();
        write_settings(tmp.path(), r#"{ "browser": "firefox", "hostCa": "auto" }"#);
        let settings = Settings::load(Some(tmp.path())).unwrap();
        let resolved = settings.resolve(None, tmp.path()).unwrap();
        assert_eq!(resolved.browser.as_deref(), Some("firefox"));
        assert_eq!(resolved.host_ca.as_deref(), Some("auto"));
    }
}
