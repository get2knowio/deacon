//! Acceptance tests for User Story 5 — "fields that broad normalization used to hide"
//! (024-deterministic-conformance-coverage, T110/T114; FR-047–FR-056, SC-008).
//!
//! Hermetic by construction: every assertion reads the REAL committed registry and the real
//! coverage renderer, so none of them can pass against a convenient synthetic model that no
//! longer ships. The boundary is worth stating:
//!
//! | Question | Answered here | Answered by the live tier |
//! |---|---|---|
//! | Does at least one case COMPARE each named field? | yes | — |
//! | Is the comparison a real assertion rather than an accidental substring? | yes | — |
//! | Is an ambiguous Feature install order pinned or dispositioned? | yes | — |
//! | Do the two CLIs actually agree on those fields? | no | `parity_conformance_docker` |

use std::collections::{BTreeMap, BTreeSet};

use deacon_conformance::coverage_report::{DENORMALIZED_FIELDS, build_coverage_reports};
use deacon_conformance::default_registry_dir;
use deacon_conformance::load::Registry;
use deacon_conformance::model::TestCase;

fn real_registry() -> Registry {
    Registry::load(&default_registry_dir()).expect("the real registry loads")
}

/// `field id -> covering case ids`, straight from the renderer that writes
/// `coverage-observables.json`.
fn covering_cases(registry: &Registry) -> BTreeMap<String, Vec<String>> {
    let inventory = deacon_conformance::obligation::generate_obligations(registry)
        .expect("obligations generate from the real registry");
    let reports = build_coverage_reports(registry, &inventory);
    reports
        .observables
        .denormalized_fields
        .iter()
        .map(|f| (f.field.clone(), f.by.clone()))
        .collect()
}

// ---------------------------------------------------------------------------
// T110 — every named field has at least one covering case (scenario 1, SC-008)
// ---------------------------------------------------------------------------

#[test]
fn every_denormalized_field_has_a_covering_case() {
    let registry = real_registry();
    let covering = covering_cases(&registry);

    let uncovered: Vec<&str> = DENORMALIZED_FIELDS
        .iter()
        .map(|(field, _)| *field)
        .filter(|field| covering.get(*field).is_none_or(|cases| cases.is_empty()))
        .collect();

    assert!(
        uncovered.is_empty(),
        "SC-008: every field previously suppressed by broad normalization must be compared \
         by at least one executable case; uncovered: {uncovered:?}"
    );
    assert_eq!(
        covering.len(),
        DENORMALIZED_FIELDS.len(),
        "the report must account for every named field, not a subset"
    );
}

/// The twelve fields are matched to compared paths by SUBSTRING, which is cheap and — on its
/// own — forgeable: `labels.devcontainer.source` contains "source", so a case that compares
/// only a LABEL can be credited with covering `mount-source`. That is exactly the false
/// equivalence US5 exists to retire, so coverage is additionally required to come from a
/// case authored for the field: one whose id names it.
///
/// Without this, `every_denormalized_field_has_a_covering_case` passes on an accident and the
/// story's whole claim rests on a substring collision.
#[test]
fn each_field_is_covered_by_a_case_authored_for_it() {
    let registry = real_registry();
    let covering = covering_cases(&registry);

    // The case-id fragment that identifies a case authored to compare each field.
    let authored_for: &[(&str, &str)] = &[
        ("lifecycle-array-vs-object", "lifecycle"),
        ("command", "state-"),
        ("entrypoint-chained", "entrypoint"),
        ("env-merge-precedence", "env-merge"),
        ("path-construction", "path-construction"),
        ("user-uid-gid", "user-"),
        ("metadata-label-namespaces", "label"),
        ("mount-source", "mount-source"),
        ("mount-shape", "mount-source-vs-shape"),
        ("network", "network"),
        ("compose-project-resources", "compose-project-resources"),
        ("null-empty-omitted", "null-empty-omitted"),
    ];
    assert_eq!(
        authored_for.len(),
        DENORMALIZED_FIELDS.len(),
        "every named field needs a deliberate carrier, so this table tracks that one"
    );

    for (field, fragment) in authored_for {
        let cases = covering
            .get(*field)
            .unwrap_or_else(|| panic!("field `{field}` is reported at all"));
        assert!(
            cases.iter().any(|c| c.contains(fragment)),
            "field `{field}` is credited only to cases that do not name it ({cases:?}); a \
             substring collision on a compared path is not evidence that the field is \
             compared"
        );
    }
}

/// A case that COMPARES a field must actually assert on it or tolerate a difference at it —
/// the two things `compared_paths` reads. Declaring a channel and asserting nothing is
/// capture, not comparison, and the distinction is the whole point of the observables report.
#[test]
fn the_us5_cases_assert_on_the_channels_they_declare() {
    let registry = real_registry();
    let us5: Vec<&TestCase> = registry
        .cases
        .iter()
        .filter(|c| {
            c.id.contains("lifecycle-")
                || c.id.contains("entrypoint-")
                || c.id.contains("env-merge")
                || c.id.contains("path-construction")
                || c.id.contains("mount-source")
                || c.id.contains("null-empty-omitted")
                || c.id.contains("compose-project-resources")
                || c.id.contains("label-namespaces")
        })
        .collect();
    assert!(
        us5.len() >= 12,
        "the US5 case set must be present; found {}",
        us5.len()
    );

    for case in us5 {
        // A live-differential compares the whole channel and IGNORES assertions by design
        // (data-model §5), so requiring one there would ask for a declaration the runner
        // never reads. Everything else must assert.
        if matches!(
            case.oracle_type,
            Some(deacon_conformance::model::OracleType::LiveDifferential)
        ) {
            continue;
        }
        for expectation in &case.expected {
            assert!(
                expectation.assertion.is_some(),
                "case {:?} declares channel {:?} with no assertion — that captures the \
                 channel without comparing it",
                case.id,
                expectation.channel
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T114 — an ambiguous Feature install order is pinned or dispositioned (scenario 5)
// ---------------------------------------------------------------------------

/// A configuration declaring two or more Features whose relative order nothing constrains
/// has an AMBIGUOUS install order. US5 scenario 5 forbids leaving that to chance: the case
/// must either pin a deterministic order or the ambiguity must be dispositioned.
///
/// "Pinned" is checked at the FIXTURE, not in prose: the configuration must carry
/// `overrideFeatureInstallOrder`, or the Features must constrain each other with `dependsOn`
/// / `installsAfter`. A case whose notes merely claim determinism does not count — that is
/// the claim, not the mechanism.
///
/// **Scoped to the operations that INSTALL.** `read-configuration` resolves and reports an
/// order without running anything, and its cases compare the whole resolved document, so
/// whatever order it reports is already pinned by the comparison itself; there is no second
/// place for chance to enter. The ambiguity scenario 5 is about is the one that decides
/// which Feature's install script runs first, and only an installing operation has one.
///
/// A REMOTE Feature's own `installsAfter` is invisible to this tree scan — its metadata is
/// not in the repository — so a multi-Feature INSTALL case built from remote Features is
/// reported even if the registry would order it. That is the safe direction: the test asks
/// the fixture to state the order it depends on.
#[test]
fn an_ambiguous_feature_install_order_is_pinned_or_dispositioned() {
    /// Subcommands that actually install Features into an image or container.
    const INSTALLING: &[&str] = &["up", "build"];

    let registry = real_registry();
    let fixtures_root = default_registry_dir()
        .parent()
        .expect("the registry lives under conformance/")
        .join("fixtures");

    let mut unpinned: Vec<String> = Vec::new();
    for case in &registry.cases {
        for op in &case.operations {
            if !INSTALLING.contains(&op.subcommand.as_str()) {
                continue;
            }
            for fixture in &op.fixtures {
                let dir = fixtures_root.join(fixture);
                let Some(config) = read_config(&dir) else {
                    continue;
                };
                let feature_ids: Vec<String> = config
                    .get("features")
                    .and_then(|f| f.as_object())
                    .map(|o| o.keys().cloned().collect())
                    .unwrap_or_default();
                if feature_ids.len() < 2 {
                    continue;
                }
                if config.get("overrideFeatureInstallOrder").is_some() {
                    continue;
                }
                if features_constrain_each_other(&dir, &feature_ids) {
                    continue;
                }
                unpinned.push(format!("{} ({fixture})", case.id));
            }
        }
    }

    assert!(
        unpinned.is_empty(),
        "US5 scenario 5: a configuration declaring several Features with nothing ordering \
         them leaves the install order to chance. Pin it with `overrideFeatureInstallOrder` \
         or with `dependsOn`/`installsAfter` between the Features. Unpinned: {unpinned:?}"
    );
}

/// The Feature-install-order behavior itself must be dispositioned by a CASE, never by a
/// bare rationale — the other half of scenario 5's "pinned or dispositioned, never left to
/// chance". A rationale can assert determinism; only a case can observe it.
#[test]
fn the_feature_install_order_behaviors_are_case_backed() {
    let registry = real_registry();
    for behavior in [
        "bhv-up-feature-install-order",
        "bhv-up-feature-entrypoint-chain",
        "bhv-readconfig-feature-resolution-order",
    ] {
        let cases: Vec<&str> = registry
            .cases
            .iter()
            .filter(|c| c.behaviors.iter().any(|b| b == behavior))
            .map(|c| c.id.as_str())
            .collect();
        assert!(
            !cases.is_empty(),
            "{behavior} must be backed by at least one executable case; an install order \
             claimed in prose and never run is exactly the ambiguity scenario 5 forbids"
        );
    }
}

/// The chained-entrypoint case pins an EXACT ordered content, not a set — the ordering claim
/// is the substance of FR-048, and an unordered "both ran" assertion would pass for a chain
/// that ran the two entrypoints backwards.
#[test]
fn the_entrypoint_chain_case_pins_the_order_exactly() {
    let registry = real_registry();
    let case = registry
        .cases
        .iter()
        .find(|c| c.id == "case-up-entrypoint-chained-features")
        .expect("the chained-entrypoint case exists");

    let pinned = case
        .expected
        .iter()
        .filter(|e| e.channel == "chan-file-content")
        .filter_map(|e| e.assertion.as_ref())
        .filter_map(|a| a.get("jsonSubset"))
        .filter_map(|s| s.get("entrypoint-chain.txt"))
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        pinned,
        vec!["conf-ep-a\nconf-ep-b\n"],
        "the chain file's exact contents are the order; a set-shaped assertion would pass \
         for a chain that ran the entrypoints backwards"
    );
}

/// Read a fixture's `devcontainer.json` from either spec discovery location.
fn read_config(dir: &std::path::Path) -> Option<serde_json::Value> {
    for rel in [".devcontainer/devcontainer.json", ".devcontainer.json"] {
        if let Ok(text) = std::fs::read_to_string(dir.join(rel)) {
            return serde_json::from_str(&text).ok();
        }
    }
    None
}

/// Whether the LOCAL Features named by `ids` order themselves with `dependsOn` /
/// `installsAfter`. A remote Feature reference is treated as unconstrained here: its
/// metadata is not in the tree, so this test cannot see an ordering it might declare, and
/// reporting it is the safe direction.
fn features_constrain_each_other(dir: &std::path::Path, ids: &[String]) -> bool {
    let mut constrained: BTreeSet<&str> = BTreeSet::new();
    for id in ids {
        let Some(rel) = id.strip_prefix("./") else {
            continue;
        };
        let path = dir
            .join(".devcontainer")
            .join(rel)
            .join("devcontainer-feature.json");
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if meta.get("dependsOn").is_some() || meta.get("installsAfter").is_some() {
            constrained.insert(id.as_str());
        }
    }
    !constrained.is_empty()
}
