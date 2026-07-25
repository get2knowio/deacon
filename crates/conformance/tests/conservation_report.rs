//! T066 (US5, FR-041 / FR-047): removing a migrated case makes the report FAIL, naming
//! the item, its origin program, and what it asserted.
//!
//! This is the feature's headline claim under test. "No coverage was lost" is worthless
//! as a slogan and worthless as an aggregate count — two sets of the same size can have
//! lost one item and gained another. So the report must name the specific thing that went
//! missing, where it came from, and what it proved, or a reviewer cannot act on it.
//!
//! Hermetic: builds modified copies of the real registry under a tempdir. No Docker, no
//! network.

mod support;

use deacon_conformance::conservation::POST_BRANCH_BEHAVIORS;
use deacon_conformance::conservation::ReportError;
use support::Fixture;

#[test]
fn the_real_registry_accounts_for_every_baseline_item() {
    let fixture = Fixture::real();
    let report = fixture
        .report()
        .expect("the real registry produces a report");
    assert!(
        report.is_clean(),
        "the committed migration must account for every baseline item:\n{:#?}",
        report.violations
    );
    assert!(
        report.accounting.unaccounted.is_empty(),
        "{:#?}",
        report.accounting.unaccounted
    );
    assert_eq!(
        report.accounting.migrated
            + report.accounting.deduplicated
            + report.accounting.residual
            + report.accounting.retired,
        report.totals.before.units,
        "every executable baseline unit needs exactly one disposition"
    );
}

#[test]
fn removing_a_mapping_entry_names_the_unit_its_program_and_its_assertion() {
    let fixture = Fixture::real();
    // The unit the mapping will forget about.
    let dropped = "parity_corpus_tier1::node-ts";
    let baseline_unit = fixture
        .baseline_unit(dropped)
        .expect("the unit is in the committed baseline");

    let fixture = fixture.without_mapping_entry(dropped);
    let report = fixture.report().expect("report still computes");

    assert!(!report.is_clean(), "a forgotten unit must fail the report");
    let hit = report
        .accounting
        .unaccounted
        .iter()
        .find(|u| u.unit == dropped)
        .unwrap_or_else(|| panic!("the dropped unit must be named: {:#?}", report.accounting));

    assert_eq!(hit.program, baseline_unit.program, "its origin program");
    assert_eq!(hit.assertion, baseline_unit.assertion, "what it asserted");
    assert!(
        !hit.assertion.trim().is_empty(),
        "an unaccounted item with no recorded assertion is unactionable"
    );

    // …and it surfaces as failure condition 1, with all three facts in the message.
    let violation = report
        .violations
        .iter()
        .find(|v| v.item == dropped)
        .expect("a violation names the unit");
    assert_eq!(violation.condition, 1);
    assert!(
        violation.message.contains(&baseline_unit.program)
            && violation.message.contains(&baseline_unit.assertion),
        "the violation must carry the origin and the assertion, not just an id: {}",
        violation.message
    );
}

#[test]
fn removing_a_migrated_case_fails_naming_the_case_and_the_unit_that_needed_it() {
    let fixture = Fixture::real().without_case("case-tier1-decl-node-ts");
    let report = fixture.report().expect("report still computes");

    assert!(!report.is_clean(), "a vanished destination must fail");
    let violation = report
        .violations
        .iter()
        .find(|v| v.item == "case-tier1-decl-node-ts")
        .unwrap_or_else(|| panic!("the missing case must be named: {:#?}", report.violations));
    assert_eq!(violation.condition, 2, "a missing counterpart");
    assert!(
        violation.message.contains("parity_corpus_tier1::node-ts"),
        "the message must say WHICH unit lost its destination: {}",
        violation.message
    );
}

#[test]
fn the_disposition_sum_must_equal_the_executable_unit_count() {
    // Failure condition 5. Removing a mapping entry breaks the sum as well as leaving an
    // orphan — both are reported, because they are different facts.
    let report = Fixture::real()
        .without_mapping_entry("parity_corpus_tier1::node-ts")
        .report()
        .expect("report computes");
    assert!(
        report
            .violations
            .iter()
            .any(|v| v.condition == 5 && v.message.contains("exactly one disposition")),
        "{:#?}",
        report.violations
    );
}

#[test]
fn a_report_cannot_be_produced_without_a_baseline() {
    // The absence of a baseline is NOT "zero unaccounted units". Reporting a clean
    // accounting for a comparison that never happened is the silent pass this whole
    // feature exists to prevent.
    let fixture = Fixture::real().without_baseline();
    match fixture.report() {
        Err(ReportError::NoBaseline) => {}
        Ok(report) => panic!(
            "a missing baseline must fail loud, not report {} clean unit(s)",
            report.totals.before.units
        ),
    }
}

#[test]
fn every_before_behavior_still_has_a_counterpart() {
    let report = Fixture::real().report().expect("report computes");
    assert!(
        !report
            .violations
            .iter()
            .any(|v| v.condition == 2 && v.item.starts_with("bhv-")),
        "no pre-migration behavior may lose its counterpart: {:#?}",
        report.violations
    );
    assert_eq!(
        report.totals.after.behaviors,
        report.totals.before.behaviors + POST_BRANCH_BEHAVIORS.len(),
        "the behavior denominator holds, except for behaviors explicitly accounted as \
         newly OBSERVED rather than re-described (conservation::POST_BRANCH_BEHAVIORS)"
    );
}

#[test]
fn the_residual_queue_and_deletion_blockers_are_populated_and_consistent() {
    let report = Fixture::real().report().expect("report computes");
    assert!(
        !report.residual_queue.is_empty(),
        "US2 authored a residual queue; an empty one would make this vacuous"
    );

    // Every carrier a residual blocks must appear as a deletion blocker naming it.
    for residual in &report.residual_queue {
        let Some(carrier) = residual.blocked_carrier.as_deref() else {
            continue;
        };
        let blocker = report
            .deletion_blockers
            .iter()
            .find(|b| b.carrier == carrier)
            .unwrap_or_else(|| panic!("carrier `{carrier}` is blocked but not reported"));
        assert!(
            blocker.reason.contains(&residual.id),
            "the blocker must name the residual pinning it: {}",
            blocker.reason
        );
        assert!(
            !report.deletable_carriers.contains(&carrier.to_string()),
            "a carrier a residual blocks may never be listed as deletable"
        );
    }

    // With no equivalence ledger, NOTHING is deletable — unproven is not safe.
    assert!(
        report.deletable_carriers.is_empty(),
        "no carrier may be declared deletable before the equivalence ledger clears it"
    );
}

/// 024 P1: a carrier pinned only by PERMANENT exclusions is reported as permanently pinned,
/// not as a pending blocker.
///
/// Without the distinction the blocker list reads as a queue — a reviewer seeing nine blocked
/// carriers infers nine pending deletions, when four of them can never become deletable. That
/// is the same defect as folding permanent residuals into `residualQueue`, one level up.
#[test]
fn permanently_pinned_carriers_are_distinguished_from_pending_blockers() {
    let fixture = Fixture::real();
    let report = fixture
        .report()
        .expect("the real registry produces a report");

    let permanent: Vec<&str> = report
        .deletion_blockers
        .iter()
        .filter(|b| b.permanent)
        .map(|b| b.carrier.as_str())
        .collect();
    let pending: Vec<&str> = report
        .deletion_blockers
        .iter()
        .filter(|b| !b.permanent)
        .map(|b| b.carrier.as_str())
        .collect();

    // The harness self-test and repository-structural carriers observe the comparator and the
    // repo, so no declarative case can ever replace them.
    for carrier in ["parity_harness_faults", "parity_registry_check"] {
        assert!(
            permanent.contains(&carrier),
            "`{carrier}` is pinned only by a permanent exclusion, so it must not read as \
             pending work; permanent = {permanent:?}, pending = {pending:?}"
        );
    }

    // A carrier with even ONE queued residual is still pending: `parity_observable_state`
    // carries a permanent lockfile-interop residual AND three migratable ones.
    assert!(
        pending.contains(&"parity_observable_state"),
        "a carrier with any migratable residual is pending, not settled: pending = {pending:?}"
    );

    assert!(
        !pending.is_empty() && !permanent.is_empty(),
        "both lists must be populated or the distinction is vacuous"
    );

    // The rendering must not silently merge them back together.
    let md = deacon_conformance::conservation::render_report_md(&report);
    assert!(
        md.contains("permanently pinned"),
        "the Markdown must report the permanent count so the blocker list is not read as a queue"
    );
}
