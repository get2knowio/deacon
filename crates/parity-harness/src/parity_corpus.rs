//! The parity-corpus registry model and the **production** corpus discovery
//! functions (023-migrate-parity-to-conformance, research D1).
//!
//! `fixtures/parity-corpus/registry.json` is the authoritative enumeration of claimed
//! pre-migration parity coverage: every oracle-comparing live binary, the
//! internal-consistency binaries, and every case corpus with its minimum expected
//! case count. It is embedded at compile time via `include_str!` so a malformed
//! registry fails loudly the moment anything loads it.
//!
//! # Why this lives in `deacon-conformance` and not in `parity-harness`
//!
//! The baseline enumerator (`baseline generate`) is hermetic tooling and therefore
//! lives in this crate (research D6), but it MUST derive corpus units by calling the
//! *same* discovery functions the live runners execute — never an independent
//! directory walk. Re-walking is precisely how the Tier-1 corpus was once counted as
//! 25 cases when discovery only ever selected 24 (research D1). Since
//! `parity-harness` already depends on `deacon-conformance` (for the waiver loader),
//! the only non-circular shared location is here; `parity_harness::registry`
//! re-exports these items so every existing caller is unchanged, and keeps the
//! nextest-profile cross-checks (which are a *checking* concern, not a data-model
//! one) on its own side of the seam.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The compile-time-embedded parity registry. A malformed registry is a hard failure
/// the moment any check loads it.
pub const REGISTRY_JSON: &str = include_str!("../../../fixtures/parity-corpus/registry.json");

/// A discovery / registry failure. Cause-specific by construction — a caller never
/// has to guess whether the corpus was missing or merely small (constitution IV).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParityCorpusError {
    /// A required corpus/fixture directory was absent.
    #[error(
        "required parity fixture is missing: {path:?}. Remedy: restore the fixture or fix \
         the corpus path — discovery must never run against absent inputs."
    )]
    FixtureMissing { path: PathBuf },

    /// A corpus discovered fewer cases than its registered floor.
    #[error(
        "corpus `{corpus}` discovered {found} case(s), below its registered minimum of \
         {min}. Remedy: restore the missing case directories or lower `min_cases` \
         deliberately."
    )]
    CorpusTooSmall {
        corpus: String,
        found: usize,
        min: usize,
    },
}

/// Whether a live binary compares a single scenario or drives a case corpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LiveKind {
    Scenario,
    Corpus,
}

/// One live (oracle-comparing) parity binary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveBinary {
    pub name: String,
    pub kind: LiveKind,
    pub docker_required: bool,
    /// The corpus this binary drives (required iff `kind == Corpus`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corpus: Option<String>,
}

/// What a discovery test binary is *for*, which decides which lanes may select it
/// (025-exploratory-parity-discovery, T060; FR-055/FR-057).
///
/// The two roles have **opposite** selection requirements, which is exactly why the role
/// is recorded rather than inferred from the `discovery_` name prefix. Inferring from the
/// prefix is the `discovery_*` glob mistake research D9 exists to prevent, one level up:
/// it would make a hermetic guard and a stochastic campaign indistinguishable to every
/// check that reads this file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiscoveryRole {
    /// A stochastic campaign binary. Selected ONLY by `[profile.discovery]`; excluded
    /// from the `default-filter` of every pull-request profile, so a green PR run never
    /// implies a campaign ran.
    Live,
    /// A hermetic guard. It MUST run in the fast lane (`default` / `dev-fast`) and MUST
    /// NOT be captured by `[profile.discovery]`'s allow-list — a guard nobody runs is a
    /// guard nobody notices going stale.
    Guard,
}

impl DiscoveryRole {
    /// The wire spelling, for diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            DiscoveryRole::Live => "live",
            DiscoveryRole::Guard => "guard",
        }
    }
}

/// One discovery test binary and where its source lives.
///
/// `tests_dir` is explicit rather than assumed: the discovery binaries are split across
/// two crates on purpose (`discovery_cli` drives the `deacon-conformance` bin and so must
/// live in that crate's test tree, while the campaign binaries and the repository-wiring
/// guard live in `crates/deacon/tests`). A checker that assumed one directory would
/// silently stop covering whichever binary moved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryBinary {
    /// The nextest binary name (`<name>.rs` in `tests_dir`).
    pub name: String,
    /// Live campaign or hermetic guard.
    pub role: DiscoveryRole,
    /// Workspace-root-relative directory holding `<name>.rs`.
    pub tests_dir: String,
    /// Whether the binary needs a Docker daemon for any of its tiers.
    pub docker_required: bool,
}

/// A case corpus with its minimum expected case count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Corpus {
    pub id: String,
    /// Workspace-root-relative path to the corpus directory.
    pub path: String,
    pub min_cases: usize,
}

/// The authoritative pre-migration coverage enumeration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParityRegistry {
    pub live_binaries: Vec<LiveBinary>,
    pub internal_consistency_binaries: Vec<String>,
    pub corpora: Vec<Corpus>,
    /// The discovery lane's binaries (025-exploratory-parity-discovery, T060).
    ///
    /// `#[serde(default)]` so the many synthetic registries in unit tests stay terse;
    /// the *real* file's completeness is enforced structurally by
    /// `parity_registry_check`, which knows the expected set and fails loudly on an
    /// empty or partial one. Defaulting here therefore never hides a missing
    /// registration — it only keeps a data-model concern out of the test fixtures.
    #[serde(default)]
    pub discovery_binaries: Vec<DiscoveryBinary>,
}

/// The (symbolic) location of the embedded registry, used in error messages.
fn registry_path() -> PathBuf {
    crate::workspace_root().join("fixtures/parity-corpus/registry.json")
}

impl ParityRegistry {
    /// Load and validate the embedded registry.
    pub fn load() -> Result<ParityRegistry, String> {
        Self::parse(REGISTRY_JSON)
    }

    /// Parse an arbitrary registry document (exposed for unit tests). Unknown fields
    /// are rejected; internal consistency (corpus kinds, corpus refs, no
    /// duplicate/overlapping names) is validated.
    pub fn parse(raw: &str) -> Result<ParityRegistry, String> {
        let reg: ParityRegistry = serde_json::from_str(raw)
            .map_err(|e| format!("malformed registry {:?}: {e}", registry_path()))?;
        reg.validate_internal()?;
        Ok(reg)
    }

    /// Structural self-consistency: corpus binaries reference a declared corpus,
    /// scenario binaries do not carry a corpus, names are unique, and the live and
    /// internal-consistency name sets are disjoint.
    fn validate_internal(&self) -> Result<(), String> {
        let mut seen = std::collections::HashSet::new();
        for b in &self.live_binaries {
            if !seen.insert(b.name.as_str()) {
                return Err(format!("duplicate live binary `{}`", b.name));
            }
            match b.kind {
                LiveKind::Corpus => match &b.corpus {
                    Some(id) if self.corpus(id).is_some() => {}
                    Some(id) => {
                        return Err(format!(
                            "corpus binary `{}` references undeclared corpus `{id}`",
                            b.name
                        ));
                    }
                    None => {
                        return Err(format!("corpus binary `{}` has no `corpus`", b.name));
                    }
                },
                LiveKind::Scenario => {
                    if b.corpus.is_some() {
                        return Err(format!(
                            "scenario binary `{}` must not carry a `corpus`",
                            b.name
                        ));
                    }
                }
            }
        }
        for name in &self.internal_consistency_binaries {
            if seen.contains(name.as_str()) {
                return Err(format!(
                    "`{name}` is both a live and an internal-consistency binary"
                ));
            }
        }
        let mut corpus_ids = std::collections::HashSet::new();
        for c in &self.corpora {
            if !corpus_ids.insert(c.id.as_str()) {
                return Err(format!("duplicate corpus id `{}`", c.id));
            }
        }
        // Discovery names are unique and disjoint from the parity namespaces. The two
        // lanes have contradictory selection rules, so a name claimed by both would make
        // the truthfulness invariant unsatisfiable rather than merely ambiguous.
        let mut discovery_names = std::collections::HashSet::new();
        for d in &self.discovery_binaries {
            if !discovery_names.insert(d.name.as_str()) {
                return Err(format!("duplicate discovery binary `{}`", d.name));
            }
            if seen.contains(d.name.as_str()) {
                return Err(format!(
                    "`{}` is both a live parity binary and a discovery binary",
                    d.name
                ));
            }
            if self
                .internal_consistency_binaries
                .iter()
                .any(|n| n == &d.name)
            {
                return Err(format!(
                    "`{}` is both an internal-consistency binary and a discovery binary",
                    d.name
                ));
            }
            if d.tests_dir.trim().is_empty() {
                return Err(format!(
                    "discovery binary `{}` declares no tests_dir — a checker cannot find \
                     a source file it is not told where to look for",
                    d.name
                ));
            }
        }
        Ok(())
    }

    /// Every discovery binary of `role`.
    pub fn discovery_of_role(&self, role: DiscoveryRole) -> Vec<&DiscoveryBinary> {
        self.discovery_binaries
            .iter()
            .filter(|b| b.role == role)
            .collect()
    }

    /// The names of every registered discovery binary.
    pub fn discovery_names(&self) -> Vec<&str> {
        self.discovery_binaries
            .iter()
            .map(|b| b.name.as_str())
            .collect()
    }

    /// Look up a corpus by id.
    pub fn corpus(&self, id: &str) -> Option<&Corpus> {
        self.corpora.iter().find(|c| c.id == id)
    }

    /// Look up a live binary by name.
    pub fn live_binary(&self, name: &str) -> Option<&LiveBinary> {
        self.live_binaries.iter().find(|b| b.name == name)
    }

    /// The names of every live binary.
    pub fn live_names(&self) -> Vec<&str> {
        self.live_binaries.iter().map(|b| b.name.as_str()).collect()
    }

    /// Enforce a corpus's minimum case count. `discovered` is the number of cases
    /// found by the corpus's discovery rule; below the minimum is a
    /// [`ParityCorpusError::CorpusTooSmall`].
    pub fn check_corpus_min(
        &self,
        corpus: &Corpus,
        discovered: usize,
    ) -> Result<(), ParityCorpusError> {
        if discovered < corpus.min_cases {
            return Err(ParityCorpusError::CorpusTooSmall {
                corpus: corpus.id.clone(),
                found: discovered,
                min: corpus.min_cases,
            });
        }
        Ok(())
    }
}

/// Discover tier1 corpus case directories: IMMEDIATE subdirectories of `root`
/// containing a `.devcontainer/` directory, excluding `errors`, `waivers`,
/// `__pycache__`, and any dot-directory. (`errors/*` cases also contain
/// `.devcontainer/` but belong only to the errors runner; they are never reached
/// because only immediate children are scanned and `errors` itself is excluded.)
///
/// This is THE definition of a Tier-1 case: the live runners and the baseline
/// enumerator both call it, so the two can never disagree (research D1).
pub fn discover_tier1_cases(root: &Path) -> Result<Vec<PathBuf>, ParityCorpusError> {
    let mut out = Vec::new();
    for path in immediate_subdirs(root)? {
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if name.starts_with('.') || matches!(name, "errors" | "waivers" | "__pycache__") {
            continue;
        }
        if path.join(".devcontainer").is_dir() {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// Discover error corpus cases: IMMEDIATE subdirectories of `errors_root`,
/// excluding dot-directories.
///
/// Each case directory carries the test input (a `.devcontainer/`, or a `.gitkeep`
/// for the "no config" / "bad `--config` path" cases that deliberately have no
/// config). Its accept/reject expectation is a `corpus_case`-scoped `wvr-` record in
/// the conformance registry.
pub fn discover_error_cases(errors_root: &Path) -> Result<Vec<PathBuf>, ParityCorpusError> {
    let mut out = Vec::new();
    for path in immediate_subdirs(errors_root)? {
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if name.starts_with('.') || name == "__pycache__" {
            continue;
        }
        out.push(path);
    }
    out.sort();
    Ok(out)
}

/// The immediate subdirectories of `dir`. A missing directory is a
/// [`ParityCorpusError::FixtureMissing`].
fn immediate_subdirs(dir: &Path) -> Result<Vec<PathBuf>, ParityCorpusError> {
    let rd = std::fs::read_dir(dir).map_err(|_| ParityCorpusError::FixtureMissing {
        path: dir.to_path_buf(),
    })?;
    Ok(rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_registry_parses_and_matches_the_surviving_set() {
        // The three corpus binaries and `parity_read_configuration` were retired in
        // 023 US7 once the equivalence ledger cleared them, and their corpora went with
        // them — the registry now enumerates only the surviving live binaries.
        let reg = ParityRegistry::load().expect("embedded registry must parse");
        assert_eq!(
            reg.live_binaries.len(),
            7,
            "5 Docker scenario binaries + the two declarative runners (024 T015/T020 split \
             `parity_conformance_runner` into a config-only driver and the Docker-backed \
             `parity_conformance_docker`)"
        );
        assert!(
            reg.live_binaries
                .iter()
                .all(|b| b.kind == LiveKind::Scenario),
            "no corpus binary survives"
        );
        assert!(
            reg.corpora.is_empty(),
            "the corpora retired with the binaries that drove them"
        );
        assert_eq!(reg.internal_consistency_binaries.len(), 2);
    }

    #[test]
    fn rejects_unknown_field_and_bad_corpus_ref() {
        assert!(
            ParityRegistry::parse(
                r#"{"live_binaries":[],"internal_consistency_binaries":[],"corpora":[],"x":1}"#
            )
            .is_err()
        );
        let bad = r#"{
          "live_binaries": [ { "name": "parity_corpus_x", "kind": "corpus", "docker_required": false, "corpus": "ghost" } ],
          "internal_consistency_binaries": [],
          "corpora": []
        }"#;
        assert!(
            ParityRegistry::parse(bad).is_err(),
            "corpus binary referencing an undeclared corpus must be rejected"
        );
    }

    #[test]
    fn rejects_a_discovery_binary_that_collides_with_a_parity_namespace() {
        // The two lanes' selection rules are contradictory (a live parity binary must be
        // selected by [profile.parity]; a live discovery binary must not be selected by
        // it), so one name in both namespaces is unsatisfiable, not merely ambiguous.
        let with_live = r#"{
          "live_binaries": [ { "name": "dup", "kind": "scenario", "docker_required": false } ],
          "internal_consistency_binaries": [],
          "corpora": [],
          "discovery_binaries": [ { "name": "dup", "role": "live", "tests_dir": "crates/deacon/tests", "docker_required": false } ]
        }"#;
        assert!(ParityRegistry::parse(with_live).is_err());

        let with_consistency = r#"{
          "live_binaries": [],
          "internal_consistency_binaries": [ "dup" ],
          "corpora": [],
          "discovery_binaries": [ { "name": "dup", "role": "guard", "tests_dir": "crates/deacon/tests", "docker_required": false } ]
        }"#;
        assert!(ParityRegistry::parse(with_consistency).is_err());

        let duplicated = r#"{
          "live_binaries": [],
          "internal_consistency_binaries": [],
          "corpora": [],
          "discovery_binaries": [
            { "name": "d", "role": "live", "tests_dir": "crates/deacon/tests", "docker_required": false },
            { "name": "d", "role": "guard", "tests_dir": "crates/deacon/tests", "docker_required": false }
          ]
        }"#;
        assert!(
            ParityRegistry::parse(duplicated).is_err(),
            "one name may not carry two roles — the roles' lane requirements are opposites"
        );

        let no_dir = r#"{
          "live_binaries": [],
          "internal_consistency_binaries": [],
          "corpora": [],
          "discovery_binaries": [ { "name": "d", "role": "live", "tests_dir": "  ", "docker_required": false } ]
        }"#;
        assert!(ParityRegistry::parse(no_dir).is_err());
    }

    #[test]
    fn rejects_overlapping_live_and_consistency() {
        let bad = r#"{
          "live_binaries": [ { "name": "dup", "kind": "scenario", "docker_required": false } ],
          "internal_consistency_binaries": [ "dup" ],
          "corpora": []
        }"#;
        assert!(ParityRegistry::parse(bad).is_err());
    }

    /// Build a synthetic corpus tree: `<root>/<case>/.devcontainer/` per name, plus the
    /// non-case siblings discovery must skip.
    fn synthetic_corpus(names: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        for name in names {
            std::fs::create_dir_all(dir.path().join(name).join(".devcontainer"))
                .expect("create case");
        }
        for skipped in ["errors", "waivers", "__pycache__", ".hidden"] {
            std::fs::create_dir_all(dir.path().join(skipped).join(".devcontainer"))
                .expect("create non-case");
        }
        // A directory with no `.devcontainer/` is not a case either.
        std::fs::create_dir_all(dir.path().join("not-a-case")).expect("create bare dir");
        dir
    }

    #[test]
    fn corpus_min_gate() {
        let reg = ParityRegistry::parse(
            r#"{"live_binaries":[],"internal_consistency_binaries":[],
                "corpora":[{"id":"probe","path":"fixtures/probe","min_cases":20}]}"#,
        )
        .expect("synthetic registry parses");
        let probe = reg.corpus("probe").expect("declared");
        assert!(reg.check_corpus_min(probe, 20).is_ok());
        assert!(reg.check_corpus_min(probe, 23).is_ok());
        let err = reg
            .check_corpus_min(probe, 19)
            .expect_err("below min fails");
        assert!(matches!(err, ParityCorpusError::CorpusTooSmall { .. }));
    }

    #[test]
    fn discovery_selects_only_devcontainer_bearing_case_dirs() {
        // The rule research D1 corrected: a case is an immediate subdirectory holding a
        // `.devcontainer/`, excluding the sibling `errors` corpus, `waivers`,
        // `__pycache__` and dot-directories. `ls -d */` counts the excluded ones, which
        // is how 24 Tier-1 cases were once reported as 25.
        let corpus = synthetic_corpus(&["alpha", "beta", "gamma"]);
        let discovered = discover_tier1_cases(corpus.path()).expect("discovery");
        let names: Vec<&str> = discovered
            .iter()
            .filter_map(|p| p.file_name().and_then(|s| s.to_str()))
            .collect();
        assert_eq!(names, vec!["alpha", "beta", "gamma"]);

        // The error corpus keeps its own rule: every immediate subdirectory except
        // dot-dirs and `__pycache__`, whether or not it holds a `.devcontainer/`.
        let errors = discover_error_cases(corpus.path()).expect("discovery");
        let error_names: Vec<&str> = errors
            .iter()
            .filter_map(|p| p.file_name().and_then(|s| s.to_str()))
            .collect();
        assert!(error_names.contains(&"errors"));
        assert!(error_names.contains(&"not-a-case"));
        assert!(!error_names.contains(&".hidden"));
        assert!(!error_names.contains(&"__pycache__"));
    }

    #[test]
    fn missing_corpus_is_fixture_missing() {
        let err = discover_tier1_cases(Path::new("/definitely/not/a/corpus"))
            .expect_err("a missing corpus root fails loud");
        assert!(matches!(err, ParityCorpusError::FixtureMissing { .. }));
    }
}
