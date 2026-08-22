//! End-to-end Docker integration tests for the compose build paths: installing
//! devcontainer features into compose services, and (the Features-free sibling)
//! `deacon build --image-name` tagging on a compose config.
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

mod support;

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
///
/// This is only useful as a best-effort *pre*-test sweep (before we have a
/// `deacon up` result to read the real project name from): deacon's compose
/// project name does not necessarily match the directory-basename default
/// `docker compose` would infer here (see [`compose_down_by_project`]).
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

/// Tear down a compose project by the exact name `deacon up` reported.
///
/// `docker compose` run bare (or with `--project-directory`) derives its
/// default project name from a directory basename, which does NOT match
/// deacon's compose project naming — so `docker compose exec`/`down` invoked
/// without `-p <name>` silently targets a different (usually nonexistent)
/// project and reports "service is not running" even though deacon's
/// container is alive. `docker compose -p <name> down` works purely from the
/// project label, no compose file needed. Always best-effort.
fn compose_down_by_project(project_name: &str) {
    let _ = StdCommand::new("docker")
        .args([
            "compose",
            "-p",
            project_name,
            "down",
            "--remove-orphans",
            "-v",
            "--rmi",
            "local",
        ])
        .output();
}

/// Extract the compose project name `deacon up` reported in its JSON result.
fn up_project_name(up_output: &std::process::Output) -> Option<String> {
    let stdout = String::from_utf8_lossy(&up_output.stdout);
    let trimmed = stdout.trim();
    let value: serde_json::Value = serde_json::from_str(trimmed).ok().or_else(|| {
        trimmed
            .rfind('{')
            .and_then(|i| serde_json::from_str(&trimmed[i..]).ok())
    })?;
    value
        .get("composeProjectName")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
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

    let up = support::deacon_command()
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

    // Read back the exact project name deacon used — a bare `docker compose`
    // here would derive a different default from the directory basename and
    // silently miss the running project (see `compose_down_by_project`).
    let project_name = up_project_name(&up).expect("deacon up should report a composeProjectName");

    // The common-utils feature drops a marker file at this canonical path.
    let exec = StdCommand::new("docker")
        .args([
            "compose",
            "-p",
            &project_name,
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
    compose_down_by_project(&project_name);

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

    let up = support::deacon_command()
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
    // compose file's directory) and does not match the directory-basename
    // default `docker compose` would otherwise infer — read back the exact
    // name deacon used and address the project by `-p`, not by directory or
    // `--project-directory` (neither reproduces deacon's naming).
    let project_name = up_project_name(&up).expect("deacon up should report a composeProjectName");
    let docker_compose = |cmd: &str| -> std::process::Output {
        StdCommand::new("docker")
            .args([
                "compose",
                "-p",
                &project_name,
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

    compose_down_by_project(&project_name);

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
    // `dockerComposeFile` resolves against the CONFIG dir (`.devcontainer/`), not
    // the workspace folder, so a compose file at the workspace root is reached
    // with `../`. Spelling it `docker-compose.yml` here pointed at a path that
    // does not exist, and `deacon build` failed with a message containing
    // "Docker" — which the tolerant early-return below then read as "no daemon",
    // so this test passed without ever building anything.
    fs::write(
        dc_dir.join("devcontainer.json"),
        r#"{
  "name": "build-compose-features",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "remoteUser": "root",
  "features": { "./features/marker": {} }
}"#,
    )
    .expect("write devcontainer.json");

    let image_tag = "deacon-test/compose-features:latest";
    let out = support::deacon_command()
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

    // Docker availability is already gated at the top of the test, so a failure
    // here is a real one. Matching a "Docker" substring instead swallowed a
    // config-resolution error for as long as this test existed.
    assert!(
        out.status.success(),
        "deacon build (compose+features) failed:\nSTDOUT:\n{}\nSTDERR:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

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

/// #619: `deacon build` on a compose config with **no** Features must create a
/// tag for EVERY `--image-name` and report all of them, in order.
///
/// The Features-free compose path never reaches `resolve_compose_feature_image`,
/// so nothing used to retag the image `docker compose build` produced: the result
/// document named tags that did not exist, and — because the first entry of
/// `tags` was a user name where every other path puts the deterministic tag —
/// `output_result` stripped one of them on the way out.
///
/// The assertion is artifact-level on purpose (CLAUDE.md's canary rule): a JSON
/// `outcome` is exactly what stayed green while no tag was created. Both names
/// must resolve, resolve to the SAME image, and that image must carry the base
/// layer's marker.
#[test]
fn build_compose_without_features_tags_every_image_name() {
    if !is_docker_available() {
        eprintln!(
            "Skipping build_compose_without_features_tags_every_image_name: Docker not available"
        );
        return;
    }

    let temp_dir = TempDir::new().expect("tempdir");
    let workspace = temp_dir.path();

    // build-shape compose service, no features anywhere in the config. alpine is
    // fine here precisely because no feature `install.sh` needs bash.
    fs::write(
        workspace.join("Dockerfile"),
        "FROM alpine:3.19\nRUN echo compose-base > /base-marker.txt\nCMD [\"sleep\", \"infinity\"]\n",
    )
    .expect("write Dockerfile");
    fs::write(
        workspace.join("docker-compose.yml"),
        "services:\n  app:\n    build:\n      context: .\n      dockerfile: Dockerfile\n    command: [\"sleep\", \"infinity\"]\n",
    )
    .expect("write compose");

    let dc_dir = workspace.join(".devcontainer");
    fs::create_dir_all(&dc_dir).expect("create .devcontainer");
    // `dockerComposeFile` is config-dir-relative, so the workspace-root compose
    // file is reached with `../` (see the sibling test above).
    fs::write(
        dc_dir.join("devcontainer.json"),
        r#"{
  "name": "build-compose-no-features",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app"
}"#,
    )
    .expect("write devcontainer.json");

    // Unique tags so the test never collides with a sibling on the shared daemon.
    let first = "deacon-test/compose-multitag-first:v1";
    let second = "deacon-test/compose-multitag-second:v1";
    let _ = StdCommand::new("docker")
        .args(["rmi", "-f", first, second])
        .output();

    let out = support::deacon_command()
        .current_dir(workspace)
        .args([
            "build",
            "--workspace-folder",
            workspace.to_str().unwrap(),
            "--image-name",
            first,
            "--image-name",
            second,
            "--output-format",
            "json",
        ])
        .env("DEACON_LOG", "warn")
        .output()
        .expect("spawn deacon build");

    // Docker availability is gated above, so any failure here is a real one.
    assert!(
        out.status.success(),
        "deacon build (compose, no features) failed:\nSTDOUT:\n{}\nSTDERR:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // Collect everything to assert BEFORE tearing down, so cleanup always runs.
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let result: serde_json::Value =
        serde_json::from_str(&stdout).expect("build should emit a single JSON document on stdout");

    let ids: Vec<String> = [first, second]
        .iter()
        .map(|tag| {
            let inspect = StdCommand::new("docker")
                .args(["image", "inspect", "--format", "{{.Id}}", tag])
                .output()
                .expect("docker image inspect");
            if inspect.status.success() {
                String::from_utf8_lossy(&inspect.stdout).trim().to_string()
            } else {
                String::new()
            }
        })
        .collect();

    // The marker proves the tag points at the service's own build, not at some
    // unrelated image that happened to answer to the name.
    let run = StdCommand::new("docker")
        .args(["run", "--rm", first, "cat", "/base-marker.txt"])
        .output()
        .expect("docker run");
    let marker = String::from_utf8_lossy(&run.stdout).to_string();

    // `RepoTags` names the compose-produced image too, so the project image is
    // reclaimed alongside the two tags rather than left on the daemon.
    let repo_tags = StdCommand::new("docker")
        .args(["image", "inspect", "--format", "{{json .RepoTags}}", first])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice::<Vec<String>>(&o.stdout).ok())
        .unwrap_or_else(|| vec![first.to_string(), second.to_string()]);
    let mut rmi = vec!["rmi".to_string(), "-f".to_string()];
    rmi.extend(repo_tags);
    let _ = StdCommand::new("docker").args(&rmi).output();

    assert_eq!(
        result["imageName"],
        serde_json::json!([first, second]),
        "every --image-name must be reported, in order; got {}",
        stdout
    );
    assert!(
        !ids[0].is_empty(),
        "'{}' must exist on the daemon after the build; result was {}",
        first,
        stdout
    );
    assert!(
        !ids[1].is_empty(),
        "'{}' must exist on the daemon after the build; result was {}",
        second,
        stdout
    );
    assert_eq!(
        ids[0], ids[1],
        "both --image-name tags must point at the same built service image"
    );
    assert!(
        marker.contains("compose-base"),
        "the tagged image must be the compose service's own build; \
         `cat /base-marker.txt` gave stdout={:?} stderr={:?}",
        marker,
        String::from_utf8_lossy(&run.stderr)
    );
}

/// #628: `deacon build` on a compose config with **no** Features must produce an
/// image carrying `devcontainer.metadata`, exactly as the reference CLI's does —
/// and must not lose anything the compose author declared in getting there.
///
/// The two halves belong in ONE test because they are the same decision. A label
/// cannot be added by retagging, and stamping one afterwards would mean a second
/// build `FROM` a daemon-local tag, which #595 forbids; so the label has to be an
/// INPUT to the build. The way that stays safe is to OVERRIDE the compose build
/// rather than replace it — Compose still resolves the service, so `build.args`
/// keep applying. Replacing it would satisfy the label assertion alone while
/// silently dropping every build arg, which is measurably what deacon's
/// compose-WITH-Features path does today (filed separately).
///
/// Artifact-level on purpose: the JSON `outcome` was `success` throughout the
/// window in which no label existed at all.
#[test]
fn build_compose_without_features_labels_the_image_and_keeps_build_args() {
    if !is_docker_available() {
        eprintln!(
            "Skipping build_compose_without_features_labels_the_image_and_keeps_build_args: \
             Docker not available"
        );
        return;
    }

    let temp_dir = TempDir::new().expect("tempdir");
    let workspace = temp_dir.path();

    // A `build:`-shape service whose Dockerfile can only produce the right marker
    // if the compose file's own `build.args` reach it.
    fs::write(
        workspace.join("Dockerfile"),
        "FROM alpine:3.19\nARG MARKER=arg-was-dropped\nRUN echo \"$MARKER\" > /arg-marker.txt\n\
         CMD [\"sleep\", \"infinity\"]\n",
    )
    .expect("write Dockerfile");
    fs::write(
        workspace.join("docker-compose.yml"),
        concat!(
            "services:\n",
            "  app:\n",
            "    build:\n",
            "      context: .\n",
            "      dockerfile: Dockerfile\n",
            "      args:\n",
            "        MARKER: arg-reached-the-build\n",
            "    command: [\"sleep\", \"infinity\"]\n",
        ),
    )
    .expect("write compose");

    let dc_dir = workspace.join(".devcontainer");
    fs::create_dir_all(&dc_dir).expect("create .devcontainer");
    // `dockerComposeFile` is config-dir-relative, so the workspace-root compose
    // file is reached with `../` (see the siblings above). `remoteUser` and
    // `shutdownAction` are the entries the label must record.
    fs::write(
        dc_dir.join("devcontainer.json"),
        r#"{
  "name": "build-compose-metadata-label",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "shutdownAction": "stopCompose",
  "remoteUser": "root"
}"#,
    )
    .expect("write devcontainer.json");

    let tag = "deacon-test/compose-metadata-label:v1";
    let _ = StdCommand::new("docker").args(["rmi", "-f", tag]).output();

    let out = support::deacon_command()
        .current_dir(workspace)
        .args([
            "build",
            "--workspace-folder",
            workspace.to_str().unwrap(),
            "--image-name",
            tag,
            "--output-format",
            "json",
        ])
        .env("DEACON_LOG", "warn")
        .output()
        .expect("spawn deacon build");

    // Docker availability is gated above, so any failure here is a real one.
    assert!(
        out.status.success(),
        "deacon build (compose, no features) failed:\nSTDOUT:\n{}\nSTDERR:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // Collect everything to assert BEFORE tearing down, so cleanup always runs.
    let label = StdCommand::new("docker")
        .args([
            "image",
            "inspect",
            "--format",
            "{{index .Config.Labels \"devcontainer.metadata\"}}",
            tag,
        ])
        .output()
        .expect("docker image inspect");
    let label = String::from_utf8_lossy(&label.stdout).trim().to_string();

    let run = StdCommand::new("docker")
        .args(["run", "--rm", tag, "cat", "/arg-marker.txt"])
        .output()
        .expect("docker run");
    let marker = String::from_utf8_lossy(&run.stdout).to_string();

    // `RepoTags` names the compose-produced image too, so the project image is
    // reclaimed alongside the tag rather than left on the daemon.
    let repo_tags = StdCommand::new("docker")
        .args(["image", "inspect", "--format", "{{json .RepoTags}}", tag])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice::<Vec<String>>(&o.stdout).ok())
        .unwrap_or_else(|| vec![tag.to_string()]);
    let mut rmi = vec!["rmi".to_string(), "-f".to_string()];
    rmi.extend(repo_tags);
    let _ = StdCommand::new("docker").args(&rmi).output();

    // Pinned WHOLE and by value, not by presence: the entries are ordered and a
    // presence check would pass on a label recording the wrong configuration.
    // Measured against the reference CLI at oracle 0.87.0 on this shape, which
    // writes the same document (modulo the cosmetic spaces it pads arrays with).
    assert_eq!(
        label, r#"[{"remoteUser":"root","shutdownAction":"stopCompose"}]"#,
        "the compose-produced image must carry devcontainer.metadata; \
         got {label:?}"
    );
    assert!(
        marker.contains("arg-reached-the-build"),
        "the compose service's own build.args must still reach the build; \
         `cat /arg-marker.txt` gave stdout={:?} stderr={:?}",
        marker,
        String::from_utf8_lossy(&run.stderr)
    );
}

/// #629: the same guarantee, on the path that declares Features — where deacon
/// used to build the Feature-extended image OUTSIDE Compose and so saw only what
/// it had thought to forward. `build.args` were not forwarded, so an `ARG` the
/// service declared reached the reference's image and not deacon's.
///
/// Every assertion here is a `build:` key the author wrote and deacon does not
/// name in its override — the arg, the label, and the `target` that decides WHICH
/// stage the Features install on. They pass together because Compose still
/// resolves the service, not because each was enumerated; the previous shape
/// dropped all three at once.
///
/// Artifact-level for the same reason as its Features-free sibling: both CLIs
/// exited 0 with `outcome: success` throughout the window in which the arg was
/// being dropped, so nothing in the result document could have caught it.
/// MEASURED against the reference CLI at oracle 0.87.0 on this exact shape: the
/// image it produces carries the same four values.
#[test]
fn build_compose_with_features_keeps_the_services_own_build_keys() {
    if !is_docker_available() {
        eprintln!(
            "Skipping build_compose_with_features_keeps_the_services_own_build_keys: \
             Docker not available"
        );
        return;
    }

    let temp_dir = TempDir::new().expect("tempdir");
    let workspace = temp_dir.path();

    // Multi-stage on a bash-capable base (a Feature's `install.sh` needs bash).
    // The `last` stage exists to be NOT built: `build.target` names `middle`, and
    // the Features must install on top of that.
    fs::write(
        workspace.join("Dockerfile"),
        concat!(
            "FROM debian:bookworm-slim AS base\n",
            "ARG MARKER=arg-was-dropped\n",
            "RUN echo \"$MARKER\" > /arg-marker.txt\n",
            "\n",
            "FROM base AS middle\n",
            "RUN echo middle > /stage.txt\n",
            "\n",
            "FROM middle AS last\n",
            "RUN echo last > /stage.txt\n",
        ),
    )
    .expect("write Dockerfile");
    fs::write(
        workspace.join("docker-compose.yml"),
        concat!(
            "services:\n",
            "  app:\n",
            "    build:\n",
            "      context: .\n",
            "      dockerfile: Dockerfile\n",
            "      target: middle\n",
            "      args:\n",
            "        MARKER: arg-reached-the-build\n",
            "      labels:\n",
            "        author.own.label: authored\n",
            "    command: [\"sleep\", \"infinity\"]\n",
        ),
    )
    .expect("write compose");

    let dc_dir = workspace.join(".devcontainer");
    fs::create_dir_all(&dc_dir).expect("create .devcontainer");
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
    // `dockerComposeFile` is config-dir-relative; the workspace-root compose file
    // is reached with `../`, as in every sibling here.
    fs::write(
        dc_dir.join("devcontainer.json"),
        r#"{
  "name": "build-compose-features-build-keys",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "remoteUser": "root",
  "features": { "./features/marker": {} }
}"#,
    )
    .expect("write devcontainer.json");

    let tag = "deacon-test/compose-features-build-keys:v1";
    let _ = StdCommand::new("docker").args(["rmi", "-f", tag]).output();

    let out = support::deacon_command()
        .current_dir(workspace)
        .args([
            "build",
            "--workspace-folder",
            workspace.to_str().unwrap(),
            "--image-name",
            tag,
            "--output-format",
            "json",
        ])
        .env("DEACON_LOG", "warn")
        .output()
        .expect("spawn deacon build");

    assert!(
        out.status.success(),
        "deacon build (compose + features) failed:\nSTDOUT:\n{}\nSTDERR:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // Collect everything to assert BEFORE tearing down, so cleanup always runs.
    let run = StdCommand::new("docker")
        .args([
            "run",
            "--rm",
            tag,
            "cat",
            "/arg-marker.txt",
            "/stage.txt",
            "/compose-feature-marker.txt",
        ])
        .output()
        .expect("docker run");
    let produced = String::from_utf8_lossy(&run.stdout).to_string();

    let author_label = StdCommand::new("docker")
        .args([
            "image",
            "inspect",
            "--format",
            "{{index .Config.Labels \"author.own.label\"}}",
            tag,
        ])
        .output()
        .expect("docker image inspect");
    let author_label = String::from_utf8_lossy(&author_label.stdout)
        .trim()
        .to_string();

    // The Compose-produced image is named by `RepoTags` too, so reclaim it
    // alongside the user's tag rather than leaving it on the daemon.
    let repo_tags = StdCommand::new("docker")
        .args(["image", "inspect", "--format", "{{json .RepoTags}}", tag])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice::<Vec<String>>(&o.stdout).ok())
        .unwrap_or_else(|| vec![tag.to_string()]);
    let mut rmi = vec!["rmi".to_string(), "-f".to_string()];
    rmi.extend(repo_tags);
    let _ = StdCommand::new("docker").args(&rmi).output();

    assert!(
        produced.contains("arg-reached-the-build"),
        "the service's own build.args must reach a Feature-installing build; \
         got {produced:?} stderr={:?}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        produced.contains("middle") && !produced.contains("last"),
        "the Features must install on top of the service's own build.target; \
         got {produced:?}"
    );
    assert!(
        produced.contains("installed"),
        "the Feature must still be installed; got {produced:?} stderr={:?}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        author_label, "authored",
        "the service's own build.labels must survive deacon's override"
    );
}
