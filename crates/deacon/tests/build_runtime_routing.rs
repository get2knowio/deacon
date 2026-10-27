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

/// A podman stand-in: the same recorder, but it answers `-v` the way podman
/// does, so flavor detection (`--docker-path podman`) sees podman too. A build
/// writes an image id to its `--iidfile`, so deacon gets past the build to what
/// follows it (the deferred push).
const PODMAN_SHIM: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$SHIM_LOG"
prev=""
for a in "$@"; do
  [ "$prev" = "--iidfile" ] && printf 'sha256:shim' > "$a"
  prev="$a"
done
case "$1 $2" in
  "image inspect"*) echo '[]' ;;
  "buildx version") echo "buildah 1.33.7" ;;
esac
case "$1" in
  -v|--version) echo "podman version 4.9.3" ;;
  version)      echo '{"Client":{"Version":"4.9.3"},"Server":{"Version":"4.9.3"}}' ;;
  inspect)
    case "$2" in
      --format*) echo '{}' ;;
      *)         echo '[]' ;;
    esac
    ;;
esac
exit 0
"#;

fn write_podman_shim(dir: &Path) -> std::path::PathBuf {
    let shim = dir.join("podman-shim");
    fs::write(&shim, PODMAN_SHIM).unwrap();
    let mut perms = fs::metadata(&shim).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&shim, perms).unwrap();
    shim
}

/// How podman was selected. Both routes must reach the same gate: the flavor
/// named outright, and the flavor detected from the binary the path names.
#[derive(Clone, Copy, Debug)]
enum PodmanBy {
    RuntimeFlag,
    DetectedFromBinary,
}

struct Run {
    success: bool,
    stdout: String,
    recorded: String,
}

fn run_build_on_podman(by: PodmanBy, flags: &[&str]) -> Run {
    let workspace = TempDir::new().unwrap();
    let bin = TempDir::new().unwrap();
    write_workspace(workspace.path());
    let shim = write_podman_shim(bin.path());
    let log = bin.path().join("invocations.log");

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    if matches!(by, PodmanBy::RuntimeFlag) {
        cmd.args(["--runtime", "podman"]);
    }
    let output = cmd
        .arg("build")
        .arg("--workspace-folder")
        .arg(workspace.path())
        .args(["--docker-path", shim.to_str().unwrap()])
        .args(["--output-format", "json", "--force"])
        .args(flags)
        // Neither a host env var nor a host `docker` may decide this.
        .env_remove("DEACON_CONTAINER_RUNTIME")
        .env_remove("DOCKER_BUILDKIT")
        .env("SHIM_LOG", &log)
        .output()
        .unwrap();
    Run {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        recorded: fs::read_to_string(&log).unwrap_or_default(),
    }
}

fn build_lines(recorded: &str) -> Vec<&str> {
    recorded
        .lines()
        .filter(|l| l.starts_with("buildx build") || l.starts_with("build "))
        .collect()
}

/// #775: the BuildKit-only shapes Buildah cannot honour are refused BEFORE any
/// build runs, under podman only, with an error that names the flag.
///
/// Each of these used to reach `podman buildx build` and, for most of them,
/// come back `outcome: success` (measured against rootless podman 4.9.3):
/// `--output type=docker` wrote a root filesystem into a directory named
/// `type=docker` in the workspace, `--output type=oci` exported nothing, and a
/// multi-platform `--platform` produced a single-architecture image.
#[test]
fn buildkit_only_flags_fail_fast_on_podman_without_building() {
    let cases: &[(&[&str], &str)] = &[
        (&["--output", "type=docker"], "--output"),
        (&["--output", "type=oci,dest=/tmp/x.tar"], "--output"),
        (&["--output", "type=registry"], "--output"),
        (
            &["--output", "type=image,name=r.example/x:1,push=true"],
            "--output",
        ),
        (&["--platform", "linux/amd64,linux/arm64"], "--platform"),
        (
            &[
                "--platform",
                "linux/amd64,linux/arm64",
                "--push",
                "--image-name",
                "r.example/x:1",
            ],
            "--platform",
        ),
        (&["--cache-to", "type=local,dest=/tmp/cache"], "--cache-to"),
        (&["--cache-to", "type=inline"], "--cache-to"),
        (
            &["--cache-from", "type=registry,ref=r.example/cache"],
            "--cache-from",
        ),
    ];
    for by in [PodmanBy::RuntimeFlag, PodmanBy::DetectedFromBinary] {
        for (flags, flag) in cases {
            let run = run_build_on_podman(by, flags);
            assert!(
                !run.success,
                "{by:?} {flags:?}: must fail, not build; stdout:\n{}",
                run.stdout
            );
            let doc: serde_json::Value =
                serde_json::from_str(run.stdout.trim()).unwrap_or_else(|e| {
                    panic!(
                        "{by:?} {flags:?}: stdout is not one JSON document ({e}):\n{}",
                        run.stdout
                    )
                });
            assert_eq!(doc["outcome"], "error", "{by:?} {flags:?}");
            assert_eq!(
                doc["message"],
                format!("Cannot use {flag} with Podman"),
                "{by:?} {flags:?}: the error must name the flag"
            );
            let description = doc["description"].as_str().unwrap_or_default();
            assert!(
                description.contains("Buildah, not BuildKit"),
                "{by:?} {flags:?}: the error must say why; description: {description}"
            );
            assert!(
                build_lines(&run.recorded).is_empty(),
                "{by:?} {flags:?}: the build was invoked anyway:\n{}",
                run.recorded
            );
        }
    }
}

/// The other half of #775: what podman DOES serve equivalently is not refused,
/// and reaches the build unchanged.
///
/// `--output type=local` / `type=tar` are Buildah's own exporters. They must
/// reach podman WITHOUT the `--output type=docker` deacon appends on Docker to
/// load the image: Buildah always commits locally, reads `type=docker` as a
/// directory to write into, and podman < 5.6 keeps only the last `--output`, so
/// the appended one silently replaced the user's destination.
#[test]
fn flags_podman_serves_reach_the_podman_build_unchanged() {
    let cases: &[(&[&str], &[&str])] = &[
        (
            &["--output", "type=local,dest=/tmp/out"],
            &["--output type=local,dest=/tmp/out"],
        ),
        (
            &["--output", "type=tar,dest=/tmp/out.tar"],
            &["--output type=tar,dest=/tmp/out.tar"],
        ),
        (&["--output", "/tmp/outdir"], &["--output /tmp/outdir"]),
        (&["--platform", "linux/arm64"], &["--platform linux/arm64"]),
        (
            &[
                "--cache-to",
                "r.example/cache",
                "--cache-from",
                "r.example/cache",
            ],
            &["--cache-to r.example/cache", "--cache-from r.example/cache"],
        ),
    ];
    for by in [PodmanBy::RuntimeFlag, PodmanBy::DetectedFromBinary] {
        for (flags, expected) in cases {
            let run = run_build_on_podman(by, flags);
            let builds = build_lines(&run.recorded);
            assert_eq!(
                builds.len(),
                1,
                "{by:?} {flags:?}: expected exactly one build; stdout:\n{}\nrecorded:\n{}",
                run.stdout,
                run.recorded
            );
            let argv = builds[0];
            for fragment in *expected {
                assert!(
                    argv.contains(fragment),
                    "{by:?} {flags:?}: `{fragment}` did not reach the build:\n{argv}"
                );
            }
            assert!(
                !argv.contains("type=docker"),
                "{by:?} {flags:?}: deacon appended a docker exporter podman cannot serve:\n{argv}"
            );
        }
    }
}

/// `--push` is NOT refused on podman: a single-platform build loads locally, and
/// the push is issued afterwards as `<runtime> push`, which podman serves.
#[test]
fn single_platform_push_on_podman_pushes_from_local_storage() {
    let run = run_build_on_podman(
        PodmanBy::RuntimeFlag,
        &["--push", "--image-name", "r.example/x:1"],
    );
    let builds = build_lines(&run.recorded);
    assert_eq!(builds.len(), 1, "recorded:\n{}", run.recorded);
    assert!(
        !builds[0].contains("--push"),
        "a single-platform push must not ride the podman build (podman has no --push):\n{}",
        builds[0]
    );
    assert!(
        run.recorded.lines().any(|l| l.starts_with("push ")),
        "the push must be issued from local storage on the selected runtime:\n{}",
        run.recorded
    );
}

/// The Docker side of the same decision, unchanged: a non-daemon `--output` is
/// still paired with a `type=docker` exporter so the local tags name the image
/// this build produced (#440). Guards the podman branch from leaking into it.
#[test]
fn docker_build_still_pairs_a_non_daemon_output_with_a_docker_exporter() {
    let workspace = TempDir::new().unwrap();
    let bin = TempDir::new().unwrap();
    write_workspace(workspace.path());
    let shim = write_shim(bin.path());
    let log = bin.path().join("invocations.log");

    let _ = Command::cargo_bin("deacon")
        .unwrap()
        .args(["--runtime", "docker"])
        .arg("build")
        .arg("--workspace-folder")
        .arg(workspace.path())
        .args(["--docker-path", shim.to_str().unwrap()])
        .args(["--force", "--output", "type=local,dest=/tmp/out"])
        .env_remove("DOCKER_BUILDKIT")
        .env("SHIM_LOG", &log)
        .output()
        .unwrap();

    let recorded = fs::read_to_string(&log).unwrap_or_default();
    let builds = build_lines(&recorded);
    assert_eq!(builds.len(), 1, "recorded:\n{recorded}");
    assert!(
        builds[0].contains("--output type=local,dest=/tmp/out")
            && builds[0].contains("--output type=docker"),
        "{}",
        builds[0]
    );
}
