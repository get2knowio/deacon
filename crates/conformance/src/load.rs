//! Registry loader (T003, FR-019).
//!
//! Reads the full `conformance/registry/` layout into a single [`Registry`]
//! aggregate:
//!
//! - collection files (`{ schemaVersion, records }`): `revisions.json`,
//!   `dimensions.json`, `channels.json`, `profiles.json`, `cases.json`,
//!   `gaps.json`, `extensions.json`, and `sources/{schema,spec,cli,observed}.json`;
//! - per-area behavior files: `behaviors/*.json` (each a collection);
//! - per-waiver files: `waivers/*.json` (each a single record object).
//!
//! Missing collection files / directories are treated as EMPTY (the seed skeleton
//! from T004 carries only revisions/dimensions/channels/profiles). A file that is
//! present but malformed is a [`SchemaError`] carrying the file path and — for
//! JSON syntax errors — a `line:column` location (constitution IV: precise
//! messages). ALL file errors are collected in a single pass (FR-019); the loader
//! never stops at the first bad file. Violation-class validation (V1–V10) is a
//! later phase and lives in `validate.rs`; this module only parses shapes.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::model::{
    BehaviorUnit, CertificationProfile, Classification, Collection, ConstraintInventory,
    ContextDimension, DeaconExtension, Gap, ObservableChannel, SchemasManifest, SourceRevision,
    SourceUnit, TestCase, Waiver,
};

/// A schema-class load failure for a single file: an unreadable file, malformed
/// JSON, or a record that violates the schema (unknown field, bad enum, missing
/// mandatory field). `location` is the `line:column` of a JSON syntax error when
/// available (constitution IV precise messages).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaError {
    pub file: PathBuf,
    pub location: Option<String>,
    pub message: String,
}

impl std::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.location {
            Some(loc) => write!(f, "{}:{}: {}", self.file.display(), loc, self.message),
            None => write!(f, "{}: {}", self.file.display(), self.message),
        }
    }
}

/// The load-time (and schema-inventory generation) error taxonomy (`thiserror`
/// domain errors).
///
/// The `Root`/`Schema` variants cover registry loading; the remaining variants are
/// the schema constraint inventory's cause-specific failures (data-model.md §5,
/// FR-009) — malformed schemas/refs, unresolved or external refs, `$ref` cycles,
/// manifest fingerprint mismatch, stable-ID collision, and a stale committed
/// inventory. They are fail-loud: no partial inventory is ever produced
/// (constitution IV).
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The registry root itself is unreadable / not a directory — a usage/IO error
    /// (CLI exit code 2), distinct from per-file schema failures.
    #[error("cannot read registry root {path:?}: {cause}")]
    Root { path: PathBuf, cause: String },

    /// One or more files failed to parse or violated the schema. Every failing file
    /// is reported (FR-019); the `Display` lists them one per line.
    #[error("{}", format_schema_errors(.0))]
    Schema(Vec<SchemaError>),

    /// A schema document (vendored or fixture) is not valid JSON or not a
    /// well-formed schema object. Fail-loud: no partial inventory is produced.
    #[error("malformed schema {document:?}: {cause}")]
    MalformedSchema { document: String, cause: String },

    /// A `$ref` value could not be parsed as an RFC 6901 JSON Pointer reference.
    #[error("malformed $ref {reference:?} at {pointer:?} in {document:?}")]
    MalformedRef {
        document: String,
        pointer: String,
        reference: String,
    },

    /// A fragment `$ref` names a target pointer absent from the pinned document.
    #[error("unresolved $ref {reference:?} (target {target:?} not found) in {document:?}")]
    UnresolvedRef {
        document: String,
        reference: String,
        target: String,
    },

    /// A `$ref` points outside the explicitly pinned schema set (a live URL or a
    /// path naming no manifest document). Never fetched — a hard error by design.
    #[error(
        "unresolved external $ref {reference:?} in {document:?}: \
         not an explicitly pinned document"
    )]
    UnresolvedExternalRef { document: String, reference: String },

    /// A pure `$ref` chain forms a cycle. The message renders the full chain so the
    /// offending loop is directly reviewable.
    #[error("reference cycle: {}", .chain.join(" -> "))]
    RefCycle { chain: Vec<String> },

    /// A vendored schema file's SHA-256 does not match the manifest fingerprint
    /// (V14). Blocking: the vendored copy no longer matches the claimed revision.
    #[error("manifest fingerprint mismatch for {file:?}: manifest {expected}, file {actual}")]
    ManifestFingerprintMismatch {
        file: PathBuf,
        expected: String,
        actual: String,
    },

    /// Two constraint units derived the same stable id — an astronomically unlikely
    /// `hash8` collision. Remedy: widen that unit's hash, never silent renumbering.
    #[error("stable id collision on {id:?}: pointers {first:?} and {second:?}")]
    IdCollision {
        id: String,
        first: String,
        second: String,
    },

    /// The committed inventory does not match a fresh regeneration (V14) — either a
    /// hand edit to the machine-owned file or a stale commit.
    #[error("committed inventory is out of date: regeneration differs from {path:?}")]
    InventoryOutOfDate { path: PathBuf },
}

/// Render a collected batch of schema errors, one per line, for `LoadError::Schema`.
fn format_schema_errors(errors: &[SchemaError]) -> String {
    let mut out = format!("registry has {} schema error(s):", errors.len());
    for e in errors {
        out.push('\n');
        out.push_str("  ");
        out.push_str(&e.to_string());
    }
    out
}

/// The in-memory registry aggregate — every record type, keyed by its collection.
///
/// Order within each `Vec` is the on-disk (file) order; ID-sort validation and
/// cross-reference resolution are validation concerns (later phase), not the
/// loader's.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    pub revisions: Vec<SourceRevision>,
    pub sources: Vec<SourceUnit>,
    pub dimensions: Vec<ContextDimension>,
    pub channels: Vec<ObservableChannel>,
    pub profiles: Vec<CertificationProfile>,
    pub behaviors: Vec<BehaviorUnit>,
    pub cases: Vec<TestCase>,
    pub gaps: Vec<Gap>,
    pub waivers: Vec<Waiver>,
    pub extensions: Vec<DeaconExtension>,
    /// Hand-authored classification records — `classifications/*.json`, one file
    /// per manifest document key (020-schema-constraint-inventory). A missing
    /// directory is empty (loaded before any classifications are authored).
    pub classifications: Vec<Classification>,
}

impl Registry {
    /// Load and parse the registry rooted at `root`, collecting ALL file errors.
    ///
    /// - `LoadError::Root` if `root` is not a readable directory.
    /// - `LoadError::Schema` if any present file is malformed / schema-invalid;
    ///   every failing file is included.
    pub fn load(root: &Path) -> Result<Registry, LoadError> {
        if !root.is_dir() {
            return Err(LoadError::Root {
                path: root.to_path_buf(),
                cause: "not a directory".to_string(),
            });
        }

        let mut errors: Vec<SchemaError> = Vec::new();

        // The four source-inventory files (each a collection of source units),
        // concatenated in a stable order.
        let sources_dir = root.join("sources");
        let mut sources: Vec<SourceUnit> = Vec::new();
        for name in ["schema.json", "spec.json", "cli.json", "observed.json"] {
            sources.append(&mut load_collection::<SourceUnit>(
                &sources_dir,
                name,
                &mut errors,
            ));
        }

        let registry = Registry {
            // Single-file collections at the registry root.
            revisions: load_collection(root, "revisions.json", &mut errors),
            dimensions: load_collection(root, "dimensions.json", &mut errors),
            channels: load_collection(root, "channels.json", &mut errors),
            profiles: load_collection(root, "profiles.json", &mut errors),
            cases: load_collection(root, "cases.json", &mut errors),
            gaps: load_collection(root, "gaps.json", &mut errors),
            extensions: load_collection(root, "extensions.json", &mut errors),
            sources,
            // behaviors/*.json — one collection per area.
            behaviors: load_dir_collections(&root.join("behaviors"), &mut errors),
            // waivers/*.json — one single-record object per waiver.
            waivers: load_waivers(&root.join("waivers"), &mut errors),
            // classifications/*.json — one collection per manifest document key.
            classifications: load_dir_collections(&root.join("classifications"), &mut errors),
        };

        if errors.is_empty() {
            Ok(registry)
        } else {
            Err(LoadError::Schema(errors))
        }
    }
}

/// Load a single collection file `dir/name`. A missing file yields an empty vector
/// (seed skeletons omit most collections). A present-but-malformed file pushes a
/// [`SchemaError`] and yields empty.
fn load_collection<T: DeserializeOwned>(
    dir: &Path,
    name: &str,
    errors: &mut Vec<SchemaError>,
) -> Vec<T> {
    let path = dir.join(name);
    if !path.exists() {
        return Vec::new();
    }
    match parse_collection::<T>(&path) {
        Ok(records) => records,
        Err(err) => {
            errors.push(err);
            Vec::new()
        }
    }
}

/// Parse one collection file into its records, or a located [`SchemaError`].
fn parse_collection<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>, SchemaError> {
    let raw = read_file(path)?;
    let collection: Collection<T> = deserialize_located(path, &raw)?;
    Ok(collection.records)
}

/// Load every `*.json` collection file directly under `dir` (used for
/// `behaviors/`), concatenating their records. A missing directory yields empty.
fn load_dir_collections<T: DeserializeOwned>(dir: &Path, errors: &mut Vec<SchemaError>) -> Vec<T> {
    let mut out = Vec::new();
    for path in json_files_sorted(dir, errors) {
        match parse_collection::<T>(&path) {
            Ok(mut records) => out.append(&mut records),
            Err(err) => errors.push(err),
        }
    }
    out
}

/// Load every per-waiver file directly under `dir` (each a single [`Waiver`]
/// object, not a collection), returning the `(path, waiver)` pairs in sorted path
/// order or ALL schema errors in one pass. A missing directory yields an empty
/// vector (mirroring [`Registry::load`]'s treatment of absent collections).
///
/// This is the focused entry point `parity-harness` consumes to read registry
/// waivers through the single conformance loader (research Decision 3) without
/// materializing the whole [`Registry`]: a parity run needs the waiver records, not
/// the behaviors/cases/sources. The returned paths let the caller attach precise
/// per-record locations to its own uniqueness/rationale diagnostics.
pub fn load_waiver_files(dir: &Path) -> Result<Vec<(PathBuf, Waiver)>, LoadError> {
    let mut errors: Vec<SchemaError> = Vec::new();
    let mut out: Vec<(PathBuf, Waiver)> = Vec::new();
    for path in json_files_sorted(dir, &mut errors) {
        match read_file(&path).and_then(|raw| deserialize_located::<Waiver>(&path, &raw)) {
            Ok(waiver) => out.push((path, waiver)),
            Err(err) => errors.push(err),
        }
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(LoadError::Schema(errors))
    }
}

/// Load every `waivers/*.json` file (each a single [`Waiver`] object, not a
/// collection). A missing directory yields empty.
fn load_waivers(dir: &Path, errors: &mut Vec<SchemaError>) -> Vec<Waiver> {
    let mut out = Vec::new();
    for path in json_files_sorted(dir, errors) {
        match read_file(&path).and_then(|raw| deserialize_located::<Waiver>(&path, &raw)) {
            Ok(waiver) => out.push(waiver),
            Err(err) => errors.push(err),
        }
    }
    out
}

/// List `*.json` files directly under `dir`, sorted by path for deterministic
/// iteration. A missing directory yields empty; an unreadable-but-present directory
/// pushes a [`SchemaError`].
fn json_files_sorted(dir: &Path, errors: &mut Vec<SchemaError>) -> Vec<PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }
    let read = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            errors.push(SchemaError {
                file: dir.to_path_buf(),
                location: None,
                message: format!("could not read directory: {e}"),
            });
            return Vec::new();
        }
    };
    let mut files: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    files.sort();
    files
}

/// Read a file's contents, mapping IO failure to a [`SchemaError`].
fn read_file(path: &Path) -> Result<String, SchemaError> {
    std::fs::read_to_string(path).map_err(|e| SchemaError {
        file: path.to_path_buf(),
        location: None,
        message: format!("could not read file: {e}"),
    })
}

/// Deserialize `raw` as `T`, mapping a serde_json error to a [`SchemaError`] that
/// carries the `line:column` location when the error exposes one.
fn deserialize_located<T: DeserializeOwned>(path: &Path, raw: &str) -> Result<T, SchemaError> {
    serde_json::from_str::<T>(raw).map_err(|e| {
        // serde_json reports 0:0 when the position is unknown; suppress that.
        let location = if e.line() == 0 && e.column() == 0 {
            None
        } else {
            Some(format!("{}:{}", e.line(), e.column()))
        };
        SchemaError {
            file: path.to_path_buf(),
            location,
            message: e.to_string(),
        }
    })
}

/// Lowercase-hex encode a byte slice (avoids an external `hex` dependency).
fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // Infallible: writing to a String never errors.
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// The lowercase-hex SHA-256 digest of `bytes` — the manifest fingerprint format.
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_lower(&hasher.finalize())
}

/// Load and fingerprint-verify a schemas manifest at `schemas_dir/manifest.json`
/// (data-model.md §1, 020-schema-constraint-inventory).
///
/// For every `documents[]` entry, reads the sibling vendored file, computes its
/// SHA-256, and compares it to the manifest's recorded `sha256`; a mismatch is a
/// blocking [`LoadError::ManifestFingerprintMismatch`] (V14) — the vendored copy no
/// longer matches the claimed revision. A missing or malformed manifest, or an
/// unreadable vendored file, is a located [`LoadError::Schema`].
pub fn load_schemas_manifest(schemas_dir: &Path) -> Result<SchemasManifest, LoadError> {
    let manifest_path = schemas_dir.join("manifest.json");
    let raw = read_file(&manifest_path).map_err(|e| LoadError::Schema(vec![e]))?;
    let manifest: SchemasManifest =
        deserialize_located(&manifest_path, &raw).map_err(|e| LoadError::Schema(vec![e]))?;

    // Rule 4 (contracts/inventory-schema.md): `documents[].key` values are UNIQUE and
    // safe to use as the `<doc>` component of `cst-<doc>-…` constraint IDs. Enforce
    // non-empty, lowercase `[a-z0-9-]` (kebab-safe — the extractor fixtures use keys
    // like `base-fixture`, so hyphens are admitted; uppercase / whitespace / `.` / `/`
    // and duplicates — the genuinely id-breaking shapes — are rejected fail-loud). The
    // real vendored manifest uses `base`/`feature`, both compliant.
    let mut seen_keys: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for doc in &manifest.documents {
        if doc.key.is_empty()
            || !doc
                .key
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(LoadError::MalformedSchema {
                document: doc.key.clone(),
                cause: format!(
                    "manifest document key {:?} is not a valid `<doc>` id component \
                     (require non-empty lowercase [a-z0-9-])",
                    doc.key
                ),
            });
        }
        if !seen_keys.insert(doc.key.as_str()) {
            return Err(LoadError::MalformedSchema {
                document: doc.key.clone(),
                cause: format!(
                    "duplicate manifest document key {:?} (keys must be unique)",
                    doc.key
                ),
            });
        }
    }

    for doc in &manifest.documents {
        let file = schemas_dir.join(&doc.file);
        let bytes = std::fs::read(&file).map_err(|e| {
            LoadError::Schema(vec![SchemaError {
                file: file.clone(),
                location: None,
                message: format!("could not read vendored schema: {e}"),
            }])
        })?;
        let actual = sha256_hex(&bytes);
        if actual != doc.sha256 {
            return Err(LoadError::ManifestFingerprintMismatch {
                file,
                expected: doc.sha256.clone(),
                actual,
            });
        }
    }
    Ok(manifest)
}

/// Load the committed constraint inventory at `path` (data-model.md §2).
///
/// Returns `Ok(None)` when the file is absent — later phases generate it, so
/// loading before the first generation is not an error. A present-but-malformed
/// file is a located [`LoadError::Schema`].
pub fn load_inventory(path: &Path) -> Result<Option<ConstraintInventory>, LoadError> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = read_file(path).map_err(|e| LoadError::Schema(vec![e]))?;
    let inventory: ConstraintInventory =
        deserialize_located(path, &raw).map_err(|e| LoadError::Schema(vec![e]))?;
    Ok(Some(inventory))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &Path, rel: &str, contents: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn missing_root_is_a_root_error() {
        let err = Registry::load(Path::new("/nonexistent/registry/xyz")).unwrap_err();
        assert!(matches!(err, LoadError::Root { .. }));
    }

    #[test]
    fn empty_registry_dir_loads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let reg = Registry::load(dir.path()).expect("empty dir loads");
        assert!(reg.revisions.is_empty());
        assert!(reg.behaviors.is_empty());
        assert!(reg.waivers.is_empty());
    }

    #[test]
    fn loads_minimal_skeleton_with_only_some_collections() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "revisions.json",
            r#"{ "schemaVersion": 1, "records": [
                { "id": "rev-oracle-0-87-0", "kind": "oracle", "pin": "0.87.0", "url": "u",
                  "verifiedAgainst": "fixtures/parity-corpus/oracle.json" }
            ] }"#,
        );
        write(
            dir.path(),
            "channels.json",
            r#"{ "schemaVersion": 1, "records": [
                { "id": "chan-stdout", "description": "standard output" }
            ] }"#,
        );
        let reg = Registry::load(dir.path()).expect("skeleton loads");
        assert_eq!(reg.revisions.len(), 1);
        assert_eq!(reg.revisions[0].pin, "0.87.0");
        assert_eq!(reg.channels.len(), 1);
        // Collections that were never written are simply empty.
        assert!(reg.profiles.is_empty());
        assert!(reg.sources.is_empty());
    }

    #[test]
    fn loads_behaviors_and_waivers_and_sources() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "behaviors/read-configuration.json",
            r#"{ "schemaVersion": 1, "records": [
                { "id": "bhv-a", "area": "read-configuration", "statement": "s",
                  "spec": "unspecified", "reference": "divergent", "decision": "intentional-divergence" }
            ] }"#,
        );
        write(
            dir.path(),
            "sources/observed.json",
            r#"{ "schemaVersion": 1, "records": [
                { "id": "src-obs-a", "inventory": "observed", "revision": "rev-oracle-0-87-0",
                  "locator": "errors/malformed-json", "summary": "reference is lenient",
                  "behaviors": ["bhv-a"] }
            ] }"#,
        );
        write(
            dir.path(),
            "waivers/wvr-malformed-json.json",
            r#"{ "id": "wvr-malformed-json", "behaviors": ["bhv-a"],
                "scope": { "kind": "corpus_case", "corpus": "errors", "case": "malformed-json" },
                "expect": { "kind": "deacon-stricter" },
                "rationale": "characterized divergence", "added": "2026-07-19", "expires": "2027-01-19" }"#,
        );
        let reg = Registry::load(dir.path()).expect("loads");
        assert_eq!(reg.behaviors.len(), 1);
        assert_eq!(reg.behaviors[0].area, "read-configuration");
        assert_eq!(reg.sources.len(), 1);
        assert_eq!(reg.waivers.len(), 1);
        assert_eq!(reg.waivers[0].id, "wvr-malformed-json");
    }

    #[test]
    fn load_waiver_files_returns_paths_and_records() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "waivers/wvr-a.json",
            r#"{ "id": "wvr-a", "behaviors": ["bhv-a"],
                "scope": { "kind": "corpus_case", "corpus": "errors", "case": "a" },
                "expect": { "kind": "both-reject" },
                "rationale": "r", "added": "2026-07-19", "expires": "2027-01-19" }"#,
        );
        let pairs = load_waiver_files(&dir.path().join("waivers")).expect("waivers load");
        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].0.ends_with("wvr-a.json"));
        assert_eq!(pairs[0].1.id, "wvr-a");

        // A missing directory is empty, not an error (absent waivers dir is fine).
        let none =
            load_waiver_files(&dir.path().join("nonexistent")).expect("missing dir is empty");
        assert!(none.is_empty());
    }

    #[test]
    fn load_waiver_files_reports_every_bad_record() {
        let dir = tempfile::tempdir().unwrap();
        // Missing the mandatory `expires` field → schema error.
        write(
            dir.path(),
            "waivers/wvr-bad.json",
            r#"{ "id": "wvr-bad",
                "scope": { "kind": "corpus_case", "corpus": "errors", "case": "b" },
                "expect": { "kind": "both-reject" }, "rationale": "r", "added": "2026-07-19" }"#,
        );
        let err = load_waiver_files(&dir.path().join("waivers")).unwrap_err();
        assert!(matches!(err, LoadError::Schema(ref e) if e.len() == 1));
    }

    #[test]
    fn loads_classifications_collection() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "classifications/base.json",
            r#"{ "schemaVersion": 1, "records": [
                { "id": "cls-base-forwardports-type-3fa9c214",
                  "constraint": "cst-base-forwardports-type-3fa9c214",
                  "disposition": "behavior-mapped",
                  "behaviors": ["bhv-readconfig-wrong-type-forwardports-rejected"] }
            ] }"#,
        );
        let reg = Registry::load(dir.path()).expect("loads");
        assert_eq!(reg.classifications.len(), 1);
        assert_eq!(
            reg.classifications[0].constraint,
            "cst-base-forwardports-type-3fa9c214"
        );
        // The scaffold sentinel disposition is a hard SCHEMA failure at load.
        write(
            dir.path(),
            "classifications/feature.json",
            r#"{ "schemaVersion": 1, "records": [
                { "id": "cls-x", "constraint": "cst-x", "disposition": "UNREVIEWED" }
            ] }"#,
        );
        let err = Registry::load(dir.path()).unwrap_err();
        assert!(matches!(err, LoadError::Schema(_)));
    }

    #[test]
    fn load_schemas_manifest_verifies_fingerprints() {
        let dir = tempfile::tempdir().unwrap();
        let schema_bytes = br#"{"type":"object"}"#;
        let good = sha256_hex(schema_bytes);
        fs::write(dir.path().join("doc.schema.json"), schema_bytes).unwrap();

        // Matching fingerprint loads.
        let manifest_json = format!(
            r#"{{ "schemaVersion": 1, "revision": "rev-schema-113500f4", "documents": [
                {{ "key": "base", "file": "doc.schema.json",
                   "upstreamUrl": "https://example/doc.json", "sha256": "{good}" }}
            ] }}"#
        );
        fs::write(dir.path().join("manifest.json"), &manifest_json).unwrap();
        let manifest = load_schemas_manifest(dir.path()).expect("verified manifest loads");
        assert_eq!(manifest.revision, "rev-schema-113500f4");
        assert_eq!(manifest.documents[0].key, "base");

        // Tampered file bytes → fingerprint mismatch, naming the file.
        fs::write(dir.path().join("doc.schema.json"), br#"{"type":"array"}"#).unwrap();
        let err = load_schemas_manifest(dir.path()).unwrap_err();
        match err {
            LoadError::ManifestFingerprintMismatch {
                file,
                expected,
                actual,
            } => {
                assert!(file.ends_with("doc.schema.json"));
                assert_eq!(expected, good);
                assert_ne!(actual, good);
            }
            other => panic!("expected fingerprint mismatch, got {other:?}"),
        }
    }

    #[test]
    fn load_schemas_manifest_rejects_malformed_and_duplicate_keys() {
        let dir = tempfile::tempdir().unwrap();
        let schema_bytes = br#"{"type":"object"}"#;
        let good = sha256_hex(schema_bytes);
        fs::write(dir.path().join("doc.schema.json"), schema_bytes).unwrap();

        // An uppercase key is not a valid `<doc>` id component → MalformedSchema.
        let bad_key = format!(
            r#"{{ "schemaVersion": 1, "revision": "rev-schema-113500f4", "documents": [
                {{ "key": "Base", "file": "doc.schema.json",
                   "upstreamUrl": "https://example/doc.json", "sha256": "{good}" }}
            ] }}"#
        );
        fs::write(dir.path().join("manifest.json"), &bad_key).unwrap();
        assert!(matches!(
            load_schemas_manifest(dir.path()).unwrap_err(),
            LoadError::MalformedSchema { .. }
        ));

        // Duplicate keys → MalformedSchema.
        let dup = format!(
            r#"{{ "schemaVersion": 1, "revision": "rev-schema-113500f4", "documents": [
                {{ "key": "base", "file": "doc.schema.json",
                   "upstreamUrl": "https://example/doc.json", "sha256": "{good}" }},
                {{ "key": "base", "file": "doc.schema.json",
                   "upstreamUrl": "https://example/doc.json", "sha256": "{good}" }}
            ] }}"#
        );
        fs::write(dir.path().join("manifest.json"), &dup).unwrap();
        assert!(matches!(
            load_schemas_manifest(dir.path()).unwrap_err(),
            LoadError::MalformedSchema { .. }
        ));

        // A kebab key (as the extractor fixtures use) is accepted.
        let kebab = format!(
            r#"{{ "schemaVersion": 1, "revision": "rev-schema-113500f4", "documents": [
                {{ "key": "base-fixture", "file": "doc.schema.json",
                   "upstreamUrl": "https://example/doc.json", "sha256": "{good}" }}
            ] }}"#
        );
        fs::write(dir.path().join("manifest.json"), &kebab).unwrap();
        assert_eq!(
            load_schemas_manifest(dir.path()).unwrap().documents[0].key,
            "base-fixture"
        );
    }

    #[test]
    fn load_schemas_manifest_missing_manifest_is_schema_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = load_schemas_manifest(dir.path()).unwrap_err();
        assert!(matches!(err, LoadError::Schema(_)));
    }

    #[test]
    fn load_inventory_missing_is_none_and_present_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("constraints.json");
        assert!(
            load_inventory(&path).expect("missing is ok").is_none(),
            "absent inventory loads as None"
        );

        fs::write(
            &path,
            r#"{ "schemaVersion": 1, "revision": "rev-schema-113500f4", "units": [
                { "id": "cst-base-forwardports-type-3fa9c214", "document": "base",
                  "pointer": "/definitions/devContainerCommon/properties/forwardPorts",
                  "kind": "type", "substance": { "type": "array" }, "context": null }
            ] }"#,
        )
        .unwrap();
        let inv = load_inventory(&path).expect("present loads").expect("some");
        assert_eq!(inv.units.len(), 1);
        assert_eq!(inv.units[0].document, "base");

        // A malformed inventory is a located schema error.
        fs::write(&path, "{ not json").unwrap();
        assert!(matches!(
            load_inventory(&path).unwrap_err(),
            LoadError::Schema(_)
        ));
    }

    #[test]
    fn collects_all_schema_errors_in_one_pass() {
        let dir = tempfile::tempdir().unwrap();
        // Malformed JSON syntax (missing closing brace).
        write(
            dir.path(),
            "revisions.json",
            r#"{ "schemaVersion": 1, "records": ["#,
        );
        // Well-formed JSON but unknown field → schema violation.
        write(
            dir.path(),
            "channels.json",
            r#"{ "schemaVersion": 1, "records": [
                { "id": "chan-x", "description": "d", "oops": 1 }
            ] }"#,
        );
        let err = Registry::load(dir.path()).unwrap_err();
        match err {
            LoadError::Schema(errors) => {
                assert_eq!(errors.len(), 2, "both bad files reported in one pass");
                let files: Vec<String> = errors
                    .iter()
                    .map(|e| e.file.file_name().unwrap().to_string_lossy().into_owned())
                    .collect();
                assert!(files.contains(&"revisions.json".to_string()));
                assert!(files.contains(&"channels.json".to_string()));
                // The JSON syntax error carries a line:column location.
                let syntax = errors
                    .iter()
                    .find(|e| e.file.ends_with("revisions.json"))
                    .unwrap();
                assert!(syntax.location.is_some(), "syntax error must be located");
            }
            other => panic!("expected schema errors, got {other:?}"),
        }
    }
}
