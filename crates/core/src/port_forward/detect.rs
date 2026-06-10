//! Pure parsing of `/proc/net/tcp{,6}` LISTEN rows into [`DetectedPort`]s.
//!
//! The forwarder polls the container's listening sockets by `cat`-ing
//! `/proc/net/tcp` and `/proc/net/tcp6` over `docker exec` and feeding the
//! concatenated text to [`parse_proc_net_tcp`]. This module is pure (no IO)
//! and fully unit-testable with fixtures.
//!
//! Format reference: Linux kernel `Documentation/networking/proc_net_tcp.txt`.
//! Each data row's whitespace-separated fields are `sl local_address
//! rem_address st …`. `local_address` is `HEXIP:HEXPORT`; `st == 0A` is
//! `TCP_LISTEN`. The IP is little-endian hex; the port is plain hex.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::{BindScope, DetectedPort, IpFamily};

/// `TCP_LISTEN` state value in `/proc/net/tcp{,6}`.
const ST_LISTEN: &str = "0A";

/// Parse concatenated `/proc/net/tcp` + `/proc/net/tcp6` text into the set of
/// distinct listening ports.
///
/// Only `st == 0A` (LISTEN) rows bound to loopback or any-interface are kept.
/// A port listening on both IPv4 and IPv6 collapses to a single logical entry
/// (FR-003); the first occurrence wins, but an any-interface bind upgrades a
/// previously-seen loopback bind so the broader scope is reported.
pub fn parse_proc_net_tcp(text: &str) -> Vec<DetectedPort> {
    let mut out: Vec<DetectedPort> = Vec::new();

    for line in text.lines() {
        let Some(detected) = parse_row(line) else {
            continue;
        };
        if let Some(existing) = out.iter_mut().find(|d| d.port == detected.port) {
            // Dedup by logical port; widen scope to AnyInterface if either bind
            // is any-interface (informational).
            if detected.bind_addr == BindScope::AnyInterface {
                existing.bind_addr = BindScope::AnyInterface;
            }
        } else {
            out.push(detected);
        }
    }

    out
}

/// Parse a single data row. Returns `None` for headers, malformed rows,
/// non-LISTEN sockets, or binds outside loopback/any-interface.
fn parse_row(line: &str) -> Option<DetectedPort> {
    let mut fields = line.split_whitespace();
    // fields[0] = "sl:" (slot, with trailing colon) — header row has "sl".
    let _slot = fields.next()?;
    let local_address = fields.next()?;
    let _rem_address = fields.next()?;
    let st = fields.next()?;

    if st != ST_LISTEN {
        return None;
    }

    let (ip_hex, port_hex) = local_address.split_once(':')?;
    let port = u16::from_str_radix(port_hex, 16).ok()?;
    if port == 0 {
        return None;
    }

    let (ip, family) = match ip_hex.len() {
        8 => (IpAddr::V4(parse_v4(ip_hex)?), IpFamily::V4),
        32 => (IpAddr::V6(parse_v6(ip_hex)?), IpFamily::V6),
        _ => return None,
    };

    let bind_addr = if ip.is_loopback() {
        BindScope::Loopback
    } else if ip.is_unspecified() {
        BindScope::AnyInterface
    } else {
        // Bound to a specific external interface — not forwarded in v1.
        return None;
    };

    Some(DetectedPort {
        port,
        bind_addr,
        family,
    })
}

/// Decode an 8-hex-char little-endian IPv4 address (`0100007F` → `127.0.0.1`).
fn parse_v4(hex: &str) -> Option<Ipv4Addr> {
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some(Ipv4Addr::from(v.to_le_bytes()))
}

/// Decode a 32-hex-char IPv6 address stored as four per-word little-endian
/// 32-bit groups (`…01000000` → `::1`).
fn parse_v6(hex: &str) -> Option<Ipv6Addr> {
    if hex.len() != 32 {
        return None;
    }
    let mut bytes = [0u8; 16];
    for i in 0..4 {
        let word = u32::from_str_radix(&hex[i * 8..i * 8 + 8], 16).ok()?;
        bytes[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    Some(Ipv6Addr::from(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Header + one loopback LISTEN (127.0.0.1:8080) + one any LISTEN
    // (0.0.0.0:5001) + one ESTABLISHED row (st=01) that must be ignored.
    const TCP_V4: &str = "\
  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 111 1 0 100 0 0 10 0
   1: 00000000:1389 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 222 1 0 100 0 0 10 0
   2: 0100007F:C5BA 0100007F:1F90 01 00000000:00000000 00:00000000 00000000  1000        0 333 1 0 100 0 0 10 0
";

    // ::1:8080 (dedups with the v4 8080) + :::5900 (any, 0x170C = 5900).
    const TCP_V6: &str = "\
  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000001000000:1F90 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000 0 444 1 0 100 0 0 10 0
   1: 00000000000000000000000000000000:170C 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0 0 555 1 0 100 0 0 10 0
";

    #[test]
    fn parses_v4_loopback_little_endian() {
        let ports = parse_proc_net_tcp(TCP_V4);
        let p = ports.iter().find(|d| d.port == 8080).unwrap();
        assert_eq!(p.bind_addr, BindScope::Loopback);
        assert_eq!(p.family, IpFamily::V4);
    }

    #[test]
    fn parses_v4_any_interface() {
        let ports = parse_proc_net_tcp(TCP_V4);
        let p = ports.iter().find(|d| d.port == 5001).unwrap();
        assert_eq!(p.bind_addr, BindScope::AnyInterface);
    }

    #[test]
    fn ignores_non_listen_rows() {
        let ports = parse_proc_net_tcp(TCP_V4);
        // The ESTABLISHED row's local port 0xC5BA = 50618 must not appear.
        assert!(ports.iter().all(|d| d.port != 0xC5BA));
        assert_eq!(ports.len(), 2);
    }

    #[test]
    fn parses_v6_loopback_and_any() {
        let ports = parse_proc_net_tcp(TCP_V6);
        // ::1:8080
        assert!(
            ports
                .iter()
                .any(|d| d.port == 8080 && d.bind_addr == BindScope::Loopback)
        );
        // :::5900
        assert!(
            ports
                .iter()
                .any(|d| d.port == 5900 && d.bind_addr == BindScope::AnyInterface)
        );
    }

    #[test]
    fn dedups_v4_and_v6_same_port() {
        let combined = format!("{TCP_V4}{TCP_V6}");
        let ports = parse_proc_net_tcp(&combined);
        // Port 8080 listed on both v4 and v6 collapses to one entry.
        assert_eq!(ports.iter().filter(|d| d.port == 8080).count(), 1);
        // Distinct ports: 8080, 5001, 5900.
        let mut nums: Vec<u16> = ports.iter().map(|d| d.port).collect();
        nums.sort_unstable();
        assert_eq!(nums, vec![5001, 5900, 8080]);
    }

    #[test]
    fn empty_input_yields_nothing() {
        assert!(parse_proc_net_tcp("").is_empty());
        assert!(parse_proc_net_tcp("garbage line without enough fields").is_empty());
    }
}
