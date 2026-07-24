//! Hermetic allowed-difference scoping tests (022-conformance-runner US4, T060,
//! FR-031/032/033/035, SC-008).
//!
//! The conformance crate owns the DATA rules: a conflicting duplicate and a
//! global-ignore-shaped entry fail loudly at LOAD; a dangling waiver/divergence id and an
//! unlinked behavior fail at VALIDATE (V19); a well-formed tolerance backed by a real
//! registry `wvr-` resolves cleanly. The RUNTIME scoping ("path A tolerated, path B still
//! fails") and self-invalidating STALENESS (FR-034) are verdict-time behavior, unit-tested
//! in `parity-harness::compare` (`covered_divergence_is_allowed_difference_uncovered_stays_diverge`,
//! `unconsumed_tolerance_is_stale`) — the conformance crate cannot run the comparison.

use std::fs;

use deacon_conformance::load::{LoadError, Registry};
use deacon_conformance::model::{
    AllowedDifference, BehaviorUnit, DeaconExtension, Decision, Expect, ExpectedObservable,
    ObservableChannel, Operation, OracleType, ReferenceStatus, Scope, SpecStatus, TestCase, Waiver,
};
use deacon_conformance::validate::run;
use deacon_conformance::workspace_root;

const TODAY: &str = "2026-07-24";

fn behavior(id: &str, decision: Decision) -> BehaviorUnit {
    BehaviorUnit {
        id: id.to_string(),
        area: "test".to_string(),
        statement: "a behavior".to_string(),
        applicability: vec![],
        spec: SpecStatus::Conformant,
        reference: ReferenceStatus::Aligned,
        decision,
        notes: None,
    }
}

fn waiver(id: &str) -> Waiver {
    Waiver {
        id: id.to_string(),
        behaviors: vec!["bhv-x".to_string()],
        scope: Scope::CorpusCase {
            corpus: "errors".to_string(),
            case: "x".to_string(),
        },
        expect: Expect::DeaconStricter { signal: None },
        rationale: "characterized".to_string(),
        added: "2026-07-19".to_string(),
        expires: "2027-01-19".to_string(),
        config: None,
    }
}

/// A declarative live-differential case linked to `bhv-x` with the given tolerances.
fn case_with(allowed: Vec<AllowedDifference>) -> TestCase {
    TestCase {
        id: "case-tol".to_string(),
        behaviors: vec!["bhv-x".to_string()],
        oracle_type: Some(OracleType::LiveDifferential),
        operations: vec![Operation {
            id: "op-up".to_string(),
            subcommand: "up".to_string(),
            ..Operation::default()
        }],
        expected: vec![ExpectedObservable {
            channel: "chan-injected-process".to_string(),
            operation: Some("op-up".to_string()),
            assertion: None,
        }],
        allowed_differences: allowed,
        ..TestCase::default()
    }
}

fn registry_with(
    cases: Vec<TestCase>,
    waivers: Vec<Waiver>,
    exts: Vec<DeaconExtension>,
) -> Registry {
    Registry {
        behaviors: vec![
            behavior("bhv-x", Decision::FollowSpec),
            behavior("bhv-intentional", Decision::IntentionalDivergence),
        ],
        channels: vec![ObservableChannel {
            id: "chan-injected-process".to_string(),
            description: "c".to_string(),
        }],
        cases,
        waivers,
        extensions: exts,
        ..Registry::default()
    }
}

fn tz_allowed(waiver_id: Option<&str>, divergence_id: Option<&str>) -> AllowedDifference {
    AllowedDifference {
        behavior: "bhv-x".to_string(),
        context: vec!["single-container".to_string()],
        observable_path: "chan-injected-process.env.TZ".to_string(),
        rationale: "reference leaks host TZ".to_string(),
        waiver_id: waiver_id.map(str::to_string),
        divergence_id: divergence_id.map(str::to_string),
    }
}

#[test]
fn well_formed_tolerance_backed_by_real_waiver_validates_cleanly() {
    let case = case_with(vec![tz_allowed(Some("wvr-x"), None)]);
    let reg = registry_with(vec![case], vec![waiver("wvr-x")], vec![]);
    let out = run(&reg, TODAY, &workspace_root());
    assert!(
        !out.iter().any(|v| v.code == "V19"),
        "a tolerance backed by a real wvr- resolves cleanly, got {out:?}"
    );
}

#[test]
fn divergence_id_resolves_to_ext_or_intentional_behavior() {
    // ext- record backing.
    let case = case_with(vec![tz_allowed(None, Some("ext-x"))]);
    let ext = DeaconExtension {
        id: "ext-x".to_string(),
        behaviors: vec!["bhv-x".to_string()],
        description: "d".to_string(),
        docs: None,
    };
    let reg = registry_with(vec![case], vec![], vec![ext]);
    assert!(
        !run(&reg, TODAY, &workspace_root())
            .iter()
            .any(|v| v.code == "V19")
    );

    // intentional-divergence behavior backing.
    let mut ad = tz_allowed(None, Some("bhv-intentional"));
    ad.observable_path = "chan-injected-process.env.LANG".to_string();
    let case2 = case_with(vec![ad]);
    let reg2 = registry_with(vec![case2], vec![], vec![]);
    assert!(
        !run(&reg2, TODAY, &workspace_root())
            .iter()
            .any(|v| v.code == "V19")
    );
}

#[test]
fn dangling_waiver_id_is_v19() {
    let case = case_with(vec![tz_allowed(Some("wvr-does-not-exist"), None)]);
    let reg = registry_with(vec![case], vec![], vec![]);
    let out = run(&reg, TODAY, &workspace_root());
    assert!(
        out.iter()
            .any(|v| v.code == "V19" && v.message.contains("wvr-does-not-exist")),
        "a dangling waiverId must be V19, got {out:?}"
    );
}

#[test]
fn dangling_divergence_id_is_v19() {
    let case = case_with(vec![tz_allowed(None, Some("ext-nope"))]);
    let reg = registry_with(vec![case], vec![], vec![]);
    let out = run(&reg, TODAY, &workspace_root());
    assert!(
        out.iter()
            .any(|v| v.code == "V19" && v.message.contains("ext-nope")),
        "a dangling divergenceId must be V19, got {out:?}"
    );
}

#[test]
fn tolerance_scoped_to_unlinked_behavior_is_v19() {
    let mut ad = tz_allowed(Some("wvr-x"), None);
    ad.behavior = "bhv-not-linked".to_string();
    let case = case_with(vec![ad]);
    let reg = registry_with(vec![case], vec![waiver("wvr-x")], vec![]);
    let out = run(&reg, TODAY, &workspace_root());
    assert!(
        out.iter()
            .any(|v| v.code == "V19" && v.message.contains("bhv-not-linked")),
        "a tolerance scoped to a behavior the case does not link must be V19, got {out:?}"
    );
}

/// Write a temp registry with just a `cases.json` and load it — the load-time structural
/// checks fire even with the rest of the registry empty.
fn load_temp_cases(cases_json: &str) -> Result<Registry, LoadError> {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("cases.json"), cases_json).expect("write cases.json");
    // Keep the tempdir alive for the load by leaking it into a thread-local is overkill;
    // load reads synchronously before this returns, so a local binding suffices.
    let result = Registry::load(dir.path());
    drop(dir);
    result
}

#[test]
fn conflicting_duplicate_fails_at_load() {
    // Two allowed differences with the SAME (behavior, observablePath) — a conflict.
    let json = r#"{
      "schemaVersion": 1,
      "records": [
        {
          "id": "case-dup",
          "behaviors": ["bhv-x"],
          "context": [],
          "oracleType": "live-differential",
          "operations": [ { "id": "op", "subcommand": "up", "argv": [] } ],
          "expected": [ { "channel": "chan-injected-process", "operation": "op" } ],
          "allowedDifferences": [
            { "behavior": "bhv-x", "context": [], "observablePath": "chan-injected-process.env.TZ",
              "rationale": "a", "waiverId": "wvr-a" },
            { "behavior": "bhv-x", "context": [], "observablePath": "chan-injected-process.env.TZ",
              "rationale": "b", "waiverId": "wvr-b" }
          ]
        }
      ]
    }"#;
    let err = load_temp_cases(json).expect_err("a conflicting duplicate must fail to load");
    match err {
        LoadError::Schema(errors) => assert!(
            errors
                .iter()
                .any(|e| e.message.contains("duplicate allowed difference")),
            "the located error must name the conflict, got {errors:?}"
        ),
        other => panic!("expected Schema error, got {other:?}"),
    }
}

#[test]
fn global_ignore_construct_fails_at_load() {
    // A bare-channel observablePath (no dotted sub-path) is a global ignore — rejected.
    let json = r#"{
      "schemaVersion": 1,
      "records": [
        {
          "id": "case-global",
          "behaviors": ["bhv-x"],
          "context": [],
          "oracleType": "live-differential",
          "operations": [ { "id": "op", "subcommand": "up", "argv": [] } ],
          "expected": [ { "channel": "chan-injected-process", "operation": "op" } ],
          "allowedDifferences": [
            { "behavior": "bhv-x", "context": [], "observablePath": "chan-injected-process",
              "rationale": "ignore the whole channel", "waiverId": "wvr-a" }
          ]
        }
      ]
    }"#;
    let err = load_temp_cases(json).expect_err("a bare-channel global ignore must fail to load");
    match err {
        LoadError::Schema(errors) => assert!(
            errors.iter().any(|e| e.message.contains("global-ignore")),
            "the located error must name the global-ignore rejection, got {errors:?}"
        ),
        other => panic!("expected Schema error, got {other:?}"),
    }
}

#[test]
fn both_and_neither_backing_ids_fail_at_load() {
    let both = r#"{
      "schemaVersion": 1,
      "records": [ {
        "id": "case-both", "behaviors": ["bhv-x"], "context": [],
        "oracleType": "live-differential",
        "operations": [ { "id": "op", "subcommand": "up", "argv": [] } ],
        "expected": [ { "channel": "chan-injected-process", "operation": "op" } ],
        "allowedDifferences": [ { "behavior": "bhv-x", "context": [],
          "observablePath": "chan-injected-process.env.TZ", "rationale": "r",
          "waiverId": "wvr-a", "divergenceId": "ext-a" } ]
      } ]
    }"#;
    let err = load_temp_cases(both).expect_err("both ids must fail");
    assert!(matches!(err, LoadError::Schema(_)));

    let neither = r#"{
      "schemaVersion": 1,
      "records": [ {
        "id": "case-neither", "behaviors": ["bhv-x"], "context": [],
        "oracleType": "live-differential",
        "operations": [ { "id": "op", "subcommand": "up", "argv": [] } ],
        "expected": [ { "channel": "chan-injected-process", "operation": "op" } ],
        "allowedDifferences": [ { "behavior": "bhv-x", "context": [],
          "observablePath": "chan-injected-process.env.TZ", "rationale": "r" } ]
      } ]
    }"#;
    let err = load_temp_cases(neither).expect_err("neither id must fail");
    match err {
        LoadError::Schema(errors) => assert!(
            errors.iter().any(|e| e.message.contains("NEITHER")),
            "must name the missing backing identity, got {errors:?}"
        ),
        other => panic!("expected Schema error, got {other:?}"),
    }
}
