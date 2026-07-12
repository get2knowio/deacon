//! Shared glue for user-scoped profile selection (017).
//!
//! Every subcommand that honors `--override-config` (`up`, `read-configuration`,
//! `build`, `outdated`) resolves the `--profile`/`DEACON_PROFILE` selection the
//! same way through [`resolve_active_profile`]: it loads the read-only
//! `{user_data_folder}/settings.json`, applies the selection precedence
//! (`--profile` > `DEACON_PROFILE` > `defaultProfile` > none), validates it, and
//! returns the ordered override fragments plus the effective scalar settings.
//!
//! **Source boundary**: profiles are read only from the user-data folder, never
//! from the workspace (FR-019), mirroring the settings/trust threat model.

use anyhow::Result;
use deacon_core::settings::{ResolvedSettings, Settings, settings_dir};
use std::path::Path;
use tracing::info;

/// Load settings and resolve the active profile selection.
///
/// `selected` is the `--profile`/`DEACON_PROFILE` value (already merged by clap).
/// On success, emits an stderr (`tracing`) diagnostic naming the applied profile
/// and its source when one applies (FR-009b) — this never touches the
/// stdout/JSON output contract. A missing/profiles-free settings file yields an
/// empty resolution (behavior identical to today). An unknown selection, a
/// dangling `defaultProfile`, or a missing fragment fails fast.
pub(crate) fn resolve_active_profile(
    user_data_folder: Option<&Path>,
    selected: Option<&str>,
) -> Result<ResolvedSettings> {
    let settings = Settings::load(user_data_folder)?;
    let dir = settings_dir(user_data_folder)?;
    let resolved = settings.resolve(selected, &dir)?;

    if let Some(name) = &resolved.active_profile {
        let source = if selected.is_some() {
            "--profile/DEACON_PROFILE"
        } else {
            "defaultProfile"
        };
        info!(profile = %name, source, "Applying profile from settings.json");
    }

    Ok(resolved)
}

/// Whether a resolved override-fragment path is authored inside the user-data
/// folder (the machine owner's trusted location).
///
/// Drives the FR-020a host-hook trust refinement: a profile fragment loaded from
/// inside the user-data folder is owner-authored (trust follows author), while a
/// fragment referenced by an absolute path *outside* it is not owner-guaranteed
/// and stays subject to the workspace-trust gate.
pub(crate) fn override_authored_in_user_data(path: &Path, settings_dir: &Path) -> bool {
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    canon(path).starts_with(canon(settings_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn missing_settings_file_yields_empty_resolution() {
        let tmp = TempDir::new().unwrap();
        let resolved = resolve_active_profile(Some(tmp.path()), None).unwrap();
        assert_eq!(resolved.active_profile, None);
        assert!(resolved.merge_paths.is_empty());
    }

    #[test]
    fn unknown_profile_is_hard_error() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("settings.json"),
            r#"{ "profiles": { "dev": {}, "agent": {} } }"#,
        )
        .unwrap();
        let err = resolve_active_profile(Some(tmp.path()), Some("nope")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("nope"), "{msg}");
        assert!(msg.contains("dev, agent"), "{msg}");
    }

    #[test]
    fn authored_in_user_data_distinguishes_inside_and_outside() {
        let udf = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let inside = udf.path().join("overrides/dev.json");
        std::fs::create_dir_all(inside.parent().unwrap()).unwrap();
        std::fs::write(&inside, "{}").unwrap();
        let out_path = outside.path().join("repo.json");
        std::fs::write(&out_path, "{}").unwrap();

        assert!(override_authored_in_user_data(&inside, udf.path()));
        assert!(!override_authored_in_user_data(&out_path, udf.path()));
    }
}
