//! Corporate-CA injection: the in-container install script and the runtime
//! orchestration that streams the bundle in and runs it.
//!
//! One idempotent POSIX `sh` script does the work both at runtime (bundle on
//! stdin, via [`exec_with_stdin`](crate::docker::Docker::exec_with_stdin)) and
//! at build time (bundle mounted as a file, run as a generated `RUN` step). It
//! writes the canonical bundle unconditionally — so the env-var-only fallback
//! always has a real file to point at — then installs into the distro trust
//! store, exiting with a sentinel that maps to an [`InjectionOutcome`].

use crate::docker::{Docker, ExecConfig};
use crate::errors::Result;
use crate::host_ca::discover::CorporateCaSet;
use crate::host_ca::env::{HOST_CA_BUNDLE_PATH, HOST_CA_BUNDLE_PATH as CANONICAL};
use std::collections::HashMap;
use tracing::{info, instrument, warn};

/// Directory holding the canonical in-container PEM bundle.
const CANONICAL_DIR: &str = "/usr/local/share/deacon";

/// Sentinel exit codes from the install script (Decision 4 / contract §7).
const EXIT_UNSUPPORTED_DISTRO: i32 = 10;
const EXIT_NOT_ROOT: i32 = 11;

/// How the CA ended up in the container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectionMode {
    /// The distro trust store was updated (`update-ca-certificates` /
    /// `update-ca-trust`).
    SystemStore,
    /// Only the canonical bundle file + CA env vars were set (unsupported
    /// distro or non-root) — FR-022.
    EnvVarOnly,
}

impl InjectionMode {
    /// Span/JSON outcome string.
    pub fn as_str(&self) -> &'static str {
        match self {
            InjectionMode::SystemStore => "system_store",
            InjectionMode::EnvVarOnly => "env_var_only",
        }
    }
}

/// Result of a runtime injection attempt (surfaced in logs/labels/JSON).
#[derive(Debug, Clone)]
pub struct InjectionOutcome {
    /// Whether the system store was updated or we fell back to env-var-only.
    pub mode: InjectionMode,
    /// Canonical in-container PEM path.
    pub bundle_path: String,
    /// Subjects actually injected (FR-028).
    pub injected_subjects: Vec<String>,
    /// Actionable warning populated on the fallback path.
    pub warning: Option<String>,
}

/// The distro-detection + system-store install core, shared by the runtime and
/// build scripts. Assumes the canonical bundle is already written. Ends by
/// `exit`-ing with `0` (installed), [`EXIT_UNSUPPORTED_DISTRO`], or
/// [`EXIT_NOT_ROOT`]; a present-but-failing updater exits non-zero with its own
/// code (mapped to the "unexpected failure" outcome).
fn install_core_script() -> String {
    format!(
        r#"
if [ "$(id -u 2>/dev/null || echo 1)" != "0" ]; then
  echo "deacon: host-CA install needs root; falling back to env-var-only" 1>&2
  exit {not_root}
fi
ID=""; ID_LIKE=""
if [ -r /etc/os-release ]; then . /etc/os-release; fi
FAMILY="$ID $ID_LIKE"
split_into() {{
  # $1 = dest dir; split the canonical bundle into one file per certificate so
  # update-ca-certificates (which reads only the first cert per file) trusts all.
  mkdir -p "$1"
  awk -v dir="$1" '
    /-----BEGIN CERTIFICATE-----/ {{ n++; fn = dir "/deacon-host-ca-" n ".crt" }}
    {{ if (fn) print > fn }}
  ' "{canonical}"
}}
case "$FAMILY" in
  *debian*|*ubuntu*)
    if command -v update-ca-certificates >/dev/null 2>&1; then
      split_into /usr/local/share/ca-certificates
      update-ca-certificates >/dev/null 2>&1 || exit 1
      exit 0
    fi ;;
  *rhel*|*fedora*|*centos*|*rocky*|*almalinux*)
    if command -v update-ca-trust >/dev/null 2>&1; then
      mkdir -p /etc/pki/ca-trust/source/anchors
      cp "{canonical}" /etc/pki/ca-trust/source/anchors/deacon-host-ca.crt
      update-ca-trust extract >/dev/null 2>&1 || exit 1
      exit 0
    fi ;;
  *alpine*)
    if command -v update-ca-certificates >/dev/null 2>&1; then
      split_into /usr/local/share/ca-certificates
      update-ca-certificates >/dev/null 2>&1 || exit 1
      exit 0
    fi ;;
esac
echo "deacon: unsupported distro for system trust store; env-var-only" 1>&2
exit {unsupported}
"#,
        canonical = CANONICAL,
        not_root = EXIT_NOT_ROOT,
        unsupported = EXIT_UNSUPPORTED_DISTRO,
    )
}

/// The runtime install script: capture the bundle from stdin into the canonical
/// path, then run the install core.
pub fn runtime_install_script() -> String {
    format!(
        "set -e\nmkdir -p {dir}\ncat > {canonical}\nset +e\n{core}",
        dir = CANONICAL_DIR,
        canonical = CANONICAL,
        core = install_core_script(),
    )
}

/// The build install script: copy the mounted bundle at `src` into the
/// canonical path, then run the install core. Build always runs as root, so a
/// non-root sentinel cannot occur; an *unsupported distro* during build is
/// treated as success (the canonical file is still written for env use) so a
/// generated feature build never hard-fails purely on distro detection.
pub fn build_install_script(src: &str) -> String {
    format!(
        r#"set -e
mkdir -p {dir}
cp "{src}" {canonical}
set +e
(
{core}
)
rc=$?
if [ "$rc" = "{unsupported}" ] || [ "$rc" = "{not_root}" ]; then
  exit 0
fi
exit $rc
"#,
        dir = CANONICAL_DIR,
        src = src,
        canonical = CANONICAL,
        core = install_core_script(),
        unsupported = EXIT_UNSUPPORTED_DISTRO,
        not_root = EXIT_NOT_ROOT,
    )
}

/// Map an install-script exit code + captured stderr to an [`InjectionOutcome`].
fn outcome_from_exit(exit_code: i32, stderr: &str, set: &CorporateCaSet) -> InjectionOutcome {
    let bundle_path = HOST_CA_BUNDLE_PATH.to_string();
    let injected_subjects = set.subjects.clone();
    match exit_code {
        0 => InjectionOutcome {
            mode: InjectionMode::SystemStore,
            bundle_path,
            injected_subjects,
            warning: None,
        },
        EXIT_UNSUPPORTED_DISTRO => InjectionOutcome {
            mode: InjectionMode::EnvVarOnly,
            bundle_path,
            injected_subjects,
            warning: Some(
                "Unsupported container distro for system trust store; CA available via env vars \
                 only (SSL_CERT_FILE, NODE_EXTRA_CA_CERTS, …) pointing at the written bundle."
                    .to_string(),
            ),
        },
        EXIT_NOT_ROOT => InjectionOutcome {
            mode: InjectionMode::EnvVarOnly,
            bundle_path,
            injected_subjects,
            warning: Some(
                "Container exec user is not root; could not update the system trust store. CA \
                 available via env vars only pointing at the written bundle."
                    .to_string(),
            ),
        },
        other => InjectionOutcome {
            mode: InjectionMode::EnvVarOnly,
            bundle_path,
            injected_subjects,
            warning: Some(format!(
                "Host-CA system-store install failed (exit {}): {}. Falling back to env-var-only.",
                other,
                stderr.trim()
            )),
        },
    }
}

/// Orchestrate runtime injection: stream the PEM bundle into the container and
/// run the install script, returning the mapped outcome. Logs every injected
/// subject under the `ca.inject` span.
#[instrument(skip(runtime, set), fields(container_id = %container_id, subject_count = set.count))]
pub async fn inject_runtime<D: Docker>(
    runtime: &D,
    container_id: &str,
    set: &CorporateCaSet,
) -> Result<InjectionOutcome> {
    let script = runtime_install_script();
    let command = vec!["sh".to_string(), "-c".to_string(), script];

    let config = ExecConfig {
        user: None,
        working_dir: None,
        env: HashMap::new(),
        tty: false,
        interactive: true,
        detach: false,
        // Capture output so we can map the exit code + stderr to an outcome;
        // keep it off deacon's stdout (the result JSON owns stdout).
        silent: true,
        stdout_to_stderr: false,
        terminal_size: None,
    };

    let result = runtime
        .exec_with_stdin(container_id, &command, set.pem_bundle.as_bytes(), &config)
        .await?;

    let outcome = outcome_from_exit(result.exit_code, &result.stderr, set);

    match &outcome.warning {
        Some(w) => warn!(outcome = outcome.mode.as_str(), "{}", w),
        None => info!(
            outcome = outcome.mode.as_str(),
            bundle_path = %outcome.bundle_path,
            "Installed corporate CA into container system store"
        ),
    }
    for subject in &outcome.injected_subjects {
        info!(subject = %subject, outcome = outcome.mode.as_str(), "injected CA");
    }

    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_ca::discover::CorporateCaSet;

    #[test]
    fn runtime_script_captures_stdin_and_detects_distro() {
        let s = runtime_install_script();
        assert!(s.contains("cat > /usr/local/share/deacon/host-ca.crt"));
        assert!(s.contains("/etc/os-release"));
        assert!(s.contains("update-ca-certificates"));
        assert!(s.contains("update-ca-trust extract"));
        assert!(s.contains(&format!("exit {}", EXIT_UNSUPPORTED_DISTRO)));
        assert!(s.contains(&format!("exit {}", EXIT_NOT_ROOT)));
    }

    #[test]
    fn build_script_copies_mounted_file_and_tolerates_unsupported() {
        let s = build_install_script("/tmp/deacon-ca/bundle.pem");
        assert!(s.contains("cp \"/tmp/deacon-ca/bundle.pem\" /usr/local/share/deacon/host-ca.crt"));
        // Unsupported/non-root during build must not fail the image build.
        assert!(s.contains("exit 0"));
    }

    fn set_with(n: usize) -> CorporateCaSet {
        CorporateCaSet {
            subjects: (0..n).map(|i| format!("CN=Corp {}", i)).collect(),
            pem_bundle: "PEM".to_string(),
            count: n,
        }
    }

    #[test]
    fn exit_zero_is_system_store() {
        let o = outcome_from_exit(0, "", &set_with(1));
        assert_eq!(o.mode, InjectionMode::SystemStore);
        assert!(o.warning.is_none());
        assert_eq!(o.injected_subjects.len(), 1);
        assert_eq!(o.bundle_path, HOST_CA_BUNDLE_PATH);
    }

    #[test]
    fn exit_10_is_unsupported_envonly() {
        let o = outcome_from_exit(EXIT_UNSUPPORTED_DISTRO, "", &set_with(1));
        assert_eq!(o.mode, InjectionMode::EnvVarOnly);
        assert!(o.warning.as_ref().unwrap().contains("Unsupported"));
    }

    #[test]
    fn exit_11_is_not_root_envonly() {
        let o = outcome_from_exit(EXIT_NOT_ROOT, "", &set_with(1));
        assert_eq!(o.mode, InjectionMode::EnvVarOnly);
        assert!(o.warning.as_ref().unwrap().contains("not root"));
    }

    #[test]
    fn other_exit_is_envonly_with_stderr() {
        let o = outcome_from_exit(7, "boom", &set_with(1));
        assert_eq!(o.mode, InjectionMode::EnvVarOnly);
        let w = o.warning.unwrap();
        assert!(w.contains("exit 7"));
        assert!(w.contains("boom"));
    }
}
