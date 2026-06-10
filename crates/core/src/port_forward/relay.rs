//! Per-connection byte relay over `docker exec -i`.
//!
//! For each accepted host connection on `127.0.0.1:host_port`, the relay spawns
//! `docker exec -i <id> <relay-program>` that dials a configurable dial host
//! (default `127.0.0.1`, or a compose service host) on the container port, then
//! pumps bytes bidirectionally. Because `docker exec` shares the container's
//! network namespace, this reaches `127.0.0.1`-bound servers that `-p` cannot.
//!
//! Relay-program selection probes the container for `socat`, then `nc`, then
//! bash `/dev/tcp`, failing fast with [`PortForwardError::NoRelayStrategy`] when
//! none is available (Principle IV — no silent fallback). A persistent embedded
//! multiplexing relay is a tracked throughput optimization (deferral T055).

use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;
use tracing::{debug, warn};

use super::PortForwardError;

/// A relay program available inside the container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayProgram {
    /// `socat` (preferred — robust bidirectional copy).
    Socat,
    /// `nc` / netcat.
    Nc,
    /// bash `/dev/tcp` pseudo-device (last resort; requires bash).
    DevTcp,
}

/// Shell snippet that prints the basename of each available relay program,
/// one per line, in preference order.
pub fn probe_script() -> &'static str {
    "for p in socat nc bash; do if command -v \"$p\" >/dev/null 2>&1; then echo \"$p\"; fi; done"
}

/// Pick the best relay program from [`probe_script`] output.
pub fn select_relay_from_probe(probe_stdout: &str) -> Option<RelayProgram> {
    let available: Vec<&str> = probe_stdout.split_whitespace().collect();
    if available.contains(&"socat") {
        Some(RelayProgram::Socat)
    } else if available.contains(&"nc") {
        Some(RelayProgram::Nc)
    } else if available.contains(&"bash") {
        Some(RelayProgram::DevTcp)
    } else {
        None
    }
}

/// Build the in-container command (after `docker exec -i <id>`) that dials
/// `dial_host:container_port` and relays stdin/stdout to it.
pub fn relay_args(program: RelayProgram, dial_host: &str, container_port: u16) -> Vec<String> {
    match program {
        RelayProgram::Socat => vec![
            "socat".to_string(),
            "-".to_string(),
            format!("TCP:{dial_host}:{container_port}"),
        ],
        RelayProgram::Nc => vec![
            "nc".to_string(),
            dial_host.to_string(),
            container_port.to_string(),
        ],
        RelayProgram::DevTcp => vec![
            "bash".to_string(),
            "-c".to_string(),
            format!("exec 3<>/dev/tcp/{dial_host}/{container_port}; cat <&3 & cat >&3; wait"),
        ],
    }
}

/// Map a failed relay-program probe to a clear fail-fast error.
pub fn no_relay_strategy(container_id: &str) -> PortForwardError {
    PortForwardError::NoRelayStrategy {
        container_id: container_id.to_string(),
    }
}

/// Accept connections on `listener` forever, relaying each into the container.
///
/// Runs until the task is aborted (forward withdrawn) or the listener errors.
/// Each connection is handled in its own task with `kill_on_drop` so an aborted
/// relay tears down its `docker exec` child.
pub async fn serve(
    listener: TcpListener,
    docker_path: String,
    container_id: String,
    relay_argv: Vec<String>,
) {
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                debug!(%peer, container_id = %container_id, "relay: accepted connection");
                let docker_path = docker_path.clone();
                let container_id = container_id.clone();
                let relay_argv = relay_argv.clone();
                tokio::spawn(async move {
                    if let Err(e) =
                        handle_connection(stream, &docker_path, &container_id, &relay_argv).await
                    {
                        warn!(container_id = %container_id, error = %e, "relay connection ended with error");
                    }
                });
            }
            Err(e) => {
                warn!(container_id = %container_id, error = %e, "relay: accept failed; stopping listener");
                return;
            }
        }
    }
}

/// Relay one host connection through a fresh `docker exec -i` child.
async fn handle_connection(
    stream: TcpStream,
    docker_path: &str,
    container_id: &str,
    relay_argv: &[String],
) -> std::io::Result<()> {
    let mut cmd = Command::new(docker_path);
    cmd.arg("exec")
        .arg("-i")
        .arg(container_id)
        .args(relay_argv)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    let mut child = cmd.spawn()?;
    let mut child_stdin = child
        .stdin
        .take()
        .ok_or_else(|| std::io::Error::other("relay child missing stdin"))?;
    let mut child_stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("relay child missing stdout"))?;

    let (mut host_read, mut host_write) = stream.into_split();

    // host -> container
    let upstream = async {
        let _ = tokio::io::copy(&mut host_read, &mut child_stdin).await;
        let _ = child_stdin.shutdown().await;
    };
    // container -> host
    let downstream = async {
        let _ = tokio::io::copy(&mut child_stdout, &mut host_write).await;
        let _ = host_write.shutdown().await;
    };

    tokio::join!(upstream, downstream);
    let _ = child.wait().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_socat_first() {
        assert_eq!(
            select_relay_from_probe("socat\nnc\nbash\n"),
            Some(RelayProgram::Socat)
        );
    }

    #[test]
    fn falls_back_to_nc_then_devtcp() {
        assert_eq!(
            select_relay_from_probe("nc\nbash\n"),
            Some(RelayProgram::Nc)
        );
        assert_eq!(
            select_relay_from_probe("bash\n"),
            Some(RelayProgram::DevTcp)
        );
    }

    #[test]
    fn no_relay_when_none_present() {
        assert_eq!(select_relay_from_probe(""), None);
        assert_eq!(select_relay_from_probe("\n  \n"), None);
    }

    #[test]
    fn relay_args_shapes() {
        assert_eq!(
            relay_args(RelayProgram::Socat, "127.0.0.1", 3000),
            vec!["socat", "-", "TCP:127.0.0.1:3000"]
        );
        assert_eq!(
            relay_args(RelayProgram::Nc, "db", 5432),
            vec!["nc", "db", "5432"]
        );
        let dev = relay_args(RelayProgram::DevTcp, "127.0.0.1", 8080);
        assert_eq!(dev[0], "bash");
        assert!(dev[2].contains("/dev/tcp/127.0.0.1/8080"));
    }
}
