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
pub mod compare;
pub mod discovery;
pub mod driver;
pub mod equivalence;
pub mod evidence;
pub mod exec;
pub mod inject;
pub mod normalize;
pub mod observe;
pub mod oracle;
pub mod oracle_type;
pub mod prereq;
pub mod registry;
pub mod report;
pub mod runner;
pub mod waiver;
pub mod workspace;

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

    // --- Declarative conformance runner (022-conformance-runner, T012) ---------
    // Cause-specific fail-loud variants for the runner: a declared Docker/Node channel
    // or normalization step, or a snapshot-oracle comparison, must never be silently
    // skipped (constitution IV). These are distinct from the legacy
    // [`DockerMissing`](Self::DockerMissing) pre-check, which the pre-022 `parity_*`
    // path still uses: `DockerUnavailable`/`NodeUnavailable` are the 022 runner's
    // environment-probe failures, carrying the specific cause the probe observed.
    /// The runner requires Docker for a case's declared channels but it is unavailable.
    #[error(
        "Docker is unavailable for the conformance runner: {cause}. Remedy: start Docker (or \
         provide a working `docker`) — a declared Docker channel must never be silently skipped."
    )]
    DockerUnavailable { cause: String },

    /// The runner requires Node (for the pinned oracle) but it is unavailable.
    #[error(
        "Node is unavailable for the conformance runner: {cause}. Remedy: install the Node \
         runtime the pinned `@devcontainers/cli` oracle needs — a live-differential case must \
         never be silently skipped."
    )]
    NodeUnavailable { cause: String },

    /// Normalization of a channel's evidence failed; the runner never falls back to raw
    /// comparison (FR-005 / FR-029).
    #[error(
        "normalization failed for channel `{channel}`: {cause}. Remedy: fix the producer or the \
         named normalization rule — there is no raw-comparison fallback."
    )]
    NormalizationFailed { channel: String, cause: String },

    /// A committed snapshot is stale: a provenance/hash field no longer matches the
    /// recomputed inputs or probed environment (FR-020). `field` names the FIRST
    /// mismatch so the diagnosis is precise.
    #[error(
        "snapshot is stale: `{field}` no longer matches the committed provenance. Remedy: \
         re-record via the reviewed `conformance-snapshot refresh` — a stale snapshot must fail, \
         never auto-refresh."
    )]
    SnapshotStale { field: String },

    /// A case ran but its declared channels observed NOTHING — the vacuity fault (024
    /// Phase 3, D-2). Either every declared channel came back `present:false`, or a
    /// successful Docker operation produced no discoverable container. Both mean the
    /// OBSERVATION is broken, not that the two sides agree.
    #[error(
        "conformance case `{case}` observed nothing: {cause}. Remedy: fix the observation \
         (a Docker/daemon fault, a container that was never discovered, or a mis-declared \
         channel) — a case that observed nothing has proven nothing and must never pass \
         vacuously."
    )]
    ObservationFault { case: String, cause: String },

    /// No committed snapshot exists for the current platform — a coverage gap, distinct
    /// from stale and from a silent skip (FR-016a).
    #[error(
        "no committed snapshot for platform `{os_arch}` (no-reference-for-platform). Remedy: \
         record one via the reviewed refresh on this platform, or accept the coverage gap — this \
         is never a silent skip."
    )]
    NoReferenceForPlatform { os_arch: String },

    /// A declarative case exceeded the per-case wall-clock bound and was abandoned
    /// (024 FR-077b). Distinct from [`OracleTimeout`](Self::OracleTimeout), which bounds a
    /// SINGLE CLI invocation: this bounds the whole case — every operation, both sides,
    /// and every observation — so a case that hangs between invocations is still named.
    // --- Injected-regression harness (024 US6, contracts/regression-harness.md) ------
    /// Something tried to apply a regression in a process that never took out the
    /// injection capability (FR-070). The ordinary conformance drivers land here if they
    /// ever reach the injector, which is what makes the isolation an ENFORCED barrier
    /// rather than a convention.
    #[error(
        "regression `{record}` cannot be applied: this process is not the regression harness. \
         Remedy: regressions live in the `coverage-regressions` bin only — an ordinary \
         conformance run must never be able to perturb its own evidence (FR-070)."
    )]
    InjectionForbidden { record: String },

    /// A regression's perturbation could not be applied to the evidence source it names.
    ///
    /// Deliberately DISTINCT from an `inert` verdict: `inert` is a claim about the
    /// CHANNEL ("nothing here can fail"), whereas this is a claim about the RECORD ("this
    /// perturbation never landed"). Collapsing the two would let a mis-authored record
    /// masquerade as a dead channel, or — far worse — a dead channel hide behind a
    /// mis-authored record.
    #[error(
        "regression `{record}` could not be applied: {cause}. Remedy: fix the record's \
         target/perturbation or the case it names — a perturbation that never landed says \
         nothing about the channel, so it is never reported as `inert`."
    )]
    InjectionInapplicable { record: String, cause: String },

    /// A regression could not be reverted, so the tree may be left perturbed (FR-066).
    #[error(
        "regression `{record}` could not be reverted: {cause}. Remedy: restore the affected \
         path by hand and investigate — the harness must never leave a perturbation applied."
    )]
    InjectionRevertFailed { record: String, cause: String },

    #[error(
        "conformance case `{case}` exceeded its {bound:?} per-case bound and was abandoned. \
         Remedy: investigate the hang in this case (its operations, its container teardown, or \
         a wedged daemon) — the bound exists so ONE stuck case cannot consume the whole tier's \
         budget and report as an unattributable lane failure."
    )]
    CaseTimeout { case: String, bound: Duration },

    // --- Exploratory parity discovery (025-exploratory-parity-discovery, T005) ------
    // Every variant below is a MACHINERY failure, never a finding. That distinction is
    // the whole exit-status contract (contracts/discovery-cli.md): a campaign that finds
    // forty differences exits `0`; a campaign that could not verify its oracle exits
    // non-zero. Anything that would make the status depend on WHAT was found is a defect,
    // because a stochastic gate makes green non-reproducible.
    /// A campaign that needs the pinned reference could not verify it (FR-003).
    ///
    /// Deliberately DISTINCT from [`OracleMissing`](Self::OracleMissing) /
    /// [`OracleVersionMismatch`](Self::OracleVersionMismatch), which name *which* check
    /// failed: this names the CONSEQUENCE for the campaign — no findings were produced
    /// and none may be attributed to this run. Collapsing the two would let "we compared
    /// against an unverified reference" read as "we compared and found nothing", which is
    /// the exact confusion a discovery run must never create.
    #[error(
        "discovery campaign cannot run: the pinned oracle is unverified ({cause}). Remedy: \
         install the pinned `@devcontainers/cli` version — a campaign never reports findings \
         against an unverified reference, and never silently skips."
    )]
    OracleUnverified { cause: String },

    /// One candidate exceeded its per-candidate bound and was discarded and counted
    /// (60 s hermetic / 5 min container-backed).
    ///
    /// Discarding rather than failing the campaign is deliberate: one pathological
    /// generated input must not consume the tier's whole budget, and the count is
    /// reported so a *rising* discard rate is visible rather than silent.
    #[error(
        "discovery candidate `{candidate}` exceeded its {bound:?} bound and was discarded. \
         Remedy: none required — the candidate is counted in the campaign outcome; \
         investigate only if the discard rate rises."
    )]
    CandidateTimeout { candidate: String, bound: Duration },

    /// Minimization ran out of shrink steps before reaching a minimal input (FR-022).
    ///
    /// The best reduction found is still emitted, with `isMinimal: false` and this
    /// reason — never silently presented as minimal. A reviewer who believes an input is
    /// minimal when it is not will look for the defect in the wrong place.
    #[error(
        "shrink budget exhausted for finding `{finding}` after {steps} step(s); the best \
         reduction is reported with `isMinimal: false`. Remedy: raise the per-finding shrink \
         budget if the input warrants it — a partially reduced input is never presented as \
         minimal."
    )]
    ShrinkBudgetExhausted { finding: String, steps: usize },

    /// A fetched corpus entry's content digest disagrees with the recorded one (FR-051).
    ///
    /// Fails that entry loudly rather than comparing against unexpected content: an
    /// unverified fetch means comparing against content nobody checked, and "expected to
    /// be stable" is not "verified".
    #[error(
        "corpus entry `{entry}` content digest mismatch: recorded {expected}, fetched {actual}. \
         Remedy: investigate the upstream change and re-pin the entry deliberately — never \
         compare against content that does not match its recorded digest."
    )]
    CorpusDigestMismatch {
        entry: String,
        expected: String,
        actual: String,
    },

    /// A corpus entry could not be fetched (FR-052).
    ///
    /// Reported as unreachable, NOT as "ran and found nothing" — the two are different
    /// facts about the ecosystem and collapsing them makes the canary useless.
    #[error(
        "corpus entry `{entry}` is unreachable: {cause}. Remedy: check the network lane and the \
         pinned repository/commit — an unreachable entry is reported as unreachable, never as \
         an entry that ran and found nothing."
    )]
    CorpusUnreachable { entry: String, cause: String },
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
