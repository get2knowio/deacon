//! Integration tests for the doctor command

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// A `deacon` invocation with the runtime selection under the test's control.
///
/// `DEACON_CONTAINER_RUNTIME` backs the global `--runtime` flag, and the Podman
/// CI lane exports it job-wide — so a doctor test that says nothing about the
/// runtime is asserting against whichever runtime that lane happens to select.
/// Every test here states its runtime, by flag or by removing the variable.
fn deacon() -> Command {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.env_remove("DEACON_CONTAINER_RUNTIME");
    cmd
}

#[test]
fn test_doctor_command_basic() {
    let mut cmd = deacon();
    cmd.arg("doctor");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Deacon Doctor Diagnostics"))
        .stdout(predicate::str::contains("CLI Version:"))
        .stdout(predicate::str::contains("Host OS:"))
        // The section names the runtime it was probed from (#516).
        .stdout(predicate::str::contains("Container Runtime (docker):"));
}

#[test]
fn test_doctor_command_json() {
    let mut cmd = deacon();
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

    let mut cmd = deacon();
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
    let mut cmd = deacon();
    cmd.arg("doctor");

    cmd.assert().success().code(0);
}

#[test]
fn test_doctor_bundle_contains_enhanced_details() {
    let temp_dir = TempDir::new().unwrap();
    let bundle_path = temp_dir.path().join("enhanced-bundle.zip");

    let mut cmd = deacon();
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

/// Write a fake runtime CLI named `name` into `bin_dir`, reporting `version`
/// and failing `info`, so the probes are exercised end to end without a real
/// daemon. The version string is the marker that says WHICH binary was probed.
///
/// Unix-only: it relies on a shebang script and the executable bit.
#[cfg(unix)]
fn write_fake_runtime(bin_dir: &std::path::Path, name: &str, version: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(bin_dir).unwrap();
    let script = bin_dir.join(name);
    fs::write(
        &script,
        format!(
            r#"#!/bin/sh
case "$1" in
  --version) echo "{name} version {version}, build deadbeef"; exit 0 ;;
  version)   echo '{{"Client":{{"Version":"{version}"}}}}'; exit 0 ;;
  info)      echo "the daemon is wedged" >&2; exit 1 ;;
  *)         exit 1 ;;
esac
"#
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
}

/// Stand up a directory holding a fake `docker` whose `info` fails, so the
/// daemon-counters probe is exercised end to end without a real daemon.
#[cfg(unix)]
fn fake_docker_dir(temp_dir: &TempDir) -> std::path::PathBuf {
    let bin_dir = temp_dir.path().join("bin");
    write_fake_runtime(&bin_dir, "docker", "99.9.9");
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

    let mut cmd = deacon();
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

    let mut cmd = deacon();
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

// ---------------------------------------------------------------------------
// #516 — `doctor` probes the runtime `--runtime` selected.
//
// Two fake runtimes with different version markers sit on PATH, so the reported
// version says unambiguously WHICH binary answered. Hermetic: no daemon, no
// Docker, no Podman needed.
// ---------------------------------------------------------------------------

/// A bin dir holding both fakes, each with its own version marker.
#[cfg(unix)]
fn fake_both_runtimes_dir(temp_dir: &TempDir) -> std::path::PathBuf {
    let bin_dir = temp_dir.path().join("bin");
    write_fake_runtime(&bin_dir, "docker", "11.1.1");
    write_fake_runtime(&bin_dir, "podman", "22.2.2");
    bin_dir
}

/// Run `doctor --json` with the fakes on PATH and return the parsed report.
#[cfg(unix)]
fn doctor_json_with(
    bin_dir: &std::path::Path,
    configure: impl FnOnce(&mut Command),
) -> serde_json::Value {
    let mut cmd = deacon();
    cmd.env("PATH", path_with(bin_dir));
    configure(&mut cmd);
    cmd.arg("doctor").arg("--json");

    let output = cmd.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).expect("doctor --json must emit one JSON document")
}

/// Assert the report was probed from `expected` — the probed binary and the
/// reported runtime name have to be the same runtime, which is the whole of
/// #516.
#[cfg(unix)]
fn assert_probed(json: &serde_json::Value, expected: &str, version_marker: &str) {
    assert_eq!(
        json["docker_info"]["runtime"], expected,
        "the diagnostics block must name the runtime it was probed from"
    );
    assert_eq!(
        json["runtime_config"]["container_runtime"], expected,
        "the reported runtime must be the one that was probed"
    );
    let version = json["docker_info"]["version"]
        .as_str()
        .expect("the fake runtime reports a version");
    assert!(
        version.contains(version_marker),
        "expected {expected}'s version marker {version_marker}, got: {version}"
    );
}

/// No selection: docker, as before.
#[cfg(unix)]
#[test]
fn test_doctor_defaults_to_docker() {
    let temp_dir = TempDir::new().unwrap();
    let bin_dir = fake_both_runtimes_dir(&temp_dir);

    let json = doctor_json_with(&bin_dir, |_| {});
    assert_probed(&json, "docker", "11.1.1");
}

/// `--runtime podman` probes podman. Before #516 this reported
/// `container_runtime: "podman"` beside docker-probed facts.
#[cfg(unix)]
#[test]
fn test_doctor_probes_runtime_selected_by_flag() {
    let temp_dir = TempDir::new().unwrap();
    let bin_dir = fake_both_runtimes_dir(&temp_dir);

    let json = doctor_json_with(&bin_dir, |cmd| {
        cmd.arg("--runtime").arg("podman");
    });
    assert_probed(&json, "podman", "22.2.2");
}

/// The env var backing the flag selects it too — clap resolves it, so doctor
/// needs no read of its own.
#[cfg(unix)]
#[test]
fn test_doctor_probes_runtime_selected_by_env() {
    let temp_dir = TempDir::new().unwrap();
    let bin_dir = fake_both_runtimes_dir(&temp_dir);

    let json = doctor_json_with(&bin_dir, |cmd| {
        cmd.env("DEACON_CONTAINER_RUNTIME", "podman");
    });
    assert_probed(&json, "podman", "22.2.2");
}

/// Flag beats env, exactly as clap's precedence says — a hand-rolled env read
/// below the CLI layer would report podman here.
#[cfg(unix)]
#[test]
fn test_doctor_runtime_flag_beats_env() {
    let temp_dir = TempDir::new().unwrap();
    let bin_dir = fake_both_runtimes_dir(&temp_dir);

    let json = doctor_json_with(&bin_dir, |cmd| {
        cmd.env("DEACON_CONTAINER_RUNTIME", "podman")
            .arg("--runtime")
            .arg("docker");
    });
    assert_probed(&json, "docker", "11.1.1");
}

/// The text report names the probed runtime in its heading, so a human reading
/// it cannot mistake podman diagnostics for docker ones.
#[cfg(unix)]
#[test]
fn test_doctor_text_names_the_probed_runtime() {
    let temp_dir = TempDir::new().unwrap();
    let bin_dir = fake_both_runtimes_dir(&temp_dir);

    let mut cmd = deacon();
    cmd.env("PATH", path_with(&bin_dir))
        .arg("--runtime")
        .arg("podman")
        .arg("doctor");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Container Runtime (podman):"))
        .stdout(predicate::str::contains("22.2.2"))
        .stdout(predicate::str::contains("11.1.1").not());
}

/// A selected runtime that is not installed is reported absent — never
/// backfilled from the other runtime that happens to be on PATH. PATH holds
/// ONLY the docker fake, so a regression would show docker's version here.
#[cfg(unix)]
#[test]
fn test_doctor_reports_selected_runtime_absent() {
    let temp_dir = TempDir::new().unwrap();
    let bin_dir = temp_dir.path().join("bin");
    write_fake_runtime(&bin_dir, "docker", "11.1.1");

    let mut cmd = deacon();
    // Not `path_with`: the real PATH may carry a real podman.
    cmd.env("PATH", bin_dir.display().to_string())
        .arg("--runtime")
        .arg("podman")
        .arg("doctor")
        .arg("--json");

    let output = cmd.assert().success().get_output().stdout.clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();

    assert_eq!(json["docker_info"]["runtime"], "podman");
    assert_eq!(json["runtime_config"]["container_runtime"], "podman");
    assert_eq!(json["docker_info"]["installed"], false);
    assert_eq!(json["docker_info"]["daemon_running"], false);
    assert!(
        json["docker_info"]["version"].is_null(),
        "an absent podman must not be reported with another runtime's version: {}",
        json["docker_info"]["version"]
    );
}
