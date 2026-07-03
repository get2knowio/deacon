//! Integration tests for build-output rendering (`deacon build`).
//!
//! These run through the streaming build executor (`run_build_once` / the
//! `BuildIo` path). In CI stderr is not a TTY, so the resolved mode is **Plain**:
//! build output is streamed verbatim to stderr. The key guarantees verified here:
//!
//! * a **failing** build surfaces the failing step's output on stderr (it is not
//!   swallowed) and exits non-zero, and
//! * a **successful** build still produces the expected JSON result on stdout.
//!
//! Both tolerate a Docker-less environment (they assert the Docker-unavailable
//! error instead), mirroring `integration_build.rs`.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

/// Whether the failure is just "Docker isn't available here" (so the test's real
/// assertion doesn't apply).
fn is_docker_unavailable(stderr: &str) -> bool {
    let lc = stderr.to_lowercase();
    stderr.contains("Docker is not installed")
        || stderr.contains("Docker daemon is not")
        || lc.contains("permission denied")
        || lc.contains("cannot connect to the docker daemon")
}

fn write_devcontainer(temp_dir: &TempDir, dockerfile: &str) {
    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(temp_dir.path().join(".devcontainer/Dockerfile"), dockerfile).unwrap();
    let config = r#"{
    "name": "Build Output Test",
    "dockerFile": "Dockerfile",
    "build": { "context": "." }
}
"#;
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        config,
    )
    .unwrap();
}

/// A failing `RUN` must surface its output on stderr (Plain mode streams it, and
/// the build error carries the captured stderr) and the command must exit
/// non-zero — i.e. the failure is not silently swallowed.
#[test]
fn build_failure_surfaces_failing_step_output() {
    let temp_dir = TempDir::new().unwrap();
    // A unique marker printed by the failing RUN step. `--no-cache` guarantees the
    // step actually executes (not served from a prior layer cache).
    let marker = "DEACON_BUILD_OUTPUT_FAIL_MARKER";
    let dockerfile =
        format!("FROM alpine:3.19\nLABEL deacon.test=build-output\nRUN echo {marker} && exit 7\n");
    write_devcontainer(&temp_dir, &dockerfile);

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("build")
        .arg("--no-cache")
        .assert();
    let output = assert.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);

    if is_docker_unavailable(&stderr) {
        eprintln!("skipping: docker unavailable ({})", stderr.trim());
        return;
    }

    assert!(
        !output.status.success(),
        "a failing RUN must produce a non-zero exit; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains(marker),
        "the failing step's output must be surfaced on stderr, not swallowed; stderr:\n{stderr}"
    );
}

/// A successful build still emits the JSON result on stdout (stdout stays
/// reserved for the result; build progress goes to stderr).
#[test]
fn build_success_emits_json_result_on_stdout() {
    let temp_dir = TempDir::new().unwrap();
    let dockerfile = "FROM alpine:3.19\nLABEL deacon.test=build-output\nRUN echo DEACON_BUILD_OK\n";
    write_devcontainer(&temp_dir, dockerfile);

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("build")
        .arg("--output-format")
        .arg("json")
        .assert();
    let output = assert.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        assert!(
            is_docker_unavailable(&stderr),
            "unexpected build failure (docker available): {stderr}"
        );
        eprintln!("skipping: docker unavailable ({})", stderr.trim());
        return;
    }

    // stdout carries only the JSON result.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(r#""outcome":"success"#),
        "stdout should carry the success JSON result; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains(r#""imageName""#),
        "stdout JSON should include the built image name; stdout:\n{stdout}"
    );
}
