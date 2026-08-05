//! Docker-backed proof that ONE Feature installs several times when the document really
//! names several Features: once per declared version (#430) and once per requested option
//! set (#489).
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

/// Write the reference's `dependsOn/local-with-options` shape into `<dir>/.devcontainer`:
/// five local Features where `./b` is requested with five different option sets.
///
/// `./b`'s `install.sh` drops one marker per option set it is executed with, so the image
/// itself records how many distinct instances really ran.
fn write_option_set_fixture(dir: &std::path::Path) {
    let config_dir = dir.join(".devcontainer");
    let metadata = [
        (
            "a",
            r#"{ "id": "a", "version": "0.0.1",
                 "dependsOn": { "./b": { "optA": "a", "optB": "a" }, "./c": {} },
                 "options": { "optA": { "type": "string", "default": "0" },
                              "optB": { "type": "string", "default": "0" } } }"#,
        ),
        (
            "b",
            r#"{ "id": "b", "version": "0.0.1",
                 "options": { "optA": { "type": "string", "default": "0" },
                              "optB": { "type": "string", "default": "0" } } }"#,
        ),
        (
            "c",
            r#"{ "id": "c", "version": "0.0.1",
                 "dependsOn": { "./b": { "optA": "b", "optB": "a" }, "./d": {}, "./e": {} } }"#,
        ),
        (
            "d",
            r#"{ "id": "d", "version": "0.0.1",
                 "dependsOn": { "./b": { "optA": "b", "optB": "b" } } }"#,
        ),
        (
            "e",
            r#"{ "id": "e", "version": "0.0.1", "dependsOn": { "./b": {} } }"#,
        ),
    ];

    for (name, body) in metadata {
        let d = config_dir.join(name);
        fs::create_dir_all(&d).expect("create feature dir");
        fs::write(d.join("devcontainer-feature.json"), body).expect("write feature metadata");
        // `./b` records the option set it ran with; the others just prove they ran.
        let script = if name == "b" {
            "#!/bin/sh\nset -e\nmkdir -p /markers\ntouch \"/markers/b-${OPTA}-${OPTB}\"\n"
        } else {
            "#!/bin/sh\nset -e\nmkdir -p /markers\ntouch /markers/FEATURE\n"
        };
        let script = script.replace("FEATURE", name);
        fs::write(d.join("install.sh"), script).expect("write install.sh");
    }

    fs::write(
        config_dir.join("devcontainer.json"),
        r#"{
  "name": "OptionSetInstances",
  "image": "debian:bookworm-slim",
  "features": {
    "./a": { "optA": "a", "optB": "b" },
    "./b": { "optA": "a", "optB": "b" }
  }
}
"#,
    )
    .expect("write devcontainer.json");
}

/// #489 — a local Feature depended on with FIVE different option sets installs five
/// times, once per set.
///
/// `feature-dependencies.md` §Definition: Feature Equality: "two Features [are] equal if
/// both Features point to the same exact contents **and are executed with the same
/// options**", and §(B1) skips a `dependsOn` target only "if the **exact** Feature […]
/// has already been added".
///
/// The same trap as #430 applies, which is why this asserts on image CONTENTS: making the
/// graph accept distinct nodes is necessary but not sufficient. Staging directories,
/// install ordering, the option env vars and the install loop must each carry the
/// instances, and every one of those failures still exits 0 with a plausible-looking JSON
/// result. Only the markers inside the image distinguish five installs from one.
#[test]
fn build_installs_one_instance_per_requested_option_set() {
    if !is_runtime_available() {
        eprintln!("Skipping: no container runtime available");
        return;
    }

    let workspace = TempDir::new().expect("temp workspace");
    write_option_set_fixture(workspace.path());

    let image_tag = format!("{}:test", unique_name("deacon-optset-features"));

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

        let listing = StdCommand::new(runtime_bin())
            .args(["run", "--rm", "--entrypoint", "ls", &image_tag, "/markers"])
            .output()
            .expect("failed to list /markers in the built image");
        assert!(
            listing.status.success(),
            "listing /markers failed: {}",
            String::from_utf8_lossy(&listing.stderr)
        );
        let mut markers: Vec<String> = String::from_utf8_lossy(&listing.stdout)
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        markers.sort();

        // The unrequested options fall back to `./b`'s declared defaults ("0"), so the
        // `{}` instance is `b-0-0`. Before #489 only `b-a-b` existed: the configuration's
        // option set, handed to all four dependents that asked for their own.
        assert_eq!(
            markers,
            vec![
                "a", "b-0-0", "b-a-a", "b-a-b", "b-b-a", "b-b-b", "c", "d", "e",
            ],
            "`./b` must install once per distinct option set it was requested with"
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
