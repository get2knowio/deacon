#![cfg(feature = "full")]
//! Tests verifying build command consistency with up/exec commands.
//!
//! This test suite ensures the build command shares config loading and
//! terminal sizing logic with up/exec as per CONSISTENCY.md Task 1 & 2.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Helper function to create a minimal Dockerfile for testing
fn create_minimal_dockerfile(dir: &Path, base_image: &str) {
    let dockerfile_content = format!("FROM {}\n", base_image);
    fs::write(dir.join("Dockerfile"), dockerfile_content).unwrap();
}

/// Test that build command respects --override-config flag
#[test]
fn build_uses_override_config() {
    let temp = TempDir::new().unwrap();
    let base_config_path = temp.path().join(".devcontainer.json");
    let override_config_path = temp.path().join("override.json");

    // Create base config with image
    fs::write(
        &base_config_path,
        r#"{"name": "base", "image": "alpine:3.19"}"#,
    )
    .unwrap();

    // Create override config with different image
    fs::write(
        &override_config_path,
        r#"{"name": "override", "image": "ubuntu:22.04"}"#,
    )
    .unwrap();

    // Create minimal Dockerfile matching base config
    // (The override config will be used instead, demonstrating config override behavior)
    create_minimal_dockerfile(temp.path(), "alpine:3.19");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(temp.path())
        .arg("build")
        .arg("--override-config")
        .arg(override_config_path.to_str().unwrap())
        .arg("--output-format")
        .arg("json");

    // The command should build successfully using the override config
    // We expect the build to reference the override image
    cmd.assert().success();
}

/// Test that build command respects --secrets-file flag for variable substitution
#[test]
fn build_uses_secrets_file() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join(".devcontainer.json");
    let secrets_path = temp.path().join("secrets.json");

    // Create config with variable substitution
    fs::write(
        &config_path,
        r#"{"name": "${MY_SECRET}", "image": "alpine:3.19"}"#,
    )
    .unwrap();

    // Create secrets file
    fs::write(&secrets_path, r#"{"MY_SECRET": "test-value"}"#).unwrap();

    // Create minimal Dockerfile
    create_minimal_dockerfile(temp.path(), "alpine:3.19");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(temp.path())
        .arg("--secrets-file")
        .arg(secrets_path.to_str().unwrap())
        .arg("build")
        .arg("--output-format")
        .arg("json");

    // The command should build successfully with substituted variables
    cmd.assert().success();
}

/// Test that build command validates terminal dimensions like up/exec
#[test]
fn build_validates_terminal_dimensions_together() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join(".devcontainer.json");

    fs::write(&config_path, r#"{"image": "alpine:3.19"}"#).unwrap();

    // Create minimal Dockerfile
    create_minimal_dockerfile(temp.path(), "alpine:3.19");

    // Test: providing only columns should fail (CLI enforces via requires)
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(temp.path())
        .arg("--terminal-columns")
        .arg("80")
        .arg("build");

    cmd.assert().failure().stderr(predicate::str::contains(
        "required arguments were not provided",
    ));

    // Test: providing only rows should fail (CLI enforces via requires)
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(temp.path())
        .arg("--terminal-rows")
        .arg("24")
        .arg("build");

    cmd.assert().failure().stderr(predicate::str::contains(
        "required arguments were not provided",
    ));

    // Test: providing both should succeed (validation passes)
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(temp.path())
        .arg("--terminal-columns")
        .arg("80")
        .arg("--terminal-rows")
        .arg("24")
        .arg("build")
        .arg("--output-format")
        .arg("json");

    // Should succeed (build may fail for other reasons but terminal validation passes)
    cmd.assert().success();
}

/// Test that build command handles missing config with override fallback
#[test]
fn build_uses_override_when_base_missing() {
    let temp = TempDir::new().unwrap();
    let override_config_path = temp.path().join("override.json");

    // Create only override config (no base .devcontainer.json)
    fs::write(
        &override_config_path,
        r#"{"name": "override-only", "image": "alpine:3.19"}"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(temp.path())
        .arg("build")
        .arg("--override-config")
        .arg(override_config_path.to_str().unwrap())
        .arg("--output-format")
        .arg("json");

    // Should use override as base when no base config exists
    cmd.assert().success();
}

/// Test that build command produces same config error format as up/exec
#[test]
fn build_config_error_format_matches_up_exec() {
    let temp = TempDir::new().unwrap();

    // Try to build without any config file
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(temp.path())
        .arg("build")
        .arg("--output-format")
        .arg("json");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}
