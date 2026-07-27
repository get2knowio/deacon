//! Prerequisite checks that fail with cause-specific errors, never booleans
//! (research D3/D10, FR-005). A missing prerequisite is a hard, named failure —
//! the harness never converts an absent Docker or fixture into a silent pass.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::HarnessError;

/// Path override for the docker CLI (the fault-injection seam).
pub const DOCKER_OVERRIDE_ENV: &str = "DEACON_PARITY_DOCKER";

/// Bound on the `docker version` probe. Docker's version handshake is quick; a
/// slow/hung daemon is itself a "Docker unavailable" signal.
pub const DOCKER_PROBE_BOUND: Duration = Duration::from_secs(60);

/// Require a working Docker CLI. Honors `DEACON_PARITY_DOCKER` (else `docker` on
/// PATH) and probes `docker version`. Any failure → [`HarnessError::DockerMissing`].
pub async fn require_docker() -> Result<(), HarnessError> {
    let bin = std::env::var_os(DOCKER_OVERRIDE_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("docker"));
    probe_docker(&bin, DOCKER_PROBE_BOUND).await
}

/// Probe a specific docker binary. Pure over its inputs so fault-injection can
/// point it at a failing stub.
///
/// **Public as the injection seam** (mirroring
/// [`runner::containers_for_workspace_with`](crate::runner::containers_for_workspace_with)):
/// the alternative is for a test to set `DEACON_PARITY_DOCKER`, and `std::env::set_var`
/// is `unsafe` under this workspace's edition — which `unsafe_code = "deny"` forbids —
/// besides being process-global and therefore hostile to a parallel test runner.
pub async fn probe_docker(bin: &Path, bound: Duration) -> Result<(), HarnessError> {
    let mut cmd = tokio::process::Command::new(bin);
    cmd.arg("version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);

    match tokio::time::timeout(bound, cmd.status()).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        _ => Err(HarnessError::DockerMissing),
    }
}

/// Require a fixture path to exist. Absence → [`HarnessError::FixtureMissing`].
pub fn require_fixture(path: &Path) -> Result<(), HarnessError> {
    if path.exists() {
        Ok(())
    } else {
        Err(HarnessError::FixtureMissing {
            path: path.to_path_buf(),
        })
    }
}

/// Environment override naming the deacon binary under test explicitly.
pub const DEACON_BIN_ENV: &str = "DEACON_PARITY_DEACON_BIN";

/// The deacon binary under test — **built, then taken from cargo's own artifact report**.
///
/// This must be the SAME binary the parity tests exercise. Every parity test binary uses
/// `env!("CARGO_BIN_EXE_deacon")`, which is the artifact cargo just compiled; a bin has no
/// such macro, so it has to establish the equivalent itself.
///
/// It previously guessed — preferring `target/release/deacon` if the file merely existed,
/// else `target/debug/deacon` — and that was a real defect with teeth. A release artifact
/// left over from an earlier day satisfied the check, so the ledger compared a **stale
/// deacon** against the current oracle. Observed live: a three-day-old `target/release/deacon`
/// still injected `${containerEnv:VAR}` into a lifecycle command string (the #332 hazard
/// since fixed), which the ledger faithfully reported as the replacement being `stricter`
/// than the legacy path. The finding was entirely an artifact of the binary chosen.
///
/// The failure mode that matters is the mirror image: a stale build that happens to AGREE
/// with the reference would have produced a false `equivalent` and authorized deleting real
/// coverage. A gate for an irreversible act cannot guess which binary it is judging, so this
/// builds and reads the path back rather than looking for one.
///
/// [`DEACON_BIN_ENV`] still overrides, for pointing at a specific build deliberately.
///
/// Lives here rather than in one bin because it is now shared by BOTH bins that run deacon
/// outside a test harness (`equivalence-report` and `coverage-regressions`). A second copy
/// of a "do not guess the binary" rule is the copy that rots (Constitution VIII).
/// Async because the build it shells out to can take minutes on a cold `target/`, and
/// both callers reach it from inside `runtime.block_on`. A `std::process::Command` here
/// blocked a tokio worker thread for the whole compile (constitution V) — the same
/// pattern the docker probe above already avoids.
pub async fn deacon_binary() -> Result<PathBuf, HarnessError> {
    if let Some(path) = std::env::var_os(DEACON_BIN_ENV) {
        let path = PathBuf::from(path);
        if path.is_file() {
            eprintln!(
                "using the deacon binary from {DEACON_BIN_ENV}: {}",
                path.display()
            );
            return Ok(path);
        }
        return Err(HarnessError::FixtureMissing { path });
    }

    let output = tokio::process::Command::new("cargo")
        .args(["build", "-p", "deacon", "--message-format", "json"])
        .current_dir(crate::workspace_root())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| HarnessError::Report {
            cause: format!("could not build the deacon binary under test: {e}"),
        })?;
    if !output.status.success() {
        return Err(HarnessError::Report {
            cause: format!(
                "building the deacon binary under test failed ({}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }

    // cargo emits one JSON object per line; the `deacon` bin artifact carries the path.
    let mut executable: Option<PathBuf> = None;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value.get("reason").and_then(|v| v.as_str()) != Some("compiler-artifact") {
            continue;
        }
        if value.pointer("/target/name").and_then(|v| v.as_str()) != Some("deacon") {
            continue;
        }
        if let Some(path) = value.get("executable").and_then(|v| v.as_str()) {
            executable = Some(PathBuf::from(path));
        }
    }

    let path = executable.ok_or_else(|| HarnessError::Report {
        cause: "cargo reported no `deacon` executable artifact — refusing to guess which \
                binary to judge"
            .to_string(),
    })?;
    eprintln!("deacon binary under test: {}", path.display());
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_fixture_ok_when_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        require_fixture(dir.path()).expect("existing dir is a valid fixture");
        let f = dir.path().join("file");
        std::fs::write(&f, b"x").expect("write");
        require_fixture(&f).expect("existing file is a valid fixture");
    }

    #[test]
    fn require_fixture_names_missing_path() {
        let missing = PathBuf::from("/definitely/not/here/fixture.json");
        let err = require_fixture(&missing).expect_err("must fail");
        match err {
            HarnessError::FixtureMissing { path } => assert_eq!(path, missing),
            other => panic!("expected FixtureMissing, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failing_docker_stub_is_docker_missing() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let stub = dir.path().join("docker");
        std::fs::write(&stub, "#!/bin/sh\nexit 1\n").expect("write stub");
        let mut perms = std::fs::metadata(&stub).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&stub, perms).unwrap();

        let err = probe_docker(&stub, DOCKER_PROBE_BOUND)
            .await
            .expect_err("failing docker must be reported missing");
        assert!(matches!(err, HarnessError::DockerMissing));
    }

    #[tokio::test]
    async fn nonexistent_docker_is_docker_missing() {
        let err = probe_docker(Path::new("/nonexistent/docker"), DOCKER_PROBE_BOUND)
            .await
            .expect_err("nonexistent docker must be reported missing");
        assert!(matches!(err, HarnessError::DockerMissing));
    }
}
