//! `deacon build` must run the BUILD on the runtime binary it was told to use,
//! not merely validate against it (#708).
//!
//! The pre-existing `--docker-path /nonexistent/…` test asserts a build FAILS on
//! an unusable path, and it passed the whole time the bug was live: the probes
//! (`check_docker_installed`, `ping`) did honour the resolved path, so an
//! unusable one failed there and never reached the build. Everything after those
//! probes ran `Command::new("docker")` — a literal. On any machine with docker
//! installed (every GitHub runner), `--docker-path podman` therefore validated
//! podman and then built with docker, silently.
//!
//! Only a test that observes WHICH binary received the build can see that, so
//! this one records every invocation through a shim and asserts the build
//! invocation is among them. It needs no daemon: the shim answers the probes and
//! exits 0 without building anything, and the assertion is on the recorded argv,
//! never on deacon's exit status.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use assert_cmd::Command;
use tempfile::TempDir;

/// A stand-in for the container runtime: appends each invocation's argv to
/// `$SHIM_LOG`, answers the version/inspect probes plausibly enough for deacon to
/// proceed, and exits 0 without doing any work.
const SHIM: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$SHIM_LOG"
case "$1 $2" in
  "image inspect"*) echo '[]' ;;
esac
case "$1" in
  -v|--version) echo "Docker version 99.0.0, build shim" ;;
  version)      echo '{"Client":{"Version":"99.0.0"},"Server":{"Version":"99.0.0"}}' ;;
  inspect)      echo '[]' ;;
esac
exit 0
"#;

fn write_shim(dir: &Path) -> std::path::PathBuf {
    let shim = dir.join("runtime-shim");
    fs::write(&shim, SHIM).unwrap();
    let mut perms = fs::metadata(&shim).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&shim, perms).unwrap();
    shim
}

fn write_workspace(dir: &Path) {
    let dc = dir.join(".devcontainer");
    fs::create_dir_all(&dc).unwrap();
    fs::write(
        dc.join("devcontainer.json"),
        r#"{ "name": "runtime-routing", "build": { "dockerfile": "Dockerfile" } }"#,
    )
    .unwrap();
    // A base the shim reports as absent, so nothing here depends on a real image.
    fs::write(dc.join("Dockerfile"), "FROM alpine:3.18\nRUN true\n").unwrap();
}

#[test]
fn build_runs_on_the_runtime_named_by_docker_path() {
    let workspace = TempDir::new().unwrap();
    let bin = TempDir::new().unwrap();
    write_workspace(workspace.path());
    let shim = write_shim(bin.path());
    let log = bin.path().join("invocations.log");

    // Exit status is deliberately not asserted: the shim cannot produce an image,
    // so the build fails downstream. What is under test is which binary was asked.
    let _ = Command::cargo_bin("deacon")
        .unwrap()
        .arg("build")
        .arg("--workspace-folder")
        .arg(workspace.path())
        .args(["--docker-path", shim.to_str().unwrap()])
        .env("SHIM_LOG", &log)
        .assert();

    let recorded = fs::read_to_string(&log).unwrap_or_default();
    assert!(
        !recorded.is_empty(),
        "the named binary was never invoked at all; --docker-path is being ignored outright"
    );

    // `docker build` is the legacy invocation and `buildx build` the default one;
    // either is a build, and pinning only one would make this test a spec for the
    // buildx decision rather than for runtime routing.
    let built = recorded
        .lines()
        .any(|l| l.starts_with("buildx build") || l.starts_with("build "));
    assert!(
        built,
        "the BUILD never reached the binary named by --docker-path (#708). Only \
         these invocations did, which is exactly the bug: validation honoured the \
         flag and execution did not.\nrecorded:\n{recorded}"
    );
}

/// The compose CLIENT is the same question one layer up (#710).
///
/// `deacon build` on a compose config used to construct its `ComposeManager` with
/// `ComposeManager::new()`, which defaults `docker_path` to the literal `"docker"`.
/// So `--docker-path` (and `--runtime podman`, whose resolved binary is `podman`)
/// selected the runtime for every probe and for `tag`, while `docker compose` did
/// the actual build — putting the service image in a DIFFERENT daemon's store than
/// the one deacon went on to tag, run and exec against. On the Podman lane that
/// surfaced as `image not known` from every step after the build, and all seven
/// tests in `integration_compose_features_build` failed on it.
///
/// MEASURED before the fix, with this same shim technique on a real compose
/// config: the resolved runtime received `-v`, `image inspect` and `tag` — and
/// zero `compose` invocations. Afterwards it received three, the service build
/// among them.
///
/// The assertion is on `compose` reaching the named binary at all, which is
/// exactly the axis that was broken; how far the compose flow then gets depends
/// on how completely the shim can impersonate Compose, and that is not what this
/// guards.
const COMPOSE_SHIM: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$SHIM_LOG"
case "$1" in
  -v|--version) echo "Docker version 99.0.0, build shim" ;;
  version)      echo '{"Client":{"Version":"99.0.0"},"Server":{"Version":"99.0.0"}}' ;;
  inspect)      echo '[]' ;;
  compose)
    # `config --services` lists services; `config --format json` renders the
    # project. Enough of each for `build` to reach the service build.
    for a in "$@"; do
      if [ "$a" = "--services" ]; then echo app; exit 0; fi
    done
    echo '{"name":"shimproj","services":{"app":{"image":"alpine:3.18"}}}'
    ;;
esac
exit 0
"#;

fn write_compose_shim(dir: &Path) -> std::path::PathBuf {
    let shim = dir.join("compose-shim");
    fs::write(&shim, COMPOSE_SHIM).unwrap();
    let mut perms = fs::metadata(&shim).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&shim, perms).unwrap();
    shim
}

fn write_compose_workspace(dir: &Path) {
    fs::write(
        dir.join("docker-compose.yml"),
        "services:\n  app:\n    image: alpine:3.18\n    command: [\"sleep\",\"infinity\"]\n",
    )
    .unwrap();
    let dc = dir.join(".devcontainer");
    fs::create_dir_all(&dc).unwrap();
    // `dockerComposeFile` is config-dir-relative, so `../` reaches the workspace root.
    fs::write(
        dc.join("devcontainer.json"),
        r#"{ "name": "compose-routing", "dockerComposeFile": "../docker-compose.yml", "service": "app" }"#,
    )
    .unwrap();
}

#[test]
fn compose_build_runs_on_the_runtime_named_by_docker_path() {
    let workspace = TempDir::new().unwrap();
    let bin = TempDir::new().unwrap();
    write_compose_workspace(workspace.path());
    let shim = write_compose_shim(bin.path());
    let log = bin.path().join("invocations.log");

    // As above, the exit status is not the subject — the shim cannot produce an
    // image. What is under test is which binary was asked to run Compose.
    let _ = Command::cargo_bin("deacon")
        .unwrap()
        .arg("build")
        .arg("--workspace-folder")
        .arg(workspace.path())
        .args(["--docker-path", shim.to_str().unwrap()])
        .env("SHIM_LOG", &log)
        .assert();

    let recorded = fs::read_to_string(&log).unwrap_or_default();
    assert!(
        !recorded.is_empty(),
        "the named binary was never invoked at all; --docker-path is being ignored outright"
    );
    assert!(
        recorded.lines().any(|l| l.starts_with("compose ")),
        "no `compose` invocation reached the binary named by --docker-path (#710). \
         deacon probed the named runtime and then ran Compose on a literal `docker`, \
         so the service image landed in another daemon's store.\nrecorded:\n{recorded}"
    );
}
