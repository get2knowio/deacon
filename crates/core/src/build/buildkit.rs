//! BuildKit capability detection
//!
//! [`probe_buildkit`] answers whether the builder a runtime's `buildx build`
//! will run is BuildKit, asked of THAT runtime's binary. It is the one place the
//! question is asked, shared by `build` and `up`, so the answer cannot come from
//! a binary the build will not run (#775, #780).
//!
//! [`BuildKitOptions`] tracks which CLI options depend on BuildKit.

use crate::docker::CliRuntime;
use tokio::process::Command;
use tracing::{debug, instrument};

/// The answer to "will this runtime's build run on BuildKit?"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildKitProbe {
    /// Docker, and `<runtime> buildx version` succeeded.
    Available,
    /// Docker, and `<runtime> buildx version` failed or could not be run.
    Unavailable,
    /// Podman. Its builder is Buildah, not BuildKit, so nothing was probed:
    /// `podman buildx version` exits 0 with Buildah's version, which would
    /// answer `Available` for a builder that is not BuildKit. What Buildah can
    /// and cannot serve is the caller's podman-specific decision.
    Buildah,
}

/// Ask the runtime a build will ACTUALLY run whether its builder is BuildKit.
///
/// On Docker this runs `<cli.runtime_path()> buildx version` — the
/// `--docker-path` / resolved binary, never a literal `docker`. A host with a
/// second docker on `PATH`, or with no docker at all because podman was
/// selected, must not decide the answer. On podman nothing is run; see
/// [`BuildKitProbe::Buildah`].
#[instrument(skip(cli), fields(runtime = %cli.runtime_path(), podman = cli.is_podman()))]
pub async fn probe_buildkit(cli: &CliRuntime) -> BuildKitProbe {
    if cli.is_podman() {
        debug!("Podman builds with Buildah; not probing BuildKit");
        return BuildKitProbe::Buildah;
    }
    if buildx_answers(cli.runtime_path()).await {
        BuildKitProbe::Available
    } else {
        BuildKitProbe::Unavailable
    }
}

/// Whether `<runtime_path> buildx version` runs and exits 0. A binary that
/// cannot be spawned counts as "no", and is logged rather than guessed past.
async fn buildx_answers(runtime_path: &str) -> bool {
    match Command::new(runtime_path)
        .args(["buildx", "version"])
        .output()
        .await
    {
        Ok(output) => {
            let available = output.status.success();
            debug!(runtime_path, available, "Probed `buildx version`");
            available
        }
        Err(e) => {
            debug!(runtime_path, error = %e, "Could not run `buildx version`");
            false
        }
    }
}

/// Tracks CLI options that require BuildKit support.
///
/// This struct captures the build options that depend on BuildKit/buildx functionality.
/// Use [`requires_buildkit`](Self::requires_buildkit) to check if any options need BuildKit,
/// and [`buildkit_required_options`](Self::buildkit_required_options) to get a list of
/// which specific options require it.
///
/// # Example
///
/// ```
/// use deacon_core::build::buildkit::BuildKitOptions;
///
/// let options = BuildKitOptions {
///     cache_from: vec!["type=registry,ref=cache:latest".to_string()],
///     cache_to: vec![],
///     builder: Some("mybuilder".to_string()),
/// };
///
/// assert!(options.requires_buildkit());
/// let required = options.buildkit_required_options();
/// assert!(required.contains(&"--cache-from"));
/// assert!(required.contains(&"--builder"));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildKitOptions {
    /// Cache sources for BuildKit builds (--cache-from)
    pub cache_from: Vec<String>,

    /// Cache destinations for BuildKit builds (--cache-to)
    pub cache_to: Vec<String>,

    /// BuildKit builder instance name (--builder)
    pub builder: Option<String>,
}

impl BuildKitOptions {
    /// Returns `true` if any of the options require BuildKit support.
    ///
    /// # Example
    ///
    /// ```
    /// use deacon_core::build::buildkit::BuildKitOptions;
    ///
    /// let empty = BuildKitOptions::default();
    /// assert!(!empty.requires_buildkit());
    ///
    /// let with_cache = BuildKitOptions {
    ///     cache_from: vec!["type=registry,ref=cache".to_string()],
    ///     ..Default::default()
    /// };
    /// assert!(with_cache.requires_buildkit());
    /// ```
    pub fn requires_buildkit(&self) -> bool {
        !self.cache_from.is_empty() || !self.cache_to.is_empty() || self.builder.is_some()
    }

    /// Returns a list of option names that require BuildKit support.
    ///
    /// This is useful for error messages that need to tell the user which
    /// specific options triggered the BuildKit requirement.
    ///
    /// # Example
    ///
    /// ```
    /// use deacon_core::build::buildkit::BuildKitOptions;
    ///
    /// let options = BuildKitOptions {
    ///     cache_from: vec!["type=registry,ref=cache".to_string()],
    ///     cache_to: vec!["type=local,dest=/tmp".to_string()],
    ///     builder: None,
    /// };
    ///
    /// let required = options.buildkit_required_options();
    /// assert_eq!(required.len(), 2);
    /// assert!(required.contains(&"--cache-from"));
    /// assert!(required.contains(&"--cache-to"));
    /// ```
    pub fn buildkit_required_options(&self) -> Vec<&'static str> {
        let mut options = Vec::new();

        if !self.cache_from.is_empty() {
            options.push("--cache-from");
        }
        if !self.cache_to.is_empty() {
            options.push("--cache-to");
        }
        if self.builder.is_some() {
            options.push("--builder");
        }

        options
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // probe_buildkit tests
    // ==========================================================================

    /// A runtime shim that records its argv to `<dir>/argv` and exits `rc`.
    #[cfg(unix)]
    fn recording_shim(dir: &std::path::Path, name: &str, rc: i32) -> String {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        let log = dir.join("argv");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit {rc}\n",
                log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    }

    /// The probe asks the binary it was handed, not a literal `docker`.
    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn probe_asks_the_selected_docker_binary() {
        for (rc, expected) in [
            (0, BuildKitProbe::Available),
            (1, BuildKitProbe::Unavailable),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let shim = recording_shim(dir.path(), "docker-selected", rc);
            let cli = CliRuntime::with_runtime_path(shim);
            assert!(!cli.is_podman());
            assert_eq!(probe_buildkit(&cli).await, expected, "rc={rc}");
            let argv = std::fs::read_to_string(dir.path().join("argv")).unwrap();
            assert_eq!(argv, "buildx version\n", "rc={rc}");
        }
    }

    /// Podman is never asked: `podman buildx version` would answer with
    /// Buildah's version and exit 0.
    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn probe_does_not_ask_podman() {
        let dir = tempfile::tempdir().unwrap();
        let shim = recording_shim(dir.path(), "podman", 0);
        let cli = CliRuntime::with_runtime_path(shim);
        assert!(cli.is_podman());
        assert_eq!(probe_buildkit(&cli).await, BuildKitProbe::Buildah);
        assert!(
            !dir.path().join("argv").exists(),
            "podman was asked a BuildKit question"
        );
    }

    /// A docker binary that cannot be run is "no BuildKit", not an error.
    #[tokio::test(flavor = "current_thread")]
    async fn probe_of_an_unrunnable_docker_is_unavailable() {
        let cli = CliRuntime::with_runtime_path("/nonexistent/deacon-test/docker".to_string());
        assert_eq!(probe_buildkit(&cli).await, BuildKitProbe::Unavailable);
    }

    // ==========================================================================
    // BuildKitOptions tests
    // ==========================================================================

    #[test]
    fn buildkit_options_default_does_not_require_buildkit() {
        let options = BuildKitOptions::default();

        assert!(!options.requires_buildkit());
        assert!(options.buildkit_required_options().is_empty());
    }

    #[test]
    fn buildkit_options_requires_buildkit_for_cache_from() {
        let options = BuildKitOptions {
            cache_from: vec!["type=registry,ref=test".to_string()],
            cache_to: vec![],
            builder: None,
        };

        assert!(options.requires_buildkit());
        assert_eq!(options.buildkit_required_options(), vec!["--cache-from"]);
    }

    #[test]
    fn buildkit_options_requires_buildkit_for_cache_to() {
        let options = BuildKitOptions {
            cache_from: vec![],
            cache_to: vec!["type=local,dest=/tmp".to_string()],
            builder: None,
        };

        assert!(options.requires_buildkit());
        assert_eq!(options.buildkit_required_options(), vec!["--cache-to"]);
    }

    #[test]
    fn buildkit_options_requires_buildkit_for_builder() {
        let options = BuildKitOptions {
            cache_from: vec![],
            cache_to: vec![],
            builder: Some("mybuilder".to_string()),
        };

        assert!(options.requires_buildkit());
        assert_eq!(options.buildkit_required_options(), vec!["--builder"]);
    }

    #[test]
    fn buildkit_options_requires_buildkit_for_combined_options() {
        let options = BuildKitOptions {
            cache_from: vec!["type=registry,ref=from".to_string()],
            cache_to: vec!["type=registry,ref=to".to_string()],
            builder: Some("mybuilder".to_string()),
        };

        assert!(options.requires_buildkit());

        let required = options.buildkit_required_options();
        assert_eq!(required.len(), 3);
        assert!(required.contains(&"--cache-from"));
        assert!(required.contains(&"--cache-to"));
        assert!(required.contains(&"--builder"));
    }

    #[test]
    fn buildkit_options_multiple_cache_entries_still_one_option() {
        let options = BuildKitOptions {
            cache_from: vec![
                "type=registry,ref=cache1".to_string(),
                "type=registry,ref=cache2".to_string(),
                "type=local,src=/tmp".to_string(),
            ],
            cache_to: vec![
                "type=registry,ref=output1".to_string(),
                "type=registry,ref=output2".to_string(),
            ],
            builder: None,
        };

        assert!(options.requires_buildkit());

        // Multiple entries in cache_from/cache_to still count as single options
        let required = options.buildkit_required_options();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&"--cache-from"));
        assert!(required.contains(&"--cache-to"));
    }

    #[test]
    fn buildkit_required_options_preserves_option_order() {
        // Options are listed in a consistent order, for error messages
        let options = BuildKitOptions {
            cache_from: vec!["test".to_string()],
            cache_to: vec!["test".to_string()],
            builder: Some("test".to_string()),
        };

        let required = options.buildkit_required_options();
        // Order should be: cache_from, cache_to, builder
        assert_eq!(required[0], "--cache-from");
        assert_eq!(required[1], "--cache-to");
        assert_eq!(required[2], "--builder");
    }
}
