//! Oracle pin loading, resolution, and exact-version verification (research D3).
//!
//! The pin (`fixtures/parity-corpus/oracle.json`) is embedded at compile time via
//! `include_str!`, so a malformed pin fails every parity test loudly the moment
//! [`Oracle::acquire`] runs. Resolution honors the `DEACON_PARITY_DEVCONTAINER`
//! override (the documented local workflow and the fault-injection seam) before
//! falling back to a `PATH` lookup. The resolved binary's `--version` is compared
//! **exactly** against the pin — a passing parity run must certify against exactly
//! the pinned reference, never a "close enough" version.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use crate::HarnessError;

/// Path override for the oracle binary (research D3; also the fault-injection seam).
pub const ORACLE_OVERRIDE_ENV: &str = "DEACON_PARITY_DEVCONTAINER";

/// The pinned reference package name; the oracle must be this package.
pub const ORACLE_PACKAGE: &str = "@devcontainers/cli";

/// The compile-time-embedded pin. A malformed pin is a hard, loud failure.
const ORACLE_PIN_JSON: &str = include_str!("../../../fixtures/parity-corpus/oracle.json");

/// Bound on the `--version` query (2 min — matches the config-only ceiling).
pub const VERSION_QUERY_BOUND: Duration = Duration::from_secs(120);

/// The single authoritative oracle pin (data-model §1).
///
/// `Serialize` is derived so the aggregator can echo the pin verbatim into the
/// `oracle.pin` block of `parity-report.json` (`deny_unknown_fields` governs
/// deserialization only).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OraclePin {
    /// Must be exactly `@devcontainers/cli`.
    pub package: String,
    /// Exact semver, no range operators.
    pub version: String,
}

impl OraclePin {
    /// The (symbolic) location of the embedded pin, used in error messages.
    fn pin_path() -> PathBuf {
        crate::workspace_root().join("fixtures/parity-corpus/oracle.json")
    }

    /// Load and validate the embedded pin.
    pub fn load() -> Result<OraclePin, HarnessError> {
        Self::parse(ORACLE_PIN_JSON)
    }

    /// Parse an arbitrary pin document (exposed for unit tests / the malformed-pin
    /// rejection case). Unknown fields are rejected; the package must match.
    pub fn parse(raw: &str) -> Result<OraclePin, HarnessError> {
        let pin: OraclePin =
            serde_json::from_str(raw).map_err(|e| HarnessError::OracleUnverifiable {
                path: Self::pin_path(),
                cause: format!("malformed oracle pin: {e}"),
            })?;
        if pin.package != ORACLE_PACKAGE {
            return Err(HarnessError::OracleUnverifiable {
                path: Self::pin_path(),
                cause: format!(
                    "oracle pin package is {:?}, expected {ORACLE_PACKAGE:?}",
                    pin.package
                ),
            });
        }
        Ok(pin)
    }
}

/// Which resolution strategy produced the oracle binary (edge case: two oracles
/// resolvable — the override always wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OracleSource {
    /// Resolved from `DEACON_PARITY_DEVCONTAINER`.
    Override,
    /// Resolved from a `PATH` lookup.
    PathLookup,
}

/// A verified oracle: the binary actually invoked, how it was resolved, and its
/// reported version (guaranteed equal to the pin). Cached process-wide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedOracle {
    pub path: PathBuf,
    pub source: OracleSource,
    pub version: String,
}

/// Zero-sized entry point for oracle acquisition.
pub struct Oracle;

/// Process-wide cache: one subprocess `--version` query per test binary.
static VERIFIED: OnceLock<Result<VerifiedOracle, HarnessError>> = OnceLock::new();

impl Oracle {
    /// Resolve the oracle (override → `PATH`), verify its version equals the pin
    /// under the 2-minute bound, and cache the result process-wide. Any failure
    /// returns a cause-specific [`HarnessError`]; test binaries `.expect()` the
    /// result so a failure FAILS the test with that message — never a silent skip.
    pub async fn acquire() -> Result<VerifiedOracle, HarnessError> {
        if let Some(cached) = VERIFIED.get() {
            return cached.clone();
        }
        let computed = Self::resolve_and_verify().await;
        match VERIFIED.set(computed.clone()) {
            Ok(()) => computed,
            // Another task set it first; return whatever is now cached.
            Err(_) => VERIFIED.get().cloned().unwrap_or(computed),
        }
    }

    async fn resolve_and_verify() -> Result<VerifiedOracle, HarnessError> {
        let pin = OraclePin::load()?;
        let override_path = std::env::var_os(ORACLE_OVERRIDE_ENV).map(PathBuf::from);
        let path_env = std::env::var_os("PATH");
        let (bin, source) = resolve_binary(override_path, path_env.as_deref())?;
        verify(&bin, source, &pin, VERSION_QUERY_BOUND).await
    }
}

/// Resolve the oracle binary path and how it was found. Pure over its inputs so it
/// is unit-testable without mutating process env.
fn resolve_binary(
    override_path: Option<PathBuf>,
    path_env: Option<&OsStr>,
) -> Result<(PathBuf, OracleSource), HarnessError> {
    if let Some(p) = override_path {
        if p.is_file() {
            return Ok((p, OracleSource::Override));
        }
        return Err(HarnessError::OracleMissing {
            hint: format!("{ORACLE_OVERRIDE_ENV} points at {p:?}, which is not an existing file"),
        });
    }
    match find_on_path("devcontainer", path_env) {
        Some(p) => Ok((p, OracleSource::PathLookup)),
        None => Err(HarnessError::OracleMissing {
            hint: "`devcontainer` was not found on PATH".to_string(),
        }),
    }
}

/// Locate an executable-looking file named `name` in the `PATH`-style value.
fn find_on_path(name: &str, path_env: Option<&OsStr>) -> Option<PathBuf> {
    let path_env = path_env?;
    for dir in std::env::split_paths(path_env) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Verify a resolved binary reports exactly the pinned version.
async fn verify(
    bin: &Path,
    source: OracleSource,
    pin: &OraclePin,
    bound: Duration,
) -> Result<VerifiedOracle, HarnessError> {
    let reported = query_version(bin, bound).await?;
    if !looks_like_version(&reported) {
        return Err(HarnessError::OracleUnverifiable {
            path: bin.to_path_buf(),
            cause: format!("`--version` produced unparsable output: {reported:?}"),
        });
    }
    if reported != pin.version {
        return Err(HarnessError::OracleVersionMismatch {
            found: reported,
            required: pin.version.clone(),
            path: bin.to_path_buf(),
        });
    }
    Ok(VerifiedOracle {
        path: bin.to_path_buf(),
        source,
        version: reported,
    })
}

/// Run `<bin> --version` under `bound`, returning the trimmed first non-empty
/// stdout line. Timeout / non-zero exit / spawn failure / empty output all map to
/// [`HarnessError::OracleUnverifiable`].
async fn query_version(bin: &Path, bound: Duration) -> Result<String, HarnessError> {
    let mut cmd = tokio::process::Command::new(bin);
    cmd.arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let output = match tokio::time::timeout(bound, cmd.output()).await {
        Err(_elapsed) => {
            return Err(HarnessError::OracleUnverifiable {
                path: bin.to_path_buf(),
                cause: format!("`--version` timed out after {bound:?}"),
            });
        }
        Ok(Err(e)) => {
            return Err(HarnessError::OracleUnverifiable {
                path: bin.to_path_buf(),
                cause: format!("could not spawn `--version`: {e}"),
            });
        }
        Ok(Ok(out)) => out,
    };

    if !output.status.success() {
        return Err(HarnessError::OracleUnverifiable {
            path: bin.to_path_buf(),
            cause: format!(
                "`--version` exited unsuccessfully ({}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    match stdout.lines().map(str::trim).find(|l| !l.is_empty()) {
        Some(line) => Ok(line.to_string()),
        None => Err(HarnessError::OracleUnverifiable {
            path: bin.to_path_buf(),
            cause: "`--version` produced no output".to_string(),
        }),
    }
}

/// A lightweight semver-shape gate distinguishing a real (possibly wrong) version
/// from garbage output. Avoids a regex dependency: must start with a digit, carry
/// at least two dots, and contain only version-legal characters.
fn looks_like_version(s: &str) -> bool {
    let mut dots = 0usize;
    let mut first = true;
    for c in s.chars() {
        if first {
            if !c.is_ascii_digit() {
                return false;
            }
            first = false;
        }
        if c == '.' {
            dots += 1;
        }
        if !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+')) {
            return false;
        }
    }
    !first && dots >= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_pin_parses_and_matches_expected() {
        let pin = OraclePin::load().expect("embedded pin must parse");
        assert_eq!(pin.package, ORACLE_PACKAGE);
        assert_eq!(pin.version, "0.87.0");
    }

    #[test]
    fn malformed_pin_is_rejected() {
        assert!(matches!(
            OraclePin::parse("{ not json"),
            Err(HarnessError::OracleUnverifiable { .. })
        ));
        // Unknown field rejected (deny_unknown_fields).
        assert!(matches!(
            OraclePin::parse(r#"{"package":"@devcontainers/cli","version":"0.87.0","x":1}"#),
            Err(HarnessError::OracleUnverifiable { .. })
        ));
        // Wrong package rejected.
        assert!(matches!(
            OraclePin::parse(r#"{"package":"other","version":"0.87.0"}"#),
            Err(HarnessError::OracleUnverifiable { .. })
        ));
    }

    #[test]
    fn version_shape_gate() {
        assert!(looks_like_version("0.87.0"));
        assert!(looks_like_version("1.2.3-rc.1"));
        assert!(!looks_like_version("not a version"));
        assert!(!looks_like_version("1.2")); // too few dots
        assert!(!looks_like_version(""));
        assert!(!looks_like_version("v1.2.3")); // must start with a digit
    }

    #[cfg(unix)]
    fn write_stub(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join(name);
        std::fs::write(&p, body).expect("write stub");
        let mut perms = std::fs::metadata(&p).expect("stat stub").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&p, perms).expect("chmod stub");
        p
    }

    #[test]
    fn override_beats_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let override_bin = dir.path().join("my-devcontainer");
        std::fs::write(&override_bin, "#!/bin/sh\n").expect("write override");
        let path_dir = dir.path().join("pathdir");
        std::fs::create_dir_all(&path_dir).expect("mkdir");
        std::fs::write(path_dir.join("devcontainer"), "#!/bin/sh\n").expect("write path bin");
        let path_env = std::ffi::OsString::from(path_dir.to_string_lossy().to_string());

        let (bin, source) =
            resolve_binary(Some(override_bin.clone()), Some(&path_env)).expect("resolve");
        assert_eq!(bin, override_bin);
        assert_eq!(source, OracleSource::Override);

        let (bin, source) = resolve_binary(None, Some(&path_env)).expect("resolve path");
        assert_eq!(bin, path_dir.join("devcontainer"));
        assert_eq!(source, OracleSource::PathLookup);
    }

    #[test]
    fn missing_override_is_named() {
        let err = resolve_binary(Some(PathBuf::from("/nonexistent/devcontainer")), None)
            .expect_err("must fail");
        assert!(matches!(err, HarnessError::OracleMissing { .. }));
        assert!(err.to_string().contains("/nonexistent/devcontainer"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn exact_version_match_and_mismatch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pin = OraclePin {
            package: ORACLE_PACKAGE.to_string(),
            version: "0.87.0".to_string(),
        };

        let good = write_stub(dir.path(), "good", "#!/bin/sh\necho 0.87.0\n");
        let v = verify(&good, OracleSource::Override, &pin, VERSION_QUERY_BOUND)
            .await
            .expect("matching version verifies");
        assert_eq!(v.version, "0.87.0");
        assert_eq!(v.source, OracleSource::Override);

        let bad = write_stub(dir.path(), "bad", "#!/bin/sh\necho 0.86.0\n");
        let err = verify(&bad, OracleSource::Override, &pin, VERSION_QUERY_BOUND)
            .await
            .expect_err("wrong version fails");
        match err {
            HarnessError::OracleVersionMismatch {
                found, required, ..
            } => {
                assert_eq!(found, "0.86.0");
                assert_eq!(required, "0.87.0");
            }
            other => panic!("expected version mismatch, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn garbage_version_is_unverifiable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pin = OraclePin {
            package: ORACLE_PACKAGE.to_string(),
            version: "0.87.0".to_string(),
        };
        let junk = write_stub(dir.path(), "junk", "#!/bin/sh\necho not a version\n");
        let err = verify(&junk, OracleSource::Override, &pin, VERSION_QUERY_BOUND)
            .await
            .expect_err("garbage fails");
        assert!(matches!(err, HarnessError::OracleUnverifiable { .. }));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn version_query_timeout_is_unverifiable_with_injected_bound() {
        let dir = tempfile::tempdir().expect("tempdir");
        let slow = write_stub(dir.path(), "slow", "#!/bin/sh\nsleep 5\necho 0.87.0\n");
        let err = query_version(&slow, Duration::from_millis(150))
            .await
            .expect_err("must time out");
        match err {
            HarnessError::OracleUnverifiable { cause, .. } => {
                assert!(cause.contains("timed out"), "cause was {cause}");
            }
            other => panic!("expected unverifiable timeout, got {other:?}"),
        }
    }
}
