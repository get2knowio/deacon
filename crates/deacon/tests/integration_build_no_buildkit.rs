//! `deacon build` produces the configured image, Features installed, when the
//! selected Docker has no buildx or BuildKit is switched off — and a Compose
//! configuration's Features install on `up` and `build` alike.
//!
//! MEASURED before this fix on docker 29.9.2 with the buildx plugin removed:
//! every image-reference and Dockerfile `build` failed with
//! `docker: unknown command: docker buildx`, and every Compose configuration with
//! Features was refused with `BuildKit is required for feature installation`. The
//! reference (0.89.0) built all of them, with the Feature in the image.
//!
//! "No buildx" is simulated by a pass-through shim whose `buildx` subcommand
//! fails, which is what deacon's probe and any `buildx build` see on such an
//! engine. Compose still finds the real plugin; it does not route through the
//! shim's `buildx`, and needs no buildx anyway. The assertions are on the IMAGE:
//! the Feature's marker must be in it, which `outcome: success` does not prove.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use tempfile::TempDir;

fn is_docker_available() -> bool {
    StdCommand::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn real_docker() -> String {
    let out = StdCommand::new("sh")
        .args(["-c", "command -v docker"])
        .output()
        .expect("locate docker");
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert!(!path.is_empty(), "docker is not on PATH");
    path
}

/// A recording pass-through shim. With `no_buildx`, every `buildx` subcommand
/// fails as it does on an engine without the plugin.
fn write_shim(path: &Path, log: &Path, no_buildx: bool) {
    let buildx = if no_buildx {
        "if [ \"$1\" = buildx ]; then echo 'docker: unknown command: docker buildx' >&2; exit 1; fi\n"
    } else {
        ""
    };
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{log}'\n{buildx}exec '{real}' \"$@\"\n",
        log = log.display(),
        real = real_docker(),
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

impl Shape {
    fn builds_a_dockerfile(self) -> bool {
        matches!(self, Shape::Dockerfile | Shape::ComposeBuild)
    }
}

const MARKER: &str = "/usr/local/share/marker-nobk";
const BASE_MARKER: &str = "/etc/base-marker";

struct Fixture {
    root: TempDir,
    workspace: PathBuf,
    shim: PathBuf,
    log: PathBuf,
}

impl Fixture {
    fn new(shape: Shape, features: bool, no_buildx: bool) -> Self {
        let root = TempDir::new().unwrap();
        // Lowercase on purpose: Compose and the cleanup sweeps key on it (#442).
        let workspace = root.path().join("ws");
        let dc = workspace.join(".devcontainer");
        let feat = dc.join("markerfeat");
        fs::create_dir_all(&feat).unwrap();
        fs::write(
            dc.join("Dockerfile"),
            format!("FROM alpine:3.18\nRUN echo base > {BASE_MARKER}\n"),
        )
        .unwrap();
        let mut config = match shape {
            Shape::Image => serde_json::json!({ "image": "alpine:3.18" }),
            Shape::Dockerfile => serde_json::json!({ "build": { "dockerfile": "Dockerfile" } }),
            Shape::ComposeImage | Shape::ComposeBuild => {
                let service = if shape == Shape::ComposeImage {
                    "    image: alpine:3.18\n"
                } else {
                    "    build:\n      context: .\n      dockerfile: Dockerfile\n"
                };
                fs::write(
                    dc.join("docker-compose.yml"),
                    format!("services:\n  app:\n{service}    command: sleep infinity\n"),
                )
                .unwrap();
                serde_json::json!({
                    "dockerComposeFile": "docker-compose.yml",
                    "service": "app",
                    "workspaceFolder": "/workspace"
                })
            }
        };
        config["name"] = "build without buildkit".into();
        if features {
            config["features"] = serde_json::json!({ "./markerfeat": {} });
        }
        fs::write(
            dc.join("devcontainer.json"),
            serde_json::to_string_pretty(&config).unwrap(),
        )
        .unwrap();
        fs::write(
            feat.join("devcontainer-feature.json"),
            r#"{ "id": "markerfeat", "version": "1.0.0", "name": "Marker" }"#,
        )
        .unwrap();
        fs::write(
            feat.join("install.sh"),
            format!("#!/bin/sh\nset -e\nmkdir -p /usr/local/share\necho installed > {MARKER}\n"),
        )
        .unwrap();
        fs::set_permissions(feat.join("install.sh"), fs::Permissions::from_mode(0o755)).unwrap();

        let log = root.path().join("docker.log");
        let shim = root.path().join("docker-shim");
        write_shim(&shim, &log, no_buildx);
        Self {
            root,
            workspace,
            shim,
            log,
        }
    }

    /// A name unique to this fixture, so parallel tests cannot share an image.
    fn image_name(&self) -> String {
        let leaf = self
            .root
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_lowercase()
            .replace(|c: char| !c.is_ascii_alphanumeric(), "");
        format!("deacon-test-nobk/{leaf}:latest")
    }

    /// Run `deacon build --image-name <unique>`; returns that image name and the
    /// recorded docker argv. Panics with everything on a failed build.
    fn build(&self, extra: &[&str], cleanup: &mut Cleanup) -> (String, String) {
        let image = self.image_name();
        cleanup.images.push(image.clone());
        let output = Command::cargo_bin("deacon")
            .unwrap()
            .env("DEACON_LOG", "warn")
            .env_remove("DOCKER_BUILDKIT")
            .env_remove("DEACON_CONTAINER_RUNTIME")
            .args([
                "build",
                "--workspace-folder",
                self.workspace.to_str().unwrap(),
                "--docker-path",
                self.shim.to_str().unwrap(),
                "--image-name",
                &image,
            ])
            .args(extra)
            .output()
            .unwrap();
        let recorded = fs::read_to_string(&self.log).unwrap_or_default();
        assert!(
            output.status.success(),
            "build failed\nSTDOUT:\n{}\nSTDERR:\n{}\nRECORDED:\n{recorded}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        (image, recorded)
    }
}

/// Removes every tag the test produced, and with it the image, whatever the
/// test's outcome. Removing by image ID drops the deterministic `deacon-build:`
/// and Compose tags that point at the same image too.
struct Cleanup {
    images: Vec<String>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        for image in &self.images {
            let id = StdCommand::new("docker")
                .args(["image", "inspect", "-f", "{{.Id}}", image])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            let target = if id.is_empty() { image.clone() } else { id };
            let _ = StdCommand::new("docker")
                .args(["image", "rm", "-f", &target])
                .output();
        }
    }
}

fn cat_in_image(image: &str, path: &str) -> Option<String> {
    let out = StdCommand::new("docker")
        .args(["run", "--rm", image, "cat", path])
        .output()
        .unwrap();
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn assert_image(what: &str, shape: Shape, features: bool, image: &str) {
    let marker = cat_in_image(image, MARKER);
    if features {
        assert_eq!(
            marker.as_deref(),
            Some("installed"),
            "{what}: the Feature is not in the produced image"
        );
    } else {
        assert_eq!(marker, None, "{what}: a Feature marker with no Feature");
    }
    if shape.builds_a_dockerfile() {
        assert_eq!(
            cat_in_image(image, BASE_MARKER).as_deref(),
            Some("base"),
            "{what}: the image was not built from the configuration's Dockerfile"
        );
    }
}

fn assert_no_buildx_build(what: &str, recorded: &str) {
    assert!(
        !recorded.lines().any(|l| l.starts_with("buildx build")),
        "{what}: a build went through buildx:\n{recorded}"
    );
}

/// Single-container shapes, with and without Features, on a docker without
/// buildx: plain `docker build`, as in the reference.
#[test]
fn build_without_buildx_produces_the_image() {
    if !is_docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }
    for shape in [Shape::Image, Shape::Dockerfile] {
        for features in [false, true] {
            let what = format!("{shape:?} features={features}");
            let fixture = Fixture::new(shape, features, true);
            let mut cleanup = Cleanup { images: Vec::new() };
            let (image, recorded) = fixture.build(&[], &mut cleanup);
            assert_no_buildx_build(&what, &recorded);
            assert_image(&what, shape, features, &image);
        }
    }
}

/// Compose shapes with Features on a docker without buildx: built, not refused.
#[test]
fn build_compose_features_without_buildx_installs_the_feature() {
    if !is_docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }
    for shape in [Shape::ComposeImage, Shape::ComposeBuild] {
        let what = format!("{shape:?}");
        let fixture = Fixture::new(shape, true, true);
        let mut cleanup = Cleanup { images: Vec::new() };
        let (image, recorded) = fixture.build(&[], &mut cleanup);
        assert_no_buildx_build(&what, &recorded);
        assert_image(&what, shape, true, &image);
    }
}

/// `--buildkit never` on Compose with Features is honoured on the classic
/// builder, and produces the image. The reference fails this on buildx >= 0.37
/// (`bhv-compose-buildkit-never-classic-builder`); deacon must not.
#[test]
fn build_compose_buildkit_never_installs_the_feature_on_the_classic_builder() {
    if !is_docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }
    for shape in [Shape::ComposeImage, Shape::ComposeBuild] {
        let what = format!("{shape:?} --buildkit never");
        let fixture = Fixture::new(shape, true, false);
        let mut cleanup = Cleanup { images: Vec::new() };
        let (image, recorded) = fixture.build(&["--buildkit", "never"], &mut cleanup);
        assert_no_buildx_build(&what, &recorded);
        assert!(
            recorded
                .lines()
                .any(|l| l.starts_with("build -t deacon-feature-content:")),
            "{what}: the Feature content was not staged as an image:\n{recorded}"
        );
        assert_image(&what, shape, true, &image);
        if shape == Shape::ComposeBuild {
            let builder = StdCommand::new("docker")
                .args([
                    "image",
                    "inspect",
                    "-f",
                    "{{index .Config.Labels \"com.docker.compose.image.builder\"}}",
                    &image,
                ])
                .output()
                .unwrap();
            assert_eq!(
                String::from_utf8_lossy(&builder.stdout).trim(),
                "classic",
                "{what}: Compose did not build on the classic builder"
            );
        }
    }
}
