//! Acceptance tests for profile applicability (T021; FR-017, SC-007).
//!
//! Asserts that an out-of-profile behavior is excluded from the denominator and
//! listed under `outOfProfile` (never "uncovered"), that each in-profile behavior is
//! counted exactly once, and that the report claims nothing beyond the active
//! profile's context (every in-profile behavior's applicability is satisfiable by
//! the profile). Hermetic; CWD-independent via `workspace_root()`.

use std::collections::HashMap;
use std::collections::HashSet;

use deacon_conformance::load::Registry;
use deacon_conformance::report::render_report_json;
use deacon_conformance::workspace_root;

fn valid_registry() -> Registry {
    let root = workspace_root().join("fixtures/conformance/valid");
    Registry::load(&root).expect("valid fixture loads")
}

fn report_value() -> serde_json::Value {
    let json = render_report_json(&valid_registry());
    serde_json::from_str(&json).expect("report.json is valid JSON")
}

#[test]
fn out_of_profile_behavior_is_excluded_from_denominator_and_listed_separately() {
    let report = report_value();

    // The podman-only behavior is out-of-profile under the active docker profile.
    let out_of_profile: HashSet<&str> = report["outOfProfile"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["id"].as_str().unwrap())
        .collect();
    assert!(
        out_of_profile.contains("bhv-exec-podman-keep-id"),
        "the podman-only behavior must be listed in outOfProfile, got {out_of_profile:?}"
    );

    // It never appears among the in-profile behaviors (never "uncovered", FR-017).
    let in_profile: HashSet<&str> = report["behaviors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["id"].as_str().unwrap())
        .collect();
    assert!(
        !in_profile.contains("bhv-exec-podman-keep-id"),
        "an out-of-profile behavior must never be counted in-profile"
    );

    // Its applicability is preserved so a reader sees the context it awaits.
    let entry = report["outOfProfile"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == "bhv-exec-podman-keep-id")
        .unwrap();
    let applicability = entry["applicability"].as_array().unwrap();
    assert!(
        !applicability.is_empty(),
        "the out-of-profile behavior must list the conditions it awaits"
    );
}

#[test]
fn in_profile_behaviors_are_counted_once_each() {
    let report = report_value();

    // Behavior IDs across the in-profile behaviors array are unique.
    let mut seen: HashSet<&str> = HashSet::new();
    for b in report["behaviors"].as_array().unwrap() {
        let id = b["id"].as_str().unwrap();
        assert!(
            seen.insert(id),
            "behavior {id} appears more than once in-profile"
        );
    }

    // behaviorsInProfile == in-profile (non-extension) + extensions, each behavior
    // once (FR-003 deduplicated denominator).
    let denominator = report["summary"]["behaviorsInProfile"].as_u64().unwrap();
    let extensions = report["summary"]["extensions"].as_u64().unwrap();
    assert_eq!(
        seen.len() as u64 + extensions,
        denominator,
        "denominator must count each in-profile behavior exactly once"
    );
}

#[test]
fn report_claims_nothing_beyond_the_profile_context() {
    let registry = valid_registry();
    let report = report_value();

    // The active profile's context assignment.
    let profile = registry
        .profiles
        .iter()
        .find(|p| p.active)
        .expect("an active profile");
    let context: HashMap<&str, &str> = profile
        .context
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    // SC-007: every in-profile behavior's applicability is satisfiable by the
    // profile — the report never claims a behavior for a context outside the profile.
    for b in report["behaviors"].as_array().unwrap() {
        let id = b["id"].as_str().unwrap();
        for cond in b["applicability"].as_array().unwrap() {
            let dim = cond["dimension"].as_str().unwrap();
            let assigned = context.get(dim).unwrap_or_else(|| {
                panic!("in-profile behavior {id} constrains unassigned dimension {dim}")
            });
            let values: Vec<&str> = cond["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert!(
                values.contains(assigned),
                "in-profile behavior {id} claims {dim} ∈ {values:?} but the profile assigns {assigned:?}"
            );
        }
    }
}
