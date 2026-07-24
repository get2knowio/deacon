//! Hermetic snapshot-staleness tests (022-conformance-runner, US2 T028/T029, FR-020,
//! SC-003). Cross-platform: they read the COMMITTED `linux-x86_64` snapshot files
//! (committed JSON, loadable on any host) and the pure staleness comparison — no Docker,
//! no oracle, no network.

use std::path::PathBuf;

use deacon_conformance::case_hash::hashes_for_case;
use deacon_conformance::load::Registry;
use deacon_conformance::model::TestCase;
use deacon_conformance::snapshot::{self, Provenance, Resolution, Staleness};
use deacon_conformance::{default_registry_dir, workspace_root};

const CASE_ID: &str = "case-readconfig-snapshot";

fn committed_snapshot_dir() -> PathBuf {
    snapshot::default_snapshots_dir()
        .join("linux-x86_64")
        .join(CASE_ID)
}

fn fixtures_root() -> PathBuf {
    workspace_root().join("conformance").join("fixtures")
}

fn the_case() -> TestCase {
    let reg = Registry::load(&default_registry_dir()).expect("registry loads");
    reg.cases
        .into_iter()
        .find(|c| c.id == CASE_ID)
        .unwrap_or_else(|| panic!("{CASE_ID} must exist in the registry"))
}

fn recorded_provenance() -> Provenance {
    snapshot::load_provenance(&committed_snapshot_dir())
        .expect("the committed snapshot provenance loads")
}

#[test]
fn committed_snapshot_carries_all_thirteen_provenance_fields() {
    // A recorded provenance with every FR-017 field populated — none fabricated: it was
    // written live by `conformance-snapshot refresh` against the pinned oracle.
    let p = recorded_provenance();
    assert_eq!(p.oracle_version, "0.87.0");
    assert_eq!(p.source_revision, "113500f4");
    assert!(!p.case_hash.is_empty() && p.case_hash.len() == 64);
    assert!(!p.fixture_hash.is_empty() && p.fixture_hash.len() == 64);
    assert!(!p.argv.is_empty());
    assert_eq!(p.platform, "linux");
    assert_eq!(p.arch, "x86_64");
    assert!(!p.node_version.is_empty());
    assert!(!p.docker_version.is_empty());
    assert!(!p.compose_version.is_empty());
    assert_eq!(p.normalizer_version, snapshot::NORMALIZER_VERSION);
    assert!(!p.captured_at.is_empty());
    // imageDigests is empty for a config-only read-configuration case (no images).
    assert!(p.image_digests.is_empty());
}

#[test]
fn committed_snapshot_hashes_match_the_committed_case() {
    // Recompute the case/fixture hashes from the committed case + fixtures; they MUST
    // equal the recorded provenance — i.e. the committed snapshot is fresh-by-hash and
    // consistent with the case it belongs to (editing the case without a refresh would
    // break this, exactly as `snapshot check` fails as stale).
    let (case_hash, fixture_hash) =
        hashes_for_case(&the_case(), &fixtures_root()).expect("recompute hashes");
    let p = recorded_provenance();
    assert_eq!(
        case_hash, p.case_hash,
        "recomputed caseHash matches recorded"
    );
    assert_eq!(
        fixture_hash, p.fixture_hash,
        "recomputed fixtureHash matches recorded"
    );
}

#[test]
fn each_staleness_field_drift_is_named() {
    type Mutate = fn(&mut Provenance);
    let cases: &[(&str, Mutate)] = &[
        ("caseHash", |p| p.case_hash = "drift".into()),
        ("fixtureHash", |p| p.fixture_hash = "drift".into()),
        ("oracleVersion", |p| p.oracle_version = "9.9.9".into()),
        ("sourceRevision", |p| p.source_revision = "deadbeef".into()),
        ("imageDigests", |p| {
            p.image_digests.insert("img".into(), "sha256:x".into());
        }),
        ("normalizerVersion", |p| p.normalizer_version = "99".into()),
    ];
    let recorded = recorded_provenance();
    for (field, mutate) in cases {
        let mut current = recorded.clone();
        mutate(&mut current);
        match snapshot::compare_staleness(&recorded, &current) {
            Staleness::Stale { field: got, .. } => {
                assert_eq!(&got, field, "drift in {field} must be named as stale")
            }
            Staleness::Fresh => panic!("drift in {field} must be stale"),
        }
    }
}

#[test]
fn selectors_and_host_versions_do_not_trigger_staleness() {
    // Selectors (platform/arch), informational fields (capturedAt/argv), AND the host
    // tool versions (node/docker/compose) must NOT trigger staleness: a snapshot recorded
    // in this dev container (e.g. Node 22) must stay fresh when replayed on the parity
    // lane (Node 20) — cross-machine CI replay (SC-003). The evidence-determining inputs
    // (case/fixture hashes, oracle/source pins, imageDigests, normalizer) still gate.
    let recorded = recorded_provenance();
    let non_staleness: &[fn(&mut Provenance)] = &[
        |p| p.captured_at = "2099-01-01T00:00:00Z".into(),
        |p| p.platform = "macos".into(),
        |p| p.arch = "aarch64".into(),
        |p| p.argv = vec!["totally".into(), "different".into()],
        |p| p.node_version = "20.0.0".into(),
        |p| p.docker_version = "27.0.0".into(),
        |p| p.compose_version = "2.29.0".into(),
    ];
    for mutate in non_staleness {
        let mut current = recorded.clone();
        mutate(&mut current);
        assert_eq!(
            snapshot::compare_staleness(&recorded, &current),
            Staleness::Fresh,
            "capturedAt/platform/arch/argv/node/docker/compose must NOT trigger staleness"
        );
    }
}

#[test]
fn missing_snapshot_for_current_os_arch_is_no_reference_for_platform() {
    // Resolving a platform with no committed snapshot yields `no-reference-for-platform`
    // — a coverage gap distinct from `stale` (a different Resolution variant entirely, so
    // no staleness comparison even runs) and from a silent skip.
    let snapshots_root = snapshot::default_snapshots_dir();
    let resolution = snapshot::resolve(&snapshots_root, "no-such-osarch", CASE_ID)
        .expect("resolve is fallible only on malformed files");
    match resolution {
        Resolution::NoReferenceForPlatform { os_arch } => assert_eq!(os_arch, "no-such-osarch"),
        Resolution::Found(_) => panic!("there is no snapshot for a bogus os-arch"),
    }

    // The committed platform DOES resolve to a snapshot (the two outcomes are distinct).
    assert!(matches!(
        snapshot::resolve(&snapshots_root, "linux-x86_64", CASE_ID).unwrap(),
        Resolution::Found(_)
    ));
}

/// T038 (FR-021): ordinary runs NEVER write a committed snapshot — the ONLY snapshot
/// writer (`evidence::write_snapshot`) is invoked from exactly one place in library/bin
/// source: the reviewed refresh bin. Any new caller in a runtime path would fail this
/// structural guard, so a snapshot can never be rewritten by a plain `cargo nextest run`.
#[test]
fn only_the_refresh_bin_writes_committed_snapshots() {
    let mut callers: Vec<String> = Vec::new();
    for crate_src in ["conformance/src", "parity-harness/src"] {
        let root = workspace_root().join("crates").join(crate_src);
        scan_for_writer(&root, &mut callers);
    }
    callers.sort();
    // Sorted: the reviewed refresh bin (only caller) + the writer's definition.
    assert_eq!(
        callers,
        vec![
            "parity-harness/src/bin/conformance-snapshot.rs".to_string(),
            "parity-harness/src/evidence.rs".to_string(),
        ],
        "`write_snapshot` must appear only in its definition + the refresh bin; a new \
         caller in a runtime path would let an ordinary run rewrite a committed snapshot"
    );
}

/// Recursively collect `crates/<crate>/src/...` files that reference `write_snapshot(`,
/// as `<crate>/src/<rel>` strings.
fn scan_for_writer(dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_for_writer(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if text.contains("write_snapshot(") {
                    // Render as `<crate>/src/<rel-to-crates>` for a stable assertion.
                    let rel = path
                        .strip_prefix(workspace_root().join("crates"))
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.push(rel);
                }
            }
        }
    }
}
