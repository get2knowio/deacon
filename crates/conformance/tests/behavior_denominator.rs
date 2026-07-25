//! T049 (US3, FR-014 / SC-005 / FR-047): mapping duplicate coverage adds **cases**, not
//! **behaviors**.
//!
//! The migration converts one coarse pointer case into many fine-grained ones. That is
//! the point — but it is also the easiest way to fake progress: authoring each new case
//! against a *new* behavior would make the registry look richer while proving exactly
//! the same things. So the behavior count is a frozen denominator
//! ([`PRE_MIGRATION_BEHAVIORS`] = 25, research §1g), and the merged-mode corpus cases are
//! **variants** of the tier-1 behaviors — the same claim, distinguished by the
//! `--include-merged-configuration` input shape.
//!
//! The converse matters too and is checked here: two cases sharing a behavior must
//! differ on some variant axis, or they are one piece of evidence counted twice.
//!
//! Hermetic: reads the real registry and evaluates pure functions. No Docker, no
//! network, no oracle.

use deacon_conformance::conservation::{
    POST_BRANCH_BEHAVIORS, PRE_MIGRATION_BEHAVIORS, denominator_counts, registry_variant_groups,
    stale_post_branch_behaviors,
};
use deacon_conformance::default_registry_dir;
use deacon_conformance::load::Registry;
use deacon_conformance::mapping::{CaseFacts, VariantAxis, check_variants, distinguishing_axes};

fn registry() -> Registry {
    Registry::load(&default_registry_dir()).expect("the real registry loads cleanly")
}

fn case(id: &str, behavior: &str, oracle: &str, input_shape: &str) -> CaseFacts {
    CaseFacts {
        id: id.to_string(),
        behaviors: vec![behavior.to_string()],
        channels: vec!["chan-exit-code".to_string()],
        fixtures: Vec::new(),
        declarative: true,
        context: Vec::new(),
        oracle: oracle.to_string(),
        input_shape: input_shape.to_string(),
    }
}

#[test]
fn the_behavior_denominator_did_not_grow() {
    let counts = denominator_counts(&registry());
    assert!(
        !counts.denominator_inflated(),
        "the migration must not inflate the behavior denominator: {} behaviors now, \
         {PRE_MIGRATION_BEHAVIORS} before (SC-005), plus {} explicitly accounted in \
         POST_BRANCH_BEHAVIORS. A variant authored as a new behavior is the usual cause.",
        counts.behaviors,
        POST_BRANCH_BEHAVIORS.len()
    );
    assert_eq!(
        counts.behaviors,
        PRE_MIGRATION_BEHAVIORS + POST_BRANCH_BEHAVIORS.len(),
        "the behavior count holds at the frozen pre-migration total plus the behaviors \
         explicitly accounted for as newly OBSERVED (not re-described) facts"
    );
}

/// Every accounted post-branch behavior must still exist, and must carry a reason.
///
/// An allowance for a behavior since deleted or renamed raises the ceiling by one
/// permanently — the "number the claimant can move" the frozen denominator exists to
/// prevent, re-entering through the exception list. Same self-invalidating discipline as
/// a waiver whose difference stopped reproducing.
#[test]
fn the_post_branch_allowance_cannot_go_stale() {
    let registry = registry();
    assert!(
        stale_post_branch_behaviors(&registry).is_empty(),
        "POST_BRANCH_BEHAVIORS names behaviors that no longer resolve: {:?}",
        stale_post_branch_behaviors(&registry)
    );
    for (id, reason) in POST_BRANCH_BEHAVIORS {
        assert!(
            reason.split_whitespace().count() >= 20,
            "{id} must say why it is not a variant of an existing behavior, in enough \
             detail to review; got {reason:?}"
        );
    }
}

#[test]
fn the_case_count_rose_while_the_behavior_count_held() {
    let counts = denominator_counts(&registry());
    // Research §1g: the branch point had 31 cases (25 legacy pointers + 6 declarative).
    assert!(
        counts.cases > 31,
        "the migration should ADD cases; found {}",
        counts.cases
    );
    assert!(
        counts.declarative_cases > counts.legacy_cases,
        "declarative destinations ({}) should now outnumber legacy pointer carriers ({})",
        counts.declarative_cases,
        counts.legacy_cases
    );
    assert!(
        counts.behaviors <= PRE_MIGRATION_BEHAVIORS + POST_BRANCH_BEHAVIORS.len(),
        "…while the behavior count holds, falls, or grows only by an explicitly \
         accounted post-branch behavior"
    );
}

#[test]
fn the_merged_mode_cases_are_variants_of_existing_behaviors() {
    let registry = registry();
    let merged: Vec<&deacon_conformance::model::TestCase> = registry
        .cases
        .iter()
        .filter(|c| c.id.starts_with("case-merged-decl-"))
        .collect();
    assert_eq!(merged.len(), 24, "one merged-mode variant per tier-1 case");

    let known: std::collections::BTreeSet<&str> =
        registry.behaviors.iter().map(|b| b.id.as_str()).collect();
    for case in &merged {
        for behavior in &case.behaviors {
            assert!(
                known.contains(behavior.as_str()),
                "merged-mode case {} must reuse an EXISTING behavior, not introduce one; \
                 {behavior} is new",
                case.id
            );
        }
    }

    // And they must genuinely be variants: the same behavior, a different input shape.
    let groups = registry_variant_groups(&registry);
    let merged_group = groups
        .iter()
        .find(|g| g.behavior == "bhv-readconfig-merged-configuration")
        .expect("the merged-configuration behavior has a variant group");
    assert!(
        merged_group.is_variant_group(),
        "the merged-mode behavior must be reached by more than one case"
    );
    assert!(
        merged_group
            .distinguished_by
            .contains(&VariantAxis::InputShape),
        "merged-mode variants are distinguished by input shape, got {:?}",
        merged_group.distinguished_by
    );
}

#[test]
fn the_tier1_corpus_behavior_carries_its_variants() {
    let groups = registry_variant_groups(&registry());
    let tier1 = groups
        .iter()
        .find(|g| g.behavior == "bhv-readconfig-tier1-corpus")
        .expect("the tier-1 corpus behavior has a variant group");
    assert!(
        tier1.cases.len() >= 24,
        "the tier-1 corpus behavior should carry its 24 per-workspace variants plus the \
         legacy pointer, found {}",
        tier1.cases.len()
    );
    assert!(
        tier1.distinguished_by.contains(&VariantAxis::InputShape),
        "per-workspace variants differ by input shape, got {:?}",
        tier1.distinguished_by
    );
}

#[test]
fn a_variant_wrongly_authored_as_a_new_behavior_is_reported() {
    // The failure this test guards: authoring the merged-mode case against a NEW
    // behavior instead of reusing the tier-1 one. That inflates the denominator without
    // proving anything new, and is invisible in a raw case count.
    let mut counts = denominator_counts(&registry());
    assert!(!counts.denominator_inflated());

    // As if one variant had been given its own behavior — one PAST the accounted
    // allowance, so this stays a real inflation test as the allowance list grows.
    counts.behaviors = PRE_MIGRATION_BEHAVIORS + POST_BRANCH_BEHAVIORS.len() + 1;
    assert!(
        counts.denominator_inflated(),
        "one extra behavior beyond the frozen total plus the accounted allowance must be \
         reported as inflation"
    );
}

#[test]
fn two_indistinguishable_cases_sharing_a_behavior_are_rejected() {
    let a = case(
        "case-a",
        "bhv-x",
        "LiveDifferential",
        "read-configuration [fx]",
    );
    let mut b = a.clone();
    b.id = "case-b".to_string();

    let problems = check_variants(&[a.clone(), b.clone()]);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].code, "V21");
    assert!(
        problems[0].message.contains("indistinguishable"),
        "{}",
        problems[0].message
    );
    assert!(distinguishing_axes(&a, &b).is_empty());
}

#[test]
fn cases_differing_on_any_axis_are_legitimate_variants() {
    let base = case(
        "case-a",
        "bhv-x",
        "LiveDifferential",
        "read-configuration [fx]",
    );

    let mut by_input = base.clone();
    by_input.id = "case-b".to_string();
    by_input.input_shape = "read-configuration --include-merged-configuration [fx]".to_string();

    let mut by_oracle = base.clone();
    by_oracle.id = "case-c".to_string();
    by_oracle.oracle = "SpecExpectation".to_string();

    let mut by_channel = base.clone();
    by_channel.id = "case-d".to_string();
    by_channel.channels = vec!["chan-stdout".to_string()];

    let mut by_context = base.clone();
    by_context.id = "case-e".to_string();
    by_context.context = vec!["compose".to_string()];

    assert_eq!(
        distinguishing_axes(&base, &by_input),
        vec![VariantAxis::InputShape]
    );
    assert_eq!(
        distinguishing_axes(&base, &by_oracle),
        vec![VariantAxis::OracleType]
    );
    assert_eq!(
        distinguishing_axes(&base, &by_channel),
        vec![VariantAxis::Channel]
    );
    assert_eq!(
        distinguishing_axes(&base, &by_context),
        vec![VariantAxis::Context]
    );

    assert!(
        check_variants(&[base, by_input, by_oracle, by_channel, by_context]).is_empty(),
        "cases differing on any axis are variants, not duplicates"
    );
}

#[test]
fn the_real_registry_has_no_indistinguishable_variants() {
    let counts = denominator_counts(&registry());
    assert!(
        counts.behaviors_with_variants > 0,
        "the migration should have produced variant groups; none found"
    );
    assert!(
        counts.variants > counts.behaviors_with_variants,
        "a variant group has more cases than behaviors by construction: {} variants \
         across {} behaviors",
        counts.variants,
        counts.behaviors_with_variants
    );
}
