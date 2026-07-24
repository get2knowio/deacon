//! Integration test for #266: `devcontainer.json` `mounts` applied on the compose `up` path.
//!
//! The single-container path applies `config.mounts` via `merge_mounts`
//! (up/container.rs); the compose path never read `config.mounts` at all.
//! This test brings up a real compose project with a `mounts` entry that uses
//! `${localWorkspaceFolder}` and verifies: the config mount lands on the
//! primary service container with the token resolved, the original compose
//! service volume is untouched, and a CLI `--mount` is applied alongside it.

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
