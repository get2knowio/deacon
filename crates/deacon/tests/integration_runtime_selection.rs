//! Integration tests for container runtime selection
//!
//! Tests the runtime selection behavior via CLI flags and environment variables.

use anyhow::Result;
use assert_cmd::Command;
use predicates::str;

#[test]
fn test_runtime_flag_docker() -> Result<()> {
    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.args(["--runtime", "docker", "up", "--help"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn test_runtime_flag_podman() -> Result<()> {
    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.args(["--runtime", "podman", "up", "--help"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn test_runtime_flag_invalid() -> Result<()> {
    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.args(["--runtime", "invalid", "--help"]);
    cmd.assert()
        .failure()
        .stderr(str::contains("invalid value 'invalid'"));
    Ok(())
}

#[test]
fn test_runtime_env_var_docker() -> Result<()> {
    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.env("DEACON_CONTAINER_RUNTIME", "docker")
        .args(["up", "--help"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn test_runtime_env_var_podman() -> Result<()> {
    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.env("DEACON_CONTAINER_RUNTIME", "podman")
        .args(["up", "--help"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn test_runtime_flag_precedence_over_env() -> Result<()> {
    // CLI flag should override environment variable
    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.env("DEACON_CONTAINER_RUNTIME", "podman")
        .args(["--runtime", "docker", "up", "--help"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn test_runtime_env_var_invalid_fallback() -> Result<()> {
    // Invalid env var should fall back to docker (default)
    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.env("DEACON_CONTAINER_RUNTIME", "invalid")
        .args(["up", "--help"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn test_runtime_selection_help_shows_options() -> Result<()> {
    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.args(["--help"]);
    cmd.assert()
        .success()
        .stdout(str::contains("--runtime"))
        .stdout(str::contains("docker"))
        .stdout(str::contains("podman"))
        .stdout(str::contains("DEACON_CONTAINER_RUNTIME"));
    Ok(())
}

// `up` under podman reaches the runtime instead of a "not implemented" stub.
//
// Hermetic on purpose: `--docker-path` names a binary that does not exist, so the outcome
// is the same on every host. This test used to run `up` against whatever podman the host
// had and accept any stderr containing one of a few words. That made it flaky in the fast
// lane (a broken rootless podman answers differently from run to run), and on a host
// with a working podman it pulled a registry image in a lane that is meant to be offline.
#[test]
fn test_up_command_with_podman_runtime() -> Result<()> {
    use tempfile::TempDir;

    let temp_dir = TempDir::new()?;
    let devcontainer_path = temp_dir
        .path()
        .join(".devcontainer")
        .join("devcontainer.json");
    std::fs::create_dir_all(devcontainer_path.parent().unwrap())?;
    std::fs::write(&devcontainer_path, r#"{"image": "alpine:3.18"}"#)?;
    let missing_podman = temp_dir.path().join("no-such-bin").join("podman");

    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.current_dir(temp_dir.path())
        .env("DEACON_CONTAINER_RUNTIME", "podman")
        .args([
            "--workspace-folder",
            temp_dir.path().to_str().unwrap(),
            "--docker-path",
            missing_podman.to_str().unwrap(),
            "up",
            "--skip-post-create",
            "--skip-non-blocking-commands",
        ]);

    let output = cmd.output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("Not implemented yet: Podman support"),
        "Podman runtime should be implemented, not stubbed. stderr: {}",
        stderr
    );
    assert!(
        !output.status.success(),
        "up must fail when the selected runtime binary does not exist. stderr: {}",
        stderr
    );
    assert!(
        stderr.to_ascii_lowercase().contains("not installed"),
        "Expected the missing runtime to be reported as not installed, got: {}",
        stderr
    );

    Ok(())
}
