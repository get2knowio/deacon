//! Docker-backed proof that a `features` map naming ONE Feature at TWO versions installs
//! BOTH of them (#430).
//!
//! `feature-dependencies.md` (spec `113500f4`) settles the shape in three passages:
//! §Definition: Feature Equality makes two OCI Features equal only when their manifest
//! digests AND options are equal — so two tags are two Features; §Feature authorship
//! states that "a single Feature may be installed more than once"; and §Definition: Round
//! Stable Sort supplies the tie-break, "Compare and sort each Feature from oldest to
//! newest tag", for exactly the case where the resource names compare equal.
//!
//! **Why this test exists and a JSON `outcome` assertion would not do.** deacon used to
//! REJECT the document at parse time. Deleting that rejection alone makes
//! `read-configuration` match the reference byte-for-byte and then silently drops the
//! second Feature: one fetch, one staged directory, one `RUN` stage, exit 0. A loud
//! rejection replaced by a silent drop is a worse bug than the one being fixed, and only
//! looking INSIDE the produced image can tell the two apart.
//!
//! The marker is `/etc/passwd`. Each entry asks `common-utils` for a differently-named
//! user, so one line per install lands in the image and the two installs cannot overwrite
//! each other — and because the feature assigns UIDs in sequence, the same file also
//! records WHICH ran first. The fixture declares the NEWER tag first so declaration order
//! and tag order disagree: an implementation that installed in map order would give the
//! newer tag the lower UID and be caught.
//!
//! Named `integration_up_*` deliberately: the binary inherits that glob's
//! `docker-slow-shared` group, 30-minute timeout and `dev-fast` exclusion in every
//! nextest profile, and it exercises the feature staging/dependency pipeline that `up`
//! and `build` share (`commands::up::features_build`).

mod support;

use std::fs;
use std::process::Command as StdCommand;

use support::{deacon_command, is_runtime_available, runtime_bin, unique_name};
use tempfile::TempDir;

/// The older of the two `common-utils` tags, and the user it creates.
const OLDER_TAG: &str = "2.5.3";
const OLDER_USER: &str = "olderdup";
/// The newer tag, declared FIRST in the document.
const NEWER_TAG: &str = "2.5.4";
const NEWER_USER: &str = "newerdup";

/// Best-effort removal of an image and every tag pointing at it.
fn remove_image(reference: &str) {
    let _ = StdCommand::new(runtime_bin())
        .args(["rmi", "-f", reference])
        .output();
}

/// `cat` a path inside a fresh throwaway container from `image`.
fn read_file_in_image(image: &str, path: &str) -> String {
    let out = StdCommand::new(runtime_bin())
        .args(["run", "--rm", "--entrypoint", "cat", image, path])
        .output()
        .expect("failed to run the built image");
    assert!(
        out.status.success(),
        "reading {path} from {image} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The numeric UID of `user` in an `/etc/passwd` body, or `None` if absent.
fn uid_of(passwd: &str, user: &str) -> Option<u32> {
    passwd
        .lines()
        .find(|l| l.starts_with(&format!("{user}:")))
        .and_then(|l| l.split(':').nth(2))
        .and_then(|uid| uid.parse().ok())
}

#[test]
fn build_installs_both_versions_of_one_feature_oldest_tag_first() {
    if !is_runtime_available() {
        eprintln!("Skipping: no container runtime available");
        return;
    }

    let workspace = TempDir::new().expect("temp workspace");
    let config_dir = workspace.path().join(".devcontainer");
    fs::create_dir_all(&config_dir).expect("create .devcontainer");
    // The NEWER tag is declared first on purpose — see the module docs.
    fs::write(
        config_dir.join("devcontainer.json"),
        format!(
            r#"{{
  "name": "DuplicateFeatureBuild",
  "image": "debian:bookworm-slim",
  "features": {{
    "ghcr.io/devcontainers/features/common-utils:{NEWER_TAG}": {{
      "username": "{NEWER_USER}",
      "installZsh": false,
      "installOhMyZsh": false,
      "upgradePackages": false
    }},
    "ghcr.io/devcontainers/features/common-utils:{OLDER_TAG}": {{
      "username": "{OLDER_USER}",
      "installZsh": false,
      "installOhMyZsh": false,
      "upgradePackages": false
    }}
  }}
}}
"#
        ),
    )
    .expect("write devcontainer.json");

    let image_tag = format!("{}:test", unique_name("deacon-dup-features"));

    let output = deacon_command()
        .args([
            "build",
            "--workspace-folder",
            workspace.path().to_str().expect("utf-8 workspace path"),
            "--image-name",
            &image_tag,
        ])
        .output()
        .expect("failed to run deacon build");

    // Resolve the image id up front so cleanup can drop every tag deacon minted for it
    // (the deterministic `deacon-build:*` / `deacon-devcontainer-features:*` names as
    // well as ours), not just the one we asked for.
    let image_id = StdCommand::new(runtime_bin())
        .args(["inspect", "-f", "{{.Id}}", &image_tag])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    let result = std::panic::catch_unwind(|| {
        assert!(
            output.status.success(),
            "deacon build failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let passwd = read_file_in_image(&image_tag, "/etc/passwd");

        // BOTH installs landed. Before #430 deacon rejected the document outright; with
        // only the parse fix, exactly one of these lines exists and exit is still 0.
        let older_uid = uid_of(&passwd, OLDER_USER).unwrap_or_else(|| {
            panic!("the Feature at :{OLDER_TAG} did not install — no {OLDER_USER} in /etc/passwd:\n{passwd}")
        });
        let newer_uid = uid_of(&passwd, NEWER_USER).unwrap_or_else(|| {
            panic!("the Feature at :{NEWER_TAG} did not install — no {NEWER_USER} in /etc/passwd:\n{passwd}")
        });

        // And in the spec's order. `common-utils` allocates the next free UID, so the
        // Feature that ran first holds the lower one; the document declares :{NEWER_TAG}
        // first, so a map-order install would invert this.
        assert!(
            older_uid < newer_uid,
            "§Round Stable Sort orders equal resource names oldest tag first, so \
             :{OLDER_TAG} must install before :{NEWER_TAG} — got {OLDER_USER}={older_uid}, \
             {NEWER_USER}={newer_uid}\n{passwd}"
        );
    });

    remove_image(&image_tag);
    if let Some(id) = image_id {
        remove_image(&id);
    }

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}
