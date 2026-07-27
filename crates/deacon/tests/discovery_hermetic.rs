//! Hermetic discovery guards (025-exploratory-parity-discovery, T023/T024).
//!
//! This binary is a **guard, not a campaign**. It loads the committed discovery data
//! root, runs the D-class validation over it, and cross-checks this repository's own lane
//! wiring — no Docker, no network, no reference oracle, no randomness. It therefore runs
//! in the `default` and `dev-fast` profiles like every other hermetic conformance guard,
//! and it must keep doing so: a guard that does not run in the fast lane is a guard
//! nobody notices going stale.
//!
//! That is exactly why `[profile.discovery]`'s `default-filter` is an explicit
//! `binary(=…)` allow-list and not a `discovery_*` glob (research D9). The glob would
//! capture *this file* and silently remove it from the fast lane — the mistake the parity
//! profile already documents having made with `parity_harness_faults` and
//! `parity_registry_check`. [`the_hermetic_guard_runs_in_the_fast_lane`] asserts the
//! allow-list actually behaves that way, so the reasoning is enforced rather than
//! merely written down.
//!
//! Later user stories add their own guards here (US3's no-network and never-gates tests,
//! US4's classification/deduplication tests, US5's no-write-path and traversal proofs,
//! US7's corpus-provenance tests). They land in this same file as independent test
//! functions.

use std::path::{Path, PathBuf};

use deacon_conformance::discovery::queue::{self, DiscoveryData};
use deacon_conformance::discovery::report as discovery_report;
use deacon_conformance::load::Registry;
use parity_harness::registry::{filter_selects, parse_nextest_profiles};

/// The workspace root, derived from this crate's manifest directory so the paths are
/// stable regardless of the per-package cargo/nextest working directory.
fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or(manifest)
}

fn load_registry() -> Registry {
    Registry::load(&deacon_conformance::default_registry_dir())
        .expect("the committed conformance registry must load")
}

fn load_discovery() -> DiscoveryData {
    DiscoveryData::load_default().unwrap_or_else(|e| {
        panic!(
            "the committed discovery data root must load — a data root that does not load \
             is not an empty queue, it is an unreadable one: {e}"
        )
    })
}

/// The three files the discovery data root is made of.
const DATA_ROOT_FILES: [&str; 3] = ["findings.json", "campaigns.json", "corpus.json"];

/// The live campaign binaries — selected ONLY by `[profile.discovery]`.
const LIVE_DISCOVERY_BINARIES: [&str; 2] = ["discovery_campaign", "discovery_metamorphic"];

/// The hermetic discovery guards — selected by the fast lane and by no discovery lane.
///
/// `discovery_cli` lives in the `deacon-conformance` crate (it drives that crate's own
/// binary), but its lane requirement is identical, so both are asserted here rather than
/// splitting one invariant across two files.
const HERMETIC_DISCOVERY_BINARIES: [&str; 2] = ["discovery_hermetic", "discovery_cli"];

/// Every profile a pull request can run through. None of them may select a live
/// discovery binary, or a green PR run would imply a campaign ran when it did not.
const PULL_REQUEST_PROFILES: [&str; 6] = [
    "default",
    "dev-fast",
    "full",
    "ci",
    "mvp-integration",
    "parity",
];

#[test]
fn the_discovery_data_root_exists_and_is_canonically_rendered() {
    let dir = deacon_conformance::default_discovery_dir();
    assert!(
        dir.is_dir(),
        "the discovery data root {dir:?} must exist — it is version-controlled, not \
         created on demand"
    );
    for name in DATA_ROOT_FILES {
        let path = dir.join(name);
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{path:?} must be readable: {e}"));
        let parsed: serde_json::Value = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("{path:?} must be valid JSON: {e}"));
        assert_eq!(
            parsed["schemaVersion"],
            serde_json::json!(queue::SCHEMA_VERSION),
            "{path:?} must declare the current schema version"
        );
        assert!(
            parsed["records"].is_array(),
            "{path:?} must carry a `records` array"
        );
        assert!(
            raw.ends_with("}\n"),
            "{path:?} must end with a trailing newline so the first campaign's write is a \
             content diff and not a whole-file reformat"
        );
    }
}

#[test]
fn the_discovery_data_root_loads_and_validates_clean() {
    let registry = load_registry();
    let data = load_discovery();

    let violations = queue::check(&data, &queue::RegistryView::from_registry(&registry));
    assert!(
        violations.is_empty(),
        "the committed discovery data root must have no D-class violations:\n{}",
        violations
            .iter()
            .map(|v| format!("  {} {}: {v}", v.class(), v.record()))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn the_queue_report_renders_from_the_committed_data_root() {
    // `discovery report` must work end to end against the real data root, including
    // while it is empty. An empty queue is a legitimate state, not a degenerate one.
    let registry = load_registry();
    let data = load_discovery();

    let pins = discovery_report::CurrentPins::from_registry(&registry);
    let report = discovery_report::build_queue_report(&data, &pins);
    assert_eq!(report.total, data.findings.len());

    let json = discovery_report::render_json(&report);
    let md = discovery_report::render_md(&report);
    assert_eq!(
        (
            discovery_report::render_json(&report),
            discovery_report::render_md(&report)
        ),
        (json.clone(), md.clone()),
        "the report must be byte-stable — no timestamps, no absolute paths"
    );
    assert!(
        md.contains("| untriaged |"),
        "the untriaged bucket is COUNTED (FR-029)"
    );
    assert!(
        md.contains("never gates"),
        "the report must say what it is: a triage queue, not a gate"
    );

    // The pins the report compares against must be the ones the registry records, or
    // every finding would read as pin-stale (or none ever would).
    assert_eq!(pins.schema_pin, deacon_conformance::CURRENT_SCHEMA_PIN);
    assert_eq!(pins.prose_pin, deacon_conformance::CURRENT_SPEC_PIN);
    assert!(
        pins.oracle_version.is_some(),
        "the registry must record an oracle revision, or oracle pin-staleness is not \
         decidable and every finding's oracle claim goes unchecked"
    );
}

/// The data root is a **sibling** of the registry, and nothing in the registry loader
/// reaches it. This is the structural half of the guarantee that an unreviewed finding
/// can never influence `certify` (research D6) — asserted rather than assumed, because
/// the failure mode is silent: a finding quietly joining the certification denominator.
#[test]
fn the_discovery_root_is_outside_the_registry_and_unreachable_from_it() {
    let registry_dir = deacon_conformance::default_registry_dir();
    let discovery_dir = deacon_conformance::default_discovery_dir();

    assert!(
        !discovery_dir.starts_with(&registry_dir),
        "the discovery root {discovery_dir:?} must not sit inside the registry \
         {registry_dir:?} — placing it there means either the loader rejects it or \
         someone wires it in and unreviewed findings reach `certify`"
    );
    assert_eq!(
        discovery_dir.parent(),
        registry_dir.parent(),
        "the discovery root must be a SIBLING of the registry under conformance/"
    );

    // Loading the registry must not surface any discovery record. There is no field
    // that could hold one, which is the point; assert on the observable consequence.
    let registry = Registry::load(&registry_dir).expect("registry loads");
    let discovery = load_discovery();
    for finding in &discovery.findings {
        assert!(
            !registry.cases.iter().any(|c| c.id == finding.id),
            "a finding id must never appear as a registry case id"
        );
    }
}

/// The one reference that crosses the root boundary points **out** of discovery into the
/// registry (`Finding.promotedTo → case-<id>`). Nothing in the registry points back, so
/// following references from the registry can never arrive at a finding.
#[test]
fn the_only_cross_root_reference_points_out_of_the_queue() {
    let registry = load_registry();
    let discovery = load_discovery();

    for finding in &discovery.findings {
        if let Some(case_id) = &finding.promoted_to {
            assert!(
                registry.cases.iter().any(|c| &c.id == case_id),
                "promoted finding {} names case `{case_id}`, which does not resolve — the \
                 queue must never claim coverage that does not exist",
                finding.id
            );
        }
    }
}

/// **T024**: this guard runs in the fast lane, and the live campaign binaries do not run
/// in any pull-request lane.
///
/// Evaluated against the real `.config/nextest.toml` with the harness's filterset
/// evaluator, so an exclusion written as `not (…)` is honored exactly rather than merely
/// token-matched.
#[test]
fn the_hermetic_guard_runs_in_the_fast_lane() {
    let toml_text = std::fs::read_to_string(workspace_root().join(".config/nextest.toml"))
        .expect("read .config/nextest.toml");
    let profiles = parse_nextest_profiles(&toml_text).expect("parse .config/nextest.toml");

    for profile in ["default", "dev-fast"] {
        let filter = profiles
            .default_filters
            .get(profile)
            .unwrap_or_else(|| panic!("[profile.{profile}] must exist"));
        for guard in HERMETIC_DISCOVERY_BINARIES {
            match filter {
                // No default-filter means "select everything", which includes the guards.
                None => {}
                Some(expr) => assert!(
                    filter_selects(expr, guard)
                        .unwrap_or_else(|e| panic!("[profile.{profile}] filter: {e}")),
                    "[profile.{profile}] must select `{guard}`: it is a guard, not a \
                     campaign, and a guard that does not run in the fast lane is a guard \
                     nobody notices going stale"
                ),
            }
        }
    }
}

/// **T006/T007**: the live campaign binaries are selected by `[profile.discovery]` and by
/// no pull-request profile.
///
/// The allow-list must also NOT capture this guard — the `discovery_*` glob mistake
/// research D9 exists to prevent.
#[test]
fn live_discovery_binaries_are_selected_by_exactly_one_lane() {
    let toml_text = std::fs::read_to_string(workspace_root().join(".config/nextest.toml"))
        .expect("read .config/nextest.toml");
    let profiles = parse_nextest_profiles(&toml_text).expect("parse .config/nextest.toml");

    let discovery_filter = profiles
        .default_filters
        .get("discovery")
        .expect("nextest.toml must declare [profile.discovery] — discovery has no lane otherwise")
        .as_deref()
        .expect(
            "[profile.discovery] must declare a default-filter; without one it selects every \
             binary in the workspace",
        );

    for name in LIVE_DISCOVERY_BINARIES {
        assert!(
            filter_selects(discovery_filter, name).expect("evaluate the discovery filter"),
            "[profile.discovery] must select live binary `{name}`"
        );
    }
    for guard in HERMETIC_DISCOVERY_BINARIES {
        assert!(
            !filter_selects(discovery_filter, guard).expect("evaluate the discovery filter"),
            "[profile.discovery] must NOT capture the hermetic guard `{guard}` — that is \
             precisely the `discovery_*` glob mistake the explicit allow-list exists to \
             avoid (research D9)"
        );
    }

    for profile in PULL_REQUEST_PROFILES {
        let filter = profiles
            .default_filters
            .get(profile)
            .unwrap_or_else(|| panic!("[profile.{profile}] must exist"));
        let expr = filter.as_deref().unwrap_or_else(|| {
            panic!(
                "[profile.{profile}] has no default-filter, so it selects every binary \
                 including the live discovery campaigns"
            )
        });
        for name in LIVE_DISCOVERY_BINARIES {
            assert!(
                !filter_selects(expr, name)
                    .unwrap_or_else(|e| panic!("[profile.{profile}] filter: {e}")),
                "[profile.{profile}] selects live discovery binary `{name}` — a green \
                 pull-request run must never imply a campaign ran (FR-055/FR-057)"
            );
        }
    }
}

/// The discovery command group is **dev-only**. It must not reach the shipped consumer
/// CLI (FR-059, constitution II).
///
/// Asserted against the **subcommand column** of `deacon --help`, not against the source
/// text: `discovery` is an ordinary English word that already appears in several flag
/// descriptions ("auto-discovery of corporate root CAs", "container identity and
/// discovery"), so a substring search over `cli.rs` would fail on prose that has nothing
/// to do with this feature. The same parsing discipline `parity_registry_check` uses,
/// and for the same reason.
#[test]
fn the_discovery_surface_never_reaches_the_shipped_cli() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_deacon"))
        .arg("--help")
        .output()
        .expect("the deacon binary runs");
    assert!(output.status.success(), "`deacon --help` must succeed");
    let help = String::from_utf8_lossy(&output.stdout);

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
    assert!(
        !subcommands.contains(&"discovery"),
        "`discovery` reached the shipped consumer CLI; it is contributor tooling and a \
         conformance-tracking command in the consumer surface is a scope violation that \
         ships to users and is then hard to withdraw. Subcommands found: {subcommands:?}"
    );
}
