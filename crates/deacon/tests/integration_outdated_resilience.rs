#![cfg(feature = "full")]
// T042: Resilience test for outdated subcommand (User Story 3)
//
// Tests graceful handling of registry failures, network issues, and invalid references

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::error::Error;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_outdated_graceful_registry_failure() -> Result<(), Box<dyn Error>> {
    // Force OCI client failure to simulate registry unavailability
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:18": {},
        "ghcr.io/devcontainers/features/python:3.11": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    // Force OCI failure by pointing to non-existent CA bundle
    let nonexist = td.path().join("nonexistent-ca.pem");

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    cmd.arg("outdated").arg("--workspace-folder").arg(td.path());

    // Should succeed (exit 0) despite registry failure
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
        ));

    Ok(())
}

#[test]
fn test_outdated_graceful_registry_failure_json() -> Result<(), Box<dyn Error>> {
    // Test JSON output with registry failures - should show nulls for latest
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:18": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

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
        .arg("json");

    let output = cmd.output()?;
    assert!(output.status.success());

    let json_str = String::from_utf8(output.stdout)?;
    let parsed: serde_json::Value = serde_json::from_str(&json_str)?;

    let node = &parsed["features"]["ghcr.io/devcontainers/features/node"];

    // current and wanted should be present, latest should be null
    assert_eq!(node["current"], "18");
    assert_eq!(node["wanted"], "18");
    assert!(node["latest"].is_null());

    Ok(())
}

#[test]
fn test_outdated_invalid_feature_reference() -> Result<(), Box<dyn Error>> {
    // Non-versionable/invalid references should not cause failure
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    // Mix valid and digest-based references
    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:18": {},
        "ghcr.io/devcontainers/features/python@sha256:deadbeef": {}
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

    // Should succeed and show both features
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "ghcr.io/devcontainers/features/node",
        ))
        .stdout(predicate::str::contains(
            "ghcr.io/devcontainers/features/python",
        ));

    Ok(())
}

#[test]
fn test_outdated_missing_lockfile_fallback() -> Result<(), Box<dyn Error>> {
    // Without lockfile, current should fall back to wanted
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:18": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    // No lockfile created

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
        .arg("json");

    let output = cmd.output()?;
    assert!(output.status.success());

    let json_str = String::from_utf8(output.stdout)?;
    let parsed: serde_json::Value = serde_json::from_str(&json_str)?;

    let node = &parsed["features"]["ghcr.io/devcontainers/features/node"];

    // Both current and wanted should be "18" (fallback)
    assert_eq!(node["current"], "18");
    assert_eq!(node["wanted"], "18");

    Ok(())
}

#[test]
fn test_outdated_partial_registry_failure() -> Result<(), Box<dyn Error>> {
    // Test that if some features can't be fetched, others still work
    // (In this test, all will fail due to forced OCI error, but the structure should handle partial failures)
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
    cmd.arg("outdated")
        .arg("--workspace-folder")
        .arg(td.path())
        .arg("--output")
        .arg("json");

    let output = cmd.output()?;
    assert!(output.status.success());

    let json_str = String::from_utf8(output.stdout)?;
    let parsed: serde_json::Value = serde_json::from_str(&json_str)?;

    let features = parsed["features"].as_object().unwrap();

    // All three features should be present in output
    assert_eq!(features.len(), 3);

    // Each should have current and wanted, but latest is null
    for (_, feature) in features.iter() {
        assert!(feature.get("current").is_some());
        assert!(feature.get("wanted").is_some());
        assert!(feature["latest"].is_null());
    }

    Ok(())
}

#[test]
fn test_outdated_deterministic_output_despite_parallel_fetching() -> Result<(), Box<dyn Error>> {
    // Run multiple times to ensure order is consistent (tests determinism)
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/a:1": {},
        "ghcr.io/devcontainers/features/b:1": {},
        "ghcr.io/devcontainers/features/c:1": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    let nonexist = td.path().join("nonexistent-ca.pem");

    let mut outputs = Vec::new();

    // Run 3 times
    for _ in 0..3 {
        let mut cmd = std::process::Command::cargo_bin("deacon")?;
        cmd.env(
            "DEACON_CUSTOM_CA_BUNDLE",
            nonexist.to_string_lossy().to_string(),
        );
        cmd.arg("outdated").arg("--workspace-folder").arg(td.path());

        let output = cmd.output()?;
        assert!(output.status.success());
        outputs.push(String::from_utf8(output.stdout)?);
    }

    // All outputs should be identical (deterministic ordering)
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[1], outputs[2]);

    Ok(())
}

#[test]
fn test_outdated_config_not_found_exit_1() -> Result<(), Box<dyn Error>> {
    // Config not found should return exit code 1 (not 0)
    let td = tempdir()?;

    // No .devcontainer directory or config

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.arg("outdated").arg("--workspace-folder").arg(td.path());

    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("not found"));

    Ok(())
}

#[test]
fn test_outdated_no_credentials_in_logs() -> Result<(), Box<dyn Error>> {
    // Verify no sensitive information appears in output
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{
      "features": {
        "ghcr.io/devcontainers/features/node:18": {}
      }
    }"#;
    fs::write(devcontainer_dir.join("devcontainer.json"), config)?;

    let nonexist = td.path().join("nonexistent-ca.pem");

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    // Set an env var that might contain sensitive info
    cmd.env("DEACON_TEST_TOKEN", "secret-token-12345");
    cmd.arg("outdated").arg("--workspace-folder").arg(td.path());

    let output = cmd.output()?;

    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;

    // Should not leak the secret token
    assert!(!stdout.contains("secret-token"));
    assert!(!stderr.contains("secret-token"));

    Ok(())
}
