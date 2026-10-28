//! `deacon up` must ask the BuildKit question of the runtime it will BUILD with
//! (#780), the `up` half of what #775 fixed for `build`.
//!
//! Before installing Features, `up` refused unless BuildKit was available, and
//! it found out by running a literal `docker buildx version`. That ignored
//! `--docker-path`, `--runtime podman` and the podman fallback (#770). MEASURED
//! with these shims before the fix: on a podman-only host every `up` with a
//! Feature failed with `BuildKit is required for feature installation`,
//! whichever way podman was selected. With `--docker-path` naming a docker that
//! HAS buildx, the `docker` on `PATH` was asked instead, and its answer refused
//! the build. On CI, where docker and buildx sit next to podman, the probe
//! passed for the wrong reason, which is why the Podman lane never saw it.
//!
//! A docker whose answer is "no buildx" no longer gets a refusal at all (#783).
//! As in the reference, it gets the classic route: a Feature-content image plus
//! a plain `build`. `--buildkit never` selects the same route.
//!
//! Every runtime here is a recording shim, and `PATH` is controlled so that a
//! host docker cannot answer. No daemon is needed. The shim cannot really run a
//! container, so `up` fails AFTER the build. The assertions are on which binary
//! received what, never on the exit status, except for the refusal case.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

/// A recording runtime shim. `LABEL` prefixes each recorded argv so one log can
/// hold several binaries, `BUILDX_RC` is what `buildx version` exits with, and
/// `VERSION` is what `-v` prints (flavor detection reads it).
fn write_shim(path: &Path, label: &str, buildx_rc: i32, version: &str) {
    let script = format!(
        r#"#!/bin/sh
printf '{label}: %s\n' "$*" >> "$SHIM_LOG"
prev=""
for a in "$@"; do
  [ "$prev" = "--iidfile" ] && printf 'sha256:shim' > "$a"
  prev="$a"
done
case "$1 $2" in
  "buildx version")
    [ {buildx_rc} -eq 0 ] && echo "github.com/docker/buildx v0.20.0 shim"
    exit {buildx_rc} ;;
  "image inspect"*)
    echo '[{{"Id":"sha256:abc","Config":{{"User":"","Labels":{{}}}},"Architecture":"amd64","Os":"linux"}}]'
    exit 0 ;;
esac
case "$1" in
  -v|--version) echo "{version}" ;;
  version)      echo '{{"Client":{{"Version":"99.0.0"}},"Server":{{"Version":"99.0.0"}}}}' ;;
  inspect)      echo '[]' ;;
esac
exit 0
"#
    );
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

const DOCKER_VERSION: &str = "Docker version 99.0.0, build shim";
const PODMAN_VERSION: &str = "podman version 4.9.3";

/// The configuration shapes whose Feature build runs the BuildKit gate.
#[derive(Clone, Copy, Debug)]
enum Shape {
    Image,
    Dockerfile,
}

fn write_workspace(dir: &Path, shape: Shape) {
    let dc = dir.join(".devcontainer");
    let feat = dc.join("feat");
    fs::create_dir_all(&feat).unwrap();
    let source = match shape {
        Shape::Image => r#""image": "alpine:3.18""#,
        Shape::Dockerfile => {
            fs::write(dc.join("Dockerfile"), "FROM alpine:3.18\n").unwrap();
            r#""build": { "dockerfile": "Dockerfile" }"#
        }
    };
    fs::write(
        dc.join("devcontainer.json"),
        format!(r#"{{ "name": "probe", {source}, "features": {{ "./feat": {{}} }} }}"#),
    )
    .unwrap();
    fs::write(
        feat.join("devcontainer-feature.json"),
        r#"{ "id": "feat", "version": "1.0.0", "name": "feat" }"#,
    )
    .unwrap();
    fs::write(feat.join("install.sh"), "#!/bin/sh\ntrue\n").unwrap();
}

struct Host {
    _root: TempDir,
    workspace: PathBuf,
    /// Prepended to a `PATH` that has nothing else on it.
    path_dir: PathBuf,
    /// Shims that are NOT on `PATH`, reachable only through `--docker-path`.
    off_path: PathBuf,
    log: PathBuf,
}

impl Host {
    fn new(shape: Shape) -> Self {
        let root = TempDir::new().unwrap();
        let workspace = root.path().join("ws");
        let path_dir = root.path().join("path");
        let off_path = root.path().join("bin");
        for d in [&workspace, &path_dir, &off_path] {
            fs::create_dir_all(d).unwrap();
        }
        write_workspace(&workspace, shape);
        let log = root.path().join("invocations.log");
        Self {
            _root: root,
            workspace,
            path_dir,
            off_path,
            log,
        }
    }

    fn up(&self, global: &[&str], flags: &[&str]) -> (bool, String, String) {
        let output = Command::cargo_bin("deacon")
            .unwrap()
            .args(global)
            .arg("up")
            .arg("--workspace-folder")
            .arg(&self.workspace)
            .arg("--trust-workspace")
            .args(flags)
            // Nothing outside the shims may answer, and no host setting may
            // choose the runtime for us.
            .env("PATH", format!("{}:/nonexistent", self.path_dir.display()))
            .env_remove("DEACON_CONTAINER_RUNTIME")
            .env_remove("DOCKER_BUILDKIT")
            .env("SHIM_LOG", &self.log)
            .output()
            .unwrap();
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
            fs::read_to_string(&self.log).unwrap_or_default(),
        )
    }
}

fn lines_of<'a>(recorded: &'a str, label: &str, prefix: &str) -> Vec<&'a str> {
    let head = format!("{label}: {prefix}");
    recorded.lines().filter(|l| l.starts_with(&head)).collect()
}

const REFUSAL: &str = "BuildKit is required for feature installation";

/// `--docker-path` names a docker that has buildx, and the `docker` on `PATH`
/// does not. The named one must be asked, and the build must go ahead on it.
#[test]
fn up_probes_buildkit_on_the_binary_named_by_docker_path() {
    for shape in [Shape::Image, Shape::Dockerfile] {
        let host = Host::new(shape);
        write_shim(&host.path_dir.join("docker"), "PATH", 1, DOCKER_VERSION);
        let selected = host.off_path.join("docker-selected");
        write_shim(&selected, "selected", 0, DOCKER_VERSION);

        let (_, stderr, recorded) = host.up(&[], &["--docker-path", selected.to_str().unwrap()]);

        assert!(
            !stderr.contains(REFUSAL),
            "{shape:?}: refused on another binary's answer:\n{stderr}\nrecorded:\n{recorded}"
        );
        assert_eq!(
            lines_of(&recorded, "selected", "buildx version").len(),
            1,
            "{shape:?}: the binary named by --docker-path was not probed:\n{recorded}"
        );
        assert!(
            lines_of(&recorded, "PATH", "").is_empty(),
            "{shape:?}: the `docker` on PATH was invoked although --docker-path named another:\n{recorded}"
        );
        assert!(
            !lines_of(&recorded, "selected", "buildx build").is_empty(),
            "{shape:?}: the Feature build never reached the selected binary:\n{recorded}"
        );
    }
}

/// The SELECTED binary decides, and a selected docker without buildx gets the
/// reference's non-BuildKit route rather than a refusal (#783): the Feature
/// content is built into an image, and the Feature image is built with plain
/// `build`, no `--build-context`. Before #780 the `PATH` docker's answer decided;
/// between #780 and #783 this was refused with `BuildKit is required for feature
/// installation`, which the reference never says.
#[test]
fn up_builds_features_without_buildkit_when_the_selected_docker_has_no_buildx() {
    for shape in [Shape::Image, Shape::Dockerfile] {
        let host = Host::new(shape);
        write_shim(&host.path_dir.join("docker"), "PATH", 0, DOCKER_VERSION);
        let selected = host.off_path.join("docker-selected");
        write_shim(&selected, "selected", 1, DOCKER_VERSION);

        let (_, stderr, recorded) = host.up(&[], &["--docker-path", selected.to_str().unwrap()]);

        assert_classic_feature_build(shape, &stderr, &recorded, "selected");
        assert!(
            lines_of(&recorded, "PATH", "").is_empty(),
            "{shape:?}: the `docker` on PATH was invoked although --docker-path named another:\n{recorded}"
        );
    }
}

/// `up --buildkit never` takes the same route even where buildx works (#783). It
/// used to be parsed and then ignored, so the Feature image went through
/// `buildx build` and a BuildKit build context anyway.
#[test]
fn up_buildkit_never_builds_features_with_the_classic_builder() {
    for shape in [Shape::Image, Shape::Dockerfile] {
        let host = Host::new(shape);
        let docker = host.path_dir.join("docker");
        write_shim(&docker, "docker", 0, DOCKER_VERSION);

        let (_, stderr, recorded) = host.up(&[], &["--buildkit", "never"]);

        assert_classic_feature_build(shape, &stderr, &recorded, "docker");
    }
}

/// The reference drops `--cache-to` on its non-BuildKit path; deacon refuses it,
/// as `deacon build --buildkit never --cache-to` does, rather than report success
/// for a cache export that never happened. Refused before any build runs.
#[test]
fn up_refuses_cache_to_on_the_classic_feature_builder() {
    let host = Host::new(Shape::Image);
    write_shim(&host.path_dir.join("docker"), "docker", 0, DOCKER_VERSION);

    let (success, stderr, recorded) = host.up(
        &[],
        &[
            "--buildkit",
            "never",
            "--cache-to",
            "type=local,dest=/tmp/x",
        ],
    );

    assert!(!success, "must refuse; recorded:\n{recorded}");
    assert!(
        stderr.contains("--cache-to requires BuildKit") && stderr.contains("--buildkit never"),
        "expected the --cache-to refusal naming its cause:\n{stderr}"
    );
    assert!(
        lines_of(&recorded, "docker", "build").is_empty()
            && lines_of(&recorded, "docker", "buildx build").is_empty(),
        "a build ran despite the refusal:\n{recorded}"
    );
}

/// The two invocations of the classic route, both on `label`, and nothing on
/// BuildKit.
fn assert_classic_feature_build(shape: Shape, stderr: &str, recorded: &str, label: &str) {
    assert!(
        !stderr.contains(REFUSAL),
        "{shape:?}: Features refused instead of built without BuildKit:\n{stderr}\nrecorded:\n{recorded}"
    );
    assert!(
        !recorded.contains("buildx build"),
        "{shape:?}: the Feature build went through buildx:\n{recorded}"
    );
    assert_eq!(
        lines_of(recorded, label, "build -t deacon-feature-content:").len(),
        1,
        "{shape:?}: the Feature content image was not built on {label}:\n{recorded}"
    );
    let feature_builds: Vec<&str> = lines_of(recorded, label, "build ")
        .into_iter()
        .filter(|l| l.contains("--target dev_containers_target_stage"))
        .collect();
    assert_eq!(
        feature_builds.len(),
        1,
        "{shape:?}: expected one plain `build` of the Feature stage on {label}:\n{recorded}"
    );
    assert!(
        !feature_builds[0].contains("--build-context"),
        "{shape:?}: the classic builder has no build contexts:\n{}",
        feature_builds[0]
    );
    assert!(
        recorded.contains(&format!("{label}: image rm -f deacon-feature-content:")),
        "{shape:?}: the staged content image was not removed:\n{recorded}"
    );
}

/// How podman came to be the runtime.
#[derive(Clone, Copy, Debug)]
enum PodmanBy {
    /// `--runtime podman`, with `podman` resolved from `PATH`.
    RuntimeFlag,
    /// `--docker-path <…/podman>`, the flavor detected from the binary.
    DockerPath,
    /// Nothing selected and no docker on `PATH`: the #770 fallback.
    Fallback,
}

/// On a podman-only host `up` installs Features: podman is not asked a BuildKit
/// question (Buildah would answer with its own version), and the build runs on
/// podman. Before #780 every one of these was refused.
#[test]
fn up_does_not_ask_podman_a_buildkit_question() {
    for shape in [Shape::Image, Shape::Dockerfile] {
        for by in [
            PodmanBy::RuntimeFlag,
            PodmanBy::DockerPath,
            PodmanBy::Fallback,
        ] {
            let host = Host::new(shape);
            let podman = host.path_dir.join("podman");
            write_shim(&podman, "podman", 0, PODMAN_VERSION);

            let podman_path = podman.to_str().unwrap().to_string();
            let (global, flags): (&[&str], Vec<&str>) = match by {
                PodmanBy::RuntimeFlag => (&["--runtime", "podman"], vec![]),
                PodmanBy::DockerPath => (&[], vec!["--docker-path", podman_path.as_str()]),
                PodmanBy::Fallback => (&[], vec![]),
            };
            let (_, stderr, recorded) = host.up(global, &flags);

            assert!(
                !stderr.contains(REFUSAL),
                "{shape:?} {by:?}: Features refused on a podman host:\n{stderr}\nrecorded:\n{recorded}"
            );
            assert!(
                lines_of(&recorded, "podman", "buildx version").is_empty(),
                "{shape:?} {by:?}: podman was asked a BuildKit question:\n{recorded}"
            );
            assert!(
                !lines_of(&recorded, "podman", "buildx build").is_empty(),
                "{shape:?} {by:?}: the Feature build never reached podman:\n{recorded}"
            );
        }
    }
}
