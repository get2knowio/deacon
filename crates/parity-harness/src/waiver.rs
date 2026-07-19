//! Unified waiver schema + loader + staleness validation (research D6,
//! FR-010..FR-012).
//!
//! There is exactly ONE waiver record schema and ONE loader for every
//! characterized divergence in the parity surface, replacing the two retired
//! mechanisms (bare pre-schema `errors/*/expect.json` and the empty
//! `KNOWN_INTENTIONAL_DIVERGENCES` / `KNOWN_GAPS` Rust consts). Records live
//! adjacent to what they waive:
//!
//! - `fixtures/parity-corpus/errors/<case>/expect.json` — corpus-case scope
//!   (the accept/reject decision matrix for the error corpus);
//! - `fixtures/parity-corpus/waivers/*.json` — state-field scope (observable
//!   state divergences) or additional corpus-case records.
//!
//! Every loaded record is schema-validated (`deny_unknown_fields`, non-empty
//! `rationale`, globally-unique `id`); a typo must fail loudly at load time, not
//! silently widen a waiver. **Staleness** (FR-011): each run, a loaded record
//! must match an existing case/field AND its expected difference must actually be
//! observed; a record that is loaded but never consumed is stale and fails the
//! run naming its `id` ([`WaiverSet::stale_among`]).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::HarnessError;

/// What a waiver attaches to (tagged union on `kind`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Scope {
    /// A single corpus case's accept/reject (and, when both accept, value)
    /// outcome — e.g. the error corpus decision matrix.
    CorpusCase { corpus: String, case: String },
    /// A single observable-state field on a named fixture of a named binary.
    /// `field` supports an exact match or a trailing-`*` prefix.
    StateField {
        binary: String,
        fixture: String,
        field: String,
    },
}

/// The characterized outcome (tagged union on `kind`).
///
/// Not `Eq`: `FieldDivergence` carries arbitrary JSON (`f64` is not `Eq`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Expect {
    /// Both CLIs reject the input; a comparable-outcome agreement. Modeled as an
    /// (empty) struct variant, not a unit variant, so `deny_unknown_fields` also
    /// rejects a stray sibling key (a unit variant would silently ignore it).
    BothReject {},
    /// Both CLIs accept; resolved values are compared normally.
    BothAccept {},
    /// deacon rejects, the reference accepts — an intentional strictness
    /// divergence (constitution IV). Optional `signal` lists informational
    /// stderr substrings (not part of the pass/fail decision).
    DeaconStricter {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signal: Option<Vec<String>>,
    },
    /// deacon ACCEPTS where the reference REJECTS — the inverse of
    /// [`Expect::DeaconStricter`]: an intentional ahead-of-spec capability
    /// (e.g. deacon resolves `extends` at merged-config time; the reference
    /// leaves the chain unresolved and errors on the missing `image`). Optional
    /// `signal` lists informational stderr substrings (not part of the pass/fail
    /// decision), mirroring `DeaconStricter`.
    ReferenceStricter {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signal: Option<Vec<String>>,
    },
    /// A specific normalized-value difference is expected between the two CLIs.
    FieldDivergence { ours: Value, reference: Value },
}

impl Expect {
    /// Whether this expectation characterizes a divergence (deacon-stricter /
    /// reference-stricter / field-divergence) rather than an agreement
    /// (both-reject / both-accept).
    pub fn is_divergence(&self) -> bool {
        matches!(
            self,
            Expect::DeaconStricter { .. }
                | Expect::ReferenceStricter { .. }
                | Expect::FieldDivergence { .. }
        )
    }
}

/// One waiver record. `config` is a schema-known optional field describing case
/// input (an explicit `--config` argument) carried over from the legacy
/// `expect.json` shape; it plays no part in waiver semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Waiver {
    pub id: String,
    pub scope: Scope,
    pub expect: Expect,
    pub rationale: String,
    pub added: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
}

/// Match an observable-state `field` against a waiver `pattern`. Matchers are
/// EXACT by default; a trailing `*` makes it a prefix match. Exact-by-default
/// matters for path-like fields where one destination is a string prefix of
/// another — e.g. `mount:/workspace` must NOT match `mount:/workspaces/sib`.
pub fn field_matches(field: &str, pattern: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => field.starts_with(prefix),
        None => field == pattern,
    }
}

/// A validated collection of waiver records: schema-valid, non-empty rationales,
/// globally-unique ids.
#[derive(Debug, Clone, Default)]
pub struct WaiverSet {
    records: Vec<Waiver>,
    by_id: HashMap<String, usize>,
}

impl WaiverSet {
    /// Load and validate every waiver record under `corpus_root`:
    /// `errors/*/expect.json` (corpus-case) and `waivers/*.json`.
    ///
    /// A malformed record, an empty rationale, a mismatched errors-case scope, or
    /// a duplicate id is a hard [`HarnessError::WaiverInvalid`].
    pub fn load(corpus_root: &Path) -> Result<WaiverSet, HarnessError> {
        let mut records = Vec::new();

        // errors/<case>/expect.json — corpus-case scope, one per error case.
        let errors_dir = corpus_root.join("errors");
        if errors_dir.is_dir() {
            for entry in read_dir_sorted(&errors_dir)? {
                if !entry.is_dir() {
                    continue;
                }
                let spec = entry.join("expect.json");
                if !spec.is_file() {
                    continue;
                }
                let case_name = entry
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                let waiver = parse_file(&spec)?;
                // Consistency: an errors-dir record MUST be corpus-case-scoped to
                // exactly this corpus/case (a mismatch is a mislabeled record).
                match &waiver.scope {
                    Scope::CorpusCase { corpus, case }
                        if corpus == "errors" && *case == case_name => {}
                    other => {
                        return Err(HarnessError::WaiverInvalid {
                            path: spec,
                            cause: format!(
                                "errors record scope must be corpus_case corpus=\"errors\" \
                                 case=\"{case_name}\", got {other:?}"
                            ),
                        });
                    }
                }
                records.push((spec, waiver));
            }
        }

        // waivers/*.json — state-field (or additional corpus-case) records.
        let waivers_dir = corpus_root.join("waivers");
        if waivers_dir.is_dir() {
            for entry in read_dir_sorted(&waivers_dir)? {
                if entry.is_dir() {
                    continue;
                }
                if entry.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let waiver = parse_file(&entry)?;
                records.push((entry, waiver));
            }
        }

        Self::from_records(records)
    }

    /// Build a validated set from `(source_path, record)` pairs: non-empty
    /// rationale and globally-unique id.
    fn from_records(pairs: Vec<(PathBuf, Waiver)>) -> Result<WaiverSet, HarnessError> {
        let mut records = Vec::with_capacity(pairs.len());
        let mut by_id: HashMap<String, usize> = HashMap::new();
        for (path, waiver) in pairs {
            if waiver.rationale.trim().is_empty() {
                return Err(HarnessError::WaiverInvalid {
                    path,
                    cause: format!("waiver `{}` has an empty rationale", waiver.id),
                });
            }
            if waiver.id.trim().is_empty() {
                return Err(HarnessError::WaiverInvalid {
                    path,
                    cause: "waiver has an empty id".to_string(),
                });
            }
            let idx = records.len();
            if by_id.insert(waiver.id.clone(), idx).is_some() {
                return Err(HarnessError::WaiverInvalid {
                    path,
                    cause: format!("duplicate waiver id `{}`", waiver.id),
                });
            }
            records.push(waiver);
        }
        Ok(WaiverSet { records, by_id })
    }

    /// All loaded records.
    pub fn records(&self) -> &[Waiver] {
        &self.records
    }

    /// Look up a record by id.
    pub fn get(&self, id: &str) -> Option<&Waiver> {
        self.by_id.get(id).map(|&i| &self.records[i])
    }

    /// The single corpus-case waiver for `(corpus, case)`, if any.
    pub fn corpus_case(&self, corpus: &str, case: &str) -> Option<&Waiver> {
        self.records.iter().find(|w| {
            matches!(&w.scope, Scope::CorpusCase { corpus: c, case: k } if c == corpus && k == case)
        })
    }

    /// Every corpus-case waiver for `corpus`.
    pub fn corpus_cases(&self, corpus: &str) -> Vec<&Waiver> {
        self.records
            .iter()
            .filter(|w| matches!(&w.scope, Scope::CorpusCase { corpus: c, .. } if c == corpus))
            .collect()
    }

    /// Every state-field waiver for `binary`.
    pub fn state_field_waivers(&self, binary: &str) -> Vec<&Waiver> {
        self.records
            .iter()
            .filter(|w| matches!(&w.scope, Scope::StateField { binary: b, .. } if b == binary))
            .collect()
    }

    /// Given the ids consumed this run and a scope predicate, return the ids of
    /// loaded records that match the predicate but were NOT consumed — i.e. stale
    /// waivers (FR-011). The caller turns a non-empty result into a run failure
    /// naming each id (see [`HarnessError::WaiverStale`]).
    pub fn stale_among<F>(&self, in_scope: F, consumed: &HashSet<String>) -> Vec<String>
    where
        F: Fn(&Waiver) -> bool,
    {
        self.records
            .iter()
            .filter(|w| in_scope(w) && !consumed.contains(&w.id))
            .map(|w| w.id.clone())
            .collect()
    }
}

/// Read a directory's entries, sorted by path for deterministic iteration. A
/// missing/unreadable directory is a [`HarnessError::FixtureMissing`].
fn read_dir_sorted(dir: &Path) -> Result<Vec<PathBuf>, HarnessError> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|_| HarnessError::FixtureMissing {
            path: dir.to_path_buf(),
        })?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    Ok(entries)
}

/// Parse and schema-validate one waiver record file.
fn parse_file(path: &Path) -> Result<Waiver, HarnessError> {
    let raw = std::fs::read_to_string(path).map_err(|e| HarnessError::WaiverInvalid {
        path: path.to_path_buf(),
        cause: format!("could not read waiver record: {e}"),
    })?;
    parse(&raw).map_err(|cause| HarnessError::WaiverInvalid {
        path: path.to_path_buf(),
        cause,
    })
}

/// Parse a waiver record from JSON text (exposed for unit tests). Unknown fields
/// are rejected.
pub fn parse(raw: &str) -> Result<Waiver, String> {
    serde_json::from_str(raw).map_err(|e| format!("malformed waiver record: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_corpus_record() -> &'static str {
        r#"{
          "id": "errors/extends-missing",
          "scope": { "kind": "corpus_case", "corpus": "errors", "case": "extends-missing" },
          "expect": { "kind": "deacon-stricter", "signal": ["extends"] },
          "rationale": "characterized divergence",
          "added": "2026-07-19"
        }"#
    }

    #[test]
    fn parses_corpus_case_deacon_stricter() {
        let w = parse(valid_corpus_record()).expect("valid record parses");
        assert_eq!(w.id, "errors/extends-missing");
        assert!(matches!(w.scope, Scope::CorpusCase { .. }));
        match &w.expect {
            Expect::DeaconStricter { signal } => {
                assert_eq!(signal.as_deref(), Some(["extends".to_string()].as_slice()));
            }
            other => panic!("expected deacon-stricter, got {other:?}"),
        }
        assert!(w.expect.is_divergence());
    }

    fn valid_reference_stricter_record() -> &'static str {
        r#"{
          "id": "extends-child-merged",
          "scope": { "kind": "corpus_case", "corpus": "tier1", "case": "extends-child" },
          "expect": { "kind": "reference-stricter", "signal": ["image"] },
          "rationale": "deacon resolves extends at merged-config time; the reference errors",
          "added": "2026-07-19"
        }"#
    }

    #[test]
    fn parses_corpus_case_reference_stricter() {
        let w = parse(valid_reference_stricter_record()).expect("valid record parses");
        assert_eq!(w.id, "extends-child-merged");
        assert!(matches!(w.scope, Scope::CorpusCase { .. }));
        match &w.expect {
            Expect::ReferenceStricter { signal } => {
                assert_eq!(signal.as_deref(), Some(["image".to_string()].as_slice()));
            }
            other => panic!("expected reference-stricter, got {other:?}"),
        }
        assert!(w.expect.is_divergence());
    }

    #[test]
    fn reference_stricter_signal_is_optional() {
        // `signal` may be omitted entirely (mirrors `DeaconStricter`).
        let raw = r#"{
          "id": "x",
          "scope": { "kind": "corpus_case", "corpus": "tier1", "case": "extends-child" },
          "expect": { "kind": "reference-stricter" },
          "rationale": "r", "added": "d"
        }"#;
        let w = parse(raw).expect("record without signal parses");
        match &w.expect {
            Expect::ReferenceStricter { signal } => assert!(signal.is_none()),
            other => panic!("expected reference-stricter, got {other:?}"),
        }
    }

    #[test]
    fn reference_stricter_rejects_unknown_nested_field() {
        let raw = r#"{
          "id": "x",
          "scope": { "kind": "corpus_case", "corpus": "tier1", "case": "extends-child" },
          "expect": { "kind": "reference-stricter", "oops": 1 },
          "rationale": "r", "added": "d"
        }"#;
        assert!(
            parse(raw).is_err(),
            "unknown nested reference-stricter field must be rejected"
        );
    }

    #[test]
    fn reference_stricter_lookup_and_staleness() {
        let w: Waiver = serde_json::from_str(valid_reference_stricter_record()).unwrap();
        let set = WaiverSet::from_records(vec![(PathBuf::from("a.json"), w)]).unwrap();
        assert!(set.corpus_case("tier1", "extends-child").is_some());
        assert!(set.corpus_case("tier1", "nope").is_none());

        // Not consumed → stale (case gone or divergence no longer observed).
        let none = HashSet::new();
        let stale = set.stale_among(
            |w| matches!(&w.scope, Scope::CorpusCase { corpus, .. } if corpus == "tier1"),
            &none,
        );
        assert_eq!(stale, vec!["extends-child-merged".to_string()]);

        // Consumed → not stale.
        let mut consumed = HashSet::new();
        consumed.insert("extends-child-merged".to_string());
        let stale = set.stale_among(
            |w| matches!(&w.scope, Scope::CorpusCase { corpus, .. } if corpus == "tier1"),
            &consumed,
        );
        assert!(stale.is_empty());
    }

    #[test]
    fn loads_real_reference_stricter_waiver() {
        // The extends-child-merged fixture waiver must load, validate, and be
        // discoverable as a tier1 corpus-case record against the live repository.
        let root = crate::workspace_root().join("fixtures/parity-corpus");
        let set = WaiverSet::load(&root).expect("real corpus must load");
        let w = set
            .corpus_case("tier1", "extends-child")
            .expect("extends-child-merged waiver must be present");
        assert_eq!(w.id, "extends-child-merged");
        assert!(matches!(w.expect, Expect::ReferenceStricter { .. }));
    }

    #[test]
    fn parses_state_field_field_divergence() {
        let raw = r#"{
          "id": "state/compose-project-label",
          "scope": { "kind": "state_field", "binary": "parity_observable_state",
                     "fixture": "compose-postgres", "field": "label:com.docker.compose.project*" },
          "expect": { "kind": "field-divergence", "ours": "deacon-x", "reference": "devcontainer-y" },
          "rationale": "compose project naming is CLI-namespaced by design",
          "added": "2026-07-19"
        }"#;
        let w = parse(raw).expect("valid state-field record parses");
        match &w.scope {
            Scope::StateField { field, .. } => assert!(field.ends_with('*')),
            other => panic!("expected state_field, got {other:?}"),
        }
        match &w.expect {
            Expect::FieldDivergence { ours, reference } => {
                assert_eq!(ours, &json!("deacon-x"));
                assert_eq!(reference, &json!("devcontainer-y"));
            }
            other => panic!("expected field-divergence, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let raw = r#"{
          "id": "x", "scope": { "kind": "corpus_case", "corpus": "errors", "case": "c" },
          "expect": { "kind": "both-reject" }, "rationale": "r", "added": "d", "typo": 1
        }"#;
        assert!(
            parse(raw).is_err(),
            "unknown top-level field must be rejected"
        );
    }

    #[test]
    fn rejects_unknown_nested_field_in_scope_and_expect() {
        let bad_scope = r#"{
          "id": "x", "scope": { "kind": "corpus_case", "corpus": "errors", "case": "c", "oops": 1 },
          "expect": { "kind": "both-reject" }, "rationale": "r", "added": "d"
        }"#;
        assert!(
            parse(bad_scope).is_err(),
            "unknown nested scope field must be rejected"
        );
        let bad_expect = r#"{
          "id": "x", "scope": { "kind": "corpus_case", "corpus": "errors", "case": "c" },
          "expect": { "kind": "both-reject", "oops": 1 }, "rationale": "r", "added": "d"
        }"#;
        assert!(
            parse(bad_expect).is_err(),
            "unknown nested expect field must be rejected"
        );
    }

    #[test]
    fn field_matches_exact_and_prefix() {
        assert!(field_matches("mount:/workspace", "mount:/workspace"));
        assert!(!field_matches("mount:/workspaces/sib", "mount:/workspace"));
        assert!(field_matches(
            "label:com.docker.compose.project",
            "label:com.docker.*"
        ));
        assert!(!field_matches("label:other", "label:com.docker.*"));
    }

    #[test]
    fn duplicate_ids_rejected() {
        let a: Waiver = serde_json::from_str(valid_corpus_record()).unwrap();
        let b = a.clone();
        let err = WaiverSet::from_records(vec![
            (PathBuf::from("a.json"), a),
            (PathBuf::from("b.json"), b),
        ])
        .expect_err("duplicate ids must be rejected");
        assert!(matches!(err, HarnessError::WaiverInvalid { .. }));
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn empty_rationale_rejected() {
        let mut w: Waiver = serde_json::from_str(valid_corpus_record()).unwrap();
        w.rationale = "   ".to_string();
        let err = WaiverSet::from_records(vec![(PathBuf::from("a.json"), w)])
            .expect_err("empty rationale must be rejected");
        assert!(matches!(err, HarnessError::WaiverInvalid { .. }));
    }

    #[test]
    fn lookup_and_staleness() {
        let a: Waiver = serde_json::from_str(valid_corpus_record()).unwrap();
        let set = WaiverSet::from_records(vec![(PathBuf::from("a.json"), a)]).unwrap();
        assert!(set.corpus_case("errors", "extends-missing").is_some());
        assert!(set.corpus_case("errors", "nope").is_none());
        assert_eq!(set.corpus_cases("errors").len(), 1);

        // Not consumed → stale.
        let none = HashSet::new();
        let stale = set.stale_among(|w| matches!(w.scope, Scope::CorpusCase { .. }), &none);
        assert_eq!(stale, vec!["errors/extends-missing".to_string()]);

        // Consumed → not stale.
        let mut consumed = HashSet::new();
        consumed.insert("errors/extends-missing".to_string());
        let stale = set.stale_among(|w| matches!(w.scope, Scope::CorpusCase { .. }), &consumed);
        assert!(stale.is_empty());
    }

    #[test]
    fn loads_real_errors_corpus() {
        // The 9 backfilled errors records must all load, validate, and be
        // uniquely-ided against the live repository corpus.
        let root = crate::workspace_root().join("fixtures/parity-corpus");
        let set = WaiverSet::load(&root).expect("real errors corpus must load");
        assert!(
            set.corpus_cases("errors").len() >= 9,
            "expected >= 9 errors waivers, got {}",
            set.corpus_cases("errors").len()
        );
        assert!(set.corpus_case("errors", "extends-missing").is_some());
        assert!(set.corpus_case("errors", "duplicate-keys").is_some());
    }
}
