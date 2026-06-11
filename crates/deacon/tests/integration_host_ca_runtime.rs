//! Docker-gated runtime host-CA injection tests (016, US2 + US4 adversarial).
//!
//! Cover: cert lands in the distro system store (debian/RHEL/alpine matrix),
//! injection happens BEFORE the first lifecycle hook, env-var-only fallback on
//! unsupported distro / non-root (no `up` abort), the six CA env vars are
//! visible to `exec`, and the FR-015 trust boundary (a workspace config cannot
//! enable injection).
//!
//! All tests use an explicit PEM bundle (`--inject-host-ca <fixture.pem>`) so
//! they don't depend on the host machine's trust store. Each uses a `TempDir`
//! workspace (in-repo `up` chowns the workspace — see CLAUDE.md) and cleans up
//! its container via `down`. They skip cleanly when Docker is unavailable.

use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use tempfile::TempDir;

/// A valid corporate CA PEM (shared with the core host_ca discovery fixtures).
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

/// Write the fixture CA into the workspace and return its absolute path.
fn write_ca_fixture(ws: &Path) -> String {
    let p = ws.join("corp-ca.pem");
    std::fs::write(&p, CORPORATE_CA_PEM).unwrap();
    p.display().to_string()
}

/// Run `up --inject-host-ca <pem> --workspace-folder ws`. Returns whether it
/// succeeded (some tests assert success; the fallback tests assert no abort).
fn up_with_ca(ws: &Path, pem: &str) -> bool {
    deacon()
        .args(["up", "--workspace-folder"])
        .arg(ws)
        .args(["--inject-host-ca", pem])
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .expect("spawn deacon up")
        .success()
}

/// Run `up --workspace-folder ws` with NO injection flag.
fn up_plain(ws: &Path) -> bool {
    deacon()
        .args(["up", "--workspace-folder"])
        .arg(ws)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .expect("spawn deacon up")
        .success()
}

/// `exec --workspace-folder ws -- sh -c '<cmd>'`; returns (success, trimmed stdout).
fn exec_sh(ws: &Path, cmd: &str) -> (bool, String) {
    let out = deacon()
        .args(["exec", "--workspace-folder"])
        .arg(ws)
        .args(["sh", "-c", cmd])
        .stderr(Stdio::inherit())
        .output()
        .expect("spawn deacon exec");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
    )
}

fn down(ws: &Path) {
    let _ = deacon()
        .args(["down", "--workspace-folder"])
        .arg(ws)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn image_config(name: &str, image: &str) -> String {
    format!(r#"{{ "name": "{name}", "image": "{image}", "overrideCommand": true }}"#)
}

/// Write a `build.dockerfile` config whose image is guaranteed to ship the
/// distro trust-store updater. Minimal base images (`debian:*-slim`,
/// `alpine`, …) omit `ca-certificates`, in which case deacon *correctly* falls
/// back to env-var-only — but the SystemStore path we want to exercise here
/// needs the updater present, which is the realistic dev-image scenario.
fn dockerfile_config(ws: &Path, name: &str, dockerfile: &str) {
    write(
        ws,
        ".devcontainer/devcontainer.json",
        &format!(
            r#"{{ "name": "{name}", "build": {{ "dockerfile": "Dockerfile" }}, "overrideCommand": true }}"#
        ),
    );
    write(ws, ".devcontainer/Dockerfile", dockerfile);
}

/// T018: debian — the injected cert lands in the system trust store (split
/// per-cert into `/usr/local/share/ca-certificates`) and the canonical bundle
/// exists. Asserted by `docker exec … cat`, not just the JSON outcome.
#[test]
fn debian_cert_lands_in_system_store() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let ws = TempDir::new().unwrap();
    dockerfile_config(
        ws.path(),
        "hostca-debian",
        "FROM debian:bookworm-slim\n\
         RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
         && rm -rf /var/lib/apt/lists/*\n",
    );
    let pem = write_ca_fixture(ws.path());

    assert!(
        up_with_ca(ws.path(), &pem),
        "up --inject-host-ca should succeed"
    );

    let (ok_bundle, head) = exec_sh(ws.path(), &format!("head -1 {BUNDLE_PATH}"));
    let (ok_store, _) = exec_sh(
        ws.path(),
        "test -f /usr/local/share/ca-certificates/deacon-host-ca-1.crt && echo STORE_OK",
    );
    down(ws.path());

    assert!(ok_bundle, "canonical bundle must be readable");
    assert_eq!(head, "-----BEGIN CERTIFICATE-----");
    assert!(
        ok_store,
        "cert must be installed into the debian system store"
    );
}

/// T019: injection happens BEFORE the first lifecycle hook — a
/// `postCreateCommand` that requires the injected cert succeeds (so `up`
/// succeeds). If injection ran after hooks, `up` would fail here.
#[test]
fn postcreate_reads_injected_cert() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let ws = TempDir::new().unwrap();
    write(
        ws.path(),
        ".devcontainer/devcontainer.json",
        &format!(
            r#"{{ "name": "hostca-order", "image": "debian:bookworm-slim",
                  "overrideCommand": true,
                  "postCreateCommand": "test -f {BUNDLE_PATH}" }}"#
        ),
    );
    let pem = write_ca_fixture(ws.path());

    let ok = up_with_ca(ws.path(), &pem);
    down(ws.path());
    assert!(
        ok,
        "postCreateCommand requiring the injected cert must pass (inject before hooks)"
    );
}

/// T020: RHEL family (rockylinux:9) — cert installed via update-ca-trust anchors.
#[test]
fn rhel_family_cert_in_store() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let ws = TempDir::new().unwrap();
    dockerfile_config(
        ws.path(),
        "hostca-rocky",
        "FROM rockylinux:9\nRUN dnf install -y ca-certificates && dnf clean all\n",
    );
    let pem = write_ca_fixture(ws.path());

    assert!(
        up_with_ca(ws.path(), &pem),
        "up should succeed on rockylinux:9"
    );
    let (ok_store, _) = exec_sh(
        ws.path(),
        "test -f /etc/pki/ca-trust/source/anchors/deacon-host-ca.crt && echo STORE_OK",
    );
    down(ws.path());
    assert!(ok_store, "cert must be installed into the RHEL anchors dir");
}

/// T020: alpine — cert installed via update-ca-certificates.
#[test]
fn alpine_cert_in_store() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let ws = TempDir::new().unwrap();
    dockerfile_config(
        ws.path(),
        "hostca-alpine",
        "FROM alpine:3.20\nRUN apk add --no-cache ca-certificates\n",
    );
    let pem = write_ca_fixture(ws.path());

    assert!(
        up_with_ca(ws.path(), &pem),
        "up should succeed on alpine:3.20"
    );
    let (ok_store, _) = exec_sh(
        ws.path(),
        "test -f /usr/local/share/ca-certificates/deacon-host-ca-1.crt && echo STORE_OK",
    );
    down(ws.path());
    assert!(ok_store, "cert must be installed into the alpine store dir");
}

/// T021: unsupported distro (busybox, no updater) — env-var-only fallback. `up`
/// must NOT abort; the canonical bundle is still written (root) but no system
/// store file appears.
#[test]
fn unsupported_distro_envonly_no_abort() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let ws = TempDir::new().unwrap();
    write(
        ws.path(),
        ".devcontainer/devcontainer.json",
        &image_config("hostca-busybox", "busybox:1.36"),
    );
    let pem = write_ca_fixture(ws.path());

    assert!(
        up_with_ca(ws.path(), &pem),
        "unsupported distro must NOT abort up (FR-022 env-var-only fallback)"
    );
    let (ok_bundle, _) = exec_sh(ws.path(), &format!("test -f {BUNDLE_PATH} && echo OK"));
    down(ws.path());
    assert!(
        ok_bundle,
        "canonical bundle must still be written for the env-var-only fallback"
    );
}

/// T021: non-root exec user — env-var-only fallback, no `up` abort. Built from a
/// Dockerfile whose default USER is non-root.
#[test]
fn nonroot_user_envonly_no_abort() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let ws = TempDir::new().unwrap();
    write(
        ws.path(),
        ".devcontainer/devcontainer.json",
        r#"{ "name": "hostca-nonroot", "build": { "dockerfile": "Dockerfile" }, "overrideCommand": true }"#,
    );
    write(
        ws.path(),
        ".devcontainer/Dockerfile",
        "FROM debian:bookworm-slim\nRUN useradd -m app\nUSER app\n",
    );
    let pem = write_ca_fixture(ws.path());

    let ok = up_with_ca(ws.path(), &pem);
    down(ws.path());
    assert!(
        ok,
        "non-root exec user must NOT abort up (FR-022 env-var-only fallback)"
    );
}

/// T031: the six CA env vars are visible to `exec`, pointing at the bundle
/// (sourced from the container label on reconnect — no re-discovery).
#[test]
fn exec_sees_six_ca_env_vars() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let ws = TempDir::new().unwrap();
    write(
        ws.path(),
        ".devcontainer/devcontainer.json",
        &image_config("hostca-env", "debian:bookworm-slim"),
    );
    let pem = write_ca_fixture(ws.path());

    assert!(up_with_ca(ws.path(), &pem), "up should succeed");
    let (ok, out) = exec_sh(
        ws.path(),
        "echo \"$SSL_CERT_FILE|$NODE_EXTRA_CA_CERTS|$REQUESTS_CA_BUNDLE|$PIP_CERT|$GIT_SSL_CAINFO|$CURL_CA_BUNDLE\"",
    );
    down(ws.path());

    assert!(ok, "exec should succeed");
    let expected = format!(
        "{BUNDLE_PATH}|{BUNDLE_PATH}|{BUNDLE_PATH}|{BUNDLE_PATH}|{BUNDLE_PATH}|{BUNDLE_PATH}"
    );
    assert_eq!(
        out, expected,
        "all six CA env vars must point at the bundle"
    );
}

/// T043 (SC-007): a workspace `devcontainer.json` CANNOT enable injection.
/// Without the flag/env/settings, `up` performs no injection — no bundle, no CA
/// env vars — even though the workspace is fully under attacker control.
#[test]
fn workspace_config_cannot_enable_injection() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }
    let ws = TempDir::new().unwrap();
    // A hostile config might *try* to set a host-CA key; deacon has no such
    // config key and never reads activation from the workspace (FR-015).
    write(
        ws.path(),
        ".devcontainer/devcontainer.json",
        r#"{ "name": "hostca-adversarial", "image": "debian:bookworm-slim",
              "overrideCommand": true,
              "hostCa": "auto", "injectHostCa": "auto" }"#,
    );

    assert!(up_plain(ws.path()), "up (no flag) should succeed");
    let (bundle_present, _) = exec_sh(ws.path(), &format!("test -f {BUNDLE_PATH} && echo PRESENT"));
    let (_, ssl) = exec_sh(ws.path(), "printf '%s' \"${SSL_CERT_FILE:-UNSET}\"");
    down(ws.path());

    assert!(
        !bundle_present,
        "no CA bundle must be injected when only the workspace config 'asks' for it (SC-007)"
    );
    assert_eq!(
        ssl, "UNSET",
        "no CA env var must be set from a workspace-driven request"
    );
}
