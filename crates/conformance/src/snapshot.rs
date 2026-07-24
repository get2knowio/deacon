//! Committed, provenance-tracked snapshots + staleness comparison (data-model §7,
//! contract snapshot-provenance.md, 022-conformance-runner, US2).
//!
//! Snapshots live under `conformance/snapshots/<os>-<arch>/<case-id>/` as three files —
//! `provenance.json` (the FR-017 identity/environment elements), `raw.json` (verbatim
//! per-channel evidence), and `normalized.json` (rule-normalized evidence, kept separate
//! per FR-016). The **pure** staleness comparison ([`compare_staleness`]) is hermetic and
//! lives here; the live re-record path is `parity-harness`'s `conformance-snapshot` bin
//! (research D5). `platform`/`arch` are SELECTORS (they pick a snapshot), never staleness
//! signals; a missing snapshot for the current `os-arch` is a distinct
//! `no-reference-for-platform` outcome (FR-016a).

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The single source of truth for the normalizer version, recorded in snapshot
/// provenance and participating in staleness (FR-030). `parity-harness`'s
/// `normalize::NORMALIZER_VERSION` re-exports THIS constant so the two never drift
/// (`parity-harness` depends on `deacon-conformance`, not the reverse). Bumped in
/// lockstep with any named-normalization-rule change (US3 set it to `"2"`).
pub const NORMALIZER_VERSION: &str = "2";

/// The `provenance.json` record — the FR-017 identity/environment elements (data-model
/// §7, contract snapshot-provenance.md). Thirteen fields: twelve identity/environment
/// elements plus the informational `capturedAt`. The captured observables (the
/// thirteenth FR-017 element) live in the sibling `raw.json`/`normalized.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Provenance {
    /// Pinned `@devcontainers/cli` version (verified via `oracle.rs` at record time).
    pub oracle_version: String,
    /// Pinned spec/source revision (e.g. `113500f4`).
    pub source_revision: String,
    /// The case hash over behavior-affecting inputs (`case_hash`).
    pub case_hash: String,
    /// The combined fixture hash.
    pub fixture_hash: String,
    /// The complete argv actually executed, with temp paths tokenized (`<WORKSPACE>`).
    pub argv: Vec<String>,
    /// OS selector (e.g. `linux`, `macos`). NOT a staleness field.
    pub platform: String,
    /// Architecture selector (e.g. `x86_64`, `aarch64`). NOT a staleness field.
    pub arch: String,
    /// Node version the oracle ran under (informational; NOT a staleness field — see
    /// [`Provenance::staleness_fields`]).
    pub node_version: String,
    /// Docker engine version (informational; NOT a staleness field).
    pub docker_version: String,
    /// Compose version (informational; NOT a staleness field).
    pub compose_version: String,
    /// image ref → digest for every pinned image the operations used.
    pub image_digests: IndexMap<String, String>,
    /// The `NORMALIZER_VERSION` the evidence was normalized under.
    pub normalizer_version: String,
    /// Provenance timestamp — informational; NOT one of the FR-017 thirteen and NOT a
    /// staleness field (contract snapshot-provenance.md).
    pub captured_at: String,
}

impl Provenance {
    /// The staleness fields, in comparison order, as `(name, recorded)` pairs — the ONLY
    /// fields [`compare_staleness`] compares. These are the *inputs* that determine the
    /// recorded evidence: the case/fixture hashes, the reference (`oracleVersion`) and spec
    /// (`sourceRevision`) pins, the pinned `imageDigests`, and the `normalizerVersion`.
    ///
    /// `nodeVersion`/`dockerVersion`/`composeVersion` are recorded in provenance for
    /// reproducibility but are DELIBERATELY NOT staleness signals. The reference CLI's
    /// output is independent of the host Node runtime, and any Docker/Compose-version
    /// effect on recorded evidence surfaces as a real evidence *divergence* on replay — not
    /// a false stale. Gating staleness on host tool versions would make every committed
    /// snapshot stale on every machine but the recorder's, defeating cross-machine CI
    /// replay (SC-003) — e.g. a snapshot recorded under Node 22 would falsely fail the
    /// parity lane's Node 20. Like `argv`/`platform`/`arch`/`capturedAt`, they are
    /// informational selectors, not staleness signals.
    fn staleness_fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("caseHash", self.case_hash.clone()),
            ("fixtureHash", self.fixture_hash.clone()),
            ("oracleVersion", self.oracle_version.clone()),
            ("sourceRevision", self.source_revision.clone()),
            ("imageDigests", canonical_digests(&self.image_digests)),
            ("normalizerVersion", self.normalizer_version.clone()),
        ]
    }
}

/// A deterministic string form of the image-digest map for staleness comparison
/// (key-sorted so ordering never falsely triggers staleness).
fn canonical_digests(digests: &IndexMap<String, String>) -> String {
    let mut pairs: Vec<(&String, &String)> = digests.iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    pairs
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// The result of a staleness comparison (FR-020).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Staleness {
    /// Every staleness field matches — the snapshot is fresh.
    Fresh,
    /// A staleness field drifted; `field` names the FIRST mismatch (recorded vs current).
    Stale {
        /// The camelCase name of the first drifted field.
        field: String,
        /// The recorded value.
        recorded: String,
        /// The current (recomputed/probed) value.
        current: String,
    },
}

/// Pure staleness comparison (FR-020, D5): compare `recorded` provenance to a `current`
/// provenance (recomputed input-derived fields), returning [`Staleness::Fresh`] or the
/// FIRST drifted field. Only the input-derived staleness fields are compared (see
/// [`Provenance::staleness_fields`]) — `argv`/`platform`/`arch`/`capturedAt` and the host
/// tool versions (`nodeVersion`/`dockerVersion`/`composeVersion`) never trigger staleness.
/// Total and hermetic.
pub fn compare_staleness(recorded: &Provenance, current: &Provenance) -> Staleness {
    let recorded_fields = recorded.staleness_fields();
    let current_fields = current.staleness_fields();
    for ((name, rec), (_, cur)) in recorded_fields.into_iter().zip(current_fields) {
        if rec != cur {
            return Staleness::Stale {
                field: name.to_string(),
                recorded: rec,
                current: cur,
            };
        }
    }
    Staleness::Fresh
}

/// A committed snapshot: provenance + the raw and normalized evidence (kept SEPARATE,
/// FR-016). `raw`/`normalized` are opaque JSON arrays here (the conformance crate does
/// not interpret channel-evidence semantics — that is `parity-harness`).
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// The 13-field provenance.
    pub provenance: Provenance,
    /// The verbatim per-channel evidence (`raw.json`).
    pub raw: Value,
    /// The rule-normalized per-channel evidence (`normalized.json`).
    pub normalized: Value,
}

/// An error loading a committed snapshot (fail-loud, constitution IV).
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    /// A snapshot file was unreadable.
    #[error("could not read snapshot file {path:?}: {cause}")]
    Read { path: PathBuf, cause: String },
    /// A snapshot file was malformed JSON / schema-invalid.
    #[error("malformed snapshot file {path:?}: {cause}")]
    Malformed { path: PathBuf, cause: String },
}

/// The outcome of resolving a snapshot for a case + platform (FR-016a).
#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    /// A snapshot exists for this `os-arch` and loaded. Boxed — the [`Snapshot`] payload
    /// is far larger than the sibling variant.
    Found(Box<Snapshot>),
    /// No snapshot directory exists for the current `os-arch` — a coverage gap, distinct
    /// from stale and from a silent skip.
    NoReferenceForPlatform {
        /// The `os-arch` selector that had no committed snapshot.
        os_arch: String,
    },
}

/// The current platform selector, `"<os>-<arch>"` (e.g. `linux-x86_64`), from the build
/// target. Both the record key and the recorded `platform`/`arch` derive from this.
pub fn current_os_arch() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// The default committed-snapshots root: `<workspace>/conformance/snapshots`.
pub fn default_snapshots_dir() -> PathBuf {
    crate::workspace_root()
        .join("conformance")
        .join("snapshots")
}

/// The probed host environment versions used to build a `current` provenance for
/// staleness (and recorded verbatim at refresh). Each is `None` when its tool is absent
/// — the caller must NOT invent a value (constitution IV: no fabricated provenance).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EnvProbe {
    /// `node --version`, `v`-stripped (e.g. `22.23.1`).
    pub node_version: Option<String>,
    /// `docker version` server version (e.g. `29.6.2`).
    pub docker_version: Option<String>,
    /// `docker compose version --short` (e.g. `2.40.3`).
    pub compose_version: Option<String>,
}

/// Probe the host for Node / Docker / Compose versions via subprocesses. Best-effort:
/// an absent or failing tool yields `None` for that field (never a fabricated value).
/// The SINGLE probe implementation, shared by `snapshot check` (comparison) and the
/// reviewed refresh (recording) so the two record/compare identical formats.
pub fn probe_environment() -> EnvProbe {
    EnvProbe {
        node_version: run_version(&["node", "--version"])
            .map(|s| s.strip_prefix('v').unwrap_or(&s).to_string()),
        docker_version: run_version(&["docker", "version", "--format", "{{.Server.Version}}"]),
        compose_version: run_version(&["docker", "compose", "version", "--short"]),
    }
}

/// Run `argv` and return the trimmed first non-empty stdout line, or `None` on any
/// failure (missing binary, non-zero exit, empty output).
fn run_version(argv: &[&str]) -> Option<String> {
    let (program, args) = argv.split_first()?;
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// The currently pinned oracle version, read from `fixtures/parity-corpus/oracle.json`
/// (the source of truth for the `oracleVersion` staleness field). `None` if the pin file
/// is absent/malformed.
pub fn current_oracle_version_pin() -> Option<String> {
    let path = crate::workspace_root()
        .join("fixtures")
        .join("parity-corpus")
        .join("oracle.json");
    let raw = std::fs::read_to_string(path).ok()?;
    let doc: Value = serde_json::from_str(&raw).ok()?;
    doc.get("version")?.as_str().map(str::to_string)
}

/// The directory a case's snapshot for `os_arch` lives in:
/// `<snapshots>/<os-arch>/<case-id>/`.
pub fn snapshot_case_dir(snapshots_root: &Path, os_arch: &str, case_id: &str) -> PathBuf {
    snapshots_root.join(os_arch).join(case_id)
}

/// Resolve the snapshot for `case_id` at `os_arch` under `snapshots_root`. A missing
/// `os-arch`/`case` directory is [`Resolution::NoReferenceForPlatform`] (not an error);
/// a present-but-malformed snapshot is a fail-loud [`SnapshotError`].
pub fn resolve(
    snapshots_root: &Path,
    os_arch: &str,
    case_id: &str,
) -> Result<Resolution, SnapshotError> {
    let dir = snapshot_case_dir(snapshots_root, os_arch, case_id);
    if !dir.is_dir() {
        return Ok(Resolution::NoReferenceForPlatform {
            os_arch: os_arch.to_string(),
        });
    }
    Ok(Resolution::Found(Box::new(load_snapshot(&dir)?)))
}

/// Load the three snapshot files from a case's snapshot directory.
pub fn load_snapshot(dir: &Path) -> Result<Snapshot, SnapshotError> {
    let provenance: Provenance = load_json(&dir.join("provenance.json"))?;
    let raw: Value = load_json(&dir.join("raw.json"))?;
    let normalized: Value = load_json(&dir.join("normalized.json"))?;
    Ok(Snapshot {
        provenance,
        raw,
        normalized,
    })
}

/// Load just the provenance (the staleness gate needs nothing else).
pub fn load_provenance(dir: &Path) -> Result<Provenance, SnapshotError> {
    load_json(&dir.join("provenance.json"))
}

fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, SnapshotError> {
    let raw = std::fs::read_to_string(path).map_err(|e| SnapshotError::Read {
        path: path.to_path_buf(),
        cause: e.to_string(),
    })?;
    serde_json::from_str(&raw).map_err(|e| SnapshotError::Malformed {
        path: path.to_path_buf(),
        cause: e.to_string(),
    })
}

/// A single field-level difference between two snapshot trees (`snapshot diff`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDiffEntry {
    /// Which artifact differs: `provenance` / `raw` / `normalized`.
    pub artifact: String,
    /// A dotted path within that artifact (empty = the whole artifact).
    pub path: String,
    /// The old (left) value.
    pub old: Value,
    /// The new (right) value.
    pub new: Value,
}

/// Deterministic drift between two loaded snapshots (`snapshot diff`), artifact then
/// path ordered. Compares provenance field-by-field and raw/normalized structurally.
pub fn diff(old: &Snapshot, new: &Snapshot) -> Vec<SnapshotDiffEntry> {
    let mut out = Vec::new();
    let old_prov = serde_json::to_value(&old.provenance).unwrap_or(Value::Null);
    let new_prov = serde_json::to_value(&new.provenance).unwrap_or(Value::Null);
    diff_value("provenance", "", &old_prov, &new_prov, &mut out);
    diff_value("raw", "", &old.raw, &new.raw, &mut out);
    diff_value("normalized", "", &old.normalized, &new.normalized, &mut out);
    out.sort_by(|a, b| a.artifact.cmp(&b.artifact).then(a.path.cmp(&b.path)));
    out
}

/// Recursively record differences between two values under `artifact`/`path`.
fn diff_value(
    artifact: &str,
    path: &str,
    old: &Value,
    new: &Value,
    out: &mut Vec<SnapshotDiffEntry>,
) {
    match (old, new) {
        (Value::Object(o), Value::Object(n)) => {
            let mut keys: Vec<&String> = o.keys().chain(n.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let child = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                diff_value(
                    artifact,
                    &child,
                    o.get(k).unwrap_or(&Value::Null),
                    n.get(k).unwrap_or(&Value::Null),
                    out,
                );
            }
        }
        _ if old != new => out.push(SnapshotDiffEntry {
            artifact: artifact.to_string(),
            path: path.to_string(),
            old: old.clone(),
            new: new.clone(),
        }),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provenance() -> Provenance {
        Provenance {
            oracle_version: "0.87.0".to_string(),
            source_revision: "113500f4".to_string(),
            case_hash: "aaaa".to_string(),
            fixture_hash: "bbbb".to_string(),
            argv: vec!["read-configuration".to_string()],
            platform: "linux".to_string(),
            arch: "x86_64".to_string(),
            node_version: "22.23.1".to_string(),
            docker_version: "29.6.2".to_string(),
            compose_version: "2.40.3".to_string(),
            image_digests: IndexMap::new(),
            normalizer_version: NORMALIZER_VERSION.to_string(),
            captured_at: "2026-07-24T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn fresh_when_all_staleness_fields_match() {
        assert_eq!(
            compare_staleness(&provenance(), &provenance()),
            Staleness::Fresh
        );
    }

    #[test]
    fn each_staleness_field_drift_is_named() {
        type Mutate = fn(&mut Provenance);
        let cases: &[(&str, Mutate)] = &[
            ("caseHash", |p| p.case_hash = "x".to_string()),
            ("fixtureHash", |p| p.fixture_hash = "x".to_string()),
            ("oracleVersion", |p| p.oracle_version = "9.9.9".to_string()),
            ("sourceRevision", |p| {
                p.source_revision = "deadbeef".to_string()
            }),
            ("imageDigests", |p| {
                p.image_digests
                    .insert("img".to_string(), "sha256:x".to_string());
            }),
            ("normalizerVersion", |p| {
                p.normalizer_version = "99".to_string()
            }),
        ];
        for (field, mutate) in cases {
            let recorded = provenance();
            let mut current = provenance();
            mutate(&mut current);
            match compare_staleness(&recorded, &current) {
                Staleness::Stale { field: got, .. } => {
                    assert_eq!(&got, field, "drift in {field} must be named")
                }
                Staleness::Fresh => panic!("drift in {field} must be stale"),
            }
        }
    }

    #[test]
    fn selectors_and_host_versions_do_not_trigger_staleness() {
        // capturedAt/platform/arch/argv are selectors/informational; the host tool
        // versions (node/docker/compose) are informational too (a snapshot recorded under
        // Node 22 must NOT be stale on a Node 20 replay — cross-machine CI replay, SC-003).
        for mutate in [
            (|p: &mut Provenance| p.captured_at = "2099-01-01T00:00:00Z".to_string())
                as fn(&mut Provenance),
            |p: &mut Provenance| p.platform = "macos".to_string(),
            |p: &mut Provenance| p.arch = "aarch64".to_string(),
            |p: &mut Provenance| p.argv = vec!["different".to_string()],
            |p: &mut Provenance| p.node_version = "20.0.0".to_string(),
            |p: &mut Provenance| p.docker_version = "27.0.0".to_string(),
            |p: &mut Provenance| p.compose_version = "2.29.0".to_string(),
        ] {
            let recorded = provenance();
            let mut current = provenance();
            mutate(&mut current);
            assert_eq!(
                compare_staleness(&recorded, &current),
                Staleness::Fresh,
                "capturedAt/platform/arch/argv/node/docker/compose are not staleness signals"
            );
        }
    }

    #[test]
    fn first_mismatch_wins() {
        let recorded = provenance();
        let mut current = provenance();
        current.fixture_hash = "x".to_string(); // 2nd staleness field
        current.normalizer_version = "9".to_string(); // later staleness field
        match compare_staleness(&recorded, &current) {
            Staleness::Stale { field, .. } => assert_eq!(field, "fixtureHash", "first mismatch"),
            Staleness::Fresh => panic!("must be stale"),
        }
    }

    #[test]
    fn image_digest_order_does_not_trigger_staleness() {
        let mut recorded = provenance();
        recorded
            .image_digests
            .insert("a".to_string(), "1".to_string());
        recorded
            .image_digests
            .insert("b".to_string(), "2".to_string());
        let mut current = provenance();
        current
            .image_digests
            .insert("b".to_string(), "2".to_string());
        current
            .image_digests
            .insert("a".to_string(), "1".to_string());
        assert_eq!(compare_staleness(&recorded, &current), Staleness::Fresh);
    }

    #[test]
    fn diff_reports_provenance_and_evidence_changes() {
        let a = Snapshot {
            provenance: provenance(),
            raw: serde_json::json!([{ "channel": "chan-exit-code", "value": 0 }]),
            normalized: serde_json::json!([{ "channel": "chan-exit-code", "value": 0 }]),
        };
        let mut b = a.clone();
        b.provenance.case_hash = "zzzz".to_string();
        b.raw = serde_json::json!([{ "channel": "chan-exit-code", "value": 1 }]);
        let d = diff(&a, &b);
        assert!(
            d.iter()
                .any(|e| e.artifact == "provenance" && e.path == "caseHash")
        );
        assert!(d.iter().any(|e| e.artifact == "raw"));
        // Identical snapshots diff empty.
        assert!(diff(&a, &a).is_empty());
    }
}
