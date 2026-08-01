//! T067 (US5, FR-042): a counterpart that loses a rejection's **direction** or its
//! **diagnostic expectation** fails the report.
//!
//! Rejections are the coverage most easily lost in a migration, because a case that no
//! longer asserts a rejection still looks like a passing case. Two distinct ways to lose
//! one:
//!
//! - **direction** — the counterpart stops asserting that the operation is *rejected*.
//!   A bare live-differential is the canonical example: it asserts "deacon's exit code
//!   equals the reference's", which passes just as happily once BOTH CLIs start accepting
//!   the input. The rejection evaporates and nothing turns red.
//! - **diagnostic** — the counterpart still asserts a rejection but no longer asserts
//!   anything about the message, so a rejection with a useless diagnostic passes.
//!
//! Hermetic: mutates copies of the real registry.

mod support;

use deacon_conformance::conservation::ErrorPathLoss;
use support::Fixture;

/// A migrated unit whose expectation is a rejection, and its decision-pinning case.
const REJECTION_UNIT: &str = "parity_corpus_errors::malformed-json";
const REJECTION_CASE: &str = "case-errors-decl-malformed-json";

#[test]
fn every_real_rejection_preserved_its_direction_and_diagnostic() {
    let report = Fixture::real().report().expect("report computes");
    assert!(
        report.error_paths.before > 0,
        "the baseline records rejection units; zero would make this vacuous"
    );
    assert_eq!(
        report.error_paths.preserved, report.error_paths.before,
        "every rejection must keep its direction and diagnostic; weakened: {:#?}",
        report.error_paths.weakened
    );
    assert!(report.error_paths.weakened.is_empty());
}

#[test]
fn losing_the_direction_is_reported_naming_the_unit() {
    // Strip the exit-code assertion: the case still runs, still names the channel, and
    // still passes — while asserting nothing about whether the input is rejected.
    let report = Fixture::real()
        .edit_case(REJECTION_CASE, |case| {
            if let Some(expected) = case.get_mut("expected").and_then(|v| v.as_array_mut()) {
                for exp in expected.iter_mut() {
                    if exp.get("channel").and_then(|v| v.as_str()) == Some("chan-exit-code") {
                        exp.as_object_mut().expect("object").remove("assertion");
                    }
                }
            }
        })
        .report()
        .expect("report computes");

    let weakened = report
        .error_paths
        .weakened
        .iter()
        .find(|w| w.unit == REJECTION_UNIT)
        .unwrap_or_else(|| {
            panic!(
                "the weakened rejection must be named: {:#?}",
                report.error_paths
            )
        });
    assert_eq!(weakened.lost, ErrorPathLoss::Direction);
    assert!(
        weakened.cases.contains(&REJECTION_CASE.to_string()),
        "the fix site must be named: {:?}",
        weakened.cases
    );

    let violation = report
        .violations
        .iter()
        .find(|v| v.item == REJECTION_UNIT)
        .expect("a violation names the unit");
    assert_eq!(violation.condition, 3);
    assert!(
        violation.message.contains("direction"),
        "{}",
        violation.message
    );
    assert!(!report.is_clean());
}

#[test]
fn losing_the_diagnostic_is_reported_naming_the_unit() {
    // Keep the rejection, drop the stderr expectation: deacon must still fail, but a
    // rejection with any message at all would now pass.
    let report = Fixture::real()
        .edit_case(REJECTION_CASE, |case| {
            if let Some(expected) = case.get_mut("expected").and_then(|v| v.as_array_mut()) {
                expected.retain(|exp| {
                    exp.get("channel").and_then(|v| v.as_str()) != Some("chan-stderr")
                });
            }
        })
        .report()
        .expect("report computes");

    let weakened = report
        .error_paths
        .weakened
        .iter()
        .find(|w| w.unit == REJECTION_UNIT)
        .unwrap_or_else(|| {
            panic!(
                "the weakened rejection must be named: {:#?}",
                report.error_paths
            )
        });
    assert_eq!(weakened.lost, ErrorPathLoss::Diagnostic);
    assert_eq!(weakened.lost.as_str(), "diagnostic");
    assert!(!report.is_clean());
}

#[test]
fn a_rejection_whose_unit_became_a_residual_is_preserved_by_its_carrier() {
    // Turning a migrated rejection into a residual does NOT weaken it: the carrier still
    // runs and still asserts both halves. Residuals are representation debt, not lost
    // coverage — reporting them as weakened would cry wolf on every residual.
    let report = Fixture::real()
        .edit_mapping_entry(REJECTION_UNIT, |entry| {
            let obj = entry.as_object_mut().expect("object");
            obj.insert("disposition".into(), serde_json::json!("residual"));
            obj.insert("caseIds".into(), serde_json::json!([]));
            obj.insert(
                "residualId".into(),
                serde_json::json!("res-harness-fault-injection"),
            );
        })
        .report()
        .expect("report computes");

    assert!(
        !report
            .error_paths
            .weakened
            .iter()
            .any(|w| w.unit == REJECTION_UNIT),
        "a residual's rejection is still asserted by its carrier: {:#?}",
        report.error_paths.weakened
    );
}

#[test]
fn a_reference_side_rejection_does_not_require_a_deacon_diagnostic() {
    // `parity_corpus_merged::extends-child` is the one rejection whose rejecting side is
    // the REFERENCE: deacon resolves the extends chain where the reference errors. Its
    // counterpart pins deacon at success, so demanding a deacon stderr diagnostic would
    // demand a message deacon never emits — the reference-side diagnostic is carried by
    // the `ext-extends-resolution` record instead (the `wvr-extends-child-merged` waiver
    // that used to duplicate it was retired 2026-08-01).
    let report = Fixture::real().report().expect("report computes");
    assert!(
        !report
            .error_paths
            .weakened
            .iter()
            .any(|w| w.unit == "parity_corpus_merged::extends-child"),
        "a reference-side rejection must not be reported as a lost deacon diagnostic"
    );
}
