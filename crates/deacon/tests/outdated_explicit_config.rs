#![cfg(feature = "full")]
use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::error::Error;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_outdated_respects_explicit_config_path() -> Result<(), Box<dyn Error>> {
    // Create a temporary workspace with custom config location
    let td = tempdir()?;

    // Create a custom config file in a non-standard location
    let custom_config_path = td.path().join("custom-config.json");
    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:18": {},
        "ghcr.io/devcontainers/features/python:3.11": {}
      }
    }"#;
    fs::write(&custom_config_path, config)?;

    // Set an env var to force the OCI client creation to fail (makes fetch_latest_stable_version return None)
    let nonexist = td.path().join("nonexistent-ca.pem");

    // Use --config flag to point to the custom config
    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    cmd.arg("--config")
        .arg(&custom_config_path)
        .arg("outdated")
        .arg("--workspace-folder")
        .arg(td.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "Feature | Current | Wanted | Latest",
        ))
        .stdout(predicate::str::contains(
            "ghcr.io/devcontainers/features/node | 18 | 18 |",
        ))
        .stdout(predicate::str::contains(
            "ghcr.io/devcontainers/features/python | 3.11 | 3.11 |",
        ));

    Ok(())
}

#[test]
fn test_outdated_respects_override_config_path() -> Result<(), Box<dyn Error>> {
    // Create a temporary workspace with multiple config files
    let td = tempdir()?;

    // Create default config
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;
    let default_config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/rust:1": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), default_config)?;

    // Create override config with different features
    let override_config_path = td.path().join("override-config.json");
    let override_config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:18": {}
      }
    }"#;
    fs::write(&override_config_path, override_config)?;

    // Set an env var to force the OCI client creation to fail
    let nonexist = td.path().join("nonexistent-ca.pem");

    // Use --override-config flag which should take precedence
    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    cmd.arg("--override-config")
        .arg(&override_config_path)
        .arg("outdated")
        .arg("--workspace-folder")
        .arg(td.path());

    // Should show node (from override) not rust (from default)
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "ghcr.io/devcontainers/features/node",
        ))
        .stdout(predicate::str::contains("ghcr.io/devcontainers/features/rust").not());

    Ok(())
}

#[test]
fn test_outdated_fails_with_nonexistent_explicit_config() -> Result<(), Box<dyn Error>> {
    let td = tempdir()?;

    // Try to use a non-existent config file
    let nonexistent_config = td.path().join("does-not-exist.json");

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.arg("--config")
        .arg(&nonexistent_config)
        .arg("outdated")
        .arg("--workspace-folder")
        .arg(td.path());

    // Should fail with appropriate error message
    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("not found"));

    Ok(())
}
