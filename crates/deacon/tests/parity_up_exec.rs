//! Parity tests comparing deacon vs upstream devcontainer CLI for `up` and `exec`.
//!
//! Assumes Docker is available and `devcontainer` is installed. No gating.

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use tempfile::TempDir;

mod parity_utils;

fn upstream_bin() -> String {
    std::env::var("DEACON_PARITY_DEVCONTAINER").unwrap_or_else(|_| "devcontainer".to_string())
}

#[test]
fn parity_up_and_exec_traditional() {
    if !parity_utils::upstream_available() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }
    // workspace with alpine image and a postCreate marker
    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();

    fs::create_dir(ws.join(".devcontainer")).unwrap();
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        r#"{
  "name": "ParityUpExec",
  "image": "alpine:3.19",
  "workspaceFolder": "/workspaces/${localWorkspaceFolderBasename}",
  "postCreateCommand": "sh -lc 'echo ready > /tmp/parity_marker'"
}
"#,
    )
    .unwrap();

    // upstream: up + exec cat marker
    let mut up1 = std::process::Command::new(upstream_bin());
    up1.current_dir(ws);
    up1.arg("up");
    up1.arg("--workspace-folder");
    up1.arg(ws);
    let st1 = up1.output().unwrap();
    assert!(
        st1.status.success(),
        "upstream up failed: {}",
        String::from_utf8_lossy(&st1.stderr)
    );

    let mut ex1 = std::process::Command::new(upstream_bin());
    ex1.current_dir(ws);
    ex1.arg("exec");
    ex1.arg("--workspace-folder");
    ex1.arg(ws);
    ex1.arg("sh");
    ex1.arg("-lc");
    ex1.arg("cat /tmp/parity_marker && pwd");
    let e1 = ex1.output().unwrap();
    assert!(
        e1.status.success(),
        "upstream exec failed: {}",
        String::from_utf8_lossy(&e1.stderr)
    );
    let out1 = String::from_utf8_lossy(&e1.stdout);
    assert!(out1.contains("ready"), "upstream marker missing: {}", out1);

    // ours: up + exec cat marker
    let mut up2 = Command::cargo_bin("deacon").unwrap();
    let st2 = up2
        .current_dir(ws)
        .arg("up")
        .arg("--workspace-folder")
        .arg(ws)
        .assert()
        .get_output()
        .to_owned();
    assert!(
        st2.status.success(),
        "deacon up failed: {}",
        String::from_utf8_lossy(&st2.stderr)
    );

    let mut ex2 = Command::cargo_bin("deacon").unwrap();
    let e2 = ex2
        .current_dir(ws)
        .arg("exec")
        .arg("--workspace-folder")
        .arg(ws)
        .arg("--")
        .arg("sh")
        .arg("-lc")
        .arg("cat /tmp/parity_marker && pwd")
        .assert()
        .get_output()
        .to_owned();
    assert!(
        e2.status.success(),
        "deacon exec failed: {}",
        String::from_utf8_lossy(&e2.stderr)
    );
    let out2 = String::from_utf8_lossy(&e2.stdout);
    assert!(out2.contains("ready"), "deacon marker missing: {}", out2);

    // Label parity checks
    // Upstream container: should be identifiable by devcontainer.local_folder and config_file labels
    fn docker_out(args: &[&str]) -> String {
        let out = std::process::Command::new("docker")
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "docker {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    let ws_str = ws.to_string_lossy().to_string();
    // Find upstream container ID by matching labels and image
    let upstream_id = {
        let format = "{{.ID}}";
        let list = docker_out(&[
            "ps",
            "--filter",
            &format!("label=devcontainer.local_folder={}", ws_str),
            "--filter",
            "ancestor=alpine:3.19",
            "--format",
            format,
        ]);
        assert!(
            !list.is_empty(),
            "no upstream container found with devcontainer.local_folder={}",
            ws_str
        );
        list.lines().next().unwrap().to_string()
    };
    let upstream_labels_json =
        docker_out(&["inspect", "-f", "{{ json .Config.Labels }}", &upstream_id]);
    let upstream_labels: Value = serde_json::from_str(&upstream_labels_json).unwrap_or(Value::Null);
    let ul = upstream_labels.as_object().expect("upstream labels object");
    // Assert key upstream labels exist and match workspace
    assert_eq!(
        ul.get("devcontainer.local_folder").and_then(|v| v.as_str()),
        Some(ws_str.as_str()),
        "upstream devcontainer.local_folder mismatch"
    );
    assert_eq!(
        ul.get("devcontainer.config_file").and_then(|v| v.as_str()),
        Some(
            ws.join(".devcontainer/devcontainer.json")
                .to_string_lossy()
                .as_ref()
        ),
        "upstream devcontainer.config_file mismatch"
    );
    assert!(
        ul.keys().any(|k| k.starts_with("devcontainer.")),
        "upstream labels missing devcontainer.* keys"
    );

    // Deacon container: identify by devcontainer.name and devcontainer.source=deacon
    let deacon_id = {
        let format = "{{.ID}}";
        let list = docker_out(&[
            "ps",
            "--filter",
            "label=devcontainer.source=deacon",
            "--filter",
            "label=devcontainer.name=ParityUpExec",
            "--filter",
            "ancestor=alpine:3.19",
            "--format",
            format,
        ]);
        assert!(
            !list.is_empty(),
            "no deacon container found with devcontainer.name=ParityUpExec"
        );
        list.lines().next().unwrap().to_string()
    };
    let deacon_labels_json =
        docker_out(&["inspect", "-f", "{{ json .Config.Labels }}", &deacon_id]);
    let deacon_labels: Value = serde_json::from_str(&deacon_labels_json).unwrap_or(Value::Null);
    let dl = deacon_labels.as_object().expect("deacon labels object");
    assert_eq!(
        dl.get("devcontainer.name").and_then(|v| v.as_str()),
        Some("ParityUpExec"),
        "deacon devcontainer.name mismatch"
    );
    assert_eq!(
        dl.get("devcontainer.source").and_then(|v| v.as_str()),
        Some("deacon"),
        "deacon devcontainer.source mismatch"
    );
    assert!(
        dl.keys().any(|k| k.starts_with("devcontainer.")),
        "deacon labels missing devcontainer.* keys"
    );

    // Note: We don't assert exact label key equality across CLIs because upstream and deacon
    // use different labeling schemes. We verify that each assigns the expected, identifying
    // devcontainer.* labels tied to the workspace/name semantics.
}
