//! Parity registry loading + completeness checks (research D5; FR-022, FR-024).
//!
//! `fixtures/parity-corpus/registry.json` is the authoritative enumeration of
//! claimed parity coverage: every oracle-comparing live binary, the reclassified
//! internal-consistency binaries (listed so a check can assert they never re-enter
//! the parity profile), and every case corpus with its minimum expected case
//! count. It is embedded at compile time via `include_str!` so a malformed
//! registry fails loudly, and is also read as data by CI/the aggregator.
//!
//! This module provides the loader plus the pure validation helpers the
//! (US5) `parity_registry_check` binary and the (US3) aggregator consume:
//! bidirectional file↔registry match, the `[profile.parity]` filter cross-check,
//! corpus discovery, and the corpus minimum-count gate.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::HarnessError;

/// The compile-time-embedded registry. A malformed registry is a hard failure the
/// moment any parity check loads it.
pub const REGISTRY_JSON: &str = include_str!("../../../fixtures/parity-corpus/registry.json");

/// Hermetic harness self-test binaries that intentionally carry the `parity_`
/// name prefix but are NOT oracle-comparing live binaries: they must never appear
/// in `live_binaries` nor be selected by `[profile.parity]`. Their source files
/// are expected under `crates/deacon/tests/` and are recognized by
/// [`ParityRegistry::check_test_files`] so the file↔registry match does not flag
/// them as "unregistered live binaries" (research D5, D10; FR-013).
pub const META_TEST_BINARIES: &[&str] = &["parity_harness_faults", "parity_registry_check"];

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

/// A case corpus with its minimum expected case count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Corpus {
    pub id: String,
    /// Workspace-root-relative path to the corpus directory.
    pub path: String,
    pub min_cases: usize,
}

/// The authoritative coverage enumeration (data-model §3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParityRegistry {
    pub live_binaries: Vec<LiveBinary>,
    pub internal_consistency_binaries: Vec<String>,
    pub corpora: Vec<Corpus>,
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

    /// Parse an arbitrary registry document (exposed for unit tests). Unknown
    /// fields are rejected; internal consistency (corpus kinds, corpus refs, no
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
        Ok(())
    }

    /// Look up a corpus by id.
    pub fn corpus(&self, id: &str) -> Option<&Corpus> {
        self.corpora.iter().find(|c| c.id == id)
    }

    /// The names of every live binary.
    pub fn live_names(&self) -> Vec<&str> {
        self.live_binaries.iter().map(|b| b.name.as_str()).collect()
    }

    /// Bidirectional file↔registry match for `parity_*` sources under `tests_dir`
    /// plus existence of the internal-consistency `consistency_*` sources. Returns
    /// human-readable problems (empty = OK). Consumed by `parity_registry_check`.
    pub fn check_test_files(&self, tests_dir: &Path) -> Vec<String> {
        let mut problems = Vec::new();

        // Registry → file: every live binary has a source file.
        for name in self.live_names() {
            if !tests_dir.join(format!("{name}.rs")).is_file() {
                problems.push(format!(
                    "registered live binary `{name}` has no source file {name}.rs"
                ));
            }
        }
        // Registry → file: every internal-consistency binary has a source file.
        for name in &self.internal_consistency_binaries {
            if !tests_dir.join(format!("{name}.rs")).is_file() {
                problems.push(format!(
                    "registered internal-consistency binary `{name}` has no source file {name}.rs"
                ));
            }
        }
        // The hermetic harness self-test binaries must also exist (they are the
        // structural + fault-injection guard themselves).
        for name in META_TEST_BINARIES {
            if !tests_dir.join(format!("{name}.rs")).is_file() {
                problems.push(format!(
                    "hermetic meta-test binary `{name}` has no source file {name}.rs"
                ));
            }
        }

        // File → registry: every `parity_*.rs` source is a registered live binary
        // (or a recognized hermetic meta-test binary — those carry the `parity_`
        // prefix by design but are never live/oracle-comparing).
        let live: std::collections::HashSet<&str> = self.live_names().into_iter().collect();
        match std::fs::read_dir(tests_dir) {
            Ok(rd) => {
                for entry in rd.filter_map(Result::ok) {
                    let file = entry.file_name();
                    let file = file.to_string_lossy();
                    if let Some(stem) = file.strip_suffix(".rs") {
                        if stem.starts_with("parity_")
                            && !live.contains(stem)
                            && !META_TEST_BINARIES.contains(&stem)
                        {
                            problems.push(format!(
                                "source file {file} looks like a live parity binary but is not \
                                 registered in registry.json live_binaries"
                            ));
                        }
                    }
                }
            }
            Err(e) => problems.push(format!("could not read tests dir {tests_dir:?}: {e}")),
        }
        problems
    }

    /// Cross-check a nextest `[profile.parity]` default-filter expression: it must
    /// select EXACTLY the live binaries and NONE of the internal-consistency
    /// binaries (FR-013, FR-014). Returns problems (empty = OK).
    pub fn check_parity_profile_filter(&self, filter_expr: &str) -> Vec<String> {
        let mut problems = Vec::new();
        let selected: std::collections::HashSet<String> =
            extract_binary_eq_tokens(filter_expr).into_iter().collect();

        for name in self.live_names() {
            if !selected.contains(name) {
                problems.push(format!(
                    "[profile.parity] filter does not select live binary `{name}`"
                ));
            }
        }
        for name in &self.internal_consistency_binaries {
            if selected.contains(name.as_str()) {
                problems.push(format!(
                    "[profile.parity] filter selects internal-consistency binary `{name}` (it must not)"
                ));
            }
        }
        let live: std::collections::HashSet<&str> = self.live_names().into_iter().collect();
        for name in &selected {
            if !live.contains(name.as_str()) {
                problems.push(format!(
                    "[profile.parity] filter selects `{name}`, which is not a registered live binary"
                ));
            }
        }
        problems
    }

    /// Full `.config/nextest.toml` cross-check (research D5; FR-013, FR-014):
    ///
    /// - `[profile.parity]` selects EXACTLY the live set and none of the
    ///   internal-consistency binaries (delegates to
    ///   [`Self::check_parity_profile_filter`], valid because the parity profile's
    ///   `default-filter` is a pure `binary(=…)` allow-list);
    /// - NO OTHER profile's `default-filter` selects any live parity binary — the
    ///   truthful-by-non-selection invariant (FR-014). This is evaluated by
    ///   [`filter_selects`] over each profile's filter expression, so an exclusion
    ///   written as `not (…)` or `binary(#parity_*) & not (…)` is honored exactly,
    ///   not merely token-matched.
    ///
    /// Returns human-readable problems (empty = OK).
    pub fn check_nextest_profiles(&self, profiles: &NextestProfiles) -> Vec<String> {
        let mut problems = Vec::new();

        match profiles.default_filters.get("parity") {
            Some(Some(filter)) => problems.extend(self.check_parity_profile_filter(filter)),
            Some(None) => problems.push(
                "[profile.parity] has no default-filter; it must select exactly the live \
                 parity binaries"
                    .to_string(),
            ),
            None => problems
                .push("nextest.toml has no [profile.parity] — live parity has no lane".to_string()),
        }

        for (name, filter) in &profiles.default_filters {
            if name == "parity" {
                continue;
            }
            let Some(expr) = filter else {
                problems.push(format!(
                    "[profile.{name}] has no default-filter, so it selects every binary \
                     including the live parity binaries (only [profile.parity] may)"
                ));
                continue;
            };
            for live in self.live_names() {
                match filter_selects(expr, live) {
                    Ok(true) => problems.push(format!(
                        "[profile.{name}] selects live parity binary `{live}` — only \
                         [profile.parity] may select live parity binaries (FR-014)"
                    )),
                    Ok(false) => {}
                    Err(e) => problems.push(format!(
                        "[profile.{name}] default-filter could not be evaluated for `{live}`: {e}"
                    )),
                }
            }
        }
        problems
    }

    /// Enforce a corpus's minimum case count. `discovered` is the number of cases
    /// found by the corpus's discovery rule; below the minimum is a
    /// [`HarnessError::CorpusTooSmall`] (FR-024).
    pub fn check_corpus_min(&self, corpus: &Corpus, discovered: usize) -> Result<(), HarnessError> {
        if discovered < corpus.min_cases {
            return Err(HarnessError::CorpusTooSmall {
                corpus: corpus.id.clone(),
                found: discovered,
                min: corpus.min_cases,
            });
        }
        Ok(())
    }
}

/// Extract each `binary(=NAME)` token from a nextest filter expression.
fn extract_binary_eq_tokens(expr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let needle = "binary(=";
    let mut rest = expr;
    while let Some(pos) = rest.find(needle) {
        rest = &rest[pos + needle.len()..];
        if let Some(end) = rest.find(')') {
            out.push(rest[..end].trim().to_string());
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    out
}

/// The parsed subset of `.config/nextest.toml` the registry check needs: each
/// `[profile.<name>]`'s `default-filter` expression (absent = selects all).
#[derive(Debug, Clone, Default)]
pub struct NextestProfiles {
    /// profile name → its `default-filter` (`None` when the profile omits one).
    pub default_filters: std::collections::BTreeMap<String, Option<String>>,
}

/// Parse the `[profile.*]` `default-filter` expressions from nextest.toml text via
/// the `toml` crate (no hand-copied literals — the real file is the source of
/// truth). Every other key (overrides, groups, timeouts) is ignored.
pub fn parse_nextest_profiles(toml_text: &str) -> Result<NextestProfiles, String> {
    #[derive(serde::Deserialize)]
    struct Root {
        #[serde(default)]
        profile: std::collections::BTreeMap<String, Prof>,
    }
    #[derive(serde::Deserialize)]
    struct Prof {
        #[serde(default, rename = "default-filter")]
        default_filter: Option<String>,
    }
    let root: Root =
        toml::from_str(toml_text).map_err(|e| format!("malformed nextest.toml: {e}"))?;
    Ok(NextestProfiles {
        default_filters: root
            .profile
            .into_iter()
            .map(|(k, v)| (k, v.default_filter))
            .collect(),
    })
}

/// Evaluate whether `binary` is selected by a nextest `default-filter` expression,
/// for the subset of the filterset grammar used in this repo's nextest.toml:
/// `binary(=NAME)`, `binary(#GLOB)`, other single-predicate matchers (`test(...)`,
/// `kind(...)`, `platform(...)`, `package(...)` — treated as non-selecting for a
/// *specific binary* question, since none of them can single out a live parity
/// binary in this file), `not`, `&`, `|`, and parentheses. `not` binds tighter
/// than `&`, which binds tighter than `|`. An unrecognized construct is an `Err`
/// (fail loud — never silently mis-evaluate a truthfulness invariant).
pub fn filter_selects(expr: &str, binary: &str) -> Result<bool, String> {
    let mut p = FilterEval {
        bytes: expr.as_bytes(),
        i: 0,
        binary,
    };
    let v = p.parse_or()?;
    p.skip_ws();
    if p.i != p.bytes.len() {
        return Err(format!(
            "trailing tokens after position {} in filter {expr:?}",
            p.i
        ));
    }
    Ok(v)
}

struct FilterEval<'a> {
    bytes: &'a [u8],
    i: usize,
    binary: &'a str,
}

impl FilterEval<'_> {
    fn skip_ws(&mut self) {
        while self.i < self.bytes.len() && (self.bytes[self.i] as char).is_whitespace() {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.bytes.get(self.i).copied()
    }

    fn parse_or(&mut self) -> Result<bool, String> {
        let mut v = self.parse_and()?;
        while let Some(b'|') = self.peek() {
            self.i += 1;
            let r = self.parse_and()?;
            v = v || r;
        }
        Ok(v)
    }

    fn parse_and(&mut self) -> Result<bool, String> {
        let mut v = self.parse_unary()?;
        while let Some(b'&') = self.peek() {
            self.i += 1;
            let r = self.parse_unary()?;
            v = v && r;
        }
        Ok(v)
    }

    fn parse_unary(&mut self) -> Result<bool, String> {
        match self.peek() {
            Some(b'(') => {
                self.i += 1;
                let v = self.parse_or()?;
                self.expect(b')')?;
                Ok(v)
            }
            Some(c) if c.is_ascii_alphabetic() => {
                let ident = self.read_ident();
                if ident == "not" {
                    Ok(!self.parse_unary()?)
                } else {
                    self.parse_predicate(&ident)
                }
            }
            other => Err(format!(
                "unexpected token {:?} at position {}",
                other.map(|c| c as char),
                self.i
            )),
        }
    }

    fn read_ident(&mut self) -> String {
        self.skip_ws();
        let start = self.i;
        while self.i < self.bytes.len()
            && (self.bytes[self.i].is_ascii_alphanumeric() || self.bytes[self.i] == b'_')
        {
            self.i += 1;
        }
        String::from_utf8_lossy(&self.bytes[start..self.i]).into_owned()
    }

    fn parse_predicate(&mut self, ident: &str) -> Result<bool, String> {
        self.expect(b'(')?;
        let arg = self.read_predicate_arg()?;
        match ident {
            "binary" => match_binary(&arg, self.binary),
            // These predicates never single out a live parity binary in this file
            // (they match test names / build kinds / platforms). For the
            // "is this binary selected" question they contribute no selection.
            "test" | "kind" | "platform" | "package" | "rdeps" | "deps" => Ok(false),
            other => Err(format!("unsupported filterset predicate `{other}(…)`")),
        }
    }

    /// Read a predicate's argument up to its matching `)`, honoring nested parens.
    fn read_predicate_arg(&mut self) -> Result<String, String> {
        let start = self.i;
        let mut depth = 1usize;
        while self.i < self.bytes.len() {
            match self.bytes[self.i] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        let arg = String::from_utf8_lossy(&self.bytes[start..self.i]).into_owned();
                        self.i += 1; // consume ')'
                        return Ok(arg);
                    }
                }
                _ => {}
            }
            self.i += 1;
        }
        Err("unterminated predicate argument".to_string())
    }

    fn expect(&mut self, b: u8) -> Result<(), String> {
        match self.peek() {
            Some(c) if c == b => {
                self.i += 1;
                Ok(())
            }
            other => Err(format!(
                "expected {:?} at position {}, found {:?}",
                b as char,
                self.i,
                other.map(|c| c as char)
            )),
        }
    }
}

/// Match a `binary(…)` argument against a specific binary name. Only the exact
/// (`=name`) and glob (`#glob`) matchers appear in this repo's nextest.toml; any
/// other matcher form is an error rather than a silent mismatch.
fn match_binary(arg: &str, binary: &str) -> Result<bool, String> {
    let arg = arg.trim();
    if let Some(name) = arg.strip_prefix('=') {
        Ok(name.trim() == binary)
    } else if let Some(glob) = arg.strip_prefix('#') {
        Ok(glob_match(glob.trim(), binary))
    } else {
        Err(format!(
            "unsupported binary() matcher {arg:?} (expected `=name` or `#glob`)"
        ))
    }
}

/// Minimal glob match supporting `*` (any run, including empty) and `?` (one
/// char). Sufficient for the prefix globs (`parity_*`, `smoke_*`, …) in use.
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    // Iterative backtracking glob matcher.
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut star_ti): (Option<usize>, usize) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            star_ti = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Discover tier1 corpus case directories: IMMEDIATE subdirectories of `root`
/// containing a `.devcontainer/` directory, excluding `errors`, `waivers`,
/// `__pycache__`, and any dot-directory. (`errors/*` cases also contain
/// `.devcontainer/` but belong only to the errors runner; they are never reached
/// because only immediate children are scanned and `errors` itself is excluded.)
pub fn discover_tier1_cases(root: &Path) -> Result<Vec<PathBuf>, HarnessError> {
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

/// Discover error corpus cases: IMMEDIATE subdirectories of `errors_root`
/// containing an `expect.json`.
pub fn discover_error_cases(errors_root: &Path) -> Result<Vec<PathBuf>, HarnessError> {
    let mut out = Vec::new();
    for path in immediate_subdirs(errors_root)? {
        if path.join("expect.json").is_file() {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// The immediate subdirectories of `dir`. A missing directory is a
/// [`HarnessError::FixtureMissing`].
fn immediate_subdirs(dir: &Path) -> Result<Vec<PathBuf>, HarnessError> {
    let rd = std::fs::read_dir(dir).map_err(|_| HarnessError::FixtureMissing {
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
    fn embedded_registry_parses_and_matches_expected() {
        let reg = ParityRegistry::load().expect("embedded registry must parse");
        assert_eq!(reg.live_binaries.len(), 9, "6 scenario + 3 corpus");
        assert_eq!(reg.internal_consistency_binaries.len(), 2);
        assert!(reg.corpus("tier1").is_some());
        assert!(reg.corpus("errors").is_some());
        assert_eq!(reg.corpus("tier1").unwrap().min_cases, 20);
        assert_eq!(reg.corpus("errors").unwrap().min_cases, 9);
        // Corpus binaries carry their corpus id.
        let tier1 = reg
            .live_binaries
            .iter()
            .find(|b| b.name == "parity_corpus_tier1")
            .unwrap();
        assert_eq!(tier1.kind, LiveKind::Corpus);
        assert_eq!(tier1.corpus.as_deref(), Some("tier1"));
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
    fn rejects_overlapping_live_and_consistency() {
        let bad = r#"{
          "live_binaries": [ { "name": "dup", "kind": "scenario", "docker_required": false } ],
          "internal_consistency_binaries": [ "dup" ],
          "corpora": []
        }"#;
        assert!(ParityRegistry::parse(bad).is_err());
    }

    #[test]
    fn extract_binary_tokens_works() {
        let expr = "binary(=a) | binary(=b) | binary(#glob_*)";
        assert_eq!(extract_binary_eq_tokens(expr), vec!["a", "b"]);
    }

    #[test]
    fn profile_filter_cross_check() {
        let reg = ParityRegistry::load().unwrap();
        let good = reg
            .live_names()
            .iter()
            .map(|n| format!("binary(={n})"))
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(
            reg.check_parity_profile_filter(&good).is_empty(),
            "a filter selecting exactly the live set has no problems"
        );

        // Missing one live binary → flagged.
        let missing = reg
            .live_names()
            .iter()
            .skip(1)
            .map(|n| format!("binary(={n})"))
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(!reg.check_parity_profile_filter(&missing).is_empty());

        // Selecting a consistency binary → flagged.
        let with_consistency = format!("{good} | binary(=consistency_env_probe_flag)");
        let problems = reg.check_parity_profile_filter(&with_consistency);
        assert!(
            problems
                .iter()
                .any(|p| p.contains("consistency_env_probe_flag"))
        );
    }

    #[test]
    fn corpus_min_gate() {
        let reg = ParityRegistry::load().unwrap();
        let tier1 = reg.corpus("tier1").unwrap();
        assert!(reg.check_corpus_min(tier1, 20).is_ok());
        assert!(reg.check_corpus_min(tier1, 23).is_ok());
        let err = reg
            .check_corpus_min(tier1, 19)
            .expect_err("below min fails");
        assert!(matches!(err, HarnessError::CorpusTooSmall { .. }));
    }

    #[test]
    fn discovers_real_corpus_cases() {
        let root = crate::workspace_root().join("fixtures/parity-corpus");
        let tier1 = discover_tier1_cases(&root).expect("tier1 discovery");
        assert!(
            tier1.len() >= 20,
            "expected >= 20 tier1 cases, got {}: {:?}",
            tier1.len(),
            tier1
        );
        // errors/ and dot-dirs are excluded from tier1 discovery.
        assert!(!tier1.iter().any(|p| p.ends_with("errors")));
        assert!(!tier1.iter().any(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|n| n.starts_with('.'))
        }));

        let errors = discover_error_cases(&root.join("errors")).expect("errors discovery");
        assert!(
            errors.len() >= 9,
            "expected >= 9 error cases, got {}",
            errors.len()
        );
    }

    #[test]
    fn check_test_files_against_real_tree() {
        let reg = ParityRegistry::load().unwrap();
        let tests_dir = crate::workspace_root().join("crates/deacon/tests");
        // With US5 landed, the bidirectional match against the real tree must be
        // clean: every registered live/consistency binary and every hermetic
        // meta-test binary exists, and every `parity_*.rs` file is either
        // registered or a recognized meta-test binary.
        let problems = reg.check_test_files(&tests_dir);
        assert!(problems.is_empty(), "registry↔tests mismatch: {problems:?}");
    }

    #[test]
    fn glob_match_prefix_and_wildcards() {
        assert!(glob_match("parity_*", "parity_exec"));
        assert!(glob_match("parity_*", "parity_"));
        assert!(!glob_match("parity_*", "consistency_x"));
        assert!(glob_match("*", "anything"));
        assert!(glob_match(
            "integration_up_*",
            "integration_up_build_options"
        ));
        assert!(!glob_match("integration_up_*", "integration_env_probe_a"));
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "ac"));
    }

    #[test]
    fn filter_selects_grammar() {
        // Exact and glob.
        assert!(filter_selects("binary(=parity_exec)", "parity_exec").unwrap());
        assert!(!filter_selects("binary(=parity_exec)", "parity_build").unwrap());
        assert!(filter_selects("binary(#parity_*)", "parity_exec").unwrap());

        // not / & / | precedence: `not a & b` == `(not a) & b`.
        assert!(
            !filter_selects(
                "not binary(=parity_exec) & binary(#parity_*)",
                "parity_exec"
            )
            .unwrap()
        );
        assert!(
            filter_selects(
                "not binary(=parity_exec) & binary(#parity_*)",
                "parity_build"
            )
            .unwrap()
        );

        // The real exclusion forms used in nextest.toml.
        let excl = "not (binary(=parity_exec) | binary(=parity_build))";
        assert!(!filter_selects(excl, "parity_exec").unwrap());
        assert!(filter_selects(excl, "something_else").unwrap());

        // docker/mvp form: parity glob minus the 9 named excludes a live binary.
        let docker = "binary(#smoke_*) | (binary(#parity_*) & not (binary(=parity_exec))) | binary(#integration_*)";
        assert!(!filter_selects(docker, "parity_exec").unwrap());
        assert!(filter_selects(docker, "parity_harness_faults").unwrap());

        // test()/kind() predicates never select a specific binary here.
        assert!(!filter_selects("test(/^env_probe::tests::/)", "parity_exec").unwrap());

        // Unsupported predicate fails loud.
        assert!(filter_selects("mystery(=x)", "parity_exec").is_err());
    }

    #[test]
    fn parses_and_checks_the_real_nextest_toml() {
        let reg = ParityRegistry::load().unwrap();
        let toml_text =
            std::fs::read_to_string(crate::workspace_root().join(".config/nextest.toml"))
                .expect("read nextest.toml");
        let profiles = parse_nextest_profiles(&toml_text).expect("parse nextest.toml");
        assert!(
            profiles.default_filters.contains_key("parity"),
            "the real nextest.toml must declare [profile.parity]"
        );
        let problems = reg.check_nextest_profiles(&profiles);
        assert!(
            problems.is_empty(),
            "nextest.toml profile cross-check problems: {problems:?}"
        );
    }

    #[test]
    fn check_nextest_profiles_flags_leaked_live_binary() {
        let reg = ParityRegistry::load().unwrap();
        let parity_filter = reg
            .live_names()
            .iter()
            .map(|n| format!("binary(={n})"))
            .collect::<Vec<_>>()
            .join(" | ");
        let mut profiles = NextestProfiles::default();
        profiles
            .default_filters
            .insert("parity".to_string(), Some(parity_filter));
        // A rogue profile that positively selects a live binary must be flagged.
        profiles.default_filters.insert(
            "rogue".to_string(),
            Some("binary(=parity_exec)".to_string()),
        );
        let problems = reg.check_nextest_profiles(&profiles);
        assert!(
            problems
                .iter()
                .any(|p| p.contains("rogue") && p.contains("parity_exec")),
            "a leaked live binary in another profile must be flagged, got: {problems:?}"
        );
    }
}
