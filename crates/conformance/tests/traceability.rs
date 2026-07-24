//! Acceptance tests for report traceability (T020; FR-022, SC-003 machine side).
//!
//! Renders `report.json` for the `valid` fixture and walks the full chain
//! source unit → behavior → applicability → case → outcome, asserting every link
//! resolves against the underlying registry and that the summary counts sum
//! consistently with the behavior inventory. Hermetic; resolves paths via
//! `workspace_root()` so it is CWD-independent and selected by `dev-fast`.

use std::collections::HashSet;

use deacon_conformance::load::Registry;
use deacon_conformance::report::{render_report_json, render_report_md};
use deacon_conformance::workspace_root;

fn valid_registry() -> Registry {
    let root = workspace_root().join("fixtures/conformance/valid");
    Registry::load(&root).expect("valid fixture loads")
}

/// Parse the rendered `report.json` into a `serde_json::Value` (the machine side of
/// SC-003 — we walk the SERIALIZED document, not the in-memory model).
fn report_value() -> serde_json::Value {
    let json = render_report_json(&valid_registry());
    serde_json::from_str(&json).expect("report.json is valid JSON")
}

#[test]
fn every_behavior_chain_link_resolves() {
    let registry = valid_registry();
    let report = report_value();

    // Declared record IDs the chain links must resolve against.
    let source_ids: HashSet<&str> = registry.sources.iter().map(|s| s.id.as_str()).collect();
    let channel_ids: HashSet<&str> = registry.channels.iter().map(|c| c.id.as_str()).collect();
    let dim_ids: HashSet<&str> = registry.dimensions.iter().map(|d| d.id.as_str()).collect();
    let case_ids: HashSet<&str> = registry.cases.iter().map(|c| c.id.as_str()).collect();

    let behaviors = report["behaviors"].as_array().expect("behaviors array");
    assert!(
        !behaviors.is_empty(),
        "the valid fixture has in-profile behaviors"
    );

    for behavior in behaviors {
        let bhv_id = behavior["id"].as_str().expect("behavior id");

        // source → behavior: every source resolves AND actually lists this behavior.
        let sources = behavior["sources"].as_array().expect("sources array");
        assert!(
            !sources.is_empty(),
            "behavior {bhv_id} must trace to ≥1 source unit"
        );
        for src in sources {
            let src_id = src.as_str().expect("source id");
            assert!(
                source_ids.contains(src_id),
                "behavior {bhv_id} references unknown source {src_id}"
            );
            let unit = registry
                .sources
                .iter()
                .find(|s| s.id == src_id)
                .expect("source unit present");
            assert!(
                unit.behaviors.iter().any(|b| b == bhv_id),
                "source {src_id} must list behavior {bhv_id} (inverse link)"
            );
        }

        // behavior → applicability: every condition names a declared dimension.
        for cond in behavior["applicability"].as_array().expect("applicability") {
            let dim = cond["dimension"].as_str().expect("condition dimension");
            assert!(
                dim_ids.contains(dim),
                "behavior {bhv_id} applicability references unknown dimension {dim}"
            );
        }

        // behavior → case → outcome: every case resolves and every outcome names a
        // declared channel.
        for case in behavior["cases"].as_array().expect("cases array") {
            let case_id = case["id"].as_str().expect("case id");
            assert!(
                case_ids.contains(case_id),
                "behavior {bhv_id} references unknown case {case_id}"
            );
            let outcomes = case["outcomes"].as_array().expect("outcomes array");
            assert!(
                !outcomes.is_empty(),
                "case {case_id} must carry ≥1 expected outcome"
            );
            for outcome in outcomes {
                let channel = outcome["channel"].as_str().expect("outcome channel");
                assert!(
                    channel_ids.contains(channel),
                    "case {case_id} outcome references undeclared channel {channel}"
                );
                assert!(
                    outcome["expectation"]
                        .as_str()
                        .is_some_and(|e| !e.is_empty()),
                    "case {case_id} outcome must state an expectation"
                );
            }
        }
    }
}

#[test]
fn summary_counts_sum_consistently_with_the_behavior_inventory() {
    let report = report_value();
    let summary = &report["summary"];

    let denominator = summary["behaviorsInProfile"].as_u64().expect("denominator");
    let conformant = summary["conformant"].as_u64().unwrap();
    let divergent = summary["divergent"].as_u64().unwrap();
    let waived = summary["waived"].as_u64().unwrap();
    let gap = summary["gap"].as_u64().unwrap();
    let extensions = summary["extensions"].as_u64().unwrap();

    // The five in-profile buckets (four coverage states + extensions) partition the
    // deduplicated denominator (FR-003).
    assert_eq!(
        conformant + divergent + waived + gap + extensions,
        denominator,
        "in-profile buckets must sum to behaviorsInProfile"
    );

    // The `behaviors` array holds exactly the four-coverage-state (non-extension)
    // behaviors.
    let behaviors_len = report["behaviors"].as_array().unwrap().len() as u64;
    assert_eq!(
        behaviors_len,
        conformant + divergent + waived + gap,
        "behaviors array length must equal the four coverage-state counts"
    );

    // Out-of-profile is excluded from the denominator (FR-017).
    let out_of_profile = summary["outOfProfile"].as_u64().unwrap();
    let out_len = report["outOfProfile"].as_array().unwrap().len() as u64;
    assert_eq!(
        out_of_profile, out_len,
        "outOfProfile count matches the array"
    );
}

// -- Three-axis disposition rendering (T026; US3 scenario 3, FR-012) ----------

/// `report.json` carries the three disposition axes as three SEPARATE fields on
/// every behavior entry — never a single combined "different but acceptable" state
/// (FR-012). The valid fixture spans conformant / waived / gap coverage states.
#[test]
fn report_json_shows_the_three_axes_as_separate_fields() {
    let report = report_value();

    let spec_values: HashSet<&str> = [
        "conformant",
        "nonconformant",
        "unspecified",
        "not-applicable",
    ]
    .into_iter()
    .collect();
    let reference_values: HashSet<&str> = ["aligned", "divergent", "unknown", "not-applicable"]
        .into_iter()
        .collect();
    let decision_values: HashSet<&str> = [
        "follow-spec",
        "align-with-reference",
        "deacon-extension",
        "intentional-divergence",
        "unresolved-gap",
    ]
    .into_iter()
    .collect();

    let behaviors = report["behaviors"].as_array().expect("behaviors array");
    assert!(
        !behaviors.is_empty(),
        "valid fixture has in-profile behaviors"
    );

    for behavior in behaviors {
        let id = behavior["id"].as_str().expect("behavior id");
        // Three DISTINCT keys must each be present and hold a value from their own
        // closed set — there is no merged axis anywhere in the shape.
        let spec = behavior["spec"]
            .as_str()
            .unwrap_or_else(|| panic!("behavior {id} missing `spec` axis"));
        let reference = behavior["reference"]
            .as_str()
            .unwrap_or_else(|| panic!("behavior {id} missing `reference` axis"));
        let decision = behavior["decision"]
            .as_str()
            .unwrap_or_else(|| panic!("behavior {id} missing `decision` axis"));
        assert!(
            spec_values.contains(spec),
            "behavior {id} spec {spec:?} outside the closed set"
        );
        assert!(
            reference_values.contains(reference),
            "behavior {id} reference {reference:?} outside the closed set"
        );
        assert!(
            decision_values.contains(decision),
            "behavior {id} decision {decision:?} outside the closed set"
        );
    }

    // The registry's one divergence has all three axes distinct in the report,
    // proving they are stored independently rather than as one code.
    let divergence = behaviors
        .iter()
        .find(|b| b["id"] == "bhv-readconfig-malformed-jsonc-rejected")
        .expect("the waived divergence is an in-profile behavior");
    assert_eq!(divergence["spec"], "unspecified");
    assert_eq!(divergence["reference"], "divergent");
    assert_eq!(divergence["decision"], "intentional-divergence");
}

/// Deacon extensions are reported UNDER Extensions and NEVER as divergences (US3
/// scenario 3, FR-012). `report.json` keeps them in their own `extensions[]` array
/// and out of the `behaviors[]` (divergence-bearing) array entirely.
#[test]
fn report_json_lists_extensions_separately_from_divergences() {
    let report = report_value();

    // The extension behavior appears in `extensions[]` …
    let extensions = report["extensions"].as_array().expect("extensions array");
    let extension_behaviors: HashSet<&str> = extensions
        .iter()
        .flat_map(|e| e["behaviors"].as_array().expect("ext behaviors"))
        .map(|b| b.as_str().expect("behavior id"))
        .collect();
    assert!(
        extension_behaviors.contains("bhv-secrets-dotenv-superset"),
        "the extension behavior must be listed under extensions, got {extension_behaviors:?}"
    );

    // … and NOT in the `behaviors[]` array, which carries the four coverage-state
    // (conformant/divergent/waived/gap) behaviors that a Divergences view renders.
    let behavior_ids: HashSet<&str> = report["behaviors"]
        .as_array()
        .expect("behaviors array")
        .iter()
        .map(|b| b["id"].as_str().expect("behavior id"))
        .collect();
    assert!(
        !behavior_ids.contains("bhv-secrets-dotenv-superset"),
        "an extension must never appear among divergence-bearing behaviors, got {behavior_ids:?}"
    );
}

/// `report.md` renders the three axes as three separate columns in the
/// "Divergences & waivers" table, and lists extensions under their own "Extensions"
/// section — never inside "Divergences & waivers" (US3 scenario 3, FR-012).
#[test]
fn report_md_separates_axes_columns_and_extensions_section() {
    let md = render_report_md(&valid_registry());

    // The Divergences & waivers table header carries three DISTINCT axis columns.
    let divergences = section(&md, "## Divergences & waivers", "## Extensions");
    for column in ["Spec", "Reference", "Decision"] {
        assert!(
            divergences.contains(column),
            "Divergences & waivers table must have a {column:?} column, got:\n{divergences}"
        );
    }

    // The extension behavior is under Extensions …
    let extensions = section(&md, "## Extensions", "## Behavior traceability index");
    assert!(
        extensions.contains("ext-secrets-file-dotenv"),
        "the extension record must render under Extensions, got:\n{extensions}"
    );

    // … and never inside the Divergences & waivers section.
    assert!(
        !divergences.contains("bhv-secrets-dotenv-superset")
            && !divergences.contains("ext-secrets-file-dotenv"),
        "no extension may appear under Divergences & waivers, got:\n{divergences}"
    );
}

/// Slice the markdown between the `start` heading (inclusive) and the next `end`
/// heading (exclusive). Both headings are required to be present and ordered.
fn section<'a>(md: &'a str, start: &str, end: &str) -> &'a str {
    let from = md
        .find(start)
        .unwrap_or_else(|| panic!("report.md missing section {start:?}"));
    let rest = &md[from..];
    let to = rest
        .find(end)
        .unwrap_or_else(|| panic!("report.md missing section {end:?} after {start:?}"));
    &rest[..to]
}

// ===========================================================================
// Clause → behavior traceability (021, T034; FR-026, SC-006). Over the REAL
// committed registry + clause inventory, checked deterministically and offline:
// every behavior-mapped clause classification resolves forward to an existing
// behavior, and every such behavior is reachable back from its clause(s). No
// network, no model — pure functions of committed data.
// ===========================================================================

use deacon_conformance::model::Disposition;
use deacon_conformance::{clause_paths_for, default_registry_dir};

#[test]
fn every_consumer_clause_maps_forward_and_back_to_a_real_behavior() {
    let registry = Registry::load(&default_registry_dir()).expect("real registry loads");
    let (_spec, clauses_file) = clause_paths_for(&default_registry_dir());
    let inventory =
        deacon_conformance::load::load_clause_inventory(&clauses_file).expect("clauses load");

    let behavior_ids: HashSet<&str> = registry.behaviors.iter().map(|b| b.id.as_str()).collect();
    let clause_ids: HashSet<&str> = inventory
        .as_ref()
        .map(|inv| inv.units.iter().map(|u| u.id.as_str()).collect())
        .unwrap_or_default();

    // Forward: every behavior-mapped clause classification points at a real clause AND
    // real behaviors.
    let mut behavior_to_clauses: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for clc in &registry.clause_classifications {
        if clc.disposition != Disposition::BehaviorMapped {
            continue;
        }
        let clause = clc
            .clause
            .as_deref()
            .expect("a behavior-mapped clause classification is per-clause, not document-scope");
        assert!(
            clause_ids.contains(clause),
            "clc {:?} references clause {clause:?} absent from the committed inventory",
            clc.id
        );
        assert!(
            !clc.behaviors.is_empty(),
            "behavior-mapped clc {:?} must name ≥1 behavior",
            clc.id
        );
        for behavior in &clc.behaviors {
            assert!(
                behavior_ids.contains(behavior.as_str()),
                "clc {:?} maps to behavior {behavior:?}, which is not an existing bhv- record",
                clc.id
            );
            behavior_to_clauses
                .entry(behavior.clone())
                .or_default()
                .push(clause.to_string());
        }
    }

    // Backward: each behavior reached from a clause is reachable back to ≥1 clause
    // (the inverse index is total by construction — assert it is navigable and stable).
    for (behavior, clauses) in &behavior_to_clauses {
        assert!(
            !clauses.is_empty(),
            "behavior {behavior:?} must back-trace to ≥1 clause"
        );
    }
}
