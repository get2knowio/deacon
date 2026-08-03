//! Docker-gated integration tests for configuration the compose `up` path used to
//! read on the single-container path only.
//!
//! - #266: `devcontainer.json` `mounts` applied on the compose `up` path. The
//!   single-container path applies `config.mounts` via `merge_mounts`
//!   (up/container.rs); the compose path never read `config.mounts` at all.
//!   This test brings up a real compose project with a `mounts` entry that uses
//!   `${localWorkspaceFolder}` and verifies: the config mount lands on the
//!   primary service container with the token resolved, the original compose
//!   service volume is untouched, and a CLI `--mount` is applied alongside it.
//! - #448: the SERVICE IMAGE's `devcontainer.metadata` label folded into the
//!   configuration the compose post-create hook runs with.

mod support;

use serde_json::Value;
use std::fs;
use std::path::Path;
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

/// Best-effort cleanup; ignore failures since the project may already be torn down.
fn deacon_down(workspace: &Path) {
    let _ = support::deacon_command()
        .current_dir(workspace)
        .arg("down")
        .arg("--workspace-folder")
        .arg(workspace)
        .output();
}

/// RAII cleanup: tears the compose project down when dropped — including
/// during panic unwinding, so a failed `expect`/assertion after `up` never
/// leaks the container. Declare it right after the workspace path.
struct DeaconDownGuard<'a>(&'a Path);
impl Drop for DeaconDownGuard<'_> {
    fn drop(&mut self) {
        deacon_down(self.0);
    }
}

/// Extract the primary service container id from `deacon up`'s JSON result.
fn up_container_id(up_output: &std::process::Output) -> Option<String> {
    let stdout = String::from_utf8_lossy(&up_output.stdout);
    let trimmed = stdout.trim();
    let value: Value = serde_json::from_str(trimmed).ok().or_else(|| {
        trimmed
            .rfind('{')
            .and_then(|i| serde_json::from_str(&trimmed[i..]).ok())
    })?;
    value
        .get("containerId")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn inspect_container(container_id: &str) -> Value {
    let output = std::process::Command::new("docker")
        .args(["inspect", container_id])
        .output()
        .expect("docker inspect should run");
    assert!(
        output.status.success(),
        "docker inspect failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let inspect_json = String::from_utf8_lossy(&output.stdout);
    let inspect_array: Vec<Value> =
        serde_json::from_str(&inspect_json).expect("docker inspect output should be valid JSON");
    inspect_array
        .into_iter()
        .next()
        .expect("docker inspect should return one entry")
}

fn find_mount<'a>(inspect: &'a Value, target: &str) -> Option<&'a Value> {
    inspect["Mounts"]
        .as_array()?
        .iter()
        .find(|m| m["Destination"].as_str() == Some(target))
}

/// #266: a `devcontainer.json` `mounts` entry using `${localWorkspaceFolder}`
/// is applied to the compose primary service container, alongside the
/// existing compose-declared volume and a CLI `--mount`.
#[test]
fn test_compose_config_mounts_applied_to_container() {
    if !is_docker_available() {
        eprintln!("Skipping test_compose_config_mounts_applied_to_container: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();
    let _down = DeaconDownGuard(workspace);

    // Bind-mount source lives inside the workspace so `${localWorkspaceFolder}/sib`
    // resolves to a real, known host path without exercising `/../` traversal.
    let sib_dir = workspace.join("sib");
    fs::create_dir_all(&sib_dir).unwrap();
    fs::write(sib_dir.join("marker.txt"), "from-sib").unwrap();

    let compose_yml = r#"services:
  app:
    image: alpine:3.18
    command: ["sleep", "infinity"]
    volumes:
      - compose-named-vol:/data
volumes:
  compose-named-vol:
"#;
    let devcontainer_json = r#"{
  "name": "Compose Config Mounts",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace",
  "mounts": [
    "source=${localWorkspaceFolder}/sib,target=/workspaces/sib,type=bind"
  ]
}"#;

    fs::write(workspace.join("docker-compose.yml"), compose_yml).unwrap();
    fs::create_dir(workspace.join(".devcontainer")).unwrap();
    fs::write(
        workspace.join(".devcontainer/devcontainer.json"),
        devcontainer_json,
    )
    .unwrap();

    // A CLI-supplied mount should still apply alongside the config mount.
    let cli_mount_source = workspace.join("cli-data");
    fs::create_dir_all(&cli_mount_source).unwrap();

    let up_output = support::deacon_command()
        .current_dir(workspace)
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--mount")
        .arg(format!(
            "type=bind,source={},target=/mnt/cli",
            cli_mount_source.display()
        ))
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&up_output.stderr).to_string();
    if !up_output.status.success() {
        panic!("deacon up failed: {}", stderr);
    }

    let container_id = up_container_id(&up_output).expect("deacon up should report a containerId");
    let inspect = inspect_container(&container_id);

    // Config mount present, `${localWorkspaceFolder}` resolved to the real host path.
    // Teardown is handled unconditionally by the `DeaconDownGuard` on scope
    // exit / panic, so the assertions below can never leak the container.
    let sib_mount = find_mount(&inspect, "/workspaces/sib");
    let cli_mount = find_mount(&inspect, "/mnt/cli");
    let compose_volume = find_mount(&inspect, "/data");

    let sib_mount = sib_mount.expect("config mount at /workspaces/sib should be present");
    assert_eq!(sib_mount["Type"].as_str(), Some("bind"));
    let source = sib_mount["Source"]
        .as_str()
        .expect("bind mount should report a Source path");
    assert!(
        source.ends_with("/sib") && !source.contains("${localWorkspaceFolder}"),
        "config mount source '{}' should resolve ${{localWorkspaceFolder}} to the real workspace path",
        source
    );

    // Original compose-declared named volume is untouched. Compose prefixes
    // named volumes with the project name, so check by suffix rather than
    // exact match.
    let compose_volume = compose_volume.expect("compose-declared volume at /data should survive");
    assert_eq!(compose_volume["Type"].as_str(), Some("volume"));
    let volume_name = compose_volume["Name"]
        .as_str()
        .expect("volume mount should report a Name");
    assert!(
        volume_name.ends_with("compose-named-vol"),
        "expected the compose-declared volume, got '{}'",
        volume_name
    );

    // CLI --mount still applies alongside the config mount.
    let cli_mount = cli_mount.expect("CLI --mount at /mnt/cli should be present");
    assert_eq!(cli_mount["Type"].as_str(), Some("bind"));
}

/// RAII cleanup for the locally built fixture image. The tag is unique per run,
/// so removing it can never disturb a concurrently running test.
struct ImageGuard(String);
impl Drop for ImageGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new("docker")
            .args(["image", "rm", "-f", &self.0])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

/// #448: a compose service image's `devcontainer.metadata` label contributes to
/// the configuration the post-create hook runs with.
///
/// `remoteUser`, `remoteEnv.META_ENV` and `postCreateCommand` live ONLY in the
/// image label — the devcontainer.json declares none of them, and contributes
/// `WS_ENV` through its own `remoteEnv`. So the single marker line pins all four
/// cells of the #448 matrix at once: the hook ran, it ran as the image's
/// `remoteUser`, the image's `remoteEnv` reached it, and the workspace's did too.
///
/// Before the fix `up`'s compose path never called
/// `merge_image_metadata_after_image_ready` and its post-create exec passed no
/// user and an empty env, so NO file was written and the exit status was still 0
/// — which is why the observation here is the file, never the status.
///
/// Measured against the pinned reference CLI 0.87.0, which writes exactly this
/// line on this fixture.
#[test]
fn test_compose_up_merges_service_image_metadata() {
    if !is_docker_available() {
        eprintln!("Skipping test_compose_up_merges_service_image_metadata: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();
    let _down = DeaconDownGuard(workspace);

    // Unique tag so parallel runs in the docker-shared group never collide.
    let image_tag = format!(
        "deacon-test-compose-image-metadata-{}-{}:local",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    );
    let _image = ImageGuard(image_tag.clone());

    // The hook is the STRING form: deacon's compose post-create runs
    // `postCreateCommand` only when it is a string. The array/object forms and
    // the other lifecycle phases are a separate pre-existing gap on this path,
    // and using the string form keeps this test measuring the metadata merge.
    let image_dir = workspace.join("image");
    fs::create_dir_all(&image_dir).unwrap();
    fs::write(
        image_dir.join("Dockerfile"),
        concat!(
            "FROM alpine:3.19\n",
            "RUN adduser -D -s /bin/sh metauser\n",
            "LABEL devcontainer.metadata='[{\"remoteUser\":\"metauser\",",
            "\"remoteEnv\":{\"META_ENV\":\"from-image-metadata\"},",
            "\"postCreateCommand\":\"echo $(whoami) ${META_ENV:-UNSET} ${WS_ENV:-UNSET} ",
            "> /workspace/image-metadata.txt\"}]'\n",
        ),
    )
    .unwrap();

    let build = std::process::Command::new("docker")
        .args(["build", "-q", "-t", &image_tag])
        .arg(&image_dir)
        .output()
        .expect("docker build should run");
    assert!(
        build.status.success(),
        "docker build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let compose_yml = format!(
        "services:\n  app:\n    image: {}\n    volumes:\n      - .:/workspace\n    command: sleep infinity\n",
        image_tag
    );
    // No `remoteUser` and no hook here: whatever the marker records can only
    // have come from the image's metadata label.
    let devcontainer_json = r#"{
  "name": "Compose Image Metadata",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace",
  "remoteEnv": {
    "WS_ENV": "from-workspace-config"
  }
}"#;

    fs::write(workspace.join("docker-compose.yml"), compose_yml).unwrap();
    fs::create_dir(workspace.join(".devcontainer")).unwrap();
    fs::write(
        workspace.join(".devcontainer/devcontainer.json"),
        devcontainer_json,
    )
    .unwrap();

    let up_output = support::deacon_command()
        .current_dir(workspace)
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .output()
        .unwrap();

    if !up_output.status.success() {
        panic!(
            "deacon up failed: {}",
            String::from_utf8_lossy(&up_output.stderr)
        );
    }

    let marker = workspace.join("image-metadata.txt");
    let contents = fs::read_to_string(&marker).unwrap_or_else(|e| {
        panic!(
            "the image-metadata postCreateCommand should have written {}: {} (up stderr: {})",
            marker.display(),
            e,
            String::from_utf8_lossy(&up_output.stderr)
        )
    });

    assert_eq!(
        contents.trim(),
        "metauser from-image-metadata from-workspace-config",
        "the hook must run as the image's remoteUser with both the image's and the \
         workspace's remoteEnv applied"
    );
}
