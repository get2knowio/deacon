//! The slim case/waiver loader — successor to the conformance crate's 1,410-line
//! `load.rs`, which materialized the whole registry (behaviors, sources, obligations,
//! clauses, inventories) to answer questions this suite no longer asks.
//!
//! What a differential parity run needs is exactly two things: the scenario records and
//! the tolerances that may excuse a difference. Everything else that used to load here
//! described the *meta-system*, not the comparison.
//!
//! Strict on mistakes (constitution IV): every record type is `deny_unknown_fields`, and a
//! malformed file is a hard, located failure — never a silent drop that would shrink the
//! scenario set without anyone noticing. Errors accumulate so one run reports every bad
//! record rather than only the first.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use crate::model::{TestCase, Waiver};

/// A located schema failure: the file that failed and why.
#[derive(Debug, Clone)]
pub struct SchemaError {
    pub path: PathBuf,
    pub message: String,
}

impl std::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

/// Every way loading can fail. A missing directory is NOT one of them for waivers (an
/// empty allowlist is legitimate), but it IS one for cases: a case root that resolves to
/// nothing means the suite would pass by running no scenarios.
#[derive(Debug)]
pub enum LoadError {
    Schema(Vec<SchemaError>),
    NoCases(PathBuf),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Schema(errs) => {
                writeln!(f, "{} record(s) failed to load:", errs.len())?;
                for e in errs {
                    writeln!(f, "  {e}")?;
                }
                Ok(())
            }
            LoadError::NoCases(p) => write!(
                f,
                "no case records found under {} — a suite that runs no scenarios passes \
                 vacuously, so this is a hard failure rather than an empty run",
                p.display()
            ),
        }
    }
}

impl std::error::Error for LoadError {}

/// The on-disk envelope shared by every case file: `{"schemaVersion":1,"records":[…]}`.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Envelope<T> {
    #[allow(dead_code)]
    schema_version: u32,
    records: Vec<T>,
}

/// `*.json` directly under `dir`, sorted so a run is deterministic across filesystems.
fn json_files_sorted(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    out.sort();
    out
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, SchemaError> {
    let raw = std::fs::read_to_string(path).map_err(|e| SchemaError {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    serde_json::from_str(&raw).map_err(|e| SchemaError {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

/// Load every case record under `cases_dir`, in sorted file order.
pub fn load_cases(cases_dir: &Path) -> Result<Vec<TestCase>, LoadError> {
    let mut errors = Vec::new();
    let mut out = Vec::new();
    for path in json_files_sorted(cases_dir) {
        match read_json::<Envelope<TestCase>>(&path) {
            Ok(env) => out.extend(env.records),
            Err(e) => errors.push(e),
        }
    }
    if !errors.is_empty() {
        return Err(LoadError::Schema(errors));
    }
    if out.is_empty() {
        return Err(LoadError::NoCases(cases_dir.to_path_buf()));
    }
    Ok(out)
}

/// Load every per-waiver file directly under `dir` (each a single [`Waiver`] object, not a
/// collection), returning `(path, waiver)` pairs. A missing directory yields empty.
pub fn load_waiver_files(dir: &Path) -> Result<Vec<(PathBuf, Waiver)>, LoadError> {
    let mut errors = Vec::new();
    let mut out = Vec::new();
    for path in json_files_sorted(dir) {
        match read_json::<Waiver>(&path) {
            Ok(w) => out.push((path, w)),
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(LoadError::Schema(errors))
    }
}

/// The loaded scenario set. Deliberately just the cases — a registry object that also
/// carried behaviors, obligations and dispositions is what let the meta-system grow.
pub struct Registry {
    pub cases: Vec<TestCase>,
}

impl Registry {
    /// Load from a registry directory, reading `<dir>/cases/`.
    pub fn load(dir: &Path) -> Result<Self, LoadError> {
        Ok(Self {
            cases: load_cases(&dir.join("cases"))?,
        })
    }
}
