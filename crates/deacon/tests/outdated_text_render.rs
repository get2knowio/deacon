#![cfg(feature = "full")]
use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::error::Error;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_outdated_text_rendering_shows_features_table() -> Result<(), Box<dyn Error>> {
    // Create a temporary workspace with a .devcontainer/devcontainer.json
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

    // Set an env var to force the OCI client creation to fail (makes fetch_latest_stable_version return None)
    let nonexist = td.path().join("nonexistent-ca.pem");

    // Use trait-provided cargo_bin via prelude import
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
            "ghcr.io/devcontainers/features/node | 18 | 18 |",
        ))
        .stdout(predicate::str::contains(
            "ghcr.io/devcontainers/features/python | 3.11 | 3.11 |",
        ));

    Ok(())
}
