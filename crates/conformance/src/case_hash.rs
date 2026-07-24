//! Case + fixture hashing (research D3, FR-020, 022-conformance-runner).
//!
//! [`case_hash`] is SHA-256 over the **canonicalized behavior-affecting inputs** of a
//! declarative case — `operations`, `oracleType`, `expected`, `fsAllowlist`, and the
//! referenced fixture hashes — and NOTHING else. `notes`, `allowedDifferences`, and
//! any other human prose are excluded, so annotating a case never invalidates its
//! committed snapshot (clarify Q4). [`fixture_hash`] is SHA-256 over the fixture
//! bytes. Both reuse the `sha2::{Digest, Sha256}` pattern already proven in
//! [`crate::clause`] (research D3).
//!
//! Canonicalization sorts JSON **object keys** recursively (object key order is
//! insignificant) while preserving **array order** (`operations`, `argv`, `expected`
//! are ordered and their order IS significant). This makes the hash independent of how
//! the author happened to order keys in an assertion object, so a cosmetic re-order
//! never re-records a snapshot.

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::model::TestCase;

/// The lowercase-hex SHA-256 of `bytes`, rendered in full (64 hex chars).
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::new().chain_update(bytes).finalize();
    let mut hex = String::with_capacity(64);
    for b in digest.iter() {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// Recursively canonicalize a JSON value: sort every object's keys (insignificant
/// order → deterministic) and recurse into arrays element-wise (significant order →
/// preserved). Scalars pass through unchanged.
fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = Map::with_capacity(map.len());
            for k in keys {
                out.insert(k.clone(), canonicalize(&map[k]));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize).collect()),
        other => other.clone(),
    }
}

/// The canonical byte serialization used as hash input: a canonicalized value rendered
/// with `serde_json::to_vec` (compact, no whitespace). Because [`canonicalize`] sorts
/// object keys, this is stable regardless of the `preserve_order` insertion order.
fn canonical_bytes(value: &Value) -> Vec<u8> {
    // `serde_json::to_vec` on a fully-canonicalized value cannot fail (no non-string
    // map keys, no NaN/Inf — the value came from parsed JSON), but we surface any
    // error defensively rather than unwrap (constitution V — panic-free).
    serde_json::to_vec(&canonicalize(value)).unwrap_or_default()
}

/// Compute `fixtureHash`: the full lowercase-hex SHA-256 of a fixture's bytes
/// (data-model §3, FR-020). Path-relative, byte-exact.
pub fn fixture_hash(bytes: &[u8]) -> String {
    sha256_hex(bytes)
}

/// Compute a fixture directory's hash: SHA-256 over every file's `(relative-path,
/// bytes)` in sorted relative-path order (data-model §3). Deterministic and portable —
/// a rename or content change alters the hash; iteration order does not. A missing
/// directory hashes as empty (the caller decides whether that is an error).
pub fn fixture_hash_dir(dir: &std::path::Path) -> std::io::Result<String> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    collect_files(dir, dir, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (rel, bytes) in &files {
        hasher.update(rel.as_bytes());
        hasher.update([0u8]); // separator so (`a`,`bc`) ≠ (`ab`,`c`)
        hasher.update(bytes);
        hasher.update([0u8]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest.iter() {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
}

/// The combined fixture hash over several fixture directories, in the given order — the
/// `fixtureHash` recorded in snapshot provenance (data-model §7). Each directory's
/// [`fixture_hash_dir`] is folded in with its id so a reordering or an added fixture
/// changes the result.
pub fn combined_fixture_hash(fixtures: &[(String, std::path::PathBuf)]) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    for (id, dir) in fixtures {
        let dir_hash = fixture_hash_dir(dir)?;
        hasher.update(id.as_bytes());
        hasher.update([0u8]);
        hasher.update(dir_hash.as_bytes());
        hasher.update([0u8]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest.iter() {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
}

/// Recursively collect `(relative-path, bytes)` for every file under `dir`, relative to
/// `base`. Symlinks are followed as ordinary files (fixtures are trusted, committed
/// inputs); directories recurse. Relative paths use `/` separators for portability.
fn collect_files(
    base: &std::path::Path,
    dir: &std::path::Path,
    out: &mut Vec<(String, Vec<u8>)>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files(base, &path, out)?;
        } else {
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, std::fs::read(&path)?));
        }
    }
    Ok(())
}

/// Compute `caseHash`: SHA-256 over the canonical JSON of the behavior-affecting
/// inputs `{ operations, oracleType, expected, fsAllowlist, fixtureHashes }` — and
/// nothing else (research D3, data-model §1). `fixture_hashes` is the list of
/// [`fixture_hash`] values for every fixture the case's operations reference, in a
/// caller-fixed order.
///
/// Excluded by construction: `id`, `behaviors`, `context`, `notes`,
/// `allowedDifferences`, `cleanup`, `resourceGroup`, and the legacy `executable` /
/// `outcomes` — none affect what the runner observes.
pub fn case_hash(case: &TestCase, fixture_hashes: &[String]) -> String {
    // Build the canonical input document explicitly so the field set is a deliberate,
    // reviewable allow-list rather than "whatever the record happens to carry".
    let input = serde_json::json!({
        "operations": case.operations,
        "oracleType": case.oracle_type,
        "expected": case.expected,
        "fsAllowlist": case.fs_allowlist,
        "fixtureHashes": fixture_hashes,
    });
    sha256_hex(&canonical_bytes(&input))
}

/// Compute the `(caseHash, fixtureHash)` pair for a case, rooting its fixtures under
/// `fixtures_root` (data-model §7). BOTH the reviewed refresh (`parity-harness`) and the
/// hermetic `snapshot check` (`deacon-conformance`) call this so the two never disagree
/// about a snapshot's freshness. Fixture ids are de-duplicated and sorted for a
/// deterministic result; a fixture directory that does not exist is a fail-loud IO error.
pub fn hashes_for_case(
    case: &TestCase,
    fixtures_root: &std::path::Path,
) -> std::io::Result<(String, String)> {
    let mut ids: Vec<String> = case
        .operations
        .iter()
        .flat_map(|op| op.fixtures.iter().cloned())
        .collect();
    ids.sort();
    ids.dedup();

    let mut fixtures: Vec<(String, std::path::PathBuf)> = Vec::with_capacity(ids.len());
    let mut per_fixture_hashes: Vec<String> = Vec::with_capacity(ids.len());
    for id in ids {
        let dir = fixtures_root.join(&id);
        let dir_hash = fixture_hash_dir(&dir)?;
        per_fixture_hashes.push(dir_hash);
        fixtures.push((id, dir));
    }
    let case_hash = case_hash(case, &per_fixture_hashes);
    let fixture_hash = combined_fixture_hash(&fixtures)?;
    Ok((case_hash, fixture_hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ExpectedObservable, Operation, OracleType};

    fn declarative_case() -> TestCase {
        TestCase {
            id: "case-hash-fixture".to_string(),
            behaviors: vec!["bhv-x".to_string()],
            operations: vec![Operation {
                id: "op-1".to_string(),
                subcommand: "read-configuration".to_string(),
                argv: vec!["--workspace-folder".to_string(), "${WORKSPACE}".to_string()],
                ..Operation::default()
            }],
            oracle_type: Some(OracleType::SpecExpectation),
            expected: vec![ExpectedObservable {
                channel: "chan-exit-code".to_string(),
                operation: Some("op-1".to_string()),
                assertion: Some(serde_json::json!({ "equals": 0 })),
            }],
            ..TestCase::default()
        }
    }

    #[test]
    fn fixture_hash_is_stable_hex_sha256() {
        let h = fixture_hash(b"hello");
        // Known SHA-256 of "hello".
        assert_eq!(
            h,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(h.len(), 64);
    }

    #[test]
    fn case_hash_is_deterministic() {
        let case = declarative_case();
        let a = case_hash(&case, &["deadbeef".to_string()]);
        let b = case_hash(&case, &["deadbeef".to_string()]);
        assert_eq!(a, b, "same inputs → same hash");
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn case_hash_ignores_prose_fields() {
        let mut annotated = declarative_case();
        annotated.notes = Some("a reviewer annotation".to_string());
        annotated.allowed_differences = vec![crate::model::AllowedDifference {
            behavior: "bhv-x".to_string(),
            context: vec![],
            observable_path: "chan-exit-code.value".to_string(),
            rationale: "characterized".to_string(),
            waiver_id: Some("wvr-x".to_string()),
            divergence_id: None,
        }];
        let base = declarative_case();
        assert_eq!(
            case_hash(&base, &[]),
            case_hash(&annotated, &[]),
            "notes / allowedDifferences must NOT affect caseHash (D3)"
        );
    }

    #[test]
    fn case_hash_tracks_behavior_affecting_inputs() {
        let base = declarative_case();
        let mut changed = declarative_case();
        changed.operations[0].argv.push("--extra".to_string());
        assert_ne!(
            case_hash(&base, &[]),
            case_hash(&changed, &[]),
            "changing argv must change caseHash"
        );
        assert_ne!(
            case_hash(&base, &[]),
            case_hash(&base, &["fixturehash".to_string()]),
            "changing referenced fixture hashes must change caseHash"
        );
    }

    #[test]
    fn canonicalize_sorts_object_keys_not_arrays() {
        let a = serde_json::json!({ "b": 1, "a": [3, 1, 2] });
        let b = serde_json::json!({ "a": [3, 1, 2], "b": 1 });
        assert_eq!(
            canonical_bytes(&a),
            canonical_bytes(&b),
            "object key order is insignificant"
        );
        let c = serde_json::json!({ "a": [1, 2, 3], "b": 1 });
        assert_ne!(
            canonical_bytes(&a),
            canonical_bytes(&c),
            "array order IS significant"
        );
    }
}
