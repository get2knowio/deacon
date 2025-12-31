#![cfg(feature = "full")]
// T039: CLI integration test for text output (User Story 1)
//
// Tests human-readable text table rendering and ordering

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::error::Error;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_outdated_text_multiple_features() -> Result<(), Box<dyn Error>> {
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:18": {},
        "ghcr.io/devcontainers/features/python:3.11": {},
        "ghcr.io/devcontainers/features/rust:1.70": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    let nonexist = td.path().join("nonexistent-ca.pem");

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    cmd.arg("outdated").arg("--workspace-folder").arg(td.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "Feature | Current | Wanted | Latest",
        ))
        .stdout(predicate::str::contains(
            "ghcr.io/devcontainers/features/node",
        ))
        .stdout(predicate::str::contains(
            "ghcr.io/devcontainers/features/python",
        ))
        .stdout(predicate::str::contains(
            "ghcr.io/devcontainers/features/rust",
        ));

    Ok(())
}

#[test]
fn test_outdated_text_with_lockfile() -> Result<(), Box<dyn Error>> {
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:20": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    // Lockfile shows older version installed
    let lockfile = r#"{
  "features": {
    "ghcr.io/devcontainers/features/node": {
      "version": "18.5.0",
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

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("18.5.0")) // current from lockfile
        .stdout(predicate::str::contains("| 20 |")); // wanted from config

    Ok(())
}

#[test]
fn test_outdated_text_empty_features() -> Result<(), Box<dyn Error>> {
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{ "features": {} }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.arg("outdated").arg("--workspace-folder").arg(td.path());

    // Should show header only
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "Feature | Current | Wanted | Latest",
        ))
        .stdout(predicate::str::contains("ghcr.io").not());

    Ok(())
}

#[test]
fn test_outdated_text_unknown_values_shown_as_dash() -> Result<(), Box<dyn Error>> {
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    // Digest reference (no wanted version)
    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node@sha256:abcdef1234567890": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    let nonexist = td.path().join("nonexistent-ca.pem");

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    cmd.arg("outdated").arg("--workspace-folder").arg(td.path());

    let output = cmd.output()?;
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout)?;

    // Should show dashes for unknown values
    assert!(stdout.contains(" - |"));

    Ok(())
}

#[test]
fn test_outdated_text_preserves_order() -> Result<(), Box<dyn Error>> {
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    // Intentionally non-alphabetical order
    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/zzz:1": {},
        "ghcr.io/devcontainers/features/aaa:1": {},
        "ghcr.io/devcontainers/features/mmm:1": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    let nonexist = td.path().join("nonexistent-ca.pem");

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    cmd.arg("outdated").arg("--workspace-folder").arg(td.path());

    let output = cmd.output()?;
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout)?;
    let lines: Vec<&str> = stdout.lines().collect();

    // Find line indices for each feature
    let zzz_idx = lines
        .iter()
        .position(|line| line.contains("/zzz"))
        .expect("zzz not found");
    let aaa_idx = lines
        .iter()
        .position(|line| line.contains("/aaa"))
        .expect("aaa not found");
    let mmm_idx = lines
        .iter()
        .position(|line| line.contains("/mmm"))
        .expect("mmm not found");

    // Order should be zzz, aaa, mmm (as declared in config)
    assert!(zzz_idx < aaa_idx, "zzz should come before aaa");
    assert!(aaa_idx < mmm_idx, "aaa should come before mmm");

    Ok(())
}

#[test]
fn test_outdated_text_with_semver_versions() -> Result<(), Box<dyn Error>> {
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:18.16.0": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    let nonexist = td.path().join("nonexistent-ca.pem");

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    cmd.arg("outdated").arg("--workspace-folder").arg(td.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("18.16.0"));

    Ok(())
}

#[test]
fn test_outdated_text_strips_v_prefix() -> Result<(), Box<dyn Error>> {
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    // Tag with v prefix
    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:v18.0.0": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    let nonexist = td.path().join("nonexistent-ca.pem");

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    cmd.arg("outdated").arg("--workspace-folder").arg(td.path());

    let output = cmd.output()?;
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout)?;

    // Should show 18.0.0 without the v prefix
    assert!(stdout.contains("18.0.0"));
    assert!(!stdout.contains("v18"));

    Ok(())
}
