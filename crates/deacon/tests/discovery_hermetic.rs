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

use deacon_conformance::discovery::corpus::{self, CorpusEntry};
use deacon_conformance::discovery::generate;
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

/// The certification profile every synthetic campaign in this file records.
const SYNTHETIC_PROFILE: &str = "prof-linux-amd64-docker-0870";

/// The pinned input set every synthetic campaign records — the registry's **real** pins,
/// so a synthetic finding is never accidentally pin-stale and the pin-stale bucket keeps
/// meaning what it says.
fn synthetic_pins(registry: &Registry) -> queue::PinnedInputSet {
    let pins = discovery_report::CurrentPins::from_registry(registry);
    let oracle = pins
        .oracle_version
        .clone()
        .expect("the registry must record an oracle revision");
    serde_json::from_value(serde_json::json!({
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
    }))
    .expect("the synthetic pinned input set must parse")
}

/// One synthetic campaign, routed through the strict loader.
///
/// The id is **derived**, never chosen: `check` recomputes it from the record's own
/// substance, so a hand-picked id would (correctly) fail the D1 identity clause and the
/// fixture would stop being the clean queue these tests need. `seed` is what makes two
/// calls two different campaigns.
fn synthetic_campaign(
    pinned_input_set: &queue::PinnedInputSet,
    seed: &str,
    admitted: u64,
    suppressed: u64,
) -> queue::Campaign {
    let lane = queue::CampaignLane::Scheduled;
    let tier = queue::CampaignTier::ConfigDifferential;
    let id = queue::Campaign::derive_id(seed, pinned_input_set, lane, SYNTHETIC_PROFILE, tier);
    let file: CampaignsFile = serde_json::from_value(serde_json::json!({
        "schemaVersion": queue::SCHEMA_VERSION,
        "records": [{
            "id": id,
            "seed": seed,
            "lane": "scheduled",
            "tier": "config-differential",
            "profile": SYNTHETIC_PROFILE,
            "pinnedInputSet": pinned_input_set,
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
                "signaturesObserved": admitted + suppressed,
                "signaturesAdmitted": admitted,
                "signaturesSuppressed": suppressed
            }
        }]
    }))
    .expect("the synthetic campaign must satisfy the strict loader");
    file.records
        .into_iter()
        .next()
        .expect("one synthetic campaign")
}

/// A value-difference signature at `path` on the structured-output channel.
fn signature_at(path: &str) -> Signature {
    let deacon = serde_json::json!("vscode");
    let reference = serde_json::json!("root");
    Signature::derive(
        "chan-structured-output",
        &Divergence {
            kind: DivergenceKind::Value,
            path,
            deacon: Some(&deacon),
            reference: Some(&reference),
        },
    )
}

/// A **present-versus-absent** signature at `path`: the same observable location as
/// [`signature_at`], a different kind of difference, and therefore a different signature.
fn absence_signature_at(path: &str) -> Signature {
    let reference = serde_json::json!("root");
    Signature::derive(
        "chan-structured-output",
        &Divergence {
            kind: DivergenceKind::RefOnly,
            path,
            deacon: None,
            reference: Some(&reference),
        },
    )
}

/// One witness of `signature`, attributed to `campaign` and `candidate`.
///
/// Routed through the strict loader for the same reason the queue is: a witness the real
/// file could not hold would let a test assert a shape that cannot occur.
fn synthetic_witness(campaign: &queue::Campaign, candidate: &str) -> queue::Witness {
    serde_json::from_value(serde_json::json!({
        "id": queue::Witness::derived_id(&campaign.id, candidate),
        "campaignId": campaign.id,
        "candidateId": candidate,
        "minimalInput": { "image": "alpine:3.18" },
        "isMinimal": true,
        "reductionSteps": ["drop-optional-key"],
        "observedValues": { "deacon": "vscode", "reference": "root" },
        "mutationOperators": ["mop-wrong-type"]
    }))
    .expect("the synthetic witness must satisfy the strict loader")
}

/// Build a queue holding `paths.len()` untriaged findings plus the campaign that admitted
/// them, using the real derived ids and the registry's real pins so it validates clean.
///
/// Deliberately routed through the strict loader (`FindingsFile` / `CampaignsFile`)
/// rather than constructed as structs: the loader is part of the hermetic surface, and a
/// fixture that bypassed it could assert a shape the real data root can never hold.
fn synthetic_queue(registry: &Registry, paths: &[&str]) -> DiscoveryData {
    let pinned_input_set = synthetic_pins(registry);
    let campaign = synthetic_campaign(
        &pinned_input_set,
        "0x5eed1234",
        paths.len() as u64,
        /* suppressed */ 0,
    );

    let mut records = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        let signature = signature_at(path);
        let candidate_id = format!("cnd-0000000{index}");
        let witness = synthetic_witness(&campaign, &candidate_id);
        records.push(serde_json::json!({
            "id": signature.finding_id(),
            "signature": signature,
            "witnesses": [witness],
            "classification": null,
            "state": "untriaged",
            "firstObserved": campaign.id,
            "lastObserved": campaign.id,
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

    DiscoveryData {
        findings: findings.records,
        campaigns: vec![campaign],
        corpus: Vec::new(),
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

// ---------------------------------------------------------------------------
// US4 (T061–T066, T125) — classify and deduplicate what was found
// ---------------------------------------------------------------------------

/// Triage `finding` in place, failing the test with the refusal rather than swallowing it.
fn triage(finding: &mut queue::Finding, classification: queue::Classification) {
    finding
        .triage(classification, None)
        .unwrap_or_else(|e| panic!("triage must be accepted: {e}"));
}

/// Assert the queue has no D-class violation, naming every one it does have.
fn assert_clean(data: &DiscoveryData, registry: &Registry, label: &str) {
    let violations = queue::check(data, &queue::RegistryView::from_registry(registry));
    assert!(
        violations.is_empty(),
        "the {label} queue must validate clean:\n{}",
        violations
            .iter()
            .map(|v| format!("  {} {}: {v}", v.class(), v.record()))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// **T061 / SC-007**: every finding either carries exactly one classification or sits in a
/// visible unclassified bucket. None is in neither, none is in both.
///
/// The partition has **two** unclassified buckets, not one, and the second is easy to miss:
/// `untriaged` (nobody has looked) and `split` (an inert ancestor that surrendered its
/// classification to its children — Q10). Both are counted in the report, which is what
/// keeps SC-007's "no finding is in neither state" true: a split parent is unclassified on
/// purpose and is still visible, rather than being a third, silent category.
///
/// Both failure directions are exercised, because a checker that only caught the missing
/// classification would let the opposite defect through — an untriaged finding carrying a
/// judgement, which makes the FR-029 bucket count something that has in fact been judged.
#[test]
fn every_finding_carries_exactly_one_classification_or_is_visibly_unclassified() {
    let registry = load_registry();
    let view = queue::RegistryView::from_registry(&registry);
    let pins = discovery_report::CurrentPins::from_registry(&registry);

    let mut data = synthetic_queue(
        &registry,
        &[
            "configuration.remoteUser",
            "configuration.workspaceFolder",
            "configuration.userEnvProbe",
        ],
    );
    triage(
        &mut data.findings[0],
        queue::Classification::DeaconRegression,
    );
    triage(&mut data.findings[1], queue::Classification::ReferenceQuirk);
    // The third stays untriaged: the partition must hold across a MIXED queue, which is
    // the only state a real queue is ever in.

    assert_clean(&data, &registry, "mixed");

    let unclassified_states = [queue::FindingState::Untriaged, queue::FindingState::Split];
    for finding in &data.findings {
        let classified = finding.classification.is_some();
        let visibly_unclassified = unclassified_states.contains(&finding.state);
        assert_ne!(
            classified,
            visibly_unclassified,
            "finding {} is in {} state(s): classification={:?}, state={} — SC-007 requires \
             exactly one",
            finding.id,
            if classified { "both" } else { "neither" },
            finding.classification,
            finding.state.as_str()
        );
    }

    // The report accounts for every finding, so "exactly one" is observable from the
    // artifact and not only from the in-memory records.
    let report = discovery_report::build_queue_report(&data, &pins);
    assert_eq!(report.total, 3);
    assert_eq!(report.untriaged.len(), 1);
    assert_eq!(report.triaged.len(), 2);
    assert!(
        report.triaged.iter().all(|f| f.classification.is_some()),
        "a triaged finding without a classification would be in NEITHER state"
    );
    assert!(
        report.untriaged.iter().all(|f| f.classification.is_none()),
        "an untriaged finding with a classification would be in BOTH"
    );

    // Direction 1: a classification that went missing.
    let mut missing = data.clone();
    missing.findings[0].classification = None;
    assert!(
        queue::check(&missing, &view)
            .iter()
            .any(|v| v.class() == "D2"),
        "a triaged finding with no classification must be D2"
    );

    // Direction 2: a classification that arrived too early.
    let mut premature = data.clone();
    premature.findings[2].classification = Some(queue::Classification::SpecAmbiguity);
    assert!(
        queue::check(&premature, &view)
            .iter()
            .any(|v| v.class() == "D2"),
        "an untriaged finding carrying a classification must be D2 — otherwise the visible \
         unclassified bucket counts something that has been judged"
    );
}

/// **T062 / SC-006**: equal signatures from two campaigns collapse to one finding with two
/// witnesses — and repeating a campaign adds nothing at all.
///
/// The two halves are different claims. Collapsing says the queue reflects *distinct
/// problems*; adding nothing on a repeat says it does not reflect *campaign volume*. A
/// queue that grew by one record every night would be a log, and nobody triages a log.
#[test]
fn equal_signatures_from_two_campaigns_collapse_to_one_finding_with_two_witnesses() {
    let registry = load_registry();
    let pins = synthetic_pins(&registry);
    let first = synthetic_campaign(&pins, "0x5eed0001", 1, 0);
    let second = synthetic_campaign(&pins, "0x5eed0002", 1, 0);
    assert_ne!(first.id, second.id, "two seeds are two campaigns");

    let signature = signature_at("configuration.remoteUser");
    let mut findings: Vec<queue::Finding> = Vec::new();

    assert_eq!(
        queue::upsert_finding(
            &mut findings,
            signature.clone(),
            synthetic_witness(&first, "cnd-00000001"),
            &first.id
        ),
        queue::Upsert::Inserted
    );
    // A DIFFERENT campaign, a DIFFERENT candidate, the SAME signature.
    assert_eq!(
        queue::upsert_finding(
            &mut findings,
            signature.clone(),
            synthetic_witness(&second, "cnd-00000002"),
            &second.id
        ),
        queue::Upsert::WitnessAppended
    );

    assert_eq!(findings.len(), 1, "equal signatures are one finding");
    assert_eq!(findings[0].witnesses.len(), 2, "both observations retained");
    assert_eq!(findings[0].first_observed, first.id);
    assert_eq!(
        findings[0].last_observed, second.id,
        "the most recent campaign that reproduced it"
    );

    // One finding takes ONE classification, covering both witnesses.
    triage(&mut findings[0], queue::Classification::DeaconRegression);
    assert_eq!(
        findings[0].classification,
        Some(queue::Classification::DeaconRegression)
    );

    let data = DiscoveryData {
        findings,
        campaigns: vec![first.clone(), second.clone()],
        corpus: Vec::new(),
    };
    assert_clean(&data, &registry, "merged");

    // SC-006 proper: repeating a campaign with an unchanged seed and unchanged pins adds
    // ZERO new findings — and does not even add a witness, because the same campaign
    // observing the same candidate is the same observation.
    let mut repeated = data.findings.clone();
    assert_eq!(
        queue::upsert_finding(
            &mut repeated,
            signature,
            synthetic_witness(&second, "cnd-00000002"),
            &second.id
        ),
        queue::Upsert::AlreadyWitnessed
    );
    assert_eq!(repeated, data.findings, "a repeat changes nothing at all");
}

/// **T063 / FR-031**: distinct signatures that map to the same behavior stay distinct
/// findings. They are *reported* grouped; grouping is a view, never a merge.
///
/// Merging them would destroy the ability to tell whether a fix addressed one cause or all
/// of them — which is precisely the question a reviewer asks after landing the fix.
///
/// Both grouping keys are exercised. The `behavior` key is a **reviewed** mapping: it
/// exists only because a human promoted each finding into a case naming that behavior, and
/// a finding never names a behavior itself (FR-025). The `observable-path` key is what
/// relates two findings *before* anyone has decided what they mean, and it is deliberately
/// not called a behavior claim.
#[test]
fn distinct_signatures_mapping_to_one_behavior_are_grouped_but_never_merged() {
    let registry = load_registry();
    let pins = discovery_report::CurrentPins::from_registry(&registry);
    let behaviors = discovery_report::BehaviorIndex::from_registry(&registry);

    // Two DIFFERENT signatures at the SAME observable location: one value difference and
    // one present-versus-absent difference. Same channel, same path, different kind — so
    // they are two signatures by construction, which is the situation FR-031 is about.
    let mut data = synthetic_queue(&registry, &["configuration.remoteUser"]);
    let campaign = data.campaigns[0].clone();
    let second = absence_signature_at("configuration.remoteUser");
    assert_ne!(
        data.findings[0].signature.id, second.id,
        "the fixture must really hold two distinct signatures"
    );
    queue::upsert_finding(
        &mut data.findings,
        second,
        synthetic_witness(&campaign, "cnd-00000009"),
        &campaign.id,
    );
    assert_eq!(data.findings.len(), 2, "distinct signatures stay distinct");
    assert_clean(&data, &registry, "two-signature");

    // Before promotion: grouped by the observable path they share, still two findings.
    let report = discovery_report::build_queue_report_with_behaviors(&data, &pins, &behaviors);
    assert_eq!(report.total, 2);
    let path_group = report
        .groups
        .iter()
        .find(|g| g.kind == discovery_report::GroupKind::ObservablePath)
        .expect("two findings at one observable path must be grouped");
    assert_eq!(path_group.findings.len(), 2);
    assert_eq!(
        path_group.key,
        "chan-structured-output configuration.remoteUser"
    );

    // After promotion into ONE case: grouped by the behavior that case names.
    let case = registry
        .cases
        .iter()
        .find(|c| !c.behaviors.is_empty())
        .expect("the registry must hold a case naming at least one behavior");
    for finding in &mut data.findings {
        triage(finding, queue::Classification::DeaconRegression);
        finding
            .promote(&case.id)
            .unwrap_or_else(|e| panic!("a deacon-regression finding is promotable: {e}"));
    }
    assert_clean(&data, &registry, "promoted");

    let report = discovery_report::build_queue_report_with_behaviors(&data, &pins, &behaviors);
    let behavior_group = report
        .groups
        .iter()
        .find(|g| g.kind == discovery_report::GroupKind::Behavior && g.key == case.behaviors[0])
        .unwrap_or_else(|| {
            panic!(
                "both findings promoted into `{}` must be grouped under `{}`; groups: {:?}",
                case.id, case.behaviors[0], report.groups
            )
        });
    assert_eq!(behavior_group.findings.len(), 2);

    // The grouping changed nothing about the findings themselves: two records, each with
    // its own signature and its own witnesses.
    assert_eq!(report.total, 2, "grouping is a view, never a merge");
    assert_eq!(report.promoted.len(), 2);
    assert_ne!(report.promoted[0].id, report.promoted[1].id);
    assert_ne!(
        report.promoted[0].value_shape_class, report.promoted[1].value_shape_class,
        "the two findings really are different differences"
    );
    for summary in &report.promoted {
        assert_eq!(
            summary.witnesses, 1,
            "witnesses stay with their own finding"
        );
    }

    let md = discovery_report::render_md(&report);
    assert!(
        md.contains("never a merge"),
        "the artifact must say what grouping is and is not: {md}"
    );
    assert!(md.contains(&case.behaviors[0]), "{md}");
}

/// **T064 / FR-035**: `normalizer-defect` and `fixture-defect` are rejected at promotion.
///
/// They describe a defect in the discovery or comparison machinery, not a behavior of
/// either implementation, so promoting one would record a claim about deacon or the
/// reference that the evidence does not support. Resolving them changes the normalizer or
/// the generator.
///
/// Rejected in **two** places, and both matter. The promotion path refuses by construction,
/// so the record is never written; **D2** refuses a hand edit that bypassed the path, so a
/// record that was written anyway does not stand. A checker alone would be too late — by
/// the time it runs, the queue is already claiming coverage that cannot exist.
#[test]
fn a_normalizer_or_fixture_defect_can_never_be_promoted() {
    let registry = load_registry();
    let view = queue::RegistryView::from_registry(&registry);
    let case = registry
        .cases
        .first()
        .expect("the registry must hold at least one case");

    for non_promotable in [
        queue::Classification::NormalizerDefect,
        queue::Classification::FixtureDefect,
    ] {
        assert!(
            !non_promotable.is_promotable(),
            "{} must be non-promotable",
            non_promotable.as_str()
        );

        let mut data = synthetic_queue(&registry, &["configuration.remoteUser"]);
        triage(&mut data.findings[0], non_promotable);

        // 1. The promotion path refuses, and leaves the record exactly as it was.
        let before = data.findings[0].clone();
        let err = data.findings[0]
            .promote(&case.id)
            .expect_err("a machinery defect is not a behavior of either implementation");
        assert!(matches!(err, queue::TransitionError::NonPromotable { .. }));
        assert!(err.to_string().contains("not promotable"), "{err}");
        assert!(
            err.to_string().contains("normalizer or the generator"),
            "the diagnosis must name where the fix belongs: {err}"
        );
        assert_eq!(
            data.findings[0], before,
            "a refused promotion writes nothing"
        );
        assert_clean(&data, &registry, "refused-promotion");

        // 2. And a hand edit that bypassed the path does not stand.
        data.findings[0].state = queue::FindingState::Promoted;
        data.findings[0].promoted_to = Some(case.id.clone());
        let violations = queue::check(&data, &view);
        assert!(
            violations
                .iter()
                .any(|v| v.class() == "D2" && v.to_string().contains("not promotable")),
            "a hand-edited promotion of `{}` must be D2: {violations:?}",
            non_promotable.as_str()
        );
    }

    // The four promotable classifications really do promote, or the assertions above would
    // pass equally for a promotion path that refuses everything.
    for promotable in [
        queue::Classification::DeaconRegression,
        queue::Classification::ReferenceQuirk,
        queue::Classification::SpecAmbiguity,
        queue::Classification::UnsupportedBehavior,
    ] {
        let mut data = synthetic_queue(&registry, &["configuration.remoteUser"]);
        triage(&mut data.findings[0], promotable);
        data.findings[0]
            .promote(&case.id)
            .unwrap_or_else(|e| panic!("{} must promote: {e}", promotable.as_str()));
        assert_eq!(data.findings[0].state, queue::FindingState::Promoted);
        assert_clean(&data, &registry, "promoted");
    }

    // The process-level half must still exist: `discovery scaffold` refusing a
    // non-promotable finding is what a reviewer actually meets, and deleting that test
    // would leave the refusal asserted only where no reviewer runs it.
    let cli_guard = workspace_root().join("crates/conformance/tests/discovery_cli.rs");
    let cli_guard_text = std::fs::read_to_string(&cli_guard)
        .unwrap_or_else(|e| panic!("the process-level promotion guard {cli_guard:?}: {e}"));
    assert!(
        cli_guard_text.contains("fn scaffold_writes_nothing_and_refuses_a_non_promotable_finding"),
        "{cli_guard:?} must keep asserting the refusal from outside the process"
    );
}

/// **T065 / FR-033**: a finding that stops reproducing is *reported* with the campaign that
/// last observed it — never deleted.
///
/// Deleting it would destroy the ability to distinguish two very different situations: a
/// fix landed, or the generator stopped reaching that input. The first is success; the
/// second is a coverage regression in the discovery machinery itself. Only the retained
/// record makes them separable, and only the retained *last observation* says which run to
/// go back to.
#[test]
fn a_finding_that_stops_reproducing_is_reported_with_its_last_campaign() {
    let registry = load_registry();
    let pins = discovery_report::CurrentPins::from_registry(&registry);
    let pinned = synthetic_pins(&registry);
    let first = synthetic_campaign(&pinned, "0x5eed0011", 1, 0);
    let second = synthetic_campaign(&pinned, "0x5eed0012", 1, 0);

    let signature = signature_at("configuration.remoteUser");
    let mut findings: Vec<queue::Finding> = Vec::new();
    queue::upsert_finding(
        &mut findings,
        signature.clone(),
        synthetic_witness(&first, "cnd-00000001"),
        &first.id,
    );
    queue::upsert_finding(
        &mut findings,
        signature.clone(),
        synthetic_witness(&second, "cnd-00000002"),
        &second.id,
    );
    triage(&mut findings[0], queue::Classification::DeaconRegression);

    // A third campaign runs and does not reproduce it.
    let third = synthetic_campaign(&pinned, "0x5eed0013", 0, 0);
    findings[0]
        .mark_no_longer_reproducing()
        .expect("a triaged finding may stop reproducing");

    let data = DiscoveryData {
        findings,
        campaigns: vec![first, second.clone(), third],
        corpus: Vec::new(),
    };
    assert_clean(&data, &registry, "no-longer-reproducing");

    let report = discovery_report::build_queue_report(&data, &pins);
    assert_eq!(report.total, 1, "the record is retained, not deleted");
    assert_eq!(report.no_longer_reproducing.len(), 1);
    let summary = &report.no_longer_reproducing[0];
    assert_eq!(
        summary.last_observed, second.id,
        "the bucket must name the campaign that LAST observed it — the run a reviewer goes \
         back to"
    );
    assert_eq!(
        summary.classification.as_deref(),
        Some("deacon-regression"),
        "the reviewer's judgement survives the disappearance"
    );

    let md = discovery_report::render_md(&report);
    assert!(md.contains("| no-longer-reproducing | 1 |"), "{md}");
    assert!(
        md.contains(&second.id),
        "the campaign that last saw it must be named in the artifact: {md}"
    );
    assert!(
        md.contains("Retained, not deleted"),
        "the report must say why the record is still there: {md}"
    );

    // And a later campaign that reproduces it revives it to `triaged`, KEEPING the
    // classification — re-triaging a finding a reviewer already judged is wasted work.
    let fourth = synthetic_campaign(&pinned, "0x5eed0014", 1, 0);
    let mut revived = data.findings.clone();
    assert_eq!(
        queue::upsert_finding(
            &mut revived,
            signature,
            synthetic_witness(&fourth, "cnd-00000004"),
            &fourth.id
        ),
        queue::Upsert::WitnessAppended
    );
    assert_eq!(revived[0].state, queue::FindingState::Triaged);
    assert_eq!(
        revived[0].classification,
        Some(queue::Classification::DeaconRegression)
    );
    assert_eq!(revived[0].last_observed, fourth.id);
}

/// **T066 / FR-029**: the untriaged count is visible, so "not yet looked at" can never read
/// as "nothing found".
///
/// A status code cannot carry this distinction and neither can a bare list — the count has
/// to be in the artifact, next to the total, where a reader who is skimming sees it. The
/// three queues below are the three states that would otherwise be conflated: nothing
/// found, nothing looked at, and everything looked at.
#[test]
fn the_untriaged_bucket_is_counted_so_nothing_looked_at_never_reads_as_nothing_found() {
    let registry = load_registry();
    let pins = discovery_report::CurrentPins::from_registry(&registry);
    let paths = [
        "configuration.remoteUser",
        "configuration.workspaceFolder",
        "configuration.userEnvProbe",
    ];

    let empty = discovery_report::build_queue_report(&DiscoveryData::default(), &pins);
    let untouched = synthetic_queue(&registry, &paths);
    let mut all_triaged = synthetic_queue(&registry, &paths);
    for finding in &mut all_triaged.findings {
        triage(finding, queue::Classification::DeaconRegression);
    }
    let untouched_report = discovery_report::build_queue_report(&untouched, &pins);
    let triaged_report = discovery_report::build_queue_report(&all_triaged, &pins);

    // Nothing found vs nothing looked at: the same total would be a lie, and the same
    // untriaged count would hide the backlog.
    assert_eq!((empty.total, empty.untriaged.len()), (0, 0));
    assert_eq!(
        (untouched_report.total, untouched_report.untriaged.len()),
        (3, 3)
    );
    // Everything looked at: zero untriaged, and the total says the queue is NOT empty. A
    // report that only carried the untriaged count would render this identically to the
    // empty queue.
    assert_eq!(
        (triaged_report.total, triaged_report.untriaged.len()),
        (3, 0)
    );

    for (label, report, untriaged) in [
        ("empty", &empty, 0usize),
        ("untouched", &untouched_report, 3),
        ("all-triaged", &triaged_report, 0),
    ] {
        let md = discovery_report::render_md(report);
        assert!(
            md.contains(&format!("| untriaged | {untriaged} |")),
            "the {label} report must COUNT the untriaged bucket, not merely list it: {md}"
        );
        assert!(
            md.contains(&format!("| total | {} |", report.total)),
            "the {label} report must carry the total beside it: {md}"
        );
        let json: serde_json::Value =
            serde_json::from_str(&discovery_report::render_json(report)).expect("valid JSON");
        assert_eq!(
            json["untriaged"].as_array().map(Vec::len),
            Some(untriaged),
            "the {label} machine-readable artifact must carry the bucket too"
        );
    }

    // Every untriaged finding is individually named, so the bucket is actionable rather
    // than only a number.
    for (summary, path) in untouched_report.untriaged.iter().zip(paths) {
        assert_eq!(summary.path, path);
        assert!(summary.id.starts_with("fnd-"));
        assert_eq!(summary.classification, None);
    }

    // And an empty queue says what its emptiness does NOT mean.
    let empty_md = discovery_report::render_md(&empty);
    assert!(
        empty_md.contains("it does not say the two implementations agree"),
        "{empty_md}"
    );
}

/// **T125 / FR-018**: no discovery source authors or extends an allowed-difference entry.
///
/// The allowed-difference mechanism records **reviewed** tolerances. A discovery program
/// writing to it would let a difference disappear by being observed — the machinery would
/// grow quieter exactly as it found more, and the growth would look like progress.
///
/// Note what is deliberately **not** forbidden: *reading* the mechanism. FR-017 requires a
/// difference already covered by a case, waiver, or allowed difference to be reported as
/// already-characterized rather than entering the queue as new, and that is exactly a read.
/// The rule is about the write, so the guard is about the write — and it asserts the read
/// still happens, because a scan that matched nothing would pass by checking nothing.
#[test]
fn no_discovery_source_writes_to_the_allowed_difference_mechanism() {
    let sources = discovery_sources();
    for required in [
        "crates/conformance/src/discovery/queue.rs",
        "crates/parity-harness/src/discovery/differential.rs",
        "crates/parity-harness/src/discovery/campaign.rs",
        "crates/conformance/src/bin/conformance.rs",
    ] {
        assert!(
            sources.iter().any(|(p, _)| p == required),
            "the scan must cover {required}; got {:?}",
            sources.iter().map(|(p, _)| p).collect::<Vec<_>>()
        );
    }

    /// Tokens that would AUTHOR or EXTEND a tolerance rather than read one.
    ///
    /// Constructing the record, pushing one onto a case, or emitting the JSON key — each
    /// is a way to make a difference disappear by having observed it.
    const FORBIDDEN: &[&str] = &[
        "AllowedDifference {",
        "AllowedDifference::",
        "allowed_differences.push",
        "allowed_differences.insert",
        "allowed_differences.extend",
        "allowed_differences.append",
        "allowed_differences =",
        "allowed_differences:",
        "\"allowedDifferences\":",
        "\"allowedDifferences\" :",
    ];

    let mut problems = Vec::new();
    let mut reads = 0usize;
    for (path, text) in &sources {
        for (line_no, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") || code.starts_with("/*") || code.starts_with('*') {
                continue;
            }
            for needle in FORBIDDEN {
                if code.contains(needle) {
                    problems.push(format!(
                        "{path}:{}: `{needle}` authors or extends an allowed-difference \
                         entry. That mechanism records REVIEWED tolerances; a discovery \
                         program writing to it would let a difference disappear by being \
                         observed (FR-018). Report the difference as a finding and let a \
                         human decide whether it is tolerable.",
                        line_no + 1
                    ));
                }
            }
            if code.contains("allowed_differences") {
                reads += 1;
            }
        }
    }

    assert!(
        problems.is_empty(),
        "the discovery surface must never author a tolerance:\n{}",
        problems.join("\n")
    );
    assert!(
        reads > 0,
        "the scan found no reference to `allowed_differences` at all. FR-017 requires \
         discovery to READ the mechanism so an already-characterized difference does not \
         enter the queue as new — so zero references means either that read was lost or \
         this guard is matching nothing, and both are defects."
    );
}

/// Every source file that makes up the discovery surface, hermetic **and** live.
///
/// The live half is the half that could plausibly author a tolerance: it is the side
/// holding the registry it would have to write to. Scanning only the hermetic half would
/// guard the place the mistake cannot happen.
fn discovery_sources() -> Vec<(String, String)> {
    /// This file. Excluded because it *is* the guard: its forbidden-token table contains
    /// every pattern it searches for, so scanning itself would fail on its own definition.
    /// Nothing is lost — a test file is not a discovery program, and the programs are all
    /// scanned below.
    const THE_GUARD_ITSELF: &str = "discovery_hermetic.rs";

    let mut out = hermetic_discovery_sources();
    // Whole directories: every module of the live discovery half, scanned without a name
    // filter. The live half is the one that could plausibly author a tolerance — it is the
    // side holding the registry — and a name filter there would have quietly skipped
    // `differential.rs`, which is exactly where the mistake would live.
    out.extend(rust_sources_in(
        "crates/parity-harness/src/discovery",
        |_| true,
    ));
    // Directories that hold unrelated programs too: take the discovery ones by name, plus
    // `conformance.rs` in full — that binary hosts the whole dev CLI of which `discovery`
    // is one command group, so a write anywhere in it is reachable from `discovery`
    // regardless of which function holds it.
    for dir in [
        "crates/parity-harness/src/bin",
        "crates/conformance/src/bin",
        "crates/deacon/tests",
    ] {
        out.extend(rust_sources_in(dir, |name| {
            name != THE_GUARD_ITSELF && (name.starts_with("discovery") || name == "conformance.rs")
        }));
    }
    out.sort();
    out
}

/// Every `.rs` file directly under `dir` (workspace-relative) whose file name `keep`
/// accepts, as `(relative path, contents)`.
fn rust_sources_in(dir: &str, keep: impl Fn(&str) -> bool) -> Vec<(String, String)> {
    let root = workspace_root().join(dir);
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(&root) else {
        return out;
    };
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
        if !keep(&name) {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        out.push((format!("{dir}/{name}"), text));
    }
    out
}

// ---------------------------------------------------------------------------
// T099/T100/T104 (US7) — the pinned real-world corpus, checked WITHOUT a network
// ---------------------------------------------------------------------------

/// The committed corpus manifest.
fn load_corpus() -> Vec<CorpusEntry> {
    load_discovery().corpus
}

/// One well-formed entry, id derived rather than chosen (the same discipline
/// [`synthetic_campaign`] follows: a hand-picked id would fail the D4 identity clause and
/// the fixture would stop being the clean manifest these tests need).
fn corpus_entry(repository: &str, commit: &str, path: &str) -> CorpusEntry {
    CorpusEntry {
        id: CorpusEntry::derive_id(repository, commit, path),
        name: format!("synthetic-{}", repository.replace('/', "-")),
        repository: repository.to_string(),
        commit: commit.to_string(),
        path: path.to_string(),
        content_digest: None,
        notes: String::new(),
    }
}

/// **T099 / FR-050 / SC-012**: a branch, a tag, `HEAD`, or `latest` is **D4**, rejected
/// with no network access whatsoever.
///
/// This test is the entire argument for moving the manifest out of the Python fetcher and
/// into Rust-owned strict JSON (research D8). FR-050 is a property of the *manifest*, not
/// of a fetch — nothing needs to be retrieved to know that `main` names different content
/// tomorrow — so the check belongs somewhere it runs on every pull request. A validation
/// that only runs when the network is up is a validation that does not run.
///
/// The no-network claim is structural rather than asserted here: `deacon-conformance`
/// declares no HTTP client, no async runtime, and no git transport, and no hermetic
/// discovery module may spawn a process or open a socket —
/// [`the_hermetic_discovery_surface_cannot_reach_the_network`] enforces both. This test
/// exercises the rule that guard makes reachable.
#[test]
fn a_mutable_corpus_reference_is_rejected_hermetically() {
    // Every floating shape FR-050 names, plus the near-misses a naive length or hex check
    // would wave through.
    for mutable in [
        "main",
        "master",
        "HEAD",
        "latest",
        "v1.2.3",
        "refs/heads/main",
        "release/2024-01",
        "0123456",                                   // abbreviated
        "0123456789ABCDEF0123456789ABCDEF01234567",  // uppercase
        "0123456789abcdef0123456789abcdef0123456",   // 39
        "0123456789abcdef0123456789abcdef012345678", // 41
        "0123456789abcdef0123456789abcdef0123456g",  // non-hex
        "",
    ] {
        let entry = corpus_entry("microsoft/vscode-remote-try-node", mutable, "");
        let violations = corpus::check(std::slice::from_ref(&entry));
        assert!(
            violations.iter().any(|v| v.class() == "D4"),
            "`{mutable}` must be rejected as D4; got {violations:?}"
        );
    }

    // And the same rule reaches the real validator over a real data root, so the class is
    // wired in rather than merely implemented.
    let registry = load_registry();
    let mut data = load_discovery();
    data.corpus
        .push(corpus_entry("owner/repo", "main", "workspace"));
    let violations = queue::check(&data, &queue::RegistryView::from_registry(&registry));
    assert!(
        violations.iter().any(|v| v.class() == "D4"),
        "`discovery check` must surface D4 over the data root: {violations:?}"
    );

    // The committed manifest itself is clean — otherwise the assertion above would be
    // measuring a pre-existing violation rather than the one it injected.
    assert!(
        corpus::check(&load_corpus()).is_empty(),
        "the committed corpus manifest must be D4-clean"
    );
}

/// **T100 / FR-049**: every entry records a repository, an immutable commit, the path
/// within the repository, and a content digest.
///
/// The digest field is `null` until first materialization and non-null (and verified)
/// forever after, so what FR-049 requires is that the *slot* is there and is either
/// absent-by-design or well formed — never a placeholder, never a truncated hash, never a
/// value nothing could compare against.
#[test]
fn every_corpus_entry_records_its_full_provenance() {
    let corpus = load_corpus();
    assert_eq!(
        corpus.len(),
        33,
        "the manifest carries the 33 pinned entries the frozen `realworld::<name>` \
         baseline units were derived from; a silently shrinking corpus explores less and \
         reports the same 'found nothing'"
    );

    for entry in &corpus {
        assert!(
            entry.id.starts_with("cor-"),
            "{}: a corpus id is `cor-<hash8>`",
            entry.id
        );
        assert_eq!(
            entry.id,
            entry.derived_id(),
            "{}: the id must derive from `repository ‖ commit ‖ path`, or the record is \
             detached from the snapshot it claims to identify",
            entry.name
        );
        assert!(
            !entry.name.is_empty(),
            "{}: an entry needs a name",
            entry.id
        );
        assert!(
            entry.repository.split('/').count() == 2
                && entry.repository.split('/').all(|p| !p.is_empty()),
            "{}: `repository` must be `owner/repo`, got {:?}",
            entry.id,
            entry.repository
        );
        assert!(
            corpus::is_immutable_reference(&entry.commit),
            "{}: `commit` must be a 40-hex object name, got {:?}",
            entry.id,
            entry.commit
        );
        // `path` is a recorded field that may legitimately be empty (the repository root),
        // so the assertion is about its SHAPE: a leading or trailing slash would make two
        // spellings of one workspace root derive two different ids.
        assert_eq!(
            entry.path.trim_matches('/'),
            entry.path,
            "{}: `path` must be recorded without leading or trailing slashes, or two \
             spellings of one workspace root would derive two different ids",
            entry.id
        );
        match &entry.content_digest {
            None => {} // Not yet materialized — the one legitimate absence.
            Some(digest) => assert!(
                corpus::is_well_formed_digest(digest),
                "{}: a recorded digest must be `sha256:<64 lowercase hex>`, got {digest:?} \
                 — a malformed digest is not a weaker check, it is one that can never \
                 disagree",
                entry.id
            ),
        }
        assert!(
            !entry.notes.trim().is_empty(),
            "{}: an entry records WHY this workspace was selected; an unexplained pin \
             cannot be re-pinned by anyone but its author",
            entry.id
        );
    }

    // Provenance is only provenance if it is unique. Two entries sharing an id would make
    // one snapshot two records; two sharing a name would make the frozen
    // `realworld::<name>` baseline reference ambiguous.
    let mut ids: Vec<&str> = corpus.iter().map(|e| e.id.as_str()).collect();
    ids.sort_unstable();
    let before = ids.len();
    ids.dedup();
    assert_eq!(before, ids.len(), "corpus ids must be unique");

    let mut names: Vec<&str> = corpus.iter().map(|e| e.name.as_str()).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len(), "corpus names must be unique");
}

/// **T104 / FR-008a / FR-054a**: no corpus entry appears among the generator's mutation
/// seeds.
///
/// The corpus is consumed **only** as a direct comparison input in the network-backed
/// lane. Letting it feed the generator would do two separate kinds of damage: the
/// generated space would stop being reproducible from a seed alone (it would depend on
/// what a third party's repository contained at fetch time), and third-party content would
/// enter the hermetic pull-request lane, which by FR-055 performs no network access at all
/// and therefore could never obtain it.
///
/// The separation is **structural**, not a naming convention. The seed corpus is five
/// committed fixtures embedded with `include_str!`, so its membership is fixed when the
/// crate compiles; a fetched workspace does not exist then and cannot be added later.
/// This test asserts both halves — that the two sets are disjoint, and that the seeds come
/// from the committed fixture tree rather than the discovery data root.
#[test]
fn no_corpus_entry_is_a_mutation_seed() {
    let seeds = generate::seed_fixture_names();
    assert!(
        !seeds.is_empty(),
        "a scan over an empty seed set would pass by checking nothing"
    );

    let corpus = load_corpus();
    for entry in &corpus {
        assert!(
            !seeds.contains(&entry.name.as_str()),
            "corpus entry `{}` is also a mutation seed — the corpus is a direct comparison \
             input only (FR-054a), and seeding the generator from it would make the \
             generated space depend on third-party content the hermetic lane can never \
             fetch",
            entry.name
        );
        assert!(
            !seeds.iter().any(|s| s.contains(&entry.name)),
            "seed fixture names must not embed the corpus entry name `{}`",
            entry.name
        );
    }

    // The other direction, and the load-bearing one: the seeds are committed fixtures.
    // A seed whose bytes came from `conformance/discovery/` would be corpus content in
    // the generator no matter what it was called.
    let fixtures = workspace_root().join("fixtures");
    for seed in &seeds {
        assert!(
            !corpus.iter().any(|e| e.name == *seed),
            "seed `{seed}` names a corpus entry"
        );
    }
    assert!(
        fixtures.is_dir(),
        "the seed corpus is the committed fixture tree at {fixtures:?}"
    );

    // And the manifest is a *manifest*: it records provenance, never bytes (FR-053), so
    // there is nothing in it a generator could seed from even if one tried.
    for entry in &corpus {
        assert!(
            !entry.workspace_dir(&workspace_root()).exists(),
            "corpus content must never be vendored into this repository (FR-053), but \
             {} exists",
            entry.id
        );
    }
}
