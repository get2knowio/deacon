//! Integration coverage for the with-features example to ensure BuildKit feature
//! installs work across the documented scenarios.

use assert_cmd::Command;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

const WORKSPACE: &str = "examples/up/with-features";

#[test]
fn test_up_with_features_basic() {
    if !docker_available() {
        eprintln!("Skipping: docker not available");
        return;
    }

    let guard = ContainerGuard::new();
    let (container_id, image_tag) = run_up(&[], &guard);
    assert!(
        image_tag.starts_with("deacon-devcontainer-features:"),
        "expected feature-extended image tag, got {image_tag}"
    );
    guard.register(container_id);
}

#[test]
fn test_up_with_additional_features() {
    if !docker_available() {
        eprintln!("Skipping: docker not available");
        return;
    }

    let guard = ContainerGuard::new();
    let (container_id, image_tag) = run_up(
        &[
            "--additional-features",
            r#"{"ghcr.io/devcontainers/features/docker-in-docker:2":{"version":"latest"}}"#,
        ],
        &guard,
    );
    assert!(
        image_tag.starts_with("deacon-devcontainer-features:"),
        "expected feature-extended image tag, got {image_tag}"
    );
    guard.register(container_id);
}

#[test]
fn test_up_with_skip_feature_auto_mapping() {
    if !docker_available() {
        eprintln!("Skipping: docker not available");
        return;
    }

    let guard = ContainerGuard::new();
    let (container_id, image_tag) = run_up(&["--skip-feature-auto-mapping"], &guard);
    assert!(
        image_tag.starts_with("deacon-devcontainer-features:"),
        "expected feature-extended image tag, got {image_tag}"
    );
    guard.register(container_id);
}

fn run_up(extra_args: &[&str], guard: &ContainerGuard) -> (String, String) {
    let workspace = workspace_path();

    let mut cmd = Command::cargo_bin("deacon").expect("deacon binary");
    let assert = cmd
        .current_dir(workspace)
        .env("DEACON_LOG", "warn")
        .args([
            "up",
            "--workspace-folder",
            ".",
            "--remove-existing-container",
            "--skip-post-create",
        ])
        .args(extra_args)
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let trimmed = stdout.trim();
    let value: Value = serde_json::from_str::<Value>(trimmed)
        .ok()
        .or_else(|| {
            trimmed
                .rfind('{')
                .and_then(|idx| serde_json::from_str::<Value>(&trimmed[idx..]).ok())
        })
        .unwrap_or_else(|| panic!("valid JSON output\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}"));
    let container_id = value["containerId"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        !container_id.is_empty(),
        "expected containerId in output: {value:?}"
    );

    // Inspect the container to capture the image tag used.
    let inspect_output = StdCommand::new("docker")
        .args(["inspect", "-f", "{{.Config.Image}}", &container_id])
        .output()
        .expect("docker inspect");
    let image_tag = String::from_utf8_lossy(&inspect_output.stdout)
        .trim()
        .to_string();
    assert!(
        !image_tag.is_empty(),
        "docker inspect returned empty image tag for {container_id}"
    );

    // Track for cleanup.
    guard.register(container_id.clone());

    (container_id, image_tag)
}

fn docker_available() -> bool {
    StdCommand::new("docker")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Drop guard to clean up containers created by this test module.
struct ContainerGuard {
    container_ids: std::cell::RefCell<Vec<String>>,
}

impl ContainerGuard {
    fn new() -> Self {
        Self {
            container_ids: std::cell::RefCell::new(Vec::new()),
        }
    }

    fn register(&self, id: String) {
        if !id.is_empty() {
            self.container_ids.borrow_mut().push(id);
        }
    }
}

fn workspace_path() -> PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = crate_dir
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    workspace_root.join(WORKSPACE)
}

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        for id in self.container_ids.borrow().iter() {
            let _ = StdCommand::new("docker").args(["rm", "-f", id]).output();
        }
    }
}
