//! End-to-end Docker integration tests for installing devcontainer features into
//! compose services.
//!
//! Bead 14a (commit `f4997b9`) shipped `image:`-shape support. Bead 14b adds
//! `build:`-shape support via the Dockerfile stage-name parser. These tests
//! exercise both code paths against a real Docker daemon and a real OCI feature
//! (`ghcr.io/devcontainers/features/common-utils:2`), asserting the resulting
//! container has the feature-installed marker
//! (`/usr/local/etc/vscode-dev-containers/common`).
//!
//! Both tests live in the `docker-shared` nextest group: they pull from a
//! public registry, run `docker buildx build`, and bring up a compose project.
//! They share the daemon with other tests but never collide with one another
//! because each test uses its own temp dir, compose project name, and image
//! tag.

use assert_cmd::Command;
use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

/// Check that the local Docker daemon is reachable. Tests that need Docker
/// skip themselves when this returns false, matching the convention used by
/// other Docker-backed integration tests in this crate.
fn is_docker_available() -> bool {
    StdCommand::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run `docker compose down --remove-orphans -v` in a project directory to
/// drop any containers, networks, or volumes left behind by a failed test.
/// Always best-effort — we ignore the exit code so cleanup never masks a
/// real test failure.
fn compose_cleanup(project_dir: &std::path::Path) {
    let _ = StdCommand::new("docker")
        .current_dir(project_dir)
        .args([
            "compose",
            "down",
            "--remove-orphans",
            "-v",
            "--rmi",
            "local",
        ])
        .output();
}

/// Bead 14a regression: a compose service that declares `image:` plus
/// `features` brings up successfully, and the feature install marker
/// (`/usr/local/etc/vscode-dev-containers/common`) is present inside the
/// running container.
#[test]
fn compose_features_image_shape_installs_feature() {
    if !is_docker_available() {
        eprintln!("Skipping compose_features_image_shape_installs_feature: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().expect("tempdir");
    let workspace = temp_dir.path();

    // docker-compose.yml: image-only service. Override the command so the
    // container stays running long enough for the docker exec assertions.
    //
    // We use debian:bookworm-slim (not alpine) because public devcontainer
    // features expect bash + apt; alpine ships neither out of the box.
    let compose_yaml = "services:\n  app:\n    image: debian:bookworm-slim\n    command: [\"sleep\", \"infinity\"]\n";
    fs::write(workspace.join("docker-compose.yml"), compose_yaml).expect("write compose");

    // devcontainer.json with a real OCI feature. `common-utils` is the
    // canonical small smoke-feature in the devcontainers org and creates
    // `/usr/local/etc/vscode-dev-containers/common` as a deterministic
    // marker; we pass options that disable the heavier optional installs
    // (zsh / oh-my-zsh / package upgrades) to keep the test fast.
    let dc_dir = workspace.join(".devcontainer");
    fs::create_dir_all(&dc_dir).expect("create .devcontainer");
    // `dockerComposeFile` is resolved relative to the directory containing
    // devcontainer.json (`.devcontainer/`) per the spec and the reference CLI.
    // The compose file lives at the workspace root (a common layout), so the
    // config references it with `../`.
    let dc_json = r#"{
  "name": "compose-features-image-shape",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace",
  "features": {
    "ghcr.io/devcontainers/features/common-utils:2": {
      "installZsh": false,
      "installOhMyZsh": false,
      "upgradePackages": false
    }
  }
}"#;
    fs::write(dc_dir.join("devcontainer.json"), dc_json).expect("write devcontainer.json");

    // Best-effort cleanup before the test in case a previous run left state.
    compose_cleanup(workspace);

    let up = Command::cargo_bin("deacon")
        .expect("deacon binary")
        .current_dir(workspace)
        .args([
            "up",
            "--workspace-folder",
            workspace.to_str().unwrap(),
            "--remove-existing-container",
            "--skip-post-create",
        ])
        .env("DEACON_LOG", "warn")
        .output()
        .expect("spawn deacon up");

    let stdout = String::from_utf8_lossy(&up.stdout);
    let stderr = String::from_utf8_lossy(&up.stderr);
    if !up.status.success() {
        compose_cleanup(workspace);
        panic!(
            "deacon up (compose image-shape) failed:\nSTDOUT:\n{}\nSTDERR:\n{}",
            stdout, stderr
        );
    }

    // The common-utils feature drops a marker file at this canonical path.
    let exec = StdCommand::new("docker")
        .current_dir(workspace)
        .args([
            "compose",
            "exec",
            "-T",
            "app",
            "sh",
            "-c",
            "test -f /usr/local/etc/vscode-dev-containers/common",
        ])
        .output()
        .expect("docker compose exec");

    let exec_ok = exec.status.success();
    // Always tear down before any assertions to avoid leaking resources.
    compose_cleanup(workspace);

    assert!(
        exec_ok,
        "expected /usr/local/etc/vscode-dev-containers/common to exist in the \
         running compose container after feature install; exec stderr={}",
        String::from_utf8_lossy(&exec.stderr)
    );
}

/// Bead 14b: a compose service that declares `build:` (context + dockerfile)
/// plus `features` runs `deacon up` to completion, and the feature install
/// marker is present in the running container.
///
/// This test exercises the Dockerfile stage-name parser path: the user's
/// Dockerfile has no `AS` alias on its final `FROM`, so the parser must
/// rewrite it before the feature install stage can target it.
///
/// The compose file lives in a subdirectory to verify the subtle compose
/// semantic: `build.context` and `build.dockerfile` are resolved relative to
/// the **compose file's directory** (`./compose-dir/`), NOT the workspace
/// folder. If the resolution were workspace-relative, the test would fail to
/// find the Dockerfile.
#[test]
fn compose_features_build_shape_installs_feature() {
    if !is_docker_available() {
        eprintln!("Skipping compose_features_build_shape_installs_feature: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().expect("tempdir");
    let workspace = temp_dir.path();

    // Put the compose file in a subdirectory so the dockerfile/context
    // resolution must be relative to that directory (not the workspace).
    let compose_dir = workspace.join("compose-dir");
    fs::create_dir_all(&compose_dir).expect("create compose-dir");

    // Dockerfile with no `AS` alias on the final FROM — the parser must
    // append one before the feature install stage can target it.
    //
    // debian (not alpine) because devcontainer features require bash + apt.
    let dockerfile = "FROM debian:bookworm-slim\nRUN echo 'compose build base' > /base-marker.txt\nCMD [\"sleep\", \"infinity\"]\n";
    fs::write(compose_dir.join("Dockerfile.dev"), dockerfile).expect("write Dockerfile.dev");

    // Note: dockerfile and context paths are RELATIVE to the compose file,
    // not the workspace. `context: .` resolves to `compose-dir/`.
    let compose_yaml = "services:\n  app:\n    build:\n      context: .\n      dockerfile: Dockerfile.dev\n    command: [\"sleep\", \"infinity\"]\n";
    fs::write(compose_dir.join("docker-compose.yml"), compose_yaml).expect("write compose");

    let dc_dir = workspace.join(".devcontainer");
    fs::create_dir_all(&dc_dir).expect("create .devcontainer");
    // dockerComposeFile is resolved relative to the config dir (`.devcontainer/`)
    // per the spec, so we reference the root-level subdir with `../`. Compose
    // THEN resolves build.context/build.dockerfile relative to its OWN directory
    // (`<workspace>/compose-dir/`), NOT the workspace.
    let dc_json = r#"{
  "name": "compose-features-build-shape",
  "dockerComposeFile": "../compose-dir/docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace",
  "features": {
    "ghcr.io/devcontainers/features/common-utils:2": {
      "installZsh": false,
      "installOhMyZsh": false,
      "upgradePackages": false
    }
  }
}"#;
    fs::write(dc_dir.join("devcontainer.json"), dc_json).expect("write devcontainer.json");

    // The compose project will be associated with the workspace folder
    // (deacon derives the project name from its `--workspace-folder`), so
    // cleanup must run from there even though the compose file lives in a
    // subdirectory.
    compose_cleanup(workspace);

    let up = Command::cargo_bin("deacon")
        .expect("deacon binary")
        .current_dir(workspace)
        .args([
            "up",
            "--workspace-folder",
            workspace.to_str().unwrap(),
            "--remove-existing-container",
            "--skip-post-create",
        ])
        .env("DEACON_LOG", "warn")
        .output()
        .expect("spawn deacon up");

    let stdout = String::from_utf8_lossy(&up.stdout);
    let stderr = String::from_utf8_lossy(&up.stderr);
    if !up.status.success() {
        compose_cleanup(workspace);
        panic!(
            "deacon up (compose build-shape) failed:\nSTDOUT:\n{}\nSTDERR:\n{}",
            stdout, stderr
        );
    }

    // Compose project name is derived from the workspace folder (not the
    // compose file's directory), so we must invoke `docker compose` with
    // `-f <compose-file>` and `--project-directory <workspace>` to attach to
    // the same project deacon brought up. Running from `compose_dir` would
    // address a different project and report "service is not running".
    let compose_file = compose_dir.join("docker-compose.yml");
    let docker_compose = |cmd: &str| -> std::process::Output {
        StdCommand::new("docker")
            .current_dir(workspace)
            .args([
                "compose",
                "-f",
                compose_file.to_str().unwrap(),
                "--project-directory",
                workspace.to_str().unwrap(),
                "exec",
                "-T",
                "app",
                "sh",
                "-c",
                cmd,
            ])
            .output()
            .expect("docker compose exec")
    };

    // Verify the base layer ran (proves we used the user's Dockerfile).
    let base_exec = docker_compose("test -f /base-marker.txt");
    // Verify the feature install ran on top (proves we layered features
    // onto the user's Dockerfile, not just used the base image).
    let feature_exec = docker_compose("test -f /usr/local/etc/vscode-dev-containers/common");

    let base_ok = base_exec.status.success();
    let feature_ok = feature_exec.status.success();

    compose_cleanup(workspace);

    assert!(
        base_ok,
        "expected /base-marker.txt (from user's Dockerfile) to exist; \
         exec stderr={}",
        String::from_utf8_lossy(&base_exec.stderr)
    );
    assert!(
        feature_ok,
        "expected /usr/local/etc/vscode-dev-containers/common (from feature \
         install) to exist in the running compose container; exec stderr={}",
        String::from_utf8_lossy(&feature_exec.stderr)
    );
}

/// `deacon build` on a compose config with features must produce a
/// feature-extended image for the target service and tag it with
/// `--image-name`. Regression guard for `execute_compose_build_with_features`.
///
/// Uses a local feature (no OCI pull) writing a deterministic marker; asserts
/// the named image contains it (i.e. `--image-name` resolves to the
/// feature-extended image, not the bare base).
#[test]
fn build_compose_with_features_tags_final_image() {
    if !is_docker_available() {
        eprintln!("Skipping build_compose_with_features_tags_final_image: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().expect("tempdir");
    let workspace = temp_dir.path();

    // build-shape compose service on a bash-capable base.
    fs::write(
        workspace.join("Dockerfile"),
        "FROM debian:bookworm-slim\nRUN echo base > /base.txt\nCMD [\"sleep\", \"infinity\"]\n",
    )
    .expect("write Dockerfile");
    fs::write(
        workspace.join("docker-compose.yml"),
        "services:\n  app:\n    build:\n      context: .\n      dockerfile: Dockerfile\n    command: [\"sleep\", \"infinity\"]\n",
    )
    .expect("write compose");

    let dc_dir = workspace.join(".devcontainer");
    fs::create_dir_all(&dc_dir).expect("create .devcontainer");
    // Local feature (resolved relative to the config dir) writing a marker.
    let feat = dc_dir.join("features/marker");
    fs::create_dir_all(&feat).expect("create feature dir");
    fs::write(
        feat.join("devcontainer-feature.json"),
        r#"{ "id": "marker", "version": "1.0.0", "name": "Marker" }"#,
    )
    .expect("write feature json");
    fs::write(
        feat.join("install.sh"),
        "#!/usr/bin/env bash\nset -e\necho installed > /compose-feature-marker.txt\n",
    )
    .expect("write install.sh");
    fs::write(
        dc_dir.join("devcontainer.json"),
        r#"{
  "name": "build-compose-features",
  "dockerComposeFile": "docker-compose.yml",
  "service": "app",
  "remoteUser": "root",
  "features": { "./features/marker": {} }
}"#,
    )
    .expect("write devcontainer.json");

    let image_tag = "deacon-test/compose-features:latest";
    let out = Command::cargo_bin("deacon")
        .expect("deacon binary")
        .current_dir(workspace)
        .args([
            "build",
            "--workspace-folder",
            workspace.to_str().unwrap(),
            "--image-name",
            image_tag,
            "--output-format",
            "json",
        ])
        .env("DEACON_LOG", "warn")
        .output()
        .expect("spawn deacon build");

    if !out.status.success() {
        // Docker unavailable / not permitted is the only acceptable failure.
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("Docker") || stderr.contains("permission denied"),
            "deacon build (compose+features) failed unexpectedly: {}",
            stderr
        );
        return;
    }

    let run = StdCommand::new("docker")
        .args([
            "run",
            "--rm",
            image_tag,
            "cat",
            "/compose-feature-marker.txt",
        ])
        .output()
        .expect("docker run");
    let _ = StdCommand::new("docker")
        .args(["rmi", "-f", image_tag])
        .output();

    assert!(
        run.status.success() && String::from_utf8_lossy(&run.stdout).contains("installed"),
        "--image-name should resolve to the feature-extended compose image; \
         stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}
