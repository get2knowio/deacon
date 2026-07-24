//! Hermetic declarative-case schema tests (022-conformance-runner, US1 T015).
//!
//! A well-formed declarative case loads and validates cleanly; an unknown behavior, an
//! undeclared channel, a legacy+declarative mix, and a non-consumer subcommand each fail
//! loudly with a located message (FR-001..004, FR-003). No Docker, no network — runs in
//! every lane including the Windows `dev-fast` lane.

use std::fs;

use deacon_conformance::load::{LoadError, Registry};
use deacon_conformance::model::{
    BehaviorUnit, CaseKind, Decision, ExpectedObservable, ObservableChannel, Operation, OracleType,
    ReferenceStatus, SpecStatus, TestCase,
};
use deacon_conformance::validate::run;
use deacon_conformance::{default_registry_dir, workspace_root};

const TODAY: &str = "2026-07-24";

fn behavior(id: &str) -> BehaviorUnit {
    BehaviorUnit {
        id: id.to_string(),
        area: "test".to_string(),
        statement: "a behavior".to_string(),
        applicability: vec![],
        spec: SpecStatus::Conformant,
        reference: ReferenceStatus::Aligned,
        decision: Decision::FollowSpec,
        notes: None,
    }
}

fn channel(id: &str) -> ObservableChannel {
    ObservableChannel {
        id: id.to_string(),
        description: "a channel".to_string(),
    }
}

/// A well-formed declarative spec-expectation case linked to `bhv-x`, exit-code only.
fn well_formed_case() -> TestCase {
    TestCase {
        id: "case-decl-ok".to_string(),
        behaviors: vec!["bhv-x".to_string()],
        oracle_type: Some(OracleType::SpecExpectation),
        operations: vec![Operation {
            id: "op-1".to_string(),
            subcommand: "read-configuration".to_string(),
            argv: vec!["--workspace-folder".to_string(), "${WORKSPACE}".to_string()],
            fixtures: vec!["fx-x".to_string()],
            ..Operation::default()
        }],
        expected: vec![ExpectedObservable {
            channel: "chan-exit-code".to_string(),
            operation: Some("op-1".to_string()),
            assertion: Some(serde_json::json!({ "equals": 0 })),
        }],
        ..TestCase::default()
    }
}

/// A registry carrying `bhv-x` + `chan-exit-code` and the given cases.
fn registry_with(cases: Vec<TestCase>) -> Registry {
    Registry {
        behaviors: vec![behavior("bhv-x")],
        channels: vec![channel("chan-exit-code")],
        cases,
        ..Registry::default()
    }
}

#[test]
fn well_formed_declarative_case_has_no_case_schema_violations() {
    // A well-formed declarative case trips no case-schema class: no V16 (shape /
    // subcommand / assertion / fsAllowlist) and no V9 on the case (its channel is
    // declared). Registry-completeness classes (a behavior needing a source unit, V1)
    // are orthogonal to case schema and are exercised elsewhere.
    let reg = registry_with(vec![well_formed_case()]);
    let out = run(&reg, TODAY, &workspace_root());
    assert!(
        !out.iter()
            .any(|v| v.code == "V16" || (v.code == "V9" && v.record == "case-decl-ok")),
        "a well-formed declarative case must trip no case-schema violation, got {out:?}"
    );
    assert_eq!(reg.cases[0].classify().unwrap(), CaseKind::Declarative);
}

#[test]
fn the_real_registry_carries_the_declarative_mvp_cases() {
    // The two 022 MVP cases live in the authoritative registry and load + validate as
    // part of it (registry_valid guards the whole set; here we assert their presence and
    // shape specifically).
    let reg = Registry::load(&default_registry_dir()).expect("real registry loads");
    let spec = reg
        .cases
        .iter()
        .find(|c| c.id == "case-readconfig-unknown-field-echo")
        .expect("spec-expectation MVP case present");
    assert_eq!(spec.classify().unwrap(), CaseKind::Declarative);
    assert_eq!(spec.oracle_type, Some(OracleType::SpecExpectation));
    let diff = reg
        .cases
        .iter()
        .find(|c| c.id == "case-readconfig-parity-exit")
        .expect("live-differential MVP case present");
    assert_eq!(diff.oracle_type, Some(OracleType::LiveDifferential));
}

#[test]
fn unknown_behavior_fails_loud_and_located() {
    let mut case = well_formed_case();
    case.behaviors = vec!["bhv-does-not-exist".to_string()];
    let reg = registry_with(vec![case]);
    let out = run(&reg, TODAY, &workspace_root());
    assert!(
        out.iter()
            .any(|v| v.code == "V1" && v.record == "case-decl-ok"),
        "an unknown behavior must be a located V1, got {out:?}"
    );
}

#[test]
fn undeclared_channel_fails_loud() {
    let mut case = well_formed_case();
    case.expected[0].channel = "chan-ghost".to_string();
    let reg = registry_with(vec![case]);
    let out = run(&reg, TODAY, &workspace_root());
    assert!(
        out.iter().any(|v| v.code == "V9"
            && v.record == "case-decl-ok"
            && v.message.contains("chan-ghost")),
        "an undeclared expected channel must be a located V9, got {out:?}"
    );
}

#[test]
fn non_consumer_subcommand_fails_loud() {
    let mut case = well_formed_case();
    case.operations[0].subcommand = "features".to_string(); // authoring-only, out of scope
    let reg = registry_with(vec![case]);
    let out = run(&reg, TODAY, &workspace_root());
    assert!(
        out.iter().any(|v| v.code == "V16"
            && v.record == "case-decl-ok"
            && v.message.contains("features")),
        "a non-consumer subcommand must be a located V16 (Principle II), got {out:?}"
    );
}

#[test]
fn legacy_declarative_mix_fails_loud_at_load() {
    // A record with BOTH `executable` (legacy) and `operations` (declarative) is a
    // structural malformation the loader rejects fail-loud, naming the offending case.
    let dir = tempfile::tempdir().expect("tempdir");
    let cases_json = r#"{
      "schemaVersion": 1,
      "records": [
        {
          "id": "case-mixed",
          "behaviors": ["bhv-x"],
          "context": [],
          "executable": { "binary": "some_binary" },
          "operations": [
            { "id": "op", "subcommand": "read-configuration", "argv": [] }
          ]
        }
      ]
    }"#;
    fs::write(dir.path().join("cases.json"), cases_json).expect("write cases.json");

    let err = Registry::load(dir.path()).expect_err("a mixed case must fail to load");
    match err {
        LoadError::Schema(errors) => {
            assert!(
                errors.iter().any(|e| {
                    e.location
                        .as_deref()
                        .is_some_and(|l| l.contains("case-mixed"))
                        && e.message.contains("BOTH")
                }),
                "the located schema error must name `case-mixed` and the mix, got {errors:?}"
            );
        }
        other => panic!("expected a located Schema error, got {other:?}"),
    }
}
