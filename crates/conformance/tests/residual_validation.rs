//! T027 (US2, FR-013 / FR-055 / FR-047): **V23** — a residual is well-formed, or it
//! fails.
//!
//! A residual is the one record type that can absorb work indefinitely without a gate
//! noticing: it never blocks certification (FR-054), because the coverage still exists —
//! carried by the program that has not been retired. That is exactly why its shape is
//! strict. A residual must name a **specific** missing capability and a **tracked**
//! follow-up, and it must name the carrier it pins unless it is one of the
//! `external-corpus-entry` residuals, which pin no program at all (research D8).
//!
//! Hermetic: fixture registries under a tempdir plus a read of the real registry.

use std::path::Path;

use deacon_conformance::load::Registry;
use deacon_conformance::residual::{ResidualFile, load_residuals};
use deacon_conformance::validate::{Violation, check_residuals};
use deacon_conformance::{default_registry_dir, workspace_root};

/// A minimal, well-formed residual document over the REAL baseline's units, so the
/// unit/carrier resolution legs have something true to resolve against.
const GOOD: &str = r##"{
  "records": [
    {
      "id": "res-probe",
      "units": ["parity_state_diff::intra-deacon-single-vs-compose"],
      "blockedCarrier": "parity_state_diff",
      "missingCapability": "cross-CLI container-state snapshot comparison across both CLIs",
      "followUp": "#4242",
      "behaviors": ["bhv-state-diff-parity"]
    }
  ]
}"##;

/// Build a registry whose `residuals.json` is `residuals` and whose baseline/mapping
/// are the real committed ones, so only the residual under test varies.
fn registry_with(residuals: &str) -> (tempfile::TempDir, Registry) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("conformance");
    let registry_dir = root.join("registry");
    copy_dir(&default_registry_dir(), &registry_dir);
    copy_dir(
        &workspace_root().join("conformance").join("migration"),
        &root.join("migration"),
    );
    std::fs::write(registry_dir.join("residuals.json"), residuals).expect("write residuals");
    let registry = Registry::load(&registry_dir).expect("fixture registry loads");
    (dir, registry)
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create dir");
    for entry in std::fs::read_dir(src).expect("read dir").flatten() {
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("copy file");
        }
    }
}

fn violations_for(residuals: &str) -> Vec<Violation> {
    let (_dir, registry) = registry_with(residuals);
    check_residuals(&registry)
}

#[test]
fn a_well_formed_residual_passes() {
    let violations = violations_for(GOOD);
    assert!(
        violations.is_empty(),
        "a well-formed residual must not fail: {violations:#?}"
    );
}

#[test]
fn a_missing_follow_up_is_rejected_at_load() {
    let raw = GOOD.replace("\"followUp\": \"#4242\",", "");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("residuals.json");
    std::fs::write(&path, &raw).expect("write");
    assert!(
        load_residuals(&path).is_err(),
        "a residual without a followUp must not load (FR-055)"
    );
    assert!(
        serde_json::from_str::<ResidualFile>(&raw).is_err(),
        "the record shape itself requires followUp"
    );
}

#[test]
fn an_untracked_follow_up_fails_validation() {
    let violations = violations_for(&GOOD.replace("#4242", "ask someone"));
    assert!(
        violations
            .iter()
            .any(|v| v.code == "V23" && v.message.contains("not a tracked reference")),
        "a followUp that is not an issue/URL/task anchor must fail: {violations:#?}"
    );
}

#[test]
fn a_vague_missing_capability_fails() {
    for vague in [
        "not supported yet",
        "TBD",
        "unknown",
        "todo: figure this out",
        "hard",
    ] {
        let violations = violations_for(&GOOD.replace(
            "cross-CLI container-state snapshot comparison across both CLIs",
            vague,
        ));
        assert!(
            violations
                .iter()
                .any(|v| v.code == "V23" && v.message.contains("is vague")),
            "missingCapability {vague:?} must be rejected as vague: {violations:#?}"
        );
    }
}

#[test]
fn an_unresolvable_blocked_carrier_fails() {
    let violations = violations_for(&GOOD.replace("parity_state_diff\"", "parity_ghost\""));
    assert!(
        violations
            .iter()
            .any(|v| v.code == "V23" && v.message.contains("does not resolve")),
        "a blockedCarrier naming no baseline program must fail: {violations:#?}"
    );
}

#[test]
fn an_absent_blocked_carrier_fails_on_a_non_external_residual() {
    let raw = GOOD.replace("\"blockedCarrier\": \"parity_state_diff\",", "");
    let violations = violations_for(&raw);
    assert!(
        violations
            .iter()
            .any(|v| v.code == "V23" && v.message.contains("`blockedCarrier` is required")),
        "only an external-corpus-entry residual may omit the carrier: {violations:#?}"
    );
}

#[test]
fn an_external_corpus_residual_may_omit_the_carrier_but_not_declare_one() {
    let without = r##"{
      "records": [
        {
          "id": "res-ext",
          "units": ["realworld::oss-ruff"],
          "missingCapability": "vendored fixtures for network-fetched third-party workspaces",
          "followUp": "#4242",
          "behaviors": []
        }
      ]
    }"##;
    assert!(
        violations_for(without).is_empty(),
        "an external-corpus-entry residual legitimately pins no program (research D8)"
    );

    let with_carrier = without.replace(
        "\"missingCapability\"",
        "\"blockedCarrier\": \"parity_state_diff\", \"missingCapability\"",
    );
    assert!(
        violations_for(&with_carrier)
            .iter()
            .any(|v| v.code == "V23" && v.message.contains("must NOT name a `blockedCarrier`")),
        "an external-corpus-entry residual naming a carrier is wrong: it blocks no program"
    );
}

#[test]
fn a_residual_covering_a_migrated_unit_fails() {
    // `parity_corpus_errors::malformed-json` is migrated in the real mapping.
    let violations = violations_for(&GOOD.replace(
        "parity_state_diff::intra-deacon-single-vs-compose",
        "parity_corpus_errors::malformed-json",
    ));
    assert!(
        violations
            .iter()
            .any(|v| v.code == "V23" && v.message.contains("may not be counted as migrated")),
        "a residual may not claim a unit the mapping says was migrated: {violations:#?}"
    );
}

#[test]
fn an_unresolvable_unit_or_behavior_fails() {
    let ghost_unit = violations_for(&GOOD.replace(
        "parity_state_diff::intra-deacon-single-vs-compose",
        "parity_ghost::nope",
    ));
    assert!(
        ghost_unit
            .iter()
            .any(|v| v.code == "V23" && v.message.contains("does not exist")),
        "{ghost_unit:#?}"
    );

    let ghost_behavior = violations_for(&GOOD.replace("bhv-state-diff-parity", "bhv-ghost"));
    assert!(
        ghost_behavior
            .iter()
            .any(|v| v.code == "V23" && v.message.contains("bhv-ghost")),
        "{ghost_behavior:#?}"
    );
}

#[test]
fn the_scaffold_sentinel_is_rejected_by_the_loader() {
    let raw = GOOD.replace(
        "cross-CLI container-state snapshot comparison across both CLIs",
        deacon_conformance::residual::UNREVIEWED_SENTINEL,
    );
    assert!(
        serde_json::from_str::<ResidualFile>(&raw).is_err(),
        "scaffolded output must never load unedited"
    );
}

#[test]
fn the_real_residual_queue_is_well_formed() {
    let registry =
        Registry::load(&default_registry_dir()).expect("the real registry loads cleanly");
    let violations = check_residuals(&registry);
    assert!(
        violations.is_empty(),
        "conformance/registry/residuals.json must be well-formed:\n{violations:#?}"
    );
    assert!(
        !registry.residuals.is_empty(),
        "US2 authors a non-empty residual queue; an empty one would make this test vacuous"
    );
}

// ---------------------------------------------------------------------------
// 024 P1: queued vs permanent. The exactly-one-of rules are enforced at DESERIALIZE
// time, so these assert on the loader; only the "does the rationale say anything"
// judgement is a V23 validation rule.
// ---------------------------------------------------------------------------

/// The permanent form of `GOOD`: rationale instead of follow-up.
fn permanent(rationale: &str) -> String {
    GOOD.replace(
        "\"followUp\": \"#4242\",",
        &format!(
            "\"disposition\": \"permanent\",\n      \"outOfScopeRationale\": \"{rationale}\","
        ),
    )
}

#[test]
fn a_permanent_residual_with_a_real_ground_passes() {
    let violations = violations_for(&permanent(
        "Constitution II: feature authoring is permanently out of scope, so no capability \
         will ever unblock this unit",
    ));
    assert!(
        violations.is_empty(),
        "a permanent residual naming its principle must pass: {violations:#?}"
    );
}

/// The FR-047 demonstration for the new V23 sub-rule: a rationale that merely restates
/// the exclusion is indistinguishable from unqueued debt, so it must fail.
#[test]
fn a_permanent_residual_whose_rationale_names_no_ground_fails() {
    for empty_claim in [
        "out of scope",
        "we are never going to do this one",
        "not possible to express as a declarative case at all",
    ] {
        let violations = violations_for(&permanent(empty_claim));
        assert!(
            violations
                .iter()
                .any(|v| v.code == "V23" && v.message.contains("outOfScopeRationale")),
            "a rationale that names no principle or mechanism must fail V23; \
             claim = {empty_claim:?}, got {violations:#?}"
        );
    }
}

#[test]
fn queued_and_permanent_fields_are_mutually_exclusive_at_load() {
    // permanent + followUp: promises work that cannot happen.
    let both = GOOD.replace(
        "\"followUp\": \"#4242\",",
        "\"disposition\": \"permanent\",\n      \"outOfScopeRationale\": \"Constitution II \
         forbids feature authoring, so nothing can unblock it\",\n      \
         \"followUp\": \"#4242\",",
    );
    let err = serde_json::from_str::<ResidualFile>(&both)
        .expect_err("a permanent residual has nothing to track");
    assert!(err.to_string().contains("followUp"), "got: {err}");

    // permanent without a rationale: asserts itself.
    let bare = GOOD.replace(
        "\"followUp\": \"#4242\",",
        "\"disposition\": \"permanent\",",
    );
    let err = serde_json::from_str::<ResidualFile>(&bare)
        .expect_err("a permanent residual must name its ground");
    assert!(
        err.to_string().contains("outOfScopeRationale"),
        "got: {err}"
    );

    // queued + rationale: claims exclusion while asking to be fixed.
    let queued_with_rationale = GOOD.replace(
        "\"followUp\": \"#4242\",",
        "\"followUp\": \"#4242\",\n      \"outOfScopeRationale\": \"Constitution II\",",
    );
    let err = serde_json::from_str::<ResidualFile>(&queued_with_rationale)
        .expect_err("a rationale contradicts queued work");
    assert!(
        err.to_string().contains("outOfScopeRationale"),
        "got: {err}"
    );
}

/// The split is the point of P1: mixing permanent exclusions into the queue would make the
/// queue asymptote at a nonzero floor forever. Assert the real registry actually separates
/// them, and that neither list is vacuous.
#[test]
fn certify_separates_the_queue_from_the_permanent_exclusions() {
    use deacon_conformance::residual::ResidualDisposition;

    let registry =
        Registry::load(&default_registry_dir()).expect("the real registry loads cleanly");
    let (queued, permanent): (Vec<_>, Vec<_>) = registry
        .residuals
        .iter()
        .partition(|r| r.disposition == ResidualDisposition::Queued);

    assert!(
        !queued.is_empty(),
        "some residuals are still migratable work; an empty queue here would make the \
         separation vacuous"
    );
    assert!(
        !permanent.is_empty(),
        "024 P1 marks the constitutionally- and structurally-excluded units permanent"
    );

    for record in &queued {
        assert!(
            record.follow_up.is_some() && record.out_of_scope_rationale.is_none(),
            "queued residual {} must carry exactly a followUp",
            record.id
        );
    }
    for record in &permanent {
        assert!(
            record.out_of_scope_rationale.is_some() && record.follow_up.is_none(),
            "permanent residual {} must carry exactly an outOfScopeRationale",
            record.id
        );
    }
}
