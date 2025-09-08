use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_help_output() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Development container CLI (Rust reimplementation)"))
        .stdout(predicate::str::contains("This is a work-in-progress implementation of a DevContainer CLI"));
}

#[test]
fn test_version_output() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("deacon 0.1.0"));
}

#[test]
fn test_default_output() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("DevContainer CLI (WIP) – no commands implemented yet"))
        .stdout(predicate::str::contains("Run with --help to see available options."));
}
