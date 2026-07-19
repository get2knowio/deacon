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
const DOCKER_PROBE_BOUND: Duration = Duration::from_secs(60);

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
async fn probe_docker(bin: &Path, bound: Duration) -> Result<(), HarnessError> {
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
