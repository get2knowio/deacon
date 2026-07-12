//! Docker-gated build-time host-CA injection tests (016, US3).
//!
//! Cover: a feature-extended image built with injection contains the corporate
//! cert (verified by `docker run <tag> cat`, not just the JSON outcome), and
//! shapes that generate no feature-layering Dockerfile (image-only) skip
//! build-time injection with a clear log line (FR-018a).
//!
//! Uses an explicit PEM bundle so it doesn't depend on the host trust store.
//! Cleans up produced images. Skips cleanly when Docker is unavailable.

use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use tempfile::TempDir;

const CORPORATE_CA_PEM: &str =
    include_str!("../../core/src/host_ca/test_fixtures/corporate_ca.pem");

const BUNDLE_PATH: &str = "/usr/local/share/deacon/host-ca.crt";

fn is_docker_available() -> bool {
    StdCommand::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn deacon() -> StdCommand {
    StdCommand::new(env!("CARGO_BIN_EXE_deacon"))
}

fn write(ws: &Path, rel: &str, body: &str) {
    let path = ws.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn write_ca_fixture(ws: &Path) -> String {
    let p = ws.join("corp-ca.pem");
    std::fs::write(&p, CORPORATE_CA_PEM).unwrap();
    p.display().to_string()
}

fn rmi(tag: &str) {
    let _ = StdCommand::new("docker")
        .args(["rmi", "-f", tag])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// T036: a feature-extended image built with `--inject-host-ca` contains the
/// corporate cert. Verified by running the produced image and reading the file.
#[test]
fn feature_extended_image_contains_cert() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let ws = TempDir::new().unwrap();
    let tag = "deacon-hostca-build-test:latest";
    write(
        ws.path(),
        ".devcontainer/devcontainer.json",
        r#"{ "name": "hostca-build", "image": "debian:bookworm-slim",
              "features": { "./features/dummy": {} } }"#,
    );
    write(
        ws.path(),
        ".devcontainer/features/dummy/devcontainer-feature.json",
        r#"{ "id": "dummy", "version": "1.0.0", "name": "dummy" }"#,
    );
    write(
        ws.path(),
        ".devcontainer/features/dummy/install.sh",
        "#!/usr/bin/env bash\nset -e\necho dummy-installed > /dummy-marker\n",
    );
    let pem = write_ca_fixture(ws.path());

    let status = deacon()
        .args(["build", "--workspace-folder"])
        .arg(ws.path())
        .args(["--inject-host-ca", &pem, "--image-name", tag])
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .expect("spawn deacon build");
    assert!(
        status.success(),
        "deacon build --inject-host-ca should succeed"
    );

    // Verify against the produced image contents (not just the JSON outcome).
    let out = StdCommand::new("docker")
        .args(["run", "--rm", tag, "head", "-1", BUNDLE_PATH])
        .output()
        .expect("docker run produced image");
    rmi(tag);

    assert!(
        out.status.success(),
        "produced image must contain the injected bundle at {BUNDLE_PATH}"
    );
    let head = String::from_utf8_lossy(&out.stdout);
    assert!(
        head.trim().starts_with("-----BEGIN CERTIFICATE-----"),
        "bundle must be a PEM certificate, got: {head:?}"
    );
}

/// T035: an image-only config (no features) generates no feature-layering
/// Dockerfile, so build-time injection is SKIPPED with a clear log line
/// (FR-018a) and the build still succeeds.
#[test]
fn image_only_skips_build_injection_with_log() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let ws = TempDir::new().unwrap();
    let tag = "deacon-hostca-skip-test:latest";
    write(
        ws.path(),
        ".devcontainer/devcontainer.json",
        r#"{ "name": "hostca-skip", "image": "debian:bookworm-slim" }"#,
    );
    let pem = write_ca_fixture(ws.path());

    let out = deacon()
        // The FR-018a skip notice is an `info!`; enable info logging so it lands
        // on stderr (the default level is WARN and would filter it out).
        .env("RUST_LOG", "deacon=info")
        .args(["build", "--workspace-folder"])
        .arg(ws.path())
        .args(["--inject-host-ca", &pem, "--image-name", tag])
        .stdout(Stdio::null())
        .output()
        .expect("spawn deacon build");
    rmi(tag);

    assert!(out.status.success(), "image-only build should succeed");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Build-time host-CA injection skipped"),
        "expected the FR-018a skip log line; stderr was:\n{stderr}"
    );
}
