//! Integration test for real-time `[key]` prefixing of parallel lifecycle
//! command output (T032, issue #138).
//!
//! An object-form lifecycle command runs its entries concurrently; each entry's
//! output is now forwarded to deacon's stderr prefixed with `[key]` so the
//! interleaved streams stay attributable.
//!
//! Requires Docker; self-skips when unavailable.
#![cfg(unix)]

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

fn is_docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

struct ContainerGuard(String);
impl Drop for ContainerGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", &self.0])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

fn find_container(name: &str) -> Option<String> {
    let output = std::process::Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            "label=devcontainer.source=deacon",
            "--filter",
            &format!("label=devcontainer.name={}", name),
            "--format",
            "{{.ID}}",
        ])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

#[test]
fn test_parallel_lifecycle_output_is_key_prefixed() {
    if !is_docker_available() {
        eprintln!("Skipping test_parallel_lifecycle_output_is_key_prefixed: Docker not available");
        return;
    }

    let name = "Deacon Parallel Output Test";
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join(".devcontainer")).unwrap();
    fs::write(
        root.join(".devcontainer/devcontainer.json"),
        format!(
            r#"{{
  "name": "{name}",
  "image": "debian:bookworm-slim",
  "remoteUser": "root",
  "workspaceFolder": "/workspace",
  "workspaceMount": "source=${{localWorkspaceFolder}},target=/workspace,type=bind",
  "postCreateCommand": {{
    "alpha": "echo hello-from-alpha",
    "beta": "echo hello-from-beta"
  }}
}}"#
        ),
    )
    .unwrap();

    let output = Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(root)
        .arg("up")
        .arg("--workspace-folder")
        .arg(root)
        .arg("--remove-existing-container")
        .env("DEACON_LOG", "warn")
        .output()
        .expect("spawn deacon up");

    // Clean up the container regardless of assertion outcome.
    let _guard = find_container(name).map(ContainerGuard);

    assert!(
        output.status.success(),
        "deacon up failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Lifecycle output is forwarded to stderr; each parallel entry's line must
    // carry its `[key]` prefix.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[alpha] hello-from-alpha"),
        "expected '[alpha] hello-from-alpha' in stderr; got:\n{}",
        stderr
    );
    assert!(
        stderr.contains("[beta] hello-from-beta"),
        "expected '[beta] hello-from-beta' in stderr; got:\n{}",
        stderr
    );
}
