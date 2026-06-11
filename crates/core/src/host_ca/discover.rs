//! Host trust-store enumeration and corporate-CA discovery.
//!
//! Two responsibilities:
//! 1. [`enumerate_host_roots`] — read the OS trust store via
//!    `rustls-native-certs` (shared with `oci::client`, which unions these into
//!    deacon's own HTTPS trust set — US1).
//! 2. [`discover_corporate_set`] — compute the *corporate* delta: host
//!    `CA:TRUE` roots whose key is **not** one of Mozilla's bundled public
//!    roots (`webpki-roots`), identified by SHA-256 over the
//!    SubjectPublicKeyInfo (SPKI). See [research Decision 2].
//!
//! The public-set match is by **SPKI content** (the bytes inside the SPKI
//! `SEQUENCE`, excluding the outer tag/length). `webpki-roots` exposes anchors
//! whose `subject_public_key_info` is exactly that content (rustls-webpki's
//! `anchor_from_trusted_cert` stores `cert.spki` sans the wrapper); for host
//! certs we strip the outer `SEQUENCE` header off `x509-parser`'s full SPKI
//! `raw`. Both sides therefore hash the identical key bytes.

use crate::errors::{DeaconError, InternalError, Result};
use crate::host_ca::activation::HostCaActivation;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use tracing::{debug, info, warn};
use x509_parser::prelude::*;

/// A single parsed host-store certificate.
#[derive(Debug, Clone)]
pub struct HostCertificate {
    /// Raw DER bytes (the unit streamed into the container / written to PEM).
    pub der: Vec<u8>,
    /// Subject distinguished name (for info logging — FR-007).
    pub subject: String,
    /// SHA-256 of the SPKI **content** — the identity key for public-set
    /// subtraction and deterministic ordering.
    pub spki_sha256: [u8; 32],
    /// `basicConstraints CA:TRUE`.
    pub is_ca: bool,
}

/// The computed corporate delta and its serialized bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CorporateCaSet {
    /// Subject DNs of the injected certs (for logs, JSON result, labels).
    pub subjects: Vec<String>,
    /// PEM concatenation of the corporate certs in deterministic (SPKI-sorted)
    /// order — the bytes streamed at runtime / mounted at build.
    pub pem_bundle: String,
    /// Number of certs in the set (== `subjects.len()`).
    pub count: usize,
}

impl CorporateCaSet {
    /// True when discovery found no corporate certs (the "proceed without
    /// injection, not an error" path — FR-008).
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// Enumerate the host OS trust store roots as raw DER.
///
/// Surfaces enumeration failure as an actionable error (FR-009). An empty store
/// is returned as `Ok(vec![])` (callers decide whether that's fatal); a hard
/// platform error is propagated.
pub fn enumerate_host_roots() -> Result<Vec<Vec<u8>>> {
    let result = rustls_native_certs::load_native_certs();
    if !result.errors.is_empty() {
        // Partial reads can still be useful, but if we got *nothing* and saw
        // errors, that's an actionable failure rather than "no corporate CA".
        let joined = result
            .errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        if result.certs.is_empty() {
            return Err(DeaconError::Internal(InternalError::Generic {
                message: format!(
                    "Failed to enumerate host trust store: {}. \
                     Check OS certificate store access, or pass an explicit \
                     PEM bundle to --inject-host-ca.",
                    joined
                ),
            }));
        }
        warn!(
            "Host trust store enumeration reported errors (continuing with {} certs): {}",
            result.certs.len(),
            joined
        );
    }
    let ders = result
        .certs
        .into_iter()
        .map(|c| c.as_ref().to_vec())
        .collect::<Vec<_>>();
    debug!(count = ders.len(), "Enumerated host trust-store roots");
    Ok(ders)
}

/// Strip the outer `SEQUENCE` tag + length from a DER-encoded structure,
/// returning the content bytes. Returns `None` if the input is not a
/// definite-length `SEQUENCE`.
fn der_sequence_content(raw: &[u8]) -> Option<&[u8]> {
    // Tag: 0x30 = constructed SEQUENCE.
    if raw.first() != Some(&0x30) {
        return None;
    }
    let len_octet = *raw.get(1)?;
    let content_start = if len_octet & 0x80 == 0 {
        // Short form: low 7 bits are the length; content follows immediately.
        2
    } else {
        // Long form: low 7 bits are the count of subsequent length octets.
        let num = (len_octet & 0x7f) as usize;
        if num == 0 {
            // Indefinite length is not valid DER.
            return None;
        }
        2 + num
    };
    raw.get(content_start..)
}

/// SHA-256 over the SPKI **content** (matching the webpki anchor encoding).
fn spki_content_hash(spki_raw: &[u8]) -> [u8; 32] {
    let content = der_sequence_content(spki_raw).unwrap_or(spki_raw);
    let mut hasher = Sha256::new();
    hasher.update(content);
    hasher.finalize().into()
}

/// Parse one DER cert into a [`HostCertificate`]. Returns `None` (with a debug
/// log) for certs `x509-parser` cannot parse, so a single odd entry in the host
/// store never aborts discovery.
pub fn parse_host_certificate(der: &[u8]) -> Option<HostCertificate> {
    match parse_x509_certificate(der) {
        Ok((_, cert)) => {
            let subject = cert.subject().to_string();
            let spki_sha256 = spki_content_hash(cert.public_key().raw);
            let is_ca = cert.is_ca();
            Some(HostCertificate {
                der: der.to_vec(),
                subject,
                spki_sha256,
                is_ca,
            })
        }
        Err(e) => {
            debug!("Skipping unparseable host certificate: {}", e);
            None
        }
    }
}

/// The set of SPKI-content SHA-256 hashes for Mozilla's bundled public roots.
fn public_spki_hashes() -> HashSet<[u8; 32]> {
    webpki_roots::TLS_SERVER_ROOTS
        .iter()
        .map(|anchor| {
            let mut hasher = Sha256::new();
            hasher.update(anchor.subject_public_key_info.as_ref());
            hasher.finalize().into()
        })
        .collect()
}

/// Render a DER cert as a PEM `CERTIFICATE` block (64-char base64 lines).
fn der_to_pem(der: &[u8]) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    let mut out = String::with_capacity(b64.len() + 64);
    out.push_str("-----BEGIN CERTIFICATE-----\n");
    for chunk in b64.as_bytes().chunks(64) {
        out.push_str(std::str::from_utf8(chunk).unwrap());
        out.push('\n');
    }
    out.push_str("-----END CERTIFICATE-----\n");
    out
}

/// Compute the corporate set from parsed host certs and the public SPKI hash
/// set: keep only `CA:TRUE` certs whose SPKI is not public, dedup by SPKI, sort
/// deterministically by SPKI hash, and build the PEM bundle. Pure + testable.
fn compute_corporate_set(
    host_certs: Vec<HostCertificate>,
    public_hashes: &HashSet<[u8; 32]>,
) -> CorporateCaSet {
    let mut corporate: Vec<HostCertificate> = host_certs
        .into_iter()
        .filter(|c| c.is_ca && !public_hashes.contains(&c.spki_sha256))
        .collect();

    // Deterministic order (FR-017) + dedup by SPKI (a key may appear twice in
    // the host store, e.g. system + user store).
    corporate.sort_by_key(|c| c.spki_sha256);
    corporate.dedup_by(|a, b| a.spki_sha256 == b.spki_sha256);

    let subjects: Vec<String> = corporate.iter().map(|c| c.subject.clone()).collect();
    let mut pem_bundle = String::new();
    for c in &corporate {
        pem_bundle.push_str(&der_to_pem(&c.der));
    }
    let count = corporate.len();
    CorporateCaSet {
        subjects,
        pem_bundle,
        count,
    }
}

/// Discover the corporate CA set per the resolved activation.
///
/// - [`HostCaActivation::Auto`]: enumerate the host store, keep `CA:TRUE` roots
///   minus the public set.
/// - [`HostCaActivation::ExplicitPath`]: read + validate the PEM verbatim.
/// - [`HostCaActivation::Off`]: returns an empty set (callers gate on activation
///   before calling, so this is just defensive).
///
/// An empty corporate set is **not** an error (FR-008): it logs and returns an
/// empty [`CorporateCaSet`].
pub fn discover_corporate_set(activation: &HostCaActivation) -> Result<CorporateCaSet> {
    match activation {
        HostCaActivation::Off => Ok(CorporateCaSet::default()),
        HostCaActivation::Auto => {
            let ders = enumerate_host_roots()?;
            let host_total = ders.len();
            let host_certs: Vec<HostCertificate> = ders
                .iter()
                .filter_map(|d| parse_host_certificate(d))
                .collect();
            let public = public_spki_hashes();
            let set = compute_corporate_set(host_certs, &public);
            info!(
                host_total,
                corporate_count = set.count,
                mode = "auto",
                "Discovered corporate CA set"
            );
            for subject in &set.subjects {
                info!(subject = %subject, "corporate CA");
            }
            if set.is_empty() {
                info!("0 corporate certs discovered; proceeding without injection");
            }
            Ok(set)
        }
        HostCaActivation::ExplicitPath(path) => validate_explicit_bundle(path),
    }
}

/// Read + validate an explicit PEM bundle, failing fast with the path + reason
/// when it is unreadable or does not parse as PEM certificates (FR-005, SC-008).
pub fn validate_explicit_bundle(path: &std::path::Path) -> Result<CorporateCaSet> {
    let bytes = std::fs::read(path).map_err(|e| {
        DeaconError::Internal(InternalError::Generic {
            message: format!("Failed to read host-CA bundle at {}: {}", path.display(), e),
        })
    })?;

    // Parse every PEM CERTIFICATE block; a file with zero parseable certs (or a
    // DER/garbage file) is a fail-fast error naming the path.
    let mut host_certs: Vec<HostCertificate> = Vec::new();
    for pem in Pem::iter_from_buffer(&bytes) {
        let pem = pem.map_err(|e| {
            DeaconError::Internal(InternalError::Generic {
                message: format!(
                    "Host-CA bundle at {} is not valid PEM: {}",
                    path.display(),
                    e
                ),
            })
        })?;
        if pem.label != "CERTIFICATE" {
            continue;
        }
        if let Some(cert) = parse_host_certificate(&pem.contents) {
            host_certs.push(cert);
        } else {
            return Err(DeaconError::Internal(InternalError::Generic {
                message: format!(
                    "Host-CA bundle at {} contains an unparseable certificate",
                    path.display()
                ),
            }));
        }
    }

    if host_certs.is_empty() {
        return Err(DeaconError::Internal(InternalError::Generic {
            message: format!(
                "Host-CA bundle at {} contained no PEM certificates",
                path.display()
            ),
        }));
    }

    // Explicit bundles are taken verbatim (no public-set subtraction): the
    // machine owner chose exactly these certs. Still sort/dedup for determinism.
    let set = compute_corporate_set(host_certs, &HashSet::new());
    info!(
        corporate_count = set.count,
        mode = "explicit",
        path = %path.display(),
        "Loaded explicit host-CA bundle"
    );
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORPORATE_CA_PEM: &str = include_str!("test_fixtures/corporate_ca.pem");
    const LEAF_NOT_CA_PEM: &str = include_str!("test_fixtures/leaf_not_ca.pem");
    const PUBLIC_ROOT_PEM: &str = include_str!("test_fixtures/public_root.pem");

    fn parse_fixture(pem: &str) -> HostCertificate {
        let p = Pem::iter_from_buffer(pem.as_bytes())
            .next()
            .unwrap()
            .unwrap();
        parse_host_certificate(&p.contents).unwrap()
    }

    #[test]
    fn der_sequence_content_short_form() {
        // SEQUENCE, length 3, content [0x01,0x02,0x03]
        let raw = [0x30, 0x03, 0x01, 0x02, 0x03];
        assert_eq!(der_sequence_content(&raw), Some(&[0x01, 0x02, 0x03][..]));
    }

    #[test]
    fn der_sequence_content_long_form() {
        // SEQUENCE, long-form length (0x81 = 1 length octet), len 2, content [0xAA,0xBB]
        let raw = [0x30, 0x81, 0x02, 0xAA, 0xBB];
        assert_eq!(der_sequence_content(&raw), Some(&[0xAA, 0xBB][..]));
    }

    #[test]
    fn der_sequence_content_rejects_non_sequence() {
        assert_eq!(der_sequence_content(&[0x02, 0x01, 0x05]), None);
    }

    #[test]
    fn strip_then_rewrap_round_trips_on_real_spki() {
        // Self-validating: the stripped content, re-wrapped with a freshly
        // computed DER header, must reproduce the original full SPKI raw.
        let p = Pem::iter_from_buffer(CORPORATE_CA_PEM.as_bytes())
            .next()
            .unwrap()
            .unwrap();
        let (_, cert) = parse_x509_certificate(&p.contents).unwrap();
        let raw = cert.public_key().raw;
        let content = der_sequence_content(raw).unwrap();
        // Re-encode the DER length (definite form) and prepend the SEQUENCE tag.
        let mut rewrapped = vec![0x30u8];
        let len = content.len();
        if len < 0x80 {
            rewrapped.push(len as u8);
        } else {
            let mut len_bytes = Vec::new();
            let mut v = len;
            while v > 0 {
                len_bytes.insert(0, (v & 0xff) as u8);
                v >>= 8;
            }
            rewrapped.push(0x80 | len_bytes.len() as u8);
            rewrapped.extend_from_slice(&len_bytes);
        }
        rewrapped.extend_from_slice(content);
        assert_eq!(rewrapped, raw, "strip+rewrap must reproduce the SPKI raw");
    }

    #[test]
    fn corporate_ca_is_ca_leaf_is_not() {
        assert!(parse_fixture(CORPORATE_CA_PEM).is_ca);
        assert!(!parse_fixture(LEAF_NOT_CA_PEM).is_ca);
    }

    #[test]
    fn discovery_diff_excludes_public_keeps_corporate_drops_leaf() {
        let corporate = parse_fixture(CORPORATE_CA_PEM);
        let leaf = parse_fixture(LEAF_NOT_CA_PEM);
        let public = parse_fixture(PUBLIC_ROOT_PEM);

        // Simulate the public set containing `public`'s SPKI.
        let mut public_hashes = HashSet::new();
        public_hashes.insert(public.spki_sha256);

        let set = compute_corporate_set(
            vec![corporate.clone(), leaf.clone(), public.clone()],
            &public_hashes,
        );
        // Only the corporate CA survives: public excluded by SPKI, leaf excluded
        // by CA:FALSE.
        assert_eq!(set.count, 1);
        assert_eq!(set.subjects.len(), 1);
        assert!(set.subjects[0].contains("ACME Corp Root CA"));
        assert!(set.pem_bundle.contains("BEGIN CERTIFICATE"));
    }

    #[test]
    fn all_public_yields_empty_set() {
        // FR-008: zero corporate certs → empty set, not an error.
        let corporate = parse_fixture(CORPORATE_CA_PEM);
        let public = parse_fixture(PUBLIC_ROOT_PEM);
        let mut public_hashes = HashSet::new();
        public_hashes.insert(corporate.spki_sha256);
        public_hashes.insert(public.spki_sha256);
        let set = compute_corporate_set(vec![corporate, public], &public_hashes);
        assert!(set.is_empty());
        assert_eq!(set.count, 0);
    }

    #[test]
    fn dedup_identical_spki() {
        let a = parse_fixture(CORPORATE_CA_PEM);
        let b = parse_fixture(CORPORATE_CA_PEM);
        let set = compute_corporate_set(vec![a, b], &HashSet::new());
        assert_eq!(set.count, 1);
    }

    #[test]
    fn explicit_bundle_valid_pem() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("bundle.pem");
        std::fs::write(&path, CORPORATE_CA_PEM).unwrap();
        let set = validate_explicit_bundle(&path).unwrap();
        assert_eq!(set.count, 1);
        assert!(set.subjects[0].contains("ACME Corp Root CA"));
    }

    #[test]
    fn explicit_bundle_unreadable_fails_fast() {
        let path = std::path::Path::new("/nonexistent/path/to/bundle.pem");
        let err = validate_explicit_bundle(path).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("/nonexistent/path/to/bundle.pem"));
        assert!(msg.to_lowercase().contains("read"));
    }

    #[test]
    fn explicit_bundle_non_pem_fails_fast() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("garbage.pem");
        std::fs::write(&path, b"this is not a certificate at all").unwrap();
        let err = validate_explicit_bundle(&path).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains(path.to_str().unwrap()));
    }
}
