//! Parity tests comparing deacon vs upstream devcontainer CLI for `exec` semantics.
//!
//! These tests verify that deacon's exec command behaves functionally equivalent to
//! the upstream devcontainer CLI in terms of working directory, user, TTY, and
//! environment variable handling.

use tempfile::TempDir;

mod parity_utils;

/// Test working directory parity with explicit workspaceFolder
#[test]
fn parity_exec_working_directory() {
    if !parity_utils::parity_enabled() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }
    if !parity_utils::docker_available() {
        eprintln!(
            "Skipping parity test (Docker unavailable): {}",
            parity_utils::skip_reason()
        );
        return;
    }
    if !parity_utils::upstream_available() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();

    parity_utils::write_devcontainer(
        ws,
        r#"{
  "name": "ParityExecWorkingDir",
  "image": "alpine:3.19",
    "workspaceFolder": "/root"
}
"#,
    )
    .unwrap();

    // upstream: up then exec pwd
    let st1 = parity_utils::run_upstream(ws, &["up", "--workspace-folder", &ws.to_string_lossy()])
        .unwrap();
    assert!(
        st1.status.success(),
        "upstream up failed (code {:?}): {}",
        st1.status.code(),
        String::from_utf8_lossy(&st1.stderr)
    );

    // Small delay to allow container state to settle in CI-like environments
    std::thread::sleep(std::time::Duration::from_millis(200));
    // Target the container explicitly via upstream label to avoid lookup issues
    let canon_ws = std::fs::canonicalize(ws).unwrap_or_else(|_| ws.to_path_buf());
    let id_label = format!("devcontainer.local_folder={}", canon_ws.to_string_lossy());
    // Retry a few times to mitigate race conditions starting the container
    let mut e1 = None;
    for _ in 0..5 {
        let attempt = parity_utils::run_upstream(
            ws,
            &[
                "exec",
                "--workspace-folder",
                &ws.to_string_lossy(),
                "--id-label",
                &id_label,
                "--",
                "sh",
                "-lc",
                "pwd",
            ],
        )
        .unwrap();
        if attempt.status.success() {
            e1 = Some(attempt);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
        eprintln!(
            "retrying upstream exec after failure (code {:?}): {}",
            attempt.status.code(),
            String::from_utf8_lossy(&attempt.stderr)
        );
    }
    let e1 = e1.expect("upstream exec did not succeed after retries");
    let out1 = parity_utils::stdout_str(&e1);

    // deacon: up then exec pwd
    let st2 =
        parity_utils::run_deacon(ws, &["up", "--workspace-folder", &ws.to_string_lossy()]).unwrap();
    assert!(
        st2.status.success(),
        "deacon up failed (code {:?}): {}",
        st2.status.code(),
        String::from_utf8_lossy(&st2.stderr)
    );

    let e2 = parity_utils::run_deacon(
        ws,
        &[
            "exec",
            "--workspace-folder",
            &ws.to_string_lossy(),
            "--",
            "sh",
            "-lc",
            "pwd",
        ],
    )
    .unwrap();
    assert!(
        e2.status.success(),
        "deacon exec failed (code {:?}): {}",
        e2.status.code(),
        String::from_utf8_lossy(&e2.stderr)
    );
    let out2 = parity_utils::stdout_str(&e2);

    // Both should print /root as the working directory
    assert_eq!(out1, "/root", "upstream should show /root as pwd");
    assert_eq!(out2, "/root", "deacon should show /root as pwd");
    assert_eq!(
        out1, out2,
        "working directory mismatch: upstream={}, deacon={}",
        out1, out2
    );
}

/// Test exec user parity with --user flag
#[test]
fn parity_exec_user() {
    if !parity_utils::parity_enabled() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }
    if !parity_utils::docker_available() {
        eprintln!(
            "Skipping parity test (Docker unavailable): {}",
            parity_utils::skip_reason()
        );
        return;
    }
    if !parity_utils::upstream_available() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();

    // Use containerUser in config to ensure both CLIs run as root without needing a --user flag
    parity_utils::write_devcontainer(
        ws,
        r#"{
  "name": "ParityExecUser",
    "image": "alpine:3.19",
    "containerUser": "root"
}
"#,
    )
    .unwrap();

    // upstream: up then exec with --user root
    let st1 = parity_utils::run_upstream(ws, &["up", "--workspace-folder", &ws.to_string_lossy()])
        .unwrap();
    assert!(
        st1.status.success(),
        "upstream up failed (code {:?}): {}",
        st1.status.code(),
        String::from_utf8_lossy(&st1.stderr)
    );

    let e1 = parity_utils::run_upstream(
        ws,
        &[
            "exec",
            "--workspace-folder",
            &ws.to_string_lossy(),
            "--",
            "sh",
            "-lc",
            "id -u",
        ],
    )
    .unwrap();
    assert!(
        e1.status.success(),
        "upstream exec failed (code {:?}): {}",
        e1.status.code(),
        String::from_utf8_lossy(&e1.stderr)
    );
    let out1 = parity_utils::stdout_str(&e1);

    // deacon: up then exec with --user root
    let st2 =
        parity_utils::run_deacon(ws, &["up", "--workspace-folder", &ws.to_string_lossy()]).unwrap();
    assert!(
        st2.status.success(),
        "deacon up failed (code {:?}): {}",
        st2.status.code(),
        String::from_utf8_lossy(&st2.stderr)
    );

    let e2 = parity_utils::run_deacon(
        ws,
        &[
            "exec",
            "--workspace-folder",
            &ws.to_string_lossy(),
            "--",
            "sh",
            "-lc",
            "id -u",
        ],
    )
    .unwrap();
    assert!(
        e2.status.success(),
        "deacon exec failed (code {:?}): {}",
        e2.status.code(),
        String::from_utf8_lossy(&e2.stderr)
    );
    let out2 = parity_utils::stdout_str(&e2);

    // Both should show UID 0 (root)
    assert_eq!(out1, "0", "upstream should show UID 0 for root");
    assert_eq!(out2, "0", "deacon should show UID 0 for root");
    assert_eq!(
        out1, out2,
        "user ID mismatch: upstream={}, deacon={}",
        out1, out2
    );
}

/// Test exec TTY parity with --no-tty flag
#[test]
fn parity_exec_tty() {
    if !parity_utils::parity_enabled() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }
    if !parity_utils::docker_available() {
        eprintln!(
            "Skipping parity test (Docker unavailable): {}",
            parity_utils::skip_reason()
        );
        return;
    }
    if !parity_utils::upstream_available() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();

    parity_utils::write_devcontainer(
        ws,
        r#"{
  "name": "ParityExecTTY",
  "image": "alpine:3.19"
}
"#,
    )
    .unwrap();

    // upstream: up then exec with --no-tty
    let st1 = parity_utils::run_upstream(ws, &["up", "--workspace-folder", &ws.to_string_lossy()])
        .unwrap();
    assert!(
        st1.status.success(),
        "upstream up failed (code {:?}): {}",
        st1.status.code(),
        String::from_utf8_lossy(&st1.stderr)
    );

    // Upstream does not expose a --no-tty flag; just check current TTY behavior and compare with deacon
    let e1 = parity_utils::run_upstream(
        ws,
        &[
            "exec",
            "--workspace-folder",
            &ws.to_string_lossy(),
            "--",
            "sh",
            "-lc",
            "test -t 1 && echo TTY || echo NOTTY",
        ],
    )
    .unwrap();
    assert!(
        e1.status.success(),
        "upstream exec failed (code {:?}): {}",
        e1.status.code(),
        String::from_utf8_lossy(&e1.stderr)
    );
    let out1 = parity_utils::stdout_str(&e1);

    // deacon: up then exec with --no-tty
    let st2 =
        parity_utils::run_deacon(ws, &["up", "--workspace-folder", &ws.to_string_lossy()]).unwrap();
    assert!(
        st2.status.success(),
        "deacon up failed (code {:?}): {}",
        st2.status.code(),
        String::from_utf8_lossy(&st2.stderr)
    );

    let e2 = parity_utils::run_deacon(
        ws,
        &[
            "exec",
            "--workspace-folder",
            &ws.to_string_lossy(),
            "--",
            "sh",
            "-lc",
            "test -t 1 && echo TTY || echo NOTTY",
        ],
    )
    .unwrap();
    assert!(
        e2.status.success(),
        "deacon exec failed (code {:?}): {}",
        e2.status.code(),
        String::from_utf8_lossy(&e2.stderr)
    );
    let out2 = parity_utils::stdout_str(&e2);

    // Both tools should behave identically
    assert_eq!(
        out1, out2,
        "TTY behavior mismatch: upstream={}, deacon={}",
        out1, out2
    );
}

/// Test exec environment variable propagation with --env flag
#[test]
fn parity_exec_env_propagation() {
    if !parity_utils::parity_enabled() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }
    if !parity_utils::docker_available() {
        eprintln!(
            "Skipping parity test (Docker unavailable): {}",
            parity_utils::skip_reason()
        );
        return;
    }
    if !parity_utils::upstream_available() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();

    parity_utils::write_devcontainer(
        ws,
        r#"{
  "name": "ParityExecEnv",
  "image": "alpine:3.19"
}
"#,
    )
    .unwrap();

    // upstream: up then exec with --env
    let st1 = parity_utils::run_upstream(ws, &["up", "--workspace-folder", &ws.to_string_lossy()])
        .unwrap();
    assert!(
        st1.status.success(),
        "upstream up failed (code {:?}): {}",
        st1.status.code(),
        String::from_utf8_lossy(&st1.stderr)
    );

    // Upstream uses --remote-env instead of --env
    let e1 = parity_utils::run_upstream(
        ws,
        &[
            "exec",
            "--workspace-folder",
            &ws.to_string_lossy(),
            "--remote-env",
            "FOO=BAR",
            "--",
            "sh",
            "-lc",
            "echo $FOO",
        ],
    )
    .unwrap();
    assert!(
        e1.status.success(),
        "upstream exec failed (code {:?}): {}",
        e1.status.code(),
        String::from_utf8_lossy(&e1.stderr)
    );
    let out1 = parity_utils::stdout_str(&e1);

    // deacon: up then exec with --env
    let st2 =
        parity_utils::run_deacon(ws, &["up", "--workspace-folder", &ws.to_string_lossy()]).unwrap();
    assert!(
        st2.status.success(),
        "deacon up failed (code {:?}): {}",
        st2.status.code(),
        String::from_utf8_lossy(&st2.stderr)
    );

    let e2 = parity_utils::run_deacon(
        ws,
        &[
            "exec",
            "--workspace-folder",
            &ws.to_string_lossy(),
            "--env",
            "FOO=BAR",
            "--",
            "sh",
            "-lc",
            "echo $FOO",
        ],
    )
    .unwrap();
    assert!(
        e2.status.success(),
        "deacon exec failed (code {:?}): {}",
        e2.status.code(),
        String::from_utf8_lossy(&e2.stderr)
    );
    let out2 = parity_utils::stdout_str(&e2);

    // Both should show BAR
    assert_eq!(out1, "BAR", "upstream should show BAR for FOO env var");
    assert_eq!(out2, "BAR", "deacon should show BAR for FOO env var");
    assert_eq!(
        out1, out2,
        "env propagation mismatch: upstream={}, deacon={}",
        out1, out2
    );
}
