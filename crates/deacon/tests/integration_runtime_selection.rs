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

// This test demonstrates that runtime selection works for up command specifically
// We expect either success (if podman is installed) or a clear error about podman not being installed
#[test]
fn test_up_command_with_podman_runtime() -> Result<()> {
    use tempfile::TempDir;

    // Create a temporary directory with a basic devcontainer.json
    let temp_dir = TempDir::new()?;
    let devcontainer_path = temp_dir
        .path()
        .join(".devcontainer")
        .join("devcontainer.json");
    std::fs::create_dir_all(devcontainer_path.parent().unwrap())?;
    std::fs::write(
        &devcontainer_path,
        r#"{"image": "mcr.microsoft.com/devcontainers/base:ubuntu"}"#,
    )?;

    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.current_dir(temp_dir.path())
        .env("DEACON_CONTAINER_RUNTIME", "podman")
        .args([
            "--workspace-folder",
            temp_dir.path().to_str().unwrap(),
            "up",
            "--skip-post-create",
            "--skip-non-blocking-commands",
        ]);

    // The test can succeed if podman is installed, or fail with a clear error if not
    // We just verify that it doesn't say "Not implemented yet"
    let output = cmd.output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should NOT contain the old "Not implemented yet" message
    assert!(
        !stderr.contains("Not implemented yet: Podman support"),
        "Podman runtime should be implemented, not stubbed. stderr: {}",
        stderr
    );

    // If it succeeded, ensure there are no availability error hints
    if output.status.success() {
        assert!(
            !stderr.to_ascii_lowercase().contains("not installed")
                && !stderr.to_ascii_lowercase().contains("not found"),
            "Successful run should not mention podman availability errors. stderr: {}",
            stderr
        );
    }

    // If it failed, it should be a clear runtime error (podman not found/available)
    // or potentially a configuration error if validation happens first
    if !output.status.success() {
        assert!(
            stderr.to_ascii_lowercase().contains("podman")
                || stderr.to_ascii_lowercase().contains("not found")
                || stderr.to_ascii_lowercase().contains("not installed")
                || stderr.to_ascii_lowercase().contains("configuration"),
            "Expected clear podman availability or configuration error, got: {}",
            stderr
        );
    }

    Ok(())
}

/// With no runtime chosen and no `docker` on PATH, `up` runs the `podman` that IS there.
///
/// The PATH holds only a recording fake `podman`, so the test proves which binary ran
/// rather than inferring it from an error message, and touches no real runtime. HOME is
/// redirected so nothing lands in the developer's `~/.deacon`. `#[cfg(unix)]` because the
/// fake is a `/bin/sh` script; the lookup itself is covered on every platform by the
/// `runtime` unit tests.
#[cfg(unix)]
#[test]
fn test_up_falls_back_to_podman_when_docker_is_not_on_path() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    let temp_dir = TempDir::new()?;
    let bin = temp_dir.path().join("bin");
    let workspace = temp_dir.path().join("ws");
    let home = temp_dir.path().join("home");
    let calls = temp_dir.path().join("podman-calls.log");
    std::fs::create_dir_all(&bin)?;
    std::fs::create_dir_all(workspace.join(".devcontainer"))?;
    std::fs::create_dir_all(&home)?;
    std::fs::write(
        workspace.join(".devcontainer").join("devcontainer.json"),
        r#"{"image": "alpine:3.18"}"#,
    )?;
    let fake = bin.join("podman");
    std::fs::write(
        &fake,
        format!("#!/bin/sh\necho \"$@\" >> '{}'\nexit 1\n", calls.display()),
    )?;
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755))?;

    let output = Command::cargo_bin("deacon")?
        .env_clear()
        .env("PATH", &bin)
        .env("HOME", &home)
        .args([
            "--workspace-folder",
            workspace.to_str().unwrap(),
            "--log-level",
            "info",
            "up",
        ])
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    let recorded = std::fs::read_to_string(&calls).unwrap_or_default();
    assert!(
        !recorded.trim().is_empty(),
        "the podman on PATH was never run. stderr: {stderr}"
    );
    assert!(
        stderr.contains("No `docker` on PATH; using podman"),
        "the fallback must announce itself. stderr: {stderr}"
    );
    assert!(
        !stderr.contains("Docker is not installed"),
        "a podman-only host must not be told Docker is missing. stderr: {stderr}"
    );
    Ok(())
}

/// An explicit `--runtime podman` whose binary is missing is reported as PODMAN missing,
/// naming the binary — not as "Docker is not installed".
#[test]
fn test_missing_podman_is_reported_as_podman() -> Result<()> {
    use tempfile::TempDir;

    let temp_dir = TempDir::new()?;
    let workspace = temp_dir.path();
    std::fs::create_dir_all(workspace.join(".devcontainer"))?;
    std::fs::write(
        workspace.join(".devcontainer").join("devcontainer.json"),
        r#"{"image": "alpine:3.18"}"#,
    )?;
    let missing = workspace.join("no-such-bin").join("podman");

    let output = Command::cargo_bin("deacon")?
        .args([
            "--runtime",
            "podman",
            "--workspace-folder",
            workspace.to_str().unwrap(),
            "--docker-path",
            missing.to_str().unwrap(),
            "up",
        ])
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "stderr: {stderr}");
    assert!(
        stderr.contains("Podman is not installed or not accessible"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains(missing.to_str().unwrap()),
        "the message must name the binary that could not be run. stderr: {stderr}"
    );
    assert!(
        !stderr.contains("Docker is not installed"),
        "stderr: {stderr}"
    );
    Ok(())
}
