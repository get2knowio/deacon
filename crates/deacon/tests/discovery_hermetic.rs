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

use deacon_conformance::discovery::grammar::Grammar;
use deacon_conformance::discovery::queue::{self, CampaignsFile, DiscoveryData, FindingsFile};
use deacon_conformance::discovery::report as discovery_report;
use deacon_conformance::discovery::rng::Prng;
use deacon_conformance::discovery::signature::{Divergence, DivergenceKind, Signature};
use deacon_conformance::load::Registry;
// `PULL_REQUEST_PROFILES` is the harness's single definition of "every lane a pull
// request runs through", shared with `parity_registry_check`. Two copies of a six-element
// list is exactly the kind of thing that drifts silently: a seventh profile added to one
// copy and not the other would leave a lane nobody checks, which is indistinguishable
// from a lane that checks out.
use parity_harness::registry::{PULL_REQUEST_PROFILES, filter_selects, parse_nextest_profiles};

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

    for &profile in PULL_REQUEST_PROFILES {
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

// ---------------------------------------------------------------------------
// T053 (US3, SC-013) — the hermetic surface cannot reach the network
// ---------------------------------------------------------------------------

/// Every source file that makes up the hermetic discovery surface.
///
/// Returned as `(relative path, contents)` so a failure names the file. The list is
/// enumerated from disk rather than hard-coded: a module added by a later user story
/// joins the scan automatically, which is the only way a guard like this keeps up with
/// the thing it guards.
fn hermetic_discovery_sources() -> Vec<(String, String)> {
    let dir = workspace_root().join("crates/conformance/src/discovery");
    let mut out = Vec::new();
    let rd = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {dir:?}: {e}"));
    for entry in rd.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        out.push((format!("crates/conformance/src/discovery/{name}"), text));
    }
    out.sort();
    out
}

/// **T053 / SC-013**: the hermetic discovery surface completes with zero network requests.
///
/// "Zero network requests" is asserted the strongest way available — as an **absent
/// capability**, not as an unobserved behaviour. A run that merely *happened* not to make
/// a request proves nothing about the next run with different data, and sandboxing a Rust
/// test's sockets from inside the test is not something the test can honestly do. So the
/// claim is established in three parts, and each covers a way the other two can be
/// evaded:
///
/// 1. **The crate declares no way to speak a network protocol.** `deacon-conformance`
///    owns the entire hermetic surface (grammar, PRNG, signature, queue, report) and
///    depends on no HTTP client, no async runtime, and no git/ssh transport. This is the
///    same structural argument `clause_determinism` makes for the clause commands, taken
///    here for the discovery ones; the git/ssh entries are specific to discovery, because
///    the corpus canary is the one part of this feature that legitimately fetches — and
///    it lives in `parity-harness`, deliberately on the other side of the seam (D8).
/// 2. **No discovery module shells out.** A dependency audit is blind to
///    `Command::new("curl")` and `git fetch`, which is exactly how a hermetic module
///    would most plausibly acquire the network by accident: the corpus manifest model
///    lives here while the fetch lives in the live half, so the tempting shortcut is one
///    line away at all times.
/// 3. **The surface actually runs.** A capability argument about code nobody executes is
///    worth little, so the whole hermetic path is exercised end to end below. Together
///    with (1) and (2) that is what "completes with zero network requests" means here.
#[test]
fn the_hermetic_discovery_surface_cannot_reach_the_network() {
    // --- 1. No network-capable dependency in the crate that owns the surface ----------
    let manifest_path = workspace_root().join("crates/conformance/Cargo.toml");
    let manifest = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("read {manifest_path:?}: {e}"));
    // Inspect dependency-declaration lines only (`name = …`), so prose in a comment
    // never trips the check: the guarantee is about actual dependencies.
    let declared: Vec<String> = manifest
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('=').map(|(name, _)| name.trim().to_string()))
        .collect();
    for forbidden in [
        // HTTP clients.
        "reqwest",
        "hyper",
        "ureq",
        "curl",
        "isahc",
        "surf",
        "attohttpc",
        "http",
        // Async runtimes — the substrate a socket would need.
        "tokio",
        "async-std",
        "smol",
        // Git / ssh transports: the corpus canary's fetch belongs in `parity-harness`
        // (research D8), never in the hermetic half that models the manifest.
        "git2",
        "gix",
        "ssh2",
        "russh",
    ] {
        assert!(
            !declared.iter().any(|d| d == forbidden),
            "deacon-conformance must not depend on {forbidden:?}: it owns the hermetic \
             discovery surface, whose no-network guarantee is that the capability is \
             ABSENT rather than merely unused (SC-013)"
        );
    }

    // --- 2. No discovery module spawns a process or opens a socket -------------------
    let sources = hermetic_discovery_sources();
    assert!(
        sources.len() >= 10,
        "expected to scan the whole hermetic discovery surface, only saw {} file(s) — a \
         scan that silently stopped finding files would pass by checking nothing",
        sources.len()
    );
    for required in [
        "queue.rs",
        "grammar.rs",
        "rng.rs",
        "signature.rs",
        "report.rs",
    ] {
        assert!(
            sources.iter().any(|(p, _)| p.ends_with(required)),
            "the scan must cover {required}; got {:?}",
            sources.iter().map(|(p, _)| p).collect::<Vec<_>>()
        );
    }

    /// Tokens that would give a hermetic module a way off the machine. Matched on
    /// non-comment lines only, so a doc comment may still *discuss* `git fetch`.
    const FORBIDDEN: &[&str] = &[
        "std::net",
        "TcpStream",
        "TcpListener",
        "UdpSocket",
        "std::process",
        "tokio::process",
        "Command::new",
        "reqwest",
        "ureq",
        "hyper::",
        "git2",
        "gix::",
    ];
    let mut problems = Vec::new();
    for (path, text) in &sources {
        for (line_no, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") || code.starts_with("/*") || code.starts_with('*') {
                continue;
            }
            for needle in FORBIDDEN {
                if code.contains(needle) {
                    problems.push(format!(
                        "{path}:{}: uses `{needle}` — the hermetic half must have no way to \
                         reach the network, and shelling out is the one route a dependency \
                         audit cannot see. Fetching belongs in \
                         `parity_harness::discovery::corpus_fetch` (research D8).",
                        line_no + 1
                    ));
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "the hermetic discovery surface must not spawn processes or open sockets:\n{}",
        problems.join("\n")
    );

    // --- 3. And the surface really runs -----------------------------------------------
    // Grammar: the pinned constraint inventory, read from disk.
    let grammar = Grammar::load_default().expect("the committed grammar must load");
    assert!(
        !grammar.is_empty(),
        "an empty grammar would make every later assertion in this test vacuous"
    );

    // PRNG: a seeded draw, entirely in-process.
    let mut prng = Prng::from_seed(0x5eed_1234);
    let first = prng.next_u64();
    assert_eq!(
        Prng::from_seed(0x5eed_1234).next_u64(),
        first,
        "the stream is a property of committed code, not of the environment"
    );

    // Signature: derived from an in-memory divergence, never re-diffed.
    let deacon = serde_json::json!("vscode");
    let reference = serde_json::json!("root");
    let signature = Signature::derive(
        "chan-structured-output",
        &Divergence {
            kind: DivergenceKind::Value,
            path: "configuration.remoteUser",
            deacon: Some(&deacon),
            reference: Some(&reference),
        },
    );
    assert!(signature.finding_id().starts_with("fnd-"));

    // Queue + report: load the committed data root, validate it, render the artifacts.
    let registry = load_registry();
    let data = load_discovery();
    assert!(queue::check(&data, &queue::RegistryView::from_registry(&registry)).is_empty());
    let report = discovery_report::build_queue_report(
        &data,
        &discovery_report::CurrentPins::from_registry(&registry),
    );
    assert!(!discovery_report::render_md(&report).is_empty());
}

// ---------------------------------------------------------------------------
// T055 (US3, SC-014) — the surface never gates on what it found
// ---------------------------------------------------------------------------

/// Build a queue holding `paths.len()` untriaged findings plus the campaign that admitted
/// them, using the real derived ids and the registry's real pins so it validates clean.
///
/// Deliberately routed through the strict loader (`FindingsFile` / `CampaignsFile`)
/// rather than constructed as structs: the loader is part of the hermetic surface, and a
/// fixture that bypassed it could assert a shape the real data root can never hold.
fn synthetic_queue(registry: &Registry, paths: &[&str]) -> DiscoveryData {
    let pins = discovery_report::CurrentPins::from_registry(registry);
    let oracle = pins
        .oracle_version
        .clone()
        .expect("the registry must record an oracle revision");
    let campaign_id = "cmp-aaaaaaaa";

    let mut records = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        let deacon = serde_json::json!("vscode");
        let reference = serde_json::json!("root");
        let signature = Signature::derive(
            "chan-structured-output",
            &Divergence {
                kind: DivergenceKind::Value,
                path,
                deacon: Some(&deacon),
                reference: Some(&reference),
            },
        );
        let candidate_id = format!("cnd-0000000{index}");
        records.push(serde_json::json!({
            "id": signature.finding_id(),
            "signature": signature,
            "witnesses": [{
                "id": queue::Witness::derived_id(campaign_id, &candidate_id),
                "campaignId": campaign_id,
                "candidateId": candidate_id,
                "minimalInput": { "image": "alpine:3.18" },
                "isMinimal": true,
                "reductionSteps": ["drop-optional-key"],
                "observedValues": { "deacon": "vscode", "reference": "root" },
                "mutationOperators": ["mop-wrong-type"]
            }],
            "classification": null,
            "state": "untriaged",
            "firstObserved": campaign_id,
            "lastObserved": campaign_id,
            "promotedTo": null,
            "splitFrom": null,
            "notes": ""
        }));
    }

    let findings: FindingsFile = serde_json::from_value(serde_json::json!({
        "schemaVersion": queue::SCHEMA_VERSION,
        "records": records,
    }))
    .expect("the synthetic findings must satisfy the strict loader");

    let campaigns: CampaignsFile = serde_json::from_value(serde_json::json!({
        "schemaVersion": queue::SCHEMA_VERSION,
        "records": [{
            "id": campaign_id,
            "seed": "0x5eed1234",
            "lane": "scheduled",
            "tier": "config-differential",
            "pinnedInputSet": {
                "schemaPin": pins.schema_pin,
                "prosePin": pins.prose_pin,
                "oracleVersion": oracle,
                "normalizerVersion": deacon_conformance::snapshot::NORMALIZER_VERSION,
                "grammarVersion": Grammar::load_default()
                    .expect("grammar loads")
                    .revision()
                    .to_string(),
                "mutationCatalogVersion": "v1",
                "generatorVersion": deacon_conformance::discovery::rng::prng_identity()
            },
            "budget": {
                "wallClockSeconds": queue::DEFAULT_WALL_CLOCK_SECONDS,
                "perCandidateSeconds": queue::DEFAULT_PER_CANDIDATE_SECONDS_HERMETIC,
                "shrinkStepsPerFinding": 64,
                "admissionCap": queue::DEFAULT_ADMISSION_CAP
            },
            "outcome": {
                "candidatesGenerated": 4820,
                "candidatesExecuted": 4629,
                "candidatesDiscardedUnsafe": 0,
                "parseStageFailures": 191,
                "budgetExhausted": false,
                "spaceCoveredFraction": 0.0,
                "mutationApplications": { "unknown-field": 512 },
                "signaturesObserved": paths.len(),
                "signaturesAdmitted": paths.len(),
                "signaturesSuppressed": 0
            }
        }]
    }))
    .expect("the synthetic campaign must satisfy the strict loader");

    DiscoveryData {
        findings: findings.records,
        campaigns: campaigns.records,
    }
}

/// **T055 / SC-014**: a queue full of findings and an empty one are indistinguishable in
/// *outcome* — both validate clean and both render — while remaining entirely
/// distinguishable in *content*.
///
/// That pairing is the whole rule. If only the first half held, "nothing found" and
/// "nothing ran" would look alike and the most comfortable way for the machinery to be
/// broken would be to report success forever (FR-062). If only the second held, discovery
/// would be a gate, and a stochastic gate makes green non-reproducible — at which point
/// somebody eventually turns the lane off.
///
/// Asserted at the library level here because this binary lives in the `deacon` crate and
/// has no handle on the `deacon-conformance` executable. The **process**-level contract —
/// `discovery report` exiting `0` on a populated queue, `discovery check` exiting `1` only
/// on a violation — is asserted from outside the process by
/// `crates/conformance/tests/discovery_cli.rs`, in the crate that owns the binary. The
/// two halves are cross-checked below so neither can be deleted while the other keeps
/// implying it is covered.
#[test]
fn a_populated_queue_and_an_empty_one_are_equally_clean() {
    let registry = load_registry();
    let view = queue::RegistryView::from_registry(&registry);
    let pins = discovery_report::CurrentPins::from_registry(&registry);

    let empty = DiscoveryData::default();
    let populated = synthetic_queue(
        &registry,
        &[
            "configuration.remoteUser",
            "configuration.workspaceFolder",
            "configuration.userEnvProbe",
        ],
    );

    // Same verdict: no quantity of findings is itself a violation.
    for (label, data) in [("empty", &empty), ("populated", &populated)] {
        let violations = queue::check(data, &view);
        assert!(
            violations.is_empty(),
            "the {label} queue must validate clean — a finding is a candidate for an \
             assertion, never a failure:\n{}",
            violations
                .iter()
                .map(|v| format!("  {} {}: {v}", v.class(), v.record()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    // Same success: both render, and both write their artifacts.
    let dir = tempfile::tempdir().expect("tempdir");
    for (label, data) in [("empty", &empty), ("populated", &populated)] {
        let report = discovery_report::build_queue_report(data, &pins);
        let out = dir.path().join(label);
        std::fs::create_dir_all(&out).expect("create out dir");
        let written = discovery_report::write_queue_report(&out, &report)
            .unwrap_or_else(|e| panic!("the {label} queue must render: {e}"));
        assert_eq!(written.len(), 2, "queue.json + queue.md");
        assert!(
            discovery_report::render_md(&report).contains("never gates"),
            "the {label} report must say what it is: a triage queue, not a gate"
        );
    }

    // Different content: the counted untriaged bucket distinguishes "nobody has looked
    // yet" from "nothing was found", which is precisely what a status code cannot carry.
    let empty_md =
        discovery_report::render_md(&discovery_report::build_queue_report(&empty, &pins));
    let full_md =
        discovery_report::render_md(&discovery_report::build_queue_report(&populated, &pins));
    assert!(empty_md.contains("| untriaged | 0 |"), "{empty_md}");
    assert!(full_md.contains("| untriaged | 3 |"), "{full_md}");
    assert!(
        full_md.contains("4820"),
        "a campaign's volume is reported whether or not it found anything (FR-062): \
         {full_md}"
    );

    // The process-level half must still exist. Without this, deleting
    // `report_exits_zero_whether_the_queue_is_empty_or_full` would leave the exit-status
    // contract — the rule that makes this lane safe to schedule — asserted nowhere, while
    // this test kept passing and looking like coverage.
    let cli_guard = workspace_root().join("crates/conformance/tests/discovery_cli.rs");
    let cli_guard_text = std::fs::read_to_string(&cli_guard)
        .unwrap_or_else(|e| panic!("the process-level exit-status guard {cli_guard:?}: {e}"));
    assert!(
        cli_guard_text.contains("fn report_exits_zero_whether_the_queue_is_empty_or_full"),
        "{cli_guard:?} must keep asserting the exit-status contract from outside the \
         process; the library-level property proven here does not imply it"
    );
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
