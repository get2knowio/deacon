//! Smoke tests for down command behavior
//!
//! Scenarios covered:
//! - Down command before any up: should succeed or gracefully handle "no container"
//! - Down command after up: should successfully tear down (Docker-gated)
//! - Idempotent down behavior: subsequent down calls should not error
//!
//! NOTE: These tests assume Docker is available and running. They will fail
//! if Docker is not present or cannot start containers.

mod support;

use std::fs;
use tempfile::TempDir;

/// The container runtime binary under test (honors `DEACON_CONTAINER_RUNTIME`,
/// the same env var deacon reads). Stale-container setup must use this so it
/// lands in the store deacon-under-podman actually sweeps.
fn runtime_bin() -> String {
    std::env::var("DEACON_CONTAINER_RUNTIME").unwrap_or_else(|_| "docker".to_string())
}

fn is_docker_available() -> bool {
    std::process::Command::new(runtime_bin())
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Test down command before any up: should succeed or gracefully handle "no container"
#[test]
fn test_down_before_up() {
    let temp_dir = TempDir::new().unwrap();

    // Create minimal devcontainer.json
    let devcontainer_config = r#"{
    "name": "Down Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test down command before any up
    let mut down_cmd = support::deacon_command();
    let down_output = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let down_stderr = String::from_utf8_lossy(&down_output.stderr);
    // Expected: should succeed when no container to tear down
    // If CLI chooses to report "no container" as non-zero, allow known message
    if !down_output.status.success() {
        assert!(
            down_stderr.contains("No running containers")
                || down_stderr.contains("no container")
                || down_stderr.contains("not found"),
            "Down before up failed unexpectedly: {}",
            down_stderr
        );
    }
}

/// Test down command after up and idempotent behavior (Docker-gated)
#[test]
fn test_down_after_up_idempotent() {
    if !is_docker_available() {
        eprintln!("Skipping test_down_after_up_idempotent: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create minimal devcontainer.json
    let devcontainer_config = r#"{
    "name": "Down After Up Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // First: up command
    let mut up_cmd = support::deacon_command();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--skip-post-create")
        .arg("--skip-non-blocking-commands")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    assert!(
        up_output.status.success(),
        "Up command failed: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );

    // Second: down command (should succeed)
    let mut down_cmd = support::deacon_command();
    let down_output = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let down_stderr = String::from_utf8_lossy(&down_output.stderr);

    assert!(
        down_output.status.success(),
        "Down command failed after up: {}",
        down_stderr
    );

    println!("Down after up succeeded");

    // Third: down command again (should be idempotent, not error)
    let mut down_cmd2 = support::deacon_command();
    let down_output2 = down_cmd2
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let down_stderr2 = String::from_utf8_lossy(&down_output2.stderr);

    if !down_output2.status.success() {
        assert!(
            down_stderr2.contains("No running containers")
                || down_stderr2.contains("no container")
                || down_stderr2.contains("not found"),
            "Unexpected error in second down: {}",
            down_stderr2
        );
    }
}

/// Test `down --all` sweeps *every* container carrying this workspace's
/// `devcontainer.local_folder` label — including a stale container that was
/// NOT created by deacon (no `source`/hash labels) and whose config never
/// matched. Regression test for `--all` over-pinning on `config_hash`.
#[test]
fn test_down_all_sweeps_stale_by_local_folder() {
    if !is_docker_available() {
        eprintln!("Skipping test_down_all_sweeps_stale_by_local_folder: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();
    // The workspace is NAMED in canonical form, so it is also what deacon writes to the
    // devcontainer.local_folder label and what we filter on below. Since #665 deacon
    // absolutizes rather than canonicalizes, so what goes in is what comes out — passing the
    // canonical spelling here keeps the filter exact on macOS's `/var` symlink too.
    let workspace = temp_dir.path().canonicalize().unwrap();

    let devcontainer_config = r#"{
    "name": "Down All Stale Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace"
}"#;
    fs::create_dir(workspace.join(".devcontainer")).unwrap();
    fs::write(
        workspace.join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Bring up the deacon-managed container.
    let up_output = support::deacon_command()
        .arg("up")
        .arg("--skip-post-create")
        .arg("--skip-non-blocking-commands")
        .arg("--workspace-folder")
        .arg(&workspace)
        .output()
        .unwrap();
    assert!(
        up_output.status.success(),
        "Up failed: {}",
        String::from_utf8_lossy(&up_output.stderr)
    );

    // Create a *stale* container that only carries the workspace's
    // local_folder label (simulating a container from an older deacon run
    // whose state file / config no longer matches).
    let local_folder_label = format!("devcontainer.local_folder={}", workspace.display());
    let stale = std::process::Command::new(runtime_bin())
        .args([
            "run",
            "-d",
            "--rm",
            "--label",
            &local_folder_label,
            "alpine:3.19",
            "sleep",
            "300",
        ])
        .output()
        .unwrap();
    assert!(
        stale.status.success(),
        "Failed to create stale container: {}",
        String::from_utf8_lossy(&stale.stderr)
    );
    let stale_id = String::from_utf8_lossy(&stale.stdout).trim().to_string();

    // Count containers with this workspace label before sweep (expect >= 2).
    let count_label = || -> usize {
        let out = std::process::Command::new(runtime_bin())
            .args([
                "ps",
                "-a",
                "--filter",
                &format!("label={}", local_folder_label),
                "-q",
            ])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count()
    };
    assert!(
        count_label() >= 2,
        "expected at least the deacon container + stale container before sweep"
    );

    // Sweep everything with --all --remove.
    let down_output = support::deacon_command()
        .arg("down")
        .arg("--workspace-folder")
        .arg(&workspace)
        .arg("--all")
        .arg("--remove")
        .arg("--force")
        .output()
        .unwrap();

    let remaining = count_label();
    // Best-effort cleanup of the stale container regardless of assertion outcome.
    let _ = std::process::Command::new(runtime_bin())
        .args(["rm", "-f", &stale_id])
        .output();

    assert!(
        down_output.status.success(),
        "down --all failed: {}",
        String::from_utf8_lossy(&down_output.stderr)
    );
    assert_eq!(
        remaining, 0,
        "down --all --remove must sweep ALL containers labeled for this workspace (including the stale, non-deacon one); {remaining} remained"
    );
}

/// A teardown that races another removal of the same container must still leave
/// the container gone — and must say so truthfully.
///
/// Docker answers a second concurrent `rm -f` with `removal of container <id> is
/// already in progress`, and the container is **still present** when it does
/// (measured 6/6). Before [#688] deacon issued a single `rm -f` and took that
/// answer at face value, which broke in two opposite directions:
///
/// * `down --remove` propagated it and exited **1** with the container present;
/// * `down --all --remove` classified it as "already gone" and exited **0** with
///   the container present — a teardown that reported success over a container
///   that was still there.
///
/// The reference retries until the removal actually completes (`removeContainer`,
/// `src/spec-shutdown/dockerUtils.ts` at v0.87.0).
///
/// **The assertion is the container's ABSENCE, not the exit code.** The `--all`
/// shape already exited 0 while failing, so an exit-code assertion could not have
/// caught it — and that is the half most likely to be written by reflex.
///
/// Both shapes run because they failed in opposite directions and a fix for one
/// is not a fix for the other.
///
/// What this test does NOT cover: the STOP step's half of the race. Narrowing the
/// shared predicate fixed the removal and regressed `--all` to exit 2, because a
/// container already being removed cannot be stopped either — but reinstating that
/// bug and re-running this test passes more often than not, since whether the stop
/// loses the race is timing. That distinction is pinned deterministically by
/// `commands::down::tests::already_in_progress_is_not_already_gone` instead. Do not
/// read a green run here as covering it.
///
/// [#688]: https://github.com/get2knowio/deacon/issues/688
#[test]
fn down_wins_a_race_with_a_concurrent_removal() {
    if !support::is_runtime_available() {
        eprintln!("skipping: container runtime unavailable");
        return;
    }
    // The race needs `docker rm -f`'s exact "already in progress" wording.
    if !support::runtime_is_docker() {
        eprintln!("skipping: concurrent-removal wording is docker-specific");
        return;
    }

    for flags in [vec!["--remove"], vec!["--all", "--remove"]] {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
        fs::write(
            temp_dir.path().join(".devcontainer/devcontainer.json"),
            r#"{"name":"Down Race","image":"debian:bookworm-slim","overrideCommand":true}"#,
        )
        .unwrap();

        let up = support::deacon_command()
            .args(["up", "--workspace-folder"])
            .arg(temp_dir.path())
            .args(["--mount-workspace-git-root", "false"])
            .output()
            .unwrap();
        if !up.status.success() {
            eprintln!(
                "skipping {flags:?}: up failed: {}",
                String::from_utf8_lossy(&up.stderr)
            );
            return;
        }

        // Take the id from `up`'s own result document rather than rebuilding the
        // identity label here: the label's spelling is normalized (#682) and a
        // test that reconstructs it is testing its own arithmetic.
        let up_stdout = String::from_utf8_lossy(&up.stdout).to_string();
        let container_id = support::extract_json_from_output(&up_stdout)
            .ok()
            .and_then(|v| v.get("containerId")?.as_str().map(str::to_string))
            .unwrap_or_default();
        assert!(
            !container_id.is_empty(),
            "up reported no containerId; stdout:\n{up_stdout}"
        );

        // Widen the removal window so the race is reliable rather than lucky:
        // a container with real bytes in its writable layer takes measurably
        // longer to delete than an empty one.
        let _ = std::process::Command::new(support::runtime_bin())
            .args([
                "exec",
                &container_id,
                "sh",
                "-c",
                "dd if=/dev/zero of=/big bs=1M count=800 2>/dev/null",
            ])
            .output();

        let racer_id = container_id.clone();
        let racer = std::thread::spawn(move || {
            let _ = std::process::Command::new(support::runtime_bin())
                .args(["rm", "-f", &racer_id])
                .output();
        });
        std::thread::sleep(std::time::Duration::from_millis(50));

        let down = support::deacon_command()
            .args(["down", "--workspace-folder"])
            .arg(temp_dir.path())
            .args(&flags)
            .output()
            .unwrap();

        let still_there = std::process::Command::new(support::runtime_bin())
            .args([
                "ps",
                "-a",
                "--filter",
                &format!("id={container_id}"),
                "--format",
                "{{.ID}}",
            ])
            .output()
            .unwrap();
        let remaining = String::from_utf8_lossy(&still_there.stdout)
            .trim()
            .to_string();

        let _ = racer.join();
        let _ = std::process::Command::new(support::runtime_bin())
            .args(["rm", "-f", &container_id])
            .output();

        assert!(
            remaining.is_empty(),
            "down {flags:?} returned while container {container_id} was still present \
             (exit {:?}); a teardown must not report completion before the container is gone.\n\
             stderr:\n{}",
            down.status.code(),
            String::from_utf8_lossy(&down.stderr)
        );
        assert!(
            down.status.success(),
            "down {flags:?} failed on a benign removal race; stderr:\n{}",
            String::from_utf8_lossy(&down.stderr)
        );
    }
}
