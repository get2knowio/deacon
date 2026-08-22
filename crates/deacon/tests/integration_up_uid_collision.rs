//! `updateRemoteUserUID` must not remap `remoteUser` onto a UID another user
//! already owns ([#618](https://github.com/get2knowio/deacon/issues/618)).
//!
//! The reference CLI's `scripts/updateUID.Dockerfile` has always carried the
//! guard:
//!
//! ```sh
//! elif [ "$OLD_UID" != "$NEW_UID" -a -n "$EXISTING_USER" ]; then \
//!     echo "User with UID exists ($EXISTING_USER=$NEW_UID)."; \
//! ```
//!
//! Without it the container ends up with two `/etc/passwd` entries sharing one
//! uid, and every name lookup for that uid resolves to the OTHER user: `id -un`
//! prints `decoy` for a configuration that named `target`, lifecycle markers get
//! the wrong owner, and even `exec --user target` lands on `decoy` — while the
//! `up` result document still reports `"remoteUser": "target"`. That last part
//! is why nothing reading the outcome could see this, and why this test asserts
//! the identity INSIDE the container and the result document TOGETHER.
//!
//! **The decoy is pinned to the host's own uid at runtime**, not to a literal.
//! The parity fixture this issue was found through used whatever uids the base
//! image handed out, which made the collision fire in a uid-1000 dev container
//! and silently not fire on a GitHub runner at uid 1001 — a case that reads
//! green while the defect is unfixed. Reading `id -u` here removes the host from
//! the claim entirely.
//!
//! Docker-gated: skips cleanly when Docker is unavailable. Unix-only — the
//! whole subject is `/etc/passwd` uid arithmetic, and `updateRemoteUserUID`
//! defaults to `false` off Linux.

#![cfg(unix)]

use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use tempfile::TempDir;

/// The container runtime binary under test (honors `DEACON_CONTAINER_RUNTIME`,
/// the same env var deacon reads).
fn runtime_bin() -> String {
    std::env::var("DEACON_CONTAINER_RUNTIME").unwrap_or_else(|_| "docker".to_string())
}

fn is_docker_available() -> bool {
    StdCommand::new(runtime_bin())
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn deacon() -> StdCommand {
    StdCommand::new(env!("CARGO_BIN_EXE_deacon"))
}

fn write(ws: &Path, rel: &str, body: &str) {
    let path = ws.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// `id -u` on the host — the uid `updateRemoteUserUID` will try to move the
/// remote user to.
fn host_uid() -> u32 {
    let out = StdCommand::new("id")
        .arg("-u")
        .output()
        .expect("spawn id -u");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .expect("parse id -u")
}

fn down(ws: &Path) {
    let _ = deacon()
        .args(["down", "--workspace-folder"])
        .arg(ws)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// `deacon exec --workspace-folder <ws> -- sh -c '<script>'`, trimmed stdout.
fn exec_sh(ws: &Path, script: &str) -> String {
    let out = deacon()
        .args(["exec", "--workspace-folder"])
        .arg(ws)
        .args(["--", "sh", "-c", script])
        .stderr(Stdio::inherit())
        .output()
        .expect("spawn deacon exec");
    assert!(
        out.status.success(),
        "`deacon exec` failed for script {script:?}"
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn up_skips_the_uid_remap_when_the_host_uid_is_already_taken() {
    if !is_docker_available() {
        eprintln!("skipping: docker unavailable");
        return;
    }

    let uid = host_uid();
    if uid == 0 {
        // deacon skips UID mapping entirely for a root host user, so there is
        // no remap to refuse and the case would pass vacuously.
        eprintln!("skipping: host user is root, updateRemoteUserUID is a no-op");
        return;
    }
    // Well clear of the decoy and of anything the base image assigns.
    let target_uid = uid + 4000;

    let ws = TempDir::new().unwrap();
    write(
        ws.path(),
        ".devcontainer/Dockerfile",
        &format!(
            "FROM alpine:3.19\n\
             RUN adduser decoy --disabled-password --uid {uid}\n\
             RUN adduser target --disabled-password --uid {target_uid}\n"
        ),
    );
    write(
        ws.path(),
        ".devcontainer/devcontainer.json",
        r#"{
  "build": { "dockerfile": "Dockerfile" },
  "remoteUser": "target",
  "updateRemoteUserUID": true,
  "overrideCommand": true
}
"#,
    );

    let out = deacon()
        .args([
            "up",
            "--workspace-folder",
            ws.path().to_str().unwrap(),
            "--mount-workspace-git-root",
            "false",
        ])
        .stderr(Stdio::inherit())
        .output()
        .expect("spawn deacon up");
    assert!(
        out.status.success(),
        "`deacon up` failed: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let result: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("up result document is JSON");

    // The result document's claim about who the container runs as …
    assert_eq!(
        result.get("remoteUser").and_then(|v| v.as_str()),
        Some("target"),
        "up result document: {result}"
    );

    // … and the container's own answer, which is the half that used to lie.
    let identity = exec_sh(
        ws.path(),
        "id -un; id -u; awk -F: -v u=\"$(id -u)\" '$3==u' /etc/passwd | wc -l",
    );
    let lines: Vec<&str> = identity.lines().map(str::trim).collect();

    assert_eq!(
        lines.first().copied(),
        Some("target"),
        "the container resolved its uid to the wrong user — the remap stamped \
         `target` onto a uid `decoy` already owned (#618). Full output: {identity:?}"
    );
    assert_eq!(
        lines.get(1).copied(),
        Some(target_uid.to_string().as_str()),
        "`target` must keep its image-assigned uid {target_uid}: the host uid {uid} is \
         taken, so the reference refuses the remap outright. Full output: {identity:?}"
    );
    // A name check alone would pass for a passwd file that merely happens to
    // list `target` first; this asserts the uid is not shared at all.
    assert_eq!(
        lines.get(2).copied(),
        Some("1"),
        "exactly one passwd entry may own the effective uid — two entries sharing \
         one uid is the #618 defect itself. Full output: {identity:?}"
    );

    // The refused remap must not have taken the host's workspace with it: the
    // TempDir is still owned by the user running the test.
    let meta = std::fs::metadata(ws.path()).expect("stat workspace");
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            meta.uid(),
            uid,
            "the workspace was chowned to a uid the host user does not own"
        );
    }

    down(ws.path());
}
