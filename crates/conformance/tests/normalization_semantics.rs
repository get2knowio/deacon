//! Conformance-side normalization coverage tests (022-conformance-runner, US3 T048).
//!
//! The named normalization RULES (`path_token`, `label_semantic`,
//! `mount_source_canonical`, `path_env_segmented`, `null_preserving`) live in
//! `parity-harness::normalize` and are unit-tested there (T039/T040) — the conformance
//! crate is hermetic and cannot execute them. What the conformance crate owns is the
//! `report`'s per-channel normalized-evidence coverage: this asserts that surface (T048)
//! reflects the declarative cases' declared channels.

use deacon_conformance::load::Registry;
use deacon_conformance::model::{
    ExpectedObservable, ObservableChannel, Operation, OracleType, TestCase,
};
use deacon_conformance::report::build_report;
use deacon_conformance::{default_registry_dir, model::CaseKind};

fn channel(id: &str) -> ObservableChannel {
    ObservableChannel {
        id: id.to_string(),
        description: "c".to_string(),
    }
}

fn declarative_case(id: &str, channels: &[(&str, bool)]) -> TestCase {
    TestCase {
        id: id.to_string(),
        behaviors: vec!["bhv-x".to_string()],
        oracle_type: Some(OracleType::SpecExpectation),
        operations: vec![Operation {
            id: "op".to_string(),
            subcommand: "read-configuration".to_string(),
            ..Operation::default()
        }],
        expected: channels
            .iter()
            .map(|(ch, asserted)| ExpectedObservable {
                channel: ch.to_string(),
                operation: Some("op".to_string()),
                assertion: asserted.then(|| serde_json::json!({ "equals": 0 })),
            })
            .collect(),
        ..TestCase::default()
    }
}

#[test]
fn report_surfaces_per_channel_declarative_coverage() {
    let reg = Registry {
        channels: vec![channel("chan-exit-code"), channel("chan-filesystem")],
        cases: vec![
            declarative_case("case-a", &[("chan-exit-code", true)]),
            declarative_case(
                "case-b",
                &[("chan-exit-code", true), ("chan-filesystem", false)],
            ),
        ],
        ..Registry::default()
    };
    let report = build_report(&reg, None);

    let by_channel = |id: &str| {
        report
            .channel_coverage
            .iter()
            .find(|c| c.channel == id)
            .unwrap_or_else(|| panic!("channel {id} present in coverage"))
    };
    // Every declared channel appears (stable shape), channel-id-sorted.
    assert_eq!(report.channel_coverage.len(), 2);
    assert_eq!(report.channel_coverage[0].channel, "chan-exit-code");

    let exit = by_channel("chan-exit-code");
    assert_eq!(exit.declarative_cases, 2, "both cases exercise exit-code");
    assert_eq!(exit.asserted_expectations, 2);

    let fs = by_channel("chan-filesystem");
    assert_eq!(fs.declarative_cases, 1, "only case-b exercises filesystem");
    assert_eq!(
        fs.asserted_expectations, 0,
        "case-b's filesystem expectation carries no assertion"
    );
}

#[test]
fn real_registry_report_covers_the_declarative_mvp_channels() {
    // The real registry's declarative cases exercise exit-code, structured-output, and
    // filesystem — the report must surface each with a nonzero declarative-case count.
    let reg = Registry::load(&default_registry_dir()).expect("real registry loads");
    let declarative = reg
        .cases
        .iter()
        .filter(|c| matches!(c.classify(), Ok(CaseKind::Declarative)))
        .count();
    assert!(declarative >= 3, "US1+US3 add declarative cases");

    let report = build_report(&reg, None);
    for ch in [
        "chan-exit-code",
        "chan-structured-output",
        "chan-filesystem",
    ] {
        let row = report
            .channel_coverage
            .iter()
            .find(|c| c.channel == ch)
            .unwrap_or_else(|| panic!("{ch} in coverage"));
        assert!(
            row.declarative_cases > 0,
            "{ch} must be exercised by >= 1 declarative case, got {row:?}"
        );
    }
}
