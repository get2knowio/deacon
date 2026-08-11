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
//! - #460: the compose path runs the FULL lifecycle phase set, in every command
//!   form, and honors `--skip-post-create` / `--skip-non-blocking-commands` the
//!   way the single-container path does.
//! - #564: the compose project name carries the sanitized workspace-folder stem,
//!   and `up` says so out loud when this workspace still has named volumes under
//!   a superseded project name (an older deacon's, or the reference CLI's).

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

/// Extract the derived compose project name from `deacon up`'s JSON result.
fn up_project_name(up_output: &std::process::Output) -> Option<String> {
    let stdout = String::from_utf8_lossy(&up_output.stdout);
    let trimmed = stdout.trim();
    let value: Value = serde_json::from_str(trimmed).ok().or_else(|| {
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
///
/// The write itself is load-bearing for #462. The hook runs as the image's
/// non-root `metauser`, pinned below to uid 1234, and the workspace is a
/// `TempDir` owned by the test process's own uid — so the hook can write only
/// once `updateRemoteUserUID` has remapped `metauser` to the HOST's uid.
/// deacon applied that mapping on the single-container path only until #462;
/// this test used to `chmod 0777` the workspace to take permissions out of what
/// it measured, and that workaround is gone. With the uid pinned to 1234 the
/// mismatch is the norm rather than an accident, so a regression in the remap
/// fails here on every host — `Permission denied` and no marker — instead of
/// only on hosts whose uid happens to disagree with the image's.
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
    // the other lifecycle phases are a separate pre-existing gap on this path
    // (#460), and using the string form keeps this test measuring the merge.
    //
    // `metauser` is pinned to uid 1234 rather than letting `adduser` pick the
    // first free uid (1000, the commonest host uid there is), which makes the
    // uid MISmatch the norm here instead of an accident: the hook's write can
    // only succeed through the #462 remap, on every host.
    let image_dir = workspace.join("image");
    fs::create_dir_all(&image_dir).unwrap();
    fs::write(
        image_dir.join("Dockerfile"),
        concat!(
            "FROM alpine:3.19\n",
            "RUN adduser -D -u 1234 -s /bin/sh metauser\n",
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

/// #462, the opt-out half: `"updateRemoteUserUID": false` in devcontainer.json
/// suppresses the uid remap on the COMPOSE path, exactly as it does on the
/// single-container one.
///
/// The sibling test above proves the remap happens by default; this one proves
/// the knob still turns it off. `metauser` is pinned to uid 1234 again, so the
/// two tests differ in exactly one input and the marker's uid is the verdict:
/// 1234 means the image's uid survived, the host's uid means it did not.
///
/// The workspace IS made world-writable here, and unlike the pre-#462 workaround
/// that is not papering over a gap — it is what the opt-out asks for. Declining
/// the remap is declining the thing that makes a non-root user able to write a
/// host-owned bind mount, so without the `chmod` the hook could not write the
/// marker this test reads and there would be nothing to measure.
#[test]
fn test_compose_up_update_remote_user_uid_false_keeps_image_uid() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_compose_up_update_remote_user_uid_false_keeps_image_uid: Docker not available"
        );
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();
    let _down = DeaconDownGuard(workspace);

    // See the note above: the opt-out leaves `metauser` at uid 1234, which is
    // not the owner of this host-side directory.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(workspace, fs::Permissions::from_mode(0o777)).unwrap();
    }

    let image_tag = format!(
        "deacon-test-compose-no-uid-remap-{}-{}:local",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    );
    let _image = ImageGuard(image_tag.clone());

    let image_dir = workspace.join("image");
    fs::create_dir_all(&image_dir).unwrap();
    fs::write(
        image_dir.join("Dockerfile"),
        concat!(
            "FROM alpine:3.19\n",
            "RUN adduser -D -u 1234 -s /bin/sh metauser\n",
            "LABEL devcontainer.metadata='[{\"remoteUser\":\"metauser\",",
            "\"postCreateCommand\":\"echo uid=$(id -u) > /workspace/no-uid-remap.txt\"}]'\n",
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
    let devcontainer_json = r#"{
  "name": "Compose No UID Remap",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace",
  "updateRemoteUserUID": false
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

    let marker = workspace.join("no-uid-remap.txt");
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
        "uid=1234",
        "updateRemoteUserUID: false must leave the image's uid alone on the compose path"
    );
}

/// Write a compose workspace whose lifecycle hooks span every command form.
///
/// Four sequential phases APPEND to one marker file, so its line order is the
/// phase order. The two `postStartCommand` named commands write their OWN files:
/// the spec does not order named commands, so asserting them inside the shared
/// file would pin an order neither CLI promises.
///
/// `image_tag` is the compose service image — a stock alpine when the test does
/// not need image metadata, a locally built one when it does. `onCreateCommand`
/// is declared in the devcontainer.json only when the caller asks for it; the
/// full-phase-set test contributes that phase from the image label instead.
fn write_lifecycle_matrix_workspace(workspace: &Path, image_tag: &str, on_create_in_config: bool) {
    let compose_yml = format!(
        "services:\n  app:\n    image: {}\n    volumes:\n      - .:/workspace\n    command: sleep infinity\n",
        image_tag
    );
    fs::write(workspace.join("docker-compose.yml"), compose_yml).unwrap();

    let on_create = if on_create_in_config {
        "  \"onCreateCommand\": \"echo onCreate-string >> /workspace/lifecycle-phases.txt\",\n"
    } else {
        ""
    };
    let devcontainer_json = format!(
        r#"{{
  "name": "Compose Lifecycle Phases",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace",
{on_create}  "updateContentCommand": ["sh", "-c", "echo updateContent-array >> /workspace/lifecycle-phases.txt"],
  "postCreateCommand": ["sh", "-c", "echo postCreate-array >> /workspace/lifecycle-phases.txt"],
  "postStartCommand": {{
    "alpha": "echo postStart-object-alpha > /workspace/lifecycle-poststart-alpha.txt",
    "beta": ["sh", "-c", "echo postStart-object-beta > /workspace/lifecycle-poststart-beta.txt"]
  }},
  "postAttachCommand": "echo postAttach-string >> /workspace/lifecycle-phases.txt"
}}"#
    );

    fs::create_dir_all(workspace.join(".devcontainer")).unwrap();
    fs::write(
        workspace.join(".devcontainer/devcontainer.json"),
        devcontainer_json,
    )
    .unwrap();
}

/// The ordered phase log written by [`write_lifecycle_matrix_workspace`]'s
/// sequential hooks, or `None` when no hook wrote anything at all.
fn phase_log(workspace: &Path) -> Option<String> {
    fs::read_to_string(workspace.join("lifecycle-phases.txt")).ok()
}

/// #460: a compose `up` runs the FULL lifecycle phase set, in every command form.
///
/// Before the fix, `up`'s compose path carried its own post-create exec whose
/// body was `if let Some(cmd_str) = post_create_cmd.as_str()`, so it ran
/// `postCreateCommand` only in its STRING form and queued no other phase.
/// Measured against the pinned reference CLI 0.87.0 on this shape: the reference
/// ran all six commands and deacon ran NONE of them — the `postCreateCommand`
/// here is the array form, so `as_str()` yielded `None` and nothing was queued.
/// Both sides exited 0, which is why the markers are the entire observation.
///
/// One run pins four things: WHICH phases run (five, where the old path had
/// one), in WHAT order (the shared file's line order), in WHICH command form
/// (string, array and object all appear), and that the object form runs EVERY
/// named command. `onCreateCommand` is contributed by the IMAGE's
/// `devcontainer.metadata` and by nothing else, so its line also proves an
/// image-contributed hook reaches the compose lifecycle (#448's merge) in a
/// phase the old path never ran at all.
#[test]
fn test_compose_up_runs_full_lifecycle_phase_set() {
    if !is_docker_available() {
        eprintln!("Skipping test_compose_up_runs_full_lifecycle_phase_set: Docker not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();
    let _down = DeaconDownGuard(workspace);

    // Unique tag so parallel runs in the docker-shared group never collide.
    let image_tag = format!(
        "deacon-test-compose-lifecycle-phases-{}-{}:local",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    );
    let _image = ImageGuard(image_tag.clone());

    let image_dir = workspace.join("image");
    fs::create_dir_all(&image_dir).unwrap();
    fs::write(
        image_dir.join("Dockerfile"),
        concat!(
            "FROM alpine:3.19\n",
            "LABEL devcontainer.metadata='[{\"onCreateCommand\":",
            "\"echo onCreate-string >> /workspace/lifecycle-phases.txt\"}]'\n",
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

    write_lifecycle_matrix_workspace(workspace, &image_tag, false);

    let up_output = support::deacon_command()
        .current_dir(workspace)
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .output()
        .unwrap();
    assert!(
        up_output.status.success(),
        "deacon up failed: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );

    let log = phase_log(workspace).unwrap_or_else(|| {
        panic!(
            "no lifecycle phase ran at all (up stderr: {})",
            String::from_utf8_lossy(&up_output.stderr)
        )
    });
    assert_eq!(
        log,
        "onCreate-string\nupdateContent-array\npostCreate-array\npostAttach-string\n",
        "every sequential phase must run, in phase order; onCreate comes from the \
         image label, the rest from the workspace config (up stderr: {})",
        String::from_utf8_lossy(&up_output.stderr)
    );

    // The object form runs EVERY named command. Separate files, because the spec
    // does not order named commands against each other.
    for (file, expected) in [
        ("lifecycle-poststart-alpha.txt", "postStart-object-alpha\n"),
        ("lifecycle-poststart-beta.txt", "postStart-object-beta\n"),
    ] {
        let got = fs::read_to_string(workspace.join(file)).unwrap_or_else(|e| {
            panic!("the object-form postStartCommand should have written {file}: {e}")
        });
        assert_eq!(got, expected, "{file}");
    }
}

/// #460/#476: `--skip-post-create` means the same thing on compose as on a single
/// container — it defers the WHOLE lifecycle, `onCreate` and `updateContent`
/// included.
///
/// This test previously asserted the opposite (base setup runs, postCreate onward
/// is skipped), read off the flag's NAME. #476 measured the pinned oracle 0.87.0:
/// `devcontainer up --skip-post-create` runs no hook at all, because it sets
/// `postCreateEnabled: false` and the whole lifecycle runner is gated on it. The
/// flag is spec-silent, so the reference is the authority.
///
/// What compose must NOT do is re-decide this locally: the rule lives in
/// `InvocationContext::should_skip_phase`, which both paths share, so the compose
/// path stays a caller rather than a second copy that can drift.
#[test]
fn test_compose_up_skip_post_create_defers_every_phase() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_compose_up_skip_post_create_defers_every_phase: Docker not available"
        );
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();
    let _down = DeaconDownGuard(workspace);

    write_lifecycle_matrix_workspace(workspace, "alpine:3.19", true);

    let up_output = support::deacon_command()
        .current_dir(workspace)
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--skip-post-create")
        .output()
        .unwrap();
    assert!(
        up_output.status.success(),
        "deacon up --skip-post-create failed: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );

    assert_eq!(
        phase_log(workspace),
        None,
        "--skip-post-create must defer EVERY phase, onCreate and updateContent \
         included, so the shared phase log must not exist at all \
         (up stderr: {})",
        String::from_utf8_lossy(&up_output.stderr)
    );
    assert!(
        !workspace.join("lifecycle-poststart-alpha.txt").exists(),
        "postStart is a non-blocking phase after postCreate; --skip-post-create must skip it"
    );
}

/// #460: `--skip-non-blocking-commands` stops the compose lifecycle at the
/// configured `waitFor` phase, which defaults to `updateContentCommand`.
///
/// Same rule as the single-container path, and it reaches compose for the same
/// reason: the phase queue is built once, in `up::lifecycle`, by
/// `should_queue_phase_for_wait_for`.
#[test]
fn test_compose_up_skip_non_blocking_commands_stops_at_wait_for() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_compose_up_skip_non_blocking_commands_stops_at_wait_for: Docker not available"
        );
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();
    let _down = DeaconDownGuard(workspace);

    write_lifecycle_matrix_workspace(workspace, "alpine:3.19", true);

    let up_output = support::deacon_command()
        .current_dir(workspace)
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--skip-non-blocking-commands")
        .output()
        .unwrap();
    assert!(
        up_output.status.success(),
        "deacon up --skip-non-blocking-commands failed: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );

    assert_eq!(
        phase_log(workspace).as_deref(),
        Some("onCreate-string\nupdateContent-array\n"),
        "the default waitFor is updateContentCommand, so everything after it is \
         non-blocking and must not run (up stderr: {})",
        String::from_utf8_lossy(&up_output.stderr)
    );
    assert!(
        !workspace.join("lifecycle-poststart-beta.txt").exists(),
        "postStart runs after the waitFor cutoff and must be skipped"
    );
}

/// #467 (compose path): the same-phase collection, measured where the service
/// image carries the metadata.
///
/// The defect is path-INDEPENDENT — it lives in the merge that folds the image's
/// `devcontainer.metadata` into the config, and `up`'s single-container path,
/// `up`'s compose path (since #448) and `container_metadata::
/// resolve_config_against_container` (`exec` / `run-user-commands`, since #405)
/// all share it. A single-path test would misattribute it to whichever path it
/// happened to cover, so both paths are pinned; the sibling lives in
/// `integration_feature_lifecycle`.
///
/// Same shape as that sibling and for the same reasons: `onCreate` and
/// `postCreate` are declared on BOTH sides (and `postCreate` in the two
/// different command forms, so the collection is not quietly form-sensitive),
/// `postStart` by the image ONLY so a double-run is visible, and the whole log
/// is asserted rather than the presence of any one line.
#[test]
fn test_compose_up_collects_image_metadata_and_config_hooks_for_the_same_phase() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_compose_up_collects_image_metadata_and_config_hooks_for_the_same_phase: Docker not available"
        );
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();
    let _down = DeaconDownGuard(workspace);

    let image_tag = format!(
        "deacon-test-compose-same-phase-hooks-{}-{}:local",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    );
    let _image = ImageGuard(image_tag.clone());

    // The hooks run as root here (the config's `remoteUser`, which must win over
    // the image metadata's `metauser`), so the bind-mounted workspace is
    // writable without any uid remap standing between the fix and the marker.
    let image_dir = workspace.join("image");
    fs::create_dir_all(&image_dir).unwrap();
    fs::write(
        image_dir.join("Dockerfile"),
        concat!(
            "FROM alpine:3.19\n",
            "RUN adduser -D -u 1234 metauser\n",
            "LABEL devcontainer.metadata='[{\"remoteUser\":\"metauser\",",
            "\"onCreateCommand\":\"echo img-onCreate >> /workspace/lifecycle.log\",",
            "\"postCreateCommand\":\"echo img-postCreate >> /workspace/lifecycle.log\",",
            "\"postStartCommand\":\"echo img-postStart >> /workspace/lifecycle.log\"}]'\n",
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
    let devcontainer_json = r#"{
  "name": "Compose Same-Phase Hooks",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace",
  "remoteUser": "root",
  "onCreateCommand": "echo ws-onCreate >> /workspace/lifecycle.log",
  "postCreateCommand": ["/bin/sh", "-c", "echo ws-postCreate >> /workspace/lifecycle.log; whoami > /workspace/hook-user.txt"]
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

    let log = fs::read_to_string(workspace.join("lifecycle.log")).unwrap_or_else(|e| {
        panic!(
            "the lifecycle hooks should have written lifecycle.log: {} (up stderr: {})",
            e,
            String::from_utf8_lossy(&up_output.stderr)
        )
    });
    let lines: Vec<&str> = log
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();

    assert_eq!(
        lines,
        vec![
            "img-onCreate",
            "ws-onCreate",
            "img-postCreate",
            "ws-postCreate",
            "img-postStart",
        ],
        "on the COMPOSE path too, image-metadata and config hooks for the same \
         phase must both run, image first, and an image-only phase exactly ONCE \
         (#467). Got: {:?}",
        lines
    );

    let hook_user = fs::read_to_string(workspace.join("hook-user.txt"))
        .expect("the postCreate hook should have recorded its user");
    assert_eq!(
        hook_user.trim(),
        "root",
        "remoteUser stays 'Last value wins' on the compose path: the config's \
         `root` must beat the image metadata's `metauser`"
    );
}

/// #564: `up` names the transition to the readable compose project name out loud.
///
/// Compose prefixes every named volume with the project name, so renaming the project
/// leaves the previous project's volumes intact but INVISIBLE to the new one. Two
/// transitions produce that silently — an older deacon's `deacon_<wsHash>_<cfgHash>`
/// project, and a `<folder>_devcontainer` project someone arrived with from the reference
/// CLI — and neither is covered by `stop_superseded_containers`, which stops CONTAINERS
/// and deliberately never touches volumes.
///
/// Both are staged here as real Docker volumes carrying Compose's own
/// `com.docker.compose.project` label, which is the handle the detection reads: no volume
/// NAME is parsed, so the assertion is about the label the daemon reports and not about
/// how either tool spells a resource. The second `up` reconnects rather than
/// re-provisions (same configuration), which is the point — the diagnostic is emitted
/// before that branch, so it is reported on every shape of the call.
#[test]
fn test_compose_up_reports_superseded_project_volumes() {
    if !is_docker_available() {
        eprintln!(
            "Skipping test_compose_up_reports_superseded_project_volumes: Docker not available"
        );
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();
    let _down = DeaconDownGuard(workspace);

    let compose_yml = r#"services:
  app:
    image: alpine:3.18
    command: ["sleep", "infinity"]
"#;
    let devcontainer_json = r#"{
  "name": "Compose Superseded Project",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace"
}"#;
    fs::write(workspace.join("docker-compose.yml"), compose_yml).unwrap();
    fs::create_dir(workspace.join(".devcontainer")).unwrap();
    fs::write(
        workspace.join(".devcontainer/devcontainer.json"),
        devcontainer_json,
    )
    .unwrap();

    // First `up` establishes the project and, with it, the workspace hash — read out of
    // deacon's own reported name rather than recomputed, so the test cannot drift from
    // the derivation it is checking.
    let first = support::deacon_command()
        .current_dir(workspace)
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--skip-post-create")
        .output()
        .expect("deacon up should run");
    assert!(
        first.status.success(),
        "first up failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let project = up_project_name(&first).expect("up should report a composeProjectName");

    let fields: Vec<&str> = project.split('_').collect();
    assert_eq!(
        fields.len(),
        4,
        "expected deacon_<stem>_<wsHash>_<cfgHash>, got {project}"
    );
    assert_eq!(fields[0], "deacon");
    let workspace_hash = fields[2];

    // The stem is the sanitized workspace basename. `tempfile` names its directories
    // `.tmpAbC123`, so this also exercises the leading-dot trim and the lowercasing on a
    // real path rather than only in a unit test.
    let basename = workspace
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let expected_stem = deacon_core::compose::sanitize_project_stem(&basename);
    assert!(
        !expected_stem.is_empty(),
        "a tempfile basename must sanitize to something"
    );
    assert_eq!(fields[1], expected_stem, "stem of {project}");

    // Stage both superseded projects: volumes carrying the Compose project label of a
    // pre-#564 deacon project for THIS workspace, and of a reference-CLI project for it.
    let legacy_project = format!("deacon_{workspace_hash}_0badc0de");
    let reference_project = format!("{expected_stem}_devcontainer");
    let staged = [
        (
            format!("{legacy_project}_probe-data"),
            legacy_project.clone(),
        ),
        (
            format!("{reference_project}_probe-data"),
            reference_project.clone(),
        ),
    ];
    for (volume, owner) in &staged {
        let created = std::process::Command::new("docker")
            .args([
                "volume",
                "create",
                "--label",
                &format!("com.docker.compose.project={owner}"),
                volume,
            ])
            .output()
            .expect("docker volume create should run");
        assert!(
            created.status.success(),
            "docker volume create failed: {}",
            String::from_utf8_lossy(&created.stderr)
        );
    }

    let second = support::deacon_command()
        .current_dir(workspace)
        .arg("up")
        .arg("--workspace-folder")
        .arg(workspace)
        .arg("--skip-post-create")
        .output()
        .expect("deacon up should run");

    // Remove the staged volumes before asserting, so a failed assertion cannot leak them.
    for (volume, _) in &staged {
        let _ = std::process::Command::new("docker")
            .args(["volume", "rm", "-f", volume])
            .output();
    }

    assert!(
        second.status.success(),
        "second up failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        stderr.contains(&legacy_project),
        "the diagnostic must name the older deacon project. stderr:\n{stderr}"
    );
    assert!(
        stderr.contains(&reference_project),
        "the diagnostic must name the reference CLI's project. stderr:\n{stderr}"
    );
    assert!(
        stderr.contains(&project),
        "the diagnostic must name the project deacon WILL use. stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("prefixes named volumes with the project name"),
        "the diagnostic must say WHY the old volumes are invisible. stderr:\n{stderr}"
    );
}
