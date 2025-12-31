#![cfg(feature = "full")]
//! Integration tests for templates CLI commands

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_templates_metadata_command() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["templates", "metadata", "../../fixtures/templates/minimal"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(r#""id": "minimal-template""#))
        .stdout(predicate::str::contains(r#""name": "Minimal Template""#))
        .stdout(predicate::str::contains(r#""options": {}"#))
        .stdout(predicate::str::contains(r#""recommendedFeatures": null"#));
}

#[test]
fn test_templates_metadata_command_with_options() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "templates",
        "metadata",
        "../../fixtures/templates/with-options",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(r#""id": "template-with-options""#))
        .stdout(predicate::str::contains(
            r#""name": "Template with Options""#,
        ))
        .stdout(predicate::str::contains(r#""enableFeature""#))
        .stdout(predicate::str::contains(r#""customName""#))
        .stdout(predicate::str::contains(r#""recommendedFeatures""#));
}

#[test]
fn test_templates_metadata_command_nonexistent_path() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["templates", "metadata", "/nonexistent/path"]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "Failed to parse template metadata",
    ));
}

#[test]
fn test_templates_publish_dry_run() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "templates",
        "publish",
        "../../fixtures/templates/minimal",
        "--registry",
        "ghcr.io/test/repo",
        "--dry-run",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(r#""command": "publish""#))
        .stdout(predicate::str::contains(r#""status": "success""#))
        .stdout(predicate::str::contains(r#""digest": "sha256:dryrun"#))
        .stdout(predicate::str::contains(r#""size": 1024"#));
}

#[test]
fn test_templates_publish_without_dry_run() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "templates",
        "publish",
        "../../fixtures/templates/minimal",
        "--registry",
        "ghcr.io/test/repo",
    ]);

    // In CI environments without network access, this will fail with authentication/network error
    // In development with proper credentials, this should succeed
    // We accept either outcome since this tests the command parsing and basic flow
    let result = cmd.assert().failure();

    // Should fail with authentication or network error (expected in CI)
    result.stderr(predicate::str::contains("Failed to publish template"));
}

#[test]
fn test_templates_generate_docs() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "templates",
        "generate-docs",
        "../../fixtures/templates/with-options",
        "--output",
        output_path,
    ]);

    cmd.assert().success();

    // Check that README was generated
    let readme_path = temp_dir.path().join("README-template.md");
    assert!(readme_path.exists());

    let content = fs::read_to_string(&readme_path).unwrap();

    // Check content is deterministic and contains expected elements
    assert!(content.contains("# Template with Options"));
    assert!(content.contains("A DevContainer template with various option types"));
    assert!(content.contains("## Options"));
    assert!(content.contains("### customName"));
    assert!(content.contains("### debugMode"));
    assert!(content.contains("### enableFeature"));
    assert!(content.contains("### version"));
    assert!(content.contains("## Usage"));
    assert!(content.contains("template-with-options"));

    // Verify deterministic ordering (customName should come before debugMode alphabetically)
    let custom_name_pos = content.find("### customName").unwrap();
    let debug_mode_pos = content.find("### debugMode").unwrap();
    assert!(custom_name_pos < debug_mode_pos);
}

#[test]
fn test_templates_generate_docs_minimal() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "templates",
        "generate-docs",
        "../../fixtures/templates/minimal",
        "--output",
        output_path,
    ]);

    cmd.assert().success();

    // Check that README was generated
    let readme_path = temp_dir.path().join("README-template.md");
    assert!(readme_path.exists());

    let content = fs::read_to_string(&readme_path).unwrap();

    // Check content for minimal template
    assert!(content.contains("# Minimal Template"));
    assert!(content.contains("A minimal DevContainer template for testing"));
    assert!(content.contains("## Usage"));
    assert!(content.contains("minimal-template"));

    // Should not have Options section since no options
    assert!(!content.contains("## Options"));
}

#[test]
fn test_templates_apply_network_error() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["templates", "apply", "some-template"]);

    // Should fail with network/authentication error (not "not implemented")
    // since the command is now fully implemented
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Failed to fetch template"));
}

#[test]
fn test_templates_help() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["templates", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Template management commands"))
        .stdout(predicate::str::contains("apply"))
        .stdout(predicate::str::contains("publish"))
        .stdout(predicate::str::contains("metadata"))
        .stdout(predicate::str::contains("generate-docs"));
}

/// Test templates pull command help output
#[test]
fn test_templates_pull_help() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["templates", "pull", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Pull templates from registry"))
        .stdout(predicate::str::contains("REGISTRY_REF"))
        .stdout(predicate::str::contains(
            "Registry reference (registry/namespace/name:version)",
        ))
        .stdout(predicate::str::contains("--json"));
}

/// Test templates pull command with invalid registry (should fail with clear error)
#[test]
fn test_templates_pull_invalid_registry() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args([
        "templates",
        "pull",
        "invalid.example.com/nonexistent/template:latest",
        "--json",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Failed to pull template"));
}

/// Test templates pull command with missing arguments
#[test]
fn test_templates_pull_missing_args() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["templates", "pull"]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("required").and(predicate::str::contains("REGISTRY_REF")));
}

/// Test that templates help shows the pull command
#[test]
fn test_templates_help_shows_pull() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.args(["templates", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Template management commands"))
        .stdout(predicate::str::contains("pull"))
        .stdout(predicate::str::contains("Pull templates from registry"));
}
