//! Integration test: `up --secrets-file` redacts secret values in lifecycle output.
//!
//! Regression guard for the plaintext-leak bug: a secret injected via
//! `--secrets-file` and echoed by a lifecycle command was streamed to deacon's
//! stderr verbatim (the reference CLI prints `********`). deacon now registers
//! `--secrets-file` values in the global redaction registry and redacts the
//! `stdout_to_stderr` lifecycle stream line-by-line. `--no-redact` still surfaces
//! the raw value (debugging escape hatch).

use assert_cmd::Command;
use std::fs;
use std::process::Command as StdCommand;
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

const SECRET: &str = "topsecret_marker_9f3a2b";

/// Run `up` with a `.env` secrets file whose `MY_SECRET` is echoed by
/// `postCreateCommand`. Returns (stderr, container_id). Caller removes the id.
fn run_up(temp: &TempDir, extra: &[&str]) -> (String, String) {
    let dc = temp.path().join(".devcontainer");
    fs::create_dir_all(&dc).unwrap();
    fs::write(
        temp.path().join("secrets.env"),
        format!("MY_SECRET={SECRET}\n"),
    )
    .unwrap();
    fs::write(
        dc.join("devcontainer.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "name": "secrets-redaction",
            "image": "debian:bookworm-slim",
            "postCreateCommand": "echo \"the-secret-is=$MY_SECRET\""
        }))
        .unwrap(),
    )
    .unwrap();

    let secrets_path = temp.path().join("secrets.env");
    let mut args = vec![
        "up",
        "--workspace-folder",
        temp.path().to_str().unwrap(),
        "--remove-existing-container",
        "--mount-workspace-git-root=false",
        "--secrets-file",
        secrets_path.to_str().unwrap(),
    ];
    args.extend_from_slice(extra);

    let out = Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(temp.path())
        .env("DEACON_LOG", "info")
        .args(&args)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let cid = serde_json::from_str::<serde_json::Value>(stdout.trim())
        .ok()
        .or_else(|| {
            stdout
                .rfind('{')
                .and_then(|i| serde_json::from_str(&stdout[i..]).ok())
        })
        .and_then(|v| v.get("containerId")?.as_str().map(str::to_string))
        .unwrap_or_default();
    (stderr, cid)
}

fn rm(cid: &str) {
    if !cid.is_empty() {
        let _ = StdCommand::new("docker").args(["rm", "-f", cid]).output();
    }
}

#[test]
fn secret_value_is_redacted_in_lifecycle_output() {
    if !is_docker_available() {
        eprintln!("Skipping secret_value_is_redacted_in_lifecycle_output: Docker not available");
        return;
    }
    let temp = TempDir::new().unwrap();
    let (stderr, cid) = run_up(&temp, &[]);
    rm(&cid);

    // The postCreate ran (its line prefix reaches the stream)...
    assert!(
        stderr.contains("the-secret-is="),
        "postCreate output should be streamed to stderr:\n{stderr}"
    );
    // ...but the secret VALUE must not appear in plaintext anywhere.
    assert!(
        !stderr.contains(SECRET),
        "secret value leaked in lifecycle output (should be redacted):\n{stderr}"
    );
}

#[test]
fn no_redact_surfaces_secret_value() {
    if !is_docker_available() {
        eprintln!("Skipping no_redact_surfaces_secret_value: Docker not available");
        return;
    }
    let temp = TempDir::new().unwrap();
    let (stderr, cid) = run_up(&temp, &["--no-redact"]);
    rm(&cid);

    // The debugging escape hatch leaves the value un-redacted.
    assert!(
        stderr.contains(SECRET),
        "--no-redact should surface the raw secret value:\n{stderr}"
    );
}
