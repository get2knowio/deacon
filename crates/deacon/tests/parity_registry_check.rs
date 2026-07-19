//! Structural completeness guard for the parity surface (018-harden-parity-
//! harness, T045; research D5; FR-013, FR-022..FR-024; SC-003).
//!
//! Hermetic and selected in ALL regular lanes (no oracle, no Docker, no network):
//! it proves — on every PR — that the registry, the test tree, the nextest profile
//! selection, and the corpora stay mutually consistent, so coverage cannot silently
//! rot. Four checks:
//!
//! 1. registry ↔ `crates/deacon/tests/*.rs` bidirectional match (every registered
//!    binary has a source file; every `parity_*.rs` file is registered or a
//!    recognized hermetic meta-test);
//! 2. `.config/nextest.toml` (parsed via the `toml` crate) — `[profile.parity]`
//!    selects EXACTLY `live_binaries`, excludes `internal_consistency_binaries`,
//!    and NO other profile selects a live parity binary (FR-014);
//! 3. every corpus directory meets its registered `min_cases` (FR-024);
//! 4. no parity/consistency source carries `#[ignore]` or a legacy silent-skip
//!    idiom (`gated(`, `upstream_available(`, the retired `DEACON_PARITY` opt-in
//!    plumbing) (FR-023).

use parity_harness::registry::{self, META_TEST_BINARIES, ParityRegistry, parse_nextest_profiles};
use parity_harness::workspace_root;

/// 1. Registry ↔ test-file bidirectional match.
#[test]
fn registry_matches_test_files_both_directions() {
    let reg = ParityRegistry::load().unwrap_or_else(|e| panic!("registry.json: {e}"));
    let tests_dir = workspace_root().join("crates/deacon/tests");
    let problems = reg.check_test_files(&tests_dir);
    assert!(
        problems.is_empty(),
        "registry ↔ tests/*.rs mismatch:\n{}",
        problems.join("\n")
    );

    // The hermetic meta-test binaries are recognized non-live `parity_*` files and
    // must exist — this file is itself one of them.
    for name in META_TEST_BINARIES {
        assert!(
            tests_dir.join(format!("{name}.rs")).is_file(),
            "hermetic meta-test binary `{name}.rs` must exist"
        );
    }
    // ...and must NEVER be registered as live binaries.
    for name in META_TEST_BINARIES {
        assert!(
            !reg.live_names().contains(name),
            "meta-test binary `{name}` must not be a live parity binary"
        );
    }
}

/// 2. nextest profile selection: parity covers exactly the live set; no other
///    profile selects a live binary.
#[test]
fn nextest_parity_profile_selects_exactly_live_binaries() {
    let reg = ParityRegistry::load().unwrap_or_else(|e| panic!("registry.json: {e}"));
    let toml_path = workspace_root().join(".config/nextest.toml");
    let toml_text =
        std::fs::read_to_string(&toml_path).unwrap_or_else(|e| panic!("read {toml_path:?}: {e}"));
    let profiles =
        parse_nextest_profiles(&toml_text).unwrap_or_else(|e| panic!("parse nextest.toml: {e}"));

    // [profile.parity] must be declared.
    assert!(
        profiles.default_filters.contains_key("parity"),
        ".config/nextest.toml must declare [profile.parity]"
    );

    let problems = reg.check_nextest_profiles(&profiles);
    assert!(
        problems.is_empty(),
        "nextest.toml parity-selection problems:\n{}",
        problems.join("\n")
    );
}

/// 3. Every corpus directory meets its registered minimum case count.
#[test]
fn corpora_meet_registered_minimums() {
    let reg = ParityRegistry::load().unwrap_or_else(|e| panic!("registry.json: {e}"));
    let root = workspace_root();

    for corpus in &reg.corpora {
        let dir = root.join(&corpus.path);
        let discovered = match corpus.id.as_str() {
            "tier1" => registry::discover_tier1_cases(&dir)
                .unwrap_or_else(|e| panic!("discover tier1 cases: {e}")),
            "errors" => registry::discover_error_cases(&dir)
                .unwrap_or_else(|e| panic!("discover error cases: {e}")),
            other => {
                panic!("registry declares an unknown corpus id `{other}` with no discovery rule")
            }
        };
        reg.check_corpus_min(corpus, discovered.len())
            .unwrap_or_else(|e| panic!("{e}"));
    }
}

/// 4. Source audit: no parity/consistency source uses `#[ignore]` or a legacy
///    silent-skip idiom.
#[test]
fn no_parity_source_uses_ignore_or_legacy_skip_idioms() {
    // Unambiguous forbidden tokens. The sanctioned override env vars
    // (`DEACON_PARITY_DEVCONTAINER`, `DEACON_PARITY_DOCKER`,
    // `DEACON_PARITY_REPORT_DIR`) are NOT legacy idioms and are intentionally not
    // matched here; only the retired opt-in gate (`DEACON_PARITY=…`) and the
    // retired read-configuration template plumbing (`DEACON_PARITY_UPSTREAM…`) are.
    const FORBIDDEN: &[&str] = &[
        "#[ignore]",
        "gated(",
        "upstream_available(",
        "DEACON_PARITY_UPSTREAM",
        "DEACON_PARITY=",
    ];
    // The auditor itself must name the forbidden tokens (in `FORBIDDEN`), so it is
    // excluded from its own scan.
    const SELF: &str = "parity_registry_check.rs";

    let tests_dir = workspace_root().join("crates/deacon/tests");
    let mut audited = 0usize;
    let mut problems = Vec::new();

    let rd = std::fs::read_dir(&tests_dir).unwrap_or_else(|e| panic!("read {tests_dir:?}: {e}"));
    for entry in rd.filter_map(Result::ok) {
        let file_name = entry.file_name();
        let file = file_name.to_string_lossy();
        let Some(stem) = file.strip_suffix(".rs") else {
            continue;
        };
        if !(stem.starts_with("parity_") || stem.starts_with("consistency_")) {
            continue;
        }
        if file == SELF {
            continue;
        }
        audited += 1;
        let text = std::fs::read_to_string(entry.path())
            .unwrap_or_else(|e| panic!("read {:?}: {e}", entry.path()));
        for needle in FORBIDDEN {
            if text.contains(needle) {
                problems.push(format!("{file}: contains forbidden idiom `{needle}`"));
            }
        }
    }

    assert!(
        audited >= 10,
        "expected to audit the full parity/consistency source set, only saw {audited} file(s)"
    );
    assert!(
        problems.is_empty(),
        "legacy silent-skip idiom(s) found in parity sources:\n{}",
        problems.join("\n")
    );
}

/// Guard: the tests dir this file audits is the real one (fail loud if the anchor
/// ever drifts, rather than silently auditing nothing).
#[test]
fn tests_dir_anchor_is_valid() {
    let tests_dir = workspace_root().join("crates/deacon/tests");
    assert!(
        tests_dir.join("parity_registry_check.rs").is_file(),
        "workspace_root()/crates/deacon/tests must contain this source file: {}",
        tests_dir.display()
    );
}
