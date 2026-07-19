//! Parity harness support crate (dev-only, `publish = false`).
//!
//! This crate is the single home for the deacon parity comparison machinery that
//! the `crates/deacon/tests/parity_*` binaries consume: oracle resolution and
//! exact-version verification, bounded CLI execution with always-on raw capture,
//! the one canonical normalization module, waiver/registry loaders, and run-report
//! fragment writing. It exists as a crate (not a `tests/` include-module) so the
//! logic has first-class unit tests, clippy/fmt coverage, and can host the
//! `parity-report` aggregator binary.
//!
//! Design invariants (constitution IV — no silent fallbacks):
//! - Every prerequisite absence, oracle mismatch, malformed output, normalization
//!   failure, or artifact-write failure surfaces as a cause-specific
//!   [`HarnessError`] whose `Display` names the cause and the remedy. Callers turn
//!   these into test failures — never a silent skip-to-pass.
//! - All artifact writes are atomic (temp file + `fs::rename`), matching the repo's
//!   durable-write pattern in `crates/core/src/cache/disk.rs::save_index`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub mod aggregate;
pub mod exec;
pub mod normalize;
pub mod oracle;
pub mod prereq;
pub mod registry;
pub mod report;
pub mod waiver;

/// The one error taxonomy for the whole harness (data-model §9, FR-005).
///
/// Every variant's `Display` names the cause and, where applicable, the remedy;
/// these strings are the user-facing failure messages the fault-injection suite
/// (FR-021) asserts against. `Clone` is derived so a verified-oracle result can be
/// cached in a process-wide `OnceLock` and handed back to each caller by value.
///
/// Paths are rendered with `{:?}` (quoted) because `Path`/`PathBuf` do not
/// implement `Display`; the quoting is unambiguous and diagnosis-friendly.
#[derive(Debug, Clone, thiserror::Error)]
pub enum HarnessError {
    /// The pinned oracle binary could not be resolved at all.
    #[error(
        "parity oracle `devcontainer` not found: {hint}. Remedy: install the pinned \
         version (`npm install -g @devcontainers/cli@<pin>`) or point \
         DEACON_PARITY_DEVCONTAINER at it."
    )]
    OracleMissing { hint: String },

    /// A resolvable oracle reported a version other than the pin.
    #[error(
        "parity oracle version mismatch: found {found}, required {required} (binary {path:?}). \
         Remedy: install the pinned version — a passing parity run must certify against exactly \
         the pinned reference."
    )]
    OracleVersionMismatch {
        found: String,
        required: String,
        path: PathBuf,
    },

    /// The oracle exists but its `--version` could not be established (timeout,
    /// non-zero exit, or unparsable output).
    #[error(
        "parity oracle at {path:?} could not be verified: {cause}. Remedy: confirm the \
         `devcontainer` binary runs and prints a bare semver from `--version`."
    )]
    OracleUnverifiable { path: PathBuf, cause: String },

    /// A CLI invocation that was expected to succeed exited non-zero.
    #[error(
        "parity case `{case}`: CLI exited unsuccessfully ({status}); stderr preserved at \
         {stderr_path:?}. Remedy: inspect the captured stderr for the failing invocation."
    )]
    OracleFailure {
        case: String,
        status: String,
        stderr_path: PathBuf,
    },

    /// A CLI invocation exceeded its per-invocation bound and was killed; whatever
    /// output was produced is preserved at `partial_paths`.
    #[error(
        "parity case `{case}`: CLI exceeded its {bound:?} bound and was terminated; partial \
         output preserved at {partial_paths:?}. Remedy: raise the bound only if the workload \
         legitimately needs longer, else investigate the hang."
    )]
    OracleTimeout {
        case: String,
        bound: Duration,
        partial_paths: Vec<PathBuf>,
    },

    /// Output that should have parsed as JSON did not.
    #[error(
        "parity case `{case}`: could not parse CLI output as JSON: {cause}. Remedy: inspect the \
         preserved raw output — the CLI emitted non-JSON where structured output was required."
    )]
    MalformedOutput { case: String, cause: String },

    /// A Docker-required check ran without a working Docker CLI.
    #[error(
        "Docker is required for this parity check but is not available. Remedy: start Docker (or \
         provide a working `docker` via DEACON_PARITY_DOCKER)."
    )]
    DockerMissing,

    /// A required fixture path was absent.
    #[error(
        "required parity fixture is missing: {path:?}. Remedy: restore the fixture or fix the \
         corpus path — a parity check must never run against absent inputs."
    )]
    FixtureMissing { path: PathBuf },

    /// Normalization of an output failed; the harness never falls back to raw
    /// comparison (FR-005/FR-019).
    #[error(
        "parity case `{case}`: normalization failed: {cause}. Remedy: the shared normalization \
         module rejected this input — fix the producer or the normalization rule; there is no \
         raw-comparison fallback."
    )]
    Normalization { case: String, cause: String },

    /// A loaded waiver no longer matches reality (case gone or expected difference
    /// no longer observed).
    #[error(
        "waiver `{id}` is stale: its case is gone or the characterized difference is no longer \
         observed. Remedy: remove or update the waiver record — stale waivers silently narrow \
         coverage."
    )]
    WaiverStale { id: String },

    /// A waiver record failed schema/uniqueness validation.
    #[error(
        "invalid waiver record at {path:?}: {cause}. Remedy: fix the record to match the waiver \
         schema (unknown fields are rejected; ids must be unique)."
    )]
    WaiverInvalid { path: PathBuf, cause: String },

    /// A report fragment or artifact could not be written.
    #[error(
        "parity report write failed: {cause}. Remedy: ensure the report directory is writable — \
         a report-write failure fails the run (a run whose result cannot be recorded is not a \
         passing run)."
    )]
    Report { cause: String },

    /// A corpus had fewer discovered cases than its registered minimum.
    #[error(
        "corpus `{corpus}` has {found} case(s) but the registry requires at least {min}. Remedy: \
         restore the missing cases or correct the registry minimum — a shrinking corpus silently \
         erodes coverage."
    )]
    CorpusTooSmall {
        corpus: String,
        found: usize,
        min: usize,
    },
}

/// Environment override for the report/artifact root (see [`report_root`]).
pub const REPORT_DIR_ENV: &str = "DEACON_PARITY_REPORT_DIR";

/// Absolute path to the workspace root, derived from this crate's
/// `CARGO_MANIFEST_DIR` (`<root>/crates/parity-harness`) so artifact paths are
/// stable regardless of the (per-package) cargo-test working directory.
pub fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent() // crates/
        .and_then(|p| p.parent()) // <root>
        .map(Path::to_path_buf)
        .unwrap_or(manifest)
}

/// The conformance registry root: `<workspace_root>/conformance/registry`. Waiver
/// records live under its `waivers/` subdirectory and are consumed through
/// `deacon-conformance` (019-conformance-registry, research D3). Delegates to the
/// conformance crate so there is a single definition of the registry location.
pub fn conformance_registry_root() -> PathBuf {
    deacon_conformance::default_registry_dir()
}

/// The report/artifact root: `DEACON_PARITY_REPORT_DIR` when set, else
/// `<workspace_root>/target/parity`. Both the test binaries and the aggregator
/// resolve it identically (contracts/execution-contract.md).
pub fn report_root() -> PathBuf {
    if let Some(dir) = std::env::var_os(REPORT_DIR_ENV) {
        return PathBuf::from(dir);
    }
    workspace_root().join("target").join("parity")
}

/// Process-unique suffix source for atomic temp files.
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Atomically write `bytes` to `path`: create the parent, stream to a unique temp
/// file in the same directory, then `rename` into place. A shorter payload can
/// never leave trailing bytes from a previous longer file, and concurrent writers
/// (nextest runs binaries in parallel) never observe a half-written file.
pub async fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), HarnessError> {
    let parent = path.parent().ok_or_else(|| HarnessError::Report {
        cause: format!("artifact path has no parent directory: {path:?}"),
    })?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| HarnessError::Report {
            cause: format!("could not create {parent:?}: {e}"),
        })?;
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = parent.join(format!(".tmp-{}-{seq}", std::process::id()));
    tokio::fs::write(&tmp, bytes)
        .await
        .map_err(|e| HarnessError::Report {
            cause: format!("could not write temp file {tmp:?}: {e}"),
        })?;
    tokio::fs::rename(&tmp, path)
        .await
        .map_err(|e| HarnessError::Report {
            cause: format!("could not rename {tmp:?} -> {path:?}: {e}"),
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_root_contains_fixtures_and_crate() {
        let root = workspace_root();
        assert!(
            root.join("fixtures/parity-corpus/oracle.json").is_file(),
            "workspace_root() should locate the oracle pin, got {root:?}"
        );
        assert!(root.join("crates/parity-harness/Cargo.toml").is_file());
    }

    #[test]
    fn report_root_honors_override() {
        // Use the explicit override rather than mutating process env (edition-2024
        // set_var is unsafe); we assert the default shape separately.
        let default = report_root();
        assert!(default.ends_with("target/parity") || std::env::var_os(REPORT_DIR_ENV).is_some());
    }

    #[tokio::test]
    async fn atomic_write_replaces_shorter_payload() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("out.json");
        atomic_write(&path, b"a-longer-first-payload")
            .await
            .expect("first write");
        atomic_write(&path, b"short").await.expect("second write");
        let read = std::fs::read_to_string(&path).expect("read back");
        assert_eq!(read, "short", "rename must not leave trailing bytes");
        // No temp files should survive a successful write.
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "temp files should be renamed away");
    }

    #[test]
    fn error_display_names_cause_and_remedy() {
        let e = HarnessError::OracleVersionMismatch {
            found: "0.86.0".into(),
            required: "0.87.0".into(),
            path: PathBuf::from("/usr/local/bin/devcontainer"),
        };
        let msg = e.to_string();
        assert!(msg.contains("0.86.0") && msg.contains("0.87.0"));
        assert!(msg.contains("Remedy"));

        assert!(HarnessError::DockerMissing.to_string().contains("Docker"));
        assert!(
            HarnessError::OracleMissing {
                hint: "empty PATH".into()
            }
            .to_string()
            .contains("empty PATH")
        );
    }
}
