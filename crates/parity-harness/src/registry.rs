//! Parity registry checks (research D5; FR-022, FR-024).
//!
//! The registry *data model* (`ParityRegistry` and friends) and the *production*
//! corpus discovery functions now live in `deacon-conformance::parity_corpus`, so the
//! hermetic baseline enumerator can call exactly the same discovery the live runners
//! execute without a dependency cycle (023-migrate-parity-to-conformance, research
//! D1/D6). They are re-exported here unchanged, so every existing
//! `parity_harness::registry::…` caller is unaffected.
//!
//! What stays on this side of the seam is the *checking* concern that needs harness
//! types: the bidirectional file↔registry match, the `.config/nextest.toml`
//! `[profile.*]` cross-check, and the corpus minimum gate expressed as a
//! [`HarnessError`]. These are free functions (an inherent `impl` may only live in
//! the crate that defines the type).

use std::path::{Path, PathBuf};

use crate::HarnessError;

pub use crate::parity_corpus::{
    Corpus, DiscoveryBinary, DiscoveryRole, LiveBinary, LiveKind, ParityCorpusError,
    ParityRegistry, REGISTRY_JSON,
};

/// Hermetic harness self-test binaries that intentionally carry the `parity_`
/// name prefix but are NOT oracle-comparing live binaries: they must never appear
/// in `live_binaries` nor be selected by `[profile.parity]`. Their source files
/// are expected under `crates/deacon/tests/` and are recognized by
/// [`check_test_files`] so the file↔registry match does not flag them as
/// "unregistered live binaries" (research D5, D10; FR-013).
pub const META_TEST_BINARIES: &[&str] = &["parity_harness_faults"];

/// Map a discovery/registry failure onto the harness's cause-specific vocabulary, so
/// a missing corpus still reads as `FixtureMissing` and a short corpus as
/// `CorpusTooSmall` at every call site.
fn map_corpus_error(e: ParityCorpusError) -> HarnessError {
    match e {
        ParityCorpusError::FixtureMissing { path } => HarnessError::FixtureMissing { path },
        ParityCorpusError::CorpusTooSmall { corpus, found, min } => {
            HarnessError::CorpusTooSmall { corpus, found, min }
        }
    }
}

/// Discover tier1 corpus case directories — the single production definition, shared
/// with the baseline enumerator (see [`crate::parity_corpus::discover_tier1_cases`]).
pub fn discover_tier1_cases(root: &Path) -> Result<Vec<PathBuf>, HarnessError> {
    crate::parity_corpus::discover_tier1_cases(root).map_err(map_corpus_error)
}

/// Discover error corpus case directories — the single production definition, shared
/// with the baseline enumerator (see [`crate::parity_corpus::discover_error_cases`]).
pub fn discover_error_cases(errors_root: &Path) -> Result<Vec<PathBuf>, HarnessError> {
    crate::parity_corpus::discover_error_cases(errors_root).map_err(map_corpus_error)
}

/// Enforce a corpus's minimum case count (FR-024), reported as a [`HarnessError`].
pub fn check_corpus_min(
    registry: &ParityRegistry,
    corpus: &Corpus,
    discovered: usize,
) -> Result<(), HarnessError> {
    registry
        .check_corpus_min(corpus, discovered)
        .map_err(map_corpus_error)
}

/// Bidirectional file↔registry match for `parity_*` sources under `tests_dir`
/// plus existence of the internal-consistency `consistency_*` sources. Returns
/// human-readable problems (empty = OK). Consumed by `parity_registry_check`.
pub fn check_test_files(registry: &ParityRegistry, tests_dir: &Path) -> Vec<String> {
    let mut problems = Vec::new();

    // Registry → file: every live binary has a source file.
    for name in registry.live_names() {
        if !tests_dir.join(format!("{name}.rs")).is_file() {
            problems.push(format!(
                "registered live binary `{name}` has no source file {name}.rs"
            ));
        }
    }
    // Registry → file: every internal-consistency binary has a source file.
    for name in &registry.internal_consistency_binaries {
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
    let live: std::collections::HashSet<&str> = registry.live_names().into_iter().collect();
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
///
/// Every verdict is reached by **evaluating** the expression with [`filter_selects`],
/// not by token-matching it. The two differ the moment the filter names a binary in
/// order to EXCLUDE it — which the parity filter now does for the discovery campaign
/// binaries (025 T007), and which any future exclusion would do again. Token matching
/// would read `… & not (binary(=discovery_campaign))` as "the parity profile selects
/// `discovery_campaign`" and fail a correct file, so the check has to understand the
/// operator it is reading. [`extract_binary_eq_tokens`] still supplies the *candidate*
/// set — the names worth asking about — because there is no other way to discover a
/// binary the filter mentions but the registry does not know.
pub fn check_parity_profile_filter(registry: &ParityRegistry, filter_expr: &str) -> Vec<String> {
    let mut problems = Vec::new();

    let selects = |name: &str, problems: &mut Vec<String>| -> Option<bool> {
        match filter_selects(filter_expr, name) {
            Ok(v) => Some(v),
            Err(e) => {
                problems.push(format!(
                    "[profile.parity] default-filter could not be evaluated for `{name}`: {e}"
                ));
                None
            }
        }
    };

    for name in registry.live_names() {
        if selects(name, &mut problems) == Some(false) {
            problems.push(format!(
                "[profile.parity] filter does not select live binary `{name}`"
            ));
        }
    }
    for name in &registry.internal_consistency_binaries {
        if selects(name, &mut problems) == Some(true) {
            problems.push(format!(
                "[profile.parity] filter selects internal-consistency binary `{name}` (it must not)"
            ));
        }
    }

    let live: std::collections::HashSet<&str> = registry.live_names().into_iter().collect();
    let mentioned: std::collections::BTreeSet<String> =
        extract_binary_eq_tokens(filter_expr).into_iter().collect();
    for name in &mentioned {
        if !live.contains(name.as_str()) && selects(name, &mut problems) == Some(true) {
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
///   internal-consistency binaries (delegates to [`check_parity_profile_filter`], which
///   EVALUATES the expression — the parity filter is an allow-list with an explicit
///   exclusion group appended, not a pure `binary(=…)` union, so a token-matching check
///   would misread the exclusion as a selection);
/// - NO OTHER profile's `default-filter` selects any live parity binary — the
///   truthful-by-non-selection invariant (FR-014). This is evaluated by
///   [`filter_selects`] over each profile's filter expression, so an exclusion
///   written as `not (…)` or `binary(#parity_*) & not (…)` is honored exactly,
///   not merely token-matched.
///
/// Returns human-readable problems (empty = OK).
pub fn check_nextest_profiles(
    registry: &ParityRegistry,
    profiles: &NextestProfiles,
) -> Vec<String> {
    let mut problems = Vec::new();

    match profiles.default_filters.get("parity") {
        Some(Some(filter)) => problems.extend(check_parity_profile_filter(registry, filter)),
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
        for live in registry.live_names() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_binary_tokens_works() {
        let expr = "binary(=a) | binary(=b) | binary(#glob_*)";
        assert_eq!(extract_binary_eq_tokens(expr), vec!["a", "b"]);
    }

    #[test]
    fn corpus_min_gate_maps_to_harness_error() {
        // The corpora retired with the corpus binaries (023 US7), so this exercises the
        // MAPPING from the shared gate onto the harness's cause-specific vocabulary,
        // which is what this crate owns.
        let reg = ParityRegistry::parse(
            r#"{"live_binaries":[],"internal_consistency_binaries":[],
                "corpora":[{"id":"probe","path":"fixtures/probe","min_cases":20}]}"#,
        )
        .expect("synthetic registry parses");
        let probe = reg.corpus("probe").expect("declared");
        assert!(check_corpus_min(&reg, probe, 20).is_ok());
        let err = check_corpus_min(&reg, probe, 19).expect_err("below min fails");
        assert!(matches!(err, HarnessError::CorpusTooSmall { .. }));
    }

    #[test]
    fn discovery_is_the_shared_production_definition() {
        // The re-exported wrappers must return exactly what the shared definition
        // returns, and map its failures onto the harness's vocabulary.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("alpha").join(".devcontainer"))
            .expect("create case");

        let via_wrapper = discover_tier1_cases(dir.path()).expect("tier1 discovery");
        let via_shared =
            crate::parity_corpus::discover_tier1_cases(dir.path()).expect("shared tier1 discovery");
        assert_eq!(via_wrapper, via_shared);

        let missing = discover_tier1_cases(Path::new("/definitely/not/a/corpus"))
            .expect_err("a missing corpus root fails loud");
        assert!(matches!(missing, HarnessError::FixtureMissing { .. }));
    }

    #[test]
    fn profile_filter_cross_check() {
        let reg = ParityRegistry::load().expect("embedded registry must parse");
        let good = reg
            .live_names()
            .iter()
            .map(|n| format!("binary(={n})"))
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(
            check_parity_profile_filter(&reg, &good).is_empty(),
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
        assert!(!check_parity_profile_filter(&reg, &missing).is_empty());

        // Selecting a consistency binary → flagged.
        let with_consistency = format!("{good} | binary(=consistency_env_probe_flag)");
        let problems = check_parity_profile_filter(&reg, &with_consistency);
        assert!(
            problems
                .iter()
                .any(|p| p.contains("consistency_env_probe_flag"))
        );
    }

    #[test]
    fn naming_a_binary_in_order_to_exclude_it_is_not_selecting_it() {
        // 025 T007 puts the discovery campaign binaries in the parity filter's `not`
        // group, so the parity lane's non-selection of them is structural rather than
        // incidental. A token-matching check would read that as "the parity profile
        // selects `discovery_campaign`" and fail a correct file — the check has to
        // understand the operator it is reading.
        let reg = ParityRegistry::load().expect("embedded registry must parse");
        let good = reg
            .live_names()
            .iter()
            .map(|n| format!("binary(={n})"))
            .collect::<Vec<_>>()
            .join(" | ");

        let excluding = format!(
            "({good}) & not (binary(=discovery_campaign) | binary(=discovery_metamorphic))"
        );
        assert!(
            check_parity_profile_filter(&reg, &excluding).is_empty(),
            "an explicit exclusion must not read as a selection: {:?}",
            check_parity_profile_filter(&reg, &excluding)
        );

        // Positively selecting an unregistered binary is still flagged — the exclusion
        // tolerance must not become a blanket amnesty for unknown names.
        let selecting = format!("{good} | binary(=discovery_campaign)");
        let problems = check_parity_profile_filter(&reg, &selecting);
        assert!(
            problems.iter().any(|p| p.contains("discovery_campaign")),
            "got: {problems:?}"
        );

        // An exclusion must not hide a MISSING live binary either.
        let missing_live = format!("({good}) & not (binary(={}))", reg.live_names()[0]);
        let problems = check_parity_profile_filter(&reg, &missing_live);
        assert!(
            problems
                .iter()
                .any(|p| p.contains("does not select live binary")),
            "got: {problems:?}"
        );
    }

    #[test]
    fn check_test_files_against_real_tree() {
        let reg = ParityRegistry::load().expect("embedded registry must parse");
        let tests_dir = crate::workspace_root().join("crates/deacon/tests");
        // The bidirectional match against the real tree must be clean: every
        // registered live/consistency binary and every hermetic meta-test binary
        // exists, and every `parity_*.rs` file is either registered or a recognized
        // meta-test binary.
        let problems = check_test_files(&reg, &tests_dir);
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
        let reg = ParityRegistry::load().expect("embedded registry must parse");
        let toml_text =
            std::fs::read_to_string(crate::workspace_root().join(".config/nextest.toml"))
                .expect("read nextest.toml");
        let profiles = parse_nextest_profiles(&toml_text).expect("parse nextest.toml");
        assert!(
            profiles.default_filters.contains_key("parity"),
            "the real nextest.toml must declare [profile.parity]"
        );
        let problems = check_nextest_profiles(&reg, &profiles);
        assert!(
            problems.is_empty(),
            "nextest.toml profile cross-check problems: {problems:?}"
        );
    }

    #[test]
    fn check_nextest_profiles_flags_leaked_live_binary() {
        let reg = ParityRegistry::load().expect("embedded registry must parse");
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
        // A rogue profile that positively selects a live binary must be flagged. The
        // binary is read from the registry rather than named literally, so retiring a
        // carrier cannot leave this test asserting against a binary that no longer exists
        // — which is exactly how it broke when the five legacy carriers were deleted.
        let leaked = reg
            .live_names()
            .first()
            .expect("the registry declares at least one live binary")
            .to_string();
        profiles
            .default_filters
            .insert("rogue".to_string(), Some(format!("binary(={leaked})")));
        let problems = check_nextest_profiles(&reg, &profiles);
        assert!(
            problems
                .iter()
                .any(|p| p.contains("rogue") && p.contains(&leaked as &str)),
            "a leaked live binary in another profile must be flagged, got: {problems:?}"
        );
    }
}
