//! Smoke tests for exec command behavior parity
//!
//! Scenarios covered:
//! - Exec behavior parity: TTY detection, exit code propagation, stdin streaming
//! - Working directory and --remote-env support
//! - remoteEnv and metadata interactions
//! - Compose/subfolder config + markers
//!
//! NOTE: These tests assume Docker is available and running. They will fail
//! if Docker is not present or cannot start containers.

mod support;

use std::fs;
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

/// Test exec without TTY prints expected stdout
#[test]
fn test_exec_stdout_without_tty() {
    if !is_docker_available() {
        eprintln!("Skipping test_exec_stdout_without_tty: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create a simple devcontainer.json
    let devcontainer_config = r#"{
    "name": "Exec Test Container",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Ensure container is up
    let mut up_cmd = support::deacon_command();
    let up_out = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--skip-post-create")
        .arg("--skip-non-blocking-commands")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
    assert!(
        up_out.status.success(),
        "up failed: {}",
        String::from_utf8_lossy(&up_out.stderr)
    );

    // Test exec command without TTY
    let mut exec_cmd = support::deacon_command();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("echo")
        .arg("Hello from exec")
        .output()
        .unwrap();

    assert!(
        exec_output.status.success(),
        "Unexpected error in exec stdout test: {}",
        String::from_utf8_lossy(&exec_output.stderr)
    );
    let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);
    assert!(
        exec_stdout.contains("Hello from exec"),
        "Exec should output command stdout"
    );

    // Cleanup
    let mut down_cmd = support::deacon_command();
    let _ = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
}

/// Test exec exit code propagation
#[test]
fn test_exec_exit_code_propagation() {
    if !is_docker_available() {
        eprintln!("Skipping test_exec_exit_code_propagation: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    let devcontainer_config = r#"{
    "name": "Exec Exit Code Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Ensure container is up for exit code test
    let mut up_cmd = support::deacon_command();
    let up_out = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--skip-post-create")
        .arg("--skip-non-blocking-commands")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
    assert!(
        up_out.status.success(),
        "up failed: {}",
        String::from_utf8_lossy(&up_out.stderr)
    );

    // Test exec command that exits with specific code
    let mut exec_cmd = support::deacon_command();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("exit 123")
        .output()
        .unwrap();

    // Should propagate exit code 123
    assert_eq!(
        exec_output.status.code(),
        Some(123),
        "Exec should propagate exit code 123, stderr: {}",
        String::from_utf8_lossy(&exec_output.stderr)
    );

    // Cleanup
    let mut down_cmd = support::deacon_command();
    let _ = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
}

/// Test exec working directory behavior
#[test]
fn test_exec_working_directory() {
    if !is_docker_available() {
        eprintln!("Skipping test_exec_working_directory: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    let devcontainer_config = r#"{
    "name": "Exec Working Dir Test",
    "image": "alpine:3.19", 
    "workspaceFolder": "/workspace",
    "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Ensure container is up
    let mut up_cmd = support::deacon_command();
    let up_out = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--skip-post-create")
        .arg("--skip-non-blocking-commands")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
    assert!(
        up_out.status.success(),
        "up failed: {}",
        String::from_utf8_lossy(&up_out.stderr)
    );

    // Test exec with working directory
    let mut exec_cmd = support::deacon_command();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("pwd")
        .output()
        .unwrap();

    assert!(
        exec_output.status.success(),
        "Unexpected error in exec working directory test: {}",
        String::from_utf8_lossy(&exec_output.stderr)
    );
    let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);
    // Should be in workspace folder
    assert!(
        exec_stdout.trim().ends_with("workspace") || exec_stdout.contains("/workspace"),
        "Exec should run in workspace directory, got: {}",
        exec_stdout
    );

    // Cleanup
    let mut down_cmd = support::deacon_command();
    let _ = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
}

/// Test exec --env merges environment variables
#[test]
fn test_exec_env_merges() {
    if !is_docker_available() {
        eprintln!("Skipping test_exec_env_merges: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    let devcontainer_config = r#"{
    "name": "Exec Env Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Ensure container is up
    let mut up_cmd = support::deacon_command();
    let up_out = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--skip-post-create")
        .arg("--skip-non-blocking-commands")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
    assert!(
        up_out.status.success(),
        "up failed: {}",
        String::from_utf8_lossy(&up_out.stderr)
    );

    // Test exec with --env
    let mut exec_cmd = support::deacon_command();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--env")
        .arg("FOO=BAR")
        .arg("--env")
        .arg("BAZ=") // empty value
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("echo FOO=$FOO BAZ=$BAZ")
        .output()
        .unwrap();

    assert!(
        exec_output.status.success(),
        "Unexpected error in exec --env test: {}",
        String::from_utf8_lossy(&exec_output.stderr)
    );
    let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);
    // Should contain the env values
    assert!(
        exec_stdout.contains("FOO=BAR"),
        "Should have FOO=BAR from --env"
    );
    assert!(
        exec_stdout.contains("BAZ="),
        "Should have empty BAZ from --env"
    );

    // Cleanup
    let mut down_cmd = support::deacon_command();
    let _ = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
}

/// Test up with remoteEnv in config makes values available to lifecycle hooks
#[test]
fn test_up_remote_env_in_config() {
    if !is_docker_available() {
        eprintln!("Skipping test_up_remote_env_in_config: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    let devcontainer_config = r#"{
    "name": "Remote Env Config Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind",
    "remoteEnv": {
        "CONFIG_VAR": "config_value",
        "EMPTY_VAR": ""
    },
    "postCreateCommand": "echo CONFIG_VAR=$CONFIG_VAR EMPTY_VAR=$EMPTY_VAR > /tmp/env_check"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up command with remoteEnv in config
    let mut up_cmd = support::deacon_command();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();

    let up_stderr = String::from_utf8_lossy(&up_output.stderr);

    assert!(
        up_output.status.success(),
        "Unexpected error in up remoteEnv test: {}",
        up_stderr
    );

    // Test that we can exec and see the environment
    let mut exec_cmd = support::deacon_command();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("printenv")
        .arg("CONFIG_VAR")
        .output()
        .unwrap();

    assert!(
        exec_output.status.success(),
        "Exec failed: {}",
        String::from_utf8_lossy(&exec_output.stderr)
    );
    let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);
    assert!(
        exec_stdout.contains("config_value"),
        "remoteEnv should be available in exec"
    );
}

/// Test exec with --config in subfolder works
#[test]
fn test_exec_subfolder_config() {
    if !is_docker_available() {
        eprintln!("Skipping test_exec_subfolder_config: Docker not available");
        return;
    }
    let temp_dir = TempDir::new().unwrap();

    // Create config in a subfolder
    let subfolder = temp_dir.path().join("subfolder");
    fs::create_dir_all(&subfolder).unwrap();

    let devcontainer_config = r#"{
    "name": "Subfolder Config Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind",
    "postCreateCommand": "echo 'subfolder-postCreate' > /tmp/marker_subfolder"
}"#;

    fs::create_dir(subfolder.join(".devcontainer")).unwrap();
    fs::write(
        subfolder.join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test up with config in subfolder
    let mut up_cmd = support::deacon_command();
    let up_output = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--workspace-folder")
        .arg(&subfolder)
        .arg("--config")
        .arg(subfolder.join(".devcontainer/devcontainer.json"))
        .output()
        .unwrap();

    let up_stderr = String::from_utf8_lossy(&up_output.stderr);

    assert!(
        up_output.status.success(),
        "Unexpected error in subfolder config test (up): {}",
        up_stderr
    );

    // Test exec with --config in subfolder
    let mut exec_cmd = support::deacon_command();
    let exec_output = exec_cmd
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(&subfolder)
        .arg("--config")
        .arg(subfolder.join(".devcontainer/devcontainer.json"))
        .arg("echo")
        .arg("subfolder exec works")
        .output()
        .unwrap();

    assert!(exec_output.status.success());
    let exec_stdout = String::from_utf8_lossy(&exec_output.stdout);
    assert!(
        exec_stdout.contains("subfolder exec works"),
        "Exec should work with subfolder config"
    );
}

/// Test TTY detection behavior
/// `deacon exec` gives the container command a terminal exactly when deacon
/// itself has one — and passes the exit code and stdin through it.
///
/// This replaces a test named `test_exec_tty_detection` that asserted nothing:
/// it ran `exec test -t 0` and accepted either outcome, commenting that "both
/// success and failure are valid". Nothing anywhere in the suite proved that a
/// command run through `deacon exec` ever receives a terminal —
/// `integration_exec_pty.rs` is mock-based and asserts the `tty` flag on the
/// `ExecConfig` deacon builds, which is the decision rather than its effect.
///
/// The three PTY claims come from the reference CLI's own e2e suite
/// (`src/test/cli.exec.base.ts` at v0.87.0, the `shellPtyExec` arms): the
/// command runs in a terminal, its exit code survives, and its stdin is
/// connected. Measured at oracle 0.87.0 against deacon on this exact fixture —
/// both CLIs agree on all three.
///
/// **Why `script(1)` and not a PTY crate.** Allocating a pty from Rust means
/// `openpty` + `dup2` in a `pre_exec` hook, which this workspace's
/// `unsafe_code = "deny"` rules out, or a new dependency that exists to
/// encapsulate that same `unsafe`. `script -qec CMD /dev/null` runs `CMD` on a
/// pty and, with `-e`, returns its exit status — no dependency, no `unsafe`.
/// It is util-linux syntax, so this is `#[cfg(target_os = "linux")]`: a visible
/// non-selection rather than a runtime skip, and the Docker-bearing CI lanes
/// are Linux.
///
/// **The first assertion is what makes the rest mean something.** Without a pty
/// the container command must NOT see a terminal, and with one it must. Either
/// half alone would pass against a deacon that hard-coded `-t` on or off.
#[test]
#[cfg(target_os = "linux")]
fn exec_propagates_a_terminal_to_the_container_command() {
    if !is_docker_available() {
        eprintln!(
            "Skipping exec_propagates_a_terminal_to_the_container_command: Docker not available"
        );
        return;
    }
    assert!(
        std::path::Path::new("/usr/bin/script").exists(),
        "script(1) from util-linux is required to allocate a pty for this test; \
         skipping silently would restore the vacuous assertion it replaced"
    );

    let temp_dir = TempDir::new().unwrap();

    let devcontainer_config = r#"{
    "name": "Exec Terminal Propagation Test",
    "image": "alpine:3.19",
    "workspaceFolder": "/workspace",
    "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind"
}"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    let mut up_cmd = support::deacon_command();
    let up_out = up_cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--skip-post-create")
        .arg("--skip-non-blocking-commands")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
    assert!(
        up_out.status.success(),
        "up failed: {}",
        String::from_utf8_lossy(&up_out.stderr)
    );

    let workspace = temp_dir.path().display().to_string();
    let deacon = assert_cmd::cargo::cargo_bin("deacon").display().to_string();

    // (1) No pty: the container command must not see a terminal. Upstream's own
    // `[ ! -t 1 ]` arm, run the ordinary way.
    let mut no_pty = support::deacon_command();
    let no_pty_out = no_pty
        .current_dir(&temp_dir)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("[")
        .arg("!")
        .arg("-t")
        .arg("1")
        .arg("]")
        .output()
        .unwrap();
    assert_eq!(
        no_pty_out.status.code(),
        Some(0),
        "without a terminal of its own, exec must not give the command one: {}",
        String::from_utf8_lossy(&no_pty_out.stderr)
    );

    // (2) Under a pty: the command runs in a terminal.
    let (code, _) = run_under_pty(
        temp_dir.path(),
        &format!("{deacon} exec --workspace-folder '{workspace}' [ -t 1 ]"),
        None,
    );
    assert_eq!(
        code,
        Some(0),
        "under a terminal, exec must give the container command one"
    );

    // (3) Under a pty: the exit code survives the round trip.
    let (code, _) = run_under_pty(
        temp_dir.path(),
        &format!("{deacon} exec --workspace-folder '{workspace}' sh -c 'exit 123'"),
        None,
    );
    assert_eq!(code, Some(123), "exit code must survive the pty");

    // (4) Under a pty: stdin reaches the command, and its output comes back.
    let (code, output) = run_under_pty(
        temp_dir.path(),
        &format!("{deacon} exec --workspace-folder '{workspace}' sh"),
        Some("FOO=BAR\necho ${FOO}hi${FOO}\nexit\n"),
    );
    assert_eq!(code, Some(0), "interactive shell must exit cleanly");
    assert!(
        output.contains("BARhiBAR"),
        "stdin must reach the shell and its output come back; got: {output}"
    );

    let mut down_cmd = support::deacon_command();
    let _ = down_cmd
        .current_dir(&temp_dir)
        .arg("down")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .output()
        .unwrap();
}

/// Run `command` on a pty via `script(1)`, returning its exit code and output.
///
/// `-q` suppresses script's own banner, `-e` makes script return the child's
/// exit status rather than its own, and the transcript goes to `/dev/null`
/// because the pty's output is already on script's stdout, which is what the
/// caller reads.
#[cfg(target_os = "linux")]
fn run_under_pty(
    cwd: &std::path::Path,
    command: &str,
    stdin: Option<&str>,
) -> (Option<i32>, String) {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("script")
        .current_dir(cwd)
        // The child inherits the environment, so the workspace-state isolation
        // `support::deacon_command()` applies has to be set here by hand.
        .env("TMPDIR", support::isolated_home_for_external_spawn())
        .env("TMP", support::isolated_home_for_external_spawn())
        .env("TEMP", support::isolated_home_for_external_spawn())
        .args(["-qec", command, "/dev/null"])
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn script(1)");

    if let Some(payload) = stdin {
        child
            .stdin
            .take()
            .expect("stdin was piped")
            .write_all(payload.as_bytes())
            .expect("failed to write stdin to the pty");
    }

    let out = child
        .wait_with_output()
        .expect("script(1) did not complete");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}
