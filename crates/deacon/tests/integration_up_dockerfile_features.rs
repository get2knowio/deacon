//! Integration test: `up` with a Dockerfile-built base AND features.
//!
//! Regression guard for the bare-digest `FROM` bug: when `up` builds an image
//! from a `build.dockerfile` (no `image`) and the config also declares features,
//! deacon used to feed the freshly-built image's bare `sha256:<digest>` ID into
//! the feature-install Dockerfile as `FROM sha256:...`. BuildKit resolves a bare
//! digest as a `docker.io/library/sha256:...` repository → pull-access-denied /
//! 404, so feature layering failed for every Dockerfile+features config.
//!
//! The fix tags the Dockerfile build with a real `deacon-build:<hash>` repo:tag
//! (mirroring `deacon build`) so the downstream `FROM` resolves to the local
//! image. This test exercises the whole path end-to-end: a multi-stage
//! Dockerfile (build arg + `target`) plus a local feature, asserting the
//! resulting container carries BOTH the Dockerfile-stage markers and the
//! feature marker (i.e. features layered successfully on the built base).

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::process::Command as StdCommand;
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

/// Removes the container on drop so the test leaves no residue.
struct ContainerGuard(std::cell::RefCell<Option<String>>);

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        if let Some(id) = self.0.borrow().as_ref() {
            let _ = StdCommand::new("docker").args(["rm", "-f", id]).output();
        }
    }
}

/// `docker exec <cid> cat <path>` → trimmed stdout (empty string on failure).
fn exec_cat(container_id: &str, path: &str) -> String {
    let output = StdCommand::new("docker")
        .args(["exec", container_id, "cat", path])
        .output()
        .expect("docker exec");
    if !output.status.success() {
        return String::new();
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn test_up_dockerfile_build_with_features_layers_on_built_base() {
    if !is_docker_available() {
        eprintln!("Skipping test_up_dockerfile_build_with_features: Docker not available");
        return;
    }

    // TempDir lives under the system temp dir (outside the repo) — required for
    // `up` tests, which chown the workspace and would otherwise touch the repo.
    let temp_dir = TempDir::new().unwrap();
    let dc = temp_dir.path().join(".devcontainer");
    fs::create_dir_all(&dc).unwrap();

    // Multi-stage Dockerfile with a build arg and a non-default `target`.
    // Uses debian:bookworm-slim (light, commonly cached) as the base.
    let dockerfile = r#"FROM debian:bookworm-slim AS base
ARG MARKER=unset
RUN echo "${MARKER}" > /etc/dockerfile-marker
FROM base AS dev
RUN echo dev > /etc/stage-marker
"#;
    fs::write(dc.join("Dockerfile"), dockerfile).unwrap();

    // Local feature that drops a marker file — keeps the test hermetic (no
    // network) while still exercising the feature-layering build path.
    let feat = dc.join("markerfeat");
    fs::create_dir_all(&feat).unwrap();
    fs::write(
        feat.join("devcontainer-feature.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "id": "markerfeat",
            "version": "1.0.0",
            "name": "Marker Feature"
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        feat.join("install.sh"),
        "#!/bin/sh\nset -e\nmkdir -p /usr/local/share\necho layered > /usr/local/share/feature-marker\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(feat.join("install.sh")).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(feat.join("install.sh"), perms).unwrap();
    }

    let config = serde_json::json!({
        "name": "Dockerfile build + features",
        "build": {
            "dockerfile": "Dockerfile",
            "context": "..",
            "args": { "MARKER": "built" },
            "target": "dev"
        },
        "features": { "./markerfeat": {} }
    });
    fs::write(
        dc.join("devcontainer.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();

    let guard = ContainerGuard(std::cell::RefCell::new(None));

    let assert = Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(temp_dir.path())
        .env("DEACON_LOG", "warn")
        .args([
            "up",
            "--workspace-folder",
            temp_dir.path().to_str().unwrap(),
            "--mount-workspace-git-root=false",
            "--remove-existing-container",
        ])
        .assert();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "deacon up should succeed for Dockerfile-build + features\nSTDOUT:\n{}\nSTDERR:\n{}",
        stdout,
        stderr
    );

    let trimmed = stdout.trim();
    let value: Value = serde_json::from_str::<Value>(trimmed)
        .ok()
        .or_else(|| {
            trimmed
                .rfind('{')
                .and_then(|idx| serde_json::from_str::<Value>(&trimmed[idx..]).ok())
        })
        .unwrap_or_else(|| panic!("expected JSON result on stdout:\n{}", stdout));

    assert_eq!(value["outcome"], "success", "outcome should be success");
    let container_id = value["containerId"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(!container_id.is_empty(), "containerId should be present");
    *guard.0.borrow_mut() = Some(container_id.clone());

    // Dockerfile build arg honored (proves the user's Dockerfile actually built).
    assert_eq!(
        exec_cat(&container_id, "/etc/dockerfile-marker"),
        "built",
        "build arg MARKER should reach the built base image"
    );
    // Non-default `target: dev` honored.
    assert_eq!(
        exec_cat(&container_id, "/etc/stage-marker"),
        "dev",
        "build target 'dev' stage should be the final image"
    );
    // Feature layered ON TOP of the Dockerfile-built base — this only succeeds
    // when the feature-install `FROM` resolved to the local build tag rather
    // than a bare `sha256:` digest (the regression).
    assert_eq!(
        exec_cat(&container_id, "/usr/local/share/feature-marker"),
        "layered",
        "local feature should be layered on the Dockerfile-built base image"
    );
}
