//! Host-CA activation decision and its precedence resolution.
//!
//! Activation answers "should deacon inject a corporate CA, and from where?".
//! It resolves with a fixed precedence — **CLI flag > `DEACON_INJECT_HOST_CA`
//! env > `settings.json` > Off** — and is **never** sourced from any
//! workspace-resident config (FR-015). This mirrors `core::trust::resolve_policy`
//! so the codebase has one activation-precedence shape.

use crate::settings::Settings;
use std::path::PathBuf;

/// The resolved host-CA activation decision (research Decision 7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostCaActivation {
    /// No discovery, no injection — the default and the only state for the
    /// byte-stable unconfigured path (FR-029).
    Off,
    /// Auto-discover corporate roots from the host trust store.
    Auto,
    /// Use this PEM bundle verbatim as the corporate set.
    ExplicitPath(PathBuf),
}

impl HostCaActivation {
    /// True when injection is enabled (Auto or ExplicitPath).
    pub fn is_enabled(&self) -> bool {
        !matches!(self, HostCaActivation::Off)
    }

    /// A short mode string for span fields (`auto` / `explicit` / `off`).
    pub fn mode_str(&self) -> &'static str {
        match self {
            HostCaActivation::Off => "off",
            HostCaActivation::Auto => "auto",
            HostCaActivation::ExplicitPath(_) => "explicit",
        }
    }
}

/// Map a raw string value (`"auto"` or a path) to an activation. An empty
/// string is treated as `Auto` (a present-but-valueless source).
fn value_to_activation(value: &str) -> HostCaActivation {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        HostCaActivation::Auto
    } else {
        HostCaActivation::ExplicitPath(PathBuf::from(trimmed))
    }
}

/// Resolve the activation decision from the three machine-owner sources, in
/// precedence order.
///
/// - `cli`: `Some(value)` when the `--inject-host-ca` flag is present. A
///   valueless flag arrives as `Some("auto")` (clap `default_missing_value`),
///   but `Some("")` is also accepted as Auto for robustness.
/// - `env`: the raw `DEACON_INJECT_HOST_CA` value, if set.
/// - `settings`: the loaded `settings.json` (its `host_ca` field).
///
/// **Never** pass any workspace-sourced value here (FR-015).
pub fn resolve_host_ca_activation(
    cli: Option<&str>,
    env: Option<&str>,
    settings: &Settings,
) -> HostCaActivation {
    if let Some(value) = cli {
        return value_to_activation(value);
    }
    if let Some(value) = env {
        if !value.trim().is_empty() {
            return value_to_activation(value);
        }
    }
    if let Some(value) = settings.host_ca.as_deref() {
        if !value.trim().is_empty() {
            return value_to_activation(value);
        }
    }
    HostCaActivation::Off
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with(host_ca: Option<&str>) -> Settings {
        Settings {
            host_ca: host_ca.map(String::from),
        }
    }

    #[test]
    fn default_is_off() {
        let a = resolve_host_ca_activation(None, None, &settings_with(None));
        assert_eq!(a, HostCaActivation::Off);
    }

    #[test]
    fn valueless_cli_flag_is_auto() {
        // clap default_missing_value = "auto"
        let a = resolve_host_ca_activation(Some("auto"), None, &settings_with(None));
        assert_eq!(a, HostCaActivation::Auto);
        // Robustness: an empty string also maps to Auto.
        let a = resolve_host_ca_activation(Some(""), None, &settings_with(None));
        assert_eq!(a, HostCaActivation::Auto);
    }

    #[test]
    fn cli_path_is_explicit() {
        let a = resolve_host_ca_activation(Some("/etc/corp/root.pem"), None, &settings_with(None));
        assert_eq!(
            a,
            HostCaActivation::ExplicitPath(PathBuf::from("/etc/corp/root.pem"))
        );
    }

    #[test]
    fn cli_beats_env_and_settings() {
        let a = resolve_host_ca_activation(
            Some("/from/cli.pem"),
            Some("auto"),
            &settings_with(Some("/from/settings.pem")),
        );
        assert_eq!(
            a,
            HostCaActivation::ExplicitPath(PathBuf::from("/from/cli.pem"))
        );
    }

    #[test]
    fn env_beats_settings() {
        let a =
            resolve_host_ca_activation(None, Some("/from/env.pem"), &settings_with(Some("auto")));
        assert_eq!(
            a,
            HostCaActivation::ExplicitPath(PathBuf::from("/from/env.pem"))
        );
    }

    #[test]
    fn settings_used_when_cli_and_env_absent() {
        let a = resolve_host_ca_activation(None, None, &settings_with(Some("auto")));
        assert_eq!(a, HostCaActivation::Auto);
    }

    #[test]
    fn empty_env_falls_through_to_settings() {
        let a = resolve_host_ca_activation(None, Some(""), &settings_with(Some("auto")));
        assert_eq!(a, HostCaActivation::Auto);
    }

    #[test]
    fn empty_settings_value_is_off() {
        let a = resolve_host_ca_activation(None, None, &settings_with(Some("  ")));
        assert_eq!(a, HostCaActivation::Off);
    }

    #[test]
    fn mode_str_and_is_enabled() {
        assert_eq!(HostCaActivation::Off.mode_str(), "off");
        assert!(!HostCaActivation::Off.is_enabled());
        assert_eq!(HostCaActivation::Auto.mode_str(), "auto");
        assert!(HostCaActivation::Auto.is_enabled());
        assert_eq!(
            HostCaActivation::ExplicitPath(PathBuf::from("/x")).mode_str(),
            "explicit"
        );
    }
}
