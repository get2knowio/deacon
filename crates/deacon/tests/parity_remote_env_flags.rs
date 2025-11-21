//! Parity checks for remote environment flag handling across up and exec.

use assert_cmd::Command;
use tempfile::TempDir;

const REMOTE_ENV_ERROR: &str =
    "Invalid remote-env format: 'INVALID_NO_EQUALS'. Expected: NAME=value";

#[test]
fn remote_env_validation_message_matches_for_up_and_exec() {
    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();
    let ws_arg = ws.to_string_lossy();

    let up_output = Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(ws)
        .arg("up")
        .arg("--workspace-folder")
        .arg(ws_arg.as_ref())
        .arg("--remote-env")
        .arg("INVALID_NO_EQUALS")
        .assert()
        .failure()
        .get_output()
        .to_owned();

    let up_stderr = String::from_utf8_lossy(&up_output.stderr);
    assert!(
        up_stderr.contains(REMOTE_ENV_ERROR),
        "up stderr missing remote-env validation message: {}",
        up_stderr
    );

    let exec_output = Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(ws)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(ws_arg.as_ref())
        .arg("--env")
        .arg("INVALID_NO_EQUALS")
        .arg("echo")
        .arg("test")
        .assert()
        .failure()
        .get_output()
        .to_owned();

    let exec_stderr = String::from_utf8_lossy(&exec_output.stderr);
    assert!(
        exec_stderr.contains(REMOTE_ENV_ERROR),
        "exec stderr missing remote-env validation message: {}",
        exec_stderr
    );
}

#[test]
fn remote_env_accepts_empty_values_for_up_and_exec() {
    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();
    let ws_arg = ws.to_string_lossy();

    let up_output = Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(ws)
        .arg("up")
        .arg("--workspace-folder")
        .arg(ws_arg.as_ref())
        .arg("--remote-env")
        .arg("EMPTY=")
        .assert()
        .failure()
        .get_output()
        .to_owned();

    let up_stderr = String::from_utf8_lossy(&up_output.stderr);
    assert!(
        !up_stderr.contains("Invalid remote-env format"),
        "up should not reject empty remote env values: {}",
        up_stderr
    );

    let exec_output = Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(ws)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(ws_arg.as_ref())
        .arg("--env")
        .arg("EMPTY=")
        .arg("echo")
        .arg("test")
        .assert()
        .failure()
        .get_output()
        .to_owned();

    let exec_stderr = String::from_utf8_lossy(&exec_output.stderr);
    assert!(
        !exec_stderr.contains("Invalid remote-env format"),
        "exec should not reject empty remote env values: {}",
        exec_stderr
    );
}
