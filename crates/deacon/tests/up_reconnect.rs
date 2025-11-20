//! Integration tests for expect-existing/id-label fast-fail and remote-env/secrets redaction
//!
//! Tests User Story 3 requirements:
//! - Expect-existing container fast-fail with id-labels
//! - Remote environment variable redaction in logs
//! - Secrets file content redaction in logs
//! - Error before any create/build operations when expect-existing fails
//!
//! Related spec: specs/001-up-gap-spec/contracts/up.md
//! Task: T019 [P] [US3]

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use std::fs;
use tempfile::TempDir;

/// Test that expect-existing fails fast when container with id-labels not found
#[test]
#[ignore] // TODO: Enable when T023 is implemented
fn test_expect_existing_fails_fast_with_id_label() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Create minimal devcontainer setup
    let devcontainer_json = json!({
        "name": "Expect Existing Test",
        "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
        "workspaceFolder": "/workspace"
    });

    fs::write(
        workspace.join("devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_json).unwrap(),
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--expect-existing-container")
        .arg("--id-label")
        .arg("project=test-nonexistent")
        .arg("--id-label")
        .arg("env=testing");

    // Should fail immediately with error JSON before any docker operations
    cmd.assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("outcome"))
        .stdout(predicate::str::contains("error"));

    // TODO: Parse JSON to verify error message mentions container not found
}

/// Test that remote-env values are redacted in stderr logs
#[test]
#[ignore] // TODO: Enable when T021 is implemented with redaction
fn test_remote_env_redaction_in_logs() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    let devcontainer_json = json!({
        "name": "Remote Env Redaction Test",
        "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
        "workspaceFolder": "/workspace"
    });

    fs::write(
        workspace.join("devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_json).unwrap(),
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--remote-env")
        .arg("API_KEY=super-secret-key-12345")
        .arg("--remote-env")
        .arg("DATABASE_PASSWORD=MyP@ssw0rd!")
        .env("RUST_LOG", "debug");

    // Should succeed (or fail for other reasons), but must not leak secrets in stderr
    let output = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Secrets should be redacted in logs
    assert!(
        !stderr.contains("super-secret-key-12345"),
        "API_KEY value should not appear in logs"
    );
    assert!(
        !stderr.contains("MyP@ssw0rd!"),
        "DATABASE_PASSWORD value should not appear in logs"
    );

    // Redacted markers should be present
    assert!(
        stderr.contains("***") || stderr.contains("REDACTED"),
        "Logs should contain redaction markers"
    );
}

/// Test that secrets file contents are redacted in logs
#[test]
#[ignore] // TODO: Enable when T021 is implemented with secrets file support
fn test_secrets_file_redaction() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    let devcontainer_json = json!({
        "name": "Secrets File Test",
        "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
        "workspaceFolder": "/workspace"
    });

    let secrets_file = r#"
GITHUB_TOKEN=ghp_1234567890abcdefghijklmnop
AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE
AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
"#;

    fs::write(
        workspace.join("devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_json).unwrap(),
    )
    .unwrap();

    let secrets_path = workspace.join("secrets.env");
    fs::write(&secrets_path, secrets_file).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--secrets-file")
        .arg(&secrets_path)
        .env("RUST_LOG", "debug");

    let output = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    // All secret values should be redacted
    assert!(
        !stderr.contains("ghp_1234567890abcdefghijklmnop"),
        "GitHub token should not appear in logs"
    );
    assert!(
        !stderr.contains("AKIAIOSFODNN7EXAMPLE"),
        "AWS access key should not appear in logs"
    );
    assert!(
        !stderr.contains("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"),
        "AWS secret key should not appear in logs"
    );

    // But environment variable names can appear
    assert!(
        stderr.contains("GITHUB_TOKEN") || stderr.contains("environment"),
        "Variable names can appear (not their values)"
    );
}

/// Test that expect-existing works properly with compose and id-labels
#[test]
#[ignore] // TODO: Enable when T023 is implemented for compose flows
fn test_expect_existing_compose_with_id_labels() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    let devcontainer_json = json!({
        "name": "Compose Expect Existing Test",
        "dockerComposeFile": "docker-compose.yml",
        "service": "app",
        "workspaceFolder": "/workspace"
    });

    let compose_yml = r#"
version: '3.8'
services:
  app:
    image: mcr.microsoft.com/devcontainers/base:ubuntu
    volumes:
      - ../..:/workspace:cached
    command: sleep infinity
"#;

    fs::write(
        workspace.join("devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_json).unwrap(),
    )
    .unwrap();
    fs::write(workspace.join("docker-compose.yml"), compose_yml).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--expect-existing-container")
        .arg("--id-label")
        .arg("project=compose-test");

    // Should fail fast before compose up
    cmd.assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("error"));
}

/// Test that multiple secrets files are merged and all values redacted
#[test]
#[ignore] // TODO: Enable when T021 supports multiple secrets files
fn test_multiple_secrets_files_merge_and_redaction() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    let devcontainer_json = json!({
        "name": "Multiple Secrets Test",
        "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
        "workspaceFolder": "/workspace"
    });

    let secrets_file1 = "DB_USER=admin\nDB_PASSWORD=secret123\n";
    let secrets_file2 = "API_TOKEN=token456\nENCRYPTION_KEY=key789\n";

    fs::write(
        workspace.join("devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_json).unwrap(),
    )
    .unwrap();

    let secrets_path1 = workspace.join("secrets1.env");
    let secrets_path2 = workspace.join("secrets2.env");
    fs::write(&secrets_path1, secrets_file1).unwrap();
    fs::write(&secrets_path2, secrets_file2).unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--secrets-file")
        .arg(&secrets_path1)
        .arg("--secrets-file")
        .arg(&secrets_path2)
        .env("RUST_LOG", "debug");

    let output = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    // All secret values from both files should be redacted
    assert!(
        !stderr.contains("secret123"),
        "DB_PASSWORD should be redacted"
    );
    assert!(!stderr.contains("token456"), "API_TOKEN should be redacted");
    assert!(
        !stderr.contains("key789"),
        "ENCRYPTION_KEY should be redacted"
    );
}

/// Test that redaction works in JSON output (should never contain secret values)
#[test]
#[ignore] // TODO: Enable when T021 is implemented
fn test_secrets_never_in_json_output() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    let devcontainer_json = json!({
        "name": "Secrets JSON Test",
        "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
        "workspaceFolder": "/workspace"
    });

    fs::write(
        workspace.join("devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_json).unwrap(),
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--remote-env")
        .arg("SECRET_VALUE=this-must-not-appear")
        .arg("--include-merged-configuration");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Secret values should NEVER appear in JSON output
    assert!(
        !stdout.contains("this-must-not-appear"),
        "Secret values must not appear in JSON output"
    );

    // JSON output should still be valid
    if output.status.success() {
        let json: serde_json::Value =
            serde_json::from_str(&stdout).expect("stdout should contain valid JSON");
        assert_eq!(json["outcome"], "success");
    }
}

/// Test expect-existing with remove-existing should error (conflicting flags)
#[test]
#[ignore] // TODO: Enable when T023 validates flag conflicts
fn test_expect_existing_with_remove_existing_conflict() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    let devcontainer_json = json!({
        "name": "Conflicting Flags Test",
        "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
        "workspaceFolder": "/workspace"
    });

    fs::write(
        workspace.join("devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_json).unwrap(),
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--expect-existing-container")
        .arg("--remove-existing-container");

    // Should fail validation (these flags are mutually exclusive)
    cmd.assert().failure().code(1);
}

/// Test that id-label discovery works when expect-existing is used
#[test]
#[ignore] // TODO: Enable when T023 and T029 are complete
fn test_expect_existing_with_id_label_discovery() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Config with id-label properties
    let devcontainer_json = json!({
        "name": "ID Label Discovery Test",
        "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
        "workspaceFolder": "/workspace",
        "customizations": {
            "devcontainer": {
                "idLabel": {
                    "app": "myapp",
                    "version": "1.0"
                }
            }
        }
    });

    fs::write(
        workspace.join("devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer_json).unwrap(),
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--expect-existing-container");

    // Should use id-labels from config to find container
    // If not found, should error immediately
    cmd.assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("error"));
}
