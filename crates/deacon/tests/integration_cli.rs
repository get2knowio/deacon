#![cfg(feature = "full")]
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_help_output() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Development container CLI"))
        .stdout(predicate::str::contains(
            "Implements the Development Containers specification",
        ))
        .stdout(predicate::str::contains("up"))
        .stdout(predicate::str::contains("build"))
        .stdout(predicate::str::contains("exec"))
        .stdout(predicate::str::contains("read-configuration"))
        .stdout(predicate::str::contains("features"))
        .stdout(predicate::str::contains("templates"))
        .stdout(predicate::str::contains("run-user-commands"));
}

#[test]
fn test_version_output() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        // Match current package version dynamically
        .stdout(predicate::str::contains(format!(
            "deacon {}",
            env!("CARGO_PKG_VERSION")
        )));
}

#[test]
fn test_default_output() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Development container CLI"))
        .stdout(predicate::str::contains(
            "Run 'deacon --help' to see available commands.",
        ));
}

#[test]
fn test_read_configuration_invalid_id_label_format() {
    // Test that read-configuration validates id-label format at CLI level
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("read-configuration")
        .arg("--id-label")
        .arg("invalid") // Missing '=' sign
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "Unmatched argument format: id-label must match <name>=<value>.",
        ));
}

#[test]
fn test_subcommand_not_implemented() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("--workspace-folder")
        .arg("/tmp/nonexistent")
        .arg("up")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "No devcontainer.json found in workspace",
        ));

    // Build command is now implemented and should try to find config
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("build")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("Configuration file not found"));

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("exec")
        .arg("echo")
        .arg("test")
        .assert()
        .failure()
        .code(1)
        // Now properly checks for configuration file first before attempting container operations
        .stderr(predicate::str::contains("Dev container config"))
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_global_logging_options() {
    // Test that debug logging flag is accepted (no errors)
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("--log-level")
        .arg("debug")
        .assert()
        .success()
        .stdout(predicate::str::contains("Development container CLI"));

    // Test json log format flag
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("--log-format")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("Development container CLI"));
}

#[test]
fn test_workspace_and_config_options() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("--workspace-folder")
        .arg("/tmp")
        .arg("--config")
        .arg("/tmp/deacon.json")
        .assert()
        .success()
        .stdout(predicate::str::contains("Development container CLI"));
}

#[test]
fn test_debug_logging_with_subcommand() {
    // Test that debug logging initialization works when a command fails
    // The debug message should be visible in stderr when RUST_LOG is set appropriately
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.env("RUST_LOG", "debug")
        .arg("--log-level")
        .arg("debug")
        .arg("--workspace-folder")
        .arg("/tmp/nonexistent")
        .arg("up")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "No devcontainer.json found in workspace",
        ));

    // Note: The actual debug log "CLI initialized with log level: debug"
    // should appear in stderr when running with RUST_LOG=debug, but it's hard
    // to test reliably due to logging initialization timing.
}
