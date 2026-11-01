//! `deacon up` installs Features without BuildKit, as the reference does (#783).
//!
//! The reference (`addContainerFeatures`, 0.89.0) has no BuildKit version when
//! `<docker-path> buildx version` fails or `--buildkit never` is passed. It then
//! builds the Feature content into an image and builds the Feature stage with plain
//! `docker build`, reading the content with `COPY --from=`. deacon refused the first
//! case (`BuildKit is required for feature installation`) and ignored the second.
//! MEASURED with the shim below before the fix: deacon refused both single-container
//! shapes, and the reference built both with the marker in the image.
//!
//! The shim fails `buildx version`, records every argv and forwards everything else
//! to the real docker. The assertions are on the IMAGE: the Feature's marker must be
//! in it, which `outcome: success` alone does not prove.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use serde_json::Value;
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

/// The real docker binary the shim forwards to.
fn real_docker() -> String {
    let out = StdCommand::new("sh")
        .args(["-c", "command -v docker"])
        .output()
        .expect("locate docker");
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert!(!path.is_empty(), "docker is not on PATH");
    path
}

/// A recording pass-through shim. With `fail_buildx`, `buildx version` exits 1 as
/// it does on an engine without the plugin.
fn write_shim(path: &Path, log: &Path, fail_buildx: bool) {
    let buildx = if fail_buildx {
        "if [ \"$1\" = buildx ] && [ \"$2\" = version ]; then echo 'buildx: not installed' >&2; exit 1; fi\n"
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

#[derive(Clone, Copy, Debug)]
enum Shape {
    Image,
    Dockerfile,
}

/// Removes the container and the image `up` produced, whatever the test's outcome.
struct Cleanup {
    container: Option<String>,
    image: Option<String>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(id) = &self.container {
            let _ = StdCommand::new("docker").args(["rm", "-f", id]).output();
        }
        if let Some(image) = &self.image {
            let _ = StdCommand::new("docker")
                .args(["image", "rm", "-f", image])
                .output();
        }
    }
}

struct Fixture {
    _root: TempDir,
    workspace: PathBuf,
    shim: PathBuf,
    log: PathBuf,
}

impl Fixture {
    fn new(shape: Shape, fail_buildx: bool) -> Self {
        let root = TempDir::new().unwrap();
        // Lowercase on purpose: Compose and the cleanup sweeps key on it (#442).
        let workspace = root.path().join("ws");
        let dc = workspace.join(".devcontainer");
        let feat = dc.join("markerfeat");
        fs::create_dir_all(&feat).unwrap();
        let source = match shape {
            Shape::Image => serde_json::json!({ "image": "alpine:3.18" }),
            Shape::Dockerfile => {
                fs::write(
                    dc.join("Dockerfile"),
                    "FROM alpine:3.18\nRUN echo base > /etc/base-marker\n",
                )
                .unwrap();
                serde_json::json!({ "build": { "dockerfile": "Dockerfile" } })
            }
        };
        let mut config = serde_json::json!({
            "name": "features without buildkit",
            "features": { "./markerfeat": {} }
        });
        for (k, v) in source.as_object().unwrap() {
            config[k] = v.clone();
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
            "#!/bin/sh\nset -e\nmkdir -p /usr/local/share\necho installed-783 > /usr/local/share/marker-783\n",
        )
        .unwrap();
        fs::set_permissions(feat.join("install.sh"), fs::Permissions::from_mode(0o755)).unwrap();

        let log = root.path().join("docker.log");
        let shim = root.path().join("docker-shim");
        write_shim(&shim, &log, fail_buildx);
        Self {
            _root: root,
            workspace,
            shim,
            log,
        }
    }

    /// Run `up`, register cleanup, and return the image the container runs plus
    /// every recorded docker argv.
    fn up(&self, extra: &[&str], cleanup: &mut Cleanup) -> (String, String) {
        let output = Command::cargo_bin("deacon")
            .unwrap()
            .env("DEACON_LOG", "warn")
            .env_remove("DOCKER_BUILDKIT")
            // The shim forwards to docker; a podman lane's runtime must not override it.
            .env_remove("DEACON_CONTAINER_RUNTIME")
            .args([
                "up",
                "--workspace-folder",
                self.workspace.to_str().unwrap(),
                "--trust-workspace",
                "--mount-workspace-git-root=false",
                "--remove-existing-container",
                "--docker-path",
                self.shim.to_str().unwrap(),
            ])
            .args(extra)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let recorded = fs::read_to_string(&self.log).unwrap_or_default();

        let value: Option<Value> = serde_json::from_str(stdout.trim()).ok();
        if let Some(id) = value
            .as_ref()
            .and_then(|v| v["containerId"].as_str())
            .filter(|id| !id.is_empty())
        {
            cleanup.container = Some(id.to_string());
        }
        assert!(
            output.status.success(),
            "up failed\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}\nRECORDED:\n{recorded}"
        );
        let container = cleanup
            .container
            .clone()
            .unwrap_or_else(|| panic!("no containerId in:\n{stdout}"));

        let inspect = StdCommand::new("docker")
            .args(["inspect", "-f", "{{.Config.Image}}", &container])
            .output()
            .unwrap();
        let image = String::from_utf8_lossy(&inspect.stdout).trim().to_string();
        assert!(!image.is_empty(), "could not read the container's image");
        cleanup.image = Some(image.clone());
        (image, recorded)
    }
}

/// `docker run --rm <image> cat <path>`, trimmed.
fn cat_in_image(image: &str, path: &str) -> String {
    let out = StdCommand::new("docker")
        .args(["run", "--rm", image, "cat", path])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "`cat {path}` failed in {image}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn assert_built_classically(shape: Shape, image: &str, recorded: &str) {
    assert_eq!(
        cat_in_image(image, "/usr/local/share/marker-783"),
        "installed-783",
        "{shape:?}: the Feature is not in the produced image"
    );
    if matches!(shape, Shape::Dockerfile) {
        assert_eq!(
            cat_in_image(image, "/etc/base-marker"),
            "base",
            "{shape:?}: the Feature stage did not build on the user's Dockerfile"
        );
    }
    assert!(
        !recorded.lines().any(|l| l.starts_with("buildx build")),
        "{shape:?}: a build went through buildx:\n{recorded}"
    );
    assert!(
        recorded
            .lines()
            .any(|l| l.starts_with("build -t deacon-feature-content:")),
        "{shape:?}: the Feature content image was not built:\n{recorded}"
    );
    assert!(
        recorded.lines().any(|l| l.starts_with("build ")
            && l.contains("--target dev_containers_target_stage")
            && !l.contains("--build-context")),
        "{shape:?}: the Feature stage was not built with plain `build`:\n{recorded}"
    );
}

#[test]
fn up_installs_features_when_the_docker_has_no_buildx() {
    if !is_docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }
    for shape in [Shape::Image, Shape::Dockerfile] {
        let fixture = Fixture::new(shape, true);
        let mut cleanup = Cleanup {
            container: None,
            image: None,
        };
        let (image, recorded) = fixture.up(&[], &mut cleanup);
        assert_built_classically(shape, &image, &recorded);
    }
}

#[test]
fn up_buildkit_never_installs_features_with_plain_docker_build() {
    if !is_docker_available() {
        eprintln!("Skipping: Docker not available");
        return;
    }
    for shape in [Shape::Image, Shape::Dockerfile] {
        let fixture = Fixture::new(shape, false);
        let mut cleanup = Cleanup {
            container: None,
            image: None,
        };
        let (image, recorded) = fixture.up(&["--buildkit", "never"], &mut cleanup);
        assert_built_classically(shape, &image, &recorded);
    }
}
