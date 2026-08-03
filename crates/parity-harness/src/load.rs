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

use crate::model::TestCase;

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

/// One tolerated difference's IDENTITY and its reasoning — a record in
/// `parity/ALLOWLIST.json`.
///
/// The record carries WHY a difference is tolerated. It deliberately does NOT carry
/// WHERE: a case's own `allowedDifferences` name the behavior and the observable path,
/// which is what keeps a tolerance scoped (FR-032/FR-033) and stale-checked per run
/// (FR-034). Restating the scope here would be a second copy that drifts, so
/// [`Registry::load`] computes the relationship instead — every referenced id must
/// resolve, and every record must be referenced.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AllowlistRecord {
    /// `wvr-…` (a difference tolerated on its own terms), `bhv-…` (a characterized
    /// behavior) or `ext-…` (a deacon capability the reference has no equivalent for).
    pub id: String,
    /// One sentence: what the difference IS.
    pub summary: String,
    /// The measured argument for tolerating it. Absent when `summary` already is it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    /// The adjudicated disposition, preserved verbatim — including
    /// `"unadjudicated"` for the ones nobody has ruled on yet, which is information a
    /// missing field would hide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ruling: Option<String>,
}

/// Load `parity/ALLOWLIST.json`. A missing file is an empty allowlist (legitimate); a
/// malformed one is a hard failure.
pub fn load_allowlist(path: &Path) -> Result<Vec<AllowlistRecord>, LoadError> {
    if !path.is_file() {
        return Ok(Vec::new());
    }
    match read_json::<Envelope<AllowlistRecord>>(path) {
        Ok(env) => Ok(env.records),
        Err(e) => Err(LoadError::Schema(vec![e])),
    }
}

/// The loaded scenario set plus the allowlist that may excuse a difference in it.
/// Deliberately nothing else — a registry object that also carried behaviors,
/// obligations and dispositions is what let the meta-system grow.
#[derive(Debug)]
pub struct Registry {
    pub cases: Vec<TestCase>,
    pub allowlist: Vec<AllowlistRecord>,
}

impl Registry {
    /// Load from the parity data root, reading `<dir>/cases/` and `<dir>/ALLOWLIST.json`.
    ///
    /// Loading FAILS if the two disagree, in either direction:
    ///
    /// - a tolerance naming an id no record defines is an **unbacked** tolerance. The
    ///   model has always required a backing identity (`AllowedDifference::resolved_id`
    ///   rejects a tolerance carrying neither id, and rejects one carrying both), but
    ///   until the allowlist existed nothing checked that the id it named was real — so
    ///   a typo, or a record deleted out from under it, left a tolerance excusing a
    ///   difference on the authority of nothing.
    /// - a record no tolerance references is an **orphan**: the difference it excuses is
    ///   no longer declared anywhere, so it is the identity-level form of the stale
    ///   tolerance FR-034 already fails on per run. Keeping it is strictly worse than
    ///   deleting it — it reads as characterized coverage that is not being exercised.
    pub fn load(dir: &Path) -> Result<Self, LoadError> {
        let cases = load_cases(&dir.join("cases"))?;
        let allowlist = load_allowlist(&dir.join("ALLOWLIST.json"))?;
        let path = dir.join("ALLOWLIST.json");

        let defined: std::collections::HashSet<&str> =
            allowlist.iter().map(|r| r.id.as_str()).collect();
        let mut referenced: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut errors = Vec::new();
        for case in &cases {
            for ad in &case.allowed_differences {
                let id = match ad.resolved_id() {
                    Ok(id) => id,
                    // Neither id, or both — the model's own well-formedness rule. Reported
                    // here rather than skipped: a tolerance with no resolvable identity is
                    // the unbacked case in its purest form.
                    Err(e) => {
                        errors.push(SchemaError {
                            path: path.clone(),
                            message: format!(
                                "case `{}` declares a tolerance at `{}` with no resolvable \
                                 backing identity: {}",
                                case.id,
                                ad.observable_path,
                                e.message()
                            ),
                        });
                        continue;
                    }
                };
                referenced.insert(id);
                if !defined.contains(id) {
                    errors.push(SchemaError {
                        path: path.clone(),
                        message: format!(
                            "case `{}` tolerates a difference at `{}` backed by `{id}`, which no \
                             allowlist record defines — an unbacked tolerance excuses a \
                             difference on the authority of nothing",
                            case.id, ad.observable_path
                        ),
                    });
                }
            }
        }
        for record in &allowlist {
            if !referenced.contains(record.id.as_str()) {
                errors.push(SchemaError {
                    path: path.clone(),
                    message: format!(
                        "allowlist record `{}` is referenced by no case — an orphan reads as \
                         characterized coverage that nothing exercises. Remedy: delete it, or \
                         declare the tolerance on the case that reproduces the difference",
                        record.id
                    ),
                });
            }
        }
        if !errors.is_empty() {
            return Err(LoadError::Schema(errors));
        }
        Ok(Self { cases, allowlist })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed data must satisfy both directions of the agreement. This is the
    /// hermetic guard: it runs in every profile, needs no Docker and no oracle, and it
    /// fails the moment a tolerance names a record nobody wrote or a record stops being
    /// referenced.
    #[test]
    fn the_committed_cases_and_allowlist_agree() {
        let root = crate::parity_root();
        let registry = Registry::load(&root)
            .unwrap_or_else(|e| panic!("the committed parity data must load:\n{e}"));
        assert!(!registry.cases.is_empty());
        assert!(
            !registry.allowlist.is_empty(),
            "the committed allowlist is not empty; an empty one here would mean the file \
             moved and `load_allowlist` silently returned nothing"
        );
    }

    #[test]
    fn an_unbacked_tolerance_fails_to_load() {
        // Break it and watch it fail: a tolerance whose backing id nothing defines.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("cases")).expect("cases dir");
        std::fs::write(
            dir.path().join("cases/probe.json"),
            r#"{"schemaVersion":1,"records":[{
                 "id":"case-probe","behaviors":["bhv-probe"],
                 "oracleType":"live-differential",
                 "operations":[{"id":"op-1","subcommand":"read-configuration","argv":[]}],
                 "expected":[],
                 "allowedDifferences":[{"behavior":"bhv-probe",
                   "observablePath":"chan-stdout.x","rationale":"probe",
                   "divergenceId":"bhv-does-not-exist"}]
               }]}"#,
        )
        .expect("write case");
        std::fs::write(
            dir.path().join("ALLOWLIST.json"),
            r#"{"schemaVersion":1,"records":[]}"#,
        )
        .expect("write allowlist");

        let err = Registry::load(dir.path()).expect_err("an unbacked tolerance must fail");
        let msg = err.to_string();
        assert!(
            msg.contains("bhv-does-not-exist") && msg.contains("authority of nothing"),
            "the failure must name the unresolved id: {msg}"
        );
    }

    #[test]
    fn an_orphan_allowlist_record_fails_to_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("cases")).expect("cases dir");
        std::fs::write(
            dir.path().join("cases/probe.json"),
            r#"{"schemaVersion":1,"records":[{
                 "id":"case-probe","behaviors":["bhv-probe"],
                 "oracleType":"live-differential",
                 "operations":[{"id":"op-1","subcommand":"read-configuration","argv":[]}],
                 "expected":[]
               }]}"#,
        )
        .expect("write case");
        std::fs::write(
            dir.path().join("ALLOWLIST.json"),
            r#"{"schemaVersion":1,"records":[
                 {"id":"wvr-nobody-uses-me","summary":"an orphan"}]}"#,
        )
        .expect("write allowlist");

        let err = Registry::load(dir.path()).expect_err("an orphan record must fail");
        assert!(
            err.to_string().contains("wvr-nobody-uses-me"),
            "the failure must name the orphan: {err}"
        );
    }
}
