#![cfg(feature = "full")]
// T041: CLI integration test for --fail-on-outdated exit 2 (User Story 2)
//
// Tests that the --fail-on-outdated flag causes exit code 2 when outdated features are detected

use assert_cmd::prelude::*;
use std::error::Error;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_outdated_fail_flag_with_outdated_features() -> Result<(), Box<dyn Error>> {
    // Create a config with features and a lockfile showing older versions
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    // Config declares node:20
    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:20": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    // Lockfile shows we have node:18 installed (outdated: current < wanted)
    let lockfile = r#"{
  "features": {
    "ghcr.io/devcontainers/features/node": {
      "version": "18.0.0",
      "resolved": "ghcr.io/devcontainers/features/node@sha256:abc1234567890abcdef1234567890abcdef1234567890abcdef1234567890abc",
      "integrity": "sha256:abc1234567890abcdef1234567890abcdef1234567890abcdef1234567890abc"
    }
  }
}"#;
    fs::write(devcontainer_dir.join("devcontainer-lock.json"), lockfile)?;

    // Force OCI failure so latest is null (not relevant for this test)
    let nonexist = td.path().join("nonexistent-ca.pem");

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    cmd.arg("outdated")
        .arg("--workspace-folder")
        .arg(td.path())
        .arg("--fail-on-outdated");

    // Should fail with exit code 2
    cmd.assert().failure().code(2);

    Ok(())
}

#[test]
fn test_outdated_fail_flag_with_up_to_date_features() -> Result<(), Box<dyn Error>> {
    // Config and lockfile match - no outdated features
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:18": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    // Lockfile matches wanted version
    let lockfile = r#"{
  "features": {
    "ghcr.io/devcontainers/features/node": {
      "version": "18.0.0",
      "resolved": "ghcr.io/devcontainers/features/node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "integrity": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    }
  }
}"#;
    fs::write(devcontainer_dir.join("devcontainer-lock.json"), lockfile)?;

    let nonexist = td.path().join("nonexistent-ca.pem");

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    cmd.arg("outdated")
        .arg("--workspace-folder")
        .arg(td.path())
        .arg("--fail-on-outdated");

    // Should succeed with exit code 0
    cmd.assert().success();

    Ok(())
}

#[test]
fn test_outdated_fail_flag_with_json_output() -> Result<(), Box<dyn Error>> {
    // Test --fail-on-outdated works with JSON output too
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/python:3.11": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    // Lockfile shows older version
    let lockfile = r#"{
  "features": {
    "ghcr.io/devcontainers/features/python": {
      "version": "3.10.0",
      "resolved": "ghcr.io/devcontainers/features/python@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "integrity": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    }
  }
}"#;
    fs::write(devcontainer_dir.join("devcontainer-lock.json"), lockfile)?;

    let nonexist = td.path().join("nonexistent-ca.pem");

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    cmd.arg("outdated")
        .arg("--workspace-folder")
        .arg(td.path())
        .arg("--output")
        .arg("json")
        .arg("--fail-on-outdated");

    let output = cmd.output()?;

    // Should fail with exit code 2
    assert_eq!(output.status.code(), Some(2));

    // But should still emit valid JSON
    let json_str = String::from_utf8(output.stdout)?;
    let _parsed: serde_json::Value = serde_json::from_str(&json_str)?;

    Ok(())
}

#[test]
fn test_outdated_fail_flag_without_flag_still_succeeds() -> Result<(), Box<dyn Error>> {
    // Without --fail-on-outdated, outdated features should not cause exit 2
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:20": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    let lockfile = r#"{
  "features": {
    "ghcr.io/devcontainers/features/node": {
      "version": "18.0.0",
      "resolved": "ghcr.io/devcontainers/features/node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "integrity": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    }
  }
}"#;
    fs::write(devcontainer_dir.join("devcontainer-lock.json"), lockfile)?;

    let nonexist = td.path().join("nonexistent-ca.pem");

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    cmd.arg("outdated").arg("--workspace-folder").arg(td.path());
    // NO --fail-on-outdated flag

    // Should succeed with exit code 0
    cmd.assert().success();

    Ok(())
}

#[test]
fn test_outdated_fail_flag_empty_features() -> Result<(), Box<dyn Error>> {
    // With no features, --fail-on-outdated should succeed (nothing is outdated)
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{ "features": {} }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.arg("outdated")
        .arg("--workspace-folder")
        .arg(td.path())
        .arg("--fail-on-outdated");

    cmd.assert().success();

    Ok(())
}
