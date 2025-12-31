#![cfg(feature = "full")]
use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

// This integration test verifies that when the core features_test runner
// returns RuntimeUnavailable, the CLI exits with code 3 and prints a message.
#[test]
fn cli_exits_with_3_when_runtime_unavailable() {
    // Create a minimal project directory structure expected by the CLI
    let temp_dir = TempDir::new().unwrap();
    let project_dir = temp_dir.path();

    // Create src feature with metadata and install.sh
    let src_dir = project_dir.join("src").join("test-feature");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(
        src_dir.join("devcontainer-feature.json"),
        r#"{"id": "test-feature", "version": "1.0.0", "name": "Test Feature"}"#,
    )
    .unwrap();
    fs::write(
        src_dir.join("install.sh"),
        "#!/bin/sh\necho install\nexit 0",
    )
    .unwrap();

    // Create test directory with test.sh
    let test_dir = project_dir.join("test").join("test-feature");
    fs::create_dir_all(&test_dir).unwrap();
    fs::write(test_dir.join("test.sh"), "#!/bin/sh\necho test\nexit 0").unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("features")
        .arg("test")
        .arg(project_dir.to_str().unwrap())
        .env("DEACON_FORCE_RUNTIME_UNAVAILABLE", "1");

    cmd.assert()
        .failure()
        .code(3)
        .stderr(predicate::str::contains("Runtime unavailable"));
}
