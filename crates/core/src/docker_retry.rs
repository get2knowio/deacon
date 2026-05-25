//! Retry classification for `docker` subprocess operations.
//!
//! This module is the docker-subprocess parallel of [`crate::oci::utils::classify_network_error`].
//! Where the OCI fetcher classifies typed `FeatureError` variants, docker
//! subprocess failures arrive only as a non-zero exit code plus a blob of
//! human-readable stderr. We pattern-match that stderr against a curated list
//! of canonical BuildKit / Docker CLI / registry error fragments to decide
//! whether a failure is transient (retryable) or terminal (not retryable).
//!
//! ## Why pattern-match stderr?
//!
//! BuildKit and the Docker CLI do not return structured error codes for the
//! classes of failure we care about (TLS timeouts, 429 rate limits, registry
//! auth, Dockerfile syntax errors). They print canonical English messages and
//! exit non-zero. The OCI distribution spec and the Docker CLI codebase use
//! stable phrases like `"too many requests"`, `"unauthorized"`, and
//! `"denied: requested access to the resource is denied"`, so substring
//! matching is reliable enough for retry decisions while remaining cheap.
//!
//! Cases we deliberately do **not** retry:
//! * `BuildFailed` — Dockerfile syntax errors, failing `RUN` commands, missing
//!   files referenced by `COPY`/`ADD`. These will fail the same way on every
//!   retry, so we fail fast (no silent fallback per the constitution).
//! * `RegistryAuth` — 401/403 from the registry. Bad credentials will not be
//!   fixed by waiting.
//! * `Other` — unknown failures. We prefer to surface them immediately rather
//!   than mask a real bug behind 3 retries.

use std::path::Path;
use std::process::Output;

use thiserror::Error;

use crate::errors::{DockerError, Result};
use crate::retry::{retry_async, RetryConfig, RetryDecision};

/// Classification of a failed `docker build` / `docker buildx build` invocation.
///
/// Variants carry the raw `stderr` and `exit_code` for diagnostics — both for
/// logging and for the upper layer that re-renders the error to the user.
#[derive(Debug, Error)]
pub enum DockerSubprocessError {
    /// Transient network issue mid-build: TLS handshake timeout, i/o timeout,
    /// connection reset, EOF from the registry, etc. Retryable.
    #[error("Transient docker network error (exit {exit_code}): {stderr}")]
    TransientNetwork { stderr: String, exit_code: i32 },

    /// Registry rate-limit response (HTTP 429 / "toomanyrequests"). Retryable
    /// — the spec-correct backoff schedule (1s/2s/4s) gives the registry time
    /// to clear the limit.
    #[error("Docker registry rate limit (exit {exit_code}): {stderr}")]
    RateLimit { stderr: String, exit_code: i32 },

    /// Registry refused the request because the caller is not authenticated /
    /// not authorized for the requested resource (401, 403). Not retryable —
    /// the credentials will not become valid by waiting.
    #[error("Docker registry auth failure (exit {exit_code}): {stderr}")]
    RegistryAuth { stderr: String, exit_code: i32 },

    /// The build itself failed deterministically: Dockerfile syntax error,
    /// a failing `RUN` command, a missing file referenced by `COPY`/`ADD`.
    /// Not retryable.
    #[error("Docker build failed (exit {exit_code}): {stderr}")]
    BuildFailed { stderr: String, exit_code: i32 },

    /// Failure we could not confidently classify. Not retryable to avoid
    /// masking real bugs; users see the raw stderr.
    #[error("Docker subprocess failed (exit {exit_code}): {stderr}")]
    Other { stderr: String, exit_code: i32 },
}

impl DockerSubprocessError {
    /// Borrow the raw stderr for diagnostics.
    pub fn stderr(&self) -> &str {
        match self {
            DockerSubprocessError::TransientNetwork { stderr, .. }
            | DockerSubprocessError::RateLimit { stderr, .. }
            | DockerSubprocessError::RegistryAuth { stderr, .. }
            | DockerSubprocessError::BuildFailed { stderr, .. }
            | DockerSubprocessError::Other { stderr, .. } => stderr,
        }
    }

    /// Exit code reported by the `docker` subprocess.
    pub fn exit_code(&self) -> i32 {
        match self {
            DockerSubprocessError::TransientNetwork { exit_code, .. }
            | DockerSubprocessError::RateLimit { exit_code, .. }
            | DockerSubprocessError::RegistryAuth { exit_code, .. }
            | DockerSubprocessError::BuildFailed { exit_code, .. }
            | DockerSubprocessError::Other { exit_code, .. } => *exit_code,
        }
    }
}

/// Classify a docker subprocess failure into one of [`DockerSubprocessError`]'s
/// variants. The caller pairs this with [`classify_retry_decision`] to drive
/// `retry_async`.
///
/// Ordering matters: rate-limit and auth checks run before the generic build
/// failure check, because BuildKit often reports `"failed to solve"` for both
/// transient and terminal failures, with the real signal embedded deeper in
/// the message.
pub fn classify_docker_error(stderr: &str, exit_code: i32) -> DockerSubprocessError {
    let lower = stderr.to_lowercase();

    // --- Rate limit (Docker Hub and other registries) ---
    // Docker Hub uses "toomanyrequests: You have reached your pull rate limit."
    // Generic 429 also surfaces as "429 Too Many Requests".
    if lower.contains("toomanyrequests")
        || lower.contains("too many requests")
        || lower.contains("rate limit")
        || lower.contains(" 429 ")
        || lower.contains("status code 429")
    {
        return DockerSubprocessError::RateLimit {
            stderr: stderr.to_string(),
            exit_code,
        };
    }

    // --- Registry auth (401 / 403) ---
    // Canonical messages from the Docker CLI / registry:
    //   "unauthorized: authentication required"
    //   "denied: requested access to the resource is denied"
    //   "no basic auth credentials"
    //   "pull access denied for ..."
    if lower.contains("unauthorized")
        || lower.contains("authentication required")
        || lower.contains("denied: requested access")
        || lower.contains("no basic auth credentials")
        || lower.contains("pull access denied")
        || lower.contains(" 401 ")
        || lower.contains(" 403 ")
        || lower.contains("status code 401")
        || lower.contains("status code 403")
    {
        return DockerSubprocessError::RegistryAuth {
            stderr: stderr.to_string(),
            exit_code,
        };
    }

    // --- Transient network errors ---
    // Patterns observed from BuildKit, containerd, and Docker CLI when the
    // registry connection flakes mid-build. Conservative on purpose: only
    // match phrases that strongly imply *network* trouble, not generic IO.
    let transient_markers: &[&str] = &[
        "tls handshake timeout",
        "i/o timeout",
        "io timeout",
        "connection reset",
        "connection refused",
        "connection timed out",
        "connection closed",
        "unexpected eof",
        "eof\n",
        "network is unreachable",
        "no route to host",
        "temporary failure in name resolution",
        "dial tcp", // BuildKit wraps dial errors as "dial tcp <addr>: ..."
        "context deadline exceeded", // BuildKit timeout on registry round-trip
        "tls: ",    // generic TLS error prefix
        "server misbehaving",
        "received unexpected http status: 5", // 5xx from registry — likely transient
        "status code 502",
        "status code 503",
        "status code 504",
        " 502 ",
        " 503 ",
        " 504 ",
    ];

    for marker in transient_markers {
        if lower.contains(marker) {
            return DockerSubprocessError::TransientNetwork {
                stderr: stderr.to_string(),
                exit_code,
            };
        }
    }

    // --- Terminal build failures ---
    // BuildKit and the legacy builder use a stable set of phrases for
    // deterministic Dockerfile / RUN failures:
    //   "dockerfile parse error"
    //   "unknown instruction: FOOBAR"
    //   "the command '/bin/sh -c ...' returned a non-zero code: N"
    //   "executor failed running"
    //   "failed to compute cache key" (missing COPY/ADD source)
    //   "lstat ... no such file or directory" (COPY/ADD source missing)
    let build_failure_markers: &[&str] = &[
        "dockerfile parse error",
        "parse error on line",
        "unknown instruction",
        "returned a non-zero code",
        "executor failed running",
        "failed to compute cache key",
        "no such file or directory",
        "syntax error",
        "invalid reference format", // bad image reference in FROM
    ];

    for marker in build_failure_markers {
        if lower.contains(marker) {
            return DockerSubprocessError::BuildFailed {
                stderr: stderr.to_string(),
                exit_code,
            };
        }
    }

    DockerSubprocessError::Other {
        stderr: stderr.to_string(),
        exit_code,
    }
}

/// Retry decision for a classified docker subprocess error.
///
/// Mirrors [`crate::oci::utils::classify_network_error`]'s contract:
/// retry only on transient/rate-limit; stop on auth, build-failed, and
/// unclassified errors.
pub fn classify_retry_decision(error: &DockerSubprocessError) -> RetryDecision {
    match error {
        DockerSubprocessError::TransientNetwork { .. }
        | DockerSubprocessError::RateLimit { .. } => RetryDecision::Retry,
        DockerSubprocessError::RegistryAuth { .. }
        | DockerSubprocessError::BuildFailed { .. }
        | DockerSubprocessError::Other { .. } => RetryDecision::Stop,
    }
}

/// Environment variable that, when set to a positive integer N, makes the
/// next N invocations of [`run_build_with_retry`] fail with a synthesized
/// transient-network stderr. Subsequent calls proceed normally. Used by the
/// integration test to simulate transient-failure-then-success without
/// needing a flaky registry. The variable is decremented on each forced
/// failure so a single test run can observe both the failure and recovery
/// branches.
///
/// Production code paths never set this variable. Mirrors the env-var-driven
/// fail-N-times approach referenced in issue #17.
pub const DEACON_TEST_DOCKER_FAIL_N: &str = "DEACON_TEST_DOCKER_FAIL_N";

/// Run a `docker` subprocess (typically `docker buildx build ...`) with
/// retry-on-transient. The caller passes the runtime binary path and the
/// already-assembled argv tail; this helper handles spawn, classification,
/// and backoff.
///
/// On success returns the captured `Output` (stdout + stderr + status). On
/// terminal failure (auth, build-failed, unknown) or exhausted retries
/// returns a `DockerError::CLIError` carrying the final stderr — preserving
/// existing call-site error rendering.
pub async fn run_build_with_retry(runtime_path: &Path, args: &[String]) -> Result<Output> {
    let config = RetryConfig::network();

    let result = retry_async(
        &config,
        || {
            // Re-borrow per attempt so the async block can `move` references
            // without consuming the outer FnMut closure state.
            async move {
                // Test hook: synthesize a transient failure for the first N calls.
                if let Some(forced) = take_forced_failure() {
                    return Err(DockerSubprocessError::TransientNetwork {
                        stderr: forced,
                        exit_code: 1,
                    });
                }

                let output = tokio::process::Command::new(runtime_path)
                    .args(args)
                    .output()
                    .await
                    .map_err(|e| DockerSubprocessError::Other {
                        stderr: format!("Failed to execute docker build: {}", e),
                        exit_code: -1,
                    })?;

                if output.status.success() {
                    return Ok(output);
                }

                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);
                Err(classify_docker_error(&stderr, exit_code))
            }
        },
        classify_retry_decision,
    )
    .await;

    result.map_err(|e| DockerError::CLIError(format!("Image build failed: {}", e.stderr())).into())
}

/// Pull-and-decrement the test-hook counter. Returns `Some(stderr)` if a
/// forced failure should be injected, else `None`. Always `None` outside
/// tests because production code does not set the env var.
fn take_forced_failure() -> Option<String> {
    let raw = std::env::var(DEACON_TEST_DOCKER_FAIL_N).ok()?;
    let n: u32 = raw.parse().ok()?;
    if n == 0 {
        return None;
    }
    // Decrement so the next attempt proceeds normally once we've consumed
    // the configured failure count.
    std::env::set_var(DEACON_TEST_DOCKER_FAIL_N, (n - 1).to_string());
    Some(format!(
        "synthetic transient failure: TLS handshake timeout (DEACON_TEST_DOCKER_FAIL_N={})",
        n
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_variant(stderr: &str, expected: fn(&DockerSubprocessError) -> bool) {
        let err = classify_docker_error(stderr, 1);
        assert!(
            expected(&err),
            "stderr {:?} classified as {:?}, did not match expected variant",
            stderr,
            err
        );
    }

    // --- Transient network classification ---

    #[test]
    fn classify_tls_handshake_timeout_is_transient() {
        let stderr = "failed to copy: httpReaderSeeker: failed open: \
                      Get \"https://registry-1.docker.io/v2/\": net/http: TLS handshake timeout";
        assert_variant(stderr, |e| {
            matches!(e, DockerSubprocessError::TransientNetwork { .. })
        });
        assert_eq!(
            classify_retry_decision(&classify_docker_error(stderr, 1)),
            RetryDecision::Retry
        );
    }

    #[test]
    fn classify_io_timeout_is_transient() {
        let stderr = "error during connect: Get \"https://index.docker.io/v1/\": \
                      dial tcp: i/o timeout";
        assert_variant(stderr, |e| {
            matches!(e, DockerSubprocessError::TransientNetwork { .. })
        });
    }

    #[test]
    fn classify_connection_reset_is_transient() {
        let stderr = "Get \"https://registry-1.docker.io/v2/library/alpine/manifests/3.18\": \
                      read tcp 172.17.0.2:443: connection reset by peer";
        assert_variant(stderr, |e| {
            matches!(e, DockerSubprocessError::TransientNetwork { .. })
        });
    }

    #[test]
    fn classify_unexpected_eof_is_transient() {
        let stderr = "failed to do request: Head \"...\" : unexpected EOF";
        assert_variant(stderr, |e| {
            matches!(e, DockerSubprocessError::TransientNetwork { .. })
        });
    }

    #[test]
    fn classify_5xx_from_registry_is_transient() {
        let stderr = "failed to fetch oauth token: unexpected status code 503 Service Unavailable";
        assert_variant(stderr, |e| {
            matches!(e, DockerSubprocessError::TransientNetwork { .. })
        });
    }

    // --- Rate limit ---

    #[test]
    fn classify_docker_hub_rate_limit() {
        let stderr = "toomanyrequests: You have reached your pull rate limit. \
                      You may increase the limit by authenticating and upgrading: \
                      https://www.docker.com/increase-rate-limit";
        let err = classify_docker_error(stderr, 1);
        assert!(matches!(err, DockerSubprocessError::RateLimit { .. }));
        assert_eq!(classify_retry_decision(&err), RetryDecision::Retry);
    }

    #[test]
    fn classify_generic_429_is_rate_limit() {
        let stderr = "received unexpected HTTP status: 429 Too Many Requests";
        assert_variant(stderr, |e| {
            matches!(e, DockerSubprocessError::RateLimit { .. })
        });
    }

    // --- Registry auth ---

    #[test]
    fn classify_401_unauthorized() {
        let stderr = "unauthorized: authentication required";
        let err = classify_docker_error(stderr, 1);
        assert!(matches!(err, DockerSubprocessError::RegistryAuth { .. }));
        assert_eq!(classify_retry_decision(&err), RetryDecision::Stop);
    }

    #[test]
    fn classify_403_denied_is_registry_auth() {
        let stderr = "denied: requested access to the resource is denied";
        assert_variant(stderr, |e| {
            matches!(e, DockerSubprocessError::RegistryAuth { .. })
        });
    }

    #[test]
    fn classify_pull_access_denied_is_registry_auth() {
        let stderr = "pull access denied for some/private-image, repository does not exist \
                      or may require 'docker login'";
        assert_variant(stderr, |e| {
            matches!(e, DockerSubprocessError::RegistryAuth { .. })
        });
    }

    // --- Build failures (terminal) ---

    #[test]
    fn classify_dockerfile_syntax_error_is_build_failed() {
        let stderr = "Dockerfile parse error on line 5: unknown instruction: FOOBAR";
        let err = classify_docker_error(stderr, 1);
        assert!(matches!(err, DockerSubprocessError::BuildFailed { .. }));
        assert_eq!(classify_retry_decision(&err), RetryDecision::Stop);
    }

    #[test]
    fn classify_missing_run_command_is_build_failed() {
        let stderr = "The command '/bin/sh -c apt-get install nonexistent-pkg' \
                      returned a non-zero code: 100";
        let err = classify_docker_error(stderr, 1);
        assert!(matches!(err, DockerSubprocessError::BuildFailed { .. }));
        assert_eq!(classify_retry_decision(&err), RetryDecision::Stop);
    }

    #[test]
    fn classify_buildkit_executor_failure_is_build_failed() {
        let stderr = "failed to solve: executor failed running [/bin/sh -c make]: \
                      exit code: 2";
        assert_variant(stderr, |e| {
            matches!(e, DockerSubprocessError::BuildFailed { .. })
        });
    }

    #[test]
    fn classify_missing_copy_source_is_build_failed() {
        let stderr = "failed to compute cache key: \"/notthere\" not found: \
                      no such file or directory";
        assert_variant(stderr, |e| {
            matches!(e, DockerSubprocessError::BuildFailed { .. })
        });
    }

    // --- Other / unclassified ---

    #[test]
    fn classify_unknown_stderr_is_other_and_not_retried() {
        let stderr = "some completely novel error message we have never seen";
        let err = classify_docker_error(stderr, 1);
        assert!(matches!(err, DockerSubprocessError::Other { .. }));
        assert_eq!(classify_retry_decision(&err), RetryDecision::Stop);
    }

    // --- Accessors ---

    #[test]
    fn stderr_and_exit_code_accessors_round_trip() {
        let err = classify_docker_error("toomanyrequests", 42);
        assert_eq!(err.exit_code(), 42);
        assert_eq!(err.stderr(), "toomanyrequests");
    }

    // --- Auth precedence over network markers ---
    // A 401 response can co-occur with a "dial tcp" message inside a wrapped
    // error; auth should win because it is terminal.
    #[test]
    fn auth_classification_wins_over_transient_markers() {
        let stderr = "Get \"https://registry/v2/\": dial tcp 1.2.3.4:443: unauthorized: bad token";
        let err = classify_docker_error(stderr, 1);
        assert!(matches!(err, DockerSubprocessError::RegistryAuth { .. }));
    }

    // --- Rate-limit precedence over generic build-failure markers ---
    #[test]
    fn rate_limit_classification_wins_over_build_markers() {
        // Constructed scenario: registry rejected with 429 but the wrapping
        // error mentions "executor failed running" further down. Rate limit
        // must take precedence because it is retryable.
        let stderr = "failed to solve: toomanyrequests: rate limit hit; \
                      executor failed running";
        let err = classify_docker_error(stderr, 1);
        assert!(matches!(err, DockerSubprocessError::RateLimit { .. }));
    }

    // ------------------------------------------------------------------
    // run_build_with_retry — integration-style tests
    //
    // These exercise the retry loop end-to-end using the
    // DEACON_TEST_DOCKER_FAIL_N env-var hook so we can simulate transient
    // failures without needing a real Docker daemon. They mirror the
    // env-var-driven fail-N-times pattern referenced in issue #17 and the
    // shape of the existing OCI retry tests in oci/utils.rs.
    //
    // Tests serialize on the global env var via a mutex.
    // ------------------------------------------------------------------

    use tokio::sync::Mutex;

    // tokio Mutex is async-aware: holding the guard across `.await` is safe
    // (sidesteps `clippy::await_holding_lock`). We serialize on the global
    // env var via this lock because tests in this module mutate
    // `DEACON_TEST_DOCKER_FAIL_N` and observe its post-state.
    static ENV_LOCK: Mutex<()> = Mutex::const_new(());

    fn clear_fail_hook() {
        std::env::remove_var(DEACON_TEST_DOCKER_FAIL_N);
    }

    /// The retry loop recovers once the synthetic transient failure counter
    /// drains. We run against `/bin/true` so that the real subprocess
    /// succeeds on the recovery attempt (no Docker daemon required).
    #[tokio::test]
    async fn run_build_recovers_after_transient_failures() {
        let _guard = ENV_LOCK.lock().await;
        clear_fail_hook();
        // Two synthetic transient failures, then real subprocess succeeds.
        std::env::set_var(DEACON_TEST_DOCKER_FAIL_N, "2");

        let result =
            run_build_with_retry(std::path::Path::new("/bin/true"), &[] as &[String]).await;

        assert!(
            result.is_ok(),
            "expected recovery after 2 transient failures, got {:?}",
            result
        );
        // Counter should be drained.
        assert_eq!(
            std::env::var(DEACON_TEST_DOCKER_FAIL_N).ok().as_deref(),
            Some("0")
        );
        clear_fail_hook();
    }

    /// If the synthetic failures exceed `max_attempts + 1`, the loop gives
    /// up and surfaces a `DockerError::CLIError` with the last stderr.
    #[tokio::test]
    async fn run_build_gives_up_after_exhausting_retries() {
        let _guard = ENV_LOCK.lock().await;
        clear_fail_hook();
        // Network profile is 3 retries (4 attempts total). 10 forced failures
        // guarantees exhaustion.
        std::env::set_var(DEACON_TEST_DOCKER_FAIL_N, "10");

        let result =
            run_build_with_retry(std::path::Path::new("/bin/true"), &[] as &[String]).await;

        assert!(
            result.is_err(),
            "expected exhaustion after retries, got {:?}",
            result
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Image build failed"),
            "error message should reference build failure, got {:?}",
            err
        );
        // After 4 attempts (initial + 3 retries) the counter should be 10-4 = 6.
        assert_eq!(
            std::env::var(DEACON_TEST_DOCKER_FAIL_N).ok().as_deref(),
            Some("6")
        );
        clear_fail_hook();
    }

    /// First attempt succeeds (no failures injected) — no retries, no env hook.
    #[tokio::test]
    async fn run_build_succeeds_on_first_attempt_without_hook() {
        let _guard = ENV_LOCK.lock().await;
        clear_fail_hook();

        let result =
            run_build_with_retry(std::path::Path::new("/bin/true"), &[] as &[String]).await;

        assert!(result.is_ok(), "expected success, got {:?}", result);
    }
}
