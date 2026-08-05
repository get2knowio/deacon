//! The nextest lane cross-check: which binaries may select live parity, and which
//! must not.
//!
//! This is the smallest surviving piece of what was once a registry data model with
//! its own JSON file, corpus enumerations and minimum-case gates. All of that
//! described binaries that no longer exist. What earned its keep is the *check*: a
//! bidirectional match between the `parity_*` sources on disk and the names declared
//! here, plus an evaluation of every `[profile.*]` `default-filter` in
//! `.config/nextest.toml`.
//!
//! It has caught two real defects that nothing else would have: an edit that silently
//! dropped the `default-filter` from four profiles (so `dev-fast` selected every
//! Docker binary), and dangling `binary(=…)` references left behind when the five
//! legacy carriers were retired.
//!
//! The invariant it defends is **truthfulness by non-selection**: live parity runs
//! ONLY under `[profile.parity]`, so a green fast run never implies parity ran. That
//! only holds if no other profile's filter selects a live binary — which is a claim
//! about a filterset expression, and therefore has to be *evaluated*, not
//! token-matched (see [`check_parity_profile_filter`]).

use std::path::Path;

/// The LIVE (oracle-comparing) parity binary. Exactly this must be selected by
/// `[profile.parity]` and by no other profile.
///
/// A plain const rather than a data file. There is one of them, and a JSON document with
/// its own schema, loader and validation tests was several hundred lines spent restating
/// this line.
pub const LIVE_BINARIES: &[&str] = &["parity_differential"];

/// Parity binaries that carry the `parity_` name prefix but are NOT oracle-comparing, so
/// they must never be treated as live and MUST run in the ordinary lanes.
///
/// Two kinds, both non-live for the same reason — they need no reference CLI:
/// `parity_hermetic` and `parity_docker` run cases whose expectation is pinned in the
/// record, and `parity_harness_faults` is the hermetic proof that the comparison can
/// fail at all. Recognized by [`check_test_files`] so the file↔name match does not flag
/// them as undeclared live binaries.
pub const META_TEST_BINARIES: &[&str] =
    &["parity_harness_faults", "parity_hermetic", "parity_docker"];

/// Bidirectional file↔name match for `parity_*` sources under `tests_dir`. Returns
/// human-readable problems (empty = OK).
pub fn check_test_files(tests_dir: &Path) -> Vec<String> {
    let mut problems = Vec::new();

    // Name → file: every declared binary has a source file.
    for name in LIVE_BINARIES.iter().chain(META_TEST_BINARIES) {
        if !tests_dir.join(format!("{name}.rs")).is_file() {
            problems.push(format!(
                "declared parity binary `{name}` has no source file {name}.rs"
            ));
        }
    }

    // File → name: every `parity_*.rs` source is a declared live binary (or a
    // recognized hermetic meta-test binary — those carry the `parity_` prefix by
    // design but are never oracle-comparing).
    match std::fs::read_dir(tests_dir) {
        Ok(rd) => {
            for entry in rd.filter_map(Result::ok) {
                let file = entry.file_name();
                let file = file.to_string_lossy();
                if let Some(stem) = file.strip_suffix(".rs") {
                    if stem.starts_with("parity_")
                        && !LIVE_BINARIES.contains(&stem)
                        && !META_TEST_BINARIES.contains(&stem)
                    {
                        problems.push(format!(
                            "source file {file} looks like a live parity binary but is not \
                             declared in registry::LIVE_BINARIES"
                        ));
                    }
                }
            }
        }
        Err(e) => problems.push(format!("could not read tests dir {tests_dir:?}: {e}")),
    }
    problems
}

/// Cross-check a nextest `[profile.parity]` default-filter expression: it must select
/// EXACTLY the live binaries and none of the hermetic meta-test binaries. Returns
/// problems (empty = OK).
///
/// Every verdict is reached by **evaluating** the expression with [`filter_selects`],
/// not by token-matching it. The two differ the moment the filter names a binary in
/// order to EXCLUDE it — which any exclusion group does. Token matching would read
/// `… & not (binary(=x))` as "the parity profile selects `x`" and fail a correct
/// file, so the check has to understand the operator it is reading.
/// [`extract_binary_eq_tokens`] still supplies the *candidate* set — the names worth
/// asking about — because there is no other way to discover a binary the filter
/// mentions but the declaration does not know.
pub fn check_parity_profile_filter(filter_expr: &str) -> Vec<String> {
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

    for name in LIVE_BINARIES {
        if selects(name, &mut problems) == Some(false) {
            problems.push(format!(
                "[profile.parity] filter does not select live binary `{name}`"
            ));
        }
    }
    for name in META_TEST_BINARIES {
        if selects(name, &mut problems) == Some(true) {
            problems.push(format!(
                "[profile.parity] filter selects hermetic meta-test binary `{name}` (it must not)"
            ));
        }
    }

    let mentioned: std::collections::BTreeSet<String> =
        extract_binary_eq_tokens(filter_expr).into_iter().collect();
    for name in &mentioned {
        if !LIVE_BINARIES.contains(&name.as_str()) && selects(name, &mut problems) == Some(true) {
            problems.push(format!(
                "[profile.parity] filter selects `{name}`, which is not a declared live binary"
            ));
        }
    }
    problems
}

/// Full `.config/nextest.toml` cross-check:
///
/// - `[profile.parity]` selects EXACTLY the live set (delegates to
///   [`check_parity_profile_filter`]);
/// - NO OTHER profile's `default-filter` selects any live parity binary — the
///   truthful-by-non-selection invariant. Evaluated by [`filter_selects`], so an
///   exclusion written as `not (…)` or `binary(#parity_*) & not (…)` is honored
///   exactly rather than merely token-matched.
///
/// A profile with NO `default-filter` selects everything, including the live
/// binaries, and is therefore a problem in its own right. That is the specific defect
/// this caught once already: an edit dropped the filter from four profiles, and
/// `tomllib` still reported those profiles as *present* because their
/// `[[…overrides]]` blocks survived. "The profile exists" is not the check.
///
/// Returns human-readable problems (empty = OK).
pub fn check_nextest_profiles(profiles: &NextestProfiles) -> Vec<String> {
    let mut problems = Vec::new();

    match profiles.default_filters.get("parity") {
        Some(Some(filter)) => problems.extend(check_parity_profile_filter(filter)),
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
        for live in LIVE_BINARIES {
            match filter_selects(expr, live) {
                Ok(true) => problems.push(format!(
                    "[profile.{name}] selects live parity binary `{live}` — only \
                     [profile.parity] may select live parity binaries"
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

/// The parsed subset of `.config/nextest.toml` this check needs: each
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

/// The five status labels a `parity/SPEC_STATUS.md` table row may carry, paired with the
/// phrase its Summary bullet uses for the same status. Row cells are matched by prefix so
/// a cell may elaborate after the label; bullets are matched by containment.
const SPEC_STATUS_LABELS: &[(&str, &str)] = &[
    ("open nonconformance", "open nonconformance"),
    ("matches the reference", "conformant and matching"),
    ("deacon follows the spec", "deacon follows the spec"),
    ("documented choice", "documented choice"),
    ("deacon extension", "deacon extension"),
];

/// Cross-check `parity/SPEC_STATUS.md`'s Summary block against a row census of its own
/// tables. Returns human-readable problems (empty = OK).
///
/// The Summary's counts are hand-maintained while rows land from independent PRs, and the
/// same-file auto-merge is silent — two PRs each adding a row and bumping the header by
/// one merge to a header short by one. That drift happened **five times** before this
/// check existed. The table is the truth; the header is derived, so it is checked the way
/// every other derived artifact here is: recomputed and compared.
///
/// Parsing is deliberately dumb — second `|`-delimited cell of each table row, matched by
/// prefix against the five known labels after stripping emphasis — because the check must
/// not invent a schema the hand-written document doesn't promise. A label this misses
/// simply doesn't count, and the total-line mismatch then flags it.
pub fn check_spec_status_census(markdown: &str) -> Vec<String> {
    let mut problems = Vec::new();

    let mut row_counts = vec![0usize; SPEC_STATUS_LABELS.len()];
    for line in markdown.lines() {
        let Some(rest) = line.trim_start().strip_prefix('|') else {
            continue;
        };
        let Some(cell) = rest.split('|').nth(1) else {
            continue;
        };
        let cell = cell.trim().trim_matches('*').trim();
        if let Some(idx) = SPEC_STATUS_LABELS
            .iter()
            .position(|(label, _)| cell.starts_with(label))
        {
            row_counts[idx] += 1;
        }
    }

    // `Of **115 recorded behaviors**:` — the emphasis wraps the number AND the phrase,
    // so take the leading digit run rather than splitting on the closing `**`.
    let header_total = markdown.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("Of **")?;
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        (digits > 0
            && rest[digits..]
                .trim_start()
                .starts_with("recorded behaviors"))
        .then(|| rest[..digits].parse::<usize>().ok())?
    });
    match header_total {
        None => problems.push(
            "SPEC_STATUS.md: no `Of **N recorded behaviors**` line found in the Summary"
                .to_string(),
        ),
        Some(total) => {
            let census: usize = row_counts.iter().sum();
            if total != census {
                problems.push(format!(
                    "SPEC_STATUS.md: Summary claims {total} recorded behaviors but the row \
                     census counts {census} — recount by rows, never header arithmetic"
                ));
            }
        }
    }

    for (idx, (label, bullet_phrase)) in SPEC_STATUS_LABELS.iter().enumerate() {
        let bullet = markdown.lines().find_map(|line| {
            let rest = line.trim().strip_prefix("- **")?;
            let (n, tail) = rest.split_once("**")?;
            tail.contains(bullet_phrase)
                .then(|| n.trim().parse::<usize>().ok())?
        });
        match bullet {
            None => problems.push(format!(
                "SPEC_STATUS.md: no Summary bullet found for status `{bullet_phrase}`"
            )),
            Some(n) if n != row_counts[idx] => problems.push(format!(
                "SPEC_STATUS.md: Summary claims {n} `{bullet_phrase}` but the row census \
                 counts {} rows with status `{label}`",
                row_counts[idx]
            )),
            Some(_) => {}
        }
    }

    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `[profile.parity]` filter that selects exactly the live set.
    fn exact_live_filter() -> String {
        LIVE_BINARIES
            .iter()
            .map(|n| format!("binary(={n})"))
            .collect::<Vec<_>>()
            .join(" | ")
    }

    /// The real ledger's header must equal its own row census. This is the guard for a
    /// drift that shipped five times: two PRs each add a row and bump the Summary by one,
    /// the silent same-file auto-merge keeps only one bump, and the header undercounts.
    #[test]
    fn spec_status_summary_matches_row_census() {
        let path = crate::workspace_root().join("parity/SPEC_STATUS.md");
        let markdown = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        let problems = check_spec_status_census(&markdown);
        assert!(problems.is_empty(), "{problems:#?}");
    }

    /// The census check can actually fail — on the header total, on a per-status bullet,
    /// and on a missing Summary line. A check that cannot fail proves nothing.
    #[test]
    fn spec_status_census_flags_drift() {
        let ledger = "\
Of **3 recorded behaviors**:

- **0** — open nonconformance
- **1** — deacon extension
- **2** — conformant and matching
- **0** — deacon follows the spec where the CLI does not
- **0** — documented choice

| behavior | status | scenarios | notes |
|---|---|---|---|
| a | matches the reference | 1 | x |
| b | matches the reference | 1 | x |
| c | deacon extension | 1 | x |
";
        assert!(check_spec_status_census(ledger).is_empty());

        // One more matching row than the header knows — the five-times drift.
        let drifted = ledger.replace("| c |", "| d | matches the reference | 1 | x |\n| c |");
        let problems = check_spec_status_census(&drifted);
        assert!(
            problems.iter().any(|p| p.contains("4 recorded")
                || p.contains("claims 3 recorded behaviors but the row census counts 4")),
            "total drift must be flagged: {problems:#?}"
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("conformant and matching")),
            "per-status drift must be flagged: {problems:#?}"
        );

        // No Summary at all.
        let headless = "| a | matches the reference | 1 | x |\n";
        assert!(
            check_spec_status_census(headless)
                .iter()
                .any(|p| p.contains("no `Of **N recorded behaviors**` line")),
            "a missing Summary must be flagged"
        );
    }

    #[test]
    fn extract_binary_tokens_works() {
        let expr = "binary(=a) | binary(=b) | binary(#glob_*)";
        assert_eq!(extract_binary_eq_tokens(expr), vec!["a", "b"]);
    }

    #[test]
    fn profile_filter_cross_check() {
        let good = exact_live_filter();
        assert!(
            check_parity_profile_filter(&good).is_empty(),
            "a filter selecting exactly the live set has no problems"
        );

        // Missing one live binary → flagged.
        let missing = LIVE_BINARIES
            .iter()
            .skip(1)
            .map(|n| format!("binary(={n})"))
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(!check_parity_profile_filter(&missing).is_empty());

        // Selecting a hermetic meta-test binary → flagged. It is hermetic by design and
        // must run in the ordinary lanes; pulling it into the parity lane would mean the
        // proof that the comparison can fail only runs when the oracle is installed.
        let with_meta = format!("{good} | binary(=parity_harness_faults)");
        let problems = check_parity_profile_filter(&with_meta);
        assert!(
            problems.iter().any(|p| p.contains("parity_harness_faults")),
            "got: {problems:?}"
        );
    }

    #[test]
    fn naming_a_binary_in_order_to_exclude_it_is_not_selecting_it() {
        // A token-matching check would read an exclusion group as a selection and fail a
        // correct file — the check has to understand the operator it is reading.
        let good = exact_live_filter();

        let excluding = format!("({good}) & not (binary(=some_other_binary))");
        assert!(
            check_parity_profile_filter(&excluding).is_empty(),
            "an explicit exclusion must not read as a selection: {:?}",
            check_parity_profile_filter(&excluding)
        );

        // Positively selecting an undeclared binary is still flagged — the exclusion
        // tolerance must not become a blanket amnesty for unknown names.
        let selecting = format!("{good} | binary(=some_other_binary)");
        let problems = check_parity_profile_filter(&selecting);
        assert!(
            problems.iter().any(|p| p.contains("some_other_binary")),
            "got: {problems:?}"
        );

        // An exclusion must not hide a MISSING live binary either.
        let missing_live = format!("({good}) & not (binary(={}))", LIVE_BINARIES[0]);
        let problems = check_parity_profile_filter(&missing_live);
        assert!(
            problems
                .iter()
                .any(|p| p.contains("does not select live binary")),
            "got: {problems:?}"
        );
    }

    #[test]
    fn check_test_files_against_real_tree() {
        let tests_dir = crate::workspace_root().join("crates/deacon/tests");
        // The bidirectional match against the real tree must be clean: every declared
        // binary exists, and every `parity_*.rs` file is either declared live or a
        // recognized meta-test binary.
        let problems = check_test_files(&tests_dir);
        assert!(
            problems.is_empty(),
            "declaration↔tests mismatch: {problems:?}"
        );
    }

    #[test]
    fn glob_match_prefix_and_wildcards() {
        assert!(glob_match("parity_*", "parity_differential"));
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
        assert!(filter_selects("binary(=parity_differential)", "parity_differential").unwrap());
        assert!(!filter_selects("binary(=parity_differential)", "parity_docker").unwrap());
        assert!(filter_selects("binary(#parity_*)", "parity_differential").unwrap());

        // not / & / | precedence: `not a & b` == `(not a) & b`.
        assert!(
            !filter_selects(
                "not binary(=parity_differential) & binary(#parity_*)",
                "parity_differential"
            )
            .unwrap()
        );
        assert!(
            filter_selects(
                "not binary(=parity_differential) & binary(#parity_*)",
                "parity_docker"
            )
            .unwrap()
        );

        // The real exclusion forms used in nextest.toml.
        let excl = "not (binary(=parity_differential))";
        assert!(!filter_selects(excl, "parity_differential").unwrap());
        assert!(filter_selects(excl, "something_else").unwrap());

        // docker/mvp form: the parity glob minus the named live binaries still selects
        // the hermetic meta-test binary, which is the point of the carve-out.
        let docker = "binary(#smoke_*) | (binary(#parity_*) & not (binary(=parity_differential))) | binary(#integration_*)";
        assert!(!filter_selects(docker, "parity_differential").unwrap());
        assert!(filter_selects(docker, "parity_harness_faults").unwrap());

        // test()/kind() predicates never select a specific binary here.
        assert!(!filter_selects("test(/^env_probe::tests::/)", "parity_differential").unwrap());

        // Unsupported predicate fails loud.
        assert!(filter_selects("mystery(=x)", "parity_differential").is_err());
    }

    #[test]
    fn parses_and_checks_the_real_nextest_toml() {
        let toml_text =
            std::fs::read_to_string(crate::workspace_root().join(".config/nextest.toml"))
                .expect("read nextest.toml");
        let profiles = parse_nextest_profiles(&toml_text).expect("parse nextest.toml");
        assert!(
            profiles.default_filters.contains_key("parity"),
            "the real nextest.toml must declare [profile.parity]"
        );
        let problems = check_nextest_profiles(&profiles);
        assert!(
            problems.is_empty(),
            "nextest.toml profile cross-check problems: {problems:?}"
        );
    }

    #[test]
    fn check_nextest_profiles_flags_leaked_live_binary() {
        let mut profiles = NextestProfiles::default();
        profiles
            .default_filters
            .insert("parity".to_string(), Some(exact_live_filter()));
        // A rogue profile that positively selects a live binary must be flagged. The
        // binary is read from the declaration rather than named literally, so retiring a
        // carrier cannot leave this test asserting against a binary that no longer exists
        // — which is exactly how it broke when the five legacy carriers were deleted.
        let leaked = LIVE_BINARIES[0];
        profiles
            .default_filters
            .insert("rogue".to_string(), Some(format!("binary(={leaked})")));
        let problems = check_nextest_profiles(&profiles);
        assert!(
            problems
                .iter()
                .any(|p| p.contains("rogue") && p.contains(leaked)),
            "a leaked live binary in another profile must be flagged, got: {problems:?}"
        );
    }

    #[test]
    fn a_profile_without_a_default_filter_is_a_problem() {
        // The specific defect this caught once: an edit dropped `default-filter` from four
        // profiles, so they selected every binary including live parity. A profile whose
        // `[[…overrides]]` survive still parses as present, so presence is not the check.
        let mut profiles = NextestProfiles::default();
        profiles
            .default_filters
            .insert("parity".to_string(), Some(exact_live_filter()));
        profiles
            .default_filters
            .insert("dev-fast".to_string(), None);
        let problems = check_nextest_profiles(&profiles);
        assert!(
            problems
                .iter()
                .any(|p| p.contains("dev-fast") && p.contains("no default-filter")),
            "got: {problems:?}"
        );
    }
}
