#![cfg(feature = "full")]
// T045: Performance validation for ~20 features with mocked registry (≤10s)
//
// Tests that the outdated command completes within acceptable time limits
// when handling a realistic number of features (using mocked/failed OCI to avoid network)

use assert_cmd::prelude::*;
use std::error::Error;
use std::fs;
use std::time::Instant;
use tempfile::tempdir;

#[test]
fn test_outdated_performance_20_features() -> Result<(), Box<dyn Error>> {
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    // Create config with 20 features
    let mut features = Vec::new();
    for i in 1..=20 {
        features.push(format!(
            r#"    "ghcr.io/devcontainers/features/feature{}:1.0": {{}}"#,
            i
        ));
    }
    let features_str = features.join(",\n");

    let config = format!(
        r#"{{
  "features": {{
{}
  }}
}}"#,
        features_str
    );

    fs::write(devcontainer_dir.join("devcontainer.json"), &config)?;

    // Force OCI client failure (mocked registry - no actual network calls)
    let nonexist = td.path().join("nonexistent-ca.pem");

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    cmd.arg("outdated").arg("--workspace-folder").arg(td.path());

    let start = Instant::now();
    let output = cmd.output()?;
    let elapsed = start.elapsed();

    assert!(output.status.success());

    // Should complete within 10 seconds (even with mocked failures)
    assert!(
        elapsed.as_secs() <= 10,
        "Command took {} seconds (expected ≤10s)",
        elapsed.as_secs()
    );

    // Verify all features are present in output
    let stdout = String::from_utf8(output.stdout)?;
    for i in 1..=20 {
        assert!(
            stdout.contains(&format!("feature{}", i)),
            "Feature {} not found in output",
            i
        );
    }

    Ok(())
}

#[test]
fn test_outdated_performance_20_features_json() -> Result<(), Box<dyn Error>> {
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    // Create config with 20 features
    let mut features = Vec::new();
    for i in 1..=20 {
        features.push(format!(
            r#"    "ghcr.io/devcontainers/features/feature{}:2.0": {{}}"#,
            i
        ));
    }
    let features_str = features.join(",\n");

    let config = format!(
        r#"{{
  "features": {{
{}
  }}
}}"#,
        features_str
    );

    fs::write(devcontainer_dir.join("devcontainer.json"), &config)?;

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

    let start = Instant::now();
    let output = cmd.output()?;
    let elapsed = start.elapsed();

    assert!(output.status.success());

    // JSON output should also complete within 10 seconds
    assert!(
        elapsed.as_secs() <= 10,
        "Command took {} seconds (expected ≤10s)",
        elapsed.as_secs()
    );

    let json_str = String::from_utf8(output.stdout)?;
    let parsed: serde_json::Value = serde_json::from_str(&json_str)?;

    let features_obj = parsed["features"].as_object().unwrap();
    assert_eq!(features_obj.len(), 20);

    Ok(())
}

#[test]
fn test_outdated_performance_with_lockfile() -> Result<(), Box<dyn Error>> {
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    // Create config with 15 features
    let mut config_features = Vec::new();
    for i in 1..=15 {
        config_features.push(format!(
            r#"    "ghcr.io/devcontainers/features/feature{}:1.0": {{}}"#,
            i
        ));
    }
    let config = format!(
        r#"{{
  "features": {{
{}
  }}
}}"#,
        config_features.join(",\n")
    );
    fs::write(devcontainer_dir.join("devcontainer.json"), &config)?;

    // Create lockfile with entries for all features
    let mut lockfile_features = Vec::new();
    for i in 1..=15 {
        lockfile_features.push(format!(
            r#"    "ghcr.io/devcontainers/features/feature{}": {{
      "version": "1.0.0",
      "resolved": "ghcr.io/devcontainers/features/feature{}@sha256:abc{}",
      "integrity": null
    }}"#,
            i, i, i
        ));
    }
    let lockfile = format!(
        r#"{{
  "features": {{
{}
  }}
}}"#,
        lockfile_features.join(",\n")
    );
    fs::write(devcontainer_dir.join("devcontainer-lock.json"), &lockfile)?;

    let nonexist = td.path().join("nonexistent-ca.pem");

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    cmd.arg("outdated").arg("--workspace-folder").arg(td.path());

    let start = Instant::now();
    let output = cmd.output()?;
    let elapsed = start.elapsed();

    assert!(output.status.success());

    // With lockfile parsing should still be fast
    assert!(
        elapsed.as_secs() <= 10,
        "Command took {} seconds (expected ≤10s)",
        elapsed.as_secs()
    );

    Ok(())
}

#[test]
fn test_outdated_performance_concurrency_env_var() -> Result<(), Box<dyn Error>> {
    // Test with custom concurrency limit
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let mut features = Vec::new();
    for i in 1..=20 {
        features.push(format!(
            r#"    "ghcr.io/devcontainers/features/feature{}:1.0": {{}}"#,
            i
        ));
    }
    let config = format!(
        r#"{{
  "features": {{
{}
  }}
}}"#,
        features.join(",\n")
    );
    fs::write(devcontainer_dir.join("devcontainer.json"), &config)?;

    let nonexist = td.path().join("nonexistent-ca.pem");

    let mut cmd = std::process::Command::cargo_bin("deacon")?;
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        nonexist.to_string_lossy().to_string(),
    );
    // Set custom concurrency limit
    cmd.env("DEACON_OUTDATED_CONCURRENCY", "3");
    cmd.arg("outdated").arg("--workspace-folder").arg(td.path());

    let start = Instant::now();
    let output = cmd.output()?;
    let elapsed = start.elapsed();

    assert!(output.status.success());

    // Even with lower concurrency, should complete reasonably fast with mocked failures
    assert!(
        elapsed.as_secs() <= 10,
        "Command took {} seconds (expected ≤10s)",
        elapsed.as_secs()
    );

    Ok(())
}

#[test]
fn test_outdated_performance_baseline_5_features() -> Result<(), Box<dyn Error>> {
    // Baseline test with smaller set for comparison
    let td = tempdir()?;
    let devcontainer_dir = td.path().join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)?;

    let config = r#"{
  "features": {
    "ghcr.io/devcontainers/features/node:18": {},
    "ghcr.io/devcontainers/features/python:3.11": {},
    "ghcr.io/devcontainers/features/rust:1.70": {},
    "ghcr.io/devcontainers/features/go:1.20": {},
    "ghcr.io/devcontainers/features/java:17": {}
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

    let start = Instant::now();
    let output = cmd.output()?;
    let elapsed = start.elapsed();

    assert!(output.status.success());

    // Smaller set should complete very quickly (well under 10s)
    assert!(
        elapsed.as_secs() <= 5,
        "Command took {} seconds (expected ≤5s for 5 features)",
        elapsed.as_secs()
    );

    Ok(())
}
