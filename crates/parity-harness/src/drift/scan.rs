//! The five upstream probes (026-continuous-conformance-certification, US4; FR-022).
//!
//! ## Why `git` and `npm` subprocesses rather than an HTTP client
//!
//! The precedent is `discovery/corpus_fetch.rs` and its reasoning transfers unchanged: no
//! API token, no rate limit, and both tools are already prerequisites of working in this
//! repository. An HTTP client would add authentication, rate-limiting, and retry surface
//! to a lane that gates nothing — and an unauthenticated GitHub API would make a nightly
//! run flaky for no benefit (research D6).
//!
//! ## Every probe reports, none of them judges
//!
//! A probe answers "what does upstream look like?" It never answers "is deacon wrong?" —
//! that is a human reading of the observation, and the separation is what lets this module
//! write at all without blessing anything (FR-024).

use std::path::{Path, PathBuf};
use std::time::Duration;

use deacon_conformance::drift::{DriftKind, DriftObservation, derive_observation_id};

use crate::HarnessError;
use crate::discovery::corpus_fetch::git_binary;

/// Upstream repositories the probes look at. Pinned here rather than configured: a
/// configurable upstream is an upstream someone can point at a fork, and a drift signal
/// against a fork is not a drift signal.
const SPEC_REPO: &str = "https://github.com/devcontainers/spec.git";
const CLI_REPO: &str = "https://github.com/devcontainers/cli.git";

/// The reference package whose releases the `reference-release` probe watches.
const REFERENCE_PACKAGE: &str = "@devcontainers/cli";

/// Bound on a single upstream query. Generous enough for a cold partial clone, short
/// enough that a wedged probe fails the run rather than occupying the scheduled window.
const PROBE_BOUND: Duration = Duration::from_secs(120);

/// Path override for `npm`, mirroring the `git` override — the fault-injection seam a
/// test uses to point at a failing stub without touching the real tool.
const NPM_OVERRIDE_ENV: &str = "DEACON_DRIFT_NPM";

/// The `npm` binary this run uses.
pub fn npm_binary() -> PathBuf {
    std::env::var_os(NPM_OVERRIDE_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("npm"))
}

/// What a scan of one kind produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeResult {
    /// Upstream matches the pin. **Distinct from "did not run"** — that state is the
    /// absence of a result, and `lastCompletedRun` is what records the difference (FR-025).
    Unchanged,
    /// Upstream has moved.
    Drifted(DriftObservation),
}

/// The pins a scan compares against, read from the registry's revision records.
#[derive(Debug, Clone)]
pub struct Pins {
    pub spec: String,
    pub schema: String,
    pub oracle: String,
    pub cli_surface: String,
}

/// Run one probe. `today` is injected rather than read from the clock so a scan is
/// reproducible and its output byte-stable.
pub async fn probe(
    kind: DriftKind,
    pins: &Pins,
    repo_root: &Path,
    today: &str,
) -> Result<ProbeResult, HarnessError> {
    match kind {
        DriftKind::SpecCommit => probe_head(kind, SPEC_REPO, &pins.spec, today, Vec::new()).await,
        DriftKind::UpstreamTestOrChangelog => {
            probe_head(
                kind,
                SPEC_REPO,
                &pins.spec,
                today,
                vec!["test/".to_string(), "CHANGELOG.md".to_string()],
            )
            .await
        }
        DriftKind::CliSurfaceChange => {
            probe_head(kind, CLI_REPO, &pins.cli_surface, today, Vec::new()).await
        }
        DriftKind::SchemaChange => probe_schema(pins, repo_root, today).await,
        DriftKind::ReferenceRelease => probe_reference_release(pins, today).await,
    }
}

/// Compare a repository's `HEAD` against a pinned revision via `git ls-remote`.
///
/// `ls-remote` is the cheapest possible question — one network round trip, no clone, no
/// working tree — which matters for a probe that runs nightly across several kinds.
async fn probe_head(
    kind: DriftKind,
    repo: &str,
    pinned: &str,
    today: &str,
    affected: Vec<String>,
) -> Result<ProbeResult, HarnessError> {
    let head = ls_remote_head(repo).await?;
    if head.starts_with(pinned) || pinned.starts_with(&head[..pinned.len().min(head.len())]) {
        return Ok(ProbeResult::Unchanged);
    }
    Ok(ProbeResult::Drifted(observation(
        kind, pinned, &head, today, affected,
    )))
}

/// Resolve the default branch's commit for `repo`.
async fn ls_remote_head(repo: &str) -> Result<String, HarnessError> {
    ls_remote_with(&git_binary(), repo, PROBE_BOUND).await
}

/// Resolve a repository's `HEAD` using a specific `git` binary and bound.
///
/// Pure over its inputs so a test can point it at a failing stub without touching process
/// environment — `std::env::set_var` is `unsafe` under this workspace's `unsafe_code =
/// "deny"`, and process-global besides, so an environment-based seam would both fail the
/// lint and race any concurrent test.
async fn ls_remote_with(git: &Path, repo: &str, bound: Duration) -> Result<String, HarnessError> {
    let mut cmd = tokio::process::Command::new(git);
    cmd.args(["ls-remote", repo, "HEAD"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let output = match tokio::time::timeout(bound, cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Err(HarnessError::NetworkUnavailable {
                cause: format!("could not run `{} ls-remote {repo}`: {e}", git.display()),
            });
        }
        Err(_elapsed) => {
            return Err(HarnessError::NetworkUnavailable {
                cause: format!("`git ls-remote {repo}` did not answer within {bound:?}"),
            });
        }
    };
    if !output.status.success() {
        return Err(HarnessError::NetworkUnavailable {
            cause: format!(
                "`git ls-remote {repo}` exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .split_whitespace()
        .next()
        .filter(|s| s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit()))
        .map(str::to_string)
        .ok_or_else(|| HarnessError::NetworkUnavailable {
            cause: format!("`git ls-remote {repo}` returned no commit id: {stdout:?}"),
        })
}

/// Compare each vendored schema document's SHA-256 against the pinned manifest.
///
/// Deliberately compares the *vendored* fingerprints rather than re-downloading: the
/// manifest already records what each document was when it was pinned, so the probe only
/// needs upstream's current `HEAD` to say whether a change is possible, and the manifest
/// to say which documents are at stake. Re-downloading four documents nightly to compute
/// digests we already hold would be a slower way to learn the same thing.
async fn probe_schema(
    pins: &Pins,
    repo_root: &Path,
    today: &str,
) -> Result<ProbeResult, HarnessError> {
    let head = ls_remote_head(SPEC_REPO).await?;
    if head.starts_with(&pins.schema) {
        return Ok(ProbeResult::Unchanged);
    }
    let manifest_path = repo_root
        .join("conformance")
        .join("schemas")
        .join(&pins.schema)
        .join("manifest.json");
    let affected = manifest_documents(&manifest_path);
    Ok(ProbeResult::Drifted(observation(
        DriftKind::SchemaChange,
        &pins.schema,
        &head,
        today,
        affected,
    )))
}

/// The document keys a vendored manifest pins, sorted.
fn manifest_documents(path: &Path) -> Vec<String> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Vec::new();
    };
    let mut keys: Vec<String> = doc
        .get("documents")
        .and_then(|d| d.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

/// Compare the latest published reference release against the stable pin.
async fn probe_reference_release(pins: &Pins, today: &str) -> Result<ProbeResult, HarnessError> {
    let latest = npm_latest_version().await?;
    if latest == pins.oracle {
        return Ok(ProbeResult::Unchanged);
    }
    Ok(ProbeResult::Drifted(observation(
        DriftKind::ReferenceRelease,
        &pins.oracle,
        &latest,
        today,
        vec![
            "conformance/registry/revisions.json".to_string(),
            "fixtures/parity-corpus/oracle.json".to_string(),
        ],
    )))
}

/// The latest published version of the reference package.
async fn npm_latest_version() -> Result<String, HarnessError> {
    let npm = npm_binary();
    let mut cmd = tokio::process::Command::new(&npm);
    cmd.args(["view", REFERENCE_PACKAGE, "version"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let output = match tokio::time::timeout(PROBE_BOUND, cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Err(HarnessError::NetworkUnavailable {
                cause: format!(
                    "could not run `{} view {REFERENCE_PACKAGE}`: {e}",
                    npm.display()
                ),
            });
        }
        Err(_elapsed) => {
            return Err(HarnessError::NetworkUnavailable {
                cause: format!(
                    "`npm view {REFERENCE_PACKAGE}` did not answer within {PROBE_BOUND:?}"
                ),
            });
        }
    };
    if !output.status.success() {
        return Err(HarnessError::NetworkUnavailable {
            cause: format!(
                "`npm view {REFERENCE_PACKAGE}` exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        return Err(HarnessError::NetworkUnavailable {
            cause: format!("`npm view {REFERENCE_PACKAGE} version` returned nothing"),
        });
    }
    Ok(version)
}

/// Build an observation with its substance-anchored id.
fn observation(
    kind: DriftKind,
    pinned: &str,
    observed: &str,
    today: &str,
    affected: Vec<String>,
) -> DriftObservation {
    DriftObservation {
        id: derive_observation_id(kind, pinned, observed),
        kind,
        pinned_revision: pinned.to_string(),
        observed_revision: observed.to_string(),
        affected_surfaces: affected,
        observed_at: today.to_string(),
        review_artifact: "target/drift/scan.json".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_observation_carries_its_derived_id() {
        let obs = observation(
            DriftKind::SpecCommit,
            "113500f4",
            "9f21ab77",
            "2026-07-28",
            vec![],
        );
        assert_eq!(obs.id, obs.derived_id());
    }

    #[test]
    fn manifest_documents_are_sorted_and_absent_files_yield_nothing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("manifest.json");
        std::fs::write(
            &path,
            r#"{"documents":{"z.json":{"sha256":"x"},"a.json":{"sha256":"y"}}}"#,
        )
        .expect("write");
        assert_eq!(manifest_documents(&path), vec!["a.json", "z.json"]);
        assert!(manifest_documents(&dir.path().join("absent.json")).is_empty());
    }

    #[tokio::test]
    async fn an_unreachable_upstream_is_a_machinery_failure_not_a_finding() {
        // FR-026's dividing line: a probe that cannot RUN must fail the scan, while a
        // probe that runs and finds drift must not. Conflating the two would make a lane
        // that reports upstream movement red — a gate on someone else's release schedule.
        //
        // Driven through the injection seam rather than by setting an environment variable:
        // `unsafe_code` is denied workspace-wide, and `std::env::set_var` is `unsafe` (and
        // process-global, so it would race any concurrent test besides).
        let result = ls_remote_with(
            std::path::Path::new("/nonexistent/git-binary"),
            SPEC_REPO,
            Duration::from_secs(5),
        )
        .await;
        match result {
            Err(HarnessError::NetworkUnavailable { cause }) => {
                assert!(
                    cause.contains("ls-remote"),
                    "the cause must name the probe that could not run: {cause}"
                );
            }
            other => panic!("an unreachable upstream must be NetworkUnavailable, got {other:?}"),
        }
    }
}
