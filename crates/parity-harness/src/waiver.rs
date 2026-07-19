//! Waiver query layer over the repository-owned conformance registry
//! (019-conformance-registry, research Decision 3; FR-027, FR-028).
//!
//! There is exactly ONE waiver record schema and ONE loader for every
//! characterized divergence in the parity surface, and both now live in
//! `deacon-conformance`: the record types ([`Scope`], [`Expect`], [`Waiver`]) are
//! re-exported from `deacon_conformance::model`, and the on-disk records are read
//! through `deacon_conformance::load::load_waiver_files`. This module is a thin
//! query wrapper — [`WaiverSet`] with the stable API the nine live parity binaries
//! and `parity_corpus_errors` already consume (`load`, `records`, `get`,
//! `corpus_case`, `corpus_cases`, `state_field_waivers`, `stale_among`).
//!
//! Records live under the conformance registry's `waivers/` directory
//! (`conformance/registry/waivers/wvr-*.json`), one JSON object per file. The
//! legacy parity locations (`fixtures/parity-corpus/waivers/` and
//! `fixtures/parity-corpus/errors/*/expect.json`) were migrated into the registry
//! and removed; `parity_registry_check` enforces their absence structurally.
//!
//! Every loaded record is schema-validated by the conformance loader
//! (`deny_unknown_fields`, mandatory `expires`); this wrapper additionally enforces
//! a non-empty `rationale` and a globally-unique `id` so a typo fails loudly at
//! load time, not silently. **Staleness** (FR-011): each run, a loaded record must
//! match an existing case/field AND its expected difference must actually be
//! observed; a record that is loaded but never consumed is stale and fails the run
//! naming its `id` ([`WaiverSet::stale_among`]).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::HarnessError;

// The single waiver record schema lives in the conformance registry crate; the
// parity harness consumes those exact types so there is no second schema to drift
// (research Decision 3). Callers that `use parity_harness::waiver::{Scope, Expect,
// Waiver}` keep working unchanged through these re-exports.
pub use deacon_conformance::model::{Expect, Scope, Waiver};

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
    /// Load and validate every waiver record under `registry_root/waivers/`
    /// (`conformance/registry/waivers/wvr-*.json`) through the single conformance
    /// loader. A missing `waivers/` directory yields an empty set.
    ///
    /// A malformed record (schema error), an empty rationale, or a duplicate id is
    /// a hard [`HarnessError::WaiverInvalid`]. The path argument is the REGISTRY
    /// root (e.g. [`crate::conformance_registry_root`]); the fault-injection suite
    /// passes a temp directory whose `waivers/` subdirectory holds its fixtures.
    pub fn load(registry_root: &Path) -> Result<WaiverSet, HarnessError> {
        let waivers_dir = registry_root.join("waivers");
        let pairs = deacon_conformance::load::load_waiver_files(&waivers_dir).map_err(|e| {
            HarnessError::WaiverInvalid {
                path: waivers_dir,
                cause: e.to_string(),
            }
        })?;
        Self::from_records(pairs)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A schema-valid registry waiver (corpus-case, deacon-stricter), mirroring the
    /// migrated `wvr-extends-missing.json` shape (behaviors + expires present).
    fn valid_corpus_record() -> &'static str {
        r#"{
          "id": "wvr-extends-missing",
          "behaviors": ["bhv-readconfig-extends-missing-rejected"],
          "scope": { "kind": "corpus_case", "corpus": "errors", "case": "extends-missing" },
          "expect": { "kind": "deacon-stricter", "signal": ["extends"] },
          "rationale": "characterized divergence",
          "added": "2026-07-19",
          "expires": "2027-01-19"
        }"#
    }

    fn parse(raw: &str) -> Waiver {
        serde_json::from_str(raw).expect("valid waiver record parses")
    }

    #[test]
    fn parses_corpus_case_deacon_stricter() {
        let w = parse(valid_corpus_record());
        assert_eq!(w.id, "wvr-extends-missing");
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
          "id": "wvr-extends-child-merged",
          "behaviors": ["bhv-readconfig-extends-merged"],
          "scope": { "kind": "corpus_case", "corpus": "tier1", "case": "extends-child" },
          "expect": { "kind": "reference-stricter", "signal": ["image"] },
          "rationale": "deacon resolves extends at merged-config time; the reference errors",
          "added": "2026-07-19",
          "expires": "2027-01-19"
        }"#
    }

    #[test]
    fn parses_corpus_case_reference_stricter() {
        let w = parse(valid_reference_stricter_record());
        assert_eq!(w.id, "wvr-extends-child-merged");
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
    fn reference_stricter_lookup_and_staleness() {
        let w = parse(valid_reference_stricter_record());
        let set = WaiverSet::from_records(vec![(PathBuf::from("a.json"), w)]).unwrap();
        assert!(set.corpus_case("tier1", "extends-child").is_some());
        assert!(set.corpus_case("tier1", "nope").is_none());

        // Not consumed → stale (case gone or divergence no longer observed).
        let none = HashSet::new();
        let stale = set.stale_among(
            |w| matches!(&w.scope, Scope::CorpusCase { corpus, .. } if corpus == "tier1"),
            &none,
        );
        assert_eq!(stale, vec!["wvr-extends-child-merged".to_string()]);

        // Consumed → not stale.
        let mut consumed = HashSet::new();
        consumed.insert("wvr-extends-child-merged".to_string());
        let stale = set.stale_among(
            |w| matches!(&w.scope, Scope::CorpusCase { corpus, .. } if corpus == "tier1"),
            &consumed,
        );
        assert!(stale.is_empty());
    }

    #[test]
    fn loads_real_registry_waivers() {
        // The migrated registry waivers (9 errors + 1 tier1) must load, validate,
        // and be discoverable against the live repository registry.
        let set = WaiverSet::load(&crate::conformance_registry_root())
            .expect("real registry waivers must load");
        assert!(
            set.corpus_cases("errors").len() >= 9,
            "expected >= 9 errors waivers, got {}",
            set.corpus_cases("errors").len()
        );
        assert!(set.corpus_case("errors", "extends-missing").is_some());
        assert!(set.corpus_case("errors", "duplicate-keys").is_some());

        let child = set
            .corpus_case("tier1", "extends-child")
            .expect("extends-child waiver must be present");
        assert_eq!(child.id, "wvr-extends-child-merged");
        assert!(matches!(child.expect, Expect::ReferenceStricter { .. }));
    }

    #[test]
    fn field_divergence_and_state_field_scope() {
        let raw = r#"{
          "id": "wvr-compose-project-label",
          "behaviors": ["bhv-compose-project-name"],
          "scope": { "kind": "state_field", "binary": "parity_state_diff",
                     "fixture": "compose-postgres", "field": "label:com.docker.compose.project*" },
          "expect": { "kind": "field-divergence", "ours": "deacon-x", "reference": "devcontainer-y" },
          "rationale": "compose project naming is CLI-namespaced by design",
          "added": "2026-07-19", "expires": "2027-01-19"
        }"#;
        let w = parse(raw);
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
        let set = WaiverSet::from_records(vec![(PathBuf::from("a.json"), w)]).unwrap();
        assert_eq!(set.state_field_waivers("parity_state_diff").len(), 1);
        assert!(set.state_field_waivers("other_binary").is_empty());
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
        let a = parse(valid_corpus_record());
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
        let mut w = parse(valid_corpus_record());
        w.rationale = "   ".to_string();
        let err = WaiverSet::from_records(vec![(PathBuf::from("a.json"), w)])
            .expect_err("empty rationale must be rejected");
        assert!(matches!(err, HarnessError::WaiverInvalid { .. }));
    }

    #[test]
    fn lookup_and_staleness() {
        let a = parse(valid_corpus_record());
        let set = WaiverSet::from_records(vec![(PathBuf::from("a.json"), a)]).unwrap();
        assert!(set.corpus_case("errors", "extends-missing").is_some());
        assert!(set.corpus_case("errors", "nope").is_none());
        assert_eq!(set.corpus_cases("errors").len(), 1);
        assert!(set.get("wvr-extends-missing").is_some());

        // Not consumed → stale.
        let none = HashSet::new();
        let stale = set.stale_among(|w| matches!(w.scope, Scope::CorpusCase { .. }), &none);
        assert_eq!(stale, vec!["wvr-extends-missing".to_string()]);

        // Consumed → not stale.
        let mut consumed = HashSet::new();
        consumed.insert("wvr-extends-missing".to_string());
        let stale = set.stale_among(|w| matches!(w.scope, Scope::CorpusCase { .. }), &consumed);
        assert!(stale.is_empty());
    }
}
