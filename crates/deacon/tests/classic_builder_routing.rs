//! Which builder `deacon build` and the Compose Feature build run on when the
//! selected Docker has no buildx, or when BuildKit is switched off.
//!
//! The reference (0.89.0) asks `<docker-path> buildx version` once per command.
//! When it fails, or `--buildkit never` was passed, it has no BuildKit version,
//! and it then:
//! - builds a single-container configuration with plain `docker build`, staging
//!   any Feature content as a `FROM scratch` image read with `COPY --from=`;
//! - builds a Compose configuration with `docker compose build`, staging the
//!   Feature content the same way.
//!
//! MEASURED before this fix, with buildx truly absent from docker 29.9.2:
//! `deacon build` failed `docker: unknown command: docker buildx` on every
//! image-reference and Dockerfile configuration, with or without Features,
//! because the primary build ran `buildx build` unless BuildKit was switched off.
//! A Compose configuration with Features was refused, on `up` and on `build`,
//! with `BuildKit is required for feature installation`. And on Compose,
//! `--buildkit never` was parsed and then ignored: every Compose build still ran
//! on BuildKit.
//!
//! Every runtime here is a recording shim on a `PATH` with nothing else on it, so
//! no daemon is needed and no host docker can answer. A shim cannot produce an
//! image, so most commands fail AFTER the build. The assertions are on what the
//! shim received, and on the exit status only for the refusals.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

/// A recording docker. Each argv is logged as `BK=<DOCKER_BUILDKIT> <argv>`, and
/// a Compose override read from stdin (`-f -`) is appended to `$SHIM_LOG.stdin`.
/// `BUILDX_RC` is what `buildx version` exits with; every other `buildx`
/// subcommand fails as it does on an engine without the plugin.
fn write_shim(path: &Path, buildx_rc: i32) {
    let script = format!(
        r#"#!/bin/sh
printf 'BK=%s %s\n' "${{DOCKER_BUILDKIT-unset}}" "$*" >> "$SHIM_LOG"
prev=""
stdin_override=0
for a in "$@"; do
  [ "$prev" = "--iidfile" ] && printf 'sha256:shim' > "$a"
  [ "$prev" = "-f" ] && [ "$a" = "-" ] && stdin_override=1
  prev="$a"
done
case "$1" in
  buildx)
    if [ "$2" = version ] && [ {buildx_rc} -eq 0 ]; then
      echo "github.com/docker/buildx v0.20.0 shim"; exit 0
    fi
    echo "docker: unknown command: docker buildx" >&2
    exit 1 ;;
  -v|--version) echo "Docker version 99.0.0, build shim"; exit 0 ;;
  version) echo '{{"Client":{{"Version":"99.0.0"}},"Server":{{"Version":"99.0.0"}}}}'; exit 0 ;;
  inspect) echo '[]'; exit 0 ;;
  image)
    if [ "$2" = inspect ]; then
      echo '[{{"Id":"sha256:abc","Config":{{"User":"","Labels":{{}},"Env":[]}},"Architecture":"amd64","Os":"linux"}}]'
    fi
    exit 0 ;;
  compose)
    [ $stdin_override -eq 1 ] && /bin/cat >> "$SHIM_LOG.stdin"
    for a in "$@"; do
      [ "$a" = --services ] && echo app && exit 0
    done
    for a in "$@"; do
      case "$a" in
        config) /bin/cat "$SHIM_COMPOSE_JSON"; exit 0 ;;
        ps) exit 0 ;;
        # Nothing after the build is under test; fail fast instead of letting
        # `up` wait for a container the shim can never start.
        up) exit 1 ;;
      esac
    done
    exit 0 ;;
esac
exit 0
"#
    );
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    Image,
    Dockerfile,
    ComposeImage,
    ComposeBuild,
}

struct Host {
    _root: TempDir,
    workspace: PathBuf,
    path_dir: PathBuf,
    log: PathBuf,
    compose_json: PathBuf,
}

impl Host {
    fn new(shape: Shape, features: bool, buildx_rc: i32) -> Self {
        let root = TempDir::new().unwrap();
        let workspace = root.path().join("ws");
        let path_dir = root.path().join("path");
        let dc = workspace.join(".devcontainer");
        let feat = dc.join("feat");
        for d in [&feat, &path_dir] {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(
            feat.join("devcontainer-feature.json"),
            r#"{ "id": "feat", "version": "1.0.0", "name": "feat" }"#,
        )
        .unwrap();
        fs::write(feat.join("install.sh"), "#!/bin/sh\ntrue\n").unwrap();
        fs::write(dc.join("Dockerfile"), "FROM alpine:3.18\nRUN true\n").unwrap();

        let source = match shape {
            Shape::Image => r#""image": "alpine:3.18""#.to_string(),
            Shape::Dockerfile => r#""build": { "dockerfile": "Dockerfile" }"#.to_string(),
            Shape::ComposeImage | Shape::ComposeBuild => {
                let service = if shape == Shape::ComposeImage {
                    "    image: alpine:3.18\n".to_string()
                } else {
                    "    build:\n      context: .\n      dockerfile: Dockerfile\n".to_string()
                };
                fs::write(
                    dc.join("docker-compose.yml"),
                    format!("services:\n  app:\n{service}    command: sleep infinity\n"),
                )
                .unwrap();
                r#""dockerComposeFile": "docker-compose.yml", "service": "app", "workspaceFolder": "/workspace""#
                    .to_string()
            }
        };
        let features = if features {
            r#", "features": { "./feat": {} }"#
        } else {
            ""
        };
        fs::write(
            dc.join("devcontainer.json"),
            format!(r#"{{ "name": "classic" , {source}{features} }}"#),
        )
        .unwrap();

        // What `docker compose config --format json` would render.
        let service = if shape == Shape::ComposeBuild {
            serde_json::json!({
                "build": { "context": dc, "dockerfile": "Dockerfile" },
                "command": ["sleep", "infinity"]
            })
        } else {
            serde_json::json!({ "image": "alpine:3.18", "command": ["sleep", "infinity"] })
        };
        let compose_json = root.path().join("compose-config.json");
        fs::write(
            &compose_json,
            serde_json::json!({ "name": "shimproj", "services": { "app": service } }).to_string(),
        )
        .unwrap();

        write_shim(&path_dir.join("docker"), buildx_rc);
        let log = root.path().join("invocations.log");
        Self {
            _root: root,
            workspace,
            path_dir,
            log,
            compose_json,
        }
    }

    /// Run `deacon <sub> …`; returns (success, stderr+stdout, recorded argv,
    /// recorded Compose overrides).
    fn run(&self, sub: &str, flags: &[&str]) -> (bool, String, String, String) {
        let mut cmd = Command::cargo_bin("deacon").unwrap();
        cmd.arg(sub)
            .arg("--workspace-folder")
            .arg(&self.workspace)
            .args(flags)
            .env("PATH", format!("{}:/nonexistent", self.path_dir.display()))
            .env_remove("DEACON_CONTAINER_RUNTIME")
            .env_remove("DOCKER_BUILDKIT")
            .env("SHIM_LOG", &self.log)
            .env("SHIM_COMPOSE_JSON", &self.compose_json);
        if sub == "up" {
            cmd.arg("--trust-workspace");
        }
        let output = cmd.output().unwrap();
        let mut said = String::from_utf8_lossy(&output.stderr).into_owned();
        said.push_str(&String::from_utf8_lossy(&output.stdout));
        let stdin_log = PathBuf::from(format!("{}.stdin", self.log.display()));
        (
            output.status.success(),
            said,
            fs::read_to_string(&self.log).unwrap_or_default(),
            fs::read_to_string(stdin_log).unwrap_or_default(),
        )
    }
}

/// Recorded lines whose argv starts with `prefix`, whatever `DOCKER_BUILDKIT` was.
fn argv_lines<'a>(recorded: &'a str, prefix: &str) -> Vec<&'a str> {
    recorded
        .lines()
        .filter(|l| {
            l.split_once(' ')
                .is_some_and(|(_, argv)| argv.starts_with(prefix))
        })
        .collect()
}

const FEATURE_REFUSAL: &str = "BuildKit is required for feature installation";

/// No `buildx build`, and exactly one plain `build` of `-f`-something: the
/// primary build, on the classic builder. With Features, the content image is
/// built first and the Feature stage carries no `--build-context`.
fn assert_classic_primary_build(what: &str, features: bool, said: &str, recorded: &str) {
    assert!(
        !said.contains("unknown command: docker buildx"),
        "{what}: the build still went through buildx:\n{said}\nrecorded:\n{recorded}"
    );
    assert!(
        argv_lines(recorded, "buildx build").is_empty(),
        "{what}: `buildx build` was invoked on a docker without buildx:\n{recorded}"
    );
    let content = argv_lines(recorded, "build -t deacon-feature-content:");
    let primary: Vec<&str> = argv_lines(recorded, "build -f ");
    assert_eq!(
        primary.len(),
        1,
        "{what}: expected one plain `build` as the primary invocation:\n{recorded}"
    );
    assert!(
        !primary[0].contains("--build-context"),
        "{what}: the classic builder has no build contexts:\n{}",
        primary[0]
    );
    assert!(
        !primary[0].starts_with("BK=1 "),
        "{what}: `DOCKER_BUILDKIT=1` on a `docker build` with no buildx fails outright:\n{}",
        primary[0]
    );
    if features {
        assert_eq!(
            content.len(),
            1,
            "{what}: the Feature content image was not built:\n{recorded}\n{said}"
        );
        assert!(
            primary[0].contains("--target dev_containers_target_stage"),
            "{what}: the primary build does not stop at the Feature stage:\n{}",
            primary[0]
        );
    } else {
        assert!(
            content.is_empty(),
            "{what}: a content image was built for a configuration with no Features:\n{recorded}"
        );
    }
}

/// `deacon build` on a docker whose `buildx version` fails runs the primary build
/// with plain `build`, on both single-container shapes, with or without
/// Features, as the reference does. It used to run `buildx build` and fail.
#[test]
fn build_without_buildx_runs_the_classic_builder() {
    for shape in [Shape::Image, Shape::Dockerfile] {
        for features in [false, true] {
            let host = Host::new(shape, features, 1);
            let (_, said, recorded, _) = host.run("build", &[]);
            let what = format!("{shape:?} features={features}");
            assert_eq!(
                argv_lines(&recorded, "buildx version").len(),
                1,
                "{what}: the builder is resolved once per build:\n{recorded}"
            );
            assert_classic_primary_build(&what, features, &said, &recorded);
        }
    }
}

/// `--buildkit auto` asks for BuildKit only where it is available. Without buildx
/// the build must not be handed `DOCKER_BUILDKIT=1`, which would make the plain
/// `docker build` fail with "the buildx component is missing".
#[test]
fn build_buildkit_auto_without_buildx_does_not_force_buildkit_on() {
    let host = Host::new(Shape::Image, true, 1);
    let (_, said, recorded, _) = host.run("build", &["--buildkit", "auto"]);
    assert_classic_primary_build("auto", true, &said, &recorded);
}

/// Where buildx answers, nothing changes: the primary build is `buildx build`
/// with the Feature content in a named build context.
#[test]
fn build_with_buildx_keeps_the_buildx_invocation() {
    let host = Host::new(Shape::Image, true, 0);
    let (_, _, recorded, _) = host.run("build", &[]);
    let buildx = argv_lines(&recorded, "buildx build");
    assert_eq!(buildx.len(), 1, "expected one `buildx build`:\n{recorded}");
    assert!(
        buildx[0].contains("--build-context dev_containers_feature_content_source="),
        "the BuildKit form carries the Feature content as a build context:\n{}",
        buildx[0]
    );
    assert!(
        argv_lines(&recorded, "build -t deacon-feature-content:").is_empty(),
        "no content image on the BuildKit form:\n{recorded}"
    );
}

/// The flags the classic builder cannot honour are refused before anything is
/// built, and the refusal names the missing buildx rather than advising the user
/// to "enable BuildKit". `--platform` and `--push` are the reference's own
/// refusal set; `--output` and `--cache-to` are refused where the reference
/// drops them silently (`bhv-build-classic-builder-export-flags-refused`).
#[test]
fn build_without_buildx_refuses_buildkit_only_flags_naming_the_cause() {
    for flag in [
        &["--platform", "linux/amd64"][..],
        &["--push", "--image-name", "localhost:5999/x:1"][..],
        &["--output", "type=tar,dest=/tmp/x.tar"][..],
        &["--cache-to", "type=inline"][..],
    ] {
        let host = Host::new(Shape::Image, false, 1);
        let (success, said, recorded, _) = host.run("build", flag);
        assert!(!success, "{flag:?}: must refuse; recorded:\n{recorded}");
        assert!(
            said.contains(&format!("BuildKit is required for {}", flag[0]))
                && said.contains("buildx version` failed"),
            "{flag:?}: expected a refusal naming the missing buildx:\n{said}"
        );
        assert!(
            argv_lines(&recorded, "build ").is_empty()
                && argv_lines(&recorded, "buildx build").is_empty(),
            "{flag:?}: a build ran despite the refusal:\n{recorded}"
        );
    }
}

/// A Compose `image:` service with Features on a docker without buildx is built
/// the single-container classic way, on `up` and on `build`. Both refused before.
#[test]
fn compose_image_service_installs_features_without_buildx() {
    for sub in ["up", "build"] {
        let host = Host::new(Shape::ComposeImage, true, 1);
        let (_, said, recorded, _) = host.run(sub, &[]);
        let what = format!("{sub} compose image:");
        assert!(
            !said.contains(FEATURE_REFUSAL),
            "{what}: refused instead of built:\n{said}\nrecorded:\n{recorded}"
        );
        assert_classic_primary_build_feature_stage(&what, &recorded);
    }
}

/// The image-arm Feature build's two classic invocations.
fn assert_classic_primary_build_feature_stage(what: &str, recorded: &str) {
    assert!(
        argv_lines(recorded, "buildx build").is_empty(),
        "{what}: `buildx build` was invoked:\n{recorded}"
    );
    assert_eq!(
        argv_lines(recorded, "build -t deacon-feature-content:").len(),
        1,
        "{what}: the Feature content image was not built:\n{recorded}"
    );
    let stage: Vec<&str> = argv_lines(recorded, "build ")
        .into_iter()
        .filter(|l| l.contains("--target dev_containers_target_stage"))
        .collect();
    assert_eq!(
        stage.len(),
        1,
        "{what}: expected one plain `build` of the Feature stage:\n{recorded}"
    );
    assert!(
        !stage[0].contains("--build-context"),
        "{what}: the classic builder has no build contexts:\n{}",
        stage[0]
    );
}

/// A Compose `build:` service with Features on a docker without buildx is built
/// by Compose, which does not need buildx: its own BuildKit client serves the
/// named build contexts. Nothing is refused and BuildKit is not switched off.
#[test]
fn compose_build_service_installs_features_without_buildx() {
    for sub in ["up", "build"] {
        let host = Host::new(Shape::ComposeBuild, true, 1);
        let (_, said, recorded, overrides) = host.run(sub, &[]);
        let what = format!("{sub} compose build:");
        assert!(
            !said.contains(FEATURE_REFUSAL),
            "{what}: refused instead of built:\n{said}\nrecorded:\n{recorded}"
        );
        let builds: Vec<&str> = argv_lines(&recorded, "compose ")
            .into_iter()
            .filter(|l| l.ends_with(" build app"))
            .collect();
        assert_eq!(
            builds.len(),
            1,
            "{what}: expected one `compose … build app`:\n{recorded}\n{said}"
        );
        assert!(
            builds[0].starts_with("BK=unset "),
            "{what}: Compose must keep its own BuildKit here:\n{}",
            builds[0]
        );
        assert!(
            overrides.contains("additional_contexts")
                && overrides.contains("dev_containers_feature_content_source"),
            "{what}: the override must carry the Feature content as a named context:\n{overrides}"
        );
        assert!(
            argv_lines(&recorded, "build -t deacon-feature-content:").is_empty(),
            "{what}: no content image when Compose has BuildKit:\n{recorded}"
        );
    }
}

/// `--buildkit never` on a Compose configuration is honoured rather than ignored:
/// every Compose invocation runs with `DOCKER_BUILDKIT=0`, the switch Compose
/// reads to build with the classic builder, and the Feature content of a
/// `build:` service is staged as an image, since that builder has no named build
/// contexts. An `image:` service takes the single-container classic route.
#[test]
fn compose_buildkit_never_builds_on_the_classic_builder() {
    for sub in ["up", "build"] {
        // `build:` service.
        let host = Host::new(Shape::ComposeBuild, true, 0);
        let (_, said, recorded, overrides) = host.run(sub, &["--buildkit", "never"]);
        let what = format!("{sub} --buildkit never compose build:");
        let compose: Vec<&str> = argv_lines(&recorded, "compose ");
        assert!(!compose.is_empty(), "{what}: Compose never ran:\n{said}");
        assert!(
            compose.iter().all(|l| l.starts_with("BK=0 ")),
            "{what}: every Compose invocation must carry DOCKER_BUILDKIT=0:\n{recorded}"
        );
        assert!(
            compose.iter().any(|l| l.ends_with(" build app")),
            "{what}: the service was not built by Compose:\n{recorded}"
        );
        assert_eq!(
            argv_lines(&recorded, "build -t deacon-feature-content:").len(),
            1,
            "{what}: the Feature content image was not built:\n{recorded}\n{said}"
        );
        assert!(
            !overrides.contains("additional_contexts"),
            "{what}: the classic builder has no named build contexts:\n{overrides}"
        );
        assert!(
            argv_lines(&recorded, "buildx build").is_empty(),
            "{what}: `buildx build` was invoked:\n{recorded}"
        );

        // `image:` service.
        let host = Host::new(Shape::ComposeImage, true, 0);
        let (_, _, recorded, _) = host.run(sub, &["--buildkit", "never"]);
        assert_classic_primary_build_feature_stage(
            &format!("{sub} --buildkit never compose image:"),
            &recorded,
        );
    }
}

/// The same `--buildkit never` with no Features: Compose still builds on the
/// classic builder rather than silently on BuildKit.
#[test]
fn compose_buildkit_never_without_features_still_switches_compose_off_buildkit() {
    let host = Host::new(Shape::ComposeBuild, false, 0);
    let (_, said, recorded, _) = host.run("build", &["--buildkit", "never"]);
    let builds: Vec<&str> = argv_lines(&recorded, "compose ")
        .into_iter()
        .filter(|l| l.ends_with(" build app"))
        .collect();
    assert_eq!(builds.len(), 1, "no Compose build:\n{said}\n{recorded}");
    assert!(
        builds[0].starts_with("BK=0 "),
        "`--buildkit never` was ignored on a Compose build:\n{}",
        builds[0]
    );
}
