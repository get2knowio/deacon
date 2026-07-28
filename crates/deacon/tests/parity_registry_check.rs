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

use parity_harness::registry::{
    self, DISCOVERY_PROFILE, DiscoveryRole, GUARD_REQUIRED_PROFILES, META_TEST_BINARIES,
    PULL_REQUEST_PROFILES, ParityRegistry, filter_selects, parse_nextest_profiles,
};
use parity_harness::workspace_root;

/// Parse `.config/nextest.toml`'s `[profile.*]` `default-filter` expressions from the
/// real file. Shared by the parity and discovery cross-checks so both read one source.
fn real_nextest_profiles() -> registry::NextestProfiles {
    let toml_path = workspace_root().join(".config/nextest.toml");
    let toml_text =
        std::fs::read_to_string(&toml_path).unwrap_or_else(|e| panic!("read {toml_path:?}: {e}"));
    parse_nextest_profiles(&toml_text).unwrap_or_else(|e| panic!("parse nextest.toml: {e}"))
}

/// 1. Registry ↔ test-file bidirectional match.
#[test]
fn registry_matches_test_files_both_directions() {
    let reg = ParityRegistry::load().unwrap_or_else(|e| panic!("registry.json: {e}"));
    let tests_dir = workspace_root().join("crates/deacon/tests");
    let problems = registry::check_test_files(&reg, &tests_dir);
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
    let profiles = real_nextest_profiles();

    // [profile.parity] must be declared.
    assert!(
        profiles.default_filters.contains_key("parity"),
        ".config/nextest.toml must declare [profile.parity]"
    );

    let problems = registry::check_nextest_profiles(&reg, &profiles);
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
        registry::check_corpus_min(&reg, corpus, discovered.len())
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

    // The floor tracks the surviving source set: 6 live `parity_*` binaries + 2 hermetic
    // meta-test binaries + 2 `consistency_*` binaries. It exists so a scan that silently
    // stopped finding files cannot pass by auditing nothing — it is NOT a coverage claim,
    // and it drops as carriers retire (023 US7 removed four).
    assert!(
        audited >= 9,
        "expected to audit the full parity/consistency source set, only saw {audited} file(s)"
    );
    assert!(
        problems.is_empty(),
        "legacy silent-skip idiom(s) found in parity sources:\n{}",
        problems.join("\n")
    );
}

/// 5. Waiver location: the conformance registry owns every parity waiver now
///    (019-conformance-registry, research D3). The migrated records must live under
///    `conformance/registry/waivers/`, and the legacy parity-corpus locations
///    (`fixtures/parity-corpus/waivers/` and `fixtures/parity-corpus/errors/*/expect.json`)
///    must be GONE — so a stray reintroduction of the retired two-file duplication
///    fails structurally on every PR (FR-027, FR-028).
#[test]
fn waivers_live_in_conformance_registry_not_legacy_locations() {
    let root = workspace_root();

    // New location exists and holds at least the migrated set (9 errors + 1 tier1).
    let registry_waivers = root.join("conformance/registry/waivers");
    assert!(
        registry_waivers.is_dir(),
        "conformance/registry/waivers/ must exist as the single waiver location: {}",
        registry_waivers.display()
    );
    let wvr_count = std::fs::read_dir(&registry_waivers)
        .unwrap_or_else(|e| panic!("read {registry_waivers:?}: {e}"))
        .filter_map(Result::ok)
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with("wvr-") && name.ends_with(".json")
        })
        .count();
    assert!(
        wvr_count >= 10,
        "expected >= 10 migrated wvr-*.json records, found {wvr_count} in {}",
        registry_waivers.display()
    );

    // Legacy location 1: the parity-corpus waivers/ directory must be removed
    // entirely (migrated into the registry, including its README).
    let legacy_waivers = root.join("fixtures/parity-corpus/waivers");
    assert!(
        !legacy_waivers.exists(),
        "legacy waiver directory {} must be removed — waivers now live in \
         conformance/registry/waivers/ (research 019 D3)",
        legacy_waivers.display()
    );

    // Legacy location 2: NO errors case may carry an `expect.json` (each was
    // migrated to a corpus-case-scoped wvr- record). The case directories and their
    // `.devcontainer/` inputs stay; only the waiver files are gone.
    let errors_root = root.join("fixtures/parity-corpus/errors");
    let mut stragglers = Vec::new();
    let rd =
        std::fs::read_dir(&errors_root).unwrap_or_else(|e| panic!("read {errors_root:?}: {e}"));
    for entry in rd.filter_map(Result::ok) {
        let case_dir = entry.path();
        if case_dir.is_dir() && case_dir.join("expect.json").is_file() {
            stragglers.push(case_dir.display().to_string());
        }
    }
    assert!(
        stragglers.is_empty(),
        "legacy per-case expect.json waiver file(s) must be removed (migrated to \
         conformance/registry/waivers/): {stragglers:?}"
    );

    // 023-migrate-parity-to-conformance (T048, FR-025): a characterized exception must
    // resolve from EXACTLY ONE authoritative location. Absence of the legacy paths
    // (above) proves no SECOND location; this proves no second RECORD — two files
    // claiming one id, or a `wvr-`/`ext-` id collision, would reintroduce the same
    // ambiguity by another route, and a reader could not tell which one governs.
    let mut per_id: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(&registry_waivers)
        .unwrap_or_else(|e| panic!("read {registry_waivers:?}: {e}"))
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let value: serde_json::Value =
            serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {path:?}: {e}"));
        let id = value
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("{path:?} has no `id`"))
            .to_string();
        // The file name must mirror the id, so "which file holds wvr-x?" has one answer.
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        assert_eq!(
            stem, id,
            "waiver file {path:?} must be named after its id so a record has exactly one \
             discoverable home"
        );
        per_id.entry(id).or_default().push(stem);
    }
    let duplicated: Vec<_> = per_id.iter().filter(|(_, files)| files.len() > 1).collect();
    assert!(
        duplicated.is_empty(),
        "a characterized exception must resolve from exactly one record: {duplicated:?}"
    );

    // `wvr-` and `ext-` namespaces must not collide either — an id that resolves to
    // both a waiver and an extension has two authoritative meanings.
    let extensions_raw = std::fs::read_to_string(root.join("conformance/registry/extensions.json"))
        .expect("read extensions.json");
    let extensions: serde_json::Value =
        serde_json::from_str(&extensions_raw).expect("parse extensions.json");
    let ext_ids: Vec<String> = extensions
        .get("records")
        .and_then(|v| v.as_array())
        .map(|records| {
            records
                .iter()
                .filter_map(|r| r.get("id").and_then(|v| v.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    for id in &ext_ids {
        assert!(
            !per_id.contains_key(id),
            "`{id}` resolves as BOTH a waiver record and an extension record"
        );
    }

    // Every characterized exception (waiver or extension) is dispositioned exactly once
    // in the migration mapping — the record-level counterpart of FR-024/FR-028, checked
    // here so a reintroduced second location cannot hide behind an unmapped exception.
    let mapping_raw = std::fs::read_to_string(root.join("conformance/migration/mapping.json"))
        .expect("read mapping.json");
    let mapping: serde_json::Value =
        serde_json::from_str(&mapping_raw).expect("parse mapping.json");
    let mut mapped: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    if let Some(entries) = mapping.get("exceptions").and_then(|v| v.as_array()) {
        for entry in entries {
            if let Some(id) = entry.get("exception").and_then(|v| v.as_str()) {
                *mapped.entry(id.to_string()).or_default() += 1;
            }
        }
    }
    // Exceptions authored AFTER the branch point have no pre-migration form to preserve,
    // so they are legitimately unmapped. The predicate is the SHARED one — this test was
    // the fourth place encoding it, and the first three had each been patched separately.
    let registry =
        deacon_conformance::load::Registry::load(&deacon_conformance::default_registry_dir())
            .expect("the real registry loads cleanly");
    let post_branch = deacon_conformance::conservation::post_branch_exceptions(&registry);

    let mut missing: Vec<&String> = per_id.keys().chain(ext_ids.iter()).collect();
    missing.retain(|id| !post_branch.contains(*id));
    missing.retain(|id| mapped.get(*id).copied().unwrap_or(0) != 1);
    assert!(
        missing.is_empty(),
        "every characterized exception must be dispositioned EXACTLY ONCE in \
         conformance/migration/mapping.json: {missing:?}"
    );
}

/// T022 (024, Block B): the Docker-backed conformance driver is wired in all three places
/// — the parity registry, the test tree, and every nextest profile.
///
/// `check_nextest_profiles` above already enforces this generically, so why name one binary?
/// Because this binary is the one whose absence is *plausible*. It was split out of
/// `parity_conformance_runner` specifically so the Docker-backed resource groups could be
/// scheduled and budgeted separately (FR-077/077a), and a half-landed split — source file
/// present, registry entry or profile filter missing — reads as "the Docker cases are
/// covered" while nothing selects them under `--profile parity`, or while a hermetic lane
/// silently picks up a binary that needs a daemon. The generic check reports that as one
/// line in a list of profile problems; this reports it as what it is.
#[test]
fn the_docker_conformance_driver_is_registered_selected_and_excluded() {
    const DOCKER_DRIVER: &str = "parity_conformance_docker";

    let root = workspace_root();
    let reg = ParityRegistry::load().unwrap_or_else(|e| panic!("registry.json: {e}"));

    // 1. Registered as a live binary, and declared Docker-requiring.
    let entry = reg
        .live_binaries
        .iter()
        .find(|b| b.name == DOCKER_DRIVER)
        .unwrap_or_else(|| {
            panic!(
                "`{DOCKER_DRIVER}` must be registered in fixtures/parity-corpus/registry.json \
                 live_binaries; found {:?}",
                reg.live_names()
            )
        });
    assert!(
        entry.docker_required,
        "`{DOCKER_DRIVER}` drives the docker-shared / docker-exclusive resource groups, so \
         its registry entry must declare docker_required = true"
    );

    // 2. Its sibling's claim must be true too. The split exists so that
    //    `parity_conformance_runner` keeps only the config-only groups — if it were still
    //    Docker-requiring, the split bought nothing and the registry would be lying again
    //    in the same way it did before 024 T020.
    let runner = reg
        .live_binaries
        .iter()
        .find(|b| b.name == "parity_conformance_runner")
        .expect("`parity_conformance_runner` must stay registered");
    assert!(
        !runner.docker_required,
        "`parity_conformance_runner` drives only the config-only resource groups (`none`, \
         `fs-heavy`); docker_required must be false"
    );

    // 3. A source file exists.
    let source = root
        .join("crates/deacon/tests")
        .join(format!("{DOCKER_DRIVER}.rs"));
    assert!(
        source.is_file(),
        "`{DOCKER_DRIVER}` is registered but has no source file: {}",
        source.display()
    );

    // 4. `[profile.parity]` selects it, and NO other profile does.
    let toml_path = root.join(".config/nextest.toml");
    let toml_text =
        std::fs::read_to_string(&toml_path).unwrap_or_else(|e| panic!("read {toml_path:?}: {e}"));
    let profiles =
        parse_nextest_profiles(&toml_text).unwrap_or_else(|e| panic!("parse nextest.toml: {e}"));

    for (profile, filter) in &profiles.default_filters {
        let Some(filter) = filter else {
            // A profile with no default-filter selects everything — which for a live binary
            // is exactly the untruthful state the exclusions exist to prevent.
            assert_eq!(
                profile, "parity",
                "[profile.{profile}] declares no default-filter, so it would select the live \
                 binary `{DOCKER_DRIVER}`"
            );
            continue;
        };
        let selected = filter_selects(filter, DOCKER_DRIVER)
            .unwrap_or_else(|e| panic!("evaluate [profile.{profile}] default-filter: {e}"));
        if profile == "parity" {
            assert!(
                selected,
                "[profile.parity] must select `{DOCKER_DRIVER}` — it is the only sanctioned \
                 entry point for the live Docker conformance tier"
            );
        } else {
            assert!(
                !selected,
                "[profile.{profile}] selects the live binary `{DOCKER_DRIVER}`; every non-parity \
                 lane must be truthful by NON-SELECTION (FR-014) — a green fast/CI run must \
                 never imply the Docker conformance tier ran"
            );
        }
    }
}

/// T022 (024, Constitution II): this feature's dev-only commands never reach the shipped
/// `deacon` CLI — at ANY depth of the command tree.
///
/// `coverage` / `coverage-regressions` are `deacon-conformance` bin surfaces: they generate
/// the obligation denominator and the injected-regression report, which are contributor
/// tooling about how deacon is *tested*, not consumer functionality described by the
/// containers.dev spec. A conformance-tracking command that ships is a scope violation
/// users can then depend on, which makes it expensive to withdraw.
///
/// The existing `the_shipped_cli_gained_no_subcommand_from_this_feature` checks the
/// top-level column only. That was sufficient when every dev-only tool was a top-level
/// group, but `coverage` has SUBCOMMANDS (`generate`/`check`/`report`/`scaffold`), and the
/// cheapest way to leak one is to hang it off an existing consumer command rather than add
/// a new top-level entry. So this walks one level deeper.
#[test]
fn no_coverage_or_regression_command_reaches_the_shipped_cli() {
    /// The 024 command surfaces, plus the nouns they would most plausibly leak as.
    const DEV_ONLY: &[&str] = &[
        "coverage",
        "coverage-regressions",
        "regressions",
        "obligations",
        "obligation",
        "scenario",
        "applicability",
    ];

    fn subcommands_of(args: &[&str]) -> Vec<String> {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_deacon"))
            .args(args)
            .arg("--help")
            .output()
            .unwrap_or_else(|e| panic!("`deacon {} --help` runs: {e}", args.join(" ")));
        assert!(
            output.status.success(),
            "`deacon {} --help` must succeed",
            args.join(" ")
        );
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .skip_while(|l| !l.trim_start().starts_with("Commands:"))
            .skip(1)
            .take_while(|l| !l.trim().is_empty())
            .filter_map(|l| l.split_whitespace().next())
            .map(str::to_string)
            .collect()
    }

    let top = subcommands_of(&[]);
    // Positive control: a parse that silently found nothing would pass vacuously.
    assert!(
        top.iter().any(|c| c == "up"),
        "expected to parse the real subcommand list from `deacon --help`; got {top:?}"
    );

    let mut leaked: Vec<String> = Vec::new();
    for dev_only in DEV_ONLY {
        if top.iter().any(|c| c == dev_only) {
            leaked.push((*dev_only).to_string());
        }
    }
    // One level deeper: a dev-only command hung off a consumer command leaks just as far.
    for parent in &top {
        if parent == "help" {
            continue;
        }
        for nested in subcommands_of(&[parent.as_str()]) {
            if DEV_ONLY.contains(&nested.as_str()) {
                leaked.push(format!("{parent} {nested}"));
            }
        }
    }

    assert!(
        leaked.is_empty(),
        "dev-only conformance command(s) reached the shipped consumer CLI: {leaked:?}. \
         `coverage` and the regression tooling belong to `deacon-conformance` \
         (constitution II)."
    );
}

/// **T054** (025 US3, FR-057): the discovery lane's profile selection.
///
/// Every **live** discovery binary is selected by `[profile.discovery]` and by no
/// pull-request profile; every hermetic **guard** is the exact opposite — selected by the
/// fast lanes and *not* captured by the discovery allow-list.
///
/// The role split is the substance of this test, not a technicality. FR-057 says "every
/// discovery program is selected by a discovery lane and by no pull-request lane", and
/// read without roles that sentence would exile `discovery_hermetic` and `discovery_cli`
/// from the fast lane — which is precisely the `discovery_*` glob failure research D9
/// exists to prevent. The guards are the lane's *machinery*, not campaigns: they are what
/// makes the campaigns' exclusion checkable, so a lane that stopped running them would
/// remove the only thing watching it.
///
/// Every verdict is reached by **evaluating** the filter expression. The profiles name
/// the discovery binaries in order to *exclude* them, and a token match would read
/// `… & not (binary(=discovery_campaign))` as a selection and fail a correct file.
#[test]
fn the_discovery_lane_selects_the_campaigns_and_no_pull_request_lane_does() {
    let reg = ParityRegistry::load().unwrap_or_else(|e| panic!("registry.json: {e}"));
    let profiles = real_nextest_profiles();

    // Positive control: a registry that lost its discovery entries would make every
    // assertion below vacuous, and the "nothing to check" state is indistinguishable
    // from "everything checks out" in a passing run.
    let live: Vec<String> = reg
        .discovery_of_role(DiscoveryRole::Live)
        .into_iter()
        .map(|b| b.name.clone())
        .collect();
    let guards: Vec<String> = reg
        .discovery_of_role(DiscoveryRole::Guard)
        .into_iter()
        .map(|b| b.name.clone())
        .collect();
    assert!(
        !live.is_empty() && !guards.is_empty(),
        "registry.json must enumerate both discovery roles; found live={live:?} guards={guards:?}"
    );

    let discovery_filter = profiles
        .default_filters
        .get(DISCOVERY_PROFILE)
        .unwrap_or_else(|| {
            panic!(".config/nextest.toml must declare [profile.{DISCOVERY_PROFILE}]")
        })
        .as_deref()
        .unwrap_or_else(|| {
            panic!(
                "[profile.{DISCOVERY_PROFILE}] must declare a default-filter; without one it \
                 selects every binary in the workspace"
            )
        });

    for name in &live {
        assert!(
            filter_selects(discovery_filter, name)
                .unwrap_or_else(|e| panic!("[profile.{DISCOVERY_PROFILE}] filter: {e}")),
            "[profile.{DISCOVERY_PROFILE}] must select live discovery binary `{name}` — it \
             is the only sanctioned entry point, so nothing else would ever run it"
        );
    }
    for name in &guards {
        assert!(
            !filter_selects(discovery_filter, name)
                .unwrap_or_else(|e| panic!("[profile.{DISCOVERY_PROFILE}] filter: {e}")),
            "[profile.{DISCOVERY_PROFILE}] captures the hermetic guard `{name}`. Its \
             default-filter must stay an explicit `binary(=…)` allow-list — a \
             `discovery_*` glob would swallow the guards and silently remove them from \
             the fast lane (research D9)"
        );
    }

    for profile in PULL_REQUEST_PROFILES {
        let expr = profiles
            .default_filters
            .get(*profile)
            .unwrap_or_else(|| panic!("[profile.{profile}] must exist"))
            .as_deref()
            .unwrap_or_else(|| {
                panic!(
                    "[profile.{profile}] has no default-filter, so it selects every binary \
                     including the live discovery campaigns"
                )
            });
        for name in &live {
            assert!(
                !filter_selects(expr, name)
                    .unwrap_or_else(|e| panic!("[profile.{profile}] filter: {e}")),
                "[profile.{profile}] selects live discovery binary `{name}` — discovery \
                 gates nothing, so a green pull-request run must never imply a campaign \
                 ran (FR-055/FR-057)"
            );
        }
        if GUARD_REQUIRED_PROFILES.contains(profile) {
            for name in &guards {
                assert!(
                    filter_selects(expr, name)
                        .unwrap_or_else(|e| panic!("[profile.{profile}] filter: {e}")),
                    "[profile.{profile}] does not select the hermetic guard `{name}` — a \
                     guard that does not run in the fast lane is a guard nobody notices \
                     going stale"
                );
            }
        }
    }
}

/// **T056** (025 US3): registry ↔ `tests/*.rs` ↔ `.config/nextest.toml` agreement for the
/// discovery lane, the same three-place rule the parity lane already lives under.
///
/// A half-landed discovery binary is the failure this catches: a source file with no
/// registry entry has no declared lane, so nothing checks which profiles select it; a
/// registry entry with no source file makes `binary(=…)` name a binary that does not
/// exist, which nextest treats as a hard config error and which therefore breaks *every*
/// lane in the workspace rather than only this one.
///
/// The two source directories are load-bearing: `discovery_cli` drives the
/// `deacon-conformance` binary and so must live in that crate's test tree, while the
/// campaigns and the repository-wiring guard live in `crates/deacon/tests`. A check that
/// assumed one directory would silently stop covering whichever binary moved, so the
/// registry records `tests_dir` per binary and the file→registry sweep scans both.
#[test]
fn the_discovery_lane_is_wired_in_registry_tests_and_nextest() {
    let reg = ParityRegistry::load().unwrap_or_else(|e| panic!("registry.json: {e}"));
    let root = workspace_root();

    // 1 + 2. Registry ↔ source files, both directions.
    let problems = registry::check_discovery_files(&reg, &root);
    assert!(
        problems.is_empty(),
        "registry ↔ discovery tests/*.rs mismatch:\n{}",
        problems.join("\n")
    );

    // Each entry's declared directory must be the one the file is actually in — an entry
    // pointing at the wrong crate passes a naive existence check only when a file of the
    // same name happens to exist in both.
    for binary in &reg.discovery_binaries {
        let declared = root
            .join(&binary.tests_dir)
            .join(format!("{}.rs", binary.name));
        assert!(
            declared.is_file(),
            "discovery binary `{}` declares tests_dir `{}`, but {} does not exist",
            binary.name,
            binary.tests_dir,
            declared.display()
        );
    }

    // 3. Registry ↔ `.config/nextest.toml`.
    let problems = registry::check_discovery_profiles(&reg, &real_nextest_profiles());
    assert!(
        problems.is_empty(),
        "discovery lane nextest wiring problems:\n{}",
        problems.join("\n")
    );

    // The two lanes must stay disjoint in BOTH directions. `check_nextest_profiles`
    // already proves no non-parity profile selects a live parity binary (so
    // `[profile.discovery]` cannot pick one up); this proves the converse, which nothing
    // else states: the parity lane must not acquire a campaign. The exclusion is about
    // the lanes answering different questions on different budgets, so "the metamorphic
    // tier is cheap" is never a reason to relax it.
    let profiles = real_nextest_profiles();
    let parity_filter = profiles
        .default_filters
        .get("parity")
        .and_then(|f| f.as_deref())
        .expect("[profile.parity] must declare a default-filter");
    for binary in reg.discovery_of_role(DiscoveryRole::Live) {
        assert!(
            !filter_selects(parity_filter, &binary.name).expect("evaluate the parity filter"),
            "[profile.parity] selects the discovery campaign `{}`; a campaign would exceed \
             the parity lane's budget and would make a certification lane stochastic",
            binary.name
        );
    }
}

/// **T057** (025 US3, FR-059): this feature adds NO subcommand to the shipped `deacon`
/// CLI, at any depth of the command tree.
///
/// The `discovery` command group is a `deacon-conformance` bin surface and the campaign /
/// proof programs are `parity-harness` bins: contributor tooling about how deacon is
/// *tested*, not consumer functionality the containers.dev spec describes. A
/// conformance-tracking command that ships is a scope violation users can then depend on,
/// which makes it expensive to withdraw.
///
/// Walks one level deeper than the top-level column for the same reason
/// `no_coverage_or_regression_command_reaches_the_shipped_cli` does: `discovery` has
/// subcommands (`check`/`report`/`triage`/`split`/`scaffold`), and the cheapest way to
/// leak one is to hang it off an existing consumer command rather than add a new
/// top-level entry.
#[test]
fn no_discovery_command_reaches_the_shipped_cli() {
    /// The 025 command surfaces plus the nouns they would most plausibly leak as. Every
    /// entry is a word that has no business being a consumer subcommand — deliberately
    /// NOT `corpus` or `campaign`-adjacent English that a future consumer feature might
    /// legitimately want, because a guard that forbids plausible names gets deleted.
    const DEV_ONLY: &[&str] = &[
        "discovery",
        "discovery-campaign",
        "discovery-proof",
        "findings",
        "triage",
        "metamorphic",
        "mutate",
        "shrink",
    ];

    fn subcommands_of(args: &[&str]) -> Vec<String> {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_deacon"))
            .args(args)
            .arg("--help")
            .output()
            .unwrap_or_else(|e| panic!("`deacon {} --help` runs: {e}", args.join(" ")));
        assert!(
            output.status.success(),
            "`deacon {} --help` must succeed",
            args.join(" ")
        );
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .skip_while(|l| !l.trim_start().starts_with("Commands:"))
            .skip(1)
            .take_while(|l| !l.trim().is_empty())
            .filter_map(|l| l.split_whitespace().next())
            .map(str::to_string)
            .collect()
    }

    let top = subcommands_of(&[]);
    // Positive control: a parse that silently found nothing would pass vacuously.
    assert!(
        top.iter().any(|c| c == "up"),
        "expected to parse the real subcommand list from `deacon --help`; got {top:?}"
    );

    let mut leaked: Vec<String> = Vec::new();
    for dev_only in DEV_ONLY {
        if top.iter().any(|c| c == dev_only) {
            leaked.push((*dev_only).to_string());
        }
    }
    for parent in &top {
        if parent == "help" {
            continue;
        }
        for nested in subcommands_of(&[parent.as_str()]) {
            if DEV_ONLY.contains(&nested.as_str()) {
                leaked.push(format!("{parent} {nested}"));
            }
        }
    }

    assert!(
        leaked.is_empty(),
        "dev-only discovery command(s) reached the shipped consumer CLI: {leaked:?}. The \
         discovery group belongs to `deacon-conformance` and the campaign/proof bins to \
         `parity-harness` (constitution II, FR-059)."
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

/// T089 (US6, FR-032): no surviving surface references a REMOVED one.
///
/// The migration deleted four carriers and their shared runner module. A reference left
/// behind in a machine-consumed file is not cosmetic — nextest would select a binary that
/// does not exist, the parity registry would claim coverage nothing provides, and the
/// Makefile or workflow would fail at a step nobody reads until CI is already red.
///
/// Documentation is held to a softer rule on purpose: a doc line may name a removed
/// surface **only while saying it is gone**. History is worth keeping; a doc that still
/// describes a deleted binary as current architecture is a lie with a long half-life.
#[test]
fn no_surface_references_a_removed_binary() {
    let root = workspace_root();

    /// The surfaces retired by 023 US7.
    const REMOVED: &[&str] = &[
        "parity_corpus_tier1",
        "parity_corpus_merged",
        "parity_corpus_errors",
        "parity_read_configuration",
        "corpus_runner",
        // Retired with the V25 baseline-drift gate (023 T099): both regenerated the
        // baseline, which a carrier deletion necessarily invalidates. `baseline_archive`
        // replaced them. `.config/nextest.toml`'s comment block still named
        // `baseline_drift` weeks after the file was gone — a stale name in a
        // machine-consumed file is exactly what this check exists to catch, so the two
        // hermetic retirements belong here alongside the live carriers.
        "baseline_drift",
        "baseline_enumeration",
        // Retired by 025 US7 (T109). The 33 pinned entries moved into the Rust-owned
        // `conformance/discovery/corpus.json`, where the immutable-reference rule (D4) can
        // run hermetically on every pull request instead of nowhere. Registered here for
        // the same reason the hermetic retirements above are: a doc that still tells a
        // reader to run a deleted script is a lie with a long half-life, and this file's
        // own baseline enumeration reads the manifest path.
        "fetch_realworld_corpus",
    ];
    /// Words that mark a mention as historical rather than current-state.
    const RETIREMENT_MARKERS: &[&str] = &[
        "retire",
        "retired",
        "delete",
        "deleted",
        "remove",
        "removed",
        "gone",
        "former",
        "formerly",
        "was ",
        "were ",
        "gains no",
        "gained",
        "gave way",
        "no longer",
        "gone (",
        "history",
        "gone.",
        "superseded",
        "gone;",
    ];

    // Machine-consumed files: ZERO tolerance — a name here is executed, not read.
    let mut problems: Vec<String> = Vec::new();
    for rel in [
        ".config/nextest.toml",
        "fixtures/parity-corpus/registry.json",
        "Makefile",
        ".github/workflows/parity.yml",
        // The discovery lane's workflow is machine-consumed on the same terms (025 T058):
        // a stale binary name in it is executed, not read.
        ".github/workflows/discovery.yml",
    ] {
        let path = root.join(rel);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        for (line_no, line) in text.lines().enumerate() {
            for removed in REMOVED {
                if line.contains(removed) {
                    problems.push(format!(
                        "{rel}:{}: references removed surface `{removed}`",
                        line_no + 1
                    ));
                }
            }
        }
    }

    // Documentation: a mention must be framed as history.
    for rel in [
        "CLAUDE.md",
        "fixtures/parity-corpus/README.md",
        "fixtures/parity-corpus/errors/README.md",
        "conformance/RULES.md",
    ] {
        let path = root.join(rel);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (line_no, line) in text.lines().enumerate() {
            for removed in REMOVED {
                if !line.contains(removed) {
                    continue;
                }
                let lowered = line.to_ascii_lowercase();
                if !RETIREMENT_MARKERS.iter().any(|m| lowered.contains(m)) {
                    problems.push(format!(
                        "{rel}:{}: names removed surface `{removed}` as current-state; \
                         either drop it or say it is gone",
                        line_no + 1
                    ));
                }
            }
        }
    }

    assert!(
        problems.is_empty(),
        "dangling references to removed surfaces (FR-032):\n{}",
        problems.join("\n")
    );
}

/// T089 (third direction, 023 T116): no machine-consumed file globs or reads a PATH the
/// migration deleted.
///
/// The name-based check above would not have caught the defect that motivated this one.
/// The parity workflow pre-pulls fixture base images so a multi-GB first pull cannot blow
/// the harness's per-invocation bound — and it globbed
/// `fixtures/parity-corpus/*/.devcontainer/devcontainer.json`, which US7 deleted. The step
/// kept succeeding while matching nothing, so the protection silently evaporated and the
/// next cold run looked like a hang in deacon. A path can rot exactly as a name can, and
/// it fails more quietly because a glob that matches nothing is not an error.
#[test]
fn no_surface_globs_a_removed_path() {
    let root = workspace_root();

    /// Paths the migration removed, as they would appear in a script or workflow.
    const REMOVED_PATHS: &[&str] = &[
        "fixtures/parity-corpus/*/",
        "fixtures/parity-corpus/errors/*",
        "fixtures/config/basic/devcontainer",
        "fixtures/config/with-variables/devcontainer",
        // 025 US7 (T109): the Python fetcher is gone; the manifest is
        // `conformance/discovery/corpus.json` and the fetch lives in
        // `parity_harness::discovery::corpus_fetch`.
        "fixtures/parity-corpus/fetch_realworld_corpus.py",
    ];

    let mut problems: Vec<String> = Vec::new();
    for rel in [
        "Makefile",
        ".github/workflows/parity.yml",
        "scripts/parity/prepull-fixture-images.sh",
    ] {
        let path = root.join(rel);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (line_no, line) in text.lines().enumerate() {
            for removed in REMOVED_PATHS {
                if line.contains(removed) {
                    problems.push(format!(
                        "{rel}:{}: reads removed path `{removed}`",
                        line_no + 1
                    ));
                }
            }
        }
    }

    // And the surviving pre-pull must actually resolve to something: a warm-cache step
    // that matches nothing is indistinguishable from no step at all.
    //
    // This check MIRRORS THE SCRIPT'S DISCOVERY RULE (recursive, `.json` or `.jsonc`).
    // An earlier revision asserted only `<fx>/.devcontainer/devcontainer.json` — the same
    // fixed relative path the script used — so it went green while two fixtures whose
    // config sits at `<fx>/<subdir>/devcontainer.jsonc` were warmed by nobody. A guard
    // that shares the blind spot of the thing it guards is not a guard.
    let fixtures = root.join("conformance").join("fixtures");
    let configs = find_devcontainer_configs(&fixtures);
    let with_image = configs
        .iter()
        .filter(|p| {
            std::fs::read_to_string(p)
                .map(|t| t.contains("\"image\""))
                .unwrap_or(false)
        })
        .count();
    assert!(
        with_image > 0,
        "no fixture under {} declares an `image`, so the pre-pull would warm nothing",
        fixtures.display()
    );

    // The nested-config shape must stay covered specifically: it is the one the fixed-path
    // glob missed, so losing it again would be invisible in the aggregate count above.
    let nested = configs
        .iter()
        .filter(|p| {
            p.parent()
                .and_then(|d| d.file_name())
                .is_some_and(|n| n != ".devcontainer")
        })
        .count();
    assert!(
        nested > 0,
        "no fixture config outside a `.devcontainer/` directory was found — if that shape \
         is genuinely gone, drop this assertion deliberately rather than letting the \
         recursive discovery quietly stop being load-bearing"
    );

    // The workflow must DELEGATE to the script, not carry a second copy of the glob.
    // Two copies of one discovery rule is precisely how T116 rotted: one was updated and
    // the other kept exiting 0 while matching nothing.
    let workflow = std::fs::read_to_string(root.join(".github/workflows/parity.yml"))
        .expect("parity workflow is readable");
    assert!(
        workflow.contains("scripts/parity/prepull-fixture-images.sh"),
        "the parity workflow must pre-pull via scripts/parity/prepull-fixture-images.sh"
    );
    for (line_no, line) in workflow.lines().enumerate() {
        let code = line.trim_start();
        if code.starts_with('#') {
            continue; // prose may name the path it is explaining
        }
        assert!(
            !code.contains("devcontainer.json"),
            ".github/workflows/parity.yml:{}: carries its own fixture-config glob; \
             discovery lives in scripts/parity/prepull-fixture-images.sh so the workflow \
             and `make test-parity` cannot drift apart",
            line_no + 1
        );
    }

    assert!(
        problems.is_empty(),
        "machine-consumed files reference removed paths:\n{}",
        problems.join("\n")
    );
}

/// Every `devcontainer.json` / `devcontainer.jsonc` under `dir`, at any depth.
///
/// Mirrors the `find` in `scripts/parity/prepull-fixture-images.sh`.
fn find_devcontainer_configs(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(find_devcontainer_configs(&path));
        } else if matches!(
            path.file_name().and_then(|n| n.to_str()),
            Some("devcontainer.json" | "devcontainer.jsonc")
        ) {
            found.push(path);
        }
    }
    found.sort();
    found
}

/// T089 (second direction): every binary the registry and nextest still name has a
/// source file, and every surviving `parity_*` source is still registered.
///
/// `check_test_files` already enforces this; asserting it HERE too states the
/// post-cut-over invariant where a reader looking for it will be, and fails with a
/// message about the cut-over rather than about generic registry drift.
#[test]
fn the_surviving_set_is_mutually_consistent() {
    let reg = ParityRegistry::load().unwrap_or_else(|e| panic!("registry.json: {e}"));
    let tests_dir = workspace_root().join("crates/deacon/tests");

    assert_eq!(
        reg.live_names().len(),
        7,
        "the surviving live set is 5 Docker scenario binaries + the declarative runner's \
         two halves (config-only + Docker-backed, 024 T015/T016); found {:?}",
        reg.live_names()
    );
    assert!(
        reg.corpora.is_empty(),
        "the corpora retired with the corpus binaries that drove them"
    );
    let problems = registry::check_test_files(&reg, &tests_dir);
    assert!(
        problems.is_empty(),
        "registry ↔ tests/*.rs must agree after the cut-over:\n{}",
        problems.join("\n")
    );
}

/// T091 (US6, Constitution II): this entire feature adds NO subcommand to the shipped
/// `deacon` CLI.
///
/// Every tool the migration built — `baseline`, `migration`, `validate`, `certify`,
/// `inventory`, `clause`, `snapshot`, `equivalence-report` — is contributor tooling in
/// `deacon-conformance` / `parity-harness`. The consumer surface is defined by the
/// containers.dev spec, and a conformance-tracking command appearing in it would be a
/// scope violation that ships to users and is then hard to withdraw.
#[test]
fn the_shipped_cli_gained_no_subcommand_from_this_feature() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_deacon"))
        .arg("--help")
        .output()
        .expect("the deacon binary runs");
    assert!(output.status.success(), "`deacon --help` must succeed");
    let help = String::from_utf8_lossy(&output.stdout);

    const DEV_ONLY: &[&str] = &[
        "baseline",
        "migration",
        "conformance",
        "equivalence",
        "certify",
        "inventory",
        "clause",
        "snapshot",
        "residual",
    ];
    // Only the subcommand column matters: prose in a description may legitimately use one
    // of these words.
    let subcommands: Vec<&str> = help
        .lines()
        .skip_while(|l| !l.trim_start().starts_with("Commands:"))
        .skip(1)
        .take_while(|l| !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert!(
        !subcommands.is_empty(),
        "expected to parse the subcommand list from `deacon --help`"
    );

    for dev_only in DEV_ONLY {
        assert!(
            !subcommands.contains(dev_only),
            "`{dev_only}` reached the shipped consumer CLI; it is contributor tooling \
             (constitution II). Subcommands found: {subcommands:?}"
        );
    }
}
