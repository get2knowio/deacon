//! Integration tests for the doctor command

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_doctor_command_basic() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("doctor");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Deacon Doctor Diagnostics"))
        .stdout(predicate::str::contains("CLI Version:"))
        .stdout(predicate::str::contains("Host OS:"))
        .stdout(predicate::str::contains("Docker:"));
}

#[test]
fn test_doctor_command_json() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("doctor").arg("--json");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"cli_version\""))
        .stdout(predicate::str::contains("\"host_os\""))
        .stdout(predicate::str::contains("\"docker_info\""));
}

#[test]
fn test_doctor_command_bundle_creation() {
    let temp_dir = TempDir::new().unwrap();
    let bundle_path = temp_dir.path().join("test-bundle.zip");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    // Explicitly enable info logging so the support bundle log message is emitted
    cmd.arg("--log-level")
        .arg("info")
        .arg("doctor")
        .arg("--bundle")
        .arg(&bundle_path);

    let assert = cmd.assert().success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.contains("Support bundle created") || stderr.contains("Support bundle created"),
        "Unexpected stdout, failed var.contains(Support bundle created)\n--- stdout ---\n{}\n--- stderr ---\n{}",
        stdout,
        stderr
    );

    // Verify bundle was created
    assert!(bundle_path.exists());

    // Verify it's a valid zip file
    let bundle_content = fs::read(&bundle_path).unwrap();
    assert!(!bundle_content.is_empty());
    assert_eq!(&bundle_content[0..2], b"PK"); // ZIP file signature
}

#[test]
fn test_doctor_command_exits_successfully() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("doctor");

    cmd.assert().success().code(0);
}

#[test]
fn test_doctor_bundle_contains_enhanced_details() {
    let temp_dir = TempDir::new().unwrap();
    let bundle_path = temp_dir.path().join("enhanced-bundle.zip");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("--log-level")
        .arg("info")
        .arg("doctor")
        .arg("--bundle")
        .arg(&bundle_path);

    cmd.assert().success();

    // Verify bundle was created
    assert!(bundle_path.exists());

    // Verify it's a valid zip and contains expected files
    let bundle_content = fs::read(&bundle_path).unwrap();
    assert!(!bundle_content.is_empty());
    assert_eq!(&bundle_content[0..2], b"PK"); // ZIP file signature

    // Use zip crate to verify contents
    let file = fs::File::open(&bundle_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();

    // Check that new files exist in the bundle
    let file_names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_string())
        .collect();

    assert!(
        file_names.contains(&"doctor.json".to_string()),
        "Bundle should contain doctor.json"
    );
    assert!(
        file_names.contains(&"environment.json".to_string()),
        "Bundle should contain environment.json"
    );
    assert!(
        file_names.contains(&"runtime-config.json".to_string()),
        "Bundle should contain runtime-config.json"
    );
    assert!(
        file_names.contains(&"resources.json".to_string()),
        "Bundle should contain resources.json"
    );

    // Verify environment.json contains expected structure
    let mut env_file = archive.by_name("environment.json").unwrap();
    let mut env_content = String::new();
    std::io::Read::read_to_string(&mut env_file, &mut env_content).unwrap();

    // Parse JSON to ensure it's valid
    let env_json: serde_json::Value = serde_json::from_str(&env_content).unwrap();
    assert!(env_json.get("variables").is_some());
    assert!(env_json.get("shell").is_some());
    assert!(env_json.get("home").is_some());
}

/// Stand up a directory holding a fake `docker` whose `info` fails, so the
/// daemon-counters probe is exercised end to end without a real daemon.
///
/// Unix-only: it relies on a shebang script and the executable bit.
#[cfg(unix)]
fn fake_docker_dir(temp_dir: &TempDir) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let bin_dir = temp_dir.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let script = bin_dir.join("docker");
    fs::write(
        &script,
        r#"#!/bin/sh
case "$1" in
  --version) echo "Docker version 99.9.9, build deadbeef"; exit 0 ;;
  version)   echo '{"Client":{"Version":"99.9.9"}}'; exit 0 ;;
  info)      echo "the daemon is wedged" >&2; exit 1 ;;
  *)         exit 1 ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    bin_dir
}

/// PATH with the fake runtime ahead of the real one.
#[cfg(unix)]
fn path_with(bin_dir: &std::path::Path) -> String {
    format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

/// A probe that produces nothing is *stated* in the human report — regression
/// cover for #507, where an unbounded `docker system df` could only ever hang
/// or vanish silently.
#[cfg(unix)]
#[test]
fn test_doctor_text_reports_skipped_probe() {
    let temp_dir = TempDir::new().unwrap();
    let bin_dir = fake_docker_dir(&temp_dir);

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.env("PATH", path_with(&bin_dir)).arg("doctor");

    cmd.assert()
        .success()
        // The fake runtime was the one probed…
        .stdout(predicate::str::contains("99.9.9"))
        // …and its failure is reported, with the cause.
        .stdout(predicate::str::contains("Probe docker_info: skipped"))
        .stdout(predicate::str::contains("the daemon is wedged"));
}

/// The same fact in `--json`: a skip is data, not a silent omission.
#[cfg(unix)]
#[test]
fn test_doctor_json_reports_skipped_probe() {
    let temp_dir = TempDir::new().unwrap();
    let bin_dir = fake_docker_dir(&temp_dir);

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.env("PATH", path_with(&bin_dir))
        .arg("doctor")
        .arg("--json");

    let output = cmd.assert().success().get_output().stdout.clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();

    let skipped = json["docker_info"]["skipped_probes"]
        .as_array()
        .expect("a failing probe must appear in skipped_probes");
    let entry = skipped
        .iter()
        .find(|e| e["probe"] == "docker_info")
        .expect("the info probe must be named");

    assert_eq!(entry["status"], "skipped");
    assert!(
        entry["reason"].as_str().unwrap().contains("exited with"),
        "the reason must say why: {}",
        entry["reason"]
    );
    // Never a fabricated stand-in for the value that could not be read.
    assert!(json["docker_info"]["info_summary"].is_null());
}
