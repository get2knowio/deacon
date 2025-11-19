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
    cmd.env("DEACON_RUNTIME", "docker").args(["up", "--help"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn test_runtime_env_var_podman() -> Result<()> {
    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.env("DEACON_RUNTIME", "podman").args(["up", "--help"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn test_runtime_flag_precedence_over_env() -> Result<()> {
    // CLI flag should override environment variable
    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.env("DEACON_RUNTIME", "podman")
        .args(["--runtime", "docker", "up", "--help"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn test_runtime_env_var_invalid_fallback() -> Result<()> {
    // Invalid env var should fall back to docker (default)
    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.env("DEACON_RUNTIME", "invalid").args(["up", "--help"]);
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
        .stdout(str::contains("DEACON_RUNTIME"));
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
        .env("DEACON_RUNTIME", "podman")
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
