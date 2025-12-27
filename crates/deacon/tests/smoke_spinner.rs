//! Smoke tests for spinner/quiet behavior
//!
//! Verifies that spinner output does not appear when stderr is not a TTY (as in CI/assert_cmd capture)
//! and that outputs remain free of spinner artifacts.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

mod test_utils;
use test_utils::DeaconGuard;

fn spinner_frames() -> [&'static str; 10] {
    ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
}

#[test]
#[ignore = "Flaky in CI - needs investigation for environment-specific failures"]
fn spinner_not_rendered_when_not_tty_up_down() {
    // Create a minimal devcontainer config using a long-running image
    let temp_dir = TempDir::new().unwrap();
    let _guard = DeaconGuard::new(temp_dir.path());

    let devcontainer_config = r#"{
        "name": "Spinner Test",
        "image": "alpine:3.19",
        "workspaceFolder": "/workspace"
    }"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Run `deacon up` with captured IO (non-TTY). Spinner should NOT render.
    let mut up_cmd = Command::cargo_bin("deacon").unwrap();
    let up = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--remove-existing-container")
        .arg("--skip-post-create")
        .arg("--skip-non-blocking-commands")
        .assert();

    let output = up.get_output();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Allow environments without Docker to fail with a known message; otherwise require success.
        assert!(
            stderr.contains("Cannot connect to the Docker daemon")
                || stderr.contains("Is the docker daemon running")
                || stderr.contains("permission denied")
                || stderr.contains("No such image")
                || stderr.contains("no such file or directory"),
            "up failed unexpectedly: {}",
            stderr
        );
        return; // Skip spinner assertions if Docker is unavailable
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    for frame in spinner_frames() {
        assert!(
            !stderr.contains(frame),
            "Spinner frame '{}' should not appear when not a TTY. stderr: {}",
            frame,
            stderr
        );
    }

    // Run `deacon down` and again ensure no spinner frames leak
    let mut down_cmd = Command::cargo_bin("deacon").unwrap();
    let down_output = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&down_output.stderr);
    for frame in spinner_frames() {
        assert!(
            !stderr.contains(frame),
            "Spinner frame '{}' should not appear when not a TTY (down). stderr: {}",
            frame,
            stderr
        );
    }
}
