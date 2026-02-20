#![cfg(feature = "full")]
//! Integration tests for templates CLI commands (consumer: pull, apply)

use assert_cmd::Command;
use predicates::prelude::*;

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
