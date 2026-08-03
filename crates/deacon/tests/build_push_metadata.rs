//! `deacon build --push` must publish an image that carries `devcontainer.metadata`
//! (#440).
//!
//! This lives in an integration test rather than the declarative parity suite because
//! the claim needs a REGISTRY to be observable at all: the label has to be read back
//! off the image a registry received, not off a local tag that happens to share a name.
//! The parity harness expresses fixtures and argv, not service dependencies, and
//! teaching it about registries would be new machinery for one case — so the registry
//! lives here, in a throwaway `registry:2` container on an ephemeral port.
//!
//! What it proves that a local-tag assertion could not: the metadata survives the
//! push. deacon builds into the daemon, stamps the label, and pushes afterwards; a
//! regression that went back to handing the push straight to BuildKit would publish an
//! unstamped image while every local assertion still passed.

use assert_cmd::Command;
use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::process::Command as StdCommand;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Whether the failure is just "Docker isn't available here" (so the test's real
/// assertion doesn't apply). Mirrors `integration_build_output.rs`.
fn is_docker_unavailable(stderr: &str) -> bool {
    let lc = stderr.to_lowercase();
    stderr.contains("Docker is not installed")
        || stderr.contains("Docker daemon is not")
        || lc.contains("permission denied")
        || lc.contains("cannot connect to the docker daemon")
}

fn docker_available() -> bool {
    StdCommand::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Removes everything the test created, on every exit path (`?`, panic, unwind).
struct DockerCleanup {
    registry: String,
    images: Vec<String>,
}

impl Drop for DockerCleanup {
    fn drop(&mut self) {
        let _ = StdCommand::new("docker")
            .args(["rm", "-f", &self.registry])
            .output();
        for image in &self.images {
            let _ = StdCommand::new("docker")
                .args(["rmi", "-f", image])
                .output();
        }
    }
}

/// Start a throwaway registry on an ephemeral host port and return that port.
///
/// The port is assigned by the daemon (`-p 127.0.0.1:0:5000`) rather than fixed, so
/// this test never collides with a concurrently running sibling in the same shared
/// Docker group.
fn start_registry(name: &str) -> Option<u16> {
    let run = StdCommand::new("docker")
        .args([
            "run",
            "-d",
            "--rm",
            "-p",
            "127.0.0.1:0:5000",
            "--name",
            name,
            "registry:2",
        ])
        .output()
        .ok()?;
    if !run.status.success() {
        eprintln!(
            "skipping: could not start the registry: {}",
            String::from_utf8_lossy(&run.stderr).trim()
        );
        return None;
    }

    let ports = StdCommand::new("docker")
        .args(["port", name, "5000/tcp"])
        .output()
        .ok()?;
    let mapping = String::from_utf8_lossy(&ports.stdout);
    let port: u16 = mapping.lines().find_map(|line| {
        line.rsplit_once(':')
            .and_then(|(_, p)| p.trim().parse().ok())
    })?;

    // Wait for the registry to accept connections before pushing at it.
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&addr, Duration::from_secs(1)).is_ok() {
            return Some(port);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    eprintln!("skipping: the registry never accepted connections on port {port}");
    None
}

fn inspect_label(image: &str, label: &str) -> String {
    let out = StdCommand::new("docker")
        .args([
            "inspect",
            image,
            "--format",
            &format!("{{{{index .Config.Labels \"{label}\"}}}}"),
        ])
        .output()
        .expect("docker inspect must run");
    assert!(
        out.status.success(),
        "docker inspect {image} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn push_publishes_an_image_carrying_devcontainer_metadata() {
    if !docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }

    // Unique per run so concurrent runs in the shared group never share a name.
    let unique = format!("{}", std::process::id());
    let registry_name = format!("deacon-test-registry-{unique}");
    let Some(port) = start_registry(&registry_name) else {
        // `start_registry` already explained why; the registry it may have created is
        // removed here.
        let _ = StdCommand::new("docker")
            .args(["rm", "-f", &registry_name])
            .output();
        return;
    };
    let pushed = format!("localhost:{port}/deacon-push-metadata:{unique}");
    let _cleanup = DockerCleanup {
        registry: registry_name,
        images: vec![pushed.clone()],
    };

    let temp_dir = TempDir::new().unwrap();
    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/Dockerfile"),
        "FROM alpine:3.19\nLABEL deacon.test=build-push-metadata\n",
    )
    .unwrap();
    // `remoteUser` and `containerEnv` are metadata properties, so the config
    // contributes an entry; a config of only `name`/`build` would contribute none and
    // the assertion would be vacuous.
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        r#"{
    "name": "Build Push Metadata",
    "build": { "dockerfile": "Dockerfile" },
    "remoteUser": "root",
    "containerEnv": { "DEACON_PUSH_METADATA": "authored-in-config" }
}
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .args(["build", "--push", "--image-name", &pushed])
        .args(["--output-format", "json"])
        .assert();
    let output = assert.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        assert!(
            is_docker_unavailable(&stderr),
            "unexpected `build --push` failure (docker available): {stderr}"
        );
        eprintln!("skipping: docker unavailable ({})", stderr.trim());
        return;
    }

    // Drop the local copy so the inspect below can only be reading what the REGISTRY
    // holds — the whole point of pushing to one.
    let _ = StdCommand::new("docker")
        .args(["rmi", "-f", &pushed])
        .output();
    let pull = StdCommand::new("docker")
        .args(["pull", "-q", &pushed])
        .output()
        .expect("docker pull must run");
    assert!(
        pull.status.success(),
        "the pushed image must be pullable from the registry: {}",
        String::from_utf8_lossy(&pull.stderr)
    );

    let metadata = inspect_label(&pushed, "devcontainer.metadata");
    assert_eq!(
        metadata,
        r#"[{"containerEnv":{"DEACON_PUSH_METADATA":"authored-in-config"},"remoteUser":"root"}]"#,
        "the PUSHED image must carry the same `devcontainer.metadata` the local build \
         shape stamps (#440); stderr:\n{stderr}"
    );
}
