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

/// #470: two concurrent builds of byte-identical content must both succeed.
///
/// `calculate_config_hash` is content-derived and deliberately workspace-agnostic
/// (Dockerfile bytes plus each context file's relative path, size and
/// mtime-in-seconds), so two such builds name the SAME deterministic
/// `deacon-build:<hash>` tag. Each one's image nevertheless carries a distinct
/// digest — BuildKit emits a per-build attestation manifest — so whichever build
/// names the shared tag last leaves the other's image unreferenced, and the
/// containerd image store drops it. deacon used to resolve its own just-built
/// image through that raw digest, so the loser died with
/// `docker inspect: no such object` AFTER BuildKit had reported success.
///
/// The `--output` side is the one that breaks, because it passes no
/// `--image-name`: its sibling's user tag keeps the sibling's image referenced,
/// while the shared tag is the export side's ONLY reference. That is exactly the
/// parity case `case-build-output-export-tar`, which flaked twice on CI.
///
/// The fixtures' mtimes are pinned to one instant so the hash collision — and
/// hence the race — is guaranteed rather than merely likely.
#[test]
fn concurrent_identical_builds_do_not_race_on_the_shared_deterministic_tag() {
    use std::time::{Duration, SystemTime};

    let dockerfile = "FROM alpine:3.19\nLABEL deacon.test=build-output-470\n";
    // A fixed, shared mtime: the context-file component of the config hash is
    // (relative path, size, mtime-in-seconds), so pinning it makes both
    // workspaces hash identically no matter how far apart they are written.
    let pinned = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);

    let make_workspace = || {
        let dir = TempDir::new().unwrap();
        write_devcontainer(&dir, dockerfile);
        for name in [
            ".devcontainer/Dockerfile",
            ".devcontainer/devcontainer.json",
        ] {
            let f = fs::File::options()
                .write(true)
                .open(dir.path().join(name))
                .unwrap();
            f.set_modified(pinned).unwrap();
        }
        dir
    };

    let exporting = make_workspace();
    let plain = make_workspace();
    let tar_path = exporting.path().join("export.tar");
    let export_spec = format!("type=docker,dest={}", tar_path.display());
    let bin = assert_cmd::cargo::cargo_bin("deacon");

    // The victim shape: `--output`, no `--image-name`.
    let export_dir = exporting.path().to_path_buf();
    let export_bin = bin.clone();
    let exporter = std::thread::spawn(move || {
        let child = std::process::Command::new(export_bin)
            .current_dir(&export_dir)
            .args(["build", "--output", &export_spec, "--output-format", "json"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        // The run-private tag is namespaced by the deacon process's pid, so
        // capturing it is what lets the leak check below scope itself to the
        // resources THIS test created rather than scanning daemon-global state.
        let pid = child.id();
        (pid, child.wait_with_output().unwrap())
    });

    // The sibling that re-points the shared tag out from under it.
    let plain_dir = plain.path().to_path_buf();
    let sibling = std::thread::spawn(move || {
        let child = std::process::Command::new(bin)
            .current_dir(&plain_dir)
            .args([
                "build",
                "--image-name",
                "deacon-test-470-sibling:latest",
                "--output-format",
                "json",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        (pid, child.wait_with_output().unwrap())
    });

    let (export_pid, export_out) = exporter.join().unwrap();
    let (sibling_pid, sibling_out) = sibling.join().unwrap();
    let export_err = String::from_utf8_lossy(&export_out.stderr);
    let sibling_err = String::from_utf8_lossy(&sibling_out.stderr);

    let _ = std::process::Command::new("docker")
        .args(["image", "rm", "deacon-test-470-sibling:latest"])
        .output();

    if !export_out.status.success() && is_docker_unavailable(&export_err) {
        eprintln!("skipping: docker unavailable ({})", export_err.trim());
        return;
    }

    assert!(
        sibling_out.status.success(),
        "the plain concurrent build should succeed; stderr:\n{sibling_err}"
    );
    assert!(
        export_out.status.success(),
        "the --output build lost the race for the shared deterministic tag (#470); stderr:\n{export_err}"
    );
    assert!(
        tar_path.exists(),
        "the --output build reported success but wrote no tar at {}",
        tar_path.display()
    );

    // The run-private tag is bookkeeping: it must not survive the build. Scoped
    // to the two pids this test spawned — a bare `deacon-build-run` substring
    // match would scan daemon-global state and report a sibling's in-flight tag
    // as this test's leak.
    let images = std::process::Command::new("docker")
        .args(["images", "--format", "{{.Repository}}:{{.Tag}}"])
        .output()
        .unwrap();
    let images = String::from_utf8_lossy(&images.stdout);
    for pid in [export_pid, sibling_pid] {
        let ours = format!("deacon-build-run:{pid}-");
        assert!(
            !images.contains(&ours),
            "the run-private build tag `{ours}*` leaked into the local image list"
        );
    }
}
