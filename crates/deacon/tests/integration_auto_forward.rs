//! Docker integration tests for `up --auto-forward` (dynamic port forwarding).
//!
//! These exercise the detached forwarder end-to-end against a real container.
//! They are docker-gated (skip cleanly when Docker is unavailable) and use a
//! `TempDir` workspace + a `--user-data-folder` so the host-global registry and
//! markers are isolated per test (and because in-repo `up` chowns the
//! workspace — see CLAUDE.md).

use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// A loopback HTTP-ish server inside an alpine container: an `nc` listen loop
/// that emits a banner on each connection. `nc` is busybox's, so it is also the
/// relay program the forwarder discovers. Backgrounded so it survives the
/// (non-TTY) postStart exec session.
const SERVER_BANNER: &str = "deacon-forward-ok";

fn is_docker_available() -> bool {
    StdCommand::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Process-unique-ish suffix without external crates.
fn unique() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}",
        std::process::id(),
        C.fetch_add(1, Ordering::Relaxed)
    )
}

fn write_config(ws: &Path, body: &str) {
    let dir = ws.join(".devcontainer");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("devcontainer.json"), body).unwrap();
}

/// devcontainer.json: alpine, kept alive, with a loopback `nc` banner server on
/// `port` started in postStart, and `forwardPorts` declaring it.
fn server_config(port: u16, declare: bool) -> String {
    let forward = if declare {
        format!(r#""forwardPorts": [{port}],"#)
    } else {
        String::new()
    };
    format!(
        r#"{{
  "name": "auto-forward-test",
  "image": "alpine:3.18",
  "overrideCommand": true,
  {forward}
  "postStartCommand": "sh -c '(while true; do echo {SERVER_BANNER} | nc -l -p {port}; done) >/dev/null 2>&1 & sleep 1'"
}}"#
    )
}

/// devcontainer.json: alpine kept alive, with NO server and NO declared ports.
/// A server is started later via `exec` to exercise auto-detection.
fn bare_config() -> String {
    r#"{
  "name": "auto-forward-bare",
  "image": "alpine:3.18",
  "overrideCommand": true
}"#
    .to_string()
}

struct UpOutcome {
    success: bool,
    container_id: Option<String>,
    stdout: String,
    stderr: String,
}

fn run_up(ws: &Path, udf: &Path, extra: &[&str]) -> UpOutcome {
    let bin = env!("CARGO_BIN_EXE_deacon");
    let mut cmd = StdCommand::new(bin);
    cmd.arg("--user-data-folder")
        .arg(udf)
        .arg("up")
        .arg("--workspace-folder")
        .arg(ws)
        .args(extra);
    let out = cmd.output().expect("run deacon up");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let container_id = serde_json::from_str::<Value>(&stdout).ok().and_then(|v| {
        v.get("containerId")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    UpOutcome {
        success: out.status.success(),
        container_id,
        stdout,
        stderr,
    }
}

/// Run a plain `deacon exec` against the container by id, with NO
/// auto-forward-related flags (proves forwarding needs no exec changes,
/// FR-018). Targeting by `--container-id` is a normal exec feature.
fn run_exec(container_id: &str, command: &[&str]) -> bool {
    let bin = env!("CARGO_BIN_EXE_deacon");
    let mut cmd = StdCommand::new(bin);
    cmd.arg("exec")
        .arg("--container-id")
        .arg(container_id)
        .args(command);
    cmd.output().map(|o| o.status.success()).unwrap_or(false)
}

/// Read the registry and return the host port allocated for `container_port`.
fn registry_host_port(udf: &Path, container_port: u16) -> Option<u16> {
    let text = std::fs::read_to_string(udf.join("forwarded_ports.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    v.get("entries")?.as_array()?.iter().find_map(|e| {
        let cp = e.get("container_port")?.as_u64()? as u16;
        if cp == container_port {
            Some(e.get("host_port")?.as_u64()? as u16)
        } else {
            None
        }
    })
}

/// Host port for a specific owning container id, if present in the registry.
fn registry_host_port_by_container(udf: &Path, container_id: &str) -> Option<u16> {
    let text = std::fs::read_to_string(udf.join("forwarded_ports.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    v.get("entries")?.as_array()?.iter().find_map(|e| {
        if e.get("container_id")?.as_str()? == container_id {
            Some(e.get("host_port")?.as_u64()? as u16)
        } else {
            None
        }
    })
}

/// Poll until the registry has a host port for `container_port` (or timeout).
fn wait_for_host_port(udf: &Path, container_port: u16, timeout: Duration) -> Option<u16> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(p) = registry_host_port(udf, container_port) {
            return Some(p);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    None
}

/// Poll until the registry no longer has an entry for `container_port`.
fn wait_for_withdrawal(udf: &Path, container_port: u16, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if registry_host_port(udf, container_port).is_none() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

/// Connect to a host loopback port, send a minimal request, and read whatever
/// the relayed server sends back. The busybox `nc` server does not half-close,
/// so we read with a per-read timeout and accept partial data (any bytes
/// received prove the relay works) rather than requiring EOF. Retries briefly
/// so the relay's first `docker exec` has time to dial.
fn fetch(host_port: u16, timeout: Duration) -> Option<String> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(mut s) = TcpStream::connect(("127.0.0.1", host_port)) {
            let _ = s.set_read_timeout(Some(Duration::from_secs(3)));
            let _ = s.write_all(b"GET / HTTP/1.0\r\n\r\n");
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                match s.read(&mut chunk) {
                    Ok(0) => break, // EOF
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    Err(_) => break, // timeout / would-block: stop reading
                }
                if buf.len() > 64 {
                    break; // got plenty; the banner is small
                }
            }
            if !buf.is_empty() {
                return Some(String::from_utf8_lossy(&buf).to_string());
            }
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    None
}

/// Host-side liveness check (Linux): does /proc/<pid> exist?
fn host_pid_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn marker_exists(udf: &Path, container_id: &str) -> bool {
    udf.join(format!("forward_daemon_{container_id}.pid"))
        .exists()
}

fn run_down(ws: &Path, udf: &Path) -> bool {
    let bin = env!("CARGO_BIN_EXE_deacon");
    StdCommand::new(bin)
        .arg("--user-data-folder")
        .arg(udf)
        .arg("down")
        .arg("--workspace-folder")
        .arg(ws)
        .arg("--remove")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn read_marker_pid(udf: &Path, container_id: &str) -> Option<u32> {
    let text =
        std::fs::read_to_string(udf.join(format!("forward_daemon_{container_id}.pid"))).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    v.get("pid").and_then(Value::as_u64).map(|p| p as u32)
}

fn kill_pid(pid: u32) {
    let _ = StdCommand::new("kill")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn teardown(ws: &Path, udf: &Path, container_id: Option<&str>) {
    // Reap the forwarder if its marker is still around.
    if let Some(id) = container_id {
        if let Some(pid) = read_marker_pid(udf, id) {
            kill_pid(pid);
        }
        let _ = StdCommand::new("docker")
            .args(["rm", "-f", id])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let bin = env!("CARGO_BIN_EXE_deacon");
    let _ = StdCommand::new(bin)
        .arg("--user-data-folder")
        .arg(udf)
        .arg("down")
        .arg("--workspace-folder")
        .arg(ws)
        .arg("--remove")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// US1 (T010): a loopback-bound container server is reachable on the host via
/// the forwarder, and `up` returns (does not occupy the terminal).
#[test]
fn auto_forward_reaches_loopback_server() {
    if !is_docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let ws = tmp.path().join(format!("ws-{}", unique()));
    std::fs::create_dir_all(&ws).unwrap();
    let udf = tmp.path().join("udf");
    std::fs::create_dir_all(&udf).unwrap();
    write_config(&ws, &server_config(3000, true));

    let up = run_up(&ws, &udf, &["--auto-forward"]);
    assert!(
        up.success,
        "up --auto-forward failed.\n--- stdout ---\n{}\n--- stderr ---\n{}",
        up.stdout, up.stderr
    );

    let host_port = wait_for_host_port(&udf, 3000, Duration::from_secs(15));
    let result = (|| {
        let host_port = host_port?;
        fetch(host_port, Duration::from_secs(15))
    })();

    teardown(&ws, &udf, up.container_id.as_deref());

    let body = result.expect("declared loopback port should be reachable on the host");
    assert!(
        body.contains(SERVER_BANNER),
        "expected banner in relayed response, got: {body:?}"
    );
}

/// US1 (T011): without `--auto-forward`, declared ports use static `-p` and no
/// forwarder process / marker / registry entry is created (backward compat).
#[test]
fn without_auto_forward_no_forwarder_artifacts() {
    if !is_docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let ws = tmp.path().join(format!("ws-{}", unique()));
    std::fs::create_dir_all(&ws).unwrap();
    let udf = tmp.path().join("udf");
    std::fs::create_dir_all(&udf).unwrap();
    write_config(&ws, &server_config(3000, true));

    let up = run_up(&ws, &udf, &[]);
    assert!(
        up.success,
        "plain up failed.\n--- stdout ---\n{}\n--- stderr ---\n{}",
        up.stdout, up.stderr
    );

    // Give a forwarder (if one were wrongly spawned) time to write artifacts.
    std::thread::sleep(Duration::from_secs(2));
    let registry_exists = udf.join("forwarded_ports.json").exists();
    let marker_exists = up
        .container_id
        .as_deref()
        .map(|id| udf.join(format!("forward_daemon_{id}.pid")).exists())
        .unwrap_or(false);

    teardown(&ws, &udf, up.container_id.as_deref());

    assert!(
        !registry_exists,
        "no registry file should exist without --auto-forward"
    );
    assert!(
        !marker_exists,
        "no forwarder marker should exist without --auto-forward"
    );
}

/// US2 (T023 + T027): a port that starts listening AFTER `up` (here via
/// `deacon exec`, with NO exec flags — documents FR-018 transparency) is
/// auto-detected and forwarded within the detection window; when it stops
/// listening the forward is withdrawn and the host port released.
#[test]
fn auto_detect_and_withdraw_port_from_exec() {
    if !is_docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let ws = tmp.path().join(format!("ws-{}", unique()));
    std::fs::create_dir_all(&ws).unwrap();
    let udf = tmp.path().join("udf");
    std::fs::create_dir_all(&udf).unwrap();
    write_config(&ws, &bare_config());

    let up = run_up(&ws, &udf, &["--auto-forward"]);
    assert!(
        up.success,
        "up --auto-forward failed.\n--- stdout ---\n{}\n--- stderr ---\n{}",
        up.stdout, up.stderr
    );

    // Start a one-shot loopback server on :4000 AFTER up, via a plain `exec`
    // (no auto-forward flag on exec — proves transparency, FR-018). `nc -l`
    // serves a single connection then exits, so consuming it later also
    // exercises withdrawal.
    let cid = up
        .container_id
        .as_deref()
        .expect("up should report a container id");
    let started = run_exec(
        cid,
        &[
            "sh",
            "-c",
            "(echo deacon-forward-ok | nc -l -p 4000) >/dev/null 2>&1 & sleep 1",
        ],
    );
    assert!(started, "exec to start in-container server failed");

    let reachable = (|| {
        let host_port = wait_for_host_port(&udf, 4000, Duration::from_secs(15))?;
        fetch(host_port, Duration::from_secs(15))
    })();

    // After the single connection, `nc -l` exited → port 4000 stops listening →
    // the daemon should withdraw the forward and release the host port.
    let withdrawn = wait_for_withdrawal(&udf, 4000, Duration::from_secs(15));

    teardown(&ws, &udf, up.container_id.as_deref());

    let body = reachable.expect("exec-started port should be auto-forwarded");
    assert!(
        body.contains(SERVER_BANNER),
        "expected banner in relayed response, got: {body:?}"
    );
    assert!(
        withdrawn,
        "forward for container port 4000 should be withdrawn after the server stops"
    );
}

/// US3 (T029): two devcontainers both serving container port 3000 get distinct,
/// collision-free host ports from the shared host-global registry; both are
/// reachable and registered. Removing one releases its host port (the daemon
/// self-exits when its container vanishes) while the other keeps working.
#[test]
fn multi_container_collision_free_and_release() {
    if !is_docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let tmp = TempDir::new().unwrap();
    // One shared host-global registry for both forwarders.
    let udf = tmp.path().join("udf");
    std::fs::create_dir_all(&udf).unwrap();

    let ws_a = tmp.path().join(format!("ws-a-{}", unique()));
    let ws_b = tmp.path().join(format!("ws-b-{}", unique()));
    std::fs::create_dir_all(&ws_a).unwrap();
    std::fs::create_dir_all(&ws_b).unwrap();
    write_config(&ws_a, &server_config(3000, true));
    write_config(&ws_b, &server_config(3000, true));

    let up_a = run_up(&ws_a, &udf, &["--auto-forward"]);
    let up_b = run_up(&ws_b, &udf, &["--auto-forward"]);
    assert!(up_a.success && up_b.success, "both ups should succeed");
    let cid_a = up_a.container_id.clone();
    let cid_b = up_b.container_id.clone();

    let outcome = (|| {
        let id_a = cid_a.as_deref()?;
        let id_b = cid_b.as_deref()?;
        // Wait for both forwarders to register.
        let mut hp_a = None;
        let mut hp_b = None;
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(15) {
            hp_a = registry_host_port_by_container(&udf, id_a);
            hp_b = registry_host_port_by_container(&udf, id_b);
            if hp_a.is_some() && hp_b.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        let hp_a = hp_a?;
        let hp_b = hp_b?;
        let body_a = fetch(hp_a, Duration::from_secs(15))?;
        let body_b = fetch(hp_b, Duration::from_secs(15))?;
        Some((hp_a, hp_b, body_a, body_b))
    })();

    // Tear down A by removing its container; its forwarder self-exits and
    // releases the host port. B keeps working.
    let release_ok = (|| {
        let id_a = cid_a.as_deref()?;
        let id_b = cid_b.as_deref()?;
        let _ = StdCommand::new("docker")
            .args(["rm", "-f", id_a])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        // Poll until A's entry is gone.
        let start = Instant::now();
        let mut a_released = false;
        while start.elapsed() < Duration::from_secs(20) {
            if registry_host_port_by_container(&udf, id_a).is_none() {
                a_released = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        let b_still = registry_host_port_by_container(&udf, id_b);
        Some((a_released, b_still))
    })();

    teardown(&ws_a, &udf, cid_a.as_deref());
    teardown(&ws_b, &udf, cid_b.as_deref());

    let (hp_a, hp_b, body_a, body_b) =
        outcome.expect("both containers should be forwarded and reachable");
    assert_ne!(hp_a, hp_b, "two containers must get distinct host ports");
    assert!(
        body_a.contains(SERVER_BANNER),
        "container A should be reachable"
    );
    assert!(
        body_b.contains(SERVER_BANNER),
        "container B should be reachable"
    );

    let (a_released, b_still) = release_ok.expect("release outcome");
    assert!(
        a_released,
        "removing container A should release its host port"
    );
    assert!(
        b_still.is_some(),
        "container B's forward should remain after A is torn down"
    );
}

/// US4 (T035a): `down` reaps the forwarder — its pid dies, the marker is
/// removed, and its registry entries are released.
#[test]
fn down_reaps_forwarder_and_releases_ports() {
    if !is_docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let ws = tmp.path().join(format!("ws-{}", unique()));
    std::fs::create_dir_all(&ws).unwrap();
    let udf = tmp.path().join("udf");
    std::fs::create_dir_all(&udf).unwrap();
    write_config(&ws, &server_config(3000, true));

    let up = run_up(&ws, &udf, &["--auto-forward"]);
    assert!(up.success, "up failed: {}", up.stderr);
    let cid = up.container_id.clone().expect("container id");

    // Forwarder should be live with a marker + registry entry.
    assert!(
        wait_for_host_port(&udf, 3000, Duration::from_secs(15)).is_some(),
        "forward should be registered"
    );
    let pid = read_marker_pid(&udf, &cid).expect("marker pid");
    assert!(
        host_pid_alive(pid),
        "forwarder pid should be alive after up"
    );

    // `down --remove` reaps it.
    let down_ok = run_down(&ws, &udf);
    assert!(down_ok, "down should succeed");

    // Within a short window: pid dead, marker gone, registry entry released.
    let start = Instant::now();
    let mut reaped = false;
    while start.elapsed() < Duration::from_secs(10) {
        if !host_pid_alive(pid)
            && !marker_exists(&udf, &cid)
            && registry_host_port_by_container(&udf, &cid).is_none()
        {
            reaped = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    teardown(&ws, &udf, Some(&cid));
    assert!(
        reaped,
        "down should reap the forwarder (pid dead, marker removed, ports released)"
    );
}

/// US4 (T035b): `up --remove-existing-container` reaps the old forwarder before
/// creating the replacement; the old pid dies and a fresh forwarder owns the
/// new container.
#[test]
fn replace_reaps_old_forwarder() {
    if !is_docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let ws = tmp.path().join(format!("ws-{}", unique()));
    std::fs::create_dir_all(&ws).unwrap();
    let udf = tmp.path().join("udf");
    std::fs::create_dir_all(&udf).unwrap();
    write_config(&ws, &server_config(3000, true));

    let up1 = run_up(&ws, &udf, &["--auto-forward"]);
    assert!(up1.success, "first up failed: {}", up1.stderr);
    let cid1 = up1.container_id.clone().expect("container id 1");
    assert!(wait_for_host_port(&udf, 3000, Duration::from_secs(15)).is_some());
    let pid1 = read_marker_pid(&udf, &cid1).expect("marker pid 1");

    // Replace the container; the old forwarder must be reaped first.
    let up2 = run_up(
        &ws,
        &udf,
        &["--auto-forward", "--remove-existing-container"],
    );
    assert!(up2.success, "replace up failed: {}", up2.stderr);
    let cid2 = up2.container_id.clone().expect("container id 2");
    assert_ne!(cid1, cid2, "replacement should be a new container");

    // New forwarder is live for the new container.
    let new_ok = wait_for_host_port(&udf, 3000, Duration::from_secs(15)).is_some();

    // Old forwarder pid should be dead and its marker gone.
    let start = Instant::now();
    let mut old_reaped = false;
    while start.elapsed() < Duration::from_secs(10) {
        if !host_pid_alive(pid1) && !marker_exists(&udf, &cid1) {
            old_reaped = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    teardown(&ws, &udf, Some(&cid2));
    let _ = StdCommand::new("docker")
        .args(["rm", "-f", &cid1])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    assert!(new_ok, "replacement forwarder should be registered");
    assert!(
        old_reaped,
        "old forwarder should be reaped on --remove-existing-container"
    );
}

/// US5 (T041): `onAutoForward` is honored — `ignore` ports are never forwarded,
/// `silent` ports are forwarded with no human mapping line, `notify` ports are
/// forwarded with a mapping line (in the per-container forwarder log).
#[test]
fn ports_attributes_on_auto_forward_honored() {
    if !is_docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let ws = tmp.path().join(format!("ws-{}", unique()));
    std::fs::create_dir_all(&ws).unwrap();
    let udf = tmp.path().join("udf");
    std::fs::create_dir_all(&udf).unwrap();
    let cfg = r#"{
  "name": "auto-forward-attrs",
  "image": "alpine:3.18",
  "overrideCommand": true,
  "forwardPorts": [3000, 3001, 3002],
  "portsAttributes": {
    "3000": { "onAutoForward": "notify" },
    "3001": { "onAutoForward": "silent" },
    "3002": { "onAutoForward": "ignore" }
  },
  "postStartCommand": "sh -c '(while true; do echo deacon-forward-ok | nc -l -p 3000; done) >/dev/null 2>&1 & (while true; do echo deacon-forward-ok | nc -l -p 3001; done) >/dev/null 2>&1 & sleep 1'"
}"#;
    write_config(&ws, cfg);

    let up = run_up(&ws, &udf, &["--auto-forward"]);
    assert!(up.success, "up failed: {}", up.stderr);
    let cid = up.container_id.clone().expect("container id");

    let outcome = (|| {
        let hp_notify = wait_for_host_port(&udf, 3000, Duration::from_secs(15))?;
        let hp_silent = wait_for_host_port(&udf, 3001, Duration::from_secs(15))?;
        let body_notify = fetch(hp_notify, Duration::from_secs(15))?;
        let body_silent = fetch(hp_silent, Duration::from_secs(15))?;
        // Give the eager declared loop a moment; the ignored port must never
        // appear in the registry.
        std::thread::sleep(Duration::from_secs(2));
        let ignored_present = registry_host_port(&udf, 3002).is_some();
        let log = std::fs::read_to_string(udf.join(format!("forward_daemon_{cid}.log")))
            .unwrap_or_default();
        Some((body_notify, body_silent, ignored_present, log))
    })();

    teardown(&ws, &udf, Some(&cid));

    let (body_notify, body_silent, ignored_present, log) =
        outcome.expect("notify+silent ports should be reachable");
    assert!(body_notify.contains(SERVER_BANNER), "notify port reachable");
    assert!(body_silent.contains(SERVER_BANNER), "silent port reachable");
    assert!(!ignored_present, "ignored port must not be forwarded");
    assert!(
        log.contains("Forwarding container 3000"),
        "notify port should emit a mapping line; log:\n{log}"
    );
    assert!(
        !log.contains("Forwarding container 3001"),
        "silent port must NOT emit a mapping line; log:\n{log}"
    );
}

/// US5 (T042): a compose `"service:port"` declared port on a non-primary
/// service is reachable on the host — the relay dials the named service over
/// the compose network from the primary container (FR-023).
#[test]
fn compose_service_port_is_forwarded() {
    if !is_docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let ws = tmp.path().join(format!("ws-{}", unique()));
    std::fs::create_dir_all(ws.join(".devcontainer")).unwrap();
    let udf = tmp.path().join("udf");
    std::fs::create_dir_all(&udf).unwrap();

    // docker-compose.yml resolves against the workspace folder (per CLAUDE.md).
    std::fs::write(
        ws.join("docker-compose.yml"),
        r#"services:
  app:
    image: alpine:3.18
    command: ["sleep", "infinity"]
  db:
    image: alpine:3.18
    command: ["sh", "-c", "while true; do echo deacon-forward-ok | nc -l -p 5432; done"]
"#,
    )
    .unwrap();
    std::fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        r#"{
  "name": "af-compose",
  "dockerComposeFile": "../docker-compose.yml",
  "service": "app",
  "workspaceFolder": "/workspace",
  "forwardPorts": ["db:5432"]
}"#,
    )
    .unwrap();

    let up = run_up(&ws, &udf, &["--auto-forward"]);
    assert!(
        up.success,
        "compose up --auto-forward failed.\n--- stdout ---\n{}\n--- stderr ---\n{}",
        up.stdout, up.stderr
    );
    let cid = up.container_id.clone();

    let result = (|| {
        let host_port = wait_for_host_port(&udf, 5432, Duration::from_secs(20))?;
        fetch(host_port, Duration::from_secs(20))
    })();

    // Reap the forwarder, then tear the compose project down.
    if let Some(id) = cid.as_deref() {
        if let Some(pid) = read_marker_pid(&udf, id) {
            kill_pid(pid);
        }
    }
    let _ = run_down(&ws, &udf);

    let body = result.expect("compose service:port should be reachable on the host");
    assert!(
        body.contains(SERVER_BANNER),
        "expected banner relayed from the db service, got: {body:?}"
    );
}

// ---------------------------------------------------------------------------
// Browser auto-open (onAutoForward: openBrowser) — hermetic via a fake browser.
// ---------------------------------------------------------------------------

/// devcontainer.json declaring `port` with a given `onAutoForward` action, plus
/// the loopback `nc` banner server (so the port actually forwards).
fn server_config_on_auto_forward(port: u16, action: &str) -> String {
    format!(
        r#"{{
  "name": "auto-forward-browser-test",
  "image": "alpine:3.18",
  "overrideCommand": true,
  "forwardPorts": [{port}],
  "portsAttributes": {{ "{port}": {{ "onAutoForward": "{action}" }} }},
  "postStartCommand": "sh -c '(while true; do echo {SERVER_BANNER} | nc -l -p {port}; done) >/dev/null 2>&1 & sleep 1'"
}}"#
    )
}

/// Write a fake "browser": a host shell script that appends its first arg (the
/// URL deacon passes) to a marker file. Returns `(script_path, marker_path)`.
fn write_fake_browser(dir: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let marker = dir.join("opened-urls.txt");
    let script = dir.join("fake-browser.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$1\" >> '{}'\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    (script, marker)
}

/// `up --auto-forward` with `DEACON_BROWSER` set to the fake browser script.
fn run_up_with_browser(ws: &Path, udf: &Path, browser: &Path) -> UpOutcome {
    let bin = env!("CARGO_BIN_EXE_deacon");
    let out = StdCommand::new(bin)
        .env("DEACON_BROWSER", browser)
        .arg("--user-data-folder")
        .arg(udf)
        .arg("up")
        .arg("--workspace-folder")
        .arg(ws)
        .arg("--auto-forward")
        .output()
        .expect("run deacon up");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let container_id = serde_json::from_str::<Value>(&stdout).ok().and_then(|v| {
        v.get("containerId")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    UpOutcome {
        success: out.status.success(),
        container_id,
        stdout,
        stderr,
    }
}

fn marker_contains(marker: &Path, needle: &str, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(s) = std::fs::read_to_string(marker) {
            if s.contains(needle) {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

/// `onAutoForward: openBrowser` launches the configured browser at the forwarded
/// loopback URL. The "browser" is a recording script (hermetic; no real browser
/// or display needed). `DEACON_BROWSER` being set also force-enables auto-open
/// despite the test having no TTY.
#[test]
fn auto_forward_opens_browser_for_openbrowser() {
    if !is_docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let ws = tmp.path().join(format!("ws-{}", unique()));
    std::fs::create_dir_all(&ws).unwrap();
    let udf = tmp.path().join("udf");
    std::fs::create_dir_all(&udf).unwrap();
    write_config(&ws, &server_config_on_auto_forward(3000, "openBrowser"));
    let (script, marker) = write_fake_browser(&udf);

    let up = run_up_with_browser(&ws, &udf, &script);
    let cid = up.container_id.clone();
    let result = (|| {
        assert!(
            up.success,
            "up --auto-forward failed.\n--- stdout ---\n{}\n--- stderr ---\n{}",
            up.stdout, up.stderr
        );
        let host_port = wait_for_host_port(&udf, 3000, Duration::from_secs(15))?;
        let url = format!("http://127.0.0.1:{host_port}");
        marker_contains(&marker, &url, Duration::from_secs(15)).then_some(url)
    })();

    teardown(&ws, &udf, cid.as_deref());

    let url =
        result.expect("openBrowser should have launched the fake browser at the loopback URL");
    assert!(url.starts_with("http://127.0.0.1:"));
}

/// `onAutoForward: notify` must NOT open a browser even though one is configured.
#[test]
fn auto_forward_notify_does_not_open_browser() {
    if !is_docker_available() {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let ws = tmp.path().join(format!("ws-{}", unique()));
    std::fs::create_dir_all(&ws).unwrap();
    let udf = tmp.path().join("udf");
    std::fs::create_dir_all(&udf).unwrap();
    write_config(&ws, &server_config_on_auto_forward(3000, "notify"));
    let (script, marker) = write_fake_browser(&udf);

    let up = run_up_with_browser(&ws, &udf, &script);
    let cid = up.container_id.clone();
    let opened = (|| {
        if !up.success {
            return false;
        }
        // Wait for the port to actually forward, then give any (erroneous) open
        // a chance to land before asserting the marker stayed empty.
        if wait_for_host_port(&udf, 3000, Duration::from_secs(15)).is_none() {
            return false;
        }
        marker_contains(&marker, "http://127.0.0.1:", Duration::from_secs(3))
    })();

    teardown(&ws, &udf, cid.as_deref());

    assert!(!opened, "notify must not open a browser");
}
